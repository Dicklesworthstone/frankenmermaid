# Ledger Resurrection — per-row table (six-class taxonomy)

Companion to [`LEDGER_RESURRECTION.md`](LEDGER_RESURRECTION.md) §7. Source: `docs/NEGATIVE_EVIDENCE.md`
@ `ca4e1d65`. **250 REJECT-verdict rows** of 668 entries, screened mechanically then hand-adjudicated
at the head of the queue (§7.3). Sorted VOID-NONULL → VOID-CV → VOID-ZEROSELF → VALID-*, then by
target-frame self-time.

`Screen` is the mechanical class. `Final` applies the structural-mechanism rescue of §7.4. Neither
column is a substitute for reading the row: three of the top six were overturned by hand.

| # | Line | Entry | Effect | Self-time | Null? | Counted mech? | Bin sha? | Screen | Final |
|---|---|---|---|---|:--:|:--:|:--:|---|---|
| 1 | L5352 | Layout: skip CGA test for axis-aligned segments in `find_*_segment_nudge_it… | 31.0% | 31.0% | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 2 | L7947 | FRONTIER MAP — the contained-lever frontier is closed; remaining wins are a… | 48.0% | 13.0% | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 3 | L9613 | REJECTED: parser byte-fast `trim_ws` (ASCII fast-path for `str::trim`) — WA… | 32.0% | 9.46% | **no** | no | yes | VOID-NONULL | VALID-MECHANISM (structural) |
| 4 | L5020 | Path `d` raw (escape-skip) serialization — REVERTED (2026-06-27) | 25.54% | 8.32% | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 5 | L7861 | write_fixed2 from_utf8 revalidation — CEILING ~0-gain, NOT PURSUED (2026-07… | 7.4% | 7.4% | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 6 | L5492 | build_smooth_path `d` capacity n*24 -> n*56 — REVERTED, load-contaminated +… | 26.2% | 7.11% | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 7 | L9250 | REJECTED: precompute u32 id-rank to replace cmp_by_id string comparison in … | 12.3% | 6.2% | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 8 | L14521 | REJECT: GitGraph current-head in-place update regresses the decisive MEDIAN… | 3.66% | 3.66% | **no** | no | yes | VOID-NONULL | VOID-NONULL |
| 9 | L8376 | INCONCLUSIVE/REVERTED: extend `trim_fast` to intern_node_auto `id.trim()` +… | 14.7% | 3.0% | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 10 | L9832 | REJECTED: memchr::memmem in strip_unused_state_css — WASH/tiny regression (… | 1.24% | 1.24% | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 11 | L14905 | SURFACE: mindmap full-pipeline (radial) is at a flat floor; layout compute-… | 14.0% | 1.0% | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 12 | L170 | <short-name> — REVERTED (<date>) | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 13 | L229 | Presize class/ER compartment children Vec - REVERTED (2026-07-04) | 10.8% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 14 | L254 | Sequence guardrail cost estimate (force Sequence layout, not Sugiyama) - RE… | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 15 | L401 | Document XML streaming + conditional edge-label CSS - REJECTED (2026-06-27) | 35.51% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 16 | L453 | Theme CSS sub-writer append path - REJECTED (2026-06-27) | 12.55% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 17 | L562 | SVG integer number manual writer - REJECTED (2026-06-26) | 20.51% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 18 | L611 | SVG static custom-attribute names — REJECTED (2026-06-26) | 13.28% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 19 | L660 | Edge `data-fm-edge-id` numeric value path — REJECTED (2026-06-26) | 44.38% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 20 | L742 | SVG root attribute direct streaming — REVERTED (2026-06-26) | 23.36% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 21 | L782 | SVG document child Vec capacity hint — REVERTED (2026-06-25) | 28.22% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 22 | L821 | Attributes Vec pre-size after edge-style fast path — CAUTION (2026-06-25) | 3.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 23 | L855 | Direct edge-path string emission — REJECTED (2026-06-25) | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 24 | L1702 | REJECT + SURFACE: pie render is 56% grisu (full-precision arc coords); inte… | 56.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 25 | L1833 | REJECT: `trim_fast` the remaining requirement header/relation trims — +13.5… | 18.412% | — | **no** | no | yes | VOID-NONULL | VOID-NONULL |
| 26 | L1937 | REJECT: sequence-only two-byte operator buckets — point estimate −5.1%, unc… | 18.0% | — | **no** | no | yes | VOID-NONULL | VOID-NONULL |
| 27 | L1984 | VOID / INVALID (correction): borrowed edge-label probe A/B missed the targe… | 7.5673% | — | **no** | no | yes | VOID-NONULL | VOID-NONULL |
| 28 | L2598 | Gate redundant node drop-shadow inline `filter` + its `<defs>` def on `embe… | 1.83% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 29 | L2720 | Drop dead `data-fm-node-id` node attribute (−1 to −2% SVG bytes, zero consu… | 2.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 30 | L2764 | Drop write-only `IrNode.span_all` accumulation (parse −12% large) — KEPT (2… | 21.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 31 | L2858 | Drop 6 redundant `data-fm-source-*` attributes (SVG −35% spans-on) — KEPT (… | 55.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 32 | L3312 | Edge path offset Vec elision — REJECTED (2026-06-25) | 28.3% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 33 | L3423 | Borrowed SVG attribute names — REJECTED (2026-06-25) | 28.46% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 34 | L3439 | Guarded SVG attribute retain skip — REJECTED (2026-06-25) | 27.21% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 35 | L3476 | Removing `Attributes::set` dedup entirely — REJECTED (correctness) (2026-06… | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 36 | L3506 | Owned accessibility title element path — REJECTED (2026-06-26) | 17.23% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 37 | L3538 | TextBuilder multiline Vec removal — REJECTED (2026-06-26) | 8.03% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 38 | L3571 | Common `-->` flowchart parser shortcut — REJECTED (2026-06-25) | 1.47% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 39 | L3591 | Plain flowchart label shortcut — REJECTED (2026-06-25) | 31.61% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 40 | L3651 | REJECTED: share `stable_node_priorities` across cycle_removal + rank_assign… | 29.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 41 | L3672 | REJECTED: cycle_removal acyclic strategy short-circuit (~0-gain) + re-profi… | 37.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 42 | L3721 | REJECTED: Brandes-Köpf neighbour precompute regresses +2-4% (neighbour reco… | 56.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 43 | L3772 | IrGraph build MEASURED ~0-gain (data closes the parked lever) (2026-06-27) | 5.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 44 | L3813 | IrGraph adapter build is dead in the render pipeline — but cheap, so a low-… | 5.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 45 | L3859 | Dead-CSS prune (~27% of the `<style>` is unused per diagram) — HIGH VALUE, … | 27.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 46 | L3886 | Byte-reduction frontier: CSS minify blocked; attr levers exhausted; post-ga… | 19.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 47 | L3931 | Node inline-style gating (the node analog of the landed edge fill/stroke ga… | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 48 | L3983 | CSS-building is sub-bar: stale 4 KB `to_svg_style` capacity (2 reallocs/ren… | 15.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 49 | L4027 | Frontier closed: the last two candidate levers are not viable; git-tracked … | 5.0% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 50 | L4092 | cmake-free `fm-parser` parse bench + the per-worker-target-dir A/B blocker … | 21.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 51 | L4127 | Edge routing is 85% of tree-path layout; `intersect_segment` bool variant —… | 15.4% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 52 | L4208 | Post-process `<style>` CSS minification — REJECTED (render +19%) (2026-06-2… | 19.39% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 53 | L4240 | Emit only used `<defs>` arrowhead markers — HIGH-VALUE LEVER, IMPLEMENTATIO… | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 54 | L4322 | Offset edge-point streaming path builder — REJECTED (2026-06-26) | 6.39% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 55 | L4363 | Agent Mail registration/reservation — BLOCKED (2026-06-24) | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 56 | L4377 | Local mermaid-js reference corpus — BLOCKED (2026-06-24) | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 57 | L4387 | `cargo bench --release` flag — BLOCKED (2026-06-24) | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 58 | L4399 | Cod-b mermaid-js denominator check — BLOCKED (2026-06-24) | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 59 | L4502 | Generated SVG id ownership — REVERTED (2026-06-27) | 1.65% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 60 | L4544 | Parser hash-key dedup maps — REVERTED (2026-06-27) | 55.7% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 61 | L4579 | Cluster CSS feature gate — REVERTED (2026-06-27) | 12.35% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 62 | L4621 | Element child Vec pre-sizing — REVERTED (2026-06-27) | 5.32% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 63 | L4665 | TextBuilder single-line line-vector skip - REVERTED (2026-06-27) | 28.15% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 64 | L4770 | Truncate-label byte-length guard - REVERTED (2026-06-27) | 28.35% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 65 | L4881 | Render: gated raw rect-node writer — REVERTED, mixed/noisy and small-size s… | 10.9% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 66 | L4953 | Attributes SmallVec inline storage — REVERTED (2026-06-27) | 44.37% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 67 | L5048 | Per-edge `pts` stack buffer (eliminate 1024 heap Vecs) — REVERTED, sub-bar/… | 23.5% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 68 | L5133 | Parse: borrowed fast-node document item — REJECTED, regression vs current O… | 56.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 69 | L5241 | write_int (direct integer serialization) — REVERTED, ~0-gain on re-confirm … | 13.2% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 70 | L5381 | Parse: borrowed simple-node IDs after borrowed-edge landing — REVERTED, no … | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 71 | L5400 | Element id builders: `format!` → direct push_str (drop format_inner) — KEPT… | 5.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 72 | L5465 | write_escaped_text: auto-vectorizable no-special fast-path — REVERTED, rend… | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 73 | L5514 | Render: rolling slice smooth-path helper after edge-stream + capacity wins … | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 74 | L5549 | Render frontier status + measurement blocker (post escape-win) (2026-06-27) | 57.0% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 75 | L5578 | Parse profile (post edge-borrow) + IR edge-capacity finding; load blocker p… | 41.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 76 | L5656 | Nodes are ~60% of render; narrow rect direct-byte is config-fragile (REVERT… | 60.0% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 77 | L5680 | Common gradient rect node shape direct-byte — byte-identical but ~0 at head… | 60.0% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 78 | L5846 | Flowchart edge-count capacity pre-scan — REJECTED, 1.58x-1.81x slower than … | 55.5% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 79 | L5909 | NO-SHIP: borrowed block-beta sort anchors regressed same-worker layout (202… | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 80 | L6112 | BLOCKER (peer-owned): incremental layout memo cache-hit is 2-4x SLOWER than… | — | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 81 | L6158 | Render streaming-serialization refactor QUANTIFIED as sub-noise — render fr… | 10.0% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 82 | L6274 | MEASURED: incremental cache-hit is hash-bound (605us), NOT clone-bound (23u… | 4.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 83 | L6445 | MEASURED fresh lever: frankenmermaid's fixed CSS block is ~9.2 KB (2.2x mer… | 16.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 84 | L6540 | REJECTED: block-beta CSS gate — const mismatch + a pre-existing block-diagr… | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 85 | L6606 | MEASURED: the CSS dead-weight wins CLOSED the sequence output gap (the last… | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 86 | L6648 | REVERTED: manual compact_display (byte-identical but unmeasurable + expecte… | 5.3% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 87 | L6659 | FRONTIER + BLOCKER: measurable render-perf levers exhausted; box-load block… | 10.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 88 | L6834 | REVERTED: bulk-copy minify hot loop -- sub-noise; the post-pass cost is the… | 24.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 89 | L6901 | REJECTED: general CSS class tree-shake -- real -6.5% corpus output, but +12… | 56.9% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 90 | L7054 | CLOSED (by experiment): the two render element-construction alloc/compute l… | 1.2% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 91 | L7074 | CLOSED: label-intern key-clone lever -- dedup is observable, only hash-dedu… | 11.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 92 | L7216 | MEASURED + REVERTED: raising the parallel-render thread cap above 8 REGRESS… | 21.7% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 93 | L7233 | FOUND (near-latent perf bug, NOT yet fixable safely): the node fast-path is… | 60.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 94 | L7286 | MEASURED: small-render fixed cost is ~130 us, 58% of it the output post-pas… | 58.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 95 | L7350 | TESTED + REVERTED: CSR on the component-detection adjacency builders is byt… | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 96 | L7930 | build_tree_layout_structure integer-rank sort keys — REVERTED, ~0-gain (202… | 30.0% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 97 | L8074 | LUT for is_fast_flow_identifier + fast-node forbidden scan — REVERTED, ~0-g… | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 98 | L8264 | REVERTED: class parser `in_block` bool flag is below the keep floor (2026-0… | 1.7435% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 99 | L8316 | REVERTED: path-writer `push(char)` → `push_str(1-byte literal)` — render re… | 10.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 100 | L8400 | REVERTED: drop `ObstacleSpatialIndex::query_segment` per-query sort (min-in… | 9.6% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 101 | L8493 | REVERTED: no-quote/bracket fast path for `find_operator_from_index` — ~0 ga… | 8.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 102 | L8639 | REJECTED (alien-graveyard §7.9): mimalloc global allocator for parse — mixe… | 37.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 103 | L9010 | find_operator_core: `char_indices` → byte loop — REJECTED (regression) (202… | 8.4% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 104 | L9056 | CgaRect::segment_crosses (boolean early-return) for obstacle nudge — REJECT… | 56.7% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 105 | L9129 | Remove the `retain` dedup from `Attributes::set` — REJECTED, reintroduces d… | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 106 | L9332 | REJECTED: pre-size build_edge_paths_with_orientation's filter_map collect —… | 2.7% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 107 | L9379 | REJECTED: minify_css Vec<u8>+from_utf8 → String+push_str runs — ~0.2% pipel… | 13.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 108 | L9476 | REJECTED: skip obstacle index for global-edge (mindmap) layouts — WASH; the… | 23.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 109 | L9501 | REJECTED (definitively): optimizing/removing the Attributes::set retain — p… | 2.0% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 110 | L9567 | REJECTED: fuse strip_dead_marker_css + minify_style_block (one find + one r… | 1.1% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 111 | L9851 | REJECTED: memmem + in-place replace_range for strip_unused_theme_css's str:… | 6.3% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 112 | L9929 | REJECTED: memchr for single-byte per-line gates in the parser — 7.4% parse … | 11.6% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 113 | L9953 | REJECTED: presize the flowchart line Vec<(usize,&str)> — count scan costs m… | 28.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 114 | L9980 | SURFACE (unmeasurable under load): memchr opt=3 candidate + persistent meas… | 57.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 115 | L10173 | FRESH PROFILE (this segment's wins confirmed) + REJECTED state_css </style>… | 29.0% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 116 | L10198 | SURFACE: sequence render profiled (last un-profiled common type) — shares t… | 26.0% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 117 | L10296 | SURFACE: Element-streaming (the last non-owner-gated structural lever) is A… | 26.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 118 | L10364 | REVERTED: precomputed SVG state/accent CSS usage mask is below landability … | 3.29% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 119 | L10472 | REJECTED: bounded per-node alloc elision in `render_class_compartments` is … | 5.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 120 | L10640 | REJECTED: block-beta source_line borrow refactor is below the parse layout-… | 3.9% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 121 | L10671 | SURFACE: clean measurable optimization frontier exhausted; only remaining g… | 5.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 122 | L10749 | REJECTED: TextBuilder move-owned-fields (avoid double-clone) is below the r… | 10.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 123 | L11088 | NO-SHIP: pie owned path/text strings (render regression) (2026-07-04) | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 124 | L11904 | NO-SHIP (redundant — peer landed it): polygon shape streaming frontier clos… | — | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 125 | L11983 | NO-SHIP: large-diagram `to_string_with_body` streaming regresses render (20… | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 126 | L12225 | NO-SHIP: class parser simple-relationship fusion regressed class_100 on sam… | 6.8717% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 127 | L12252 | NO-SHIP: class parser dense member insertion plus borrowed cardinality text… | 16.25% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 128 | L12426 | NO-SHIP: large raw-part body fusion regresses large wide render (2026-07-10) | 26.1% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 129 | L13940 | REJECT: stream class-compartment member rows (remove per-row format! alloc)… | 40.0% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 130 | L14004 | FRONTIER + HOLD: parse-allocation new-primitive blocked by string-ownership… | 9.0% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 131 | L14033 | REJECT: partial ER attribute-row streaming (raw_svg child) — wash; need WHO… | 50.0% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 132 | L14053 | REJECT: whole-entity ER streaming fast path — byte-identical but REGRESSES … | 20.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 133 | L14116 | SURFACE / REMOTE BLOCKER: ER entity-header direct lowering held before edit… | 9.2869% | — | **no** | no | yes | VOID-NONULL | VOID-NONULL |
| 134 | L14181 | SURFACE / REMOTE BLOCKER: move-owned rich flowchart labels held before edit… | 13.27% | — | **no** | no | yes | VOID-NONULL | VOID-NONULL |
| 135 | L14409 | REJECT: memoize `effects_css` (and the memoize-per-render pattern for CSS f… | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 136 | L14506 | REJECT: stream the a11y `describe_diagram_with_layout` description (format!… | 2.65% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 137 | L14679 | SURFACE / HOLD: timeline owned-ID handoff blocked before baseline by fleet-… | — | — | **no** | no | yes | VOID-NONULL | VOID-NONULL |
| 138 | L14705 | REJECT: architecture group index `BTreeMap<String, _>` to `FxHashMap` (2026… | 19.84% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 139 | L14730 | REJECT: Kanban dense-ID class appends miss the decisive large-row floor (20… | 6.986% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 140 | L14761 | REJECT: `coordinate_assignment` per-rank `Vec<usize>` clone elision is flat… | 5.8929% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 141 | L14791 | REJECT: BK direction iterators do not prove a stable gain (2026-07-12) | 8.9704% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 142 | L14819 | REJECT: timeline owned-ID handoff is flat-to-slower (2026-07-12) | 2.0961% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 143 | L14941 | REJECT: skip building the throwaway value String for non-matching DOT attri… | 12.7% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 144 | L15385 | REJECT: direct-lower uppercase sequence messages to skip the boxed statemen… | 3.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 145 | L15544 | ⛔ HOLD / INVALID: direct terminal `CellBuffer` serialization never reached … | — | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 146 | L15573 | ⛔ HOLD / INVALID RETRY: direct terminal `CellBuffer` serialization again mi… | — | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 147 | L15629 | ⛔ HOLD / INVALID: stream ASCII block line iteration never reached timed pat… | — | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 148 | L15769 | ❌REJECTED: pack terminal diff LCS scores from `usize` to `u16` — 4.284% slo… | 4.284% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 149 | L15999 | REJECT: sort contiguous terminal diff node indexes — 3.155% slower/noisy (2… | 3.155% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 150 | L16028 | REJECT: pre-size terminal diff node output — 0.881% faster/noisy (2026-07-1… | 3.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 151 | L16057 | ❌REJECT: drop the per-line `format!` temporary in the render_diff output lo… | 1.571% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 152 | L16126 | ❌REJECT (firms a prior soft-dismissal, now MEASURED at N=600): precompute n… | 11.2% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 153 | L16267 | REJECT: move C4 fast-node owned id/label into the fresh-node interner — fla… | 11.695% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 154 | L16379 | REJECT: collapse terminal Canvas pixel state into generation stamps — slowe… | 3.81% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 155 | L16436 | ❌REJECTED: DOT `find_edge_operator` no-dash fast path — dot_50 REGRESSED +8… | 12.23% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 156 | L16459 | ❌REJECTED: `normalize_compound_identifier` already-clean fast path — WASH o… | 14.0% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 157 | L16514 | ❌REJECTED: BK `contains_key(&adjacent_rank)` → `ordered_ranks.binary_search… | 7.63% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 158 | L16584 | REJECT: borrow terminal diff edge labels before comparison — +20.5% slower/… | 20.532% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 159 | L16612 | ❌REJECTED: `crossing_minimization_impl` dense_node_rank build via iterate-r… | 8.77% | — | **no** | no | **no** | VOID-NONULL | VALID-MECHANISM (structural) |
| 160 | L16727 | ❌REJECTED: reuse the rank `Vec` buffer (get_mut+clear+extend) vs `insert(ra… | 47.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 161 | L16771 | ❌REJECTED (confirm): egraph `layer_edges` dense-probe on `layout_dense` (th… | 1.8% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 162 | L17070 | 🔴REJECTED: streaming ASCII block detection regresses sparse documents (2026… | 4.206% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 163 | L17283 | 🔴REJECTED: pad owned terminal-diff rows in place (2026-07-14) | 13.619% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 164 | L17580 | 🟡INVALID/HOLD: Canvas class-compartment font hoist did not reach remote pro… | — | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 165 | L17712 | 🔴REJECTED: borrow ordinary link targets during validation (2026-07-15) | 3.0% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 166 | L18634 | 🔴REJECTED: emit fixed-width FNV-1a hex with direct nibble pushes — isolated… | 51.43% | — | **no** | no | yes | VOID-NONULL | VOID-NONULL |
| 167 | L18668 | REJECTED: axis-aligned CGA-skip in `vertical_nudge_for_obstacle` — CGA test… | 19.36% | — | **no** | no | **no** | VOID-NONULL | VOID-NONULL |
| 168 | L13590 | REJECT MEASUREMENT / RETRY OPEN: lint-clean packed crossing run has no dual… | 24.56% | 24.56% | yes | no | yes | VOID-CV | VOID-CV |
| 169 | L19064 | BLOCKER / REJECT: bd-1buv.2 full parse-layout-SVG micro-frontier is closed … | 28.57% | 8.0% | yes | no | **no** | VOID-CV | VOID-CV |
| 170 | L3261 | Barycenter sweep precomputed edge adjacency — ~~REJECTED~~ **MEASUREMENT IN… | 47.64% | 47.64% | **no** | no | **no** | VOID-ZEROSELF | VOID-ZEROSELF |
| 171 | L2355 | REJECT (WASH): clean-scan fast path for `write_escaped_text` short labels —… | 33.0% | — | **no** | yes | **no** | VOID-ZEROSELF | VOID-ZEROSELF |
| 172 | L3341 | Dense crossing-count position maps — ~~REJECTED~~ **MEASUREMENT INVALID (bu… | 23.8% | — | **no** | no | **no** | VOID-ZEROSELF | VOID-ZEROSELF |
| 173 | L3373 | Flat-array `total_crossings` position/edge tables — ~~REJECTED~~ **MEASUREM… | 47.64% | — | **no** | no | **no** | VOID-ZEROSELF | VOID-ZEROSELF |
| 174 | L8089 | graph_metrics_cache_key inline-hash (drop the throwaway resolved_edges Vec)… | 2.0% | — | **no** | no | **no** | VOID-ZEROSELF | VOID-ZEROSELF |
| 175 | L12908 | CORRECTION (self-caught): the `bd-w5sn` criterion A/B used an INVALID subst… | 14.5% | — | yes | no | **no** | VOID-ZEROSELF | VOID-ZEROSELF |
| 176 | L16634 | ❌REJECTED: delete the (provably dead) per-node `outgoing.sort_by` in `rank_… | 7.6% | — | **no** | no | **no** | VOID-ZEROSELF | VOID-ZEROSELF |
| 177 | L16747 | ❌REJECTED: dense_node_rank in egraph `layer_edges_between_ranks` (probe→den… | 3.6% | — | **no** | no | **no** | VOID-ZEROSELF | VOID-ZEROSELF |
| 178 | L16912 | 🟡INVALID / HOLD: borrow the legacy Canvas dotted-edge dash slice (2026-07-1… | 3.0% | — | **no** | yes | **no** | VOID-ZEROSELF | VOID-ZEROSELF |
| 179 | L18561 | 🔴REJECTED (as-is; needs function extraction): reuse a scratch buffer for cl… | 29.0% | — | **no** | yes | **no** | VOID-ZEROSELF | VOID-ZEROSELF |
| 180 | L13134 | INTEGRITY AUDIT + REJECT: the 3 double-copy rejections HOLD; capacity pre-s… | 30.0% | 21.79% | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 181 | L885 | Make `write_uint_into` inlinable by cold-splitting its recursion — REVERTED… | 28.0% | 15.0% | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 182 | L12779 | REJECTED: word-packed incremental span hashing misses the keep gate (2026-0… | 51.37% | 15.0% | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 183 | L2114 | REJECT: fuse the fast-node reject + `[`-locate scans into one table-driven … | 10.17% | 10.17% | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 184 | L5270 | Parse: eliminate per-line `line_items` Vec in `parse_flowchart_document_ite… | 17.0% | 7.95% | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 185 | L8844 | node-path ends_with/strip_suffix ']' → byte ops — REJECTED (~0, rch false-p… | 16.0% | 6.6% | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 186 | L2157 | REJECT: `memchr::memchr` for `\n` in `ByteLines::next` — +0.26% parse REGRE… | 5.31% | 5.31% | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 187 | L1875 | WASH: move-through `normalize_sequence_display_text` to skip the entity-dec… | 2.44% | 2.44% | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 188 | L13804 | REJECTED (~0, free-list-recycled) + FRONTIER SURFACE: BK median-of-four per… | 14.6% | 1.38% | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 189 | L182 | Class member label allocation rewrite - REVERTED (2026-07-04) | 21.11% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 190 | L938 | `write_escaped_text` single-pass tight-reject restructure — REVERTED, wash … | 0.22% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 191 | L1896 | REJECT: fuse sequence entity-marker scans with `memchr2` — lint-clean sourc… | 10.8% | — | yes | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 192 | L1917 | REJECT: first-byte guard before `starts_with` in the operator scan — wins s… | 13.4% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 193 | L2039 | REJECT: borrow state-label cleanup and action prefixes with `Cow` — +11.52%… | 11.52% | — | **no** | yes | yes | VALID-MECHANISM | VALID-MECHANISM |
| 194 | L2063 | REJECT (valid retry): borrow cleaned edge-label probes into the interner — … | 8.3274% | — | **no** | yes | yes | VALID-MECHANISM | VALID-MECHANISM |
| 195 | L2087 | REJECT: move terminal 1×1 edge components into `FlowAst` — paired inconclus… | 4.285% | — | yes | yes | yes | VALID-MECHANISM | VALID-MECHANISM |
| 196 | L2373 | REJECT: consolidate `extract_style_directives`'s 3-scan gate to 2 via a sho… | 4.4% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 197 | L3834 | Dead-CSS prune VALIDATED — landed concurrently; standing down from fm-rende… | 27.0% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 198 | L3958 | Dead-output sweep of the benched render is exhausted after the `data-fm-nod… | — | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 199 | L4289 | `fm-source-span` static-name + `data_owned` allocation trim — ZERO-GAIN (20… | 3.0% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 200 | L5734 | Full-node direct-byte: byte-identical but ~0 (sub-noise) — REVERTED; correc… | 35.0% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 201 | L6945 | REJECTED: render-path alloc reduction -- hot buffers already pre-sized; rem… | 12.0% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 202 | L7880 | with_capacity_hint edges = input_lines (vs input_lines/3) — REVERTED, sub-b… | 52.0% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 203 | L8817 | query_segment single-cell candidate fast-path — REJECTED (~0 gain) (2026-07… | 16.0% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 204 | L8870 | itoa crate for write_uint_into — REJECTED (regression) (2026-07-02) | 12.2% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 205 | L8891 | layout sort_by → sort_unstable_by (19 usize index-tiebreak sorts) — REJECTE… | 40.0% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 206 | L9588 | RE-REJECTED: strip_unused_theme_css str::replace → memmem::find + replace_r… | 7.9% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 207 | L10004 | REJECTED (now MEASURED): memchr opt=3 — ~1.8% render REGRESSION, not a win … | 1.8% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 208 | L10080 | MEASURED (owner-gated lever closed): x86-64-v3 (AVX2) is a MIXED bag — net … | 3.4% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 209 | L10152 | REJECTED: skip-escape of marker-end (generated url ref) — WASH (short value… | 3.7% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 210 | L10221 | REJECTED: bounded .fm-cluster search in strip_unused_state_css — CODE-LAYOU… | 26.0% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 211 | L11262 | NO-SHIP (reverted ~0-gain): content-aware edge-buffer presize for cardinali… | 24.0% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 212 | L11411 | NO-SHIP (both reverted): first-byte gate does NOT generalize to find_operat… | 8.9% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 213 | L11496 | NO-SHIP: pre-size the streamed pie raw fragment (same-worker no-change, rev… | 13.34% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 214 | L11583 | NO-SHIP (reverted ~0-gain): make the common-node fast-path `user_class_suff… | 9.0% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 215 | L11791 | NO-SHIP (4 washes reverted): constant-factor micro-opts after the scaling f… | 0.03% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 216 | L12517 | NO-SHIP: large render empty between-child guards are flat/noise (2026-07-10) | 24.11% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 217 | L12592 | REJECTED: topology-stable incremental dependency-cache hit patch is flat/sl… | 47.51% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 218 | L12938 | DIG / NO-SHIP: the large-diagram "double-copy" is OVER-ATTRIBUTED — it is ~… | 22.65% | — | yes | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 219 | L14880 | REJECT: memmem the `parse_init_directives` `%%{` gate — input-dependent, re… | 0.4% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 220 | L14971 | ~~REJECT~~ → **LANDED `ca0cc2e`**: stream `DefsBuilder::write_to_string` di… | 6.2% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 221 | L15270 | ⛔ REJECTED (wash): streaming the sequence ACTIVATION-BAR / lifecycle-marker… | 25.0% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 222 | L16291 | ⛔ REJECTED (wash): dense_node_rank one-walk (same map.get→dense-Vec pattern… | 44.7% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 223 | L16788 | REJECT: iterate canvas text lines twice to avoid the transient `Vec<&str>` … | 3.0% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 224 | L16820 | 🟡INVALID / HOLD: remove the dead WASM render budget-ledger clone (2026-07-1… | — | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 225 | L16866 | 🟡INVALID / HOLD: count compact terminal node-label width without `Vec<char>… | — | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 226 | L18134 | 🔴REJECTED: defer capability status-key ownership until the final map (2026-… | 7.26% | — | **no** | yes | yes | VALID-MECHANISM | VALID-MECHANISM |
| 227 | L18292 | 🔴REJECTED: hoist the per-cell render-mode dispatch in Canvas::render_char_g… | 21.7% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 228 | L18358 | 🔴REJECTED: mask QuotientFilter circular slot steps — isolated query +4.364%… | 31.4% | — | **no** | yes | yes | VALID-MECHANISM | VALID-MECHANISM |
| 229 | L18585 | 🔴REJECTED (follow-up): the `draw_class_compartments` EXTRACTION does NOT fi… | 28.1% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 230 | L18808 | 🔴REJECTED: binary-search structured class definitions — realistic styled-30… | 27.47% | — | **no** | yes | yes | VALID-MECHANISM | VALID-MECHANISM |
| 231 | L18853 | REJECTED: `sort_by` → `sort_unstable_by` for the `cmp_by_id` layout sorts —… | 17.2% | — | **no** | yes | **no** | VALID-MECHANISM | VALID-MECHANISM |
| 232 | L13338 | REJECTED SAMPLE / RETRY OPEN: flat-CSR barycenter direction is large, but l… | 47.3% | 76.84% | yes | no | yes | VALID-AB | VALID-AB |
| 233 | L13614 | REJECT ARTIFACT / RETRY OPEN: clean packed-crossing timing ELF vanished bef… | 24.56% | 24.56% | yes | no | yes | VALID-AB | VALID-AB |
| 234 | L13565 | REJECT BUILD / RETRY OPEN: packed crossing counter wins timing but exact so… | 24.06% | 24.06% | yes | no | yes | VALID-AB | VALID-AB |
| 235 | L56 | REJECT: streaming the flowchart line table is slower on both pinned large s… | 9.88% | 9.88% | yes | no | **no** | VALID-AB | VALID-AB |
| 236 | L78 | REJECT: two-entry endpoint temporal cache misses flow gate and regresses wi… | 49.9% | 9.39% | yes | no | yes | VALID-AB | VALID-AB |
| 237 | L102 | SURFACE / BLOCKER: two-entry endpoint temporal cache held by stale parser l… | 49.9% | 9.39% | yes | no | **no** | VALID-AB | VALID-AB |
| 238 | L38 | REJECT: fusing fast-edge reject/operator/chained scans regresses headline f… | 8.53% | 8.53% | yes | no | **no** | VALID-AB | VALID-AB |
| 239 | L24 | REJECT: canonical numeric node-index representation is slower (2026-07-22) | 13.52% | 8.0% | yes | no | **no** | VALID-AB | VALID-AB |
| 240 | L13 | REJECT: endpoint-only numeric node-index representation is slower (2026-07-… | 10.0% | 1.0% | yes | no | **no** | VALID-AB | VALID-AB |
| 241 | L3 | REJECT: dense single-ID endpoint slot is slower (2026-07-22) | 10.1% | — | yes | no | **no** | VALID-AB | VALID-AB |
| 242 | L13362 | REJECTED SAMPLE / RETRY OPEN: quiescent flat-CSR run identifies the 2 ms sa… | 25.78% | — | yes | no | yes | VALID-AB | VALID-AB |
| 243 | L13380 | REJECTED SAMPLE / RETRY OPEN: 20 ms flat-CSR samples improve dispersion but… | 25.93% | — | yes | no | yes | VALID-AB | VALID-AB |
| 244 | L13397 | REJECTED SAMPLE / RETRY OPEN: 200 ms whole-arm pairs still track co-tenant … | 33.28% | — | yes | no | yes | VALID-AB | VALID-AB |
| 245 | L14282 | REJECT: memoize `Theme::to_svg_style` output — build is only ~772 ns, cachi… | — | — | yes | no | **no** | VALID-AB | VALID-AB |
| 246 | L14293 | FRONTIER + HOLD: arrowhead-marker `<defs>` memoization — measured ~1-6 µs/r… | — | — | yes | no | **no** | VALID-AB | VALID-AB |
| 247 | L18979 | REJECTED: minimap overlay row streaming — signal present, strict dispersion… | 15.11% | — | yes | no | **no** | VALID-AB | VALID-AB |
| 248 | L19001 | REJECT: dense-rank crossing stage is decisively slower on current SCC fixtu… | 20.7% | — | yes | no | yes | VALID-AB | VALID-AB |
| 249 | L19028 | REJECT x3: bd-1buv.2 pinned large-flowchart node-metadata micro sweep (2026… | 5.0% | — | yes | no | yes | VALID-AB | VALID-AB |
| 250 | L19089 | BLOCKER / NEGATIVE: bd-1buv.2 current-head frontier revalidated after expli… | 10.0% | 8.0% | **no** | no | yes | VALID-PROFILE | VALID-PROFILE |
