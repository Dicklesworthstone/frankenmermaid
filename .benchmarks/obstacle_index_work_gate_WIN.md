# WIN: obstacle-index work-gate — dense-DAG layout 2.34× faster, byte-identical

**Date:** 2026-07-10 · **Agent:** cc_fm · **Base:** `aa4d10c` · **File:** `crates/fm-layout/src/lib.rs`
(edge-routing region, now peer-free — cod moved to rendering). **Verdict: KEEP.**

## Profile-first (the mechanism, not a guess)

`perf record --call-graph=dwarf` on `dense_dag_200` (200 nodes / 790 edges) put **`find_obstacle_nudge_x` at
19.36% of the whole pipeline** — the largest non-barycenter layout frame in the repo. The folded call-chain
showed it going through **`find_vertical_segment_nudge_iter`** (the *linear* scan over ALL obstacles), not the
spatial-index `_by_indices` path. So the obstacle spatial index was **off** for this graph, and edge routing
was paying O(edges · obstacles) = 790 × 200 ≈ **158k** obstacle tests per layout.

## Root cause

`build_edge_paths_with_orientation` gates the index (`lib.rs:12697`):

```rust
let sparse_routing = ir.edges.len() <= nodes.len() * 3 / 2;
let index_eligible = sparse_routing || obstacle_bounds.len() >= DENSE_INDEX_OBSTACLES; // 256
```

`DENSE_INDEX_OBSTACLES = 256` uses **obstacle count** as the crossover proxy — but it was calibrated on the
*wide layered* family, where edges ≈ obstacles. The linear scan's real cost is `edges × obstacles`.
`dense_dag_200` has only 200 obstacles (< 256, so **excluded**) but 790 edges, so it pays the SAME 158k scan
work the already-indexed 12x24 wide graph (288 × 552 ≈ 159k, measured index −25%) pays — yet is denied the
index because the gate looks at the wrong quantity.

## The lever (one)

Add a third, **work-based** disjunct — purely additive:

```rust
let linear_scan_work = ir.edges.len().saturating_mul(obstacle_bounds.len());
let index_eligible = sparse_routing
    || obstacle_bounds.len() >= DENSE_INDEX_OBSTACLES
    || linear_scan_work >= DENSE_INDEX_LINEAR_WORK;   // new const = 100_000
```

`100_000` sits **above the one measured index LOSS** (8x16: 128 × 224 ≈ 29k, index +5% → stays excluded) and
**below the measured WINS** (12x24/16x32, and `dense_dag_200` at 158k → newly indexed). Additive ⇒ it can only
*enable* the index for more graphs, never disable it, so **no currently-indexed case can regress**. No runtime
overhead on the hot path (one `saturating_mul` at the once-per-layout decision).

## Byte-identical

The index query is a **conservative superset** of the linear AABB scan filtered by the exact CGA test, so the
routing result is identical whether the index is used or not. Proven three ways:

- **New `dense_obstacle_field_index_matches_linear_scan`**: a 10×10 dense obstacle grid (100 obstacles),
  `find_obstacle_nudge_x/y` index vs linear over 12 vertical + 12 horizontal segments — all equal. (Existing
  tests only covered a 2-obstacle field; this covers the dense case my change actually flips.)
- **New `dense_index_work_gate_matches_measured_crossover`**: pins the gate boundary — 8x16 excluded, 12x24
  indexed, `dense_dag_200` newly indexed, tiny dense graph excluded.
- `cargo test -p fm-layout` **439 passed**; `golden_layout_test` **2/2**; `frankentui_conformance_test` green —
  the determinism gate holds. `cargo fmt --check` clean; `ubs` 158→160 (both new = the `assert_eq!`
  "secret compared with ==" false positive).

## Measurement — same-worker A/B with a built-in null

New `layout_dense/dag` bench in `pipeline_bench` (fm-cli): `layout_diagram` on dense DAGs (`edges ≈ 4·nodes`).
Two binaries — cand (worktree) and base (`git show HEAD:lib.rs > lib.rs`, lever reverted, bench kept) — run via
`RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch exec -- cargo bench`.

**`dense_400` and `dense_800` are built-in NULL controls:** 400 and 800 obstacles both exceed the old 256 gate,
so base and cand behave *identically* there — their cand/base ratio is pure worker-speed drift.

Run 1 (cand on `vmi1227854`, base on `hz2` — different workers, self-calibrated via the null rows):

| bench | cand p50 | base p50 | cand/base | worker-corrected | note |
|---|---:|---:|---:|---:|---|
| `dense_200` (newly indexed) | 116.63 µs | 257.17 µs | 0.454× | **0.409× (2.45× faster)** | treatment |
| `dense_400` (null: both indexed) | 247.32 µs | 222.34 µs | 1.112× | ~1.00× | worker drift = +11.2% |
| `dense_800` (null: both indexed) | 491.67 µs | 443.30 µs | 1.109× | ~1.00× | worker drift = +10.9% |

The two null rows agree (11.2% / 10.9%) and, corrected, sit at ~1.00 — exactly as an unaffected case must.

Run 2 — **both arms on the same worker `hz2`** (direct read, no correction needed):

| bench | cand | base | ratio |
|---|---:|---:|---:|
| `dense_200` | 112.85 µs [110.53, 115.83] | 264.43 µs [258.04, 272.27] | **0.427× = 2.34× faster** |

CIs nowhere near overlapping. The same-worker read (2.34×) confirms the self-calibrated run-1 estimate (2.45×).
**Dense-DAG layout is ~57% faster**, and `dense_400`/`dense_800` (already indexed) are provably untouched.

## Scope

Helps any diagram with **many edges relative to obstacles** and < 256 obstacles: dense DAGs, cyclic-SCC graphs,
call graphs, dependency graphs. Neutral on wide layered graphs (already gated by obstacle count) and sparse
flowcharts (already indexed via `sparse_routing`). `find_obstacle_nudge_x` was 19.36% of the `dense_dag_200`
pipeline; converting its linear scan to the localized index query removes the bulk of that.
