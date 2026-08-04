# WIN: hoist the per-member class-node lookup out of the block loop — class parse ~6% faster

**Date:** 2026-07-10 · **Agent:** cc_fm · **Base:** HEAD (`c618251`) · **File:** `crates/fm-parser/src/ir_builder.rs`
**Verdict: KEEP** (byte-identical IR, CI-disjoint against adverse null drift, scales per member).

## Profile-first (mechanism)

The `class_stages` bench added last turn showed class diagrams are **parse-dominated** (at 512 classes parse
~776µs vs render ~243µs). Reading the class parse path: `parse_class` calls `add_class_member` for every member
line, and `add_class_member` did `self.node_id_index.get(class_name, &self.ir.nodes)` **per member** — an
`FxHasher` run over the class-name string + a bucket lookup + an `id == class_name` string compare (the
collision guard). For the bench's 512 classes × 4 members = **2048 members**, that is 2048 hash+lookup+compare
calls, all resolving to the same node within a block (the class name is invariant across a block's members).

## The lever (one)

Resolve the node id **once per block**. `set_current_class` (called right after the class node is interned, in
`lower_class_statement`'s `BlockStart` arm) now stores `current_class_node_id: Option<IrNodeId>`;
`add_class_member` reads that cached id directly, skipping the lookup. `clear_current_class` resets it. The old
`current_class: Option<String>` field became write-only and was removed, which also drops a per-block
`name.to_string()` allocation. Net: 2048 → 512 lookups (one per block), and one fewer String alloc per block.

## Byte-identical (proven)

Node ids are stable append indices and the class node is interned *before* `set_current_class`, so the id
resolved once equals what each per-member `get` would return (deterministic, including duplicate-name `Many`
buckets). The built IR is unchanged:

- **`cargo test -p fm-parser --lib`: 408 passed, 0 failed** — includes the class-member parsing tests
  (`class_parses_nodes_edges_and_assignments`, `class_member_visibility_markers`, return-type/stereotype cases).
- **`golden_svg_test`: only `gantt_basic` fails** (documented pre-existing); `class_basic` and all others pass —
  the class-diagram SVG rendered from the IR is byte-identical.

## Measurement — same-worker A/B on hz2, layout+render as built-in null

`class_stages` bench (`pipeline_bench`). cand = worktree; base = `git show HEAD:ir_builder.rs > ir_builder.rs`
(lever reverted — `ir_builder.rs` has no peer WIP, so a file-level revert is safe and never touches cod's
`mermaid_parser.rs` git-graph WIP). `layout/*` and `render/*` are null rows (a parse-code change can't touch
them). Both arms on **hz2**.

| stage/size | cand p50 (µs) | base p50 (µs) | cand/base | note |
|---|---:|---:|---:|---|
| **parse/512** | **745.74 [736.9, 756.7]** | **776.45 [766.6, 788.2]** | **0.960** | treatment — **CIs disjoint** |
| parse/256 | 377.76 | 385.87 | 0.979 | treatment |
| parse/64 | 94.70 | 93.01 | 1.018 | small-N, masked by adverse drift |
| layout/512 (null) | 118.33 | 115.94 | 1.021 | cand session ~2% SLOWER |
| render/512 (null) | 243.99 | 237.92 | 1.026 | cand session ~2.6% SLOWER |

**Read:** this hz2 pairing had *adverse* drift — the null rows show cand's session ~2% **slower** on identical
layout/render code. Yet **parse/512 is CI-disjoint faster** (cand upper 756.7 < base lower 766.6), moving
*opposite* to the null drift. Drift-correcting parse/512 by the ~1.02–1.03 null gives **≈ −6%**. The effect
scales with class count (parse/64 masked by drift, 256 marginal, 512 clear) — the signature of removing a
per-member cost (~15–22 ns/member). A first cross-worker pair (cand on the faster ovh-a) showed parse/512
0.895 raw, consistent once worker speed is accounted for.

## Scope

Any class diagram with member compartments (the more members per class, the bigger the win); neutral on all other
diagram types (`add_class_member`/`set_current_class` are class-only). Byte-identical + monotonic-less-work.
LEVER (reusable): a lookup keyed by a value that is **invariant across an inner loop** (here the open class name
across its member lines) → resolve once at the loop's entry hook, cache, reuse. Same family as the gantt task-dep
hoist and the Brandes-Köpf dense-lookup.
