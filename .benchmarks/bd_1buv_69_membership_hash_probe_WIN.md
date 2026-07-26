# bd-1buv.69 — drop the redundant membership hash probe — WIN, −2.92% instructions on `arch_100x50`

## Provenance

| | |
|---|---|
| Lever | one: remove the `FxHashSet<(group_index, node_id)>` dedup probe from the hot path of `IrBuilder::add_node_to_cluster` and `add_node_to_subgraph` |
| Surfaced by | `arch_100x50`, a workload class that did not exist before `bd-1buv.68` |
| Base ELF | `d00c33b8c701005474f4431f…` (HEAD `ir_builder.rs`) |
| Candidate ELF | `0c0b7469d44364050f100cd0…` |
| Build | both arms back to back from explicit source states, `RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch exec -- cargo build --release -p frankenmermaid-cli --example headtohead`, **same worker `hz2`** (119.4 s / 111.1 s) |
| Measurement host | local, `taskset -c 40`, load ~12 |

Both ELFs are post the `sha2 0.10→0.11 / toml / crossterm / criterion` bump, so the merge is not
confounded with the lever. The earlier `base-headtohead` (`d896e0ec…`) predates that bump and is
**not** the arm used for the reported numbers.

## Profile attribution (before touching source)

`perf record -F 999`, non-LTO + `strip=false` + `debug=true` symbolized build, single pinned core.
On `arch_100x50` (5,000 nodes in 100 subgraphs) the **largest self-time frame** is:

```
7.19% / 8.73%   <hashbrown::map::HashMap<(usize, fm_core::IrNodeId), (), FxBuildHasher>>::insert
```

(two runs). It does not appear in any flowchart profile at any scale. Full tables:
`.benchmarks/bd_1buv_68_workload_class_profiles.md`.

That frame is the dedup set behind `add_node_to_cluster` / `add_node_to_subgraph`. **No benchmark in
this repo had ever routed through it at scale** — the only subgraph fixture,
`crates/fm-cli/tests/fixtures/frankentui_conformance/flowchart_subgraph.mmd`, has four nodes. This is
the `docs/LEDGER_RESURRECTION.md` §5 predicate (the bench did not exercise the code under test) in
its third scope: workload class rather than algorithm or input property.

## The lever, and why it is behavior-preserving

Both functions kept **two** membership records and consulted only one of them for dedup:

```rust
if self.cluster_member_set.insert((cluster_index, node_id)) {   // global hash set
    cluster.members.push(node_id);
    graph_cluster.members.push(node_id);
}
if let Some(graph_node) = … {
    if !graph_node.clusters.contains(&cluster_id) {              // per-node mirror
        graph_node.clusters.push(cluster_id);
    }
}
```

`ir.graph.nodes[i].clusters` / `.subgraphs` are appended **only** in these two functions (verified by
grep: `crates/fm-parser/src/ir_builder.rs:1186` and `:1278` are the sole push sites), start empty at
node creation (`:1077`, `:1420`), and grow on exactly the calls that append to `members`. So the
per-node list already **is** the membership index, and the two guards test the same predicate.

The candidate consults the mirror and drops the hash probe. The mirror is ~1 element long (a node
belongs to its own group plus its ancestors); the set was sized to the whole diagram.

Equivalence rests on one invariant: every `IrNodeId` handed out has a corresponding
`ir.graph.nodes` entry. Both `ir.nodes.push` sites push to `ir.graph.nodes` in the next statement, so
the two vectors are always the same length and an id is never issued before its graph node exists.
Rather than *assume* that, the candidate keeps the original set-based dedup as the `None` arm of the
match. The two paths partition node ids — a given id either has a graph node on every call or on
none — so a node can never dedup against the wrong structure.

Counted as ONE lever across two sites per this repo's own duplicated-helper lesson: it is a single
monomorphized `insert` symbol serving both mirrored call sites, and fixing one would have left the
other paying.

## Byte-identity — proven BEFORE timing

All **19** corpus items, comparing `output_sha256` (the SHA-256 of *every* revision's SVG
concatenated, so the edit trace compares all 201 documents and the doc build all 40):

```
19 items, 0 mismatches — BYTE-IDENTICAL
```

`base → base2` is also byte-identical, which independently shows the dependency bump moved no output
either. `flowchart_large_500` reproduces `408ecdcc…2d2d21`, the hash already recorded in the ledger.

## A/B with A/A null control, arms alternated per round

Statistic: **instructions**, `perf stat -e instructions,cycles`, `FM_H2H_FORCE_PROFILE=default` so
the batch factor is pinned to 1 and work is exactly proportional to reps. Instructions are
deterministic for a fixed workload and load-immune, which is what makes this decidable on a box at
load ~12. This is not an ISA change, so instruction count is a measure of work removed, not a proxy.

**`arch_100x50`** (600 reps/run, 9 rounds each):

| arm | min | median | max |
|---|---:|---:|---:|
| A/A null (base vs base) | 0.999982 | **1.000050** | 1.000204 |
| A/B (base vs candidate) | 0.970682 | **0.970791** | 0.972152 |

**−2.92% instructions.** The A/A null half-width is ±0.011%; every one of the nine A/B rounds sits
outside the entire A/A range, and the effect is ~265× the null half-width. Cycles corroborate at
roughly −2.8%.

**Negative control, `flowchart_large_500`** (no subgraphs, so neither function is ever called):

| arm | min | median | max |
|---|---:|---:|---:|
| A/A null | 0.998901 | **1.000040** | 1.001798 |
| A/B | 1.000357 | **1.000543** | 1.000568 |

**+0.054%**, inside the A/A null range and 20× smaller than the win. Reported rather than rounded to
zero: the sign is systematic across all seven rounds, so it is most likely a real sub-0.1% inlining
side-effect of changing the function bodies, not noise. It is two orders of magnitude below the 1%
decidability threshold and does not approach a shippable regression.

Wall clock, same binaries, arms alternated, 5 rounds: two rounds were contaminated (MAD 11.4% and
19.6%, failing this repo's own 5% MAD gate) and are discarded. The gate-passing rows give base
median 5.633 ms vs candidate 5.449 ms, **−3.3%**, directionally consistent. Wall is corroboration
here, not the gate.

## Correctness

- Byte-identity across all 19 corpus items (above), including subgraph-heavy `arch_100x50`.
- `cargo clippy -p frankenmermaid-cli --example headtohead -- -D warnings`: clean.
- `cargo test -p fm-parser`: see the commit message for the run this landed on.

## Retry / rollback

Rollback is a plain revert; there is no flag because the change is byte-identical and monotonically
less work — a flag would only preserve a slower path with identical output.

The `cluster_member_set` / `subgraph_member_set` fields are deliberately **kept**, not deleted: they
back the `None` arm. If a future change makes `ir.nodes` and `ir.graph.nodes` diverge in length, that
fallback is what keeps dedup correct, and the invariant note above is the thing to re-check.
