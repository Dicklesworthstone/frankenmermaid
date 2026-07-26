# Performance Ledger — frankenmermaid (KEEP rows)

Campaign `perf-campaign-20260725` §4 splits the record: **KEEP → this file, REJECT →
`docs/NEGATIVE_EVIDENCE.md`.** Rows are never deleted.

Historically this repo recorded kept wins inside `docs/NEGATIVE_EVIDENCE.md` (see its "Kept Wins Also
Recorded Here By Request" section); those rows stay where they are. New KEEP rows land here.

Every row records: hypothesis · profile-first attribution (samples, % self-time) · the ONE lever ·
byte-identity proof · A/B **and A/A null** with worker id and binary sha · verdict · retry predicate.

---

## KEEP: drop the redundant membership hash probe — `arch_100x50` −2.92% instructions (2026-07-25)

**Bead:** `bd-1buv.69` (parent `bd-1buv`). **Lane:** cc/STRUCTURAL (`BoldPanther`).
**Full artifact:** `.benchmarks/bd_1buv_69_membership_hash_probe_WIN.md`.

- **Profile-first attribution.** Non-LTO (`lto=false strip=false debug=true`) symbolized build,
  `perf record -F 999`, single pinned core. On `arch_100x50` (5,000 nodes across 100 subgraphs) the
  **largest self-time frame** is `HashMap<(usize, IrNodeId), (), FxBuildHasher>::insert` at
  **7.19% / 8.73%** across two runs. It appears in **no** flowchart profile at any scale.
  This workload class did not exist before `bd-1buv.68`; the repo's only subgraph fixture has four
  nodes, so no benchmark had ever routed through the frame at scale. That is the
  `docs/LEDGER_RESURRECTION.md` §5 predicate in its third scope — workload class rather than
  algorithm or input property.
- **The ONE lever.** `IrBuilder::add_node_to_cluster` and `add_node_to_subgraph` each kept *two*
  membership records and consulted only the hash set for dedup. `ir.graph.nodes[i].{clusters,
  subgraphs}` is appended **only** in those two functions (`ir_builder.rs:1186`, `:1278` are the sole
  push sites), starts empty at node creation, and grows on exactly the calls that append to
  `members` — so the per-node list already *is* the membership index, and it is ~1 element long
  versus a hash probe into a set sized to the whole diagram. The probe is removed from the hot path
  and kept as the `None` arm for an id with no graph node (unreachable via the interner, since
  `ir.nodes` and `ir.graph.nodes` are pushed in lockstep; retained rather than assumed). Counted as
  one lever across two sites: a single monomorphized `insert` symbol served both.
- **Byte-identity, proven BEFORE timing.** All **19** corpus items identical, comparing the SHA-256
  of every revision's SVG concatenated (so the 201-document edit trace and the 40-diagram doc build
  are compared in full): **0 mismatches**. `flowchart_large_500` reproduces the already-recorded
  `408ecdcc…2d2d21`.
- **A/B + A/A null, arms alternated per round, instructions (`perf stat`), batch pinned to 1.**
  Both ELFs built back to back from explicit source states on the **same worker `hz2`**:
  base `d00c33b8c701005474f4431f…`, candidate `0c0b7469d44364050f100cd0…`.

  | workload | A/A null (median) | A/B (median) | verdict |
  |---|---:|---:|---|
  | `arch_100x50` | 1.000050 (range 0.999982–1.000204) | **0.970791** (range 0.970682–0.972152) | **−2.92%**, ~265× the null half-width; every round outside the entire A/A range |
  | `flowchart_large_500` (negative control, no subgraphs) | 1.000040 (range 0.998901–1.001798) | 1.000543 | +0.054%, **inside** the null range |

  Cycles corroborate at ~−2.8%. Wall clock, gate-passing rows only (two of five rounds discarded at
  MAD 11.4%/19.6%): 5.633 → 5.449 ms, −3.3%.
- **Why instructions is the gate here.** Not an ISA change — the candidate removes real work — so
  instruction count measures the mechanism rather than proxying for it, and it is deterministic and
  load-immune on a box at load ~12. Wall is corroboration.
- **Correctness.** `cargo test -p fm-parser` green on `ovh-a`; `cargo clippy -p frankenmermaid-cli
  --example headtohead -- -D warnings` clean; byte-identity above covers the subgraph-heavy,
  state-diagram and ER items that route through both changed functions.
- **Verdict: KEEP.** No feature flag: the change is byte-identical and monotonically less work, so a
  flag would only preserve a slower path with identical output. Rollback is a plain revert.
- **Retry / re-check predicate.** Re-open only if (1) a future change makes `ir.nodes` and
  `ir.graph.nodes` differ in length — which would move dedup onto the retained `*_member_set`
  fallback and is the one invariant this rests on — or (2) a profile of a subgraph-heavy workload
  shows `contains` on `graph_node.{clusters,subgraphs}` exceeding 3% self-time, which would mean the
  per-node lists have grown long enough (deep nesting) that a set is the right structure after all.

## CERTIFIED: ranked top-five VOID resurrection under the median-CI contract (2026-07-25)

**Bead:** `bd-1buv.67`. **Lane:** cod / HARNESS+FRONTIER (`CreamGorge`).
**Full artifact:** `.benchmarks/bd-1buv.67_harness_resurrection.md`.

The mechanical audit found **62 / 251 reject-class entries (24.7%)** whose original verdict was
VOID: 19 VOID-A plus 43 VOID-B. Only 22 / 251 had an A/A control and only 5 / 251 recorded a bench
binary SHA-256. The ranked top five were re-run on current code with an in-process ELF hash,
same-invocation A/A plus A/B, bootstrap median 95% CIs, and CV/MAD report-only:

| rank / resurrected lineage | A/A median 95% CI | A/B median 95% CI | disposition |
|---|---:|---:|---|
| #1 flat-CSR, SCC 100/300/800 | [0.9998, 1.0008] to [0.9992, 1.0003] | **2.835x / 4.348x / 7.997x** | certified re-win |
| #2 precomputed adjacency, SCC 100/300/800 | [0.9999, 1.0006] to [0.9996, 1.0009] | **10.900x / 19.981x / 41.646x** | certified re-win |
| #3 packed crossings, SCC 100/300/800 | [1.0000, 1.0003] to [1.0001, 1.0011] | **1.174x / 1.074x / 1.047x** | certified re-win |
| #4 short clean text escaping | [0.961192, 1.048964] | **1.387162x [1.354079, 1.420410]** | certified re-win |
| #5 borrowed Canvas dotted dash | [0.999427, 1.000666] | **3.793855x [3.792880, 3.796789]** | certified re-win |

Ranks 1-3 used self-reported ELF
`2439b3cad0ddd002ca7c697aa1d0ce6b21079b6c29038771dfa95705d2bd994c` on `ovh-a`;
rank 4 used `08d4042140f76ef95071f8872a4826b63666e947305fbf27d4fa64f906630f8c`
on `vmi1293453`; rank 5 used
`7d9314ae65055046e7b138fd6c3ea62345a02f444caa9afbc09f6e9c59c4c014`
on `ovh-a`. All arms proved exact parity.

**Retry / re-check predicate:** repeat a row only if its production lineage changes or a current
profile puts its target below 3% self-time. A retry remains admissible only with exact parity,
self-reported ELF identity, and the same-invocation A/A median-CI gate. Dense rank was also
re-decided and rejected; that result lives in `docs/NEGATIVE_EVIDENCE.md`.

## KEEP: feature-keyed transformed theme-CSS cache — `doc_build_40` +30.61% (2026-07-25)

**Bead:** `bd-1buv.67`. **Lane:** cod / HARNESS+FRONTIER (`CreamGorge`).
**Full artifact:** `.benchmarks/render_theme_css_minify_memoization_CANDIDATE.md`.

- **Profile-first attribution.** On the pinned 40-document workload,
  `render_svg_with_layout` was **20.02% self** and the theme-CSS post-pass
  `memchr::memmem::Finder::find_impl` was **9.00% self**. With its associated memmove, the fixed
  strip/minify block accounted for roughly 34% of the profile.
- **The ONE lever.** Cache only the transformed `<style>` bytes under an equality-checked key made
  from the exact raw CSS, state/accent/body-variable masks, and live built-in marker mask. The
  thread-local cache is bounded to 32 entries; unknown markers bypass it and the existing 100 KB
  post-pass cap is unchanged.
- **Correctness before timing.** All 40 timed outputs were byte-identical. A permanent miss/hit
  parity matrix covered the 40-document corpus plus dynamic classes/styles, special
  shapes/clusters, dotted markers, edgeless pie, four themes, effects/animation/print CSS, and the
  scene backend: **264 / 264 exact**. Unknown markers separately prove the conservative bypass.
- **Same-binary gate.** Strict-remote `ovh-a`, 41 interleaved rounds, minimum of three, cache cold
  at each timed 40-document batch. Self-reported ELF
  `85a81ad5c196a27ed56503ca7756c6a68e4de2fa74ef7a30c4cab8624dbad786`
  (10,931,720 bytes). A/A median **0.991926**, 95% CI **[0.988105, 1.000418]**.
  A/B median **1.306114**, 95% CI **[1.302648, 1.318433]**. The mandatory lower threshold was
  **1.030000**. CV was report-only at 3.27% / 3.36%.
- **Verdict: KEEP.** The exact cold docs-build corpus is **30.61% faster at the median** and the
  complete A/B CI clears twice the A/A null-CI margin.
- **Retry / re-check predicate.** Re-run the cold-per-batch probe if marker identities, any cached
  post-pass, the 100 KB cap, or representative cache cardinality changes. Revert only if the A/B
  lower CI crosses the mandatory 2x null threshold or any cached/uncached byte comparison fails.
