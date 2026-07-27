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

## CERTIFIED INFRASTRUCTURE: fleet harness rollout and unmeasured corpus admission (2026-07-26)

**Bead:** `bd-jil4`. **Lane:** L / LEDGER+LOW BURN (`CreamGorge`).
**Full status:** `docs/HARNESS_CONTRACT_ADOPTION.md`.

- **No performance claim.** Lane L requested no RCH worker and timed neither engine. This row
  certifies reproducible measurement infrastructure, not a speedup.
- **Fleet contract status.** All 11 campaign repositories were audited against code. Ten have
  executing-ELF SHA-256, same-invocation A/A, and median-CI verdict machinery in at least one
  harness. franken_numpy still lacks executing-ELF self-report and a true bootstrap median-CI gate.
  Live hard-CV verdicts remain in frankenredis, frankenlibc, and frankenpandas; franken_networkx has
  only one fully converted harness. Exact corrective handoffs were sent to the active owner in all
  ten other repositories under Agent Mail thread `harness-contract-20260725`.
- **New unmeasured boundaries.** The additive corpus now includes:
  `er_schema_5000x8` (`f43a2b41…e46c56`), `er_schema_10000x8`
  (`185a597d…7b341`), `edit_trace_500x1000` (`4813beb8…aa2cb`), and
  `ci_batch_500` (`65b8f69a…de432f`). Together they cover 5k/10k-node ER, 1,001 live revisions,
  and a 500-diagram CI job; the existing 5k/10k architecture endpoints complete the named range.
- **Construction proof.** Both JavaScript modules pass `node --check`; importing the deterministic
  generators yields 25 unique items; every generated hash matches `pins.json`; and all 21 older
  hashes remain unchanged. The new trace has 1,001 revisions and the CI batch has 500.
- **Measurement / re-check predicate.** Do not time these rows until the campaign grants
  frankenmermaid a quiet measurement window. When granted, use the existing self-ELF provenance,
  byte-determinism, DNF classification, and same-invocation noise-floor contract. Re-audit a fleet
  repo only after it claims a gate change; the decisive check is whether its next REJECT cites CV.
  The deferred measurement window is tracked by `bd-ktx5`.

## CORRECTION: the live head-to-head now satisfies harness parts 1–3 (2026-07-26)

**Bead:** `bd-w1po`. **Lane:** L / LEDGER+LOW BURN (`CreamGorge`).
**Contract detail:** `docs/HARNESS_CONTRACT_ADOPTION.md`.

- **Why this correction exists.** The `bd-jil4` fleet audit found A/A and bootstrap-CI helpers in
  the repo and marked frankenmermaid complete without tracing the published head-to-head exit path.
  That path self-reported its ELF, but ran no A/A control and exited on a 5% MAD threshold. The
  status claim was therefore too generous even though the fleet inventory was otherwise useful.
- **Live contract now.** The Rust process calls one paired routine as A/A and then A/B, timing both
  arms back-to-back inside each round with alternating order, one batch, median of per-round ratios,
  deterministic bootstrap 95% CIs, and output SHA-256 checksums. The driver rejects a missing or
  malformed executing-ELF self-report. The Chromium process emits its own paired A/A null. The
  cross-runtime gate uses the larger per-engine null-CI radius and requires
  `magnitude >= max(1.01, 1 + 2 * radius)`. Every record says `cv_gate=never`; CV and MAD are
  report-only. A missing or fewer-than-nine-round null fails closed.
- **No performance claim.** No corpus item was timed: Lane L still requires the campaign-owned
  quiet window for the new architecture/ER, live-edit, and CI-scale classes. This is harness
  conformance, not a speedup.
- **Validation.** Both JavaScript files pass `node --check`; both deterministic self-test modes
  pass. Strict-remote focused Rust tests passed on `ovh-a` (2/2). Strict-remote workspace check and
  workspace Clippy with `-D warnings` passed on `hz2`. The broader workspace test run reached an
  unrelated pre-existing `gantt_basic` golden-SVG hash mismatch; the modified example/driver is not
  in that render path.
- **Retry / re-check predicate.** Re-open immediately if an `ok` head-to-head row can be emitted
  without a self-reported executing ELF hash, two sufficient per-engine A/A controls, or
  `median_ci_gate.rule == "null_ci95_2x_margin"`; or if any live exit path again depends on CV or
  MAD. Run the unmeasured corpus only when `bd-ktx5` receives a quiet measurement window.

## MODEL INTEGRITY RE-AUDIT: fallback-window commits (2026-07-27)

**Bead:** `bd-yp9s`. **Audited window:** 2026-07-25 20:40 through 2026-07-26 00:35
America/New_York. **Scope:** the exact 11 commits authored on `main` during the provider's silent
model fallback.

This was a judgment audit, not a wholesale remeasurement. In-process ELF self-reports,
same-invocation A/A controls, exact byte/parity proofs, and raw comparator failures were preserved.
Each commit was re-read for workload routing, proof soundness, whether its own numbers support its
verdict, and code quality outside the original gates. `CORRECTED` means a durable later correction
or this re-audit narrows a claim; it does not discard unaffected artifacts.

| Commit | Verdict | Re-audit finding and concrete re-check predicate |
|---|---|---|
| `bb84aa3b` | **CORRECTED** | The generators, pins, process-self ELF report, and 16 completed rows route through the intended full pipelines. Its first XL artifact nevertheless assigned budget-derived lower bounds to three `RangeError` failures that ended after 6–15 seconds; a failure is not a timeout. `d229bdd7` supplies the corrected `kind=failed`, `CANNOT`, no-ratio artifact. Re-check only if DNF classification changes; only `kind=timeout` may carry a budget lower bound. |
| `243f9586` | **SOUND** | This is a clean two-parent merge with no combined conflict-resolution diff. Parent two is the independently tested dependency/API update; the lowercase digest encoder preserves the prior SHA-256 text contract. Re-check if a bumped dependency changes output hashes, golden SVGs, or the digest API. |
| `88ade078` | **CORRECTED** | The four discovery workloads genuinely route through the named frames: the subgraph hash insert appears only at architecture scale and the fixed CSS pass is paid by every document in `doc_build_40`. The profiles were correctly labelled non-LTO discovery evidence. Its provisional resurrection ratios and two-class ranking were later superseded by the final artifact in `1049817b` and the strict six-class audit in §10 of `docs/LEDGER_RESURRECTION.md`. Re-profile if a generator, routing selector, or production lineage changes. |
| `16c2bb96` | **SOUND** | The parser KEEP measured the right path: `arch_100x50` makes the removed membership probe the top frame, while the no-subgraph control stays inside its A/A range. The behavior proof holds: both builder node-issuance sites append `ir.nodes` and `ir.graph.nodes` in lockstep, and the only builder mutation sites for the mirror memberships are the two changed functions. Exact 19-item output identity plus −2.92% instructions outside the complete null range justify KEEP. Re-open on vector-length divergence or if deep nesting puts mirror `contains` above 3% self-time. |
| `d229bdd7` | **SOUND** | Raw rows record `kind=failed`, `RangeError`, positive elapsed time, `speedup_lower_bound=null`, and a self-reported ELF; the commit correctly replaces the flattering DNF bounds with `CANNOT`. Re-check if mermaid's pinned version or error/timeout mapping changes. |
| `470ca188` | **SOUND** | Documentation-only provenance correction: it distinguishes one green parser test from later RCH admission refusals and does not turn an exit-1 infrastructure refusal into a test failure. The 19-item byte proof remains the correctness gate. Re-check only if the cited artifact or RCH result is amended. |
| `1049817b` | **CORRECTED** | The transformed-CSS cache KEEP stands: raw CSS is equality-checked; state, accent, body-variable, and live-marker observations cover every post-pass dependency; unknown markers bypass; the cache is thread-local and bounded; 264/264 permanent comparisons and the timed 40-document outputs are exact. Its 1.306114x median and CI [1.302648, 1.318433] clear the A/A-derived 1.03 floor. The five resurrection measurements also retain ELF/A/A/parity evidence, but calling them the strict “ranked top five” was wrong; §10.3 owns the unresolved strict queue. Re-check the KEEP on any cache-key/post-pass/marker/cap change. |
| `83533872` | **CORRECTED** | The 10k architecture and 2.5k ER inputs, hashes, self-ELF records, and mermaid `RangeError` failures are real. The commit wrongly marked frankenmermaid P2/P3-complete by finding helpers outside the published decision path, and selected absolute Rust latency ranges with the old MAD gate and no paired A/A. `8379f65e` fixed the live path; this audit reclassifies those absolute timings as exploratory in `README.md`. Re-certify them only through `bd-ktx5` under v2; the comparator `CANNOT` rows stand unless the mermaid pin changes. |
| `2fef0bf7` | **CORRECTED** | The additive 5k/10k ER, 1,001-edit, and 500-render generators are sound: 25 unique items regenerate all 25 pins, the older pins are unchanged, and the new batch cardinalities are exact. The fleet snapshot inherited the same false local P2/P3 completion claim; `8379f65e` corrects it. Re-audit a fleet row when its recorded tip moves, and re-pin only after intentional generator review. |
| `c4f3e3a7` | **CORRECTED** | It correctly adopted the six class names and identified `VOID-NONULL` as dominant, but its regex admitted structural prose/ceilings as `VALID-MECHANISM` and its queue promoted rows before full reading. Sections 9–10 later correct both errors: the strict population is 189 REJECTs, 159 VOID, and the unresolved top five are `L9613`, `L5020`, `L7861`, `L5492`, and `L9250`. Re-run only those rows, only in an assigned window, and only after satisfying each recorded predicate. |
| `90934f71` | **CORRECTED** | The first preflight accepted four prose-shaped justifications and did not enforce executing-ELF provenance on KEEP rows, so it could still admit undecidable verdicts. `ce58ca06` replaced inference with explicit A/A/counted-mechanism/ELF markers, installed the local pre-commit chain and CI check, and added boundary self-tests; `c681aa80` corrected the documentation. Re-open if self-test, staged lint, hook wiring, or the two-evidence-shape contract regresses. |

No commit is retracted wholesale. The two source KEEPs (`16c2bb96` and the CSS cache inside
`1049817b`) remain justified by their own routing, proof, and numbers. The corrections are to
classification, reproducibility claims, and pre-contract timing status.

**Lane-L continuation.** No micro-lever or benchmark was started. The current live harness
self-tests pass with `cv_gate=never`; all 25 corpus pins regenerate exactly; the 1,001-revision and
500-render cardinalities are correct; and the strict ledger preflight passes all nine boundary
self-tests. The measurement work remains explicitly queued in `bd-8f9a` and `bd-ktx5` for a
campaign-assigned quiet window.
