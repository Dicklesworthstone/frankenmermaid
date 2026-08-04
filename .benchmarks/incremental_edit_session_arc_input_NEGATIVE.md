# REJECT: edit-session Arc-input entry point (elide per-pass IR clone) — mimalloc-wash (2026-07-23)

Agent: CopperCliff (cc). Base: `fccfbb21`. Lane: bd-12e / bd-9rq7 item d.

## Lever

The per-pass full-IR `Arc::new(ir.clone())` snapshot was 16.5% of the (now-fast) size-stable
incremental path. Candidate added an owning entry point `layout_diagram_traced_arc(&mut self,
ir_arc: Arc<MermaidDiagramIr>, …)` that retains the caller's `Arc` by refcount bump (pre-seeding the
pass snapshot so `track_dependency_graph_query`'s `get_or_insert_with(|| Arc::new(ir.clone()))` skips
the deep clone). The borrowing entry point delegated to a shared `_impl` with `None` pre-snapshot,
preserving the original lazy-clone-on-miss / no-clone-on-memo-hit behavior. New within-binary bench
`fresh_ir_edit` modeled the real WASM `WebRenderer` workload (fresh owned IR per edit, i.e. re-parse)
and compared `borrowing` (engine deep-clones) vs `owning` (refcount).

## Correctness (all held)

441 fm-layout tests green including a new parity test asserting the owning entry point produces
byte-identical layout + query_type to the borrowing entry point across warm + 6-edit sequences at 64
and 1000 nodes (fast path, slow relayout, and full-recompute fallback all exercised). Borrowing path
proven perf-neutral vs `567c86ef` on the mutate-in-place bench (mins 229.6µs vs 230.3µs, dead even).

## Measured — the "win" was an allocator-contention artifact

`fresh_ir_edit` owning vs borrowing, one binary, interleaved:

| load | row | borrowing | owning | apparent Δ |
|---|---|---|---|---|
| **34** (multi-agent) | /500 | ~236µs | ~182µs | −23% |
| **34** | /1000 | ~462µs | ~352µs | −24% |
| **5** (representative) | /500 | 168–179µs | 176–177µs | **~0% (wash, reverses in noise)** |
| **5** | /1000 | 338–342µs | 336–343µs | **~0% (wash)** |

Under the multi-agent load that inflated the first measurement, allocator contention amplified the
extra IR deep clone, manufacturing a −23% gap. Under representative **low-contention** load — which
is what the incremental engine actually runs under (single-threaded browser WASM `WebRenderer`, or
`fm-cli watch`) — mimalloc makes the deep clone cheap, and the owning path's elided clone is offset
by the `Arc::new` control-block allocation the caller now pays. Net wall wash.

This is the documented **`Vec`/alloc-removal mimalloc-wash** pattern
(`project_ab_substrate_rules_v2`, `project_layout_stable_priorities_hoist_and_mimalloc_profile`):
removing an allocation is instruction-real but wall-neutral when the allocator is uncontended, and
the target runtime is uncontended. Wall (low-load, interleaved) is the decision metric per substrate
rules; the high-load delta is not representative.

## Verdict: REJECT. Candidate reverted to `fccfbb21`.

Candidate source preserved at session scratchpad (`cand_arc_lib.rs.bak`, `cand_arc_bench.rs.bak`).

## Do Not Retry

Do not re-add an Arc-input incremental entry point to elide the per-pass IR clone **unless** the
whole parse→layout pipeline becomes `Arc<MermaidDiagramIr>`-native so the caller pays *no* extra
`Arc::new` (the current parser yields an owned IR, so wrapping is a fresh allocation that cancels the
saving), AND a profile under **representative low-contention load** shows the IR clone as a top-2
wall frame, AND a one-binary interleaved A/B at CPU-load <8 shows ≥3% direction-consistent wall
improvement with CV<5% and byte-identical output. The 16.5% figure came from a `perf record` sample
that counts the clone's instructions/allocation, which mimalloc services cheaply on the wall clock
when uncontended.
