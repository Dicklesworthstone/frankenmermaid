# Ledger Resurrection Audit — full table

Generated companion to [`LEDGER_RESURRECTION.md`](LEDGER_RESURRECTION.md); see §1 there for the
classifier that produced the Verdict column. Source: `docs/NEGATIVE_EVIDENCE.md` @ `ca4e1d65`,
251 reject-class rows of 667 entries, sorted VOID-A → VOID-B → SOUND-noNull → SOUND → finding,
then by target-frame self-time.

"Effect claimed" is the largest per-lever delta quoted in the entry body and is a mechanical
extraction, not a hand-read verdict — the ranked queue in §3 of the main document is the
hand-verified part.

| # | Ledger line | Entry | Effect claimed | Null control | Target self-time | Binary sha? | Verdict |
|---|---|---|---|---|---|---|---|
| 1 | L13338 | REJECTED SAMPLE / RETRY OPEN: flat-CSR barycenter direction is large, but loaded-work… | 47.3% | yes | 76.84% | **no** | **VOID-A** |
| 2 | L3261 | Barycenter sweep precomputed edge adjacency — ~~REJECTED~~ **MEASUREMENT INVALID / LE… | 47.64% | **no** | 47.64% | **no** | **VOID-A** |
| 3 | L13590 | REJECT MEASUREMENT / RETRY OPEN: lint-clean packed crossing run has no dual-CV-qualif… | 24.56% | yes | 24.56% | yes | **VOID-A** |
| 4 | L19037 | BLOCKER / REJECT: bd-1buv.2 full parse-layout-SVG micro-frontier is closed (2026-07-2… | 28.57% | yes | 8.0% | **no** | **VOID-A** |
| 5 | L938 | `write_escaped_text` single-pass tight-reject restructure — REVERTED, wash (2026-07-1… | 0.22% | **no** | not quoted | **no** | **VOID-A** |
| 6 | L2355 | REJECT (WASH): clean-scan fast path for `write_escaped_text` short labels — 0.00% ren… | 33.0% | **no** | not quoted | **no** | **VOID-A** |
| 7 | L3341 | Dense crossing-count position maps — ~~REJECTED~~ **MEASUREMENT INVALID (but headroom… | 23.8% | **no** | not quoted | **no** | **VOID-A** |
| 8 | L3373 | Flat-array `total_crossings` position/edge tables — ~~REJECTED~~ **MEASUREMENT INVALI… | 47.64% | **no** | not quoted | **no** | **VOID-A** |
| 9 | L8089 | graph_metrics_cache_key inline-hash (drop the throwaway resolved_edges Vec) — REVERTE… | 2.0% | **no** | not quoted | **no** | **VOID-A** |
| 10 | L9567 | REJECTED: fuse strip_dead_marker_css + minify_style_block (one find + one replace_ran… | 1.1% | **no** | not quoted | **no** | **VOID-A** |
| 11 | L11791 | NO-SHIP (4 washes reverted): constant-factor micro-opts after the scaling frontier cl… | 0.03% | **no** | not quoted | **no** | **VOID-A** |
| 12 | L13362 | REJECTED SAMPLE / RETRY OPEN: quiescent flat-CSR run identifies the 2 ms sample floor… | 25.78% | yes | not quoted | **no** | **VOID-A** |
| 13 | L13380 | REJECTED SAMPLE / RETRY OPEN: 20 ms flat-CSR samples improve dispersion but do not am… | 25.93% | yes | not quoted | **no** | **VOID-A** |
| 14 | L13397 | REJECTED SAMPLE / RETRY OPEN: 200 ms whole-arm pairs still track co-tenant phases ins… | 33.28% | yes | not quoted | **no** | **VOID-A** |
| 15 | L14880 | REJECT: memmem the `parse_init_directives` `%%{` gate — input-dependent, regresses co… | 0.4% | **no** | not quoted | **no** | **VOID-A** |
| 16 | L16634 | ❌REJECTED: delete the (provably dead) per-node `outgoing.sort_by` in `rank_assignment… | 7.6% | **no** | not quoted | **no** | **VOID-A** |
| 17 | L16747 | ❌REJECTED: dense_node_rank in egraph `layer_edges_between_ranks` (probe→dense) — scc_… | 3.6% | **no** | not quoted | **no** | **VOID-A** |
| 18 | L16912 | 🟡INVALID / HOLD: borrow the legacy Canvas dotted-edge dash slice (2026-07-14) | 3.0% | **no** | not quoted | **no** | **VOID-A** |
| 19 | L18561 | 🔴REJECTED (as-is; needs function extraction): reuse a scratch buffer for class-member… | 29.0% | **no** | not quoted | **no** | **VOID-A** |
| 20 | L9832 | REJECTED: memchr::memmem in strip_unused_state_css — WASH/tiny regression (str::conta… | 1.24% | **no** | 1.24% | **no** | **VOID-B** |
| 21 | L170 | <short-name> — REVERTED (<date>) | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 22 | L254 | Sequence guardrail cost estimate (force Sequence layout, not Sugiyama) - REVERTED (20… | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 23 | L855 | Direct edge-path string emission — REJECTED (2026-06-25) | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 24 | L2598 | Gate redundant node drop-shadow inline `filter` + its `<defs>` def on `embed_theme_cs… | 1.83% | **no** | not quoted | **no** | **VOID-B** |
| 25 | L2720 | Drop dead `data-fm-node-id` node attribute (−1 to −2% SVG bytes, zero consumers) — KE… | 2.0% | **no** | not quoted | **no** | **VOID-B** |
| 26 | L3476 | Removing `Attributes::set` dedup entirely — REJECTED (correctness) (2026-06-26) | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 27 | L3571 | Common `-->` flowchart parser shortcut — REJECTED (2026-06-25) | 1.47% | **no** | not quoted | **no** | **VOID-B** |
| 28 | L3931 | Node inline-style gating (the node analog of the landed edge fill/stroke gates) — BLO… | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 29 | L3958 | Dead-output sweep of the benched render is exhausted after the `data-fm-node-id` drop… | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 30 | L4240 | Emit only used `<defs>` arrowhead markers — HIGH-VALUE LEVER, IMPLEMENTATION-BLOCKED … | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 31 | L4363 | Agent Mail registration/reservation — BLOCKED (2026-06-24) | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 32 | L4377 | Local mermaid-js reference corpus — BLOCKED (2026-06-24) | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 33 | L4387 | `cargo bench --release` flag — BLOCKED (2026-06-24) | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 34 | L4399 | Cod-b mermaid-js denominator check — BLOCKED (2026-06-24) | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 35 | L4502 | Generated SVG id ownership — REVERTED (2026-06-27) | 1.65% | **no** | not quoted | **no** | **VOID-B** |
| 36 | L5381 | Parse: borrowed simple-node IDs after borrowed-edge landing — REVERTED, no significan… | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 37 | L5465 | write_escaped_text: auto-vectorizable no-special fast-path — REVERTED, render regress… | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 38 | L5514 | Render: rolling slice smooth-path helper after edge-stream + capacity wins — REVERTED… | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 39 | L5909 | NO-SHIP: borrowed block-beta sort anchors regressed same-worker layout (2026-07-04) | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 40 | L6112 | BLOCKER (peer-owned): incremental layout memo cache-hit is 2-4x SLOWER than full reco… | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 41 | L6540 | REJECTED: block-beta CSS gate — const mismatch + a pre-existing block-diagram render … | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 42 | L7054 | CLOSED (by experiment): the two render element-construction alloc/compute levers (202… | 1.2% | **no** | not quoted | **no** | **VOID-B** |
| 43 | L7350 | TESTED + REVERTED: CSR on the component-detection adjacency builders is byte-identica… | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 44 | L8074 | LUT for is_fast_flow_identifier + fast-node forbidden scan — REVERTED, ~0-gain (2026-… | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 45 | L8264 | REVERTED: class parser `in_block` bool flag is below the keep floor (2026-07-04) | 1.7435% | **no** | not quoted | **no** | **VOID-B** |
| 46 | L9129 | Remove the `retain` dedup from `Attributes::set` — REJECTED, reintroduces double-set … | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 47 | L9501 | REJECTED (definitively): optimizing/removing the Attributes::set retain — pervasive `… | 2.0% | **no** | not quoted | **no** | **VOID-B** |
| 48 | L10004 | REJECTED (now MEASURED): memchr opt=3 — ~1.8% render REGRESSION, not a win (2026-07-0… | 1.8% | **no** | not quoted | **no** | **VOID-B** |
| 49 | L11088 | NO-SHIP: pie owned path/text strings (render regression) (2026-07-04) | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 50 | L11904 | NO-SHIP (redundant — peer landed it): polygon shape streaming frontier closed by a pe… | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 51 | L11983 | NO-SHIP: large-diagram `to_string_with_body` streaming regresses render (2026-07-05) | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 52 | L14282 | REJECT: memoize `Theme::to_svg_style` output — build is only ~772 ns, caching saves ~… | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 53 | L14409 | REJECT: memoize `effects_css` (and the memoize-per-render pattern for CSS format! bui… | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 54 | L14819 | REJECT: timeline owned-ID handoff is flat-to-slower (2026-07-12) | 2.0961% | **no** | not quoted | **no** | **VOID-B** |
| 55 | L15544 | ⛔ HOLD / INVALID: direct terminal `CellBuffer` serialization never reached timed path… | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 56 | L15573 | ⛔ HOLD / INVALID RETRY: direct terminal `CellBuffer` serialization again missed timed… | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 57 | L15629 | ⛔ HOLD / INVALID: stream ASCII block line iteration never reached timed path (2026-07… | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 58 | L16057 | ❌REJECT: drop the per-line `format!` temporary in the render_diff output loop — −1.57… | 1.571% | **no** | not quoted | **no** | **VOID-B** |
| 59 | L16771 | ❌REJECTED (confirm): egraph `layer_edges` dense-probe on `layout_dense` (the ledger-r… | 1.8% | **no** | not quoted | **no** | **VOID-B** |
| 60 | L16820 | 🟡INVALID / HOLD: remove the dead WASM render budget-ledger clone (2026-07-14) | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 61 | L16866 | 🟡INVALID / HOLD: count compact terminal node-label width without `Vec<char>` (2026-07… | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 62 | L17580 | 🟡INVALID/HOLD: Canvas class-compartment font hoist did not reach remote proof (2026-0… | not quoted | **no** | not quoted | **no** | **VOID-B** |
| 63 | L5352 | Layout: skip CGA test for axis-aligned segments in `find_*_segment_nudge_iter` — REVE… | 31.0% | **no** | 31.0% | **no** | **SOUND-noNull** |
| 64 | L13134 | INTEGRITY AUDIT + REJECT: the 3 double-copy rejections HOLD; capacity pre-shaping (`b… | 30.0% | **no** | 21.79% | **no** | **SOUND-noNull** |
| 65 | L885 | Make `write_uint_into` inlinable by cold-splitting its recursion — REVERTED (2026-07-… | 28.0% | **no** | 15.0% | **no** | **SOUND-noNull** |
| 66 | L12779 | REJECTED: word-packed incremental span hashing misses the keep gate (2026-07-10) | 51.37% | **no** | 15.0% | **no** | **SOUND-noNull** |
| 67 | L2114 | REJECT: fuse the fast-node reject + `[`-locate scans into one table-driven pass — +0.… | 10.17% | **no** | 10.17% | **no** | **SOUND-noNull** |
| 68 | L9613 | REJECTED: parser byte-fast `trim_ws` (ASCII fast-path for `str::trim`) — WASH at pars… | 32.0% | **no** | 9.46% | **no** | **SOUND-noNull** |
| 69 | L5020 | Path `d` raw (escape-skip) serialization — REVERTED (2026-06-27) | 25.54% | **no** | 8.32% | **no** | **SOUND-noNull** |
| 70 | L19062 | BLOCKER / NEGATIVE: bd-1buv.2 current-head frontier revalidated after explicit contin… | 10.0% | **no** | 8.0% | **no** | **SOUND-noNull** |
| 71 | L5270 | Parse: eliminate per-line `line_items` Vec in `parse_flowchart_document_items` — REVE… | 17.0% | **no** | 7.95% | **no** | **SOUND-noNull** |
| 72 | L7861 | write_fixed2 from_utf8 revalidation — CEILING ~0-gain, NOT PURSUED (2026-07-01) | 7.4% | **no** | 7.4% | **no** | **SOUND-noNull** |
| 73 | L5492 | build_smooth_path `d` capacity n*24 -> n*56 — REVERTED, load-contaminated + over-allo… | 26.2% | **no** | 7.11% | **no** | **SOUND-noNull** |
| 74 | L8844 | node-path ends_with/strip_suffix ']' → byte ops — REJECTED (~0, rch false-positive) (… | 16.0% | **no** | 6.6% | **no** | **SOUND-noNull** |
| 75 | L9250 | REJECTED: precompute u32 id-rank to replace cmp_by_id string comparison in build_tree… | 12.3% | **no** | 6.2% | **no** | **SOUND-noNull** |
| 76 | L2157 | REJECT: `memchr::memchr` for `\n` in `ByteLines::next` — +0.26% parse REGRESSION (202… | 5.31% | **no** | 5.31% | **no** | **SOUND-noNull** |
| 77 | L14521 | REJECT: GitGraph current-head in-place update regresses the decisive MEDIAN row (2026… | 3.66% | **no** | 3.66% | **no** | **SOUND-noNull** |
| 78 | L8376 | INCONCLUSIVE/REVERTED: extend `trim_fast` to intern_node_auto `id.trim()` + node-pars… | 14.7% | **no** | 3.0% | **no** | **SOUND-noNull** |
| 79 | L1875 | WASH: move-through `normalize_sequence_display_text` to skip the entity-decode copy —… | 2.44% | **no** | 2.44% | **no** | **SOUND-noNull** |
| 80 | L13804 | REJECTED (~0, free-list-recycled) + FRONTIER SURFACE: BK median-of-four per-node Vec,… | 14.6% | **no** | 1.38% | **no** | **SOUND-noNull** |
| 81 | L182 | Class member label allocation rewrite - REVERTED (2026-07-04) | 21.11% | **no** | not quoted | **no** | **SOUND-noNull** |
| 82 | L229 | Presize class/ER compartment children Vec - REVERTED (2026-07-04) | 10.8% | **no** | not quoted | **no** | **SOUND-noNull** |
| 83 | L401 | Document XML streaming + conditional edge-label CSS - REJECTED (2026-06-27) | 35.51% | **no** | not quoted | **no** | **SOUND-noNull** |
| 84 | L453 | Theme CSS sub-writer append path - REJECTED (2026-06-27) | 12.55% | **no** | not quoted | **no** | **SOUND-noNull** |
| 85 | L562 | SVG integer number manual writer - REJECTED (2026-06-26) | 20.51% | **no** | not quoted | **no** | **SOUND-noNull** |
| 86 | L611 | SVG static custom-attribute names — REJECTED (2026-06-26) | 13.28% | **no** | not quoted | **no** | **SOUND-noNull** |
| 87 | L660 | Edge `data-fm-edge-id` numeric value path — REJECTED (2026-06-26) | 44.38% | **no** | not quoted | **no** | **SOUND-noNull** |
| 88 | L742 | SVG root attribute direct streaming — REVERTED (2026-06-26) | 23.36% | **no** | not quoted | **no** | **SOUND-noNull** |
| 89 | L782 | SVG document child Vec capacity hint — REVERTED (2026-06-25) | 28.22% | **no** | not quoted | **no** | **SOUND-noNull** |
| 90 | L821 | Attributes Vec pre-size after edge-style fast path — CAUTION (2026-06-25) | 3.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 91 | L1702 | REJECT + SURFACE: pie render is 56% grisu (full-precision arc coords); integer fast-p… | 56.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 92 | L1833 | REJECT: `trim_fast` the remaining requirement header/relation trims — +13.5% parse re… | 18.412% | **no** | not quoted | **no** | **SOUND-noNull** |
| 93 | L1917 | REJECT: first-byte guard before `starts_with` in the operator scan — wins seq/class b… | 13.4% | **no** | not quoted | **no** | **SOUND-noNull** |
| 94 | L1937 | REJECT: sequence-only two-byte operator buckets — point estimate −5.1%, uncertainty s… | 18.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 95 | L1984 | VOID / INVALID (correction): borrowed edge-label probe A/B missed the target path (20… | 7.5673% | **no** | not quoted | **no** | **SOUND-noNull** |
| 96 | L2039 | REJECT: borrow state-label cleanup and action prefixes with `Cow` — +11.52% midpoint … | 11.52% | **no** | not quoted | **no** | **SOUND-noNull** |
| 97 | L2063 | REJECT (valid retry): borrow cleaned edge-label probes into the interner — paired +3.… | 8.3274% | **no** | not quoted | **no** | **SOUND-noNull** |
| 98 | L2373 | REJECT: consolidate `extract_style_directives`'s 3-scan gate to 2 via a shorter share… | 4.4% | **no** | not quoted | **no** | **SOUND-noNull** |
| 99 | L2764 | Drop write-only `IrNode.span_all` accumulation (parse −12% large) — KEPT (2026-06-26) | 21.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 100 | L2858 | Drop 6 redundant `data-fm-source-*` attributes (SVG −35% spans-on) — KEPT (2026-06-26) | 55.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 101 | L3312 | Edge path offset Vec elision — REJECTED (2026-06-25) | 28.3% | **no** | not quoted | **no** | **SOUND-noNull** |
| 102 | L3423 | Borrowed SVG attribute names — REJECTED (2026-06-25) | 28.46% | **no** | not quoted | **no** | **SOUND-noNull** |
| 103 | L3439 | Guarded SVG attribute retain skip — REJECTED (2026-06-25) | 27.21% | **no** | not quoted | **no** | **SOUND-noNull** |
| 104 | L3506 | Owned accessibility title element path — REJECTED (2026-06-26) | 17.23% | **no** | not quoted | **no** | **SOUND-noNull** |
| 105 | L3538 | TextBuilder multiline Vec removal — REJECTED (2026-06-26) | 8.03% | **no** | not quoted | **no** | **SOUND-noNull** |
| 106 | L3591 | Plain flowchart label shortcut — REJECTED (2026-06-25) | 31.61% | **no** | not quoted | **no** | **SOUND-noNull** |
| 107 | L3651 | REJECTED: share `stable_node_priorities` across cycle_removal + rank_assignment (~0-g… | 29.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 108 | L3672 | REJECTED: cycle_removal acyclic strategy short-circuit (~0-gain) + re-profiled hotspo… | 37.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 109 | L3721 | REJECTED: Brandes-Köpf neighbour precompute regresses +2-4% (neighbour recompute is n… | 56.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 110 | L3772 | IrGraph build MEASURED ~0-gain (data closes the parked lever) (2026-06-27) | 5.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 111 | L3813 | IrGraph adapter build is dead in the render pipeline — but cheap, so a low-priority p… | 5.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 112 | L3834 | Dead-CSS prune VALIDATED — landed concurrently; standing down from fm-render-svg to a… | 27.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 113 | L3859 | Dead-CSS prune (~27% of the `<style>` is unused per diagram) — HIGH VALUE, blocked on… | 27.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 114 | L3886 | Byte-reduction frontier: CSS minify blocked; attr levers exhausted; post-gates standi… | 19.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 115 | L3983 | CSS-building is sub-bar: stale 4 KB `to_svg_style` capacity (2 reallocs/render) saves… | 15.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 116 | L4092 | cmake-free `fm-parser` parse bench + the per-worker-target-dir A/B blocker — INFRA (2… | 21.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 117 | L4127 | Edge routing is 85% of tree-path layout; `intersect_segment` bool variant — ~0-GAIN (… | 15.4% | **no** | not quoted | **no** | **SOUND-noNull** |
| 118 | L4208 | Post-process `<style>` CSS minification — REJECTED (render +19%) (2026-06-26) | 19.39% | **no** | not quoted | **no** | **SOUND-noNull** |
| 119 | L4289 | `fm-source-span` static-name + `data_owned` allocation trim — ZERO-GAIN (2026-06-26) | 3.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 120 | L4322 | Offset edge-point streaming path builder — REJECTED (2026-06-26) | 6.39% | **no** | not quoted | **no** | **SOUND-noNull** |
| 121 | L4544 | Parser hash-key dedup maps — REVERTED (2026-06-27) | 55.7% | **no** | not quoted | **no** | **SOUND-noNull** |
| 122 | L4579 | Cluster CSS feature gate — REVERTED (2026-06-27) | 12.35% | **no** | not quoted | **no** | **SOUND-noNull** |
| 123 | L4621 | Element child Vec pre-sizing — REVERTED (2026-06-27) | 5.32% | **no** | not quoted | **no** | **SOUND-noNull** |
| 124 | L4665 | TextBuilder single-line line-vector skip - REVERTED (2026-06-27) | 28.15% | **no** | not quoted | **no** | **SOUND-noNull** |
| 125 | L4770 | Truncate-label byte-length guard - REVERTED (2026-06-27) | 28.35% | **no** | not quoted | **no** | **SOUND-noNull** |
| 126 | L4881 | Render: gated raw rect-node writer — REVERTED, mixed/noisy and small-size slower (202… | 10.9% | **no** | not quoted | **no** | **SOUND-noNull** |
| 127 | L4953 | Attributes SmallVec inline storage — REVERTED (2026-06-27) | 44.37% | **no** | not quoted | **no** | **SOUND-noNull** |
| 128 | L5048 | Per-edge `pts` stack buffer (eliminate 1024 heap Vecs) — REVERTED, sub-bar/unmeasurab… | 23.5% | **no** | not quoted | **no** | **SOUND-noNull** |
| 129 | L5133 | Parse: borrowed fast-node document item — REJECTED, regression vs current ORIG (2026-… | 56.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 130 | L5241 | write_int (direct integer serialization) — REVERTED, ~0-gain on re-confirm (2026-06-2… | 13.2% | **no** | not quoted | **no** | **SOUND-noNull** |
| 131 | L5400 | Element id builders: `format!` → direct push_str (drop format_inner) — KEPT, render ~… | 5.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 132 | L5549 | Render frontier status + measurement blocker (post escape-win) (2026-06-27) | 57.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 133 | L5578 | Parse profile (post edge-borrow) + IR edge-capacity finding; load blocker persists (2… | 41.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 134 | L5656 | Nodes are ~60% of render; narrow rect direct-byte is config-fragile (REVERTED) (2026-… | 60.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 135 | L5680 | Common gradient rect node shape direct-byte — byte-identical but ~0 at headline (REVE… | 60.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 136 | L5734 | Full-node direct-byte: byte-identical but ~0 (sub-noise) — REVERTED; corrects the edg… | 35.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 137 | L5846 | Flowchart edge-count capacity pre-scan — REJECTED, 1.58x-1.81x slower than ORIG parse… | 55.5% | **no** | not quoted | **no** | **SOUND-noNull** |
| 138 | L6158 | Render streaming-serialization refactor QUANTIFIED as sub-noise — render frontier CLO… | 10.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 139 | L6648 | REVERTED: manual compact_display (byte-identical but unmeasurable + expected sub-nois… | 5.3% | **no** | not quoted | **no** | **SOUND-noNull** |
| 140 | L6834 | REVERTED: bulk-copy minify hot loop -- sub-noise; the post-pass cost is the marker O(… | 24.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 141 | L6901 | REJECTED: general CSS class tree-shake -- real -6.5% corpus output, but +12-23 us/ren… | 56.9% | **no** | not quoted | **no** | **SOUND-noNull** |
| 142 | L6945 | REJECTED: render-path alloc reduction -- hot buffers already pre-sized; remaining chu… | 12.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 143 | L7074 | CLOSED: label-intern key-clone lever -- dedup is observable, only hash-dedup preserve… | 11.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 144 | L7233 | FOUND (near-latent perf bug, NOT yet fixable safely): the node fast-path is DEAD for … | 60.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 145 | L7880 | with_capacity_hint edges = input_lines (vs input_lines/3) — REVERTED, sub-bar + free-… | 52.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 146 | L7930 | build_tree_layout_structure integer-rank sort keys — REVERTED, ~0-gain (2026-07-01) | 30.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 147 | L8316 | REVERTED: path-writer `push(char)` → `push_str(1-byte literal)` — render regressed (2… | 10.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 148 | L8400 | REVERTED: drop `ObstacleSpatialIndex::query_segment` per-query sort (min-index in con… | 9.6% | **no** | not quoted | **no** | **SOUND-noNull** |
| 149 | L8493 | REVERTED: no-quote/bracket fast path for `find_operator_from_index` — ~0 gain (2026-0… | 8.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 150 | L8639 | REJECTED (alien-graveyard §7.9): mimalloc global allocator for parse — mixed, regress… | 37.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 151 | L8817 | query_segment single-cell candidate fast-path — REJECTED (~0 gain) (2026-07-02) | 16.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 152 | L8870 | itoa crate for write_uint_into — REJECTED (regression) (2026-07-02) | 12.2% | **no** | not quoted | **no** | **SOUND-noNull** |
| 153 | L8891 | layout sort_by → sort_unstable_by (19 usize index-tiebreak sorts) — REJECTED (wash + … | 40.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 154 | L9010 | find_operator_core: `char_indices` → byte loop — REJECTED (regression) (2026-07-02) | 8.4% | **no** | not quoted | **no** | **SOUND-noNull** |
| 155 | L9056 | CgaRect::segment_crosses (boolean early-return) for obstacle nudge — REJECTED (~0) (2… | 56.7% | **no** | not quoted | **no** | **SOUND-noNull** |
| 156 | L9332 | REJECTED: pre-size build_edge_paths_with_orientation's filter_map collect — REGRESSIO… | 2.7% | **no** | not quoted | **no** | **SOUND-noNull** |
| 157 | L9379 | REJECTED: minify_css Vec<u8>+from_utf8 → String+push_str runs — ~0.2% pipeline (below… | 13.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 158 | L9476 | REJECTED: skip obstacle index for global-edge (mindmap) layouts — WASH; the O(N²) is … | 23.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 159 | L9588 | RE-REJECTED: strip_unused_theme_css str::replace → memmem::find + replace_range — COD… | 7.9% | **no** | not quoted | **no** | **SOUND-noNull** |
| 160 | L9851 | REJECTED: memmem + in-place replace_range for strip_unused_theme_css's str::replace —… | 6.3% | **no** | not quoted | **no** | **SOUND-noNull** |
| 161 | L9929 | REJECTED: memchr for single-byte per-line gates in the parser — 7.4% parse REGRESSION… | 11.6% | **no** | not quoted | **no** | **SOUND-noNull** |
| 162 | L9953 | REJECTED: presize the flowchart line Vec<(usize,&str)> — count scan costs more than t… | 28.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 163 | L10152 | REJECTED: skip-escape of marker-end (generated url ref) — WASH (short value, escape a… | 3.7% | **no** | not quoted | **no** | **SOUND-noNull** |
| 164 | L10173 | FRESH PROFILE (this segment's wins confirmed) + REJECTED state_css </style> memmem (2… | 29.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 165 | L10221 | REJECTED: bounded .fm-cluster search in strip_unused_state_css — CODE-LAYOUT NOISE ma… | 26.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 166 | L10364 | REVERTED: precomputed SVG state/accent CSS usage mask is below landability floor (202… | 3.29% | **no** | not quoted | **no** | **SOUND-noNull** |
| 167 | L10472 | REJECTED: bounded per-node alloc elision in `render_class_compartments` is below the … | 5.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 168 | L10640 | REJECTED: block-beta source_line borrow refactor is below the parse layout-noise floo… | 3.9% | **no** | not quoted | **no** | **SOUND-noNull** |
| 169 | L10749 | REJECTED: TextBuilder move-owned-fields (avoid double-clone) is below the render floo… | 10.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 170 | L11262 | NO-SHIP (reverted ~0-gain): content-aware edge-buffer presize for cardinality/labeled… | 24.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 171 | L11411 | NO-SHIP (both reverted): first-byte gate does NOT generalize to find_operator or stat… | 8.9% | **no** | not quoted | **no** | **SOUND-noNull** |
| 172 | L11496 | NO-SHIP: pre-size the streamed pie raw fragment (same-worker no-change, reverted) (20… | 13.34% | **no** | not quoted | **no** | **SOUND-noNull** |
| 173 | L11583 | NO-SHIP (reverted ~0-gain): make the common-node fast-path `user_class_suffix` lazy (… | 9.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 174 | L12225 | NO-SHIP: class parser simple-relationship fusion regressed class_100 on same worker (… | 6.8717% | **no** | not quoted | **no** | **SOUND-noNull** |
| 175 | L12252 | NO-SHIP: class parser dense member insertion plus borrowed cardinality text slowed cl… | 16.25% | **no** | not quoted | **no** | **SOUND-noNull** |
| 176 | L12426 | NO-SHIP: large raw-part body fusion regresses large wide render (2026-07-10) | 26.1% | **no** | not quoted | **no** | **SOUND-noNull** |
| 177 | L12517 | NO-SHIP: large render empty between-child guards are flat/noise (2026-07-10) | 24.11% | **no** | not quoted | **no** | **SOUND-noNull** |
| 178 | L12592 | REJECTED: topology-stable incremental dependency-cache hit patch is flat/slower (2026… | 47.51% | **no** | not quoted | **no** | **SOUND-noNull** |
| 179 | L13940 | REJECT: stream class-compartment member rows (remove per-row format! alloc) — sub-noi… | 40.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 180 | L14033 | REJECT: partial ER attribute-row streaming (raw_svg child) — wash; need WHOLE-node by… | 50.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 181 | L14053 | REJECT: whole-entity ER streaming fast path — byte-identical but REGRESSES render at … | 20.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 182 | L14506 | REJECT: stream the a11y `describe_diagram_with_layout` description (format! → push_st… | 2.65% | **no** | not quoted | **no** | **SOUND-noNull** |
| 183 | L14705 | REJECT: architecture group index `BTreeMap<String, _>` to `FxHashMap` (2026-07-12) | 19.84% | **no** | not quoted | **no** | **SOUND-noNull** |
| 184 | L14730 | REJECT: Kanban dense-ID class appends miss the decisive large-row floor (2026-07-12) | 6.986% | **no** | not quoted | **no** | **SOUND-noNull** |
| 185 | L14761 | REJECT: `coordinate_assignment` per-rank `Vec<usize>` clone elision is flat (2026-07-… | 5.8929% | **no** | not quoted | **no** | **SOUND-noNull** |
| 186 | L14791 | REJECT: BK direction iterators do not prove a stable gain (2026-07-12) | 8.9704% | **no** | not quoted | **no** | **SOUND-noNull** |
| 187 | L14941 | REJECT: skip building the throwaway value String for non-matching DOT attributes — co… | 12.7% | **no** | not quoted | **no** | **SOUND-noNull** |
| 188 | L14971 | ~~REJECT~~ → **LANDED `ca0cc2e`**: stream `DefsBuilder::write_to_string` directly ins… | 6.2% | **no** | not quoted | **no** | **SOUND-noNull** |
| 189 | L15270 | ⛔ REJECTED (wash): streaming the sequence ACTIVATION-BAR / lifecycle-marker loops (20… | 25.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 190 | L15385 | REJECT: direct-lower uppercase sequence messages to skip the boxed statement — flat/s… | 3.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 191 | L15769 | ❌REJECTED: pack terminal diff LCS scores from `usize` to `u16` — 4.284% slower (2026-… | 4.284% | **no** | not quoted | **no** | **SOUND-noNull** |
| 192 | L15999 | REJECT: sort contiguous terminal diff node indexes — 3.155% slower/noisy (2026-07-13) | 3.155% | **no** | not quoted | **no** | **SOUND-noNull** |
| 193 | L16028 | REJECT: pre-size terminal diff node output — 0.881% faster/noisy (2026-07-13) | 3.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 194 | L16085 | 🟢LANDED: reuse dead CSR degree storage for fill cursors — isolated sparse builder 35.… | 35.61% | **no** | not quoted | **no** | **SOUND-noNull** |
| 195 | L16126 | ❌REJECT (firms a prior soft-dismissal, now MEASURED at N=600): precompute node→style … | 11.2% | **no** | not quoted | **no** | **SOUND-noNull** |
| 196 | L16267 | REJECT: move C4 fast-node owned id/label into the fresh-node interner — flat/slower (… | 11.695% | **no** | not quoted | **no** | **SOUND-noNull** |
| 197 | L16291 | ⛔ REJECTED (wash): dense_node_rank one-walk (same map.get→dense-Vec pattern as nodes_… | 44.7% | **no** | not quoted | **no** | **SOUND-noNull** |
| 198 | L16379 | REJECT: collapse terminal Canvas pixel state into generation stamps — slower/wash (20… | 3.81% | **no** | not quoted | **no** | **SOUND-noNull** |
| 199 | L16436 | ❌REJECTED: DOT `find_edge_operator` no-dash fast path — dot_50 REGRESSED +8% (code-la… | 12.23% | **no** | not quoted | **no** | **SOUND-noNull** |
| 200 | L16459 | ❌REJECTED: `normalize_compound_identifier` already-clean fast path — WASH on gantt/jo… | 14.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 201 | L16514 | ❌REJECTED: BK `contains_key(&adjacent_rank)` → `ordered_ranks.binary_search` — scc_30… | 7.63% | **no** | not quoted | **no** | **SOUND-noNull** |
| 202 | L16584 | REJECT: borrow terminal diff edge labels before comparison — +20.5% slower/noisy (202… | 20.532% | **no** | not quoted | **no** | **SOUND-noNull** |
| 203 | L16612 | ❌REJECTED: `crossing_minimization_impl` dense_node_rank build via iterate-ranks — scc… | 8.77% | **no** | not quoted | **no** | **SOUND-noNull** |
| 204 | L16727 | ❌REJECTED: reuse the rank `Vec` buffer (get_mut+clear+extend) vs `insert(rank, collec… | 47.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 205 | L16788 | REJECT: iterate canvas text lines twice to avoid the transient `Vec<&str>` — 0.684% (… | 3.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 206 | L17040 | 🟢LANDED: remove the dead WASM render budget-ledger clone after warm-up (2026-07-14) | 35.484% | **no** | not quoted | **no** | **SOUND-noNull** |
| 207 | L17070 | 🔴REJECTED: streaming ASCII block detection regresses sparse documents (2026-07-14) | 4.206% | **no** | not quoted | **no** | **SOUND-noNull** |
| 208 | L17283 | 🔴REJECTED: pad owned terminal-diff rows in place (2026-07-14) | 13.619% | **no** | not quoted | **no** | **SOUND-noNull** |
| 209 | L17712 | 🔴REJECTED: borrow ordinary link targets during validation (2026-07-15) | 3.0% | **no** | not quoted | **no** | **SOUND-noNull** |
| 210 | L18134 | 🔴REJECTED: defer capability status-key ownership until the final map (2026-07-16) | 7.26% | **no** | not quoted | **no** | **SOUND-noNull** |
| 211 | L18292 | 🔴REJECTED: hoist the per-cell render-mode dispatch in Canvas::render_char_grid — inst… | 21.7% | **no** | not quoted | **no** | **SOUND-noNull** |
| 212 | L18358 | 🔴REJECTED: mask QuotientFilter circular slot steps — isolated query +4.364% slower (2… | 31.4% | **no** | not quoted | **no** | **SOUND-noNull** |
| 213 | L18585 | 🔴REJECTED (follow-up): the `draw_class_compartments` EXTRACTION does NOT fix the non-… | 28.1% | **no** | not quoted | **no** | **SOUND-noNull** |
| 214 | L18634 | 🔴REJECTED: emit fixed-width FNV-1a hex with direct nibble pushes — isolated primitive… | 51.43% | **no** | not quoted | **no** | **SOUND-noNull** |
| 215 | L18668 | REJECTED: axis-aligned CGA-skip in `vertical_nudge_for_obstacle` — CGA test reached 0… | 19.36% | **no** | not quoted | **no** | **SOUND-noNull** |
| 216 | L18808 | 🔴REJECTED: binary-search structured class definitions — realistic styled-300 path 13.… | 27.47% | **no** | not quoted | yes | **SOUND-noNull** |
| 217 | L18853 | REJECTED: `sort_by` → `sort_unstable_by` for the `cmp_by_id` layout sorts — +6..17% l… | 17.2% | **no** | not quoted | **no** | **SOUND-noNull** |
| 218 | L13614 | REJECT ARTIFACT / RETRY OPEN: clean packed-crossing timing ELF vanished before exact … | 24.56% | yes | 24.56% | yes | **SOUND** |
| 219 | L13565 | REJECT BUILD / RETRY OPEN: packed crossing counter wins timing but exact source fails… | 24.06% | yes | 24.06% | yes | **SOUND** |
| 220 | L56 | REJECT: streaming the flowchart line table is slower on both pinned large shapes (202… | 9.88% | yes | 9.88% | **no** | **SOUND** |
| 221 | L78 | REJECT: two-entry endpoint temporal cache misses flow gate and regresses wide (2026-0… | 49.9% | yes | 9.39% | **no** | **SOUND** |
| 222 | L38 | REJECT: fusing fast-edge reject/operator/chained scans regresses headline flow pipeli… | 8.53% | yes | 8.53% | **no** | **SOUND** |
| 223 | L24 | REJECT: canonical numeric node-index representation is slower (2026-07-22) | 13.52% | yes | 8.0% | **no** | **SOUND** |
| 224 | L13 | REJECT: endpoint-only numeric node-index representation is slower (2026-07-22) | 10.0% | yes | 1.0% | **no** | **SOUND** |
| 225 | L3 | REJECT: dense single-ID endpoint slot is slower (2026-07-22) | 10.1% | yes | not quoted | **no** | **SOUND** |
| 226 | L1896 | REJECT: fuse sequence entity-marker scans with `memchr2` — lint-clean source +3.8%, n… | 10.8% | yes | not quoted | **no** | **SOUND** |
| 227 | L2087 | REJECT: move terminal 1×1 edge components into `FlowAst` — paired inconclusive (2026-… | 4.285% | yes | not quoted | **no** | **SOUND** |
| 228 | L12938 | DIG / NO-SHIP: the large-diagram "double-copy" is OVER-ATTRIBUTED — it is ~3.4% of re… | 22.65% | yes | not quoted | **no** | **SOUND** |
| 229 | L18979 | REJECTED: minimap overlay row streaming — signal present, strict dispersion gate fail… | 15.11% | yes | not quoted | **no** | **SOUND** |
| 230 | L19001 | REJECT x3: bd-1buv.2 pinned large-flowchart node-metadata micro sweep (2026-07-24) | 5.0% | yes | not quoted | yes | **SOUND** |
| 231 | L7947 | FRONTIER MAP — the contained-lever frontier is closed; remaining wins are architectur… | 48.0% | **no** | 13.0% | **no** | **N/A-finding** |
| 232 | L102 | SURFACE / BLOCKER: two-entry endpoint temporal cache held by stale parser leases (202… | 49.9% | yes | 9.39% | **no** | **N/A-finding** |
| 233 | L14905 | SURFACE: mindmap full-pipeline (radial) is at a flat floor; layout compute-once lever… | 14.0% | **no** | 1.0% | **no** | **N/A-finding** |
| 234 | L4027 | Frontier closed: the last two candidate levers are not viable; git-tracked snapshots … | 5.0% | **no** | not quoted | **no** | **N/A-finding** |
| 235 | L6274 | MEASURED: incremental cache-hit is hash-bound (605us), NOT clone-bound (23us) — O(1) … | 4.0% | **no** | not quoted | **no** | **N/A-finding** |
| 236 | L6445 | MEASURED fresh lever: frankenmermaid's fixed CSS block is ~9.2 KB (2.2x mermaid) with… | 16.0% | **no** | not quoted | **no** | **N/A-finding** |
| 237 | L6606 | MEASURED: the CSS dead-weight wins CLOSED the sequence output gap (the last workload … | not quoted | **no** | not quoted | **no** | **N/A-finding** |
| 238 | L6659 | FRONTIER + BLOCKER: measurable render-perf levers exhausted; box-load blocks the rema… | 10.0% | **no** | not quoted | **no** | **N/A-finding** |
| 239 | L7216 | MEASURED + REVERTED: raising the parallel-render thread cap above 8 REGRESSES (box is… | 21.7% | **no** | not quoted | **no** | **N/A-finding** |
| 240 | L7286 | MEASURED: small-render fixed cost is ~130 us, 58% of it the output post-passes; clean… | 58.0% | **no** | not quoted | **no** | **N/A-finding** |
| 241 | L9980 | SURFACE (unmeasurable under load): memchr opt=3 candidate + persistent measurement bl… | 57.0% | **no** | not quoted | **no** | **N/A-finding** |
| 242 | L10080 | MEASURED (owner-gated lever closed): x86-64-v3 (AVX2) is a MIXED bag — net full-pipel… | 3.4% | **no** | not quoted | **no** | **N/A-finding** |
| 243 | L10198 | SURFACE: sequence render profiled (last un-profiled common type) — shares the blocked… | 26.0% | **no** | not quoted | **no** | **N/A-finding** |
| 244 | L10296 | SURFACE: Element-streaming (the last non-owner-gated structural lever) is ALSO ~5%-fl… | 26.0% | **no** | not quoted | **no** | **N/A-finding** |
| 245 | L10671 | SURFACE: clean measurable optimization frontier exhausted; only remaining gap is the … | 5.0% | **no** | not quoted | **no** | **N/A-finding** |
| 246 | L12908 | CORRECTION (self-caught): the `bd-w5sn` criterion A/B used an INVALID substrate — the… | 14.5% | yes | not quoted | **no** | **N/A-finding** |
| 247 | L14004 | FRONTIER + HOLD: parse-allocation new-primitive blocked by string-ownership origin in… | 9.0% | **no** | not quoted | **no** | **N/A-finding** |
| 248 | L14116 | SURFACE / REMOTE BLOCKER: ER entity-header direct lowering held before edit (2026-07-… | 9.2869% | **no** | not quoted | **no** | **N/A-finding** |
| 249 | L14181 | SURFACE / REMOTE BLOCKER: move-owned rich flowchart labels held before edit (2026-07-… | 13.27% | **no** | not quoted | **no** | **N/A-finding** |
| 250 | L14293 | FRONTIER + HOLD: arrowhead-marker `<defs>` memoization — measured ~1-6 µs/render, IMP… | not quoted | yes | not quoted | **no** | **N/A-finding** |
| 251 | L14679 | SURFACE / HOLD: timeline owned-ID handoff blocked before baseline by fleet-wide remot… | not quoted | **no** | not quoted | **no** | **N/A-finding** |
