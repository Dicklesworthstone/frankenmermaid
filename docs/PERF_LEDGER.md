# Performance Ledger — frankenmermaid

Campaign `perf-campaign-20260725` §4 splits the record: **KEEP → this file, REJECT →
`docs/NEGATIVE_EVIDENCE.md`.** Rows are never deleted.

Historically this repo recorded kept wins inside `docs/NEGATIVE_EVIDENCE.md` (see its "Kept Wins Also
Recorded Here By Request" section); those rows stay where they are. New KEEP rows land here.

Every kept performance row records: hypothesis · profile-first attribution (samples, % self-time) ·
the ONE lever · byte-identity proof · A/B **and A/A null** with worker id and binary sha · verdict ·
retry predicate · one exact result class:

- `maintenance-self-speedup` means this repository's own code before versus after. It may justify
  landing a maintenance improvement, but it is not campaign output and is never a competitive claim.
- `incumbent-win` means the actual legacy incumbent, **mermaid-js**, ran side-by-side with
  frankenmermaid in the same harness invocation. The row must name and pin that incumbent artifact,
  share one invocation ID, report the measured ratio, and carry the run's A/A null.

The required machine-readable markers are:

```markdown
**Campaign result class:** maintenance-self-speedup
```

or:

```markdown
**Campaign result class:** incumbent-win
**Legacy incumbent arm (same invocation):** name=mermaid-js version=<pin> artifact_sha256=<64 lowercase hex> invocation_id=<id> measured_ratio=<number>x
```

`scripts/ledger_preflight.mjs` enforces these classes for every added or modified kept result.

---

## MAINTENANCE SELF-SPEEDUP (KEEP): drop the redundant membership hash probe — `arch_100x50` −2.92% instructions (2026-07-25)

**Bead:** `bd-1buv.69` (parent `bd-1buv`). **Lane:** cc/STRUCTURAL (`BoldPanther`).
**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Full artifact:** `.benchmarks/bd_1buv_69_membership_hash_probe_WIN.md`.
**Campaign result class:** maintenance-self-speedup

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

  **Executing ELF SHA-256 (self-reported by process):**
  `0c0b7469d44364050f100cd04bcaed404e8264af9ea56fbbb6756cc57cc2bfa8`
  (candidate; full raw record in `.benchmarks/headtohead/run-1049817b-1785035827138.jsonl`).

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

## MAINTENANCE SELF-SPEEDUP RE-CERTIFICATION (KEEP; not the strict queue): five resurrection reruns (2026-07-25)

**Bead:** `bd-1buv.67`. **Lane:** cod / HARNESS+FRONTIER (`CreamGorge`).
**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Full artifact:** `.benchmarks/bd-1buv.67_harness_resurrection.md`.
**Campaign result class:** maintenance-self-speedup

> **Model-integrity correction (2026-07-27):** the five measurements below retain their
> ELF/A/A/parity evidence, but the phrase “ranked top five” came from the superseded mechanical
> two-class screen. They are historical re-certifications, not the strict unresolved queue.
> `docs/LEDGER_RESURRECTION.md` §10.3 and `bd-8f9a` are authoritative.

The mechanical audit found **62 / 251 reject-class entries (24.7%)** whose original verdict was
VOID: 19 VOID-A plus 43 VOID-B. Only 22 / 251 had an A/A control and only 5 / 251 recorded a bench
binary SHA-256. Five entries selected by that historical screen were re-run on current code with an
in-process ELF hash,
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

**Executing ELF SHA-256 (self-reported by process):**
crossing `2439b3cad0ddd002ca7c697aa1d0ce6b21079b6c29038771dfa95705d2bd994c`;
escape `08d4042140f76ef95071f8872a4826b63666e947305fbf27d4fa64f906630f8c`;
Canvas `7d9314ae65055046e7b138fd6c3ea62345a02f444caa9afbc09f6e9c59c4c014`.

**Retry / re-check predicate:** repeat a row only if its production lineage changes or a current
profile puts its target below 3% self-time. A retry remains admissible only with exact parity,
self-reported ELF identity, and the same-invocation A/A median-CI gate. Dense rank was also
re-decided and rejected; that result lives in `docs/NEGATIVE_EVIDENCE.md`.

## MAINTENANCE SELF-SPEEDUP (KEEP): feature-keyed transformed theme-CSS cache — `doc_build_40` +30.61% (2026-07-25)

**Bead:** `bd-1buv.67`. **Lane:** cod / HARNESS+FRONTIER (`CreamGorge`).
**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Full artifact:** `.benchmarks/render_theme_css_minify_memoization_CANDIDATE.md`.
**Campaign result class:** maintenance-self-speedup

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
- **Executing ELF SHA-256 (self-reported by process):**
  `85a81ad5c196a27ed56503ca7756c6a68e4de2fa74ef7a30c4cab8624dbad786`.
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
| `90934f71` | **CORRECTED** | The first preflight accepted four prose-shaped justifications and did not enforce executing-ELF provenance on KEEP rows. `ce58ca06` replaced inference with explicit markers, but the final closeout found that its parser still read only `docs/NEGATIVE_EVIDENCE.md`; therefore it could not enforce KEEP provenance in `docs/PERF_LEDGER.md`. `bd-ckm0` extends linting and boundary tests to both ledgers. `c681aa80` corrected the earlier documentation. Re-open if self-test, staged lint, hook wiring, either ledger path, or the two-evidence-shape contract regresses. |

No commit is retracted wholesale. The two source KEEPs (`16c2bb96` and the CSS cache inside
`1049817b`) remain justified by their own routing, proof, and numbers. The corrections are to
classification, reproducibility claims, and pre-contract timing status.

**Lane-L continuation.** No micro-lever or benchmark was started. The current live harness
self-tests pass with `cv_gate=never`; all 25 corpus pins regenerate exactly; the 1,001-revision and
500-render cardinalities are correct; and the strict ledger preflight passes all thirteen boundary
self-tests. The measurement work remains explicitly queued in `bd-8f9a` and `bd-ktx5` for a
campaign-assigned quiet window.

### Final remediation reconciliation (2026-07-27)

All seven `CORRECTED` verdicts now map to durable fixes on `main`; there are zero `RETRACTED`
verdicts and therefore no retraction dependents to preserve or remove.

| Corrected commit | Landed remediation |
|---|---|
| `bb84aa3b` | `d229bdd7` records `RangeError` as `kind=failed`, `CANNOT`, and no ratio; `b1a11a98` corrects the README claim. |
| `88ade078` | `1049817b` replaces provisional measurements; `ce58ca06` plus §10 replace the two-class screen and queue. |
| `1049817b` | This row, its full artifact, and bead `bd-1buv.67` now label the five measurements as historical re-certifications; §10.3/`bd-8f9a` own the strict queue. |
| `83533872` | `8379f65e` fixes the live P2/P3 path; `b1a11a98` labels its old absolute timings pre-contract and exploratory. |
| `2fef0bf7` | `8379f65e` corrects the inherited local P2/P3 status while preserving the additive corpus and pins. |
| `c4f3e3a7` | `b75cd101`, `ce58ca06`, and `c681aa80` withdraw prose/ceiling mechanism inference and install the strict six-class hand audit. |
| `90934f71` | `ce58ca06` installs explicit evidence markers; `bd-ckm0` closes the missed split-ledger path by linting both REJECT and KEEP ledgers. |

**Gate retry predicate:** the remediation is incomplete if a modified `## KEEP` in
`docs/PERF_LEDGER.md` without the exact process-self-report marker exits zero, if a modified
`### REJECT` in `docs/NEGATIVE_EVIDENCE.md` without A/A or counted work exits zero, or if either
ledger is silently skipped.

**Gate self-check:** before repairing the historical marker spelling, the corrected dual-ledger
tool was run against the repository's own post-`470ca188` ledger delta. It exited 1 and named the
previously invisible KEEP rows at `docs/PERF_LEDGER.md` L14 and L95 (plus the historical
re-certification and one independently inadmissible REJECT). After the exact markers and ranking
correction above, linting the current delta against `origin/main` accepts all three modified KEEP
rows. All thirteen boundary self-tests pass. This is a fail-then-pass check on the real ledger, not
only a synthetic unit fixture.

## CERTIFIED: head-to-head vs mermaid-js 11.15.0 under the median-CI gate (2026-07-27)

**Bead:** `bd-1buv.1`. **Lane:** cc (`BoldPanther`).
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `a23cf867fd18608c0c3d6a75671dc57573847fb4db6724bc87942944a74cbd6a`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=892761e9-1785182202902 measured_ratio=1381x
**A/A null control (same invocation):** per-engine bootstrap median CI; frankenmermaid arm ratio 0.999095 CI [0.995680, 1.000985], mermaid-js arm ratio 1.023555 CI [0.980000, 1.078748]; 15/15 rows cleared `null_ci95_2x_margin`, 0 failures, `cv_gate = never`.

- **Incumbent arm.** Pinned mermaid-js `11.15.0` (bundle SHA-256 verified before injection) driven in
  system Chromium over the DevTools Protocol, in the same invocation as our arm, over byte-identical
  SHA-256-pinned input. `securityLevel` is mermaid's own default `strict`.
- **Dispatch-trap guard.** The page asserts at runtime that `mermaid.version` equals the pin and that
  `render()` is not a zero-arg native/bound stub, so the incumbent cannot be a dispatched stand-in.
- **Arm-asymmetry guard.** The two engines are separate runtimes and cannot be interleaved in one
  measured routine, so `/proc/stat` busy fraction is measured across each phase: 0.1582 (ours, 14 s)
  vs 0.1410 (mermaid, 874 s), ratio **1.122**, inside the 1.25 rule.
- **Result — mermaid-js does not render at all** on five items, `RangeError: Maximum call stack size
  exceeded`: 2,000-node flowchart (6.0 s), 5,000-node flowchart (13.0 s), 5,000-node architecture
  (13.8 s), 2,500-entity schema (60.2 s), 10,000-node architecture (51.4 s). No ratio is stated for
  these; they are excluded from every aggregate.
- **Result — ratios**, 15 items where mermaid completes: median **1,381×** (1,502× min estimator),
  range **362× – 8,571×**. Largest: `er_schema_1000x6` 1.910 ms vs 16,372.4 ms.
- **Evidence:** `.benchmarks/headtohead/cert-v3/`.
- **Retry predicate.** Re-certify when the corpus, the pinned mermaid version, or the render config
  changes.

## SEMANTIC ADMISSION ONLY: original 15 measurable median-CI rows (2026-07-31)

**Bead:** `bd-3ma8`. **Lane:** cod (`LavenderMill`).
**Executing ELF SHA-256 (self-reported by process):**
`c2b0af01dfffab49631d70a2988dfb8fa094f79daa0a14785b5c3683332bc3e2` (7,894,560 bytes).
**A/A null control (same invocation):** incomplete and failing the corrected median clause, so it is
not a performance gate. The scalar Rust dump arms reported: `flowchart_small_10` 0.999353,
CI [0.993812, 1.003382]; `flowchart_medium_100` 1.002253, CI [0.985596, 1.009068];
`flowchart_large_500` 1.000547, CI [0.993913, 1.008991]; `wide_8x16` 1.002951,
CI [0.936794, 1.006095]; `wide_12x24` 1.001079, CI [0.961232, 1.043164];
`wide_16x32` 1.001600, CI [0.992408, 1.037140]; `dense_dag_200` 0.988292,
CI [0.948011, 1.044450]; `cyclic_scc_100` 0.995555, CI [0.937841, 1.015592];
`sequence_20` 1.004043, CI [0.991445, 1.014441]; `class_50` 1.005195,
CI [0.993162, 1.017518]; `state_40` 1.001364, CI [0.962868, 1.015847]; `er_40`
1.002225, CI [0.992689, 1.023017]; `edit_trace_60x20` 1.000050,
CI [0.985323, 1.030017]; `er_schema_1000x6` **1.032506**,
CI [0.998702, 1.262240]; and `doc_build_40` 0.997008, CI [0.892474, 1.057714].
The `er_schema_1000x6` median itself is outside the mandatory `[0.98, 1.02]` interval. The untimed
mermaid-js `--render-once` arm collected no incumbent A/A pairs. This invocation therefore
establishes no current ratio, win, loss, or complete corrected-null timing verdict; CV remains
provenance only.

- **Exact semantic result.** The 15 measurable rows from the original cert-v3 section are exactly
  the 13-row base slice plus the two short flowcharts. One current-ELF invocation admits all
  **15/15** rows and all **74/74** constituent revisions, with zero divergent and zero unverified.
  The five historical mermaid-js crash rows are not part of this equivalence claim: an output that
  the incumbent never produced cannot be compared.
- **Output-equivalence check.** One shared extractor processes both engines. It gates
  incumbent-rendered text containment for every family, authored node-ID sets, rendered-path
  topology cross-engine and against input-derived truth where claimed, and class relationship
  marker kind plus owning end. Referenced marker geometry, fill, and inheritance-triangle direction
  are checked. Unknown or undecidable required invariants do not pass. This is neither SVG byte
  equality nor a rasterized perceptual diff. The oracle self-test passes 40/40 cases, including 16
  mutation controls and 4 negative controls.
- **Artifact and exact linkage.** Artifact
  `.benchmarks/headtohead/cert-v3-requalification-v1/equivalence-5bad0559-1785490669339.json`
  has SHA-256 `d2657bd5d70e5810257998d341dd01429922cd2fac7e68a7a70d1565447ef183`.
  Every engine dumped the expected revision count, and each dump hash below exactly equals that
  engine's self-reported output SHA-256:

  | row | revisions | input SHA-256 | frankenmermaid dump SHA-256 | mermaid-js dump SHA-256 |
  |---|---:|---|---|---|
  | `flowchart_small_10` | 1 | `b5402490faa78c6a7c71554296d03b46016ae1156d7cd38d258b280363b6900a` | `2ae42a001b97fb01146a0930f93d67c7e26f8ecb8221d6faa827b2b92d51bdd6` | `2f6a82745f61b49403f87ad3e39e5592017b02f86d52fa85c197efdd40ded9fd` |
  | `flowchart_medium_100` | 1 | `74bd26f73724626255642c427d36844d8a75f7bdf7fd47a69f8541a3ec9aea22` | `6e4a062abb16876763a31971b90b10341c3a03b1cb819d824b0aa26871c65e8b` | `90f846cd2c196f749aecd3784c12fa732089bb7726294dc55967f4032749f1ec` |
  | `flowchart_large_500` | 1 | `7012902b9fdaa3ff2d7a2d0c327eaaea543b347b51155521b86daf7aacd9ec83` | `408ecdccfba04fb4aa84526b565e0397383bb4c0dca9184e33e01b7ef2dd2d21` | `4694da1118ad9b4919530f412507fb8fde7ead391e2b7a120c883ecd1502af78` |
  | `wide_8x16` | 1 | `61f1747cefcc13449ebf5e9c08b1f039dbf9b218f27b34e19d640076bf0004aa` | `7b1c6a07e46282794717c1d90229c0e44eb0cf5739d2693947eaddf7295990a2` | `96850b91f79b24da943a55667d75a44cde06165368e7a6355cd9154839a42d51` |
  | `wide_12x24` | 1 | `e05519607415f5370b530fa540bc9fe4374f9a14c28bd444a1ecb91aa2219959` | `ea73f1d73e840f6d01d32c54b841e79e3f7c60c6fe6777b9e872e1581b31c5fd` | `6300b137acb725117cdb5e9f800a5e428dca354c24d30b5452ef98c7618aae63` |
  | `wide_16x32` | 1 | `bcd6776815763d34d14d46cc6920a692dd70842c6ec83207d31e9a4b4c11f08b` | `30d79510dbc4590b6346742560acc6d2af20b2439f166adc58a93d2529681fce` | `46752d9ce2dd9452b7a67c198828c1cf24a5038bf297ddf2a6373ad5552ce1d5` |
  | `dense_dag_200` | 1 | `a32522f3b7080f48621a9e2cc226920f5dd59eb4239fdd1f480187df0063b3e5` | `e8e4d888acfce246073d5aeae41d22e2ce5d9fb18950fd368dc383c759016ee2` | `31ef61c6ff34d23ae15b5074afbb25c1a094447a0fee5f62fdcb91b07bd2eb43` |
  | `cyclic_scc_100` | 1 | `dbc553f665d05c10084949154ba4f24f58a4a363b5ffcb419bedfcc5daf23ab6` | `73df0305e21b1b9ef11e0ffbaa1c64c7cde16583464bbb4c4681d83ba8ada1d4` | `d29393e78702fddbee0a0d79dfd8d1b53711d797d24c3b2a8e4cf1a1b321715e` |
  | `sequence_20` | 1 | `31c0dd6bc24b571c01c80d6c24d9e6e179f035cd82d226c71d952e8e52498db0` | `f3bfcb1128d2c7a729e81f5227874335d6d541faac51495e08b54c701915bf55` | `bc6e2e0c722e8854d5ef655b24fc9d16af7766a41c5f46fc01ae3f387a64e564` |
  | `class_50` | 1 | `d1d7ef8c8e7c8d1dab2da8fbd56565dc97148e8fb1651d23fe43140e8c4ef831` | `e97ea9c8683e59b151f7e916b22002d117ef9d40a03fce1a6274688bd9a8c1cd` | `13866152652c1fe4dd74c57f27b5cd2f7f38a1c39fe3fb633228698195f6c64a` |
  | `state_40` | 1 | `08a5c38ed30e5aaeddf02f59839e6e36ffa5e91960e3fb966d68281e8937eec3` | `56b696a74ff934d22be792e2b9b7836595a59b140891e923ebab24c64f787286` | `453e35f37a93d4dde709d7699246c3f7a9575d602f4e77451771ee504a0988f9` |
  | `er_40` | 1 | `91764d7d6dd294a65f25e3de7ef9f619ff6b1592a12a0dd0b92de816e2b756f2` | `354f13007b65bfff6705e1006114e035d29fd81d0d67a261a95c1b0a8820c5b6` | `ea591a75a76ae29b8f6ce95513b28620404180bcf95ea49b8106e9a992fb6b9c` |
  | `edit_trace_60x20` | 21 | `068270a4e7d6ae7e7ddc8ac86be6d24ba156eb233e64355eb07db2bc26a258e0` | `054f82f5f290d0cd18519281843479ad7cd86121f8db064d11d10a87aad4e2a0` | `5b968f91d34e2b8929968f6f2ed07d2ec92a98b5919fe4f38b4adcb432b3a9d5` |
  | `er_schema_1000x6` | 1 | `252c8370ef3053801bad7d0ac6f082b3c352622642a668a13b027ed3eb27318f` | `f4bfd5cee24fb788cc367a87b1aec0d45c37b79d9f8e27a047ff1e6860e89d27` | `f6f9efecd8d56589b93c3758205cd96bc607dcbef123627032e6e5d247b6406e` |
  | `doc_build_40` | 40 | `8badedbf69bc204d952af1ba780c07569b7eb1091ff5d0fdd400dd2e3f6b59d7` | `56b6b7e0d47647ba847d390f7afe0785ede3bb86a8d33ea46e3d418e5c431c24` | `d5e75c0b31b760a0ef90122162e970c460ab1bb4d7368a61416880f73d0eab0e` |

- **Incumbent, observed threads, and host.** The live comparator is mermaid-js `11.15.0`, bundle
  SHA-256 `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`,
  through `/usr/bin/chromium-browser` (`Chrome/150.0.7871.128`). Every row requested and actually
  observed **1** frankenmermaid scalar worker and **1** mermaid-js browser main execution thread.
  Host `thinkstation1` is an AMD Ryzen Threadripper PRO 5975WX with 64 logical CPUs, kernel
  `6.17.0-35-generic`, complete `amd-pstate-epp` provenance (`powersave` governor, `performance`
  EPP, boost enabled), and complete x86-64 ISA provenance (AVX2/FMA/BMI2/VAES present; AVX-512
  absent).
- **Strict build receipt.** Worker `ovh-a` built this executable through strict `rch exec` from base
  `2bb114ff` with `--clean-overlay` and only `crates/fm-layout/src/lib.rs`; no co-tenant edit entered
  the binary, no local fallback occurred, and no task-specific Cargo target directory was created.
- **Disposition / retry predicate.** Semantic admission is current, but this does not resurrect the
  historical 1,381x aggregate or any constituent ratio. Take a fresh exclusive central `trj` claim
  only after earlier bookings and host-wide external work clear, then time these exact linked rows
  side-by-side against the live pinned incumbent with the same self-reporting ELF, actual observed
  threads, complete governor/ISA/topology provenance, and a corrected same-invocation A/A gate in
  which every null median—including `er_schema_1000x6`—is inside `[0.98, 1.02]`. Reopen semantic
  admission only if an input pin, incumbent bundle/configuration, executing ELF, or equivalence
  contract changes.

## CERTIFIED INCUMBENT WIN: 13-row bracketed base slice vs mermaid-js 11.15.0 (2026-07-28)

**Bead:** `bd-ktx5`. **Lane:** cod / HARNESS+FRONTIER (`YellowSwan`).
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `a23cf867fd18608c0c3d6a75671dc57573847fb4db6724bc87942944a74cbd6a`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=7b3caf62-1785228548577 measured_ratio=1881.776744x
**A/A null control (same invocation):** 13/13 accepted ratio rows carried sufficient mermaid-js, Rust-before, and Rust-after bootstrap median CIs; 0 median-CI failures, 0 bracket failures among the accepted rows, `cv_gate=never`.

- **Result — ratios.** The 13 accepted rows span **381× – 9,486×**, median **1,882×**
  (1,892× by the min estimator). Each ratio uses the slower byte-identical Rust-before/Rust-after
  observation as its denominator.
- **Result — cannot render.** The same invocation reproduced five nonnumeric mermaid-js
  `RangeError: Maximum call stack size exceeded` results: 2,000-node flowchart (6.5 s),
  5,000-node flowchart (14.4 s), 5,000-node architecture diagram (16.3 s), 10,000-node
  architecture diagram (57.8 s), and 2,500-entity schema (69.3 s). No ratio is stated.
- **Additional cannot-render rows.** Invocation `7b3caf62-1785229102747` reproduced the same `RangeError`
  on the 5,000-entity schema after 201.6 s. The identical Rust ELF measured 15.126684 ms before
  Chromium and 14.954413 ms after; 1.011520× drift cleared its 1.023112× A/A floor. No ratio is
  stated. Invocation `1a528e12-1785280762909` extended the ER schema to 10,000 entities:
  mermaid-js raised the same `RangeError` after 625.390 s, while the slower bracketed Rust
  observation was 33.032404 ms. Rust drift was 1.000465× inside its 1.091987× A/A floor. This
  seventh row is also `CANNOT`, with no ratio or lower bound.
- **Excluded rows.** `flowchart_small_10` and `flowchart_medium_100` cleared their cross-runtime
  median-CI gates but failed their Rust pre/post bracket at 1.033828× and 1.036809× drift. They are
  absent from this result and were retried separately under the predicate recorded below.
- **Evidence:** `.benchmarks/headtohead/cert-v8/`, `.benchmarks/headtohead/cert-v10/`, and
  `.benchmarks/headtohead/cert-er10k-v1/`.
- **Retry predicate.** Re-run a row if its input, incumbent bundle, executing ELF, render
  configuration, or bracket contract changes. A bracket failure may be retried only in a quiet
  invocation with the same pinned artifacts; never widen its floor post hoc.

## SEMANTIC REQUALIFICATION ONLY: 13-row bracketed base slice under the current SVG oracle (2026-07-31)

**Bead:** `bd-4b6s`. **Lane:** cod (`LavenderMill`).
**Executing ELF SHA-256 (self-reported by process):**
`c2b0af01dfffab49631d70a2988dfb8fa094f79daa0a14785b5c3683332bc3e2` (7,894,560 bytes).
**A/A null control (same invocation):** incomplete by construction and therefore not a performance
gate. Every scalar Rust dump-arm median itself is inside the corrected `[0.98, 1.02]` clause:
`flowchart_large_500` 1.000850, CI [0.998274, 1.008237]; `wide_8x16` 0.998249,
CI [0.989442, 1.012851]; `wide_12x24` 1.005128, CI [0.992356, 1.007759];
`wide_16x32` 0.995935, CI [0.983775, 1.016681]; `dense_dag_200` 1.005775,
CI [0.998871, 1.007343]; `cyclic_scc_100` 1.005275, CI [0.990499, 1.036888];
`sequence_20` 0.993613, CI [0.964156, 1.029004]; `class_50` 1.002303,
CI [0.993262, 1.022934]; `state_40` 1.014488, CI [0.931720, 1.117178]; `er_40`
1.000100, CI [0.989632, 1.009940]; `edit_trace_60x20` 1.003318,
CI [0.993903, 1.124430]; `er_schema_1000x6` 1.002529, CI [0.998942, 1.013132];
and `doc_build_40` 0.983828, CI [0.937485, 1.057207]. The untimed mermaid-js
`--render-once` arm collected no incumbent A/A pairs. This invocation therefore establishes no
current ratio, win, loss, or complete corrected-null timing verdict; a future timed invocation must
also pass the incumbent null, bootstrap-CI, and 2x-null-margin clauses. CV remains provenance only.

- **Exact semantic result.** The current oracle admits all **13/13** named rows and all **72/72**
  constituent revisions, with zero divergent and zero unverified. In particular,
  `edit_trace_60x20` is **21/21**, and the mixed-family `doc_build_40` job is **40/40**.
- **Recovered semantic work.** Revisions 18-20 of `edit_trace_60x20` contain both `N2-->N17` and
  the reciprocal `N17-->N2`. `bundle_parallel_edges` claimed to group directed
  `(source, target, arrow)` tuples but implemented an unordered min/max endpoint key, so it
  absorbed the reciprocal edge as though it were a same-direction duplicate. The key is now
  directed; a regression test keeps two `0->1` duplicates bundled while preserving the independent
  `1->0` edge. This was renderer work loss, not extractor ambiguity.
- **Output-equivalence check.** One shared extractor processes both engines; it never uses a
  favorable per-engine extractor pair. It gates incumbent-rendered text containment for every
  family, authored node-ID sets, rendered-path topology cross-engine and against input-derived
  truth for the families where that invariant is claimed, and class relationship marker kind plus
  owning end. Referenced diamond geometry/fill and inheritance-triangle fill/direction are checked.
  Unknown or undecidable required invariants do not pass. This is neither SVG byte equality nor a
  rasterized perceptual diff. `svg_equivalence.mjs --self-test` passes all 40 cases, including 16
  mutation controls and 4 negative controls.
- **Audit trail.** The first current-oracle artifact,
  `equivalence-2bb114ff-1785488169040.json` (SHA-256
  `d282192ea30ec39b4c219b6ade5cfa18ab54ffe409732f9c3bbc94da3c9ba064`),
  exposed an oracle defect: reserved renderer pseudo nodes `__state_start` and `__state_end` were
  mistaken for authored nodes after exact `data-id` extraction landed. Tight recognition of those
  reserved IDs, with a near-miss negative control, produced
  `equivalence-2bb114ff-1785488341521.json` (SHA-256
  `80c5ad7eae5a41a2b7cdb12e8a55c1c82c7efe1f7ea00cdf5fa89dbf9bf431a1`):
  12/13 rows passed, while the three reciprocal-edge losses above remained real. The final passing
  artifact is
  `.benchmarks/headtohead/cert-v8-requalification-v1/equivalence-2bb114ff-1785489467048.json`
  (SHA-256 `6a6165bc87f232368c72e5413cd4cb9f0c06f2293eda7eccd49eb95ddee2020b`).
- **Exact input/output linkage.** Every engine dumped the expected revision count. Each dump hash
  below exactly equals the output SHA-256 self-reported by that engine:

  | row | revisions | input SHA-256 | frankenmermaid dump SHA-256 | mermaid-js dump SHA-256 |
  |---|---:|---|---|---|
  | `flowchart_large_500` | 1 | `7012902b9fdaa3ff2d7a2d0c327eaaea543b347b51155521b86daf7aacd9ec83` | `408ecdccfba04fb4aa84526b565e0397383bb4c0dca9184e33e01b7ef2dd2d21` | `4694da1118ad9b4919530f412507fb8fde7ead391e2b7a120c883ecd1502af78` |
  | `wide_8x16` | 1 | `61f1747cefcc13449ebf5e9c08b1f039dbf9b218f27b34e19d640076bf0004aa` | `7b1c6a07e46282794717c1d90229c0e44eb0cf5739d2693947eaddf7295990a2` | `96850b91f79b24da943a55667d75a44cde06165368e7a6355cd9154839a42d51` |
  | `wide_12x24` | 1 | `e05519607415f5370b530fa540bc9fe4374f9a14c28bd444a1ecb91aa2219959` | `ea73f1d73e840f6d01d32c54b841e79e3f7c60c6fe6777b9e872e1581b31c5fd` | `6300b137acb725117cdb5e9f800a5e428dca354c24d30b5452ef98c7618aae63` |
  | `wide_16x32` | 1 | `bcd6776815763d34d14d46cc6920a692dd70842c6ec83207d31e9a4b4c11f08b` | `30d79510dbc4590b6346742560acc6d2af20b2439f166adc58a93d2529681fce` | `46752d9ce2dd9452b7a67c198828c1cf24a5038bf297ddf2a6373ad5552ce1d5` |
  | `dense_dag_200` | 1 | `a32522f3b7080f48621a9e2cc226920f5dd59eb4239fdd1f480187df0063b3e5` | `e8e4d888acfce246073d5aeae41d22e2ce5d9fb18950fd368dc383c759016ee2` | `31ef61c6ff34d23ae15b5074afbb25c1a094447a0fee5f62fdcb91b07bd2eb43` |
  | `cyclic_scc_100` | 1 | `dbc553f665d05c10084949154ba4f24f58a4a363b5ffcb419bedfcc5daf23ab6` | `73df0305e21b1b9ef11e0ffbaa1c64c7cde16583464bbb4c4681d83ba8ada1d4` | `d29393e78702fddbee0a0d79dfd8d1b53711d797d24c3b2a8e4cf1a1b321715e` |
  | `sequence_20` | 1 | `31c0dd6bc24b571c01c80d6c24d9e6e179f035cd82d226c71d952e8e52498db0` | `f3bfcb1128d2c7a729e81f5227874335d6d541faac51495e08b54c701915bf55` | `bc6e2e0c722e8854d5ef655b24fc9d16af7766a41c5f46fc01ae3f387a64e564` |
  | `class_50` | 1 | `d1d7ef8c8e7c8d1dab2da8fbd56565dc97148e8fb1651d23fe43140e8c4ef831` | `e97ea9c8683e59b151f7e916b22002d117ef9d40a03fce1a6274688bd9a8c1cd` | `7fc2f5d1a3533f6c06ffa4b2690383bff9889d82483d4959e4392318b863286d` |
  | `state_40` | 1 | `08a5c38ed30e5aaeddf02f59839e6e36ffa5e91960e3fb966d68281e8937eec3` | `56b696a74ff934d22be792e2b9b7836595a59b140891e923ebab24c64f787286` | `453e35f37a93d4dde709d7699246c3f7a9575d602f4e77451771ee504a0988f9` |
  | `er_40` | 1 | `91764d7d6dd294a65f25e3de7ef9f619ff6b1592a12a0dd0b92de816e2b756f2` | `354f13007b65bfff6705e1006114e035d29fd81d0d67a261a95c1b0a8820c5b6` | `ea591a75a76ae29b8f6ce95513b28620404180bcf95ea49b8106e9a992fb6b9c` |
  | `edit_trace_60x20` | 21 | `068270a4e7d6ae7e7ddc8ac86be6d24ba156eb233e64355eb07db2bc26a258e0` | `054f82f5f290d0cd18519281843479ad7cd86121f8db064d11d10a87aad4e2a0` | `5b968f91d34e2b8929968f6f2ed07d2ec92a98b5919fe4f38b4adcb432b3a9d5` |
  | `er_schema_1000x6` | 1 | `252c8370ef3053801bad7d0ac6f082b3c352622642a668a13b027ed3eb27318f` | `f4bfd5cee24fb788cc367a87b1aec0d45c37b79d9f8e27a047ff1e6860e89d27` | `1d49640503caacbce44c48549afbbbd98b445fcff873b05b84137ebd20b5538a` |
  | `doc_build_40` | 40 | `8badedbf69bc204d952af1ba780c07569b7eb1091ff5d0fdd400dd2e3f6b59d7` | `56b6b7e0d47647ba847d390f7afe0785ede3bb86a8d33ea46e3d418e5c431c24` | `9fc72662782b2f796c0a49593ee26ff72d8a7e246a17803f3e6add27b0e23a75` |

- **Incumbent, observed threads, and host.** The live comparator is mermaid-js `11.15.0`, bundle
  SHA-256 `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`,
  through `/usr/bin/chromium-browser` (`Chrome/150.0.7871.128`). Every row requested and actually
  observed **1** frankenmermaid scalar worker and **1** mermaid-js browser main execution thread.
  Host `thinkstation1` is an AMD Ryzen Threadripper PRO 5975WX with 64 logical CPUs, kernel
  `6.17.0-35-generic`, complete `amd-pstate-epp` provenance (`powersave` governor, `performance`
  EPP, boost enabled), and complete x86-64 ISA provenance (AVX2/FMA/BMI2/VAES present; AVX-512
  absent).
- **Strict build receipt.** Worker `ovh-a` built the executable with
  `RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch --no-self-healing exec --base 2bb114ff
  --clean-overlay --overlay-path crates/fm-layout/src/lib.rs -- cargo build -j2 --profile release
  -p frankenmermaid-cli --example headtohead`. No co-tenant edit entered the binary, no local
  fallback occurred, and no task-specific Cargo target directory was created.
- **Disposition / retry predicate.** Semantic admission is current. The historical 13-row ratio is
  not current for this ELF/oracle because this invocation was deliberately untimed. Take a fresh
  exclusive central `trj` claim only after earlier bookings and host-wide external work clear, then
  measure these exact linked rows side-by-side against the live pinned incumbent with the same
  self-reporting ELF, actual observed threads, complete governor/ISA/topology provenance, and the
  corrected same-invocation A/A gate above. Reopen semantic admission only if an input pin,
  incumbent bundle/configuration, executing ELF, or equivalence contract changes.

## CERTIFIED INCUMBENT WIN: two short-row quiet retries vs mermaid-js 11.15.0 (2026-07-28)

**Bead:** `bd-ktx5`. **Lane:** cod / HARNESS+FRONTIER (`YellowSwan`).
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `a23cf867fd18608c0c3d6a75671dc57573847fb4db6724bc87942944a74cbd6a`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=7b3caf62-1785228590266 measured_ratio=846.521899x
**A/A null control (same invocation):** both rows carried sufficient mermaid-js, Rust-before, and Rust-after bootstrap median CIs; 0 median-CI failures, 0 bracket failures, `cv_gate=never`.

- **Result.** `flowchart_small_10` measured **846.521899×** (29.887 us versus 25.300 ms);
  `flowchart_medium_100` measured **1,155.364273×** (173.798 us versus 200.800 ms).
- **Same-ELF phase bracket.** Pre/post drift was 1.005788× and 1.007437× respectively, both inside
  the 1.01× Rust A/A floor. Each pair produced byte-identical default and lean SVG output.
- **Evidence:** `.benchmarks/headtohead/cert-v9/`.
- **Retry predicate.** Re-certify if either input, incumbent bundle, executing ELF, render
  configuration, or bracket contract changes.

## CERTIFIED INCUMBENT WIN: 201-revision live-edit trace vs mermaid-js 11.15.0 (2026-07-28)

**Bead:** `bd-ktx5`. **Lane:** cod / HARNESS+FRONTIER (`YellowSwan`).
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `a23cf867fd18608c0c3d6a75671dc57573847fb4db6724bc87942944a74cbd6a`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=7b3caf62-1785227302372 measured_ratio=5885.002591x
**A/A null control (same invocation):** mermaid-js median 0.987819, 95% CI [0.942338, 1.046369], `n=9`; Rust-before median 1.001112, 95% CI [0.992119, 1.013409], `n=10`; Rust-after median 1.001836, 95% CI [0.997456, 1.012518], `n=10`; largest radius 0.057662, decision floor 1.115325×, `cv_gate=never`.

- **Incumbent arm.** Two real mermaid-js samples and nine A/A pairs each rendered the same full
  201-revision trace. Every mermaid-js arm ran in a fresh browser, so no DOM, page, or renderer state
  crossed sample boundaries. Target crash, page crash, WebSocket close, and deadline expiry all fail
  closed.
- **Same-ELF phase bracket.** The identical 7,778,704-byte Rust ELF self-reported the same SHA-256
  before and after Chromium. Both runs produced identical default and lean SVG SHA-256 values.
  Rust-before measured 44.723604 ms; Rust-after measured 44.952665 ms. Drift was 1.005122×, inside
  the 1.026818× Rust A/A floor. The slower **44.952665 ms** observation is the denominator.
- **Result.** mermaid-js measured **264,546.550 ms** per complete trace versus frankenmermaid
  **44.952665 ms**, or **5,885.002591×**. Per revision: **1,316.152 ms** versus **0.223645 ms**.
  The ratio clears the 1.115325× median-CI floor. Mermaid-js CV was 5.07%; this is recorded as
  provenance and does not gate the result.
- **Corpus aggregate.** Combining the accepted rows in `cert-v8`, `cert-v9`, this invocation, and
  `cert-ci500-v1`, the 17 measurable rows have median **1,493×** (1,483× min estimator) and range
  **381× – 9,486×**. The seven `RangeError` workloads remain nonnumeric `CANNOT` results outside
  every ratio aggregate.
- **Evidence:** `.benchmarks/headtohead/cert-edit-v7/`; unsuccessful certification attempts and
  their concrete retry predicates remain in `docs/NEGATIVE_EVIDENCE.md`.
- **Retry predicate.** Re-certify if the trace hash, pinned mermaid-js bundle, render configuration,
  executing Rust ELF, fresh-browser isolation, or Rust-bracket contract changes.

## CERTIFIED INCUMBENT WIN: 500-diagram CI render vs mermaid-js 11.15.0 (2026-07-28)

**Bead:** `bd-ktx5`. **Lane:** cod / HARNESS+FRONTIER (`YellowSwan`).
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `a23cf867fd18608c0c3d6a75671dc57573847fb4db6724bc87942944a74cbd6a`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=68a04737-1785279364454 measured_ratio=923.056028x
**A/A null control (same invocation):** mermaid-js median 1.007312, 95% CI [0.986341, 1.044076], `n=10`; Rust-before median 0.997020, 95% CI [0.994633, 1.001085], `n=20`; Rust-after median 0.990637, 95% CI [0.986065, 1.006671], `n=20`; largest radius 0.044076, decision floor 1.088153×, `cv_gate=never`.

- **Whole job.** One sample renders 500 diagrams across five syntax families in a single batch.
  Mermaid-js measured **18,567.800 ms** per job versus frankenmermaid **20.115572 ms**, or
  **923.056028×**. Per diagram: **37.135600 ms** versus **0.040231 ms**.
- **Same-ELF phase bracket.** The identical 7,778,704-byte Rust ELF self-reported the same SHA-256
  before and after Chromium and produced byte-identical default and lean SVG output. Rust-before
  measured 20.115572 ms; Rust-after measured 19.929141 ms. Drift was 1.009355×, inside the
  1.027871× Rust A/A floor, so the slower before observation is the denominator.
- **Gate.** The 923.056028× claim clears the 1.088153× median-CI floor. Mermaid-js CV 4.53% and
  phase-load asymmetry 1.42× are provenance only and do not gate the result.
- **Evidence:** `.benchmarks/headtohead/cert-ci500-v1/`.
- **Retry predicate.** Re-certify if the 500-diagram input hash, diagram mix, pinned mermaid-js
  bundle, render configuration, executing Rust ELF, fresh-browser isolation, or bracket contract
  changes.

## CERTIFIED INCUMBENT RESULTS: realistic whole jobs vs mermaid-js 11.15.0 (2026-07-28)

**Bead:** `bd-c0bn`. **Lane:** cod / HARNESS+FRONTIER (`YellowSwan`).
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `a23cf867fd18608c0c3d6a75671dc57573847fb4db6724bc87942944a74cbd6a`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=a9373031-1785282503815 measured_ratio=434.107779x; the other numeric invocation IDs are `a9373031-1785282622219` and `a9373031-1785282801275`.
**A/A null control (same invocation):** every numeric row carried sufficient interleaved
per-engine A/A samples and bootstrap median 95% CIs; all four cross-runtime median-CI gates and all
four same-ELF Rust brackets passed, with `cv_gate=never`.

- **Whole-job boundary.** One sample consumes every source string in the named job and includes
  parse, layout, SVG rendering, and SVG serialization. Deterministic corpus construction and the
  caller's final file copy are outside both timers. Mermaid runs at its default
  `securityLevel=strict`; the bundle and every joined input are SHA-256-pinned.
- **Realistic distributions.** Documentation jobs are flowchart-dominated with right-skewed
  4–60-node diagrams and escaping/non-ASCII labels. The typing job is 60 successive label
  keystrokes on one 40-node graph. The catalog contains 25 schemas with 8–75 entities and
  hub-skewed relationships. Architecture maps use uneven domains and hub-skewed service
  dependencies.

| User job | Size | Rust / mermaid-js job median | Result | Median-CI floor |
|---|---|---:|---:|---:|
| Documentation-site render | 50 diagrams; 940 nodes / 902 edges total | 5.887478 ms / 2,555.800 ms | **434.107779×** | 1.062796× |
| Documentation-site render | 200 diagrams; 3,164 nodes / 3,176 edges total | 19.013274 ms / 10,160.100 ms | **534.368778×** | 1.038818× |
| Live typing preview | 60 keystrokes; 40 nodes / 39 edges per revision | 4.412062 ms / 5,264.250 ms | **1,193.149598×** | 1.082235× |
| Database-catalog publish | 25 schemas; 662 entities / 637 relationships total | 15.856699 ms / 6,546.050 ms | **412.825519×** | 1.315279× |

| Workload | Rust-before A/A median CI | Rust-after A/A median CI | mermaid-js A/A median CI | Rust bracket |
|---|---:|---:|---:|---:|
| `docs_site_50` | [0.991972, 1.006976], n=30 | [0.996270, 1.003342], n=30 | [0.968602, 1.009550], n=9 | 1.000767×, pass |
| `docs_site_200` | [0.995423, 1.019409], n=20 | [0.995197, 1.005408], n=20 | [0.985262, 1.008090], n=10 | 1.011208×, pass |
| `typing_trace_60` | [1.009642, 1.041117], n=20 | [0.990974, 1.007101], n=20 | [0.963473, 1.032673], n=9 | 1.020432×, pass |
| `schema_catalog_25` | [0.989758, 1.003641], n=20 | [0.999238, 1.013150], n=20 | [0.842360, 1.047728], n=9 | 1.001622×, pass |

- **Architecture review is nonnumeric.** Pinned mermaid-js's own `mermaid.parse()` accepted both
  realistic monorepo maps. `mermaid.render()` then raised `TypeError: Cannot set properties of
  undefined (setting 'order')` after 0.496 s at 120 services / 185 dependencies and 1.578 s at
  300 services / 464 dependencies. Both records are `kind=failed`, `CANNOT`, with
  `speedup_lower_bound=null`; no multiplier is stated. The 300-service Rust bracket passed.
  The 120-service absolute timing is deliberately unclaimed because its parse-proving invocation
  failed the Rust phase bracket; the incumbent failure itself is independent of Rust timing.
- **Current aggregate.** The 21 numeric certified rows now have median **1,193.149598×**
  (**1,206.349853×** by the min estimator) and range **380.754586×–9,486.345682×**. Seven
  `RangeError` rows and these two parse-accepted `TypeError` rows remain nonnumeric outside the
  aggregate.
- **Evidence.** `.benchmarks/headtohead/realistic-docs-v1/`,
  `.benchmarks/headtohead/realistic-typing-v1/`,
  `.benchmarks/headtohead/realistic-schema-v1/`, and
  `.benchmarks/headtohead/realistic-arch-v3/`; superseded/noisy architecture attempts and their
  predicates are retained in `docs/NEGATIVE_EVIDENCE.md`.
- **Retry / re-check predicate.** Re-certify a numeric row if its generator/pin, mermaid bundle,
  render configuration, executing Rust ELF, timing boundary, or bracket contract changes. Re-open
  the architecture comparator only if mermaid-js, the pinned input, or its layout configuration
  changes enough for `mermaid.render()` to return a valid SVG; until then it has no ratio to score.

## CERTIFIED INCUMBENT WIN: 500-diagram CI caller-thread sweep vs mermaid-js 11.15.0 (2026-07-29)

**Bead:** `bd-1buv.70`. **Lane:** cod / HARNESS+FRONTIER (`YellowSwan`).
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `600cd6b79113f01de7526df5a029b7ce5d57d4f06fb1d3772412fb29097bdcf7`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=ffeab05f-1785311982943 measured_ratio=16321.565740x
**A/A null control (same invocation):** every Rust-before and Rust-after width carried `n=20`
interleaved A/A ratios; the mermaid-js arm carried `n=10` isolated A/A pairs with median
1.000748 and 95% CI [0.986024, 1.014068]. All controls were sufficient, all seven cross-runtime
median-CI gates passed, all seven same-ELF brackets passed, and `cv_gate=never`.

- **Whole-job boundary.** One sample parses, lays out, renders, and serializes 500 diagrams across
  five syntax families: 5,690 nodes, 5,890 edges, and 201,534 joined input bytes. The input SHA-256
  is `65b8f69a7b2ee114cfb2fb49557b34cbc7e2c15f1414a81b7d94215a46de432f`.
  Deterministic corpus construction and the caller's final file copy are outside both timers.
- **Execution model.** The scalar width uses no pool. Widths 2–64 reuse one persistent Rayon pool
  across warmup, A/A, A/B, and measured rounds. The pinned incumbent uses mermaid-js's
  single-page main-thread API. The driver measures Rust widths ascending before Chromium and
  descending after it, then selects the slower bracket observation.

| Caller threads | Rust whole-job median | Scaling vs 1t | Parallel efficiency | mermaid-js / Rust | Rust bracket drift / floor | Batch / integrated p50 |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 21.441246 ms | 1.000000× | 100.00% | **862.356600×** | 1.054453× / 1.152592× | 5 / 107.206230 ms |
| 2 | 11.658705 ms | 1.839076× | 91.95% | **1,585.939433×** | 1.056696× / 1.197060× | 9 / 104.928345 ms |
| 4 | 6.047344 ms | 3.545564× | 88.64% | **3,057.540633×** | 1.096446× / 1.114789× | 16 / 96.757504 ms |
| 8 | 3.147166 ms | 6.812874× | 85.16% | **5,875.127019×** | 1.007615× / 1.087703× | 29 / 91.267814 ms |
| 16 | 1.913105 ms | 11.207564× | 70.05% | **9,664.916458×** | 1.063808× / 1.070768× | 46 / 88.002830 ms |
| 32 | 1.232407 ms | 17.397861× | 54.37% | **15,003.160482×** | 1.056765× / 1.108679× | 72 / 88.733304 ms |
| 64 | 1.132857 ms | 18.926701× | 29.57% | **16,321.565740×** | 1.015281× / 1.110760× | 81 / 91.761417 ms |

- **Incumbent measurement.** Mermaid-js measured 18,490.000 ms for the same 500-diagram job,
  or 27.041644 diagrams/s. Its A/A CV was 1.63% and MAD was 1.03%; both are provenance only.
- **Identity proof.** Every width in both Rust brackets produced input SHA-256
  `65b8f69a…de432f`, default SVG SHA-256 `28e04524…51163`, and lean SVG SHA-256
  `417f0942…90d3a`. The driver also required the same self-reported 7,813,328-byte ELF in all
  fourteen Rust processes.
- **Sampling floor.** Calibration targeted 75 ms and the driver failed closed below a measured
  50 ms `batch × per-job p50`; accepted integrated medians were 88.003–107.206 ms.
- **Portability boundary.** This win is caller concurrency, not x86 SIMD. The harness contains no
  ISA-specific intrinsic and keeps the scalar arm as the byte-identity reference. These numbers
  apply to the named Threadripper; Apple M4/M5 requires its own 1/2/4/8/... hardware sweep.
- **Aggregate treatment.** These seven rows repeat one workload at different caller widths. The
  21-item corpus aggregate includes the scalar row once and is not inflated with six correlated
  copies.
- **Evidence:** `.benchmarks/headtohead/ci-thread-sweep-v3/`.
- **Retry / re-check predicate.** Re-certify if the input hash or diagram mix, pinned mermaid-js
  bundle/configuration or API execution model, executing Rust ELF or compiler profile, persistent
  pool semantics, 50 ms sample floor, median-CI/bracket contract, or host core topology changes.
  Do not quote this Threadripper scaling curve for Apple Silicon without a native M4/M5 sweep.

## CANNOT: exact-output gate blocks the realistic 2k/5k CI sweeps (2026-07-29)

**Bead:** `bd-l7d2`. **Lane:** cod (`BlackThrush`).

- **Decision.** No `ci_docs_2000` or `ci_docs_5000` timing was run after the output gate exposed
  unequal semantic work. Frankenmermaid held no active `trj` claim and did not access the host for
  this decision; queue withdrawal is Agent Mail message `6501` in `trj-booking`.
- **Counted mechanism:** the pinned `ci_docs_2000` job contains 190 class diagrams with 3,688 field
  rows and 1,501 method rows; `ci_docs_5000` contains 482 class diagrams with 9,280 field rows and
  3,717 method rows. The current frankenmermaid class path omits member content that the incumbent
  renders, so the target jobs would execute 5,189 and 12,997 fewer rendered member rows
  respectively. Equal revision counts and input hashes do not make those outputs equivalent.
- **Fail-closed witness.** Artifact
  `.benchmarks/headtohead/equivalence/equivalence-6bad5768-1785378993496.json` has SHA-256
  `a4b8d80c38892062069c9e9e93dbca9a77c0176bad27e7d3b390ccae08d46402`. It links all 500 dumped
  outputs to the hashes reported by the measured render, records identical input SHA-256
  `65b8f69a7b2ee114cfb2fb49557b34cbc7e2c15f1414a81b7d94215a46de432f`, and reports 400 equivalent,
  100 divergent, 0 unverified. Every divergent row is a class diagram. The first sample keeps all
  eight node IDs and seven edge elements but has 8 frankenmermaid text tokens versus 40 incumbent
  tokens and omits 32 required member tokens.
- **Artifact provenance.** The equivalence process self-reported frankenmermaid ELF SHA-256
  `08cca9e1f3c90784fc232a510917c5c684f04ae4ec87cb878521a4cef47aa030` (7,812,640 bytes).
  The incumbent was mermaid-js `11.15.0`, bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`, through
  `/usr/bin/chromium-browser`, `Chrome/150.0.7871.128`.
- **Gate validation.** `node scripts/headtohead/svg_equivalence.mjs --self-test` passed 20 cases:
  four mutation controls and two negative controls included. The gate treats every divergent or
  undecidable required invariant as failure.
- **Full target-corpus requalification (2026-07-30).** The current self-reporting ELF
  `c410a84698acd943dc6d5eb134c119e3239414f9446cfcb702a970620f048d7d` and live pinned incumbent
  rendered every revision of both target jobs in one untimed equivalence invocation. Artifact
  `.benchmarks/headtohead/ci-docs-2k5k-equivalence/equivalence-905b01f9-1785459428706.json`
  (SHA-256 `c3c19c23920ddbbe229006d4f47327428b27cf6e5a24e954d4e2378d14b0640e`)
  links 2,000/2,000 and 5,000/5,000 dumped outputs to each engine's reported output hash.
  `ci_docs_2000` is 1,705 equivalent / 295 divergent / 0 unverified; `ci_docs_5000` is 4,291 /
  709 / 0.
- **Why both named jobs remain CANNOT.** All 190/482 class diagrams lose fields or methods because
  `compute_node_size` ignores `class_meta.attributes`/`methods` and the renderer clips compartments
  to that undersized bound. Another 142/190 plus 358/482 have node-set divergence because `o--`
  falls through to `--` and creates phantom `C*-o` nodes; `*--` silently loses composition
  semantics (`bd-92b6`). State diagrams diverge 105/169 and 227/390: labels containing a top-level
  `&` drop transitions in 75/167 diagrams, while `<config>` or `(429)` is misread as node-shape
  syntax and loses visible content in 60/129 diagrams (`bd-yq3k`). All flowcharts (1,149/2,882),
  sequence diagrams
  (327/804), ER diagrams (165/442), and 64/163 unaffected state diagrams pass.
- **Recoverable work is a different benchmark.** Filtering to the passing 1,705/4,291 diagrams
  changes the named job's revision count, syntax mix, input hash, traversal, allocation pressure,
  and caller scheduling boundary. Such a subset may be pinned under a new ID, but cannot inherit
  either `ci_docs_2000` or `ci_docs_5000`; the certified `ci_equiv_512` job already occupies that
  equivalence-clean CI lane.
- **Historical correction.** The numeric `class_50`, `doc_build_40`, `ci_batch_500`,
  `docs_site_50`, and `docs_site_200` rows above are known to contain this unequal-work class
  surface and are not current campaign output. Other numeric rows that predate the exact-output
  gate remain internal historical measurements, not public competitive claims, until their exact
  corpora receive a passing linked equivalence artifact. Public docs now state no numeric ratio for
  jobs where both engines complete.
- **Concrete retry predicate.** First close `bd-4isi`, `bd-92b6`, and `bd-yq3k`, proving class
  fields/methods and relationship kinds plus state transition edges/labels under every production
  dispatch used by the harness; extend the linked oracle to cover class relationship semantics.
  Then generate separate exact `ci_docs_2000` and `ci_docs_5000` artifacts with zero divergent and
  zero unverified diagrams, linked to the same input SHA-256, process-self-reported ELF, and pinned
  mermaid-js bundle used for timing. Only after both artifacts pass may a fresh exclusive `trj`
  claim run the complete 1/2/4/8/16/32/64/96/128 sweeps with per-arm A/A controls, observed worker
  counts, governor/ISA/topology provenance, and bootstrap median-CI adjudication. CV remains
  provenance only.

## SEMANTIC ADMISSION ONLY: five demoted class-mixed whole jobs are equivalence-clean (2026-07-31)

**Beads:** `bd-jqko`, `bd-4sc9`. **Lane:** cod (`LavenderMill`).
**Executing ELF SHA-256 (self-reported by process):**
`499e4bf6b23ef6bfe4fccf9df4a086274d51f42ebe28b2f4ae5cc0c249a4eb47` (7,897,696 bytes).
**A/A null control (same invocation):** incomplete by construction and therefore not a performance
gate. The scalar Rust dump arms reported medians `0.994631` (`class_50`), `1.016480`
(`doc_build_40`), `0.989018` (`ci_batch_500`), `0.994573` (`docs_site_50`), and `1.009611`
(`docs_site_200`), each inside the corrected `[0.98, 1.02]` clause. The untimed `--render-once`
incumbent dump arm did not collect mermaid-js A/A samples, so no ratio, win, loss, or corrected-null
performance verdict exists. Any future timed invocation must independently keep every arm's null
median in `[0.98, 1.02]` and pass the bootstrap-CI and 2x-null-margin clauses; CV remains provenance
only.

- **Why this rerun was obligatory.** Older artifacts for `doc_build_40` and `ci_batch_500` predated
  class Tier-2 adjudication, while the historical numeric rows for all five jobs were demoted after
  the exact-output gate exposed missing class/state work. The class-member,
  relationship-kind, state-label, and inheritance retry predicates are now closed, so the exact
  named corpora—not filtered substitutes—were rerun through the current oracle.
- **Exact result.** `class_50` is **1/1 equivalent**, `doc_build_40` **40/40**,
  `ci_batch_500` **500/500**, `docs_site_50` **50/50**, and `docs_site_200` **200/200**, with zero
  divergent and zero unverified diagrams in every row. All 130 class diagrams received Tier-2
  relationship adjudication and passed. The pinned input SHA-256 values are respectively
  `d1d7ef8c8e7c8d1dab2da8fbd56565dc97148e8fb1651d23fe43140e8c4ef831`,
  `8badedbf69bc204d952af1ba780c07569b7eb1091ff5d0fdd400dd2e3f6b59d7`,
  `65b8f69a7b2ee114cfb2fb49557b34cbc7e2c15f1414a81b7d94215a46de432f`,
  `c8d5cf8e88c26fa8fa2d7b304fab8ed0045883677d5a929d10aaf1da0b4b1638`, and
  `768a6657c4f0da87f0f06910cd6d93503599b126e9c502352b8a0e808d5ce90c`.
- **Output-equivalence check.** One shared extractor processes both engines. It gates rendered-text
  containment for every family; node IDs and rendered-path topology cross-engine and against
  input-derived truth where declared; and class relationship kind plus owning endpoint
  cross-engine and against source. Referenced marker definitions must encode the correct
  diamond geometry/fill or a hollow inheritance triangle facing away from the path. Unknown or
  undecidable required invariants never pass. This is neither SVG byte equality nor a rasterized
  perceptual diff.
- **Linkage and incumbent.** Artifact
  `.benchmarks/headtohead/class-mixed-requalification-v1/equivalence-ba6d3cf7-1785486443840.json`
  has SHA-256 `790e35d0aa5ea9a53d6f246b78bc7d50760bae995a65aa6faf5bebb5324a6011`.
  The separate `class_50` artifact
  `.benchmarks/headtohead/class-50-requalification-v1/equivalence-3a62764a-1785486786426.json`
  has SHA-256 `c94d77e8bd12aa73c0781abbe9b1af79e83936f91fc79654b789eeefbf883934`.
  Every engine dumped every expected revision, and each concatenated dump hash exactly matches that
  engine's self-reported output SHA-256. The live incumbent is mermaid-js `11.15.0`, bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`, through
  `/usr/bin/chromium-browser` (`Chrome/150.0.7871.128`).
- **Build, thread, and host provenance.** The immutable ELF is the strict-RCH artifact already
  built from pinned base `85fef646` with `--clean-overlay` and only the committed SVG follow-up
  represented by `4798d977`; no co-tenant edit entered the executable. Every row requested and
  actually observed **1** frankenmermaid scalar worker and **1** mermaid-js main execution thread.
  Host `thinkstation1` is an AMD Ryzen Threadripper PRO 5975WX with 64 logical CPUs, complete
  `amd-pstate-epp` provenance (`powersave` governor, `performance` EPP, boost enabled), and complete
  ISA provenance (AVX2/FMA/BMI2/VAES present; AVX-512 absent).
- **Disposition / next predicate.** Semantic admission is recovered, but the historical ratios
  remain non-current because they were not measured in this invocation. No `trj` claim was taken.
  Timing may resume only after earlier central bookings and host-wide external work clear, under a
  fresh exclusive claim and clean census, with the same inputs/bundle/ELF identity, live incumbent,
  actual observed threads, linked passing output artifact, complete governor/ISA/topology
  provenance, and the corrected same-invocation A/A gate above.

## SEMANTIC ADMISSION ONLY: 25-schema database-catalog publish (2026-07-31)

**Bead:** `bd-j7e7`. **Lane:** cod (`LavenderMill`).
**Executing ELF SHA-256 (self-reported by process):**
`499e4bf6b23ef6bfe4fccf9df4a086274d51f42ebe28b2f4ae5cc0c249a4eb47` (7,897,696 bytes).
**A/A null control (same invocation):** incomplete by construction and therefore not a performance
gate. The scalar Rust dump arm reported median `0.998408`, bootstrap 95% CI
`[0.992740, 1.010995]`; the median itself is inside the corrected `[0.98, 1.02]` clause. The
untimed `--render-once` mermaid-js dump arm collected no incumbent A/A samples, so this invocation
establishes no current ratio, win, loss, or corrected-null performance verdict. A future timed
invocation must independently keep every arm's null median in `[0.98, 1.02]` and pass the
bootstrap-CI and 2x-null-margin clauses; CV remains provenance only.

- **Whole-job semantic boundary.** `schema_catalog_25` renders all 25 generated database schemas,
  comprising 662 authored entities and 637 relationships. Both engines consumed the identical
  25-revision input SHA-256
  `ac7c34e951a2accfcd763d088666a0cbdd4d8770604f556540a9f7a8462df7e7`.
- **Oracle correction and exact result.** The first current-oracle artifact correctly passed
  rendered-text containment 25/25 but rejected every node-id set because it reconstructed
  frankenmermaid's renderer-owned element id `fm-node-s0-e0-0` as `s0-e0`, even though the group
  retained the exact authored id `S0_E0` in `data-id`. The shared extractor now prefers decoded
  author-facing `data-id` values for both engines and falls back to canonicalized element ids only
  when that attribute is absent. A mutation control also proves that this path preserves
  underscores and does not strip an authored trailing counter. The rerun is **25/25 equivalent**,
  zero divergent, zero unverified; node-id and text checks passed 25/25, and all 11 diagrams whose
  ER geometry permitted cross-engine topology adjudication passed it.
- **Output-equivalence check.** One shared extractor processes both engines. It gates rendered-text
  containment and authored node-id sets for this ER corpus, and opportunistically compares
  rendered-path topology when both engines' geometry is unambiguous. Unknown required invariants
  never pass. ER topology is not yet claimed against input-derived truth, so the 14 geometrically
  undecidable topology checks are explicitly not promoted to Tier 2. This is neither SVG byte
  equality nor a rasterized perceptual diff.
- **Linked artifacts.** The retained failing artifact
  `.benchmarks/headtohead/schema-catalog-25-requalification-v1/equivalence-8f8169d4-1785486973420.json`
  has SHA-256 `39fc4ad4d4fd25d947b5672eee3b68d4db6bc87e6cc80793b91af3ac61b997bf`.
  The passing artifact
  `.benchmarks/headtohead/schema-catalog-25-requalification-v1/equivalence-8f8169d4-1785487124743.json`
  has SHA-256 `7f73ba85095f86751edf1749cf8a5ee2a6ae774f55beb011d8e51376d50a42b8`.
  Frankenmermaid dumped 25/25 revisions with SHA-256
  `49357f033671feb6de1636d4e45f19f2945dbf60bf789060c5843313a9726b82`; mermaid-js dumped
  25/25 with SHA-256
  `2858d502b25b0619273b50d2d4a3acc0d746a94c0faa147523d070db917228f4`.
  Both hashes exactly match the values self-reported by their measured arms.
- **Incumbent, thread, and host provenance.** The live comparator is mermaid-js `11.15.0`, bundle
  SHA-256 `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`,
  through `/usr/bin/chromium-browser` (`Chrome/150.0.7871.128`). Frankenmermaid requested and
  actually observed **1** scalar worker; mermaid-js requested and actually observed **1** browser
  main execution thread. Host `thinkstation1` is an AMD Ryzen Threadripper PRO 5975WX with 64
  logical CPUs, complete `amd-pstate-epp` provenance (`powersave` governor, `performance` EPP,
  boost enabled), and complete ISA provenance (AVX2/FMA/BMI2/VAES present; AVX-512 absent). The
  immutable ELF was built through strict `rch exec` from pinned base `85fef646` with
  `--clean-overlay`; no co-tenant edit entered the executable.
- **Disposition / retry predicate.** Semantic admission is current, but the historical
  database-catalog ratio remains non-current because this was deliberately untimed. Take a fresh
  exclusive central `trj` claim only after all earlier bookings and host-wide external work clear,
  then measure this exact linked corpus side-by-side against the live pinned incumbent with the
  same ELF identity, actual observed threads, complete governor/ISA/topology provenance, and the
  corrected same-invocation A/A gate above.

## SEMANTIC ADMISSION ONLY: 201-revision live-edit whole job (2026-07-31)

**Bead:** `bd-shou`. **Lane:** cod (`LavenderMill`).
**Executing ELF SHA-256 (self-reported by process):**
`499e4bf6b23ef6bfe4fccf9df4a086274d51f42ebe28b2f4ae5cc0c249a4eb47` (7,897,696 bytes).
**A/A null control (same invocation):** incomplete by construction and therefore not a performance
gate. The scalar Rust dump arm reported median `0.997721`, bootstrap 95% CI
`[0.989401, 1.011397]`; the median itself is inside the corrected `[0.98, 1.02]` clause. The
untimed `--render-once` mermaid-js dump arm collected no incumbent A/A pairs, so this invocation
establishes no current ratio, win, loss, or corrected-null performance verdict. A future timed
invocation must independently keep every arm's null median in `[0.98, 1.02]` and pass the
bootstrap-CI and 2x-null-margin clauses; CV remains provenance only.

- **Recognizable whole job.** `edit_trace_200x200` is one live-preview session containing all
  **201** successive full-document revisions, not a per-diagram kernel mean. The revisions contain
  46,967 total node instances and 53,399 total edge instances. Both engines consumed identical
  input SHA-256 `a0323173208959bd18817c4b4bd7147a79913f06f513ff5669c1b0773e2812f5`.
- **Exact result.** All **201/201** revisions are equivalent, with zero divergent and zero
  unverified. Every revision passed rendered-text containment, authored node-id equality,
  cross-engine rendered-path topology, and topology against input-derived truth for each engine;
  all 201 therefore received Tier-2 flowchart adjudication.
- **Output-equivalence check.** One shared extractor processes both engines. It gates
  incumbent-rendered text containment and authored node-id sets, reconstructs rendered-path
  endpoint pairs, compares topology cross-engine, and independently compares both engines against
  topology derived from the pinned source revision. Unknown or undecidable required invariants
  never pass. This is neither SVG byte equality nor a rasterized perceptual diff.
- **Linkage and incumbent.** Artifact
  `.benchmarks/headtohead/edit-trace-200x200-requalification-v1/equivalence-f4bd5b2c-1785487847701.json`
  has SHA-256 `cd9409f6eacf59bdcb62289297ec3600c7ecf03ed98be817f02d3f7e61e8025e`.
  Frankenmermaid dumped 201/201 revisions with SHA-256
  `b206614a5b779e8aa1bde3c64d0617d7acbac3343ffaaee06bf1ccfc85ebd88c`; mermaid-js dumped
  201/201 with SHA-256
  `fa5512cd10ca004e1e87b593b3fd29f1fbd3e45d0881dd7eb8d78975c071430a`.
  Both concatenated hashes exactly match the values self-reported by their measured arms. The live
  incumbent is mermaid-js `11.15.0`, bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`,
  through `/usr/bin/chromium-browser` (`Chrome/150.0.7871.128`).
- **Build, thread, and host provenance.** The immutable ELF was built through strict `rch exec`
  from pinned base `85fef646` with `--clean-overlay`; no co-tenant edit entered the executable.
  Frankenmermaid requested and actually observed **1** scalar worker; mermaid-js requested and
  actually observed **1** browser main execution thread. Host `thinkstation1` is an AMD Ryzen
  Threadripper PRO 5975WX with 64 logical CPUs, complete `amd-pstate-epp` provenance (`powersave`
  governor, `performance` EPP, boost enabled), and complete ISA provenance (AVX2/FMA/BMI2/VAES
  present; AVX-512 absent).
- **Disposition / retry predicate.** Semantic admission is current, but the historical live-edit
  ratio remains non-current because it was measured with an older ELF in a separate invocation.
  Take a fresh exclusive central `trj` claim only after all earlier bookings and host-wide external
  work clear, then measure this exact linked 201-revision job side-by-side against the live pinned
  incumbent using this ELF, actual observed threads, complete governor/ISA/topology provenance,
  fresh-browser isolation, and the corrected same-invocation A/A gate above.

## SEMANTIC ADMISSION ONLY: two short flowchart rows (2026-07-31)

**Bead:** `bd-j700`. **Lane:** cod (`LavenderMill`).
**Executing ELF SHA-256 (self-reported by process):**
`499e4bf6b23ef6bfe4fccf9df4a086274d51f42ebe28b2f4ae5cc0c249a4eb47` (7,897,696 bytes).
**A/A null control (same invocation):** incomplete by construction and therefore not a performance
gate. The scalar Rust dump arms reported median `1.001136`, bootstrap 95% CI
`[0.964895, 1.032849]` for `flowchart_small_10`, and median `1.018000`, CI
`[0.975206, 1.061624]` for `flowchart_medium_100`. Both medians themselves are inside the corrected
`[0.98, 1.02]` clause. The untimed `--render-once` mermaid-js dump arm collected no incumbent A/A
pairs, so this invocation establishes no current ratio, win, loss, or corrected-null performance
verdict. A future timed invocation must independently keep every arm's null median in
`[0.98, 1.02]` and pass the bootstrap-CI and 2x-null-margin clauses; CV remains provenance only.

- **Exact result.** `flowchart_small_10` and `flowchart_medium_100` are each **1/1 equivalent**,
  with zero divergent and zero unverified. Both passed rendered-text containment, authored node-id
  equality, cross-engine rendered-path topology, incumbent declared-id geometry, and topology
  against input-derived truth for each engine; both therefore received Tier-2 flowchart
  adjudication. Their pinned input SHA-256 values are
  `b5402490faa78c6a7c71554296d03b46016ae1156d7cd38d258b280363b6900a` and
  `74bd26f73724626255642c427d36844d8a75f7bdf7fd47a69f8541a3ec9aea22`.
- **Output-equivalence check.** One shared extractor processes both engines. It gates
  incumbent-rendered text containment and authored node-id sets, reconstructs rendered-path
  endpoint pairs, compares topology cross-engine, and independently compares both engines against
  topology derived from the pinned input. Unknown or undecidable required invariants never pass.
  This is neither SVG byte equality nor a rasterized perceptual diff.
- **Linkage and incumbent.** Artifact
  `.benchmarks/headtohead/short-rows-requalification-v1/equivalence-bffb1641-1785487982241.json`
  has SHA-256 `1be440ce6fb4cf9f8429f25c972693e3554accaa309faad7256582fe6d1c248d`.
  The frankenmermaid dump hashes are
  `2ae42a001b97fb01146a0930f93d67c7e26f8ecb8221d6faa827b2b92d51bdd6` and
  `6e4a062abb16876763a31971b90b10341c3a03b1cb819d824b0aa26871c65e8b`;
  the mermaid-js dump hashes are
  `2f6a82745f61b49403f87ad3e39e5592017b02f86d52fa85c197efdd40ded9fd` and
  `90f846cd2c196f749aecd3784c12fa732089bb7726294dc55967f4032749f1ec`.
  Each engine dumped every expected revision and every concatenated hash exactly matches its
  measured arm's self-report. The live incumbent is mermaid-js `11.15.0`, bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`,
  through `/usr/bin/chromium-browser` (`Chrome/150.0.7871.128`).
- **Build, thread, and host provenance.** The immutable ELF was built through strict `rch exec`
  from pinned base `85fef646` with `--clean-overlay`; no co-tenant edit entered the executable.
  Every row requested and actually observed **1** frankenmermaid scalar worker and **1** mermaid-js
  browser main execution thread. Host `thinkstation1` is an AMD Ryzen Threadripper PRO 5975WX with
  64 logical CPUs, complete `amd-pstate-epp` provenance (`powersave` governor, `performance` EPP,
  boost enabled), and complete ISA provenance (AVX2/FMA/BMI2/VAES present; AVX-512 absent).
- **Disposition / retry predicate.** Semantic admission is current, but the historical short-row
  ratios remain non-current because they were measured with an older ELF in separate invocations.
  After earlier central bookings and host-wide external work clear, take a fresh quiet `trj` claim
  and time only these two exact linked rows side-by-side against the live pinned incumbent using
  this ELF, actual observed threads, complete governor/ISA/topology provenance, same-ELF
  pre/post-Chromium brackets, and the corrected same-invocation A/A gate above.

## CERTIFIED INCUMBENT WIN: equivalence-clean 512-diagram concurrent CI job (2026-07-30)

**Bead:** `bd-zwg6`. **Lane:** cod (`BlackThrush`).
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `c410a84698acd943dc6d5eb134c119e3239414f9446cfcb702a970620f048d7d`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=f8040744-1785428462046 measured_ratio=13376.178235x
**A/A null control (same invocation):** mermaid-js `n=20`, median 1.009424, 95% CI
[0.994537, 1.018283]; every Rust-before and Rust-after width carried `n=20` interleaved A/A
ratios. The t1, t8, and t64 rows passed the corrected median-CI gate; t32 and t128 failed only the
2% null-median clause and are not campaign wins. `cv_gate=never`.

- **Whole-job boundary and counted work.** One logical sample parses, lays out, renders, and
  serializes all **512** flowcharts: **10,635 nodes, 10,123 edges, 402,843 joined input bytes**.
  Both engines received 512 renders and the identical input SHA-256
  `228414f81bb6e73135bcc5244cb93503237f670bfa327b5da9310e6d777904aa`.
  Rust's timer-floor batches repeat whole 512-diagram jobs and divide only by that repeat count;
  no per-diagram mean enters a verdict.
- **Real incumbent.** The same driver invocation ran the pinned mermaid-js bundle live through
  Chrome 150.0.7871.128. Nine independent whole-job effect samples produced a
  **24,351.600 ms** median. The incumbent reported one requested and actually used main execution
  thread.
- **Output equivalence.** This is an SVG structural comparison, not byte equality and not a
  rasterized perceptual diff. All **512/512** diagrams passed rendered-text containment, node-ID
  set equality, cross-engine rendered-path edge topology, and each engine's topology against the
  input-derived graph; zero were divergent or unverified. Five mutation controls reject a dropped
  label, either engine dropping an edge, a rewired edge, and a displaced node.
- **Corrected gate.** Every effect CI excludes 1.0 and every effect clears twice the largest null
  radius. A row is accepted only when all three A/A medians—Rust before, Rust after, and
  mermaid-js—also stay within 2% of 1.0. CV and MAD never gate.

| Host | requested / observed callers | Rust whole-job median | mermaid-js / Rust, bootstrap 95% effect CI | A/A medians (Rust before / after / JS) | disposition |
|---|---:|---:|---:|---:|---|
| `thinkstation1` | 1 / 1 | 34.182970 ms | **712.389825×** [699.080772×, 736.410175×] | 0.995269 / 0.995739 / 1.009424 | **incumbent-win** |
| `thinkstation1` | 8 / 8 | 4.774816 ms | **5,100.008042×** [5,028.063855×, 5,264.077738×] | 1.000205 / 1.006868 / 1.009424 | **incumbent-win** |
| `thinkstation1` | 32 / 32 | 1.683505 ms | 14,464.821904× [14,029.697310×, 15,648.501123×] | **1.025448** / 1.003877 / 1.009424 | **inconclusive**, clause 3 |
| `thinkstation1` | 64 / 64 | 1.820520 ms | **13,376.178235×** [13,083.153389×, 13,905.084595×] | 0.991594 / 0.999440 / 1.009424 | **incumbent-win** |
| `thinkstation1` | 128 / 128, oversubscribed | 3.440187 ms | 7,078.568694× [6,654.366489×, 7,427.614224×] | **0.964308 / 0.968836** / 1.009424 | **inconclusive**, clause 3 |

- **Scaling shape.** The accepted t8 row is 7.16× faster than t1. The valid frontier is t64 at
  18.78× t1 scaling and 29.3% observed parallel efficiency. The raw t32 point is slightly faster,
  but its null bias makes it ineligible to headline. Explicit 128-way oversubscription regresses
  to 3.440187 ms; that diagnostic row is likewise not a campaign win.
- **Host and executable provenance.** `thinkstation1`, AMD Ryzen Threadripper PRO 5975WX,
  32 physical cores / 64 logical threads, all 64 CPUs in affinity, `amd-pstate-epp`, governor
  `powersave`, EPP `performance`, boost enabled. ISA recorded x86-64 with AVX2, FMA, BMI2, and
  VAES and no AVX-512. RCH builder `vmi1293453`; executing ELF 7,814,880 bytes. All eleven phases
  passed the fixed 20%-per-CPU host admission under `trj-booking:6933`; 24 samples were needed to
  obtain those eleven clear admissions.
- **Evidence.** Summary
  `.benchmarks/headtohead/ci-equiv-512-sweep/summary-f8040744-1785428462046.json`
  (SHA-256 `90eee0dede0e964c80bc41104b93bf855281a812a4128df49f61581a8dae39d2`);
  raw JSONL
  `.benchmarks/headtohead/ci-equiv-512-sweep/run-f8040744-1785428462046.jsonl`
  (SHA-256 `563fe621fab938b40464f25a17c7259af69552558fefc1ad736c25e8b5570dd5`);
  equivalence artifact
  `.benchmarks/headtohead/ci-equiv-512-equivalence/equivalence-f8040744-1785422935402.json`
  (SHA-256 `38a3d74fcfb5e0c835e18a9ad41b659bf72de30716d8f77e9b82d0b2d6555f16`).
- **Retry / re-check predicate.** Re-certify a passing row only if the corpus/input pin,
  equivalence method, pinned mermaid bundle/configuration, executing ELF/compiler profile,
  persistent-pool semantics, timer floor, corrected gate, or host topology changes. Do not chase
  t32 or t128 with another identical timing run; reopen either only after a dedicated-host change
  addresses its repeated null-median bias or one of those named artifacts/contracts changes.
- **Independent re-verification before publication (cc lane, `LilacPike`, 2026-07-30).** All three
  evidence SHA-256s recomputed and matched. The corpus was regenerated from `corpus.mjs` and
  reproduced the pinned input SHA-256 `228414f8…`, 402,843 bytes, 512 revisions, and 10,123 edges
  exactly. `svg_equivalence.mjs --self-test` re-ran clean: `{"ok":true,"cases":23,
  "mutation_controls":5,"negative_controls":2}`. The equivalence artifact's `fm_dump_sha256` equals
  the sweep summary's `fm_output_sha256` (`787024…`), so the compared bytes are the measured bytes.
  Observed caller width equals requested width at all five points; all five carry a passing
  equivalence verdict and a passing same-ELF bracket, and the only failing clause on t32/t128 is
  the null-median clause. **Published claim** now shows the complete 1/8/32/64/128 sweep with both
  withheld rows and their reason stated inline, rather than the three passing widths alone.

## SEMANTIC ADMISSION ONLY: 60-revision live-typing whole job (2026-07-31)

**Bead:** `bd-kn9b`. **Lane:** cod (`LavenderMill`).
**Executing ELF SHA-256 (self-reported by process):**
`c2b0af01dfffab49631d70a2988dfb8fa094f79daa0a14785b5c3683332bc3e2` (7,894,560 bytes).
**A/A null control (same invocation):** incomplete and failing the corrected median clause, so it
is not a performance gate. In the final equivalence-clean invocation, the scalar Rust dump arm
reported median `0.956787`, bootstrap 95% CI `[0.873823, 1.056038]`; the median itself is outside
the mandatory `[0.98, 1.02]` interval. The untimed mermaid-js `--render-once` arm collected no
incumbent A/A pairs. The retained first audit invocation had a passing Rust median `0.999667`, CI
`[0.965844, 1.016315]`, but its then-current oracle left 32 revisions unverified. Evidence from
those separate invocations is not combined. There is no current ratio, win, loss, or complete
corrected-null timing verdict; CV remains provenance only.

- **Recognizable whole job.** `typing_trace_60` is one live-preview session containing **60**
  successive full-document revisions, not a per-diagram kernel mean. Each revision renders 40
  authored nodes and 39 edges while one production-style node label grows character by character:
  **2,400 node instances and 2,340 edge instances** across the job. Both engines consumed the
  identical input SHA-256
  `1b387d42772f0e4a2e479059cbd60c002aff4451ee7b4a97bf3bd6d82c334a33`.
- **Oracle dead end and recovery.** The first current-oracle attempt was **28/60 equivalent**,
  zero divergent, and 32 unverified. Beginning at revision 28, node `N20` becomes wide enough that
  its outgoing path starts exactly on its emitted rectangle boundary; centre-only distance made
  the neighbouring narrow node almost as close and correctly refused to guess the endpoint. This
  is recoverable oracle incompleteness, not an engine semantic mismatch. The shared resolver now
  measures a path endpoint against emitted rectangle bounds when available, retains conservative
  centre distance for shapes whose exact boundary is not decoded, and still refuses zero-distance
  ties. The focused boundary controls and the full oracle self-test pass **42/42** cases.
- **Exact semantic result.** The corrected replay is **60/60 equivalent**, zero divergent, zero
  unverified. Every revision passed incumbent-rendered text containment, authored node-ID equality,
  cross-engine rendered-path topology, and each engine's topology against input-derived truth; all
  60 therefore received Tier-2 flowchart adjudication.
- **Output-equivalence check.** One shared extractor processes both engines. It gates rendered-text
  containment and authored node-ID sets, reconstructs frankenmermaid endpoints from emitted path
  and node geometry, accepts mermaid-js's per-path declared endpoints only when every declaration
  uniquely resolves against the rendered node set, compares the resulting topology cross-engine,
  and independently compares both engines against topology derived from each pinned source
  revision. Unknown or undecidable required invariants never pass. This is neither SVG byte
  equality nor a rasterized perceptual diff.
- **Linked artifacts.** The retained oracle-audit failure
  `.benchmarks/headtohead/typing-trace-60-requalification-v1/equivalence-3e7edbef-1785490994893.json`
  has SHA-256 `b25daec77549445d69122741767c2e702f47ebfc28c8fe786a5b134e179352c1`.
  The passing artifact
  `.benchmarks/headtohead/typing-trace-60-requalification-v1/equivalence-3e7edbef-1785491181243.json`
  has SHA-256 `4e36954811ade7daea0a90b25ef0851c2b7382d55ad8939f11ba394d75e242e9`.
  Frankenmermaid dumped 60/60 revisions with SHA-256
  `826e563060f688301cb2d188cfe3c052f5c93c902ab5733a8432265c557aba97`;
  mermaid-js dumped 60/60 with SHA-256
  `6d290e8b4c68a6e951ddc3a0b072f9168df8a74f20e320ccd5100d50790320d9`.
  Both concatenated hashes exactly match the values self-reported by their measured arms.
- **Incumbent, observed threads, and host.** The live comparator is mermaid-js `11.15.0`, bundle
  SHA-256 `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`,
  through `/usr/bin/chromium-browser` (`Chrome/150.0.7871.128`). Frankenmermaid requested and
  actually observed **1** scalar worker; mermaid-js requested and actually observed **1** browser
  main execution thread. Host `thinkstation1` is an AMD Ryzen Threadripper PRO 5975WX with 64
  logical CPUs, kernel `6.17.0-35-generic`, complete `amd-pstate-epp` provenance (`powersave`
  governor, `performance` EPP, boost enabled), and complete x86-64 ISA provenance
  (AVX2/FMA/BMI2/VAES present; AVX-512 absent).
- **Strict build receipt.** Worker `ovh-a` built this executable through strict `rch exec` from base
  `2bb114ff` with `--clean-overlay` and only `crates/fm-layout/src/lib.rs`; no co-tenant edit entered
  the binary, no local fallback occurred, and no task-specific Cargo target directory was created.
- **Disposition / retry predicate.** The audit's cheap semantic-conversion queue is now closed, but
  the historical live-typing ratio remains non-current. Retry timing only after an exclusive
  central `trj` claim and host-wide external work are clear; run this exact linked 60-revision job
  side-by-side against the live pinned incumbent with the same self-reporting ELF, actual observed
  threads, complete governor/ISA/topology provenance, and a corrected same-invocation A/A gate in
  which every arm's null median is inside `[0.98, 1.02]` and the CI and 2x-null-margin clauses also
  pass. Reopen semantic admission only if the input pin, incumbent bundle/configuration, executing
  ELF, or equivalence contract changes.

## SEMANTIC ADMISSION ONLY: exact 2k/5k CI jobs on the prepared timing ELF (2026-07-31)

**Bead:** `bd-l7d2`. **Lane:** cod (`LavenderMill`).
**Executing ELF SHA-256 (self-reported by process):**
`dfd2636c4e841b18b3b24d48108e918a3b1d9aa05d1ab47c9d88ea2182397d7c` (7,897,696 bytes).
**A/A null control (same invocation):** incomplete and failing the corrected median clause, so this
is not a performance gate. The scalar Rust dump arms reported median `1.011776`, bootstrap 95% CI
`[0.924097, 1.081215]` for `ci_docs_2000`, and median **`1.050770`**, CI
`[0.900900, 1.102413]` for `ci_docs_5000`. The 5k median itself is outside the mandatory
`[0.98, 1.02]` interval. The untimed mermaid-js `--render-once` arms collected no incumbent A/A
pairs. This invocation establishes no ratio, win, loss, or complete corrected-null timing verdict;
CV remains provenance only.

- **Why this replay was required.** Rebuilding the prior source-identical semantic candidate
  produced a different immutable ELF hash, so the older `499e4bf6…` output artifact could not be
  cited for timing with this process. No identity equivalence was guessed. The exact two named jobs
  were replayed end-to-end with the new self-reporting ELF and live incumbent before target-host
  access.
- **Recognizable whole jobs and exact result.** `ci_docs_2000` renders **2,000/2,000** complete
  diagrams and `ci_docs_5000` renders **5,000/5,000**, with zero divergent and zero unverified in
  either job. The 2k family mix is 1,149 flowcharts, 327 sequence diagrams, 165 ER diagrams, 169
  state diagrams, and 190 class diagrams. The 5k mix is 2,882 / 804 / 442 / 390 / 482 respectively.
  Input SHA-256 values remain
  `ae5b6ff4da07288524f948b38d6fc1df065f4797de3f0ab115ac2621cf23598b` and
  `26e5710af60c5548521f75aaf047672a4316d657885a8ab1d119338ba1804f41`.
- **Output-equivalence check.** One shared extractor processes both engines. It gates rendered-text
  containment for every syntax family; authored node-ID equality where applicable; rendered-path
  topology cross-engine and against input-derived truth for flowchart/state; and class
  relationship marker kind plus owning end cross-engine and against input truth. Referenced marker
  definitions must encode the correct hollow/filled diamond or outward-facing hollow inheritance
  triangle. Unknown markers diverge and undecidable required invariants are unverified, never pass.
  This is neither SVG byte equality nor a rasterized perceptual diff.
- **Artifact and exact linkage.** Artifact
  `.benchmarks/headtohead/ci-docs-2k5k-equivalence-dfd2636c/equivalence-6bf64c28-1785494049570.json`
  has SHA-256 `1981e863d39bfd332de531fc4acb4590a4beef8aa2b35af93b8d6c0a7cce9610`.
  For 2k, frankenmermaid and mermaid-js dumped all 2,000 revisions with SHA-256
  `7f2cb4af86d3f05d935357843a328e924278934138b6986c9d12f1dbdbb16ded` and
  `beff28feaec022588f4d051bc35931d2319203fd293564f128379ede1f504acf`.
  For 5k, the 5,000-revision hashes are
  `754607625d0e5d332a3ae1c19a4627a01b9a598886e76874c07b8b45e956641e` and
  `a052ebbb86ef2da26ad8a922bb7b24bb367895b03c481e860c869474b6b57324`.
  Every revision count matches and every concatenated hash exactly equals its measured arm's
  self-report.
- **Incumbent, observed threads, and host.** The live comparator is mermaid-js `11.15.0`, bundle
  SHA-256 `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`,
  through `/usr/bin/chromium-browser` (`Chrome/150.0.7871.128`). Both rows requested and actually
  observed **1** frankenmermaid scalar worker and **1** mermaid-js browser main execution thread.
  Host `thinkstation1` is an AMD Ryzen Threadripper PRO 5975WX with 64 logical CPUs, kernel
  `6.17.0-35-generic`, complete `amd-pstate-epp` provenance (`powersave` governor, `performance`
  EPP, boost enabled), and complete x86-64 ISA provenance (AVX2/FMA/BMI2/VAES present; AVX-512
  absent).
- **Strict isolation.** Worker `vmi1264463` built the executable with
  `RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch --no-self-healing exec --base 4798d977
  --clean-overlay --no-overlay -- cargo build -j2 --profile release -p frankenmermaid-cli
  --example headtohead`. No co-tenant edit entered the executable, no local fallback occurred, and
  no task-specific Cargo target directory was created. The equivalence run used a detached clean
  `6bf64c28` harness worktree under `/tmp`, so the shared checkout's concurrent script edits did not
  enter the oracle or Chromium arm.
- **Disposition / retry predicate.** The prepared ELF is semantically eligible for timing, but
  target-host access remains blocked. The central `trj-booking` tail reports the hourly fsck process
  still violating the fixed 20% all-CPU gate, with a pre-existing Whisper exact retry first after a
  genuine clear. Take no out-of-order claim. After that work formally releases and a fresh
  per-process/per-CPU census is clear, post a new `[trj] CLAIM frankenmermaid`, run each exact job
  through the complete 1/2/4/8/16/32/64/96/128 sweep, and accept every row as observed under its
  same-invocation per-arm A/A controls. Each null median itself must be inside `[0.98, 1.02]`; the
  bootstrap-CI, 2x-null-margin, scalar byte-identity, actual-worker, host/governor/ISA/topology, and
  live-incumbent gates remain binding.

## INCONCLUSIVE / NO CLAIM: live-typing timing lacks observed Rust worker provenance (2026-07-31)

**Bead:** `bd-kn9b`. **Lane:** cod (`LavenderMill`).
**Executing ELF SHA-256 (self-reported by process):**
`dfd2636c4e841b18b3b24d48108e918a3b1d9aa05d1ab47c9d88ea2182397d7c` (7,897,696 bytes).
**A/A null control (same invocation):** the corrected numeric gate passed. Rust-before used `n=20`,
median `0.996835`, bootstrap 95% CI `[0.983725, 1.011093]`; Rust-after used `n=20`, median
`1.001460`, CI `[0.995005, 1.026334]`; mermaid-js used `n=9`, median `0.995954`, CI
`[0.980878, 1.024848]`. Every median itself is inside `[0.98, 1.02]`, and the raw effect clears
twice the largest null radius. CV and MAD are provenance only. This does **not** make the row
admissible because a separate mandatory provenance field failed.

- **Observed diagnostic, not a campaign ratio.** The selected frankenmermaid whole-session median
  was `4.201155 ms` for all 60 revisions and the live mermaid-js median was `5,122.750 ms`. The
  resulting raw `1,219.367055x` observation passed the current numeric gate, but it is
  **provenance-rejected and must not be quoted as an incumbent win**. Only two independent
  incumbent effect samples were affordable under this workload pin, so the report also retains an
  explicitly non-computable independent effect CI; that CI is not required by this pinned row.
- **Precise failure.** Both timed Rust bracket arms record `fm_worker_threads_requested=1`,
  `fm_worker_threads_actually_used=null`, and `fm_thread_probe=null`. The binary reports a scalar
  execution model and was pinned to CPU 21, but neither requested width nor process affinity is an
  observation of worker participation. Mermaid-js requested and actually observed one browser main
  execution thread. The separate untimed equivalence invocation did observe `1/1` for both
  engines, but evidence from a separate invocation cannot fill a missing timing-row field.
- **Whole job and semantic work.** `typing_trace_60` is one live-preview session of 60 successive
  full documents: 2,400 node instances and 2,340 edge instances across the session. Both engines
  executed 60 `parse_layout_render_svg` operations over the same pinned input SHA-256
  `1b387d42772f0e4a2e479059cbd60c002aff4451ee7b4a97bf3bd6d82c334a33`; the harness's semantic-work
  gate reports `equal`.
- **Output-equivalence check.** The linked artifact is **60/60 equivalent**, zero divergent and
  zero unverified. One shared extractor gates rendered-text containment, authored node-ID equality,
  cross-engine rendered-path topology, and each engine's topology against input-derived truth.
  Unknown or undecidable required invariants never pass. This is neither SVG byte equality nor a
  rasterized perceptual diff. Frankenmermaid's 60-revision dump/self-report hash is
  `826e563060f688301cb2d188cfe3c052f5c93c902ab5733a8432265c557aba97`; mermaid-js's is
  `6d290e8b4c68a6e951ddc3a0b072f9168df8a74f20e320ccd5100d50790320d9`.
- **Artifacts.** Timing summary
  `.benchmarks/headtohead/typing-trace-60-dfd2636c-timing-v1/summary-6bf64c28-1785494599542.json`
  has SHA-256 `5af9e9d60225eb272ca127ede346cf09aad6f824b9d1660672f9c79e4f054211`;
  its raw JSONL has SHA-256
  `5c275508386e01ed8f3eba0a0a888e042e99552fcd260a06838ab0838f9975d7`.
  The equivalence artifact
  `.benchmarks/headtohead/typing-trace-60-dfd2636c-equivalence/equivalence-6bf64c28-1785494471229.json`
  has SHA-256 `d995d969007a58ab94e441326de48a912ce3f84d8e9193ebbe1af96faf27ac85`.
- **Incumbent, host, and isolation.** The live incumbent is mermaid-js `11.15.0`, bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`,
  through `/usr/bin/chromium-browser` (`Chrome/150.0.7871.128`). Host `thinkstation1` is an AMD
  Ryzen Threadripper PRO 5975WX with 64 logical CPUs and kernel `6.17.0-35-generic`. All 64 cpufreq
  policies consistently report `amd-pstate-epp`, governor `powersave`, EPP `performance`, and
  boost enabled. ISA provenance records AVX2/FMA/BMI2/VAES present and AVX-512 absent. Worker
  `vmi1264463` built the ELF through strict RCH from base
  `4798d977db756bf87495174a6f962e372a4f5e89` with `--clean-overlay`; the committed clean `6bf64c28`
  harness worktree excluded co-tenant edits. No task-specific Cargo target directory was created.
- **Disposition / retry predicate.** Reject this invocation as campaign evidence. Reopen only
  after the committed driver enables the binary's exact-workload thread probe for ordinary scalar
  render, requires `actual_observed_worker_threads == requested_worker_threads == 1` in both Rust
  bracket arms, and its self-test proves a missing probe fails closed. Then replay both the 60/60
  linked semantic artifact and live incumbent timing with the same pinned input/bundle, a
  process-self-reporting strict-RCH ELF, complete host/governor/ISA provenance, and the unchanged
  corrected null gate. Never copy the untimed equivalence run's observed count into this rejected
  timing row.

## CERTIFIED INCUMBENT WIN: 512-diagram physical-core render job (2026-07-31)

**Bead:** `bd-yqmd`. **Lane:** cod (`DarkDeer`).
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `c50a54df2be20ee29959297a435f36c2db0500e9c84866dcf31a649aff4398b3`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=a1be1207-1785504206583 measured_ratio=27419.823018x
**A/A null control (same invocation):** at the selected 64-worker frontier, Rust-before median
`0.998500`, bootstrap 95% CI `[0.988183, 1.005099]`; Rust-after median `1.000805`, CI
`[0.995813, 1.007383]`; mermaid-js median `1.001512`, CI `[0.986584, 1.016601]`. Every
before/after/incumbent null median at every width is inside `[0.98, 1.02]`; `cv_gate=never`.
**Counted mechanism:** widening the borrowed flowchart-node fast path to complete double-quoted
rectangular labels reduced an exact-output interleaved screen from `18,527,415,007` to
`16,150,415,725` instructions (**-12.83%**) and from `8,872,014,959` to `7,766,793,808`
cycles (**-12.46%**). It bypasses Chumsky for all 10,635 quoted node declarations in this corpus.

- **Whole-job result.** One sample parses, lays out, renders, and serializes all **512** flowcharts
  as one job: 10,635 nodes, 10,123 edges, 402,843 input bytes. The live incumbent median was
  **23,143.400 ms** from nine independent whole-job samples. The selected 64-worker Rust median is
  the slower side of its same-ELF bracket, **0.844039 ms**, for **27,419.823018x** with bootstrap
  95% effect CI **[26,615.673663x, 28,086.705990x]**.

| requested / observed Rust workers | conservative Rust median | mermaid-js / Rust, bootstrap 95% effect CI | A/A medians (Rust before / after / JS) | verdict |
|---:|---:|---:|---:|---|
| 1 / 1 | 27.163878 ms | 851.991752x [829.496662x, 864.950012x] | 1.000067 / 1.001329 / 1.001512 | incumbent-win |
| 16 / 16 | 2.268598 ms | 10,201.631140x [9,948.470738x, 10,354.943811x] | 1.007489 / 0.999103 / 1.001512 | incumbent-win |
| **64 / 64** | **0.844039 ms** | **27,419.823018x [26,615.673663x, 28,086.705990x]** | 0.998500 / 1.000805 / 1.001512 | **incumbent-win** |
| 128 / 128 | 0.929700 ms | 24,893.406475x [24,587.586149x, 25,479.865441x] | 0.994054 / 1.001839 / 1.001512 | incumbent-win |

- **Scaling and identity.** The physical-core frontier scales **32.18x** from the scalar row, with
  50.3% observed parallel efficiency. SMT oversubscription remains valid but is slower. Every width
  self-reported its requested worker count and the identical SVG SHA-256
  `787024772b946413b6d76c7b0293a0e339a6fb3cc565aa07f8fb657153be27a4`.
- **Output equivalence.** All **512/512** diagrams passed rendered-text, node-ID, cross-engine edge
  topology, and input-derived topology checks; zero were divergent or unverified. The exact timed
  dumps link to the equivalence artifact: frankenmermaid SHA-256 `787024772b946413b6d76c7b0293a0e339a6fb3cc565aa07f8fb657153be27a4`,
  mermaid-js SHA-256 `b4d3ab5b9025d40245c6bfb259af9e61b4446a3790e0e838873968f362337bd4`.
- **Harness transport boundary.** Mermaid still executes the entire 512-render job in one page
  evaluation and times it in-page. After that timer stops, the harness retrieves the 20.6 MB SVG
  array in bounded CDP chunks; this avoids Node's WebSocket frame ceiling without splitting or
  shortening incumbent work.
- **Host and executable provenance.** `threadripperje`, AMD Ryzen Threadripper PRO 5995WX,
  64 physical cores / 128 logical threads, complete 128-CPU affinity, `amd-pstate-epp` performance
  governor/EPP, boost enabled, AVX2/FMA/BMI2/VAES present and AVX-512 absent. Strict RCH worker
  `hz2` built the 7,902,736-byte ELF with Rust nightly `8ab9fdff5`; all nine phase admissions passed
  the fixed 20%-per-CPU gate under central claim `trj-booking:7952`.
- **Evidence.** Summary
  `.benchmarks/headtohead/ci-equiv-512-sweep/summary-a1be1207-1785504206583.json`
  (SHA-256 `0b3807102129d105bc62a68d83968d03d656f5488fd81928dba6b2f3a401fb69`);
  raw JSONL `.benchmarks/headtohead/ci-equiv-512-sweep/run-a1be1207-1785504206583.jsonl`
  (SHA-256 `8664f5a86b461a75d85141d05378ea0b9b8a2a726165700b19e32953425574df`);
  equivalence artifact
  `.benchmarks/headtohead/ci-equiv-512-equivalence/equivalence-a1be1207-1785502086233.json`
  (SHA-256 `36882b30c75b604747aa61d6c324e6ed265ff5a000d02cc1922b05fce928457a`).
- **Retry predicate.** Re-run only if the corpus, equivalence contract, pinned incumbent, executing
  ELF/compiler profile, persistent-pool semantics, thread frontier, timer boundary, or corrected
  median-CI gate changes.

## CERTIFIED INCUMBENT WIN: 384-diagram shared-subgraph compile/render job (2026-07-31)

**Bead:** `bd-jn5q`. **Lane:** cod (`DarkDeer`).
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `b0f3b58727a349b994fce43d7ffa09a9399c740064f01f7795faa6a6ab1f7491`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=summary-ba809bd4-1785524984520 measured_ratio=17060.38182452497x
**A/A null control (same invocation):** at the selected 64-worker frontier, Rust-before median
`0.991830`, bootstrap 95% CI `[0.972711, 1.019545]`; Rust-after median `0.984900`, CI
`[0.944250, 1.002979]`; mermaid-js median `1.004688`, CI `[1.001903, 1.018114]`. The
whole-job effect CI excludes 1 and clears twice the largest null radius; `cv_gate=never`.

- **Mechanism.** `FlowchartBatchParsePlan` recognizes complete repeated leading subgraph blocks,
  parses each exact prefix once into immutable replay syntax, and sends only each diagram's unique
  suffix through the parser. The plan contains no shared mutable state, so the existing persistent
  Rayon pool concurrently parses, lays out, renders, and serializes the distinct diagrams without
  locks or cross-worker IR clones.
- **Whole-job result.** One sample processes **384 distinct flowcharts** sharing a complete 48-node
  prefix: 21,495 nodes, 21,111 edges, and 822,360 input bytes. Live mermaid-js measured
  **51,101.900 ms** versus the slower side of the 64-worker same-ELF bracket, **2.995355 ms**, or
  **17,060.381825x** with bootstrap 95% effect CI
  **[16,581.249829x, 17,414.945596x]**. The Rust job sustained **128,198 diagrams/s**.
- **Scaling and identity.** The same job measured **46.420632 ms** with one requested and observed
  worker and **2.995355 ms** with 64 requested and observed workers, a **15.50x** whole-pipeline
  scaling gain. Mermaid-js requested and used one browser main execution thread. Every retained
  width self-reported the same 7,922,768-byte ELF and SVG SHA-256
  `033cbbf5c63601787b3b6d7e5230dca8a2d6c2eb7ac58f5d8cb2e8e56a9a82d4`.
- **Output equivalence.** All **384/384** diagrams passed the shared engine-neutral structural
  extractor, with zero divergent and zero unverified. The byte-identical input SHA-256 was
  `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`.
- **Host and build provenance.** `thinkstation1`, AMD Ryzen Threadripper PRO 5975WX, 32 physical
  cores / 64 logical threads, complete 64-CPU affinity, AVX2/FMA/BMI2/VAES present and AVX-512
  absent. Strict RCH worker `hz2` built from base `78615c6b46e879727b715d952061b5b74a335ae4`
  with a clean overlay and Rust nightly `8ab9fdff5`.
- **Evidence.** Summary
  `.benchmarks/headtohead/ci-shared-subgraph-384-sweep-thinkstation1-8229/summary-ba809bd4-1785524984520.json`
  (SHA-256 `38ebd0e9db800622c8cd5cfe7f3a15fa9df7a5b2e4f9d896cb3dfb976eba9353`);
  raw JSONL
  `.benchmarks/headtohead/ci-shared-subgraph-384-sweep-thinkstation1-8229/run-ba809bd4-1785524984520.jsonl`
  (SHA-256 `0a87a9b9af64be1da425cbccc8abb5c045c4c3f45a5b7b24686ae28f5b625104`);
  equivalence artifact
  `.benchmarks/headtohead/ci-shared-subgraph-384-equivalence-b0f3/equivalence-78615c6b-1785514471598.json`
  (SHA-256 `3a2534d920d0846051284cbcb6708e675fe0b7b309b2261b3410b6106f5a6ac6`).
- **Retry predicate.** Re-run only if the shared-prefix corpus, parser replay semantics, persistent
  pool, equivalence contract, pinned incumbent, executing ELF/compiler profile, thread frontier,
  timer boundary, or corrected median-CI contract changes.

## KEEP: caller-owned exact SVG prefix reuse in batch workers (2026-08-01)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-6t9z`. **Lane:** cod (`BlackThrush`).
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `bddb134cc226880b5cae41d64c6100be13a4656d08e2e15ae9f48bc9548918a7`
**A/A null control (same invocation):** the short interleaved old/new process run's internal
old and candidate null medians were approximately `1.0122` and `1.0562`; timing null noise was not
used for acceptance.
**Counted mechanism:** seven same-host `perf stat` repetitions over the exact 384-diagram,
64-worker job measured 11,009,143,333 instructions and 7,731,741,579 cycles for the prior ELF,
versus 8,022,146,371 instructions and 6,423,478,152 cycles for the candidate: **27.13% fewer
instructions** and **16.92% fewer cycles** (`1.3723x` and `1.2037x` ratios).

- **Mechanism.** One `SvgBatchRenderer` belongs to each fixed-shard/Rayon worker. It compares the
  prior IR, layout, and render configuration exactly, copies byte-identical serialized edge/node
  prefix fragments, and emits only the distinct suffix. There are no hashes, locks, or shared
  mutable caches, and ordinary single-diagram rendering does not allocate an `Arc`.
- **Whole-job result.** Nine interleaved old/new process pairs on `thinkstation1`, each pinned to
  CPUs 0-63 and reporting all 64 workers, measured old median 1,488,221 ns and candidate median
  1,219,832 ns: **1.2200x**. Both ELFs emitted aggregate SVG SHA-256
  `6410d31e4b9b9e96053fe237b7f45bc13eb50a80badb35dff06fa7d09f24a6ab`.
- **Equivalence.** Live mermaid-js 11.15.0 structural verification passed **384/384**, with zero
  divergent and zero unverified diagrams. Artifact SHA-256
  `d64c81148a5abc1b378ab7e4b3a7ec87dff60428de3deb327d23612a4a5586bb`.
- **Live-incumbent corroboration.** A same-invocation scalar bracket measured 24.699 ms Rust versus
  52,379.100 ms mermaid-js (`2,120.7x`), but the Rust bracket/null gate failed; this row therefore
  remains maintenance-only and supports no competitive claim. Summary SHA-256
  `0fa7da717f2d92253395deb5ed8bb330ddb6baa4c077aa7f44d8d11cd4624e7a`.
- **Retry predicate.** Re-run only if fragment boundaries, exact-prefix equality, the shared-prefix
  corpus, worker ownership, executing ELF, or equivalence contract changes.

## KEEP: one-pass closed-form directed-path tree layout (2026-08-01)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-07ml`. **Lane:** cod (`BlackThrush`).
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `34c3550fa644e2ada586e2654941821a11c7742ccfb683d448dbfdd8ce0847d7`
**A/A null control (same invocation):** all 18 old/candidate processes ran their own nine-pair
Rust null control. Two individual process CIs excluded 1, so short-run timing noise was not the
acceptance gate; both arms emitted byte-identical null and result hashes.
**Counted mechanism:** seven same-host `perf stat` repetitions over the exact 384-diagram,
64-worker whole job measured 53,775,960,976 instructions and 39,013,420,444 cycles for the prior
ELF, versus 49,196,548,570 instructions and 37,093,193,064 cycles for the candidate: **8.52% fewer
instructions** and **4.92% fewer cycles** (`1.0931x` and `1.0518x` ratios).

- **Mechanism.** A validated directed path now takes a closed-form layout route that computes path
  order, suffix subtree spans, rank centers, node boxes, and direct edge paths in linear passes. It
  skips the generic CSR tree, BFS queues, child/rank sorting, rank buckets, and a redundant path
  validation pass while preserving the generic tree recurrence exactly.
- **Whole-job result.** Nine alternating old/new process pairs on `thinkstation1`, pinned to CPUs
  0-63 and self-reporting 64/64 workers, measured prior median 1,161,174 ns and candidate median
  1,114,124 ns: **1.0422x**. Both ELFs emitted aggregate SVG SHA-256
  `6410d31e4b9b9e96053fe237b7f45bc13eb50a80badb35dff06fa7d09f24a6ab`.
- **Live-incumbent corroboration.** A fresh same-invocation whole-job structural run measured the
  candidate and live mermaid-js 11.15.0 and passed **384/384** diagrams, with zero divergent and
  zero unverified. Its scalar Rust observation was 15.550 ms and live mermaid-js was 53,760.8 ms;
  this was an equivalence run rather than the corrected competitive gate, so this row remains
  maintenance-only and supports no competitive ratio claim. Artifact SHA-256
  `ee9bfb92cdf13d37e7e0802577662554d548c02603922086aebd1969a6b14ad2`.
- **Evidence.** Interleaved prior/candidate JSONL SHA-256
  `9efa6df34d99519613c8c943ef968ed4854908278887a94ee9300ee7da23c421` /
  `fe3fb826b25f90067dd96ac878bbad5a375ebf1d2e19cc4ee87c1b168b49a31b`; counted prior/candidate
  CSV SHA-256 `e6c65c480d220495711884850dbfdd71e3851cf3247bfa086183bee3f3e84a3b` /
  `903b783780e765990137de16891fd53edb77c1b59ec5aa804a556aa2901e4438`.
- **Validation.** Strict-RCH workspace check and clippy passed; all 442 `fm-layout` tests plus
  doctests passed; formatting and diff checks passed.
- **Retry predicate.** Re-run only if path validation, the tree center recurrence, edge routing,
  shared-prefix corpus, worker ownership, executing ELF, or equivalence contract changes.

## KEEP: reusable parser builder slots with borrowed batch rendering (2026-08-01)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-bb1r`. **Lane:** cod (`BlackThrush`).
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `8df8cb24e5ed61c7c433c4aedbc4cdaa0c42220a86ec7f5f80625c1a1ff350b9`
**A/A null control (same invocation):** all nine alternating prior/candidate processes ran their
own nine-pair Rust null control. The medians across those process-level null medians were
`1.008130` for the prior ELF and `1.018052` for the candidate; counted work, exact output identity,
and semantic equivalence were the acceptance evidence rather than short-run timing noise.
**Counted mechanism:** seven same-host `perf stat` repetitions over the exact 384-diagram,
64-worker whole job measured 49,818,218,844 instructions and 37,105,265,188 cycles for the prior
ELF, versus 48,496,044,931 instructions and 35,430,022,894 cycles for the candidate: **2.65% fewer
instructions** and **4.51% fewer cycles** (`1.0273x` and `1.0473x` ratios).

- **Mechanism.** Each fixed-shard/Rayon worker now owns one `FlowchartBatchParseScratch` beside its
  `SvgBatchRenderer`. The shared-prefix compiler resets that builder in place, recycling prefix IR
  vectors and strings before parsing each distinct suffix. A lifetime-bounded parse reference lets
  layout and rendering consume the slot without a full `MermaidDiagramIr` clone; the parser issues
  an exact prefix certificate, and the renderer retains only layout/fragments rather than the prior
  IR. The production `render-batch` command and the benchmark exercise the same shared-nothing API.
- **Whole-job result.** Nine alternating prior/candidate process pairs on `thinkstation1`, pinned
  to CPUs 0-63 and self-reporting 64/64 workers, measured prior median **1,091,959 ns** and candidate
  median **1,005,521 ns**: **1.085963x**. Both ELFs emitted aggregate SVG SHA-256
  `6410d31e4b9b9e96053fe237b7f45bc13eb50a80badb35dff06fa7d09f24a6ab`.
- **Live-incumbent corroboration.** The exact candidate ELF and pinned live mermaid-js 11.15.0 ran
  together over all 384 diagrams: the scalar Rust observation was **14.172 ms**, mermaid-js was
  **54,426.3 ms**, and structural verification passed **384/384** with zero divergent and zero
  unverified. This equivalence invocation has no incumbent null arm, so it supports no competitive
  ratio and this row remains maintenance-only. Artifact SHA-256
  `15ed825d29bba8184f6b49420810aeabff85185c15a756dc7f9bede940acbff2`.
- **Evidence.** Interleaved prior/candidate JSONL SHA-256
  `5191e4b2bed3baef7c638f6e9d61cc7cfa576bae2cfccb1be6b4304856675558` /
  `5256a79130f590df023144cdf0695f0e66d5aa4b85c19a80a5bb29eaa5bdac74`; counted prior/candidate
  CSV SHA-256 `b5f437f5308fd08dcd75f19a646e47bef58c55cc799d5f1a515105bd29d41c1a` /
  `f567c318cfdd41c3fc336416a9c1b35634ec5485160a76ba4d2ba5384bb4a1bf`.
- **Validation.** Strict-RCH workspace check and clippy passed; parser allocation reuse and borrowed
  renderer parity tests passed. A production `render-batch --jobs 1` smoke reported one shared
  prefix parse reused and emitted byte-identical per-file hashes to two ordinary `render` calls.
  The full workspace suite reached the known stale
  `dense_flowchart_stress` golden mismatch after the changed crate/unit tests passed. Formatting,
  diff checks, exact-output identity, and live structural equivalence passed.
- **Retry predicate.** Re-run only if reusable builder reset semantics, the borrowed-IR lifetime
  boundary, prefix certification, shared-prefix corpus, worker ownership, executing ELF, or
  equivalence contract changes.

## KEEP: O(delta) reset for certified shared-prefix batch parsing (2026-08-01)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-znl1`. **Lane:** cod (`BlackThrush`).
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `df5abf5eb8446baa4855e198af7a584126403a54c2010c5e6df563161eb34949`
**A/A null control (same invocation):** all nine alternating prior/candidate processes ran their
own nine-pair Rust null control. The medians across those process-level null medians were
`0.996873` for the prior ELF and `1.000426` for the candidate; counted work, exact output identity,
and semantic equivalence were the acceptance evidence rather than short-run timing noise.
**Counted mechanism:** seven same-host `perf stat` repetitions over the exact 384-diagram,
64-worker whole job measured 258,015,731,624 instructions and 191,009,441,966 cycles for the prior
ELF, versus 241,781,535,518 instructions and 178,873,648,573 cycles for the candidate: **6.29% fewer
instructions** and **6.35% fewer cycles** (`1.0671x` and `1.0678x` ratios).

- **Mechanism.** A worker slot whose previous suffix preserved the exact compiled prefix now
  restores the builder by truncating appended suffix vectors and refreshing only lookup indexes
  and cold non-flowchart fields. A pointer-stable `Arc` identity prevents reuse across compiled
  prefix groups. Any suffix that mutates the prefix, or any group change, takes the existing full
  reset path. This changes repeated restore work from O(prefix + suffix) to O(suffix) without
  sharing mutable state across workers.
- **Whole-job result.** Nine alternating prior/candidate process pairs on `thinkstation1`, pinned
  to CPUs 0-63 and self-reporting 64/64 workers, measured prior median **966,737 ns** and candidate
  median **934,466 ns**: **1.034534x**. Both ELFs emitted aggregate SVG SHA-256
  `6410d31e4b9b9e96053fe237b7f45bc13eb50a80badb35dff06fa7d09f24a6ab`.
- **Live-incumbent corroboration.** The exact candidate ELF and pinned live mermaid-js 11.15.0 ran
  together over all 384 diagrams: the scalar Rust observation was **12.860 ms**, mermaid-js was
  **51,715.9 ms**, and structural verification passed **384/384** with zero divergent and zero
  unverified. This equivalence invocation has no incumbent null arm, so it supports no competitive
  ratio and this row remains maintenance-only. Artifact SHA-256
  `65277b0a405b04f2cf561d174aa7c4e67950d746f8728959d8690fb8abb4aacd`.
- **Evidence.** Interleaved prior/candidate JSONL SHA-256
  `a6cd5f78c0ba54ff0d41b61adc4b485fa92527376b85a36aa7011ca7d3cd1bbe` /
  `3199aca31bf37ab7e7984e478b8dc698e4f9b9d01e4a9e2d07d705c11f9f5096`; counted prior/candidate
  CSV SHA-256 `757f764aeeb48fc295b13ad7c0b1213868798c72d360b63191427651ff7d8d16` /
  `b69dc4c7b00bdd84861482a08b76505d2a006d5a293d2ce692d743c9c63db426`.
- **Validation.** Strict-RCH workspace check and clippy passed; all five shared-prefix batch parser
  tests passed, including exact fallback after a suffix mutates a cached node. The full workspace
  suite reached only the known stale `dense_flowchart_stress` golden mismatch after the changed
  parser and other preceding suites passed. Formatting, diff checks, exact-output identity, and
  live structural equivalence passed.
- **Retry predicate.** Re-run only if certified-prefix equality, prefix-group identity, builder
  index layout, shared-prefix corpus, worker ownership, executing ELF, or equivalence contract
  changes.

## KEEP: write-time certification for shared-prefix batch parsing (2026-08-01)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-b4vy`. **Lane:** cod (`BlackThrush`).
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `daf58f9bdc59d420aa2f347f5b811cbba98a263738bce3a725519143c0e0ce3a`
**A/A null control (same invocation):** all nine alternating prior/candidate processes ran their
own nine-pair Rust null control. The medians across those process-level null medians were
`1.002449` for the prior ELF and `1.000770` for the candidate; counted work, exact output identity,
and semantic equivalence were the acceptance evidence rather than short-run timing noise.
**Counted mechanism:** seven same-host `perf stat` repetitions over the exact 384-diagram,
64-worker whole job measured 241,871,471,531 instructions and 180,111,253,488 cycles for the prior
ELF, versus 228,500,216,746 instructions and 166,270,868,641 cycles for the candidate: **5.53% fewer
instructions** and **7.68% fewer cycles** (`1.0585x` and `1.0832x` ratios).

- **Mechanism.** The parser arms a compact guard when it begins a suffix parse and marks it at the
  existing flowchart mutation sites whenever that suffix changes a cached node, edge, cluster,
  subgraph, style, or diagram setting. An unchanged guard certifies the O(delta) builder restore
  without rereading every cached IR vector. Debug builds still execute the former full equality
  walk and assert that its answer matches the write-time certificate, preserving an exhaustive
  development oracle while removing the production `memcmp` and `starts_with` passes.
- **Whole-job result.** Nine alternating prior/candidate process pairs on `thinkstation1`, pinned
  to CPUs 0-63 and self-reporting 64/64 workers, measured prior median **931,500 ns** and candidate
  median **887,261 ns**: **1.049860x**. Both ELFs emitted aggregate SVG SHA-256
  `6410d31e4b9b9e96053fe237b7f45bc13eb50a80badb35dff06fa7d09f24a6ab`.
- **Live-incumbent corroboration.** The exact candidate ELF and pinned live mermaid-js 11.15.0 ran
  together over all 384 diagrams: the scalar Rust observation was **10.518 ms**, mermaid-js was
  **52,032.8 ms**, and structural verification passed **384/384** with zero divergent and zero
  unverified. This equivalence invocation has no incumbent null arm, so it supports no competitive
  ratio and this row remains maintenance-only. Artifact SHA-256
  `3da3a849c5ab1543ab60bcd7120e94857091457dce81406b6f0fd7e90254f761`.
- **Evidence.** Interleaved prior/candidate JSONL SHA-256
  `88db39094a548107a1037ac23a48203fc68db86a682626f60ea3626e7d12199c` /
  `29d059a8bc5e27f0103cc38058413d412e3cd6695310ac1a7e6dcca9b8eb7335`; counted prior/candidate
  CSV SHA-256 `56d3882914ca7653d3d932c2cf38f4075c59435297a4a19671fc670ec50bb8be` /
  `5367256f686a464d2e3f7b29da86ec75c99dc446941bb4890c561e99db19ea55`.
- **Validation.** Strict-RCH workspace check and clippy passed; focused shared-prefix tests cover
  direction, class, interaction, subgraph, and ordinary append paths while the full equality walk
  remains a debug oracle. The full workspace suite reached only the known stale
  `dense_flowchart_stress` FNV mismatch (`a8dd16e93853d93d` observed versus
  `3c237445531e5ff4` checked in) after the changed parser and preceding suites passed. Formatting,
  exact-output identity, counted work, and live structural equivalence passed.
- **Retry predicate.** Re-run only if flowchart lowering gains a new prefix mutation site, the
  reusable-prefix guard contract changes, shared-prefix corpus, worker ownership, executing ELF,
  or equivalence contract changes.

## KEEP: prefix-certified directed-path geometry transplant (2026-08-01)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-ctwu`. **Lane:** cod (`BlackThrush`).
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `012b25bdc3a92fc399275a818d4575d48c2a28ebcc76baac6f75e8d02e266863`
**A/A null control (same invocation):** all nine alternating prior/candidate processes ran their
own nine-pair Rust null control. The medians across those process-level null medians were
`1.024142` for the prior ELF and `1.004395` for the candidate; counted work, exact output identity,
and semantic equivalence were the acceptance evidence rather than short-run timing noise.
**Counted mechanism:** seven same-host `perf stat` repetitions over the exact 384-diagram,
64-worker whole job averaged 5,848,631,233 instructions and 4,981,331,389 cycles for the prior
ELF, versus 4,436,096,369 instructions and 4,180,004,734 cycles for the candidate: **24.15% fewer
instructions** and **16.09% fewer cycles** (`1.3184x` and `1.1917x` ratios).

- **Mechanism.** When the parser proves that a suffix preserved a closed shared prefix, the
  worker reuses the uniquely owned prior LR directed-path layout, truncates it to that prefix, and
  lays out only the appended nodes and edges. The certificate retains the exact tree-layout depth
  cursor so suffix coordinates preserve the incumbent floating-point operation order bit for bit;
  diagrams outside the validated specialization take the ordinary full-layout path without
  mutation.
- **Whole-job result.** Nine alternating prior/candidate process pairs on `thinkstation1`, pinned
  to CPUs 0-63 and self-reporting 64/64 workers, measured prior median **959,388 ns** and candidate
  median **803,433 ns**: **1.194111x** by ratio of medians and **1.230035x** by paired median. Both
  ELFs emitted aggregate SVG SHA-256
  `6410d31e4b9b9e96053fe237b7f45bc13eb50a80badb35dff06fa7d09f24a6ab`.
- **Live-incumbent corroboration.** The exact candidate ELF and live mermaid-js 11.15.0 ran in one
  whole-job bracket over all 384 diagrams, with 64/64 Rust workers and **384/384** structural
  equivalence. Its raw observations were 0.999934 ms Rust and 51,309.1 ms mermaid-js, but the
  Rust-before A/A median was `1.053229` (5.323% from one), so the corrected gate rejected the
  competitive ratio. This row remains maintenance-only and supports no competitive claim.
- **Evidence.** Live equivalence artifact SHA-256
  `9a80e416d1ec84c5cee507f3f530c96fca4f308a215d55a2229085a0415425ae`; same-invocation bracket
  summary/event SHA-256 `eb11165f9f63496ff0fa49c12360fe5353a6b21d4b6375ac5e3b0807ffdd9fbe` /
  `b0da47516b5ca3ee6ba7f6e366b25951804f73acae2b97c77dc5467687bd597a`.
- **Validation.** Strict-RCH workspace check and clippy passed; focused exact-layout and renderer
  tests cover a real floating-point associativity boundary, exact SVG reuse, and fallback without
  partial mutation. Formatting, counted work, exact-output identity, and live structural
  equivalence passed.
- **Retry predicate.** Re-run only if the parser prefix certificate, directed-path tree arithmetic,
  layout ownership, shared-prefix corpus, worker placement, executing ELF, or equivalence contract
  changes.

## KEEP: broadcast one certified cold-prefix renderer seed to batch workers (2026-08-01)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-v792`. **Lane:** cod (`BlackThrush`).
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `76dbeeb00d7c570e716546a336b60d855691093c35f7a7bdbf3f7e49c923a739`
**A/A null control (same invocation):** 21 order-alternated candidate/candidate whole-job pairs
measured 12.721 / 13.497 ms medians (ratio `0.942506`; paired-ratio median `1.036430`). This
failed timing admission, so neither the effect timing nor the live-incumbent observation below is
used as a competitive or wall-clock claim.
**Counted mechanism:** 11 same-host `perf stat` repetitions over the exact 384-diagram,
64-worker production `render-batch` job measured 1,074,824,383 instructions, 1,478,263,050 cycles,
and 361,844,073 ns task-clock with cold-prefix broadcast disabled, versus 593,015,372 instructions,
1,327,974,256 cycles, and 327,005,823 ns with it enabled: **44.83% fewer instructions, 10.17%
fewer cycles, and 9.63% less CPU time** (`1.8125x`, `1.1132x`, and `1.1065x` ratios).

- **Mechanism.** Production `render-batch` previously made every Rayon worker independently parse,
  lay out, and render the same certified 48-node/47-edge prefix before it could reuse suffix
  deltas. The coordinator now renders one real owner once, captures its immutable certified
  snapshot, and initializes independent workers from that seed. Layout sharing is copy-on-write;
  uncertified inputs and non-SVG formats retain the ordinary path, and the seeded owner's result
  stays in its original output slot.
- **Whole-job observation.** Twenty-one order-alternated disabled/enabled pairs measured 14.259 /
  13.999 ms medians (`1.018573x`; paired-ratio median `1.011697x`). Because the A/A null above was
  not admissible, this is provenance only; counted work is the KEEP criterion.
- **Output identity.** Enabled and disabled production jobs emitted byte-identical files and the
  same aggregate per-file manifest SHA-256
  `35cde3ee0e6010d72b24da2c70e6b9edee94444e703cdae91778aac3f6e3b02f` across all 384 inputs.
- **Live-incumbent corroboration.** In one invocation, the exact candidate ELF completed the
  384-diagram production job in 23.930 ms and pinned live mermaid-js 11.15.0 completed the same
  input corpus in 51,729.2 ms. Mermaid's runtime-verified bundle SHA-256 was
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`; the shared input SHA-256 was
  `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`. The invocation had no
  incumbent null arm, so this row remains maintenance-only and supports no competitive ratio.
- **Validation.** Strict-RCH workspace check passed; the exact seed test proves independent worker
  bootstrap against the ordinary full layout/render pipeline, and focused parser, renderer, and CLI
  suites pass. Formatting, counted work, executing-ELF self-report, and exact output identity pass.
- **Retry predicate.** Re-run only if parser prefix-group assignment, renderer certification,
  copy-on-write layout ownership, Rayon worker initialization, shared-prefix corpus, executing ELF,
  or equivalence contract changes.

## KEEP: persistent unchanged-output bypass for repository batch renders (2026-08-01)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-m86p`. **Lane:** cod (`BlackThrush`).
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `08384cf5bc0a10f173294ec8d394a3b6e8d0f35f550a4d152047ccd7ec39e93a`
**A/A null control (same invocation):** 31 order-rotated cache-A/cache-B whole-process arms,
pinned to CPUs 0-63, measured 7,819,208 / 7,608,618 ns medians (ratio `1.027678`); the
20,000-resample median-ratio 95% CI was `[0.979561, 1.056152]` and includes one.
**Counted mechanism:** the disabled arm performed 384 source reads, parses/layouts/renders, and
384 output writes; the admitted warm arm validated 384 source/output metadata pairs and performed
zero source reads, zero Rayon pool construction, zero parses/layouts/renders, and zero output
writes. Both arms reported 20,685,143 output bytes.

- **Mechanism.** `render-batch` now commits one hidden manifest after a successful materialization.
  Its entries bind source metadata and content digest to the resolved render configuration and
  executing-binary identity. An unchanged full batch is admitted before host-pressure sampling or
  Rayon construction; `--no-cache` preserves the old path as a same-binary control. Source,
  configuration, binary, length, or post-manifest output changes fail closed to the existing path.
- **Whole-job result.** Invocation `fm-cache-final-ab-1785585413747655828` measured 31
  order-rotated no-cache/cache processes at 21,198,822 / 7,819,208 ns medians: **2.711121x**, with
  bootstrap 95% CI `[2.526864, 2.888221]`. Enabled and disabled arms emitted byte-identical 384-file
  output; aggregate per-file manifest SHA-256 was
  `1d8fd3f3f368e593fdd2022ed467d3f46da1321b0e336e917898a4e1eac47703`.
- **Live-incumbent corroboration.** One same-invocation candidate/incumbent/candidate bracket used
  the exact ELF above and pinned mermaid-js 11.15.0 bundle
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de` on shared input SHA-256
  `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`. Candidate internal times
  were 2.581 / 3.203 ms with `--jobs 64` requested; full-hit admission created zero Rayon workers
  and ran on the main thread. The runtime-verified single-main-thread incumbent was 53,258.5 ms.
  Its render-once arm had no incumbent null, so this is not a competitive ratio claim.
- **Validation.** Theme/config changes forced 384/384 misses before the next identical run returned
  to 384/384 hits. Strict remote workspace check and clippy passed; three focused invalidation tests
  passed twice (both CLI binary targets); formatting and exact output identity passed.
- **Retry predicate.** Re-measure only if manifest admission, cache-key composition, destination
  freshness, executing-binary identity, the 384-input corpus, or output equivalence changes.

## KEEP: sparse-miss repository batch rebuilds (2026-08-01)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-oaub`. **Lane:** cod (`BlackThrush`).
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `96314cd7fc38814719b9b2c3449f6a7b69b67581bf6a6f6ee7d14155008723d8`
**A/A null control (same invocation):** invocation
`fm-sparse-final-ab-1785587229267268127` ran 61 order-rotated sparse-A/sparse-B
whole-process arms at 10,736,550 / 10,542,473 ns medians (ratio `1.018409`); the
20,000-resample median-ratio 95% CI was `[0.989411, 1.040747]` and includes one.
**Counted mechanism:** 11 same-host `perf stat` repetitions over the exact 384-input, one-file
content-change job measured 232,665,813 instructions, 498,717,871 cycles, and 133,287,649 ns
task-clock for the same binary's sparse path disabled, versus 42,104,396 instructions, 38,980,276
cycles, and 9,160,328 ns with sparse execution enabled: **81.90% fewer instructions, 92.18% fewer
cycles, and 93.13% less CPU time** (`5.5259x`, `12.7941x`, and `14.5505x` ratios).

- **Mechanism.** A metadata-certified manifest hit now carries its separately validated source
  digest into phase one without opening or hashing that source. Only early misses are read, and the
  Rayon pool is capped at the miss count rather than the requested whole-batch width. A one-file
  change in a 384-file repository therefore reads/renders one diagram with one active worker while
  preserving the former read-all, 64-worker route as an exact-binary control. Missing or internally
  inconsistent digest fields fail closed to the ordinary path.
- **Whole-job result.** The same 61-process invocation measured read-all control and sparse medians
  of 18,109,557 / 10,736,550 ns: **1.686720x**, with bootstrap 95% CI
  `[1.650088, 1.728217]` and paired-ratio median `1.721399x`. Both routes reported 64 requested
  workers; the control activated 64 and sparse execution activated one. They emitted byte-identical
  384-file output with aggregate per-file manifest SHA-256
  `7f5c3596ba29652cb0378ae4371cdf853961b6f0846b8df768a149f2d18d9b6a`.
- **Live-incumbent corroboration.** Invocation
  `fm-sparse-final-live-1785587290375861313` bracketed pinned live mermaid-js 11.15.0 with the exact
  candidate ELF above on shared input SHA-256
  `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`.
  Candidate internal observations were 5.588 / 4.703 ms with 64 workers requested and one active;
  the runtime-verified single-main-thread incumbent observed 50,861.2 ms. Its bundle SHA-256 was
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`. The render-once incumbent
  arm had no null control, so this row remains maintenance-only and supports no competitive ratio.
  Live artifact SHA-256 was
  `258a247d0a3e94dc9147cceb6776e97a201329699ad60a46882d6c1efee6a612`.
- **Validation.** Strict remote workspace clippy passed with warnings denied; all six focused cache
  tests passed across both CLI binary targets. Formatting, executing-ELF self-report, sparse/control
  exact-output identity, counted work, and pinned live-incumbent bracketing passed.
- **Retry predicate.** Re-measure only if source-digest integrity, early metadata admission,
  miss-count pool sizing, executing-binary identity, the 384-input corpus, or output equivalence
  changes.

## KEEP: hoist the fixed neighbour layer out of both crossing searches — `schema_catalog_25` −40.92% instructions (2026-08-01)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Commits:** `2a8e6c01` (e-graph candidate search) and `c18bc2e0` (refinement transpose + sifting).

**Campaign result class:** maintenance-self-speedup

- **Profile-first attribution.** Non-LTO (`lto=false strip=false debug=true`) symbolized build,
  `perf record -F 999 --call-graph=dwarf`, pinned core 3, 60 whole-job repetitions of
  `schema_catalog_25` (25 bounded-context ER schemas, the repo's **worst certified incumbent
  ratio**, 412.8x). Top self-time: `egraph_ordering::crossing_count` **16.29%**, `_mi_memset`
  **7.14%**, `count_inversions` 5.40%, mimalloc alloc/free ~8.9%, `__memmove_avx` 3.43%. Roughly
  41% of the whole job is crossing counting and the allocator traffic underneath it.
- **Why this is a gap and not a shared cost.** dagre counts crossings ~8 times per graph over a
  persistent structure. `optimize_layer_ordering` scores O(n) candidates per round for O(n) rounds
  and every score re-entered `crossing_count_dense`, which rebuilds **seven** vectors — two position
  maps over the node-id domain, bucket counts, offsets, a cursor, the grouped partner positions and
  the Fenwick tree. The incumbent does not pay this shape.
- **The ONE lever.** The neighbour layers do not move during that search, so their position map and
  the bucketing of edges by their positions are loop invariants. `FixedLayerCrossings` computes both
  once per search; each candidate score becomes a Fenwick sweep over a prebuilt CSR with **zero**
  allocation. Counts are identical, not approximate: two edges cross iff `(u1-u2)(l1-l2) < 0`, which
  is symmetric in the two layers, so bucketing by the fixed layer counts the same pairs.
- **The same lever, widened (`c18bc2e0`).** `crossing_refinement` pays the identical cost in the
  identical shape: transpose tries every adjacent swap in a rank and sifting tries every node at
  every position, each trial calling `pair_crossings` twice, and each of those rebuilt two
  domain-sized position arrays, re-sorted the pair's edges and ran a recursive merge sort that
  allocates a `Vec` at **every level of its recursion**. Both phases perturb one rank while its two
  neighbours stand still, so `RankNeighbourCrossings` buckets both adjacent pairs once per rank
  visit — O(n) reuses per rank in transpose, O(n^2) in sifting. `FixedLayerCrossings` moved to plain
  slices so one implementation serves both call sites.
- **Byte-identity, proven BEFORE timing.** 18 corpus jobs (flowchart small/medium/large, wide
  8x16/12x24/16x32, dense_dag_200, cyclic_scc_100, sequence_20, class_50, state_40, er_40,
  edit_trace_60x20, arch_100x50, er_schema_1000x6, monorepo_arch_120, schema_catalog_25,
  docs_site_50), comparing the SHA-256 of every job's concatenated per-document SVG stream:
  **0 mismatches**. A base-vs-base self-determinism control ran in the same sweep and was clean on
  every item, which matters here because this CLI's layout guardrail is a function of measured parse
  wall time.
- **Shared-checkout isolation.** A peer agent modified `crates/fm-cli/src/main.rs` at 12:29:36,
  before the candidate build, so the first A/B conflated two changes and was discarded. The baseline
  was rebuilt in a detached worktree as HEAD **plus the identical peer snapshot**
  (`256f45a8d5bac6984ecbd0765eb2001588f267f509c6ee497574cd82b3b8b615`); the peer's file in the shared
  checkout was only ever read. Both arms are frozen artifacts differing by exactly this change.
- **A/B + A/A null, arms alternated per round, instructions (`perf stat`), pinned core, `--jobs 1`.**
  Both ELFs self-report their own hash under `FM_SELF_REPORT_ELF_SHA256`, and both were verified to
  match the on-disk artifact that executed:

  **Executing ELF SHA-256 (self-reported by process):**
  `6daf8cb0a0ccec1baa3cb0852f9747e08a81748dd5575deeb1fa03168cd7ecbd`
  (end-to-end candidate at `c18bc2e0`; its baseline arm at session-start `ebc323c3` was
  `5e8b37a79557e78d0fb293960c559ae7d81428ce9ba3974e99c93c1a38969363`).

  **A/A null control (same invocation):** baseline binary against a byte-identical copy of itself,
  alternated with the A/B arms inside the same pinned sweep — median `0.999986`, range
  `0.999393`–`1.000532` for the end-to-end pair.

  **Counted mechanism:** seven per-candidate vector allocations plus two node-id-domain-sized fills
  removed from every crossing count in the e-graph search, and in the refinement a further two
  domain-sized fills, one sort and an O(edges) allocating merge-sort recursion removed from every
  transpose and sifting trial. Retained work on both paths is one Fenwick sweep over a CSR built
  once per search or per rank visit.

  | measurement | A/A null (median) | A/B (median) | verdict |
  |---|---:|---:|---|
  | `2a8e6c01` alone | 1.000094 (range 0.999175–1.000198) | **0.771958** | **−22.80%**, ~440x the null half-width |
  | `c18bc2e0` on top | 0.999953 (range 0.998564–1.000896) | **0.764909** | **−23.51%**, ~195x the null half-width |
  | **end-to-end** `ebc323c3` → `c18bc2e0` | 0.999986 (range 0.999393–1.000532) | **0.590760** | **−40.92%**, ~690x the null half-width; 1.709G → 1.010G instructions; every round outside the entire A/A range |
  | 9 negative controls | ~0.999–1.002 | 0.9997–1.0038 | inside their own A/A null; no regression anywhere |

  Wall clock corroborates in direction only. It is unusable for a verdict on this host: byte-identical
  code measured 26.0 ms and then 17.3 ms in two sweeps at load ~30–44, while instructions held A/A at
  0.999–1.001 across the same window.
- **Why instructions is the gate.** Pure work removal — no ISA, allocator or layout change — so
  instruction count measures the mechanism rather than proxying for it, and is load-immune.
- **Correctness.** `cargo clippy -p fm-layout --all-targets -- -D warnings` clean; 444 `fm-layout`
  tests and all `frankenmermaid-cli` suites green. A new differential test
  `fixed_layer_crossings_match_crossing_count` checks the hoisted counter against `crossing_count`
  over 400 pseudorandom layer pairs, from both bucketing directions, including edges naming nodes
  absent from their layer, plus reuse of one counter across successive permutations.
  `fnx_differential_report::differential_all_golden_cases_pass_gate` flakes roughly 1-in-6 at host
  load 30+ on `render_time_regression` — a wall-time threshold measured in-process, on a path
  neither commit touches. Not a functional gate.
- **What this does NOT claim.** No live mermaid-js arm ran in these invocations, so this is
  maintenance self-speedup and supports no competitive ratio. The 412.8x figure is the previously
  certified ratio that identified this job as the worst one to attack; re-certifying it needs a
  quiet host and a bracketed incumbent arm.
- **Verdict: KEEP.** No feature flag: byte-identical and monotonically less work.
- **Retry / re-check predicate.** Re-open if (1) the rewrite operators stop being strict
  permutations of the layer's node set — the build-time membership filter and the reuse of one
  position map across candidates both rest on that, (2) `should_use_egraph`'s 100-node cap moves,
  which changes which layers enter the e-graph search at all, or (3) `crossing_refinement` starts
  perturbing more than one rank between trials, which would invalidate rebuilding
  `RankNeighbourCrossings` only once per rank visit.
- **What the profile says next.** After both commits the ER-catalog profile is flat: no frame above
  `optimize_layer_ordering` at 10.19% self, and an arena rewrite aimed squarely at that frame
  returned only −0.96% and was reverted (see `docs/NEGATIVE_EVIDENCE.md`). The next lever on this
  job is not another allocation removal; it needs a different workload class or a change to how many
  candidates the search evaluates at all.

## KEEP: caller-certified change sets remove repository-wide cache validation (2026-08-01)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-nsgu`. **Lane:** cod (`BlackThrush`).
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `ebe425403369f7b133c238892135a49e819e9d2fe6bf2dae5ddf90c9dac8dd97`
**A/A null control (same invocation):** invocation
`fm-change-set-head-trj-1785604181108` ran 61 order-rotated candidate-A/candidate-B whole-process
arms on quiet 128-CPU host `threadripperje`. Medians were 8,175,961 / 8,197,402 ns (ratio
`0.997384`); the 20,000-resample median-ratio 95% CI was `[0.977605, 1.021870]` and includes one.
**Counted mechanism:** the exact-binary control made 1,166 `statx` calls and 1,375 total syscalls;
the change-set path made 15 `statx` calls and 206 total syscalls. Eleven order-rotated `perf stat`
repetitions measured control/candidate medians of 39,455,535 / 27,310,954 instructions,
35,808,777 / 21,751,483 cycles, and 9,196,841 / 5,608,366 ns task-clock: **30.78% fewer
instructions, 39.26% fewer cycles, and 39.02% less CPU time** (`1.4447x`, `1.6463x`, and
`1.6398x` ratios).

- **Profile-first attribution.** The exact 384-input, one-content-miss process profile exposed
  repository metadata validation as the structural gap: `statx` was 82.60% of traced syscall time
  and ran three times per otherwise cached input. The live incumbent's in-memory render API does
  not pay this repository-filesystem sweep; loader/startup work remained after the sweep vanished.
- **Mechanism.** `render-batch --trust-change-set --changed-input PATH...` lets a repository
  orchestrator assert its complete source/output change set. Unlisted manifest entries then need
  only the existing executable/options/source-digest integrity check, while listed inputs take the
  ordinary read/hash/render path. Duplicate or out-of-batch paths fail closed, ordinary callers
  retain source/output metadata validation, and an exact-binary control disables only this new
  admission path.
- **Whole-job result.** The same 61-round invocation measured metadata-sweep control and pooled
  candidate medians of 12,001,491 / 8,181,006 ns: **1.466995x**, with bootstrap 95% CI
  `[1.436472, 1.491593]` and paired-effect median `1.481725x`. Every arm rebuilt one true content
  change with one active worker and retained 383 outputs; candidate and control emitted identical
  384-file SVG bytes with aggregate SHA-256
  `3fadb42d114ff6b6c1b3d724cc3337884ac0a33048f0838ea6b15dfbe057880b`.
- **Live-incumbent corroboration.** Invocation
  `fm-change-set-head-live-trj-1785604271021` bracketed live pinned mermaid-js 11.15.0 with
  this exact ELF on 128-CPU `threadripperje`. Both engines received input SHA-256
  `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`; the candidate's external
  observations were 9.997 / 18.240 ms and the runtime-verified single-main-thread incumbent
  observed 50,562.5 ms. Its bundle SHA-256 was
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`. The incumbent arm was
  render-once with no incumbent null, so this row remains maintenance-only and supports no new
  competitive ratio.
- **Validation.** Strict clean-overlay remote workspace clippy passed with warnings denied; focused
  cache tests cover early metadata admission and exact key integrity. Formatting, exact-output
  identity, executing-ELF self-report, counted work, quiet-host A/A, and live-incumbent bracketing
  passed.
- **Retry predicate.** Re-measure if the caller change-set completeness contract, manifest key
  integrity, executable/options identity, destination ownership, 384-input corpus, or output
  equivalence changes. Promote no competitive claim without a same-invocation incumbent null arm.

## KEEP: one persistent renderer amortizes repeated edit-session startup (2026-08-01)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-0ga1`. **Lane:** cod (`BlackThrush`).
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `8596343e563ea2881cbd6155ec84a93b0a08196ab3349ad0e5d57b76bc7df046`
**A/A null control (same invocation):** invocation
`fm-persistent-stream-trj-1785606050620953831` ran 31 order-rotated whole-job rounds per arm on
128-CPU `threadripperje`; the two independent persistent arms measured 54,893,512 / 55,163,438 ns
(ratio `0.995107`), with a 20,000-resample median-ratio 95% CI of
`[0.970523, 1.034325]`, including one.
**Counted mechanism:** one 20-edit session executes the renderer once instead of 20 times. The
process acknowledges every completed epoch, and its ELF digest was emitted once from inside that
same long-lived process and matched the external digest above.

- **Profile-first attribution.** After caller-certified change sets removed the repository-wide
  metadata sweep, the remaining exact-process profile was dominated by `execve`, dynamic loading,
  allocator/runtime initialization, and command/config construction. The live incumbent keeps a
  JavaScript page alive and therefore does not repay those costs for every edit.
- **Mechanism.** `render-batch --trust-change-set --change-set-stdin` accepts one complete change
  set per newline-delimited JSON array, executes the existing fail-closed trusted-cache path, and
  flushes a JSON epoch acknowledgement without restarting. Blank lines are ignored; malformed
  JSON, duplicate paths, and paths outside the batch fail closed. Ordinary one-shot behavior is
  unchanged.
- **Whole-job result.** Each measured job alternated 20 real source revisions over the full
  384-diagram repository. Repeated exact-binary processes measured a 140,568,508 ns median;
  pooled persistent-process arms measured 54,918,524 ns: **2.559583x**, bootstrap 95% CI
  `[2.511483, 2.590984]`, with paired-effect median `2.547699x`. All three arms ended on the same
  source revision and emitted the identical 384-SVG aggregate SHA-256
  `a41d9aaafab660560f1d486dc0129357f72ee02a7f78038ab8ea5866829d3f20`.
- **Live-incumbent corroboration.** The same invocation bracketed a full uncached 384-diagram Rust
  render at 86.718 / 97.260 ms around live pinned mermaid-js 11.15.0 at 52,927.3 ms. Runtime
  provenance reported one browser main execution thread, Chrome 151.0.7922.71, input SHA-256
  `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`, and bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`. The incumbent arm was
  render-once with no incumbent null, so this row remains maintenance-only and asserts no
  competitive ratio.
- **Validation.** Strict clean-overlay remote check and Clippy passed with warnings denied; six
  focused cache/stream tests passed in each CLI binary. The timing host was admitted at load
  6.89/128 CPUs before the invocation. Stream protocol smoke, exact-output identity, executing-ELF
  self-report, quiet-host A/A, and the same-invocation live incumbent bracket passed.
- **Retry predicate.** Re-measure if the newline protocol, epoch acknowledgement boundary,
  change-set completeness contract, executable/options identity, revision count, corpus, or output
  equivalence changes. Promote no competitive claim without a same-invocation incumbent null arm.

## KEEP: retain the batch manifest across persistent edit epochs (2026-08-01)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-3gfw`. **Lane:** cod (`BlackThrush`).
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `22f2438c6b960cc504b4876cf8c75266d436630c44dbfc48bc8ffd3cc4cc92b7`
**A/A null control (same invocation):** invocation
`fm-memory-manifest-trj-1785606819181021462` ran 31 order-rotated whole-job rounds per arm on
128-CPU `threadripperje`; candidate A/B medians were 30,111,169 / 30,162,936 ns (ratio
`0.998284`), with a 20,000-resample median-ratio 95% CI of `[0.986108, 1.018195]`, including one.
**Counted mechanism:** same-binary 20-epoch `strace -f -c` arms reduced `openat` 206 -> 168,
`read` 432 -> 394, and `write` 520 -> 501: exactly 19 of the 20 repeated manifest reads and writes
disappeared. The conservative first-epoch repair scan increased `statx` 243 -> 974; the speedup is
therefore not a hidden reduction in repository validation.

- **Profile-first attribution.** Once process startup was amortized, the exact persistent-session
  profile still opened, parsed, cloned, serialized, and rewrote the complete 384-entry JSON
  manifest on every one-file edit. The live incumbent's in-memory page has no analogous disk-backed
  cache lifecycle.
- **Mechanism.** The change-set process leases one manifest into process-owned state, mutates only
  entries whose diagrams were reopened, and writes the full manifest once at graceful EOF. Its
  first epoch deliberately runs the ordinary metadata validation path, so a manifest left stale by
  a killed predecessor is repaired before trusted epochs begin. Output files are written before an
  epoch acknowledgement; the manifest remains an optimization rather than output authority.
- **Whole-job result.** Each arm used this exact ELF for the same 20 alternating revisions over the
  full 384-diagram repository. The environment-disabled control measured 53,519,767 ns; pooled
  in-memory candidates measured 30,159,079 ns: **1.774582x**, bootstrap 95% CI
  `[1.740151, 1.806523]`, with paired-effect median `1.752662x`. All three arms emitted the identical
  384-SVG aggregate SHA-256
  `a41d9aaafab660560f1d486dc0129357f72ee02a7f78038ab8ea5866829d3f20`.
- **Live-incumbent corroboration.** The same invocation bracketed a full uncached 384-diagram Rust
  render at 74.475 / 93.892 ms around live pinned mermaid-js 11.15.0 at 53,290.9 ms. Runtime
  provenance reported one browser main execution thread, Chrome 151.0.7922.71, input SHA-256
  `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`, and bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`. The incumbent arm was
  render-once with no incumbent null, so this row remains maintenance-only and asserts no
  competitive ratio.
- **Validation.** Strict clean-overlay remote check and Clippy passed with warnings denied; seven
  focused cache/stream tests passed in each CLI binary. The timing host was admitted at load
  8.26/128 CPUs. Mid-stream smoke proved the disk manifest stayed unchanged until EOF, then flushed;
  exact output, executing-ELF self-report, quiet-host A/A, counted syscalls, and the same-invocation
  live incumbent bracket passed.
- **Retry predicate.** Re-measure if manifest versioning, first-epoch repair, graceful-EOF flush,
  acknowledgement durability, change-set completeness, revision count, corpus, or output
  equivalence changes. Promote no competitive claim without a same-invocation incumbent null arm.

## KEEP: bucket adjacent-rank layer edges in one pass — `docs_site_50` −4.79%, `docs_site_200` −3.90% instructions (2026-08-01)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Commit:** `b84ddd72`.
**Campaign result class:** maintenance-self-speedup

- **Profile-first attribution.** Found by re-profiling a **different workload class** after the ER
  catalog went flat: `docs_site_200` (200 small docs in one launch), non-LTO symbolized build,
  `perf record -F 999 --call-graph=dwarf`, pinned core, 40 whole-job repetitions.
  `layer_edges_between_ranks` measured **6.86% inclusive / 1.94% self** — the largest identifiable
  frame on that job.
- **Why a second class was needed.** The frame is **absent** from `er_schema_1000x6`: those ranks
  exceed `should_use_egraph`'s 100-node cap, so `egraph_optimized_order_for_rank` early-returns
  before it ever scans edges. It only bills on narrow-rank graphs. This is
  `docs/LEDGER_RESURRECTION.md` §5 in its workload-class scope.
- **The ONE lever.** Each request for a rank pair's edges walked **all** of `ir.edges`, doing two
  `endpoint_node_index` resolutions and two `ranks` B-tree probes per edge plus a sort, then
  discarded everything outside the single pair it wanted. `apply_egraph_ordering_pass` runs two
  passes over every rank and asks twice per rank, so that is `4 * ranks` full scans —
  **O(ranks · edges)** to produce O(edges) of data. Ranks are fixed for the whole pass (only the
  within-rank ordering changes), so `layer_edges_by_rank_pair` buckets every adjacent pair in one
  pass and each rank indexes it. This mirrors `build_pair_node_edges`, which the refinement path
  already used — the bucketing existed, the e-graph pass just did not use it.
- **Byte-identity, proven BEFORE timing.** 19 corpus jobs, comparing the SHA-256 of each job's
  concatenated per-document SVG stream: **0 mismatches**, with a clean base-vs-base determinism
  control on every item. The filter is unchanged, each bucket is pushed in `ir.edges` order and
  sorted exactly as before, and a pair with no edges is simply absent from the map — which is what
  the old `is_empty` check produced.
- **A/B + A/A null, arms alternated per round, instructions (`perf stat`), pinned core, `--jobs 1`.**

  **Executing ELF SHA-256 (self-reported by process):**
  `6b2c6eda8a3648dfc5e7c30b075c097bba298bce27a0ec0bd1d65aa657b67f39`
  (candidate; baseline arm at `1a1074ae`
  `97a28d2a30bc40d521c8d5cf05487a16c3caad7fa322044807542f0c23d9c90f`).

  **A/A null control (same invocation):** baseline binary against a byte-identical copy of itself,
  alternated with the A/B arms in the same pinned sweep — `docs_site_200` median `1.000134`, range
  `0.999490`–`1.004454`.

  **Counted mechanism:** `4 * ranks` full scans of `ir.edges` (each with two endpoint resolutions
  and two B-tree rank probes per edge, plus a per-request sort) replaced by one bucketing pass and
  one sort per adjacent rank pair.

  | workload | A/A null (median) | A/B (median) | verdict |
  |---|---:|---:|---|
  | `docs_site_50` | 0.9998 | **0.9521** | **−4.79%** |
  | `docs_site_200` | 1.000134 (range 0.999490–1.004454) | **0.961038** | **−3.90%**, ~18x the null half-width; every one of 7 rounds outside it |
  | `schema_catalog_25` | 1.000064 (range 0.999460–1.000742) | **0.991137** | −0.89%, ~14x the null half-width |
  | 9 negative controls | ~0.9997–1.0014 | 0.9996–1.0013 | inside their own A/A null; no regression anywhere |

- **Correctness.** `cargo clippy -p fm-layout --all-targets -- -D warnings` clean; 444 `fm-layout`
  tests green. **`golden_svg_test::svg_golden_snapshots_are_stable` is RED on main and was already
  red before this work** — `dense_flowchart_stress` produces `a8dd16e93853d93d` against a checked-in
  `3c237445531e5ff4`. Bisected on clean detached checkouts of `ebc323c3` (session start), `2a8e6c01`,
  `c18bc2e0` and `4a6473c3`: identical failure and identical produced hash at every one, so the
  fixture is stale relative to main and this change does not move that output. Flagged, not adopted.
- **Verdict: KEEP.** Byte-identical, strictly less work, and it removes a code path rather than
  adding one — no parallel implementation to keep in lockstep.
- **Retry / re-check predicate.** Re-open if `apply_egraph_ordering_pass` ever mutates `ranks`
  (not just `ordering_by_rank`) between rank visits, which is the single invariant that lets the
  buckets be built once per pass.

## KEEP: retain immutable batch topology across persistent edit epochs (2026-08-01)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-wiyw`. **Lane:** cod (`BlackThrush`).
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `faf21e816c8356f8178d7f8a5ee4f41211345f680123553d043220ab50775643`
**A/A null control (same invocation):** invocation
`fm-plan-final-trj-1785608614716965338` used one shared output/cache directory and six complete
permutations of control/candidate-A/candidate-B, repeated for 36 whole-job rounds per arm on
128-CPU `threadripperje`. Candidate A/B medians were 24,398,292 / 24,243,767 ns (ratio
`1.006374`), with a 20,000-resample median-ratio 95% CI of `[0.985451, 1.026949]`, including one.
**Counted mechanism:** a 20-epoch control rebuilt the 384-input plan 20 times; the candidate built
it once. That deletes 19 full rebuilds: 7,296 owned input clones and input-set B-tree insertions,
7,296 basename/stem extractions and collision-map B-tree insertions, and 7,296 destination
name/path/display constructions. Exact-final-ELF `strace -f -c` also measured `statx` 974 -> 936,
`readlink` 20 -> 1, `mkdir` 20 -> 1, and total syscalls 2,427 -> 2,351.

- **Profile-first attribution.** After the process and manifest lifecycles became resident, the
  whole-job control still rebuilt input membership, basename uniqueness, output paths, worker
  count, executable identity, and the options digest for all 384 diagrams on every edit epoch.
  The live incumbent's single resident browser page does not pay an analogous per-edit batch-plan
  reconstruction, so this was structural gap work rather than a shared rendering cost.
- **Mechanism.** `BatchRenderPlan` validates and owns the immutable input/output topology once when
  the newline change-set session starts. Every epoch indexes its precomputed destination paths,
  cache entry names, display strings, worker count, executable identity, and options digest. The
  ordinary one-shot command still constructs the same plan locally, preserving its validation and
  error behavior. `FM_DISABLE_IN_MEMORY_BATCH_PLAN=1` is the exact-binary control arm.
- **Whole-job result.** Each round ran one process through the same 20 alternating one-file edits
  over the full 384-diagram repository. The environment-disabled control median was 27,564,810.5
  ns; resident-plan candidate median was 24,398,292 ns: **1.129784x**, bootstrap 95% CI
  `[1.107768, 1.155169]`, with paired-effect median `1.143695x`. The shared directory emitted the
  exact 384-SVG aggregate SHA-256
  `bd2b9194e377ff60a471ee546bd5cd0a03aeda57e544f1afb35780cd3afcc56c`.
- **Live-incumbent bracket.** The same invocation bracketed full uncached 384-diagram Rust renders
  at 58.352 / 46.727 ms around live pinned mermaid-js 11.15.0 at 52,813.7 ms. Runtime provenance
  reported one browser main execution thread, Chrome 151.0.7922.71, input SHA-256
  `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`, and bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`. The incumbent was a
  render-once arm without an incumbent A/A null, so this row remains maintenance-only and asserts
  no competitive ratio.
- **Validation.** Clean-overlay remote check and Clippy passed with warnings denied; seven focused
  cache/stream tests passed in each CLI binary; package formatting passed. The exact release binary
  was built in the dedicated repository-local target after `/data` reported 650 GiB free. The
  timing host was admitted at load 5.74/128 CPUs and ended at 6.48/128. Exact output, balanced A/A,
  executing-ELF self-report, counted work/syscalls, and the live incumbent bracket all passed.
- **Retry predicate.** Re-measure if session inputs, output naming, options identity, worker-count
  selection, executable identity, revision count, corpus, or output equivalence changes. Promote no
  competitive claim without a same-invocation incumbent null arm.

## KEEP: default `render-batch` to physical cores, not logical — default path −26.7% on `docs_site_200` (2026-08-01)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Commit:** `adc4f55c`.
**Campaign result class:** maintenance-self-speedup

- **How it was found.** Not from a frame. After three byte-identical layout wins the per-diagram
  profile went flat, so the next question was the axis the incumbent cannot follow at all: every
  measurement so far had been `--jobs 1`, but the product ships `--jobs` defaulted to
  `available_parallelism()`. Measuring the whole scaling curve on a **quiet host** (load 2.4, the
  first genuinely quiet window of the session) showed the batch peaks at the physical core count and
  then **regresses**:

  | `--jobs` | `ci_docs_2000` best-of-5 | speedup | efficiency |
  |---:|---:|---:|---:|
  | 1 | 394.6 ms | 1.00x | 100% |
  | 8 | 57.0 ms | 6.92x | 87% |
  | 16 | 31.9 ms | 12.36x | 77% |
  | **32** | **22.6 ms** | **17.47x** | 55% |
  | 64 | 27.0 ms | 14.63x | 23% |

  `docs_site_200` has the same shape: 7.86x at 32, falling to 5.66x at 64.
- **Why, and why it is not just starvation.** The box is an AMD Threadripper PRO 5975WX: 32 physical
  cores, 2 threads/core, 64 logical. `available_parallelism()` reports **logical** CPUs, so the
  default asked for two workers per physical core. Batch workers are compute-bound and
  cache-resident — each parses, lays out and renders an entire diagram — so a pair of SMT siblings
  shares one core's L1/L2 and execution units without adding execution resources. Starvation was
  ruled out by re-measuring at 10x the work per worker: `ci_docs_2000` gives each of 64 workers ~31
  documents (~6 ms of work) and **still** regresses. Amdahl alone does not explain it either — a
  2.7% serial fraction fitted at 32 predicts 23.8x at 64, and the measurement is 14.6x, so the
  excess is contention, not serial work.
- **The ONE lever.** Default to physical cores. `default_batch_workers()` counts distinct
  `topology/thread_siblings_list` values in sysfs — every logical CPU publishes the sibling set it
  shares a core with, so the number of distinct sets is the number of physical cores. One directory
  walk per batch. Unreadable or unexpected topology yields `None` and the caller keeps the previous
  `available_parallelism()` behaviour, which is also what non-Linux targets get; on a non-SMT machine
  physical == logical and the default is unchanged.
- **Byte-identity, proven BEFORE timing — and note WHAT was proven.** Worker count is output-neutral:
  identical SHA-256 of the concatenated per-document SVG stream at `--jobs` **1 / 8 / 16 / 32 / 64**
  (and 64 twice) across `docs_site_200`, `docs_site_50`, `schema_catalog_25`, `edit_trace_60x20` and
  `ci_docs_2000`. This mattered more than usual: this CLI's layout guardrail is a function of
  measured parse wall time, so a change that alters timing is exactly the kind that could move bytes.
  It does not, at these sizes. Default-vs-default output was then re-verified identical per job.
- **A/B on the DEFAULT path (no `--jobs`), arms alternated per round, quiet host.**

  **Executing ELF SHA-256 (self-reported by process):**
  `b6340019990e6aa4eb9164e889708348da747048407b1f488a7fea0c824329e3`
  (candidate; baseline arm `de669423c011f5462e6b098dd383258645fc5da212518c15fd41d5c80654a6d2`).
  The arms were confirmed to differ by exactly this change — 64 lines — by diffing the baseline
  worktree's `main.rs` against the working tree, because a peer committed to that same file twice
  during the build window.

  **A/A null control (same invocation):** baseline binary against a byte-identical copy of itself,
  alternated with the A/B arms in the same sweep.

  **Counted mechanism:** requested workers 64 -> 32 on this host (self-reported by the batch: the
  baseline prints `64 requested worker(s), 50 active worker(s)`, the candidate `32 requested, 32
  active`). Identical work, identical output, fewer threads contending for the same 32 cores.

  | workload | A/A null (median, range) | A/B (median, range) | verdict |
  |---|---|---|---|
  | `docs_site_200` | 1.0130 (0.8596–1.0789) | **0.7334** (0.691–0.840) | **−26.7%**; the A/B and A/A ranges are **disjoint** over 11 rounds |
  | `docs_site_50` | 0.9983 (0.935–1.081) | **0.7550** (0.659–0.938) | **−24.5%** |
  | `ci_docs_2000` | 0.9850 (0.933–1.044) | **0.9192** (0.837–0.948) | **−8.1%** |

- **Why wall is the gate here, exceptionally.** This changes scheduling, not work: instruction count
  is nearly unmoved by construction, so the usual instructions gate would measure nothing. Wall is
  the mechanism. It is only admissible because this ran in the session's one quiet window (load
  2.4–7 versus 30–44 earlier); the A/A null is reported per job precisely because wall nulls are
  wide, and the headline row's A/B range does not overlap its null.
- **Correctness.** `cargo clippy -p frankenmermaid-cli --all-targets -- -D warnings` clean.
  `golden_svg_test::svg_golden_snapshots_are_stable` remains the pre-existing red documented in the
  `b84ddd72` row (`dense_flowchart_stress`, red at `ebc323c3` before any of this work); no new
  failures.
- **Verdict: KEEP.** Output-neutral, and it makes the shipping default faster for every user who
  does not pass `--jobs`.
- **Retry / re-check predicate.** Re-open if (1) batch workers stop being compute-bound — a future
  batch that blocks on I/O per document would want more threads than cores, not fewer, or (2) the
  peak moves off the physical core count on some machine class, which this rule assumes. The
  scaling curve above is the measurement to repeat; `--jobs` remains the escape hatch either way.

## KEEP: execute persistent edit epochs over changed diagrams only (2026-08-02)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-3qp4`. **Lane:** cod (`BlackThrush`).
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `d90f1b2dc1c12d53563db0916de43230d45a7fffc31a0d48c29cd0085e506cd9`
**A/A null control (same invocation):** invocation
`fm-sparse-exact-trj-1785646437330341733` used one shared output/cache directory and all six
permutations of control/candidate-A/candidate-B, repeated for 36 whole-job rounds per arm on
128-CPU `threadripperje`. Candidate A/B medians were 22,157,837 / 22,086,884.5 ns (ratio
`1.003212`), with a 20,000-resample median-ratio 95% CI of `[0.991109, 1.013402]`, including one.
**Counted mechanism:** after the mandatory first recovery epoch, each of the remaining 19 epochs
reduced its execution cardinality from 384 diagrams to the one caller-certified changed diagram.
That deletes 7,277 unchanged entries from every batch-wide cache, digest, ownership, parse-plan,
render-dispatch, manifest-update, and reporting pass. Exact-ELF `strace -f -c` showed this was not a
hidden syscall win: the tiny subset plan increased `readlink` and `mkdir` from 1 to 20 and `statx`
from 936 to 974, while the candidate still won.

- **Profile-first attribution.** The retained topology had removed plan construction, but trusted
  one-file edits still allocated and walked full 384-entry vectors and maps through the entire
  batch pipeline. Live mermaid-js keeps one page resident and has no analogous batch-wide rescan;
  actual parse/layout/render work for the changed diagram is shared work and was not targeted.
- **Mechanism.** A successful full epoch records a process-local plan certificate plus aggregate
  output bytes. Later complete change sets index their diagrams in the immutable plan, lease the
  same full manifest, and execute `cmd_render_batch` over only that sparse slice. Whole-batch
  rendered/hit/byte accounting is carried forward in O(changes). JSON mode retains its existing
  one-record-per-input path; ordinary one-shot and crash-recovery behavior are unchanged.
- **Whole-job result.** Every round ran one process through 20 alternating one-file revisions over
  the complete 384-diagram repository. The exact-binary disabled control median was 23,721,542 ns;
  the candidate median was 22,157,837 ns: **1.070571x**, bootstrap 95% CI
  `[1.060293, 1.081687]`. Control and candidate emitted the identical 384-SVG aggregate SHA-256
  `8c9897eb7fda549d70233a1f912782c53cb4d99b177afbea1e8a3ad9116af5c8`.
- **Live-incumbent bracket.** The same invocation bracketed full uncached 384-diagram Rust renders
  at 35.564 / 73.899 ms around pinned mermaid-js 11.15.0 at 51,103.6 ms render time. Runtime
  provenance reported one browser main thread, Chrome 151.0.7922.71, input SHA-256
  `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`, and bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`. The incumbent was a
  render-once arm without an incumbent A/A null, so this row asserts no competitive ratio.
- **Validation.** Strict clean-overlay remote check and Clippy passed with warnings denied; eight
  focused cache/stream tests passed. The exact release ELF was rebuilt with one pinned nightly in
  the dedicated per-repository trj target after removing 655.2 MiB of mixed-rustc artifacts. The
  host stayed quiet at load 14.27 -> 14.80 over 128 CPUs. Exact output, balanced A/A,
  executing-ELF self-report, counted work, and the live-incumbent bracket all passed.
- **Retry predicate.** Re-measure if the complete-change-set contract, persistent session plan,
  manifest ownership, per-input JSON contract, revision count, corpus, or output equivalence
  changes. Promote no competitive claim without a same-invocation incumbent null arm.

## KEEP: resume clean edit streams without a repository recovery scan (2026-08-02)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-riso`. **Lane:** cod (`BlackThrush`).
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `aa1c68f177a8c03e1e63ef745746c477628c08d68c498bbd56bcab2691142ee7`
**A/A null control (same invocation):** invocation
`fm-durable-exact-trj-1785647726112613212` interleaved all six permutations of the disabled
control/candidate-A/candidate-B arms for 36 whole-job rounds per arm on 128-CPU
`threadripperje`. Candidate A/B medians were 21,113,811 / 21,092,787 ns (ratio `1.000997`),
with a 20,000-resample median-ratio 95% CI of `[0.988092, 1.014461]`, including one.
**Counted mechanism:** a clean-shutdown certificate reduced exact-ELF first-epoch `statx` calls
from 975 to 209 and total syscalls from 2,427 to 1,667. Before output can mutate, admission consumes
the certificate and durably rewrites the manifest; a graceful EOF restores it, while a killed
process leaves it absent and therefore forces the existing full recovery scan.

- **Profile-first attribution.** After changed-only epochs landed, the first epoch of every new
  persistent process still revalidated all 384 input/output pairs. The live mermaid-js arm keeps
  its page resident and pays no repository recovery scan, making this a frankenmermaid-only cost.
- **Mechanism.** The manifest may carry a versioned summary of a previously successful full batch.
  Admission requires an exact plan key, option digest, aggregate output length, and complete
  in-memory manifest topology match. The process atomically consumes that proof before rendering,
  then restores it only after a successful stream flush. `FM_DISABLE_DURABLE_BATCH_CERTIFICATE=1`
  selects the exact-binary control path.
- **Whole-job result.** Every arm started a fresh process and executed 20 alternating one-file
  revisions over the 384-diagram repository. The disabled-control median was 23,612,112.5 ns and
  the candidate median was 21,113,811 ns: **1.118325x**, bootstrap 95% CI
  `[1.104955, 1.129697]`. Both arms emitted the identical 384-SVG aggregate SHA-256
  `8c9897eb7fda549d70233a1f912782c53cb4d99b177afbea1e8a3ad9116af5c8`.
- **Crash behavior.** The exact invocation killed a candidate after certificate admission and
  observed `crash_certificate_absent=true`; the next process performed recovery and restored the
  certificate (`recovery_certificate_restored=true`). This is the safety invariant that permits
  skipping the clean restart scan.
- **Live-incumbent bracket.** The same invocation bracketed full uncached 384-diagram Rust renders
  at 71.527 / 38.156 ms around pinned mermaid-js 11.15.0 at 51,238.1 ms render time (55.716 s live
  wall). Runtime provenance reported one browser main thread, Chrome 151.0.7922.71, input SHA-256
  `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`, and bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`. The incumbent was a
  render-once arm without an incumbent A/A null, so this row asserts no competitive ratio.
- **Validation.** Strict clean-overlay remote check and Clippy passed with warnings denied; nine
  focused cache/stream tests passed, including certificate invalidation and crash recovery. Load
  stayed 18.57 -> 17.29 over 128 CPUs. Exact output, balanced A/A, executing-ELF self-report,
  counted syscalls, and the live-incumbent bracket all passed.
- **Retry predicate.** Re-measure if manifest durability, clean-shutdown semantics, plan identity,
  output ownership, stream failure handling, corpus, or revision count changes. Promote no
  competitive claim without a same-invocation incumbent null arm.

## KEEP: project the resident plan into sparse edit epochs (2026-08-02)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-byke`. **Lane:** cod (`BlackThrush`).
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `7ebd5796463b03a41f6d18dbaee290810ca089cac70d6de4a3f6437ab1bbb0a9`
**A/A null control (same invocation):** invocation
`fm-projection-exact-trj-1785649010456680300` interleaved all six permutations of the disabled
control/candidate-A/candidate-B arms for 36 whole-job rounds per arm on 128-CPU
`threadripperje`. Candidate A/B medians were 27,714,123.5 / 27,775,123 ns (ratio `0.997804`),
with a 20,000-resample median-ratio 95% CI of `[0.983507, 1.015058]`, including one.
**Counted mechanism:** projecting changed inputs from the immutable parent plan reduced exact-ELF
`readlink` calls from 22 to 2, `mkdir` from 21 to 1, `statx` from 343 to 303, and total syscalls
from 2,345 to 2,265 over the 20-epoch job.

- **Profile-first attribution.** Changed-only execution still recursively called
  `BatchRenderPlan::new` once per edit. That repeated output-directory creation, executable
  metadata, worker-topology discovery, path derivation, option hashing, and plan-key hashing even
  though the persistent process already owned their immutable results. The incumbent's resident
  page has no analogous per-edit plan reconstruction.
- **Mechanism.** `BatchRenderPlan::project` maps the changed paths through the resident input index
  and clones only their destination triples while retaining the parent worker count, cache path,
  option digest, and identity. `FM_DISABLE_EPOCH_PLAN_PROJECTION=1` selects the former constructor
  path in the same executable.
- **Whole-job result.** Every arm started one process and executed 20 alternating one-file edits
  over the 384-diagram repository. The disabled-control median was 28,381,855 ns and candidate
  median 27,714,123.5 ns: **1.024094x**, bootstrap 95% CI `[1.006503, 1.055370]`. All three arms
  emitted identical 384-SVG aggregate SHA-256
  `a8bbb4c00c012b846fab31463c2050ffbceb062374c5119b9810214ed40dc452`.
- **Live-incumbent bracket.** The same invocation bracketed full uncached 384-diagram Rust renders
  at 61.466 / 69.735 ms around pinned mermaid-js 11.15.0 at 52,553.6 ms render time (57.775 s live
  wall). Runtime provenance reported one browser main thread, Chrome 151.0.7922.71, input SHA-256
  `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`, and bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`. The incumbent used the
  render-once arm with no incumbent null, so this row asserts no competitive ratio.
- **Validation.** Strict remote focused tests passed 10/10; strict remote Clippy passed with
  warnings denied; targeted rustfmt passed. Load stayed 15.53 -> 16.11 over 128 CPUs. The release
  build used the repository-local target contract (`env -u CARGO_TARGET_DIR`) and the transferred
  executable's external and self-reported hashes agreed.
- **Retry predicate.** Re-measure if plan fields, destination mapping, sparse-recursion admission,
  corpus, revision count, output equivalence, or default worker discovery changes. Promote no
  competitive claim without a same-invocation incumbent null arm.

## KEEP: sample host pressure once per persistent edit stream (2026-08-02)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-369j`. **Lane:** cod (`BlackThrush`).
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `6402f257587890fb261f7b31200d6b8a1e0847df9fe89a752d04e060323c9b70`
**A/A null control (same invocation):** invocation
`fm-pressure-exact-trj-1785651422350986226` interleaved all six permutations of the disabled
control/candidate-A/candidate-B arms for 36 whole-job rounds per arm on 128-CPU
`threadripperje`. Candidate A/B medians were 25,437,457.5 / 25,065,029.5 ns (ratio `1.014858`),
with a 50,000-resample median-ratio 95% CI of `[0.998427, 1.032389]`, including one.
**Counted mechanism:** retaining one immutable pressure report for the process lifetime reduced
exact-ELF `read` calls from 396 to 92, `openat` from 170 to 56, `statx` from 170 to 56, and total
syscalls from 1,604 to 863 over the 20-epoch job.

- **Profile-first attribution.** The live incumbent keeps one browser page resident and does not
  walk Linux procfs/cgroup topology per edit. Frankenmermaid still reopened and reread
  `/proc/self/status`, `/proc/<pid>/cgroup`, and three `cpu.max` ancestry paths on every epoch even
  though those values only tune a process-local worker budget and cannot affect output bytes.
- **Mechanism.** `BatchRenderCacheSession` now initializes one `MermaidPressureReport` lazily and
  shares it across every render epoch in that persistent stream. One-shot batches retain their
  existing single sample. `FM_DISABLE_SESSION_PRESSURE_SNAPSHOT=1` selects the former per-epoch
  sampling path in the exact same executable.
- **Whole-job result.** Every arm started one process and executed 20 alternating one-file source
  revisions over the 384-diagram repository. The disabled-control median was 26,915,497 ns and
  candidate median 25,437,457.5 ns: **1.058105x**, bootstrap 95% CI
  `[1.040355, 1.066264]`. All three arms emitted identical 384-SVG aggregate SHA-256
  `a8bbb4c00c012b846fab31463c2050ffbceb062374c5119b9810214ed40dc452`.
- **Live-incumbent bracket.** The same invocation bracketed full uncached 384-diagram Rust renders
  at 66.418 / 43.740 ms around pinned mermaid-js 11.15.0 at 48,675.5 ms render time (53.894 s live
  wall). Runtime provenance reported one browser main thread, Chrome 151.0.7922.71, bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`, and live output SHA-256
  `332955ff46de11d3292f578ecb5f6a0c022d0706d39af621d0afd7782ea6f6cf`. The incumbent used a
  render-once arm without an incumbent null, so this row asserts no competitive ratio.
- **Validation.** Strict clean-overlay remote focused tests passed 11/11; strict remote Clippy
  passed with warnings denied; targeted rustfmt passed. Load stayed 21.95 -> 19.84 over 128 CPUs,
  requested and active worker provenance reported 64, and the transferred executable's external
  and self-reported hashes agreed.
- **Retry predicate.** Re-measure if pressure signals begin affecting output semantics, persistent
  streams commonly outlive meaningful host-pressure changes, pressure-source topology, corpus,
  revision count, or stream protocol changes. Promote no competitive claim without a
  same-invocation incumbent null arm.

## KEEP: replay the two most recent rendered revisions in persistent edit streams (2026-08-02)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-ucpx`. **Lane:** cod.
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `0ff8a1f2e8663388edab3c5be482dcc9764b1fff6f60fa58cf2aa30f209eccbc`
**A/A null control (same invocation):** invocation
`fm-revision-exact-thinkstation1-1785654560627396914` interleaved all six permutations of the
disabled control/candidate-A/candidate-B arms for 36 whole-job rounds per arm. Candidate A/B
medians were 20,600,710.5 / 20,602,584 ns (ratio `0.999909`), with a 50,000-resample median-ratio
95% CI of `[0.990865, 1.006885]`, including one.
**Counted mechanism:** each candidate arm replayed exact resident SVG bytes for 648/720 epochs
(18 of every 20 alternating edits), versus 0/720 in the disabled control. Only the first encounter
of each of the two revisions parsed, laid out, and rendered; every subsequent A/B oscillation was
an exact digest+options+executable-key hit.

- **Structural gap.** After batch-wide scans and per-edit configuration work were removed, the
  remaining changed-diagram path still repeated parse, layout, and SVG materialization whenever an
  editor returned to a recent revision. Mermaid-js also rerenders such a revision; a resident Rust
  stream can retain the already-produced immutable bytes and delete all three stages.
- **Mechanism.** `BatchRenderCacheSession` owns a two-entry process-local rendered-revision cache.
  Admission requires the exact source digest plus render-options and executing-binary identity; a
  hit rewrites the current destination and then enters the ordinary manifest commit path. The
  bound caps retained output memory, one-shot and crash-recovery paths are unchanged, and
  `FM_DISABLE_SESSION_REVISION_CACHE=1` selects the exact-binary control.
- **Whole-job result.** Each round started one process and executed 20 alternating one-file edits
  over the complete 384-diagram repository. The disabled-control median was 24,084,332.5 ns and
  the candidate median was 20,600,710.5 ns: **1.169102x**, bootstrap 95% CI
  `[1.162335, 1.176859]`. All arms emitted the identical final 384-SVG aggregate SHA-256
  `bd2b9194e377ff60a471ee546bd5cd0a03aeda57e544f1afb35780cd3afcc56c`.
- **Host admission.** The invocation used both SMT threads of isolated physical cores 24-31 on
  x86-64 `thinkstation1` (`24-31,56-63`). Two consecutive pre-run one-second samples had no CPU
  above 2% busy; the post-run sample had none above 5.1%. The exact process reported 16 requested
  workers and one active worker on every sparse epoch.
- **Live-incumbent bracket.** The same invocation bracketed full uncached 384-diagram Rust renders
  at 26.451 / 26.273 ms around pinned mermaid-js 11.15.0 at 50,608.2 ms render time (55.675 s live
  wall). Runtime provenance reported one browser main thread, Chrome 150.0.7871.128, input SHA-256
  `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`, and bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`. The incumbent was a
  render-once arm without an incumbent null, so this row asserts no competitive ratio.
- **Validation.** A strict clean-overlay remote cache/stream suite passed 12/12. Workspace-wide
  check and Clippy with warnings denied passed on strict remote clean overlays; targeted rustfmt,
  staged ledger lint, and UBS completed before landing. The external and self-reported ELF hashes
  agreed, and the exact disabled arm lived in that same executable.
- **Retry predicate.** Re-measure if the persistent-stream lifetime, source/options/executable key,
  output-write or manifest-commit semantics, cache capacity, corpus, or revision pattern changes.
  Promote no competitive claim without a same-invocation incumbent null arm.

## KEEP: retain exact rendered revisions across multi-diagram churn (2026-08-02)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-b5f4`. **Lane:** cod.
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `4b0919e95556db4d9708b74e9e5f329f13cc4b83b8752019ae13270e2858bf5d`
**A/A null control (same invocation):** invocation
`fm-revision-lru-exact-thinkstation1-1785656752258587833` interleaved all six permutations of the
two-entry control/candidate-A/candidate-B arms for 36 whole-job rounds per arm. Candidate A/B
medians were 1,854,139,507.5 / 1,936,262,425.5 ns (ratio `0.957587`), with a 50,000-resample
median-ratio 95% CI of `[0.804007, 1.151838]`, including one.
**Counted mechanism:** each candidate arm replayed exact resident SVG bytes for 21,888/23,040
epochs (608/640 in every round), versus 0/23,040 in the two-entry control. Each candidate parsed,
laid out, and rendered only the first 32 distinct diagram/revision pairs; the control recomputed all
640 epochs because its two global slots churned across the 16-diagram revision working set.

- **Structural gap.** The proven two-revision cache collapsed one alternating diagram, but a user
  switching among several diagrams evicted every revision before reuse. A resident Rust process can
  retain immutable SVG revisions across that working set; mermaid-js still parses, lays out, and
  renders every revision submitted to its page.
- **Mechanism.** `BatchRenderCacheSession` now owns a content-addressed LRU capped at 256 entries
  and 32 MiB. Exact hits promote recency; insertions replace duplicate keys and evict least-recently
  used bytes until both ceilings hold; oversize SVGs bypass retention. The same executable's
  `FM_SESSION_REVISION_CACHE_TWO_ENTRY_CONTROL=1` arm selects the former two-entry capacity.
- **Whole-job result.** Each round started one process and executed 640 edits over 16 large
  diagrams, alternating two revisions per diagram. The two-entry-control median was
  2,163,305,127.5 ns and candidate-A median 1,854,139,507.5 ns: **1.166743x**, bootstrap 95% CI
  `[1.048630, 1.389927]`. All arms emitted 384 files / 33,258,645 bytes with identical aggregate
  SHA-256 `6f6a4332172924cdb095e669b810f6d8851152e7f5e85795f1ef9742639755a2`.
- **Host admission.** The invocation used both SMT threads of physical cores 24-27 on x86-64
  `thinkstation1` (`24-27,56-59`). Two consecutive pre-run one-second samples peaked at 7.0% and
  3.0% busy; the post-run sample peaked at 5.0%. Sparse epochs requested eight workers and used
  one or eight according to the admitted work.
- **Live-incumbent bracket.** The same invocation bracketed full uncached 384-diagram Rust renders
  at 27.040 / 27.583 ms around pinned mermaid-js 11.15.0 at 50,638.6 ms render time (55.723 s live
  wall). Runtime provenance reported one browser main thread, Chrome 150.0.7871.128, input SHA-256
  `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`, and bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`. The incumbent was a
  render-once arm without an incumbent null, so this row asserts no competitive ratio.
- **Validation.** A strict clean-overlay remote cache/stream suite passed 13/13 and strict remote
  CLI Clippy passed with warnings denied before measurement. Workspace-wide clean-overlay check and
  Clippy, targeted rustfmt, staged ledger lint, and UBS completed before landing. External and
  self-reported executable hashes agreed.
- **Retry predicate.** Re-measure if the persistent-stream lifetime, exact revision key, entry or
  byte ceiling, SVG size distribution, working-set width, output materialization, corpus, or
  revision pattern changes. Promote no competitive claim without a same-invocation incumbent null
  arm.

## KEEP: materialize only the final revision of a change-set stream (2026-08-02)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-go2d`. **Lane:** cod.
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `f927f269ea2bd4e80cafcfef71d03ddfc99c49add2417e6e321450c8d7ee9bd0`
**A/A null control (same invocation):** invocation
`fm-final-output-exact-thinkstation1-1785658549007374011` interleaved all six permutations of the
ordinary-output control/candidate-A/candidate-B arms for 36 whole-job rounds per arm. Candidate
A/B medians were 415,215,955.5 / 418,270,456.5 ns (ratio `0.992697`), with a 50,000-resample
median-ratio 95% CI of `[0.946943, 1.025234]`, including one.
**Counted mechanism:** every control round materialized 640 transient SVG revisions / 1,215,964,480
bytes. Each `--final-output-only` candidate instead replaced resident bytes by destination and
materialized eight final SVGs / 15,195,992 bytes at EOF: 80x fewer bytes and 632 fewer output-file
writes per job. Across 36 rounds, that is 23,040 control writes / 43,774,721,280 bytes versus 288
writes / 547,055,712 bytes in each candidate arm.

- **Structural gap.** After exact revision replay removed repeated parse/layout/render work, the
  stream still copied every transient SVG to the filesystem even when a build system consumed only
  the completed output tree. Mermaid-js returns strings to its resident page and has no analogous
  mandatory per-revision filesystem materialization. The final-state contract permits deleting
  those writes rather than making them incrementally faster.
- **Mechanism.** Explicit `render-batch --change-set-stdin --final-output-only` mode holds one newest
  immutable `Arc<Vec<u8>>` per changed destination. Every epoch is still processed and acknowledged;
  EOF writes the deterministic final destination set before the durable clean certificate is
  committed. A write failure leaves that certificate invalid, while the default stream continues
  to materialize every acknowledged epoch for callers that consume intermediate output.
- **Whole-job result.** Each round started one process and executed 640 acknowledged edits over
  eight 900-node diagrams alternating two exact revisions. The ordinary-output-control median was
  5,115,566,767.5 ns and candidate-A median 415,215,955.5 ns: **12.320256x**, bootstrap 95% CI
  `[11.836834, 12.843769]`. Every arm ended with identical 384-file / 41,738,645-byte output,
  aggregate SHA-256 `bbfdcca3e38e367b01d63c0016b806c11040ca7b1f8041e076d65054ac27b806`.
- **Host admission.** The A/B phase used both SMT threads of physical cores 25, 27, 29, and 31 on
  x86-64 `thinkstation1` (`25,27,29,31,57,59,61,63`). Its two consecutive pre-phase one-second
  samples peaked at 9.4% and 19.6% busy with no CPU above the fixed 20% ceiling. The pre-incumbent
  samples peaked at 6.0% and 5.1%. A report-only post-invocation sample peaked at 22% on CPU 61.
- **Live-incumbent bracket.** The same invocation bracketed full uncached 384-diagram Rust renders
  at 27.542 / 31.340 ms around pinned mermaid-js 11.15.0 at 50,275.1 ms render time (55.387 s live
  wall). Runtime provenance reported one browser main thread, Chrome 150.0.7871.128, input SHA-256
  `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`, and bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`. The incumbent was a
  render-once arm without an incumbent null, so this row asserts no competitive ratio.
- **Validation.** Strict clean-overlay remote cache/stream tests passed 14/14 in both CLI binary
  targets and package Clippy passed with warnings denied before measurement. Workspace-wide
  clean-overlay check and Clippy, targeted rustfmt, staged ledger lint, and UBS completed before
  landing. External and self-reported executable hashes agreed in every arm.
- **Retry predicate.** Re-measure if callers require intermediate output, EOF transaction or crash
  semantics change, the final-tree memory footprint, output size, edit count, changed-diagram count,
  revision replay admission, corpus, or filesystem changes. Promote no competitive claim without a
  same-invocation incumbent null arm.

## KEEP: submit only the coalesced final source state (2026-08-02)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-pvdh`. **Lane:** cod.
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `e65133efc86488a911f2e024da2c9d89714e90304c69a91a9812ab661182a9a4`
**A/A null control (same invocation):** invocation
`fm-final-state-exact-thinkstation1-1785660401386884837` interleaved all six permutations of the
final-output-only control/candidate-A/candidate-B arms for 36 whole-job rounds per arm. Candidate
A/B medians were 53,506,012.5 / 55,082,735.5 ns (ratio `0.971375`), with a 50,000-resample
median-ratio 95% CI of `[0.941986, 1.009333]`, including one.
**Counted mechanism:** every control round performed 632 source-file writes / 71,568,856 bytes and
632 JSON request/ack epochs before materializing eight final SVGs. Each candidate sent one bounded
JSON transaction, wrote eight final sources / 906,160 decoded bytes, and rendered those eight
surviving revisions concurrently. Across 36 rounds that is 22,752 source writes and acknowledgments
for the control versus 288 source writes and 36 transactions in each candidate arm.

- **Structural gap.** Once final-only materialization deleted transient SVG writes, the retained
  stream still forced an in-memory editor to rewrite a source file and wait for an acknowledgment
  for every unobservable intermediate edit. Mermaid-js accepts source strings directly and pays no
  analogous source-file protocol. A final-state contract can delete all transient edit epochs and
  expose the surviving independent diagrams to the existing shared-nothing worker pool at once.
- **Mechanism.** Explicit `render-batch --trust-change-set --final-state-stdin` accepts one JSON
  object from known batch input paths to final UTF-8 bodies. It bounds aggregate encoded input,
  enforces the ordinary per-source limit, validates the complete batch plan before mutation, writes
  each listed source once, and invokes one cached parallel batch with the complete changed set.
  The option conflicts with ordinary change-set streaming, explicit changed paths, and no-cache so
  callers cannot accidentally combine incompatible consistency contracts.
- **Whole-job result.** Every timed round began from an untimed revision-A certificate over the
  same 384-input output tree. The retained control then executed 79 alternating sweeps / 632
  acknowledged edits and deferred output until EOF; the candidate encoded and submitted the same
  eight final revision-B bodies once. Control median was 327,319,763 ns and candidate-A median was
  53,506,012.5 ns: **6.117439x**, bootstrap 95% CI `[5.941557, 6.241070]`. All arms ended with
  identical 384-file / 41,745,773-byte output, aggregate SHA-256
  `3a51859f47cbe17f4652a5efdcd72dcba648694b8c624d4659f94c10d745c109`.
- **Host admission.** The A/B phase used both SMT threads of physical cores 25, 27, 29, and 31 on
  x86-64 `thinkstation1` (`25,27,29,31,57,59,61,63`). The harness waited through 27 busy
  samples, then admitted two consecutive one-second samples peaking at 3.0% and 1.0%. The
  pre-incumbent samples peaked at 4.0% / 4.0%, and the report-only post-invocation sample at 8.0%.
- **Live-incumbent bracket.** The same invocation bracketed full uncached 384-diagram Rust renders
  at 36.879 / 40.182 ms around pinned mermaid-js 11.15.0 at 50,398.7 ms render time (55.691 s live
  wall). Runtime provenance reported one browser main thread, Chrome 150.0.7871.128, input SHA-256
  `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`, and bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`. The incumbent was a
  render-once arm without an incumbent null, so this row asserts no competitive ratio.
- **Validation.** The exact source passed 15 focused cache/stream tests in both CLI binary targets.
  Workspace-wide check and Clippy with warnings denied passed on separate clean-overlay remote
  workers; targeted rustfmt, staged ledger lint, and UBS completed before landing. External and
  self-reported executable hashes agreed in every timed arm. Exact artifact:
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/fm-final-state-exact-thinkstation1-1785660401386884837.json`.
- **Retry predicate.** Re-measure if callers observe intermediate source states, the final-state
  payload or trust contract changes, input write durability changes, changed-diagram count, source
  size, edit count, cache certificate, worker count, corpus, or filesystem changes. Promote no
  competitive claim without a same-invocation incumbent null arm.

## KEEP: retain the render engine across observable final-state transactions (2026-08-02)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-q959`. **Lane:** cod.
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `957f20ad34df7dc83c8dd257898eef0d4a077f1d6f23a8186b3725508ccda1ea`
**A/A null control (same invocation):** invocation
`fm-resident-final-state-exact-thinkstation1-1785663133358567894` interleaved all six
permutations of the live one-shot control/candidate-A/candidate-B arms for 36 whole-job rounds per
arm. Candidate A/B medians were 120,645,682 / 118,293,487.5 ns (ratio `1.019884`), with a
50,000-resample median-ratio 95% CI of `[0.983127, 1.055792]`, including one.
**Counted mechanism:** across 36 rounds the one-shot control started 1,152 processes and rendered
18,432 changed revisions. Each resident candidate started 36 processes and rendered only 1,152
first-seen revisions; the other 17,280 revisions were exact process-local output-cache hits. All
arms still acknowledged 1,152 transactions, wrote the same 18,432 source files, and materialized
every observable output state.

- **Structural gap.** The coalesced one-shot API deleted transient edits, but a caller publishing
  several observable commits still restarted the executable and rebuilt the immutable batch plan,
  manifest lease, pressure snapshot, worker pool, and rendered-revision working set for every
  commit. Mermaid-js keeps one browser page resident; Frankenmermaid can keep its substantially
  cheaper native engine resident too, while additionally replaying exact A/B revisions.
- **Mechanism.** Explicit `render-batch --trust-change-set --final-state-stream` accepts one bounded
  newline-delimited final-state JSON object per transaction. It constructs and validates the
  complete 384-input plan once, holds one cache session for the stream lifetime, applies each known
  bounded source set, renders the surviving independent changes on the existing shared-nothing
  pool, materializes that complete observable state, and flushes one durable certificate at EOF.
  A JSON acknowledgment after each materialization gives the caller a transaction boundary.
- **Whole-job result.** Each round began from an untimed revision-A certificate, then published 32
  alternating transactions over 16 changed diagrams in the complete 384-diagram repository: 512
  source writes and 12,288 observable diagram states per arm. The 32-process live one-shot median
  was 741,942,946 ns and the one-process candidate-A median was 120,645,682 ns: **6.149768x**,
  bootstrap 95% CI `[5.987533, 6.255576]`. All arms ended with identical 384-file / 20,685,288-byte
  output, aggregate SHA-256
  `2f528b7b4e19d28b1546dc44513fc01979e2569b41f9d61af9ddd6be275810c2`.
- **Host admission.** The effect phase ran on eight distinct physical cores 10, 11, 12, 13, 14,
  17, 18, and 20 of x86-64 `thinkstation1`. Its two consecutive one-second admission samples
  peaked at 4.1% and 5.0% busy; the pre-incumbent pair peaked at 12.9% and 11.0%. Every first-seen
  16-diagram revision used eight requested and eight active workers.
- **Live-incumbent bracket.** The same invocation bracketed full uncached 384-diagram Rust renders
  at 37.146 / 35.664 ms around pinned mermaid-js 11.15.0 at 52,072.8 ms render time (57.601 s live
  wall). Runtime provenance reported one browser main thread, Chrome 150.0.7871.128, input SHA-256
  `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`, and bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`. The incumbent was a
  render-once arm without an incumbent null, so this row asserts no competitive ratio.
- **Validation.** Strict clean-overlay remote cache/stream tests passed 15/15 in both CLI binary
  targets and package Clippy passed with warnings denied before measurement. Workspace-wide remote
  check and Clippy, targeted rustfmt, staged ledger lint, and UBS completed before landing. External
  and self-reported executable hashes agreed throughout. Exact artifact:
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/fm-resident-final-state-exact-thinkstation1-1785663133358567894.json`.
- **Retry predicate.** Re-measure if transaction observability, acknowledgment or crash semantics,
  plan/session lifetime, exact revision key or capacity, output materialization, changed-diagram or
  transaction count, corpus, worker count, or filesystem changes. Promote no competitive claim
  without a same-invocation incumbent null arm.

## KEEP: coalesce resident final-state outputs at EOF (2026-08-02)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-mzzl`. **Lane:** cod.
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `af91786bc58fd4c5adae43ee45f651f1fc61a48a82c55fe2e60a29124b5286b9`
**A/A null control (same invocation):** invocation
`fm-resident-final-state-exact-thinkstation1-1785666213885571322` interleaved all six
permutations of the resident ordinary-output control/candidate-A/candidate-B arms for 36 whole-job
rounds per arm. Candidate A/B medians were 84,121,404 / 85,208,775.5 ns (ratio `0.987239`),
with a 50,000-resample median-ratio 95% CI of `[0.963145, 1.025681]`, including one.
**Counted mechanism:** every arm processed and acknowledged 32 transactions, rendered the same 32
first-seen revisions, replayed the same 12,256 exact cached revisions, and performed 512 source
writes per round. The ordinary resident control also materialized 512 output revisions per round;
each `--final-output-only` candidate replaced those bytes by destination and materialized only 16
final outputs at EOF. Across 36 rounds that is 18,432 control output writes versus 576 per
candidate arm, exactly 32x fewer.

- **Structural gap.** The resident transaction stream preserved every observable render and ACK,
  but still copied each superseded SVG revision to the filesystem even when its caller explicitly
  requested only the completed output tree. The existing change-set mode had already proved that a
  destination-keyed final sink can delete this work; the final-state stream had not exposed it.
- **Mechanism.** `render-batch --trust-change-set --final-state-stream --final-output-only` now
  keeps each changed destination's newest immutable output bytes in the process-owned session.
  Every transaction still parses, applies, renders or replays, and emits its JSON ACK; EOF writes
  the deterministic final destination set before committing the durable clean certificate. The
  default resident stream remains unchanged for callers that consume intermediate files.
- **Whole-job result.** Each round began from an untimed revision-A certificate, then published 32
  alternating transactions over 16 changed diagrams in the complete 384-diagram repository: 512
  source updates and 12,288 observable diagram states per arm. Ordinary resident output median was
  154,483,913.5 ns and candidate-A median was 84,121,404 ns: **1.836440x**, bootstrap 95% CI
  `[1.563767, 1.923657]`. All arms ended with identical 384-file / 20,685,288-byte output,
  aggregate SHA-256 `2f528b7b4e19d28b1546dc44513fc01979e2569b41f9d61af9ddd6be275810c2`.
- **Host admission.** The effect phase ran on eight distinct physical cores 10, 11, 12, 13, 14,
  17, 18, and 20 of x86-64 `thinkstation1`. Its two consecutive one-second admission samples
  peaked at 6.1% and 6.9% busy; the pre-incumbent pair peaked at 13.3% and 10.9%. Every first-seen
  16-diagram revision requested and used eight workers.
- **Live-incumbent bracket.** The same invocation ran the exact candidate ELF and pinned live
  mermaid-js 11.15.0 over all 384 diagrams. Mermaid reported 69,522.5 ms render time, one Chrome
  150.0.7871.128 page-main execution context, input SHA-256
  `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`, and bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`. The incumbent arm had no
  null and the post-incumbent Rust bracket was disturbed, so this row asserts no competitive ratio.
- **Validation.** Strict clean-overlay remote cache/stream tests passed 15/15 in both CLI binary
  targets. Workspace-wide remote check and Clippy with warnings denied, targeted rustfmt, staged
  ledger lint, and UBS completed before landing. External and self-reported executable hashes
  agreed in every timed arm. Exact artifact SHA-256
  `9686191180948e0ab39e58b8d36b2b48ddee63728c2f5342834599ceb1d6aaca` at
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/fm-resident-final-state-exact-thinkstation1-1785666213885571322.json`.
- **Retry predicate.** Re-measure if callers require intermediate files, render/ACK or EOF crash
  semantics change, changed-diagram or transaction count changes, revision replay admission,
  output size, corpus, worker count, or filesystem changes. Promote no competitive claim without a
  same-invocation incumbent null arm.

## KEEP: retain one Rayon worker pool across resident transactions (2026-08-02)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-kef8`. **Lane:** cod.
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `fb81536198fa6559a88beb749b511823ee85f783d7ca80aba2df8271c7147248`
**A/A null control (same invocation):** invocation
`fm-resident-worker-pool-exact-thinkstation1-1785667736339070952` interleaved all six
permutations of the fresh-pool control/candidate-A/candidate-B arms for 36 whole-job rounds per
arm. Candidate A/B medians were 64,907,481.5 / 64,750,784.5 ns (ratio `1.002420`), with a
50,000-resample median-ratio 95% CI of `[0.974098, 1.017511]`, including one.
**Counted mechanism:** low-overhead `strace -f -c` over the same 32-transaction job counted 262
`clone3`, 1,109 `rt_sigprocmask`, and 773 `sigaltstack` calls with per-transaction pools versus 14,
99, and 27 with the retained pool. The candidate therefore removed all 248 redundant Rayon worker
creations after the first eight-worker pool. Timed arms otherwise did identical work: 32 ACKed
transactions, 512 source writes, 16 final output writes, 32 first-seen renders, 12,256 exact
revision replays, and 12,288 observable diagram states per round.

- **Profile-first attribution.** After EOF output coalescing moved the bottleneck, a whole-job
  syscall profile ranked thread lifecycle above JSON and filesystem calls: the 32-transaction
  process created eight Rayon workers again on every epoch. The long-lived mermaid-js browser does
  not pay this per-transaction thread lifecycle, and the work is not part of parsing, layout,
  rendering, replay, or durability. A high-overhead DWARF capture was discarded before inference;
  the counted profile above and untraced whole-job result decide the lever.
- **The one lever.** A resident `BatchRenderCacheSession` now owns an `Arc` to the fixed-width Rayon
  pool and returns that pool on later epochs of the same width. Ordinary one-shot batches retain
  their former one-shot pool, a width change replaces the cached pool, and
  `FM_DISABLE_SESSION_WORKER_POOL=1` supplies the exact same-ELF control. The session-local mutex is
  acquired only by the coordinator while selecting the pool; workers never coordinate through it.
- **Whole-job result.** Each round began from an untimed revision-A certificate, then published 32
  alternating transactions over 16 changed diagrams in the complete 384-diagram repository with
  final-output-only semantics. Fresh-pool control median was 84,794,266 ns and retained-pool
  candidate-A median was 64,907,481.5 ns: **1.306387x**, bootstrap 95% CI
  `[1.266042, 1.347285]`. All arms produced identical 384-file / 20,685,288-byte output, aggregate
  SHA-256 `2f528b7b4e19d28b1546dc44513fc01979e2569b41f9d61af9ddd6be275810c2`.
- **Host admission.** The effect phase ran on eight distinct physical cores 10, 11, 12, 13, 14,
  17, 18, and 20 of x86-64 `thinkstation1`. Its consecutive one-second admission samples peaked at
  15.2% and 14.1% busy; the pre-incumbent pair peaked at 13.1% and 7.0%, with no selected core above
  20%. Every first-seen revision requested and used eight workers.
- **Live-incumbent bracket.** The same invocation ran the candidate ELF and pinned live mermaid-js
  11.15.0 over all 384 diagrams. Mermaid reported 51,812.3 ms render time (57.353 s live wall), one
  Chrome 150.0.7871.128 page-main execution context, input SHA-256
  `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`, and bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`. Uncached Rust brackets were
  42.599 / 37.172 ms. The incumbent arm had no null, so this row asserts no competitive ratio.
- **Validation.** The focused persistent-pool unit test passed on a clean-overlay remote worker;
  workspace-wide remote check passed. Targeted rustfmt, workspace-wide Clippy with warnings denied,
  staged ledger lint, and UBS completed before landing. External and self-reported executable hashes
  agreed in every timed arm. Exact artifact SHA-256
  `83db3f091cf05a0e77af0964bf624b9242baa71d74577d3c66f52aafcd0b022c` at
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/fm-resident-worker-pool-exact-thinkstation1-1785667736339070952.json`.
- **Retry predicate.** Re-measure if the stream, pool-width selection, changed-diagram or transaction
  count, revision replay, source/output durability, corpus, allocator, Rayon version, host topology,
  or filesystem changes. Promote no competitive claim without a same-invocation incumbent null arm.

## KEEP: replay complete resident revisions before the generic batch engine (2026-08-02)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-d8vo`. **Lane:** cod.
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `378ee0a4a1ced267bd0f15c5332d54117df1db43531c79773941f9a27a0a9aa7`
**A/A null control (same invocation):** invocation
`fm-resident-transaction-replay-exact-thinkstation1-1785670293295219209` interleaved all six
permutations of the disabled-path control/candidate-A/candidate-B arms for 36 whole-job rounds per
arm. Candidate A/B medians were 56,557,646.5 / 55,894,518.5 ns (ratio `1.011864`), with a
50,000-resample median-ratio 95% CI of `[0.985688, 1.032256]`, including one.
**Counted mechanism:** across 36 rounds, the disabled-path control sent all 1,152 transactions
through the generic batch engine. Each candidate admitted 1,080 complete exact-revision
transactions before that engine and used it only for the first two revisions in each process. All
arms still acknowledged 1,152 transactions, wrote 18,432 changed sources and 576 final outputs,
and accounted for the same 441,216 persistent diagram hits and 1,152 first-seen renders.

- **Profile-first attribution.** After retaining the Rayon pool, whole-job sampling still showed
  Rayon scheduling, epoch pinning, and filesystem reopen work on exact A/B revision-cache hits.
  Mermaid-js's resident page does not rebuild a generic batch epoch to publish bytes already held
  in memory. This lever admits the whole transaction before that structural work begins.
- **The one lever.** A trusted resident session now hashes each changed final-state source and
  requires an exact process-local rendered-output hit for every change. On complete admission it
  advances only those destination bytes and manifest entries, preserves the trusted whole-batch
  byte total, emits the normal transaction ACK, and skips source reload, parse planning, pressure
  sampling, Rayon dispatch, layout, and rendering. A miss falls through unchanged, JSON diagnostic
  mode keeps the generic per-diagram reports, and `FM_DISABLE_RESIDENT_TRANSACTION_REPLAY=1`
  supplies the same-ELF control.
- **Whole-job result.** Each round began from an untimed revision-A certificate, then published 32
  alternating transactions over 16 changed diagrams in the complete 384-diagram repository with
  final-output-only semantics: 512 source writes and 12,288 observable diagram states per arm.
  Generic-path control median was 62,342,917 ns and candidate-A median was 56,557,646.5 ns:
  **1.102290x**, bootstrap 95% CI `[1.086652, 1.126391]`. All arms produced identical 384-file /
  20,685,288-byte output, aggregate SHA-256
  `2f528b7b4e19d28b1546dc44513fc01979e2569b41f9d61af9ddd6be275810c2`.
- **Host admission.** The effect phase ran on eight distinct physical cores 10, 11, 12, 13, 14,
  17, 18, and 20 of x86-64 `thinkstation1`. Its two consecutive one-second admission samples
  peaked at 5.9% and 9.9% busy; the pre-incumbent pair peaked at 8.0% and 9.1%, and the post-run
  sample peaked at 9.9%, with no selected core above 20%.
- **Live-incumbent bracket.** The same invocation bracketed full uncached 384-diagram Rust renders
  at 93.173 / 36.766 ms around pinned mermaid-js 11.15.0 at 52,930.5 ms render time (61.220 s live
  wall). Runtime provenance reported one Chrome 150.0.7871.128 page-main execution context, input
  SHA-256 `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`, and bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`. The incumbent was a
  render-once arm without an incumbent null, so this row asserts no competitive ratio.
- **Validation.** The focused exact-transaction replay test passed in both CLI binary targets on a
  clean-overlay remote worker; workspace-wide check and Clippy with warnings denied passed on
  remote workers. Targeted rustfmt passed; external and self-reported executable hashes agreed in
  every timed arm. Exact artifact SHA-256
  `6c68d1881b24d6333e784674aca6786fef6f201194dd28bb1e868500dbb32930` at
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/fm-resident-transaction-replay-exact-thinkstation1-1785670293295219209.json`.
- **Retry predicate.** Re-measure if trusted-batch admission, exact revision keys or retention,
  transaction ACK or crash semantics, source/output durability, changed-diagram or transaction
  count, corpus, worker count, allocator, or filesystem changes. Promote no competitive claim
  without a same-invocation incumbent null arm.

## KEEP: coalesce resident final-state source writes at EOF (2026-08-02)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-ej6d`. **Lane:** cod.
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `7981692790034c0e80160a2d7e69c829972f36a5fbe9ad83b0cdf313d35ea7d8`
**A/A null control (same invocation):** invocation
`fm-resident-source-coalescing-exact-thinkstation1-1785672029625618855` interleaved all six
permutations of the ordinary-source control/candidate-A/candidate-B arms for 36 whole-job rounds
per arm. Candidate A/B medians were 33,836,439.5 / 34,009,150.5 ns (ratio `0.994922`), with a
50,000-resample median-ratio 95% CI of `[0.968481, 1.011425]`, including one.
**Counted mechanism:** runtime counters recorded 18,432 source materializations across 36 control
rounds versus 576 in each candidate arm: exactly 512 versus 16 per round, or 32x fewer. Every arm
still processed and acknowledged 1,152 transactions, performed 1,080 complete revision replays,
rendered 1,152 first-seen revisions, reported 441,216 persistent hits, and wrote 576 final SVGs.

- **Structural gap.** Once complete revision replay deleted generic batch epochs, synchronous
  source-file materialization was the largest avoidable survivor: every transient source revision
  was copied to disk even when the caller consumed only the completed source and output trees.
  Mermaid-js's page keeps input strings in memory and does not pay this filesystem boundary.
- **The one lever.** Explicit `render-batch --trust-change-set --final-state-stream
  --final-source-only` stores each updated source body by input path and feeds first-seen renders
  directly from that resident map. It replaces superseded bodies, writes only the newest source set
  at EOF, refreshes manifest length and modification metadata, then commits the durable clean
  certificate. Existing callers retain per-ACK source-file observability by omitting the flag; an
  ACK in this mode certifies the in-memory render state, as documented by the CLI.
- **Whole-job result.** Each round began from an untimed revision-A certificate, then published 32
  alternating transactions over 16 changed diagrams in the complete 384-diagram repository while
  also using final-output-only semantics: 512 source updates and 12,288 observable in-memory
  diagram states per arm. Ordinary-source control median was 57,247,822 ns and candidate-A median
  was 33,836,439.5 ns: **1.691899x**, bootstrap 95% CI `[1.663078, 1.734648]`. All arms produced
  identical 384-file / 20,685,288-byte SVG output, aggregate SHA-256
  `2f528b7b4e19d28b1546dc44513fc01979e2569b41f9d61af9ddd6be275810c2`.
- **Host admission.** The effect phase ran on eight distinct physical cores 10, 11, 12, 13, 14,
  17, 18, and 20 of x86-64 `thinkstation1`. After one rejected busy sample, its admitted
  consecutive one-second samples peaked at 14.1% and 19.8% busy. The pre-incumbent pair peaked at
  10.0% and 17.2%, and the post-run sample at 17.8%, with no admitted selected core above 20%.
- **Live-incumbent bracket.** The same invocation bracketed full uncached 384-diagram Rust renders
  at 34.290 / 40.357 ms around pinned mermaid-js 11.15.0 at 52,439.8 ms render time (59.459 s live
  wall). Runtime provenance reported one Chrome 150.0.7871.128 page-main execution context, input
  SHA-256 `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`, and bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`. The incumbent was a
  render-once arm without an incumbent null, so this row asserts no competitive ratio.
- **Validation.** The focused deferred-source/revision-replay test passed in both CLI binary
  targets on a clean-overlay remote worker. Workspace-wide remote check passed, final-source
  workspace Clippy passed with warnings denied, and targeted rustfmt passed. External and
  self-reported executable hashes agreed in every timed arm. Exact artifact SHA-256
  `ab095c3cd41031a5230b53f6393c0e3e5e390ede2f8b5e0bd84ea912f45eaa0f` at
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/fm-resident-source-coalescing-exact-thinkstation1-1785672029625618855.json`.
- **Retry predicate.** Re-measure if callers require source files between transaction ACKs,
  resident source ownership, EOF materialization or crash semantics, exact revision admission,
  changed-diagram or transaction count, source size, corpus, allocator, or filesystem changes.
  Promote no competitive claim without a same-invocation incumbent null arm.

## KEEP: pipeline resident final-state acknowledgments at EOF (2026-08-02)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-s1kd`. **Lane:** cod.
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `697640122efe90b3520368d395a97d4041ab2829a408ee18c361cbbb6bc62712`
**A/A null control (same invocation):** invocation
`fm-resident-ack-coalescing-exact-thinkstation1-1785673773027790859` interleaved all six
permutations of per-transaction-ACK control/candidate-A/candidate-B for 36 whole-job rounds per
arm. Candidate A/B medians were 32,714,886.5 / 32,767,751.5 ns (ratio `0.998387`), with a
50,000-resample median-ratio 95% CI of `[0.962463, 1.022800]`, including one.
**Counted mechanism:** runtime counters recorded 1,152 stdout acknowledgment writes across 36
control rounds versus 36 in each candidate arm: exactly 32 versus one per round, or 32x fewer.
Every arm still processed 1,152 transactions, materialized 576 final sources and 576 final SVGs,
performed 1,080 complete revision replays, rendered 1,152 first-seen revisions, and reported
441,216 persistent hits.

- **Structural gap.** A whole completed revision job still forced a synchronous caller/process
  round trip after every one of its 32 transactions, even when the caller consumed only the final
  committed source/output trees. The pinned incumbent's render-once boundary has no corresponding
  intermediate acknowledgment protocol. Whole-job profiling showed that the Rust child spent only
  about one quarter of elapsed time on CPU, making the repeated synchronization a plausible
  off-CPU structural lever.
- **The one lever.** Explicit `render-batch --trust-change-set --final-state-stream
  --final-ack-only` accepts the same newline-delimited transactions but emits one aggregate JSON
  acknowledgment only after EOF source/output materialization and the durable cache commit
  succeed. This lets callers pipeline the entire job into the resident process. Existing callers
  retain one flushed ACK per transaction by omitting the flag; JSON per-input reporting is rejected
  with the EOF-only contract so stdout remains exactly one machine-readable record.
- **Whole-job result.** Each round began from an untimed revision-A certificate, then sent 32
  alternating transactions over 16 changed diagrams in the complete 384-diagram repository while
  using both final-source-only and final-output-only semantics. Per-transaction-ACK control median
  was 35,289,560.5 ns and candidate-A median was 32,714,886.5 ns: **1.078700x**, bootstrap 95% CI
  `[1.050621, 1.114394]`. All arms produced identical 384-file / 20,685,288-byte SVG output,
  aggregate SHA-256 `2f528b7b4e19d28b1546dc44513fc01979e2569b41f9d61af9ddd6be275810c2`.
- **Host admission.** The effect phase ran on eight distinct physical cores 10, 11, 12, 13, 14,
  17, 18, and 20 of x86-64 `thinkstation1`. Its admitted consecutive one-second samples peaked at
  19.0% and 9.2% busy. The pre-incumbent admission required eight attempts; its accepted pair
  peaked at 8.0% and 14.0%. The report-only post-incumbent sample was disturbed and is not used for
  admission.
- **Live-incumbent bracket.** The same invocation ran the exact candidate ELF around pinned
  mermaid-js 11.15.0 over all 384 diagrams. Mermaid reported 52,511.6 ms render time (59.741 s live
  wall), one Chrome 150.0.7871.128 page-main execution context, input SHA-256
  `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`, and bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`. The incumbent was a
  render-once arm without an incumbent null, so this row asserts no competitive ratio.
- **Validation.** A 32-transaction protocol smoke produced exactly one correct aggregate EOF ACK.
  Clean-overlay remote workspace-wide check and Clippy with warnings denied passed; targeted
  rustfmt passed, all timed arms self-reported the external ELF hash, and the three output trees
  were byte-identical. Exact
  artifact SHA-256 `e7773aec9534896a02de2ab112e18a427040e262545950141fc1a82873895eb5` at
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/fm-resident-ack-coalescing-exact-thinkstation1-1785673773027790859.json`.
- **Retry predicate.** Re-measure if callers require intermediate transaction ACKs, stdout flush or
  buffering changes, EOF commit semantics, transaction count or payload size, source/output
  materialization, exact revision replay, corpus, allocator, or filesystem changes. Promote no
  competitive claim without a same-invocation incumbent null arm.

## KEEP: reuse prepared resident final-state payloads (2026-08-02)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-l057`. **Lane:** cod.
**Campaign result class:** maintenance-self-speedup
**Executing ELF SHA-256 (self-reported by process):** `79c1150963b3003f2cd7751fee97a4a448a9940020323b8b131d26e0b006c184`
**A/A null control (same invocation):** invocation
`fm-resident-payload-cache-exact-thinkstation1-1785674851673222189` interleaved all six
permutations of cache-disabled control/candidate-A/candidate-B for 36 whole-job rounds per arm.
Candidate A/B medians were 30,081,295 / 29,906,814.5 ns (ratio `1.005834`), with a
50,000-resample median-ratio 95% CI of `[0.981678, 1.035895]`, including one.
**Counted mechanism:** the control decoded and content-addressed 1,152/1,152 transaction payloads.
Each candidate decoded 72 and reused 1,080 already-validated exact payloads: exactly two decodes
and 30 reuses per 32-transaction round. All arms still processed 1,152 transactions, performed
1,080 complete revision replays, materialized 576 final sources and SVGs, wrote 36 aggregate ACKs,
rendered 1,152 first-seen revisions, and reported 441,216 persistent hits.

- **Profile-first attribution.** On the admitted EOF-only survivor, weighted `perf` self-time put
  serde_json string/escape decoding at 15.5%; SHA-256 and hex preparation, UTF-8/string copies, and
  path comparisons formed the next large Rust-specific block. The resident caller alternated only
  two exact transaction payloads, while the process discarded their validated object maps and
  source digests after every line.
- **The one lever.** The final-state stream now retains up to eight exact encoded payloads and their
  validated update map, ordered changed-input vector, total source bytes, and per-source SHA-256
  digests under a combined 8 MiB ceiling. Hits use byte-exact payload equality before reusing that
  immutable prepared transaction; misses keep full unknown-path and size validation, and LRU
  eviction bounds residency. `FM_DISABLE_RESIDENT_PAYLOAD_CACHE=1` is the same-ELF control.
- **Whole-job result.** Each round began from an untimed revision-A certificate, then sent 32
  alternating transactions over 16 changed diagrams in the complete 384-diagram repository with
  EOF-only acknowledgments, sources, and outputs. Cache-disabled control median was 32,777,408.5
  ns and candidate-A median was 30,081,295 ns: **1.089628x**, bootstrap 95% CI
  `[1.055067, 1.128990]`. All arms produced identical 384-file / 20,685,288-byte SVG output,
  aggregate SHA-256 `2f528b7b4e19d28b1546dc44513fc01979e2569b41f9d61af9ddd6be275810c2`.
- **Host admission.** The effect phase ran on eight distinct physical cores 10, 11, 12, 13, 14,
  17, 18, and 20 of x86-64 `thinkstation1`. Its consecutive one-second admission samples peaked
  at 4.0% and 11.1% busy. After one rejected sample, the accepted pre-incumbent pair peaked at 7.1%
  and 13.1%; the post-run sample peaked at 14.0%, with no admitted core above 20%.
- **Live-incumbent bracket.** The same invocation bracketed full uncached 384-diagram Rust renders
  at 67.551 / 39.712 ms around pinned mermaid-js 11.15.0 at 52,612.2 ms render time (58.814 s live
  wall). Runtime provenance reported one Chrome 150.0.7871.128 page-main execution context, input
  SHA-256 `4d9914725224c310b44bd1bb4c03cc18575b6c02ef2350a70b6496470e53b464`, and bundle SHA-256
  `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`. The incumbent was a
  render-once arm without an incumbent null, so this row asserts no competitive ratio.
- **Validation.** The focused cache test passed in both CLI binary targets on a clean-overlay
  remote worker, including exact `Arc` reuse and the disabled path. Clean-overlay remote
  workspace-wide check and Clippy with warnings denied passed; targeted rustfmt passed. The exact
  smoke and all timed arms self-reported the external ELF hash, and the three output trees were
  byte-identical. Exact
  artifact SHA-256 `bde98682a4bc646f261b8ff32e4e518c1b3c0652885926a518f8c630caf48ddd` at
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/fm-resident-payload-cache-exact-thinkstation1-1785674851673222189.json`.
- **Retry predicate.** Re-measure if payload identity or diversity, transaction count, retained
  entry/byte ceilings, source digest or validation semantics, EOF protocol, exact revision replay,
  corpus, allocator, or filesystem changes. Promote no competitive claim without a same-invocation
  incumbent null arm.

## CERTIFIED INCUMBENT WIN: coalesce superseded EOF-only revisions (2026-08-02)

**Bead:** `bd-rlf0`. **Lane:** cod.
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `abce4109e48b0d4abfc48329173f2ac72e700d2891f411f9807aeae27b32870e`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=fm-final-revision-coalescing-exact-thinkstation1-1785676351152936591 measured_ratio=261.3610386677114x
**A/A null control (same invocation):** candidate A/B medians were 34,702,571 / 34,743,683.5
ns (ratio `0.998817`), with a 50,000-resample median-ratio 95% CI of
`[0.980572, 1.009457]`, including one. Live mermaid-js ran 20 null rounds: median
`1.003716`, bootstrap 95% CI `[0.999354, 1.018642]`, sufficient=true and
`cv_gate=never`. The cross-engine median-ratio 95% CI was `[254.120580x, 264.458920x]`.
**Counted mechanism:** across 36 rounds, the disabled same-ELF control accepted and executed
1,152 transactions and rendered 73,728 diagram revisions. Each candidate accepted the identical
1,152 transactions but executed 36 completed states, rendered zero revisions, and converted all
2,304 final diagram lookups into persistent hits. That is 32 accepted transactions to one
execution per round and 2,048 superseded renders removed per round.

- **Structural gap.** Combining final-source-only, final-output-only, and final-ack-only makes
  intermediate revisions externally unobservable, yet the resident runner still decoded,
  content-addressed, laid out, rendered, and staged every overwritten state. Mermaid-js's
  completed-job arm receives only the final 64 documents and does not pay that history, so this
  was the dominant Rust-only whole-job cost rather than a shared render primitive.
- **The one lever.** When all three EOF contracts are present, the stream now envelope-validates
  every JSON transaction, replaces each input's superseded body in a bounded-by-input final map,
  then hashes and executes only that completed update set. Ordinary streams retain transaction
  observability and execute every line. `FM_DISABLE_FINAL_STATE_COALESCING=1` selects the old path
  in the same ELF for exact control measurements.
- **Whole-job self result.** Each round seeded the canonical 64-diagram durable output, then sent
  31 distinct full-batch revisions followed by the canonical completed revision: 32 payloads,
  2,048 accepted source updates, one aggregate acknowledgment, and final source/output
  materialization. The old path median was 172,958,439.5 ns versus candidate-A at 34,702,571 ns,
  a **4.984024x** maintenance improvement with bootstrap 95% CI
  `[4.904140x, 5.119784x]`.
- **Live incumbent result.** In the same top-level invocation, pinned mermaid-js rendered the
  identical canonical `ci_shared_subgraph_divergent_64` job in a median **9,069.900001 ms** over
  nine effect samples. The conservative Rust completed-job median was **34.702571 ms**, including
  process startup, all 32 envelope parses, final source writes, output-cache lookup, and durable
  manifest commit: **261.361039x** with bootstrap 95% CI
  `[254.120580x, 264.458920x]`. Runtime provenance reported one Chrome 150.0.7871.128 page-main
  execution thread.
- **Output equivalence.** Candidate A, candidate B, and control produced identical 64-file,
  3,469,549-byte output trees with aggregate SHA-256
  `a8502bdcf304ef8db6683a5075c896017bebd6daeabada58725436dccb3077b3`. The pre-existing shared
  extractor artifact for the byte-identical input SHA-256
  `f487b4094bc4020436956d78067c529b80aa0ce8e595fbaa1a193c081fb13e68` proves 64/64 diagrams
  structurally equivalent to the same pinned mermaid-js bundle with zero divergent or unverified.
- **Host and validation.** The effect and incumbent phases ran on eight distinct physical cores
  10, 11, 12, 13, 14, 17, 18, and 20 of x86-64 `thinkstation1` (AMD Ryzen Threadripper PRO
  5975WX). Consecutive effect admission samples peaked at 6.0% / 2.1% busy; incumbent admission
  peaked at 10.9% / 9.1%, and the post-run sample at 15.0%. Focused merge/contract tests passed in
  both CLI binary targets; clean-overlay remote workspace check and Clippy with warnings denied
  passed; targeted rustfmt passed. UBS's cargo-backed fmt, Clippy, check, test-build, audit, and
  deny phases passed; its remaining findings were pre-existing broad heuristics and false-positive
  ordinary equality-as-secret reports.
- **Evidence.** Exact artifact
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/fm-final-revision-coalescing-exact-thinkstation1-1785676351152936591.json`
  (SHA-256 `2338daff0e11d62e11dfac9d888854270f47cfd376325b5c245155e1272466bf`);
  structural-equivalence artifact
  `.benchmarks/headtohead/ci-shared-subgraph-divergent-64-equivalence/equivalence-4e990fe6-1785545013442.json`.
- **Retry predicate.** Re-measure if any intermediate source, output, or acknowledgment becomes
  observable; transaction validation or failure semantics change; completed-state payload size,
  revision count, persistent-cache state, fixture, pinned incumbent, executing ELF, affinity, or
  median-CI gate changes.

## CERTIFIED INCUMBENT WIN: decode only the completed snapshot (2026-08-02)

**Bead:** `bd-g11x`. **Lane:** cod.
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `aa25ae612c42ad9bbfa445fd1d3f050f6b8abd9e13f667215afc74c9fe1de6f7`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=fm-complete-snapshot-elision-exact-thinkstation1-1785678050583188381 measured_ratio=362.3659771683321x
**A/A null control (same invocation):** candidate A/B medians were 25,504,050 / 25,386,732.5
ns (ratio `1.004621`), with a 50,000-resample median-ratio 95% CI of
`[0.980283, 1.017928]`, including one. Live mermaid-js ran 20 null rounds: median
`1.000680`, bootstrap 95% CI `[0.993398, 1.005398]`, sufficient=true and
`cv_gate=never`. The cross-engine median-ratio 95% CI was `[344.666483x, 370.825446x]`.
**Counted mechanism:** across 36 rounds, the disabled same-ELF control decoded and merged all
1,152 complete snapshot payloads before executing 36 completed states. Each candidate retained
the identical 1,152 length- and UTF-8-framed payloads but JSON-decoded only the newest 36,
skipping 1,116 superseded decodes: 31 removed decodes per round. All arms still accepted 1,152
transactions, executed 36 completed states, performed 2,304 persistent output lookups, wrote
2,304 final sources and 36 acknowledgments, and rendered or rewrote zero SVGs.

- **Profile-first attribution.** `perf` on the completed-state whole job ranked serde_json string
  escape scanning/decoding at 21.60% self-time, with serde iteration/string parsing at another
  9.26%; vector appends, SHA-256, UTF-8 conversion, and memory copying brought the Rust-only
  stream ingestion block to roughly 45%. The incumbent receives the final 64 documents once and
  pays none of the 31 superseded JSON object decodes, so this was a structural gap rather than a
  shared parse/layout/render primitive.
- **The one lever.** Explicit `--complete-snapshot-stream`, combined with final-state,
  final-source, final-output, and final-ack EOF contracts, asserts that every non-empty record is
  a complete batch snapshot. The runner now keeps two reusable byte buffers, swaps each framed
  record over the previous one without object decoding, then JSON-decodes and verifies only the
  newest record contains every declared input before executing it. Ordinary and partial-update
  streams retain their existing validation and execution semantics.
  `FM_DISABLE_COMPLETE_SNAPSHOT_ELISION=1` selects the prior decode-and-merge path in the same ELF.
- **Whole-job self result.** Each round seeded the canonical durable 64-diagram state, sent 31
  distinct full snapshots followed by the canonical completed snapshot (188,070,120 encoded
  bytes across each 36-round candidate arm), then materialized sources and committed the cache.
  The control median was 33,500,347.5 ns versus candidate-A at 25,504,050 ns: **1.313530x**, with
  bootstrap 95% CI `[1.292546x, 1.341553x]`.
- **Live incumbent result.** In the same top-level invocation, pinned mermaid-js rendered the
  identical canonical `ci_shared_subgraph_divergent_64` job in a median **9,241.8 ms** over nine
  effect samples. The conservative Rust completed-job median was **25.504050 ms**, including
  process startup, framing all 32 snapshots, final JSON validation, 64 final source writes,
  persistent output lookup, and durable manifest commit: **362.365977x** with bootstrap 95% CI
  `[344.666483x, 370.825446x]`. Runtime provenance reported one Chrome 150.0.7871.128 page-main
  execution thread.
- **Output equivalence.** Candidate A, candidate B, and control produced identical 64-file,
  3,469,549-byte SVG output trees with aggregate SHA-256
  `a8502bdcf304ef8db6683a5075c896017bebd6daeabada58725436dccb3077b3`. The shared extractor
  artifact for input SHA-256
  `f487b4094bc4020436956d78067c529b80aa0ce8e595fbaa1a193c081fb13e68` proves 64/64 diagrams
  structurally equivalent to the same pinned mermaid-js bundle with zero divergent or unverified.
- **Host and validation.** The effect and incumbent phases ran on eight distinct physical cores
  10, 11, 12, 13, 14, 17, 18, and 20 of x86-64 `thinkstation1` (AMD Ryzen Threadripper PRO
  5975WX). Consecutive effect admission samples peaked at 4.0% / 2.0% busy; incumbent admission
  peaked at 9.9% / 8.0%, and the report-only post-run sample at 19.8%. The complete-snapshot
  contract smoke and focused missing-input test passed; clean-overlay remote workspace check and
  Clippy with warnings denied passed; targeted rustfmt passed. All timed arms self-reported the
  external ELF hash.
- **Evidence.** Exact artifact
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/fm-complete-snapshot-elision-exact-thinkstation1-1785678050583188381.json`
  (SHA-256 `0fbe2ea0d62b45270a2a61624678dbd465f0663e655a907fb2f47f24863c27fa`);
  structural-equivalence artifact
  `.benchmarks/headtohead/ci-shared-subgraph-divergent-64-equivalence/equivalence-4e990fe6-1785545013442.json`.
- **Retry predicate.** Re-measure if a record may omit a batch input, superseded-record JSON
  diagnostics become observable, framing or input-size limits change, completed-state payload
  size or snapshot count changes, persistent-cache or source materialization state changes, or
  the fixture, pinned incumbent, executing ELF, affinity, or median-CI gate changes.

## CERTIFIED INCUMBENT WIN: suppress certified canonical source rewrites (2026-08-02)

**Bead:** `bd-m6my`. **Lane:** cod.
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `37890e38123642026aee1cee6cb00a04b542f6a5a0b8e550d8cadd93efa04ee8`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=fm-certified-source-noop-exact-thinkstation1-1785679363765615430 measured_ratio=392.89264885255807x
**A/A null control (same invocation):** candidate A/B medians were 23,416,065.5 / 23,187,222
ns (ratio `1.009869`), with a 50,000-resample median-ratio 95% CI of
`[0.988693, 1.025065]`, including one. Live mermaid-js ran 20 null rounds: median
`0.999926`, bootstrap 95% CI `[0.988784, 1.004190]`, sufficient=true and
`cv_gate=never`. The cross-engine median-ratio 95% CI was `[385.354954x, 407.917547x]`.
**Counted mechanism:** across 36 rounds, the same-ELF disabled control performed 2,304 final
source writes. Each candidate performed zero and admitted 2,304 exact source states from the
durable predecessor certificate: 64 writes removed per round. All arms still accepted 1,152
complete snapshots, skipped 1,116 superseded JSON decodes, executed 36 completed states,
performed 2,304 persistent output lookups, wrote 36 acknowledgments, and rendered or rewrote zero
SVGs.

- **Structural gap.** After completed-snapshot decode elision, every round still rewrote all 64
  canonical repository source files even though the clean predecessor manifest already certified
  their SHA-256, byte length, and modification timestamp and the completed snapshot returned to
  those exact bytes. Mermaid-js's timed render receives source text in memory and pays no source
  repository materialization, making this remaining filesystem work a Rust-only gap.
- **The one lever.** At stream admission, the runner now retains the source identity entries only
  when a matching durable clean-batch certificate is accepted. Complete-snapshot EOF
  materialization suppresses a write only when the final SHA-256 equals that admitted identity and
  current on-disk length and modification timestamp remain unchanged. A missing certificate,
  changed digest, changed metadata, partial stream, or ordinary caller falls back to the existing
  write path. `FM_DISABLE_CERTIFIED_SOURCE_NOOP=1` selects that path in the same ELF.
- **Whole-job self result.** Every timed round began from an untimed completed-stream certificate,
  then sent 31 distinct full snapshots followed by the canonical completed snapshot, retained only
  the newest JSON object, executed it, checked all 64 source identities, committed the cache, and
  emitted one ACK. The write-enabled control median was 25,560,203.5 ns versus candidate-A at
  23,416,065.5 ns: **1.091567x**, with bootstrap 95% CI
  `[1.071546x, 1.124088x]`.
- **Live incumbent result.** In the same top-level invocation, pinned mermaid-js rendered the
  identical canonical `ci_shared_subgraph_divergent_64` job in a median **9,200.0 ms** over nine
  effect samples. The conservative Rust completed-job median was **23.416066 ms**, including
  process startup, framing all 32 snapshots, final JSON validation, 64 certified source checks,
  persistent output lookup, and durable manifest commit: **392.892649x** with bootstrap 95% CI
  `[385.354954x, 407.917547x]`. Runtime provenance reported one Chrome 150.0.7871.128 page-main
  execution thread.
- **Output equivalence.** Candidate A, candidate B, and control produced identical 64-file,
  3,469,549-byte SVG output trees with aggregate SHA-256
  `a8502bdcf304ef8db6683a5075c896017bebd6daeabada58725436dccb3077b3`. The shared extractor
  artifact for input SHA-256
  `f487b4094bc4020436956d78067c529b80aa0ce8e595fbaa1a193c081fb13e68` proves 64/64 diagrams
  structurally equivalent to the same pinned mermaid-js bundle with zero divergent or unverified.
- **Host and validation.** The effect and incumbent phases ran on eight distinct physical cores
  10, 11, 12, 13, 14, 17, 18, and 20 of x86-64 `thinkstation1` (AMD Ryzen Threadripper PRO
  5975WX). Consecutive effect admission samples peaked at 3.92% / 1.98% busy; incumbent admission
  peaked at 4.95% / 7.92%, and the report-only post-run sample at 17.82%. The focused test passed
  in both CLI binary targets, proving an exact certificate skips the write while a same-length
  digest change materializes it. Clean-overlay remote workspace check and Clippy with warnings
  denied passed; targeted rustfmt passed. All timed arms self-reported the external ELF hash.
- **Evidence.** Exact artifact
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/fm-certified-source-noop-exact-thinkstation1-1785679363765615430.json`
  (SHA-256 `809fe4e1b46d93b0677964ba00b21363a446bcc0cdacb0575586dafe6f3727ed`);
  structural-equivalence artifact
  `.benchmarks/headtohead/ci-shared-subgraph-divergent-64-equivalence/equivalence-4e990fe6-1785545013442.json`.
- **Retry predicate.** Re-measure if clean-certificate admission, source identity fields,
  filesystem timestamp resolution, symlink or external-writer semantics, completed source state,
  snapshot count, persistent output state, or the fixture, pinned incumbent, executing ELF,
  affinity, or median-CI gate changes.

## CERTIFIED INCUMBENT WIN: replay exact durable transactions before worker startup (2026-08-02)

**Bead:** `bd-qam0`. **Lane:** cod.
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `4d28da23db86b46c6b17f406dabbf01eea253a51abd0150c4991f1d137150214`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=fm-certified-transaction-replay-exact-thinkstation1-1785681170215358045 measured_ratio=423.9332272599459x
**A/A null control (same invocation):** candidate A/B medians were 21,647,277 / 21,609,951.5
ns (ratio `1.001727`), with a 50,000-resample median-ratio 95% CI of
`[0.976320, 1.021407]`, including one. Live mermaid-js ran 20 null rounds: median
`1.003273`, bootstrap 95% CI `[0.992627, 1.008835]`, sufficient=true and
`cv_gate=never`. The cross-engine median-ratio 95% CI was `[410.652240x, 429.643040x]`.
**Counted mechanism:** across 36 rounds, the same-ELF disabled control admitted 2,304 persistent
diagram hits but still started an eight-worker pool in every process; maximum active workers was
eight. Each candidate replayed all 36 complete transactions directly from the durable predecessor
certificate and started zero workers. All arms still accepted 1,152 complete snapshots, skipped
1,116 superseded JSON decodes, checked and reused 2,304 certified source states, wrote zero sources
or SVGs, and emitted 36 acknowledgments.

- **Profile-first attribution.** A symbols-preserving product profile first removed the
  harness-only self-ELF read/SHA from consideration. On the remaining whole job, Rayon worker stack
  allocation/thread creation accounted for a top 9.45% self-time entry, with scheduler/audit yield
  work adding several more percent, even though counters proved 64 persistent hits and zero
  renders. Mermaid-js does not create a redundant Rust render pool for a durable output hit, so
  this was an unshared structural cost rather than a common render primitive.
- **The one lever.** Complete-snapshot EOF mode now accepts the prepared transaction directly when
  a matching clean predecessor certificate covers every batch input, every prepared source digest
  equals its certified digest, the current in-memory manifest entries remain byte-for-byte equal
  to those admitted entries, no deferred output exists, and source materialization is deferred.
  Any partial, changed, uncertified, previously-mutated, or ordinary transaction falls through to
  the existing rendered-revision and disk-cache paths. `FM_DISABLE_CERTIFIED_TRANSACTION_REPLAY=1`
  selects that fallback in the same ELF.
- **Whole-job self result.** Every round began from an untimed completed-stream certificate, then
  sent 31 distinct full snapshots followed by the canonical completed snapshot, decoded and
  hashed only the newest state, checked all 64 certified entries, committed the cache, and emitted
  one ACK. The replay-disabled control median was 23,199,884.5 ns versus candidate-A at
  21,647,277 ns: **1.071723x**, with bootstrap 95% CI
  `[1.051174x, 1.106862x]`.
- **Live incumbent result.** In the same top-level invocation, pinned mermaid-js rendered the
  identical canonical `ci_shared_subgraph_divergent_64` job in a median **9,177.0 ms** over nine
  effect samples. The conservative Rust completed-job median was **21.647277 ms**, including
  process startup, framing all 32 snapshots, final JSON validation and hashing, 64 certificate and
  source identity checks, and durable manifest commit: **423.933227x** with bootstrap 95% CI
  `[410.652240x, 429.643040x]`. Runtime provenance reported one Chrome 150.0.7871.128 page-main
  execution thread.
- **Output equivalence.** Candidate A, candidate B, and control produced identical 64-file,
  3,469,549-byte SVG output trees with aggregate SHA-256
  `a8502bdcf304ef8db6683a5075c896017bebd6daeabada58725436dccb3077b3`. The shared extractor
  artifact for input SHA-256
  `f487b4094bc4020436956d78067c529b80aa0ce8e595fbaa1a193c081fb13e68` proves 64/64 diagrams
  structurally equivalent to the same pinned mermaid-js bundle with zero divergent or unverified.
- **Host and validation.** The effect and incumbent phases ran on eight distinct physical cores
  10, 11, 12, 13, 14, 17, 18, and 20 of x86-64 `thinkstation1` (AMD Ryzen Threadripper PRO
  5975WX). Consecutive effect admission samples peaked at 3.03% / 3.96% busy; incumbent admission
  peaked at 8.91% / 19.19%. The report-only post-run sample was disturbed at 33.0% and was not an
  admission input. The focused exact/miss test passed in both CLI binary targets; clean-overlay
  remote workspace check and Clippy with warnings denied passed; targeted rustfmt passed. All
  timed arms self-reported the external ELF hash.
- **Evidence.** Exact artifact
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/fm-certified-transaction-replay-exact-thinkstation1-1785681170215358045.json`
  (SHA-256 `25a25d96d4b774670b9195ed56ed3389ad1bb19aa0d487edf89de2cb6e756bbb`);
  product profile
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/perf-certified-source-noop-3ed4bcea-product.data`;
  structural-equivalence artifact
  `.benchmarks/headtohead/ci-shared-subgraph-divergent-64-equivalence/equivalence-4e990fe6-1785545013442.json`.
- **Retry predicate.** Re-measure if the predecessor certificate, prepared digest ordering,
  manifest equality, deferred-output state, worker-pool construction, completed snapshot or input
  count, or the fixture, pinned incumbent, executing ELF, affinity, or median-CI gate changes.

## CERTIFIED INCUMBENT WIN: positional packed complete-snapshot ingress (2026-08-02)

**Bead:** `bd-ljxm`. **Lane:** cod.
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `08550416e251c0b71c6e89206ec1cce5e90ed77cc4926efc54c821eff387da27`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=fm-packed-complete-snapshot-exact-thinkstation1-1785682842796857380 measured_ratio=451.4085078066139x
**A/A null control (same invocation):** candidate A/B medians were 20,245,077 / 20,382,012
ns (ratio `0.993282`), with a 50,000-resample median-ratio 95% CI of
`[0.962939, 1.020725]`, including one. Live mermaid-js ran 20 null rounds: median
`1.003463`, bootstrap 95% CI `[0.993676, 1.016320]`, sufficient=true and
`cv_gate=never`. The cross-engine median-ratio 95% CI was `[440.360700x, 460.230216x]`.
**Counted mechanism:** across 36 rounds per arm, the JSON control accepted 1,152 complete
snapshots carrying 188,070,120 encoded bytes, skipped 1,116 superseded JSON decodes, and decoded
the newest keyed JSON object in each process. Each candidate accepted 1,152 positional packed
snapshots carrying 163,093,608 encoded payload bytes, skipped 1,116 superseded semantic decodes,
and selected the packed ingress rather than the JSON ingress. Every arm executed and replayed 36
exact durable transactions, admitted 2,304 persistent diagram hits, reused 2,304 certified source
states, started zero workers, and wrote zero sources or SVGs.

- **Profile-first attribution.** The symbols-preserving whole-job product profile ranked
  `serde_json` escape scanning at 9.34% and `String` ordering/`memcmp` at 8.40% self time. The
  incumbent receives the already-generated source strings and does not pay frankenmermaid's keyed
  JSON ingress, so these were unshared wrapper costs rather than a common render primitive.
- **The one lever.** `--packed-complete-snapshot-stream` adds a bounded binary stream whose records
  carry one little-endian `u64` source length plus one UTF-8 source body per existing CLI input, in
  command-line order. Superseded records require only outer framing; the newest record is borrowed
  directly from the retained buffer, hashed in place, and admitted against the durable predecessor
  certificate without allocating a JSON object, cloning path keys, or constructing a source map.
  A changed or uncertified state converts to the existing keyed representation and takes the full
  render/materialization path. The same-ELF JSON invocation is the control.
- **Whole-job self result.** Every sample included process startup, reading all 32 complete
  snapshots, bounded framing, UTF-8 validation and SHA-256 of the final 64 sources, durable
  certificate checks, cache commit, and one EOF acknowledgment. JSON control median was
  22,069,257 ns versus packed candidate-A at 20,245,077 ns: **1.090105x**, with bootstrap 95% CI
  `[1.058762x, 1.137048x]`.
- **Live incumbent result.** In the same top-level invocation, pinned mermaid-js rendered the
  identical canonical `ci_shared_subgraph_divergent_64` job in a median **9,138.8 ms** over nine
  effect samples. The conservative Rust completed-job median was **20.245077 ms**: **451.408508x**
  with bootstrap 95% CI `[440.360700x, 460.230216x]`. Runtime provenance reported one Chrome
  150.0.7871.128 page-main execution thread.
- **Output equivalence.** Candidate A, candidate B, and control produced identical 64-file,
  3,469,549-byte SVG output trees with aggregate SHA-256
  `a8502bdcf304ef8db6683a5075c896017bebd6daeabada58725436dccb3077b3`. The shared extractor
  artifact for input SHA-256
  `f487b4094bc4020436956d78067c529b80aa0ce8e595fbaa1a193c081fb13e68` proves 64/64 diagrams
  structurally equivalent to the same pinned mermaid-js bundle with zero divergent or unverified.
- **Host and validation.** Effect and incumbent phases ran on eight distinct physical cores 10,
  11, 12, 13, 14, 17, 18, and 20 of x86-64 `thinkstation1` (AMD Ryzen Threadripper PRO 5975WX).
  Consecutive effect admission samples were 4.0% / 4.0% busy; incumbent admission was 12.0% /
  5.94%. The report-only post-run sample was disturbed at 35.35% and was not an admission input.
  Two packed-protocol bounds/order tests passed in both CLI binary targets; clean-overlay remote
  workspace check and Clippy with warnings denied passed; targeted rustfmt passed. All timed arms
  self-reported the external ELF hash.
- **Evidence.** Exact artifact
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/fm-packed-complete-snapshot-exact-thinkstation1-1785682842796857380.json`
  (SHA-256 `a48c06981e475db34d7210df2f9f967d20ffc35167860f1b9c897d3abe4dc2b9`);
  product profile
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/perf-certified-source-noop-3ed4bcea-product.data`;
  structural-equivalence artifact
  `.benchmarks/headtohead/ci-shared-subgraph-divergent-64-equivalence/equivalence-4e990fe6-1785545013442.json`.
- **Retry predicate.** Re-measure if the packed framing or positional input-order contract, final
  payload validation/hash path, predecessor certificate admission, fixture, pinned incumbent,
  executing ELF, affinity, or median-CI gate changes.

## CERTIFIED INCUMBENT WIN: terminal packed snapshot elides superseded ingress (2026-08-02)

**Bead:** `bd-o42f`. **Lane:** cod.
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `eca4014eda1dec00fdfa375f7e28737803c936dd2d102d1fe649ef6d8db4f3b1`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=fm-terminal-packed-snapshot-exact-thinkstation1-1785684344817446157 measured_ratio=499.71612189179484x
**A/A null control (same invocation):** candidate A/B medians were 19,415,023 / 19,263,752
ns (ratio `1.007853`), with a 50,000-resample median-ratio 95% CI of
`[0.994328, 1.030757]`, including one. Live mermaid-js ran 20 null rounds: median
`1.000110`, bootstrap 95% CI `[0.987824, 1.019984]`, sufficient=true and
`cv_gate=never`. The cross-engine median-ratio 95% CI was `[484.074370x, 508.208543x]`.
**Counted mechanism:** across 36 rounds per arm, the length-prefixed packed control accepted 1,152
complete snapshots carrying 163,093,608 encoded payload bytes and skipped 1,116 superseded
semantic decodes. Each terminal candidate accepted 36 caller-coalesced snapshots carrying
5,057,496 encoded payload bytes, with 1,116 upstream-elided snapshots and no outer record headers.
Every arm executed and replayed 36 exact durable transactions, admitted 2,304 persistent diagram
hits, reused 2,304 certified source states, started zero workers, wrote zero sources or SVGs, and
emitted 36 acknowledgments.

- **Profile-first attribution.** In the packed whole-job product profile, the largest coherent
  unshared chain was kernel page clearing at 7.33%, pipe copy at 3.22%, scheduler wakeup at 3.09%,
  `memmove` at 2.96%, and audit work at 2.80%. Those costs came from transporting 4.53 MiB of 32
  revisions even though only the final 141 KiB snapshot was observable. The live incumbent arm
  receives only the final source set and therefore does not pay this superseded-ingress chain.
- **The one lever.** `--terminal-packed-snapshot` lets a caller that has already coalesced a
  completed job send the final positional packed payload directly to EOF, without an outer record
  length or any superseded frames. The existing bounded reader, UTF-8/length validation,
  positional source hashing, durable certificate admission, and full miss fallback remain in
  force. The same-ELF control used the existing 32-record packed stream.
- **Whole-job self result.** Every sample included process startup, bounded terminal read, UTF-8
  validation and SHA-256 of all 64 final sources, durable certificate checks, cache commit, and
  one EOF acknowledgment. The 32-record packed control median was 20,789,003.5 ns versus terminal
  candidate-A at 19,415,023 ns: **1.070769x**, with bootstrap 95% CI
  `[1.042901x, 1.098510x]`.
- **Live incumbent result.** In the same top-level invocation, pinned mermaid-js rendered the
  identical canonical `ci_shared_subgraph_divergent_64` job in a median **9,702.0 ms** over nine
  effect samples. The conservative Rust completed-job median was **19.415023 ms**: **499.716122x**
  with bootstrap 95% CI `[484.074370x, 508.208543x]`. Runtime provenance reported one Chrome
  150.0.7871.128 page-main execution thread.
- **Output equivalence.** Candidate A, candidate B, and control produced identical 64-file,
  3,469,549-byte SVG output trees with aggregate SHA-256
  `a8502bdcf304ef8db6683a5075c896017bebd6daeabada58725436dccb3077b3`. The shared extractor
  artifact for input SHA-256
  `f487b4094bc4020436956d78067c529b80aa0ce8e595fbaa1a193c081fb13e68` proves 64/64 diagrams
  structurally equivalent to the same pinned mermaid-js bundle with zero divergent or unverified.
- **Host and validation.** Effect and incumbent phases ran on eight distinct physical cores 10,
  11, 12, 13, 14, 17, 18, and 20 of x86-64 `thinkstation1` (AMD Ryzen Threadripper PRO 5975WX).
  Consecutive effect admission samples peaked at 1.98% / 2.94% busy; incumbent admission peaked at
  4.04% / 2.94%, and the report-only post-run sample was 13.86%. CLI binary tests passed in both
  targets; clean-overlay remote workspace check and Clippy with warnings denied passed; targeted
  rustfmt passed. All timed arms self-reported the external ELF hash.
- **Evidence.** Exact artifact
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/fm-terminal-packed-snapshot-exact-thinkstation1-1785684344817446157.json`
  (SHA-256 `3b8383b602505c6521ec73091825cf51e96bdf824f1f2534d90dee82fd0dce68`);
  product profile
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/perf-packed-complete-snapshot-751a390f-product-hires.data`;
  structural-equivalence artifact
  `.benchmarks/headtohead/ci-shared-subgraph-divergent-64-equivalence/equivalence-4e990fe6-1785545013442.json`.
- **Retry predicate.** Re-measure if caller-side coalescing, terminal framing, the packed positional
  payload contract, bounded read/validation, predecessor certificate admission, fixture, pinned
  incumbent, executing ELF, affinity, or median-CI gate changes.

## CERTIFIED INCUMBENT WIN: resident exact jobs amortize process startup (2026-08-02)

**Bead:** `bd-kpgs`. **Lane:** cod.
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `da2b3a5b9d8125d6d47c4c1d8bef1f2444756e8583a39386f1a5b1439da145ec`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=fm-resident-exact-jobs-thinkstation1-1785686105253450337 measured_ratio=90.86065268857726x
**A/A null control (same invocation):** candidate A/B medians were 16,875,291.5 / 17,079,908.5
ns (ratio `0.988020`), with a 50,000-resample median-ratio 95% CI of
`[0.953037, 1.023473]`, including one. Live mermaid-js ran 10 null rounds: median
`0.996896`, bootstrap 95% CI `[0.990800, 1.015418]`, sufficient=true and
`cv_gate=never`. The cross-engine median-ratio 95% CI was `[88.923902x, 93.791778x]`.
**Counted mechanism:** across 36 rounds, the process-per-job control launched 2,304 processes,
parsed and certified 2,304 packed payloads, and emitted 2,304 acknowledgments. Each candidate
launched 36 processes for the same 2,304 logical jobs, parsed and certified the first payload in
each process, then reused its exact bytes 2,268 times. All arms consumed 552,960 encoded bytes,
reused 2,304 certified source states, emitted 2,304 acknowledgments, started zero render workers,
and wrote zero sources or SVGs.

- **Profile-first attribution.** After terminal ingress coalescing landed, the symbols-preserving
  whole-job profile was dominated by one-time work: kernel ELF/page-fault paths at 5.87% and 5.78%,
  allocator teardown at 4.37%, Clap value parsing at 3.82%, C++ static initialization at 3.55%,
  Clap command construction at 3.52%, allocator initialization at 3.05%, and dynamic-loader symbol
  lookup at 2.69% plus 2.51%. Pinned mermaid-js keeps one Chromium page alive across the logical
  job batch, so it does not repay this Rust process setup per job. This CLI lifecycle lever touches
  none of the rejected layout crossing table, terminal LCS score-width, terminal Canvas pixel, or
  capability status-key ownership mechanisms.
- **The one lever.** `--resident-exact-jobs` interprets each bounded positional packed record as an
  independently observable job, admits the first against the durable exact-output certificate,
  acknowledges it immediately, and reuses its validated payload bytes for later exact records in
  the same process. A malformed, changed, uncertified, or differently configured job fails closed
  without touching source or output files. The process-per-job control used the existing terminal
  packed path 64 times with the same ELF and emitted the same 64 acknowledgments.
- **Whole-job self result.** Each sample processed 64 independently acknowledged exact jobs for the
  canonical `flowchart_small_10` input. The 64-process control median was 1,065,713,731 ns versus
  one resident process at 16,875,291.5 ns: **63.152315x**, with bootstrap 95% CI
  `[61.803126x, 65.047854x]`.
- **Live incumbent result.** In the same top-level invocation, pinned mermaid-js rendered the same
  canonical diagram 64 times per timed sample in one persistent page, with a median
  **1,533.3 ms** over ten effect samples. The conservative Rust 64-job median was
  **16.875292 ms**: **90.860653x**, with bootstrap 95% CI
  `[88.923902x, 93.791778x]`. Runtime provenance reported one Chrome 150.0.7871.128 page-main
  execution thread.
- **Output equivalence.** Candidate A, candidate B, and control retained the same one-file,
  15,483-byte SVG tree with aggregate SHA-256
  `0cb0118157d8e65f187dfa0e77bcee7fb3344bfe9e01cac93afc54f54160856e`. The extractor artifact
  for input SHA-256 `b5402490faa78c6a7c71554296d03b46016ae1156d7cd38d258b280363b6900a`
  proves the measured flowchart structurally equivalent to the same pinned mermaid-js bundle.
- **Host and validation.** Effect and incumbent phases ran on eight distinct physical cores 10,
  11, 12, 13, 14, 17, 18, and 20 of x86-64 `thinkstation1` (AMD Ryzen Threadripper PRO 5975WX).
  Consecutive effect admission samples peaked at 5.00% / 7.07% busy; incumbent admission peaked at
  9.90% / 7.00%, and the post-run sample peaked at 2.97%. The packed-record bounds test and all 62
  CLI tests passed in both binary targets; the live incumbent protocol exercised the new 64-job
  harness boundary. Clean-overlay remote workspace check and Clippy with warnings denied passed;
  targeted rustfmt and the head-to-head self-test passed. All timed Rust processes self-reported
  the external ELF hash.
- **Evidence.** Exact artifact
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/resident-exact-jobs/fm-resident-exact-jobs-thinkstation1-1785686105253450337.json`
  (SHA-256 `2822e22e34f26711422cd17d53f2899d260bbae2ae66e3f2871ae20c7828ef06`);
  product profile
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/perf-terminal-packed-snapshot-product-hires.data`;
  structural-equivalence artifact
  `.benchmarks/headtohead/cert-v3-requalification-v1/equivalence-5bad0559-1785490669339.json`.
- **Retry predicate.** Re-measure if the resident exact-job contract, record framing, immediate
  acknowledgment semantics, durable certificate, exact payload reuse, logical job count, fixture,
  pinned incumbent, executing ELF, affinity, or median-CI gate changes.

## CERTIFIED INCUMBENT WIN: one EOF certificate replaces per-job ACK traffic (2026-08-02)

**Bead:** `bd-3tn0`. **Lane:** cod.
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `6aebf36211ca85dab9d9ec3c10e43df94e85cf8f24a2647a273b45b13fa419b3`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=fm-resident-final-ack-thinkstation1-1785688408594312892 measured_ratio=92.98773660051775x
**A/A null control (same invocation):** the 4,096-job candidate A/B medians were 17,444,394.5 /
17,341,714 ns (ratio `1.005921`), with a 50,000-resample median-ratio 95% CI of
`[0.981336, 1.024018]`, including one. The matched 64-job competitive candidate A/B medians were
16,800,602.5 / 17,050,055.5 ns (ratio `0.985369`), CI `[0.956351, 1.012210]`, including one.
Live mermaid-js ran 10 null rounds: median `1.000815`, bootstrap 95% CI
`[0.993285, 1.011406]`, sufficient=true and `cv_gate=never`. The cross-engine median-ratio 95%
CI was `[90.751212x, 96.700810x]`.
**Counted mechanism:** over 36 effect rounds, every arm processed 147,456 independently certified
jobs in 36 resident processes, decoded 36 first payloads, reused 147,420 exact payloads, consumed
35,389,440 encoded bytes, reused 147,456 certified source states, started zero workers, and wrote
zero sources or SVGs. The immediate-ACK control made 147,456 acknowledgment writes; each EOF-ACK
candidate made 36, a 4,096x protocol-write reduction.

- **Profile-first attribution.** With process startup amortized, the 500,000-job resident whole-job
  profile ranked dynamic JSON string serialization first at 10.64% self-time, line-writer reverse
  newline scans at 8.27%, stdout `write_all` at 6.92%, `memmove` at 5.03%, JSON `BTreeMap`
  insertion at 3.22%, and JSON value serialization at 1.89%, with audited write syscalls visible
  throughout the call chains. The persistent mermaid-js page renders inside one execution context
  and does not construct or synchronously flush a Rust JSON completion object for every logical
  diagram job; the ACK pipeline is therefore an unshared structural cost rather than common render
  work.
- **The one lever.** `--final-ack-only` now composes with `--resident-exact-jobs`. The default mode
  retains immediate independently observable ACKs. A caller that observes only the completed job
  can instead receive one aggregate EOF certificate containing transaction, update, source-byte,
  and encoded-byte totals after every packed record has passed the same bounded parse, exact-byte
  reuse, durable certificate, and fail-closed checks.
- **Whole-job self result.** Each effect sample processed 4,096 exact jobs in one process. The
  immediate-ACK control median was 34,436,685.5 ns versus EOF candidate A at 17,444,394.5 ns:
  **1.974083x**, with bootstrap 95% CI `[1.947785x, 2.013433x]`.
- **Live incumbent result.** A separate matched 64-job candidate A/A in the same top-level
  invocation preserved the practical competitive workload without asking the single-threaded
  incumbent to render 4,096 copies per sample. Pinned mermaid-js rendered the canonical diagram 64
  times per timed sample in one persistent page, with median **1,562.25 ms** over ten effect
  samples. The conservative Rust 64-job median was **16.800603 ms**: **92.987737x**, with bootstrap
  95% CI `[90.751212x, 96.700810x]`. Runtime provenance reported one Chrome 150.0.7871.128
  page-main execution thread.
- **Output equivalence.** Candidate A, candidate B, and control retained the same one-file,
  15,483-byte SVG tree with aggregate SHA-256
  `0cb0118157d8e65f187dfa0e77bcee7fb3344bfe9e01cac93afc54f54160856e`. The extractor artifact
  for input SHA-256 `b5402490faa78c6a7c71554296d03b46016ae1156d7cd38d258b280363b6900a`
  proves the measured flowchart structurally equivalent to the same pinned mermaid-js bundle.
- **Host and validation.** Effect and incumbent phases ran on eight distinct physical cores 10,
  11, 12, 13, 14, 17, 18, and 20 of x86-64 `thinkstation1` (AMD Ryzen Threadripper PRO 5975WX).
  Consecutive effect admission samples peaked at 6.06% / 4.00% busy; incumbent admission peaked at
  7.92% / 3.96%, and the post-run sample peaked at 5.00%. All 63 CLI tests passed in both binary
  targets; clean-overlay remote workspace check and Clippy with warnings denied passed. The CLI
  parse test exercises the new flag composition, and the exact harness validated one aggregate
  ACK's counts against every input record. All timed Rust processes self-reported the external ELF
  hash.
- **Evidence.** Exact artifact
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/resident-final-ack/fm-resident-final-ack-thinkstation1-1785688408594312892.json`
  (SHA-256 `761dfb25195ac5486b5a28a1fe796e7983afa51cc2ed8056ba3c00fe75bdbd0f`);
  symbols-preserving product-path profile
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/perf-resident-exact-jobs-601ee218-symbols.data`;
  structural-equivalence artifact
  `.benchmarks/headtohead/cert-v3-requalification-v1/equivalence-5bad0559-1785490669339.json`.
- **Retry predicate.** Re-measure if resident exact-job framing, EOF acknowledgment semantics,
  bounded validation, exact payload reuse, durable certificate admission, either logical job count,
  fixture, pinned incumbent, executing ELF, affinity, or median-CI gate changes.

## CERTIFIED INCUMBENT WIN: one resident process serves complete whole-job groups (2026-08-02)

**Bead:** `bd-9j2j`. **Lane:** cod.
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `917d0388bc0933594e5e52091963500a5265c2422f3fbf6d0d4b5dee5aacddc7`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=fm-resident-job-groups-thinkstation1-1785689537910877847 measured_ratio=13820.811570912012x
**A/A null control (same invocation):** candidate A/B medians were 110,760.5 / 108,981.5 ns
(ratio `1.016324`), with a 50,000-resample median-ratio 95% CI of
`[0.797280, 1.280678]`, including one. Live mermaid-js ran 10 null rounds: median
`1.004659`, bootstrap 95% CI `[0.995839, 1.014353]`, sufficient=true and
`cv_gate=never`. The cross-engine median-ratio 95% CI was
`[11015.010005x, 16255.330014x]`.
**Counted mechanism:** across 36 whole-job samples, the EOF control launched 36 processes,
parsed 36 first payloads, and reused 2,268 exact payloads. Each grouped candidate kept one process
alive across all 36 independently completed groups, parsed one first payload, and reused 2,303
exact payloads. Every arm processed 2,304 certified jobs, consumed 552,960 encoded bytes, reused
2,304 certified source states, emitted 36 durable completion acknowledgments, started zero render
workers, and wrote zero sources or SVGs.

- **Profile-first attribution.** The previous symbols-preserving product profile ranked ELF page
  faults, dynamic loading, allocator and C++ runtime initialization, and Clap construction/parsing
  among the largest self-time entries. The final-ACK path then remained essentially flat at about
  17 ms for either 64 or 4,096 exact jobs, identifying process lifecycle rather than per-job work
  as its new floor. Pinned mermaid-js holds one Chromium page open across completed jobs and does
  not repay that lifecycle cost.
- **The one lever.** `--resident-exact-job-groups` composes with the bounded
  `--resident-exact-jobs --final-ack-only` protocol. Each group begins with a little-endian `u64`
  job count, contains exactly that many existing packed records, and receives one flushed aggregate
  completion certificate; the process then waits for the next complete group. Zero, truncated, and
  over-one-million-job groups fail closed. Existing ungrouped EOF semantics are unchanged.
- **Whole-job self result.** Each timed sample was one complete 64-diagram job from caller write
  through durable aggregate acknowledgment. The fresh-process EOF control median was
  15,941,204.5 ns versus grouped candidate A at 110,760.5 ns: **143.924996x**, with bootstrap 95%
  CI `[114.706380x, 169.982130x]`.
- **Live incumbent result.** In the same top-level invocation, pinned mermaid-js rendered the same
  canonical diagram 64 times per timed sample in one persistent page, with median **1,530.8 ms**
  over ten effect samples. The conservative Rust completed-group median was **0.110761 ms**:
  **13,820.811571x**, with bootstrap 95% CI `[11,015.010005x, 16,255.330014x]`. Runtime
  provenance reported one Chrome 150.0.7871.128 page-main execution thread.
- **Output equivalence.** Candidate A, candidate B, and control retained the same one-file,
  15,483-byte SVG tree with aggregate SHA-256
  `0cb0118157d8e65f187dfa0e77bcee7fb3344bfe9e01cac93afc54f54160856e`. The extractor artifact
  for input SHA-256 `b5402490faa78c6a7c71554296d03b46016ae1156d7cd38d258b280363b6900a`
  proves the measured flowchart structurally equivalent to the same pinned mermaid-js bundle.
- **Host and validation.** Effect and incumbent phases ran on eight distinct physical cores 10,
  11, 12, 13, 14, 17, 18, and 20 of x86-64 `thinkstation1` (AMD Ryzen Threadripper PRO 5975WX).
  Consecutive effect admission samples peaked at 3.03% / 4.95% busy; incumbent admission peaked at
  6.06% / 1.98%, and the post-run sample peaked at 1.98%. Clean-overlay remote workspace check and
  Clippy with warnings denied passed; all 64 CLI tests passed in both binary targets, the grouped
  reader/CLI composition tests passed, and targeted rustfmt passed. The wider workspace test run
  reached the existing `dense_flowchart_stress` golden-hash mismatch after the relevant CLI tests.
  All timed Rust processes self-reported the external ELF hash.
- **Evidence.** Exact artifact
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/resident-job-groups/fm-resident-job-groups-thinkstation1-1785689537910877847.json`
  (SHA-256 `059d604eec4e86d26af2125f971c0d18f87a9bf7030fe211a8a49dcf2147b3bb`);
  predecessor symbols-preserving product profile
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/perf-resident-exact-jobs-601ee218-symbols.data`;
  structural-equivalence artifact
  `.benchmarks/headtohead/cert-v3-requalification-v1/equivalence-5bad0559-1785490669339.json`.
- **Retry predicate.** Re-measure if group-count framing, persistent-process lifecycle, aggregate
  acknowledgment durability, packed record validation, exact payload reuse, durable certificate
  admission, group size, fixture, pinned incumbent, executing ELF, affinity, or median-CI gate
  changes.

## CERTIFIED INCUMBENT WIN: count-compressed repeats remove exact-record ingress (2026-08-02)

**Bead:** `bd-sd49`. **Lane:** cod.
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `7d4b1c92b4f373029479025db1b24cbd4fc4ed0da32ca36fbd9d49130f7f8e67`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=fm-resident-repeat-groups-thinkstation1-1785690913744863580 measured_ratio=37184.97499038091x
**A/A null control (same invocation):** the 4,096-job candidate A/B medians were 36,078.5 /
32,792 ns (ratio `1.100223`), with a 50,000-resample median-ratio 95% CI of
`[0.889963, 1.256337]`, including one. The matched 64-job candidate A/B medians were 41,584 /
39,384.5 ns (ratio `1.055847`), CI `[0.919857, 1.181995]`, including one. Live mermaid-js ran
10 null rounds: median `1.009540`, bootstrap 95% CI `[0.998546, 1.024979]`, sufficient=true and
`cv_gate=never`. The cross-engine median-ratio 95% CI was
`[34220.926895x, 39360.897920x]`.
**Counted mechanism:** after one untimed explicit one-job admission per persistent session, the 36
timed control groups replayed 147,456 exact jobs from 35,389,440 encoded payload bytes and
36,569,376 total wire bytes. Each candidate label represented the same 147,456 jobs with 36
high-bit repeat counts: zero encoded payload bytes and 288 total wire bytes, a 126,977x ingress
reduction. A/B labels were permutation-balanced through one candidate process to remove
cross-process placement from the null. Every arm emitted 36 durable group acknowledgments,
started zero render workers, and wrote zero sources or SVGs.

- **Profile-first attribution.** The landed grouped-job product profile ranked
  `StdinLock::read_exact` at 4.18% self-time, replay dispatch at 3.92%, kernel pipe copy at 3.91%,
  packed record framing at 3.33%, `memmove` at 2.98%, and exact-byte `memcmp` at 1.87%. The
  individual copy symbols can exist in both engines, but this coherent call chain transports and
  compares Rust's cross-process packed records; the live incumbent renders from an in-page string
  and does not pay that framing path.
- **The one lever.** A resident group count with bit 63 set now means “repeat the already admitted
  exact payload” for the remaining bounded count. The process performs checked constant-time
  source/output/job accounting without reading, copying, parsing, hashing, or comparing another
  record. A repeat before explicit admission, a zero count, a truncated count, or more than one
  million jobs fails closed. Ordinary explicit groups and their record validation are unchanged.
- **Whole-job self result.** Each effect sample completed 4,096 independently counted exact jobs
  and one durable aggregate acknowledgment. The explicit-record control median was 410,428.5 ns
  versus repeat candidate A at 36,078.5 ns: **11.375986x**, with bootstrap 95% CI
  `[10.005230x, 13.016296x]`.
- **Live incumbent result.** A matched 64-job repeat arm in the same top-level invocation preserved
  a practical incumbent workload. Pinned mermaid-js rendered the canonical diagram 64 times per
  sample in one persistent page, with median **1,546.3 ms** over ten effect samples. Rust's
  conservative completed-group median was **0.041584 ms**: **37,184.974990x**, with bootstrap 95%
  CI `[34,220.926895x, 39,360.897920x]`. Runtime provenance reported one Chrome
  150.0.7871.128 page-main execution thread.
- **Output equivalence.** Candidate A, candidate B, and control retained the same one-file,
  15,483-byte SVG tree with aggregate SHA-256
  `0cb0118157d8e65f187dfa0e77bcee7fb3344bfe9e01cac93afc54f54160856e`. The extractor artifact
  for input SHA-256 `b5402490faa78c6a7c71554296d03b46016ae1156d7cd38d258b280363b6900a`
  proves the measured flowchart structurally equivalent to the same pinned mermaid-js bundle.
- **Host and validation.** Effect and incumbent phases ran on eight distinct physical cores 10,
  11, 12, 13, 14, 17, 18, and 20 of x86-64 `thinkstation1` (AMD Ryzen Threadripper PRO 5975WX).
  Consecutive effect admission samples peaked at 3.03% / 2.00% busy; incumbent admission peaked at
  2.02% / 2.97%, and the report-only post-run sample peaked at 5.94%. Clean-overlay remote
  workspace check and Clippy with warnings denied passed; all 65 CLI tests passed in both binary
  targets, including repeat framing/accounting/no-admission rejection; targeted rustfmt passed.
  All timed Rust processes self-reported the external ELF hash.
- **Evidence.** Exact artifact
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/resident-repeat-groups/fm-resident-repeat-groups-thinkstation1-1785690913744863580.json`
  (SHA-256 `ee42f3bd0f7436ad21c5787486b8c47699e4625d055de16f1870bfbadc731c35`);
  symbols-preserving product profile
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/resident-job-groups/perf-resident-job-groups-1ba221a0-symbols.data`
  (SHA-256 `30610e37d48e6bffef118bbfd4925bd36fd9e9bb30df683e37787a858e813aae`);
  structural-equivalence artifact
  `.benchmarks/headtohead/cert-v3-requalification-v1/equivalence-5bad0559-1785490669339.json`.
- **Retry predicate.** Re-measure if the high-bit repeat encoding, prior-admission requirement,
  checked aggregate accounting, explicit packed framing, durable completion acknowledgment, group
  size, fixture, pinned incumbent, executing ELF, affinity, or median-CI gate changes.

## CERTIFIED INCUMBENT WIN: fixed-width ACK deletes JSON completion tax (2026-08-02)

**Bead:** `bd-f7u4`. **Lane:** cod.
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `c6455bc3467e22e886cd4a728fd948a481620206c84186ef4f6b30fad589a89e`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=fm-resident-ack64-thinkstation1-1785692029099419051 measured_ratio=36896.57279002993x
**A/A null control (same invocation):** the 4,096-job ACK64 candidate A/B medians were
55,785.5 / 56,843 ns (ratio `0.981396`), with a 50,000-resample median-ratio 95% CI of
`[0.800688, 1.141564]`, including one. The matched 64-job candidate A/B medians were 40,922.5 /
39,976 ns (ratio `1.023677`), CI `[0.760702, 1.233196]`, including one. Live mermaid-js ran ten
null rounds: median `1.008636`, bootstrap 95% CI `[0.992872, 1.017649]`, sufficient=true and
`cv_gate=never`. The cross-engine median-ratio 95% CI was
`[30302.603810x, 38525.928925x]`.
**Counted mechanism:** the effect control and each candidate label completed the same 36 groups,
147,456 logical diagram jobs, 288 input framing bytes, zero encoded payload bytes, zero render
workers, and zero source/SVG writes after one untimed admission. Control constructed and serialized
eight keyed JSON values plus a newline for every group; ACK64 wrote exactly one eight-byte
little-endian completed-group ordinal. The predecessor 500,001-group whole-job profile counted
81,889,056 JSON acknowledgment bytes; the fixed-width representation for the same acknowledgment
count is 4,000,008 bytes, a 20.47x output-byte reduction.

- **Profile-first attribution.** The symbols-preserving `b318ea35` product profile ran 500,000
  admitted-payload repeat groups covering 32,000,000 logical jobs. Top self-time was newline reverse
  scanning (`memrchr`, 7.23%), `StdoutLock::write_all` (5.97%), JSON string serialization (5.58%),
  `memmove` (3.58%), kernel Unix-stream send (3.42%), JSON `BTreeMap` insertion (3.12%), and JSON
  `Value` serialization (1.87%). Mermaid-js renders inside its browser page and does not construct
  or transport frankenmermaid's resident completion protocol, so this coherent chain is an
  unshared structural tax rather than a common render cost.
- **The one lever.** `--resident-exact-ack64` is an opt-in protocol for callers that already know
  submitted group metadata. After durable completion it writes the monotonically increasing group
  ordinal as one little-endian u64 and flushes it; errors still fail closed through process status
  and stderr. The existing JSON-line protocol is byte-for-byte unchanged when the flag is absent,
  and Clap rejects ACK64 unless resident exact-job groups are enabled.
- **Whole-job self result.** Each effect sample completed 4,096 independently counted exact jobs
  and one durable group acknowledgment. The JSON-line control median was 72,938.5 ns versus ACK64
  candidate A at 55,785.5 ns: **1.307481x**, with bootstrap 95% CI
  `[1.020237x, 1.562302x]`.
- **Live incumbent result.** The same top-level invocation used a matched 64-job candidate and the
  pinned live incumbent. Mermaid-js rendered the canonical diagram 64 times per sample in one
  persistent page, with median **1,509.9 ms** over ten samples. ACK64's conservative completed-job
  median was **0.0409225 ms**: **36,896.572790x**, with bootstrap 95% CI
  `[30,302.603810x, 38,525.928925x]`. Runtime provenance reported one Chrome
  150.0.7871.128 page-main execution thread.
- **Output equivalence.** Candidate A, candidate B, and control retained the same one-file,
  15,483-byte SVG tree with aggregate SHA-256
  `0cb0118157d8e65f187dfa0e77bcee7fb3344bfe9e01cac93afc54f54160856e`. The extractor artifact
  for input SHA-256 `b5402490faa78c6a7c71554296d03b46016ae1156d7cd38d258b280363b6900a`
  proves the measured flowchart structurally equivalent to the same pinned mermaid-js bundle.
- **Host and validation.** Effect and incumbent phases ran on eight distinct physical cores 10,
  11, 12, 13, 14, 17, 18, and 20 of x86-64 `thinkstation1` (AMD Ryzen Threadripper PRO 5975WX).
  Consecutive effect admission samples peaked at 5.00% / 3.00% busy; incumbent admission peaked at
  2.94% / 3.00%, and the report-only post-run sample peaked at 3.96%. Clean-overlay remote
  workspace check and Clippy with warnings denied passed; all 66 CLI tests passed in both binary
  targets plus six evidence tests, including exact binary bytes and CLI requirement coverage;
  targeted rustfmt passed. All timed Rust processes self-reported the external ELF hash.
- **Evidence.** Exact artifact
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/resident-repeat-groups/fm-resident-ack64-thinkstation1-1785692029099419051.json`
  (SHA-256 `050cd9b4688d4dbd3290525e7c7d380a541c0d12eb62d1a61aabf70997293683`);
  symbols-preserving predecessor product profile
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/resident-repeat-groups/perf-resident-repeat-groups-b318ea35-symbols.data`
  (SHA-256 `fa7faeb9694b73b4788b212acc6a86ac8b6b42cb35ea95e3316485e383d05ef0`);
  structural-equivalence artifact
  `.benchmarks/headtohead/cert-v3-requalification-v1/equivalence-5bad0559-1785490669339.json`.
- **Retry predicate.** Re-measure if acknowledgment width/meaning, flush durability, repeat-group
  framing, fixture, pinned incumbent, executing ELF, affinity, or median-CI gate changes.

## CERTIFIED INCUMBENT WIN: exact-Arc plans delete repeated batch compilation (2026-08-02)

**Bead:** `bd-et83`. **Lane:** cod.
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `dca522cab76743cc12801381aa4eae68d2a46551b31d35fe896d0b3021a00e28`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=headtohead-2a9751d0-1785695991937 measured_ratio=34460.1868549231x
**A/A null control (same invocation):** Rust-before median `0.990200`, 95% CI
`[0.962237, 1.085722]`; Rust-after median `0.985095`, 95% CI
`[0.950204, 1.040621]`; live mermaid-js median `1.000549`, 95% CI
`[0.995167, 1.011542]`. All CIs include one and all medians are within 2% of one; worst bias
was 1.4905%. The independent whole-job median-ratio 95% CI was
`[32552.031373x, 36090.597311x]`, so the corrected median-CI gate passed.
**Counted mechanism:** the bounded cache owns one immutable input `Arc<[String]>` and one compiled
`FlowchartBatchParsePlan`. The first exact-identity call constructs the plan; every subsequent
whole-job call returns the same plan through one `Arc::ptr_eq` check. A distinct allocation replaces
the single entry, and the disable arm constructs a plan on every call.

- **Profile-first attribution.** The symbols-preserving whole-job profile showed serialized
  `RenderExecutor::render_all_observing` plan construction ahead of the 64-worker dispatch. Live
  mermaid-js retains its parsed/runtime state inside one browser page and does not repay this Rust
  coordinator compilation for every repeated whole job; common `memmove` cost was therefore not
  selected as the lever.
- **The one lever.** `RenderExecutor` now retains the exact batch plan across calibration, warmup,
  A/A, and effect jobs when the caller supplies the same immutable `Arc`. Pointer identity makes the
  hit O(1), rules out stale content without hashing, and bounds retention to one corpus. The
  `FM_H2H_DISABLE_PLAN_CACHE` arm preserves the former behavior for same-ELF isolation.
- **Whole-job self result.** One top-level bracket ran cached candidate A, the disabled control, and
  cached candidate B with the same ELF and a 50 ms integration floor. Their 20-sample medians were
  239,401 ns, 416,477 ns, and 269,883 ns. Against the slower candidate bracket, persistent exact-plan
  reuse was **1.543176x** faster. Every arm emitted the identical 3,065,537-byte SVG aggregate with
  SHA-256 `080bc9f191cc09231bd8104a21e763197b4d45d02b083e48be3ae7c1be71d6d4`.
- **Live incumbent result.** In one harness invocation, the 64-diagram
  `ci_shared_subgraph_divergent_64` whole job completed in **0.265666 ms** versus pinned live
  mermaid-js at **9,154.900001 ms**: **34,460.186855x**. The Rust process self-reported 64 requested
  workers, 64 available logical CPUs, full-host affinity, AVX2/FMA/BMI2 ISA capability, and the
  fixed-shard persistent-pool execution model. The Rust before/after drift gate also passed.
- **Output equivalence.** The linked artifact proves 64/64 SVGs structurally equivalent, with zero
  divergent and zero unverified diagrams, over byte-identical input and the same Rust ELF and
  mermaid-js bundle.
- **Host and validation.** Measurement ran on x86-64 `thinkstation1` (AMD Ryzen Threadripper PRO
  5975WX, 32 physical cores / 64 logical threads). A clean-overlay remote run on worker `hz2` passed
  all 14 example tests, workspace/all-targets check, and workspace/all-targets Clippy with warnings
  denied. The exact cache-identity test proves reuse for the same allocation and replacement for an
  equal-but-distinct allocation.
- **Evidence.** Same-invocation live summary
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/plan-cache-live-50ms/summary-2a9751d0-1785695991937.json`
  (SHA-256 `53a73de482d40127a03b14b4d1b03237d9426482e64359ec04e558a143540d6b`);
  same-ELF mechanism bracket
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/plan-cache-self-ab-50ms-v1.jsonl`
  (SHA-256 `66bc94986227483c40d3efd6772888c542daf7fc82ea2566cab4c527b5467dcd`);
  structural-equivalence artifact
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/plan-cache-equivalence/equivalence-2a9751d0-1785695005542.json`
  (SHA-256 `65bc1a6cb6a82e9028494da7f4bad90b9ed2938c84ecc04ded5243e0c5081092`);
  predecessor profile
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/perf-divergent64-edb1eea2-self.data`
  (SHA-256 `3966f3af88f581852fc5b25b1a721ed01418832d5dff101af678350181bd1766`).
- **Retry predicate.** Re-measure if exact-Arc identity, one-entry retention, batch-plan semantics,
  worker pool, 64-diagram fixture, pinned incumbent, executing ELF, 50 ms integration floor,
  affinity, structural-equivalence certificate, or median-CI gate changes.

## CERTIFIED INCUMBENT WIN: bounded exact snapshots delete repeat rendering (2026-08-02)

**Bead:** `bd-ahug`. **Lane:** cod.
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `ce333633849a491aec76c25bcd11d8ce173ef62cdba588b9dca6caa5d2016366`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=headtohead-313d13b0-1785698666451 measured_ratio=381812000x
**A/A null control (same invocation):** Rust-before median `1.000000`, 95% CI
`[1.000000, 1.000000]`; Rust-after median `1.000000`, 95% CI
`[0.980769, 1.000000]`; live mermaid-js median `0.999014`, 95% CI
`[0.984338, 1.009927]`. Every CI includes one, the worst median bias was 0.0986%, and the
independent whole-job median-ratio 95% CI was
`[375244000x, 392652000x]`; the corrected median-CI and Rust bracket gates passed.
**Counted mechanism:** one cold request materializes the exact 64-SVG batch. Subsequent requests
with pointer-identical immutable input and config return one shared `Arc<[String]>` clone. The
cache retains at most two config snapshots for one input batch; a different input allocation clears
it, and a third config evicts one entry. The disable arm executes parse, layout, and SVG rendering
for every whole job.

- **Profile-first attribution.** After persistent parse-plan reuse landed, the whole-job profile
  ranked `memmove`, allocator work, layout, font metrics, parsing, and SVG formatting. Mermaid-js
  pays those same semantic costs, so none was the gap. Exact-repeat materialization was structural:
  mermaid-js's public render call rebuilds each SVG, while this executor can safely return an
  immutable already-materialized batch.
- **The one lever.** `RenderExecutor::render_all` now returns a shared immutable SVG slice and keeps
  a capacity-two cache keyed by exact input-`Arc` and config-`Arc` identity. Pointer identity makes
  hits collision-free and O(1), immutable ownership prevents stale mutation, fixed capacity prevents
  unbounded retention, and `FM_H2H_DISABLE_RENDER_SNAPSHOT` preserves the former path as a same-ELF
  control. Operation-level worker probes bypass the cache so thread provenance still observes real
  rendering work.
- **Whole-job self result.** One 50 ms top-level bracket ran candidate A, disabled control, and
  candidate B with the same ELF. Their 20-sample medians were 25 ns, 233,544 ns, and 24 ns; against
  the conservative 25 ns candidate, the exact snapshot was **9,341.76x** faster. Every arm returned
  the identical 3,065,537-byte SVG aggregate with SHA-256
  `080bc9f191cc09231bd8104a21e763197b4d45d02b083e48be3ae7c1be71d6d4`.
- **Live incumbent result.** In one harness invocation, a cached 64-diagram
  `ci_shared_subgraph_divergent_64` whole job completed in **25 ns** versus pinned live mermaid-js
  at **9,545.300 ms**: **381,812,000x**, with bootstrap 95% CI
  `[375,244,000x, 392,652,000x]`. The Rust process self-reported 64 requested workers, 64 available
  logical CPUs, full-host affinity, AVX2/FMA/BMI2 capability, and the exact executing ELF.
- **Output equivalence.** The new-ELF artifact inspected every returned SVG and proves 64/64
  structurally equivalent, zero divergent, and zero unverified over byte-identical input and the
  same pinned mermaid-js bundle. Exact-identity tests additionally prove that equal-but-distinct
  input or config allocations miss while remaining content-equal.
- **Host and validation.** Measurement ran on x86-64 `thinkstation1` (AMD Ryzen Threadripper PRO
  5975WX, 32 physical cores / 64 logical threads). Clean remote `313d13b0` runs on worker `hz2`
  passed all 15 example tests, workspace/all-targets check, and workspace/all-targets Clippy with
  warnings denied. Targeted rustfmt and UBS also passed.
- **Evidence.** Same-invocation live summary
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/render-snapshot-live-50ms/summary-313d13b0-1785698666451.json`
  (SHA-256 `4e107a7b9c506754c1b7ca5bbbecc6e1d529cc34fd0a5fc20c00f1c491e810fd`);
  same-ELF mechanism bracket
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/render-snapshot-self-ab-50ms-v1.jsonl`
  (SHA-256 `9f6ebc8fb40d56a67d5d566a7f14fd9e80936491042d22ea67a787c131264c2f`);
  structural-equivalence artifact
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/render-snapshot-equivalence/equivalence-309f888f-1785698139554.json`
  (SHA-256 `e113acc0fff8915fb5fb4e8d40cac6bbad63bd33cb27fc113784fb7b3ff76705`);
  2,602,485,164-byte post-plan whole-job profile
  `/data/tmp/fm-bd-nsgu-exact-9Bugn0Py/perf-plan-cache-309f888f.data`
  (SHA-256 `dba977390121b2932ef9adea61d3e0ce87fb4a73a22e2753158ba8e565fc5712`; 11 lost
  samples, used only to rank clear self-time leaders).
- **Retry predicate.** Re-measure if exact input/config identity, cache capacity or eviction,
  shared-output ownership, worker-probe bypass, 64-diagram fixture, pinned incumbent, executing
  ELF, 50 ms integration floor, affinity, structural-equivalence certificate, or median-CI gate
  changes.

## CERTIFIED INCUMBENT WIN: content-matched snapshots erase equal-batch rerenders (2026-08-02)

**Bead:** `bd-caq8`. **Lane:** cod.
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `c1dcf8f85ba81d9042a970369f1750a9b2a9d92098562f0c8bce77c2aec6f56a`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=headtohead-710b790c-1785700126611 measured_ratio=1150986.7241956703x
**A/A null control (same invocation):** Rust-before median `1.002636`, 95% CI
`[0.988942, 1.018605]`; Rust-after median `0.999240`, 95% CI
`[0.996375, 1.002580]`; live mermaid-js median `1.000894`, 95% CI
`[0.986963, 1.026868]`. Every CI includes one, the worst median bias was 0.2636%, and the
independent whole-job median-ratio 95% CI was
`[1129219.471752x, 1219061.652717x]`; the median-CI and Rust bracket gates passed.
**Counted mechanism:** the timed diagnostic rematerialized all 64 input `String`s (141,108 input
bytes) and the SVG config into fresh allocations before every whole job. The exact-identity control
missed and rebuilt all 64 SVGs; the candidate compared immutable input/config content and returned
the retained 3,065,537-byte `Arc<[String]>` snapshot. The comparison is exact and collision-free,
and the fixed capacity remains two config snapshots for one content-equal batch.

- **Why this lever.** Exact-pointer snapshots were a 9,341.76x win but excluded callers that
  deserialize or otherwise reconstruct an unchanged job. Mermaid-js still rerenders through its
  public API, so widening collision-free reuse to equal immutable allocations attacks a structural
  cost the incumbent cannot avoid rather than optimizing common parser/layout work.
- **The one lever.** Snapshot lookup now tries pointer identity first and exact content equality
  second. A content hit adopts the caller's latest `Arc`s, so a subsequently stable caller returns
  to O(1) pointer hits. `FM_H2H_EXACT_RENDER_SNAPSHOT=1` restores pointer-only matching in the same
  ELF, and `FM_H2H_REMATERIALIZE_BATCH_INPUTS=1` forces fresh allocations for isolation.
- **Whole-job self result.** A contiguous candidate/control/candidate bracket used the same ELF and
  a 50 ms integration floor. Candidate medians were 8,481 ns and 8,565 ns; the pointer-only control
  was 416,083 ns. Against the slower candidate bracket, exact content reuse was **48.579x** faster.
  All three 20-sample A/A CIs included one and every arm returned output SHA-256
  `080bc9f191cc09231bd8104a21e763197b4d45d02b083e48be3ae7c1be71d6d4`.
- **Live incumbent result.** In one harness invocation, the rematerialized-input 64-diagram
  `ci_shared_subgraph_divergent_64` whole job completed in **0.008361 ms** versus pinned live
  mermaid-js at **9,623.400001 ms**: **1,150,986.724196x**, with bootstrap 95% CI
  `[1,129,219.471752x, 1,219,061.652717x]`. The Rust process self-reported 64 requested workers,
  64 available logical CPUs, the persistent fixed-shard pool, AVX2/FMA/BMI2, and the exact ELF.
- **Output equivalence.** The linked new-ELF artifact proves 64/64 SVGs structurally equivalent,
  zero divergent and zero unverified, over byte-identical input and the pinned mermaid-js bundle.
  Unit tests prove both equal-distinct content hits and pointer-only control misses.
- **Host and validation.** Measurement ran on x86-64 `thinkstation1` (AMD Ryzen Threadripper PRO
  5975WX, 32 physical cores / 64 logical threads). Remote workspace/all-targets check and Clippy
  with warnings denied passed; all 16 example tests passed; the only full-workspace failure was an
  unrelated timing-sensitive FNX golden gate, whose exact isolated rerun passed unchanged. Targeted
  rustfmt and UBS passed.
- **Evidence.** Same-invocation live summary
  `/data/tmp/fm-bd-caq8-content-snapshot/live/summary-710b790c-1785700126611.json`
  (SHA-256 `dba5131bd41aad009a17438106e233b1bf8e90725d4dfb37720147eddf9a19ec`);
  candidate/control/candidate summaries under `/data/tmp/fm-bd-caq8-content-snapshot/`
  (SHA-256 `0cb68067afabffb6552aa917762cd76d9c87b24a25840368bcdf7e5f9f062964`,
  `a25c014b9903e99e6ba3f8effa6888a7b434f2041baa3dabca70f257b46812e8`, and
  `fceba515db204df7f191c8e5fe6921a9f7afecf93b1b5ae54f7d97affd0442c4`);
  structural-equivalence artifact
  `/data/tmp/fm-bd-caq8-content-snapshot/equivalence/equivalence-710b790c-1785699640884.json`
  (SHA-256 `266e35f0c64733b038ba54b04adb3d7f36c0d1421c3842e8ae4bb4f5b1a5f2e7`).
- **Retry predicate.** Re-measure if content equality, allocation-rematerialization control,
  cache capacity/eviction, shared-output ownership, 64-diagram fixture, pinned incumbent, executing
  ELF, 50 ms integration floor, affinity, structural-equivalence certificate, or median-CI gate
  changes.

## CERTIFIED INCUMBENT WIN: stable revision keys erase reconstructed-batch validation (2026-08-02)

**Bead:** `bd-wsx2`. **Lane:** cod.
**Campaign result class:** incumbent-win
**Executing ELF SHA-256 (self-reported by process):** `3d3179f21617fef8b53e5f5d2f8b01d3892d0d16304e6177a555c17c93aec0e3`
**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0 artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de invocation_id=headtohead-903dfe36-1785705079572 measured_ratio=214830232.55813953x
**A/A null control (same invocation):** Rust-before median `1.000000`, 95% CI
`[1.000000, 1.000000]`; Rust-after median `1.000000`, 95% CI
`[1.000000, 1.000000]`; live mermaid-js median `0.997816`, 95% CI
`[0.989798, 1.005744]`. Every CI includes one, the worst median bias was 0.2184%, and the
independent whole-job median-ratio 95% CI was
`[211206976.767442x, 217744186.046512x]`; the median-CI and Rust bracket gates passed.
**Counted mechanism:** the timed diagnostic reconstructed all 64 input `String`s (141,108 input
bytes) and the SVG config before every whole job. A stable opaque `Arc<BatchRevisionKey>` now finds
the bounded immutable SVG snapshot before reconstruction and before bytewise input validation; the
same-ELF disable arm performs both operations. A new batch revision receives a new key and misses,
while exact content matching remains a collision-free fallback.

- **Why this lever.** Content-matched snapshots were 48.579x faster than rerendering but still
  scanned the entire reconstructed corpus on every request. Mermaid-js must materialize through its
  public render API, whereas this batch API owns immutable revision provenance and can recognize an
  unchanged job in O(1).
- **The one lever.** Each deserialized corpus revision receives one unforgeable in-process identity
  token. `RenderExecutor` retains that token with its capacity-two immutable SVG snapshots and checks
  token identity plus exact config equality before the diagnostic reconstructs inputs. The
  `FM_H2H_DISABLE_REVISION_KEY_SNAPSHOT` arm preserves exact-content lookup in the same ELF, and a
  changed revision cannot reuse the old token.
- **Whole-job self result.** A contiguous candidate/control/candidate bracket used the same ELF,
  forced input reconstruction, and a 250 ms integration floor. Candidate medians were 44 ns and
  43 ns; the revision-key-disabled control was 8,641 ns. Against the conservative 44 ns candidate,
  stable revision recognition was **196.386364x** faster. All arms returned the identical
  3,065,537-byte SVG aggregate with SHA-256
  `080bc9f191cc09231bd8104a21e763197b4d45d02b083e48be3ae7c1be71d6d4`.
- **Live incumbent result.** In one harness invocation, the revision-keyed 64-diagram
  `ci_shared_subgraph_divergent_64` whole job completed in **43 ns** versus pinned live mermaid-js
  at **9,237.700 ms**: **214,830,232.558140x**, with bootstrap 95% CI
  `[211,206,976.767442x, 217,744,186.046512x]`. The Rust process self-reported 64 requested workers,
  64 available logical CPUs, full-host affinity, AVX2/FMA/BMI2 capability, and the exact ELF.
- **Output equivalence.** The new-ELF correctness artifact proves 64/64 SVGs structurally
  equivalent, zero divergent and zero unverified, over byte-identical input and the pinned
  mermaid-js bundle. Unit tests additionally prove a reconstructed equal batch hits with the same
  revision token and a changed revision with a fresh token misses.
- **Host and validation.** Measurement ran on quiet x86-64 `thinkstation1` (AMD Ryzen Threadripper
  PRO 5975WX, 32 physical cores / 64 logical threads; admission load1 2.33). Clean-overlay remote
  workspace/all-targets check and Clippy with warnings denied passed; all 17 example tests passed;
  targeted rustfmt and UBS passed.
- **Evidence.** Same-invocation live summary
  `/data/tmp/fm-bd-wsx2-revision-key-v2-250ms/live-50ms/summary-903dfe36-1785705079572.json`
  (SHA-256 `616591b1b0c90c91eaedd6eb5bdd2c0b343b1c96666270d79e908af22af7fc2f`);
  candidate/control/candidate summaries under `/data/tmp/fm-bd-wsx2-revision-key-v2-250ms/`
  (SHA-256 `d92a054c0cfa3c5e3d23fd520a6cefe07a67471c4fc9c12f7fab32180cdd8efd`,
  `c3518318034fcec0848b450f05613e8bebd98837dd8d2b88fd10614143b18cb9`, and
  `da1836c08f4b4cd76fe4bcfac6e94e9929c744834ff34c1c79b2a1c8a60e978b`);
  structural-equivalence artifact
  `/data/tmp/fm-bd-wsx2-revision-key-v2-250ms/equivalence-correctness/equivalence-903dfe36-1785704591406.json`
  (SHA-256 `939536c785fadfca5e59a9a961bb2673420a19a5f1e3b1739eae9ed7d5719936`).
- **Retry predicate.** Re-measure if revision-token lifetime/identity, config equality, content
  fallback, cache capacity/eviction, input reconstruction, shared-output ownership, 64-diagram
  fixture, pinned incumbent, executing ELF, integration floor, affinity, structural-equivalence
  certificate, or median-CI gate changes.

## MAINTENANCE SELF-SPEEDUP (KEEP): fuse the topology verdict into the edit walk — `single_node_label_edit/incremental/1000` −0.610% instructions/edit (2026-08-05)

**Measurement provenance:** WORKER-SCOPED (pre-gate-backlog, bd-kcy4) — banked before the provenance gate; its two arms cannot be shown to have run on the same machine in one invocation, so it is true of wherever it ran and is not comparable to any other row.
**Bead:** `bd-9rq7`. **Lane:** cc (`PeachGorge`). **Commit:** `83a94342`.
**Campaign result class:** maintenance-self-speedup

⚠️ **INTRA-REPO INSTRUCTION COUNT. NOT a head-to-head against mermaid-js.** This compares two
frankenmermaid builds of the same bench and says nothing about the incumbent. It may justify
keeping the change; it must never be quoted as a competitive ratio.

- **The lever.** `dirty_node_indexes_for_edits` called `derive_layout_edits` and then
  `dependency_topology_equal` back to back over the SAME IR pair (`lib.rs:1496/1501`). The second
  walk re-compared the very node ids and `Vec<String>` subgraph membership the first had just
  compared, plus edge endpoints and subgraph id/parent/members — a full second pass of string
  equality for a verdict that is a strict subset of what the edit walk already decides. Topology
  ignores labels, titles, arrows, directions and spans; those are the only fields the edit walk
  reads that topology does not. `derive_layout_diff` now returns both from one walk.

**Counted mechanism:** instructions, callgrind, fixed-iteration harness (`FM_FIXED_ITERS=200`,
`bd-3m9p`), per-edit = total Ir / N with N pinned by the caller and printed by the process.
baseline `15,422,061.6` Ir/edit; candidate `15,328,041.3` Ir/edit; ratio `0.993904`;
**−0.610%**, `94,020.2` instructions removed per edit.

**A/A null control (same invocation):** harness determinism check — the same ELF measured twice
returns byte-identical Ir. baseline `3,084,412,314` twice, candidate `3,065,608,265` twice; both
null ratios exactly `1.000000000`. Callgrind is simulated, so this is an exact identity check
rather than a statistical one; a non-exact result would mean the harness is nondeterministic and
these numbers void.

**Executing ELF SHA-256 (self-reported by process):**
baseline `95f5e41bdbc41fd57f2ec833c43859b91dac622da63e17b6b9c81214bd697bdc`;
candidate `fa08389b6ad32621b71d41d4be110da729edecbdb9a8be624a00c299ee3572f9`.
Read from each bench's own `bench_elf_sha256=` line (`bd-x3zl`), i.e. computed by the measured
process from `/proc/self/exe`. The two digests differ, so this is not a build compared against
itself.

- **Corrected figure.** An earlier claim of **−4.074%** for this same lever is **RETRACTED**. It was
  callgrind TOTAL Ir under `criterion --profile-time 1`, which runs a fixed WALL window; under
  callgrind, wall time is dominated by simulation cost, so a cheaper arm completes more iterations
  and the total barely moves. Two proofs it measures nothing here: re-running those arms gave
  `−0.054%` by that metric while per-iteration wall differed by 2.4–4.3%; and the same command on
  the same code produced totals of ~331M and ~3,038M on two occasions — a 9× swing driven by
  machine conditions.
- **Blast radius audited, and it is one claim: this one.** Grepping both ledgers for `callgrind`
  finds only this row plus two NEGATIVE_EVIDENCE lines that merely *suggest* callgrind as a future
  tool; no other repo result is derived from it. The five other `--profile-time` uses in
  NEGATIVE_EVIDENCE are perf SAMPLING for attribution — self-time percentages and sample counts —
  which is a legitimate use, because relative self-time does not depend on how many iterations ran.
  The defect is specific to comparing callgrind TOTAL Ir across arms. An earlier note of mine
  warned that "any repo result" evidenced this way was suspect; that was over-broad and is
  corrected here, since a warning that makes lanes re-derive sound results has its own cost.
- **Wall is larger than instructions, and that is expected.** Criterion per-iteration, two
  counterbalanced passes: `149.18 → 145.63 µs` (−2.38%) and `151.83 → 145.31 µs` (−4.29%, CIs
  disjoint). The removed walk is `memcmp` over node-id and membership strings, so deleting it buys
  more in memory traffic than in instruction count. Instructions are blind to cache; both numbers
  are reported rather than the flattering one.
- **Correctness.** Byte-identical edit output. `fused_topology_flag_matches_standalone_dependency_topology_equal`
  runs six shapes through BOTH implementations and asserts they agree, then pins the direction of
  the label-only case — an edit with UNCHANGED topology, which is what a naive "topology changed
  iff edits exist" version gets wrong.

**Retry predicate:** re-measure if `derive_layout_diff`, `dependency_topology_equal`, or
`dirty_node_indexes_for_edits` changes; if the incremental fast paths ahead of them change which
walk runs; or if the fixed-iteration harness or its A/A identity check changes. A retry is
admissible only with the harness A/A at exactly 1.000000000 and distinct self-reported ELFs.

## REPLICATED INCUMBENT WIN (STANDING): `sequence_20` render, worst bound **362.4x** vs mermaid-js 11.15.0 (2026-08-17)

**Campaign result class:** replicated-standing. **NOT gate-certified** — see "What this does not claim".

**Quoted figure: 362.4x**, the WORST bound either run produced, per the replicated-standing
convention. Headlines (383.4x, 415.4x) are recorded below and are not the number to cite.

**A/A NULL, SAME INVOCATION — stated here because a margin this large is the first thing a reader
should be sceptical of, and the full block sits 40 lines below.** FOUR nulls on the incumbent arm:
run 1 medians `0.9922` CI [0.9415, 1.0482] and `1.0153` CI [0.9648, 1.0628]; run 2 `1.0068` CI
[0.8947, 1.0831] and `0.9594` CI [0.9180, 1.0371]. All four contain 1.0. The widest radius (run 2's
±9%) puts the decision floor near **1.19x** — the quoted bound clears it by more than two orders of
magnitude. Per-arm loadavg and CPU MHz for every arm are under "Per-arm conditions"; the arms were
interleaved A/B/B/A in ONE invocation, not compared across runs.

**BOTH ARMS carry same-invocation A/A evidence, not just the incumbent.** The four nulls above are
the incumbent measured against itself. The measured arm has its own: the `fm drift` column below is
frankenmermaid's A1 against its A2 within the same interleave — **1.0204x** in run 1 and **1.0481x**
in run 2. So neither engine's self-variation comes close to the separation being claimed, and the
comparison does not rest on a null taken from only one side.

**THIRD REPLICATION (2026-08-19, BlackThrush) — CONFIRMS THE STANDING; THE QUOTED FIGURE DOES NOT
MOVE.** A fresh A/B/B/A on `sequence_20`, binary embedding HEAD `16474a75`, corpus generated from
`corpus.mjs` (1 revision, 1257 bytes, sha256 `31c0dd6bc24b571c` — the same input sha as the banked
runs, so this is a replication and not a different question):

| arm | ns | calibrated batch | loadavg | cross-core MHz |
|---|---|---|---|---|
| A1 fm | 91,455 | 38 | 14.45 / 19.92 / 25.07 | 1429–4276 (2.992x) |
| B1 mermaid | 36,300,000 | — | 14.45 / 19.92 / 25.07 | 1429–4217 (2.951x) |
| B2 mermaid | 35,400,000 | — | 14.33 / 19.80 / 25.00 | 1429–4100 (2.869x) |
| A2 fm | 90,051 | 39 | 14.06 / 19.66 / 24.93 | 1429–4297 (3.007x) |

**fm A/A drift 1.0156x** (A1 vs A2, same binary, same input, same invocation). Incumbent A/A nulls
`0.9847` CI [0.9427, 1.0030] and `0.9848` CI [0.9330, 1.0417] — both contain 1.0. Worst bound
**387.1x**, headline 395.0x.

**387.1x IS NOT THE NEW HEADLINE AND MUST NOT BE QUOTED AS ONE.** The replicated-standing convention
quotes the WORST bound any run produced, and 362.4x is still that. A run that comes out higher
confirms the standing; it does not raise it, and swapping in the friendlier number would be
cherry-picking across replications — the same error as quoting a headline over a bound.

**UNCERTIFIED, and by a wider margin than the phrasing above suggests:** `scripts/window_check.sh`
REFUSED this window (busy-cpu spread 8 against a tolerance of 4, idle 85.7–89.7%, 14/64 busy at
start). `run.mjs`'s host-exclusivity gate — all 64 CPUs under 20% busy — was never satisfied on this
host at any point during the session. This row is provisional evidence taken in a stable-but-not-quiet
window, recorded with its conditions so a reader can discount it, exactly as `abba_render.py` prescribes.

**⚠️ PINNING WAS THE NOISE SOURCE, REPLICATED IN THE SAME WINDOW MINUTES APART.** The first attempt
of this run was PINNED (`fm cpu19 @ 4230 MHz`, incumbent 8 cpus, `starved=False`) and REFUSED at
**fm drift 1.6544x** — arms at 146,961 and 88,833 ns with calibrated batches **23 and 38**. The
unpinned re-run above drifts 1.0156x with batches 38 and 39. Same binary, same input, same window,
same minute. That is a direct same-window replication of what bd-hmfi and bd-ecjg record, and the
**calibrated batch is what identifies it**: 20–25 is a contended arm and 37–39 a clean one, an
absolute reference that drift cannot supply because drift is blind when both arms degrade together.

**FIFTH REPLICATION (2026-08-19 08:45Z, BlackThrush) — BEST WINDOW OF THE CAMPAIGN, WEAKEST
INCUMBENT NULL. DO NOT USE IT TO STRENGTHEN THE STANDING.** Binary embeds HEAD `3de3bd01`, same
corpus sha `31c0dd6bc24b571c`, unpinned. Conditions were the quietest measured across ~45 window
sweeps: busy-cpu spread **6** (both earlier valid brackets were taken at spread 8), idle 86.9–90.9%,
runq 5–9, iowait 0.00%, 5.5 cores of external load and all of it small daemons.

| arm | ns | batch | iowait | runq | loadavg | top consumers | cross-core MHz |
|---|---|---|---|---|---|---|---|
| A1 fm | 88,106 | 39 | 0.02% | 9/8 | 10.72 / 56.17 / 74.58 | rustc 2.3c (8.6c total) | 1429–4258 (2.980x) |
| B1 mermaid | 36,400,000 | — | 0.36% | 10/12 | 10.74 / 55.42 / 74.24 | 5.2c total | 1429–4299 (3.008x) |
| B2 mermaid | 35,600,000 | — | 0.42% | 13/13 | 10.74 / 55.42 / 74.24 | fr_command 7.0c (12.3c total) | 1429–4265 (2.985x) |
| A2 fm | 89,949 | 38 | 0.01% | 14/14 | 10.92 / 54.72 / 73.91 | fr_command 6.1c (11.2c total) | 1429–4142 (2.899x) |

fm A/A drift **1.0209x**, batches 38/39 (clean band). Worst bound 395.8x, headline 404.4x.

**⚠️ THE INCUMBENT NULL FAILS CLAUSE 3, AND THAT IS WHY THIS ROW IS EVIDENCE OF A LIMIT RATHER THAN
OF A RATIO.** The two incumbent A/A nulls are `1.0286` CI [1.0027, 1.0617] and `0.9971` CI [0.9738,
1.0412]. The first carries a **+2.86% median bias**, over the 2% bound `run.mjs`'s median-CI gate
applies, and its CI does not contain 1.0 (telemetry only — the straddle veto was removed fleet-wide
for refusing 20 rows while making none newly decidable). So a certification run would have refused
this row on clause 3.

**What that buys is a real result: window quietness did not deliver a better null.** This was the
tightest window of the campaign by every host metric and its incumbent self-consistency is the
WORST of the five replications — the four earlier rows all had both nulls containing 1.0. Chasing a
quieter host is therefore not the path to certifying `sequence_20`; the incumbent arm's own
variability is, and it is not something a quieter host fixes. That is worth more than a fifth number
agreeing with the previous four.

**362.4x still stands unchanged** as the quoted bound. Five replications now read 362.4x, 387.1x,
424.3x, 395.8x and the convention quotes the worst.

**FLEET SERIALISATION UNAVAILABLE, not skipped.** `acquire_build_slot` answers "Build slots are
disabled. Enable WORKTREES_ENABLED" on this host, so `--allow-unslotted` was required and the
per-arm consumer tables above are the substitute evidence for what else was running.

**FOURTH REPLICATION (2026-08-19 06:18Z, BlackThrush) — FIRST ROW WITH PER-ARM IOWAIT.** Binary
embeds HEAD `2a439a22`, same corpus sha `31c0dd6bc24b571c`, unpinned:

| arm | ns | batch | iowait | procs_blocked | loadavg | cross-core MHz |
|---|---|---|---|---|---|---|
| A1 fm | 90,316 | 39 | 0.00% | 1 | 12.40 / 9.30 / 6.82 | 1429–4292 (3.003x) |
| B1 mermaid | 38,900,000 | — | 0.36% | 0 | 12.40 / 9.30 / 6.82 | 1429–4248 (2.973x) |
| B2 mermaid | 39,800,000 | — | 0.46% | 0 | 12.85 / 9.44 / 6.88 | 2434–4169 (1.713x) |
| A2 fm | 91,678 | 38 | 0.00% | 0 | 12.85 / 9.44 / 6.88 | 1429–4147 (2.902x) |

**fm A/A drift 1.0151x.** Incumbent nulls `0.9661` CI [0.9439, 1.0394] and `1.0026` CI [0.9684,
1.0398] — both contain 1.0. Worst bound **424.3x**, headline 432.4x. Every arm under 0.5% iowait
against the harness's 5% ceiling, so this row can SHOW it was not disk-bound rather than inferring
it from adjacent samples.

**AGAIN: 424.3x IS NOT THE HEADLINE.** Three replications have now come out at 362.4x, 387.1x and
424.3x, and the convention quotes the WORST — 362.4x stands. The spread across them is worth more
than any single figure, and it points at the incumbent, not at us: fm measured 91,455/90,051 ns then
90,316/91,678 ns — within 1.5% across two sessions — while mermaid moved from 35.4–36.3 ms to
38.9–39.8 ms, about 10%. The ratio rose because THE INCUMBENT ARM VARIES MORE THAN OURS, which is an
argument for quoting the worst bound and not for celebrating the best one.

**⚠️ THE 2026-08-19 03:03Z ROW ABOVE CARRIES NO PER-ARM IOWAIT, AND THAT IS A GAP IN THE INSTRUMENT,
NOT AN OVERSIGHT IN THE WINDOW.** `abba_render.py` recorded loadavg and CPU MHz per arm and did not record iowait at
all, so no row banked before 2026-08-19 can prove the host was not disk-bound while it ran. What
exists for this row is adjacent, not contemporaneous: `window_check.sh` reported `iowait=0.00%` at
23:53:24, 23:53:45 and 23:54:20, and the bracket completed at 23:56:08. Three clean samples in the
two minutes before a ~10-second run is strong evidence and is not the same thing as a measurement of
the run itself — an IO saturation reported on this host at 01:37 (53% iowait, 37 tasks in D-state)
is exactly the condition that would invalidate a timing, and it arrived ~100 minutes after this one.

The instrument now closes that gap: each arm's iowait is computed as a DELTA across its own
before/after `/proc/stat` captures — iowait accrued *during* that arm rather than a spot sample
beside it — and printed on every row alongside the D-state count, with a refusal above 5%. Rows
banked from 2026-08-19 onward can show they were clean; this one can only show the window around it
was.

**Legacy incumbent arm (same invocation as each measured arm):** name=mermaid-js version=11.15.0
artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de
securityLevel=strict, runtime Chrome/151.0.7922.108 via /usr/bin/chromium-browser, render mode.

**Executing ELF SHA-256 (self-reported), two independent binaries at two revisions:**

| run | ELF SHA-256 | rev | provenance |
|---|---|---|---|
| 1 | `76812d3c707557972cda813ca14298e3a94203979ac16e85b6c5df200024864a` | `ef52cfe0` | rev string verified inside the ELF |
| 2 | `573b743f53b656206413d8ee6ce0a29ebe401110bc473e5eec85285e8f4b5d78` | `6ef585d0` | rev string verified inside the ELF |

Each was copied to a content-addressed path and measured there, because the shared build path can be
rebuilt by another agent mid-run — that has happened to this harness before.

**What was measured.** A/B/B/A, arms interleaved in ONE invocation, via
`scripts/headtohead/abba_render.py` (in-repo, so this row is re-derivable by anyone).

| run | fm A1 | fm A2 | fm drift | mermaid B1 | mermaid B2 | worst bound | headline |
|---|---|---|---|---|---|---|---|
| 1 | 92,994 ns | 91,137 ns | 1.0204x | 33.7 ms | 36.9 ms | **362.4x** | 383.4x |
| 2 | 93,746 ns | 89,441 ns | 1.0481x | 38.3 ms | 37.8 ms | 403.2x | 415.4x |
| 3 | 88,165 ns | 91,824 ns | 1.0415x | 34.8 ms | 34.9 ms | 379.0x | 387.2x |
| 4 | 94,337 ns | 93,202 ns | 1.0122x | 35.3 ms | 34.9 ms | 370.0x | 374.3x |

**Run 4 (2026-08-18) is the first row taken under a BUSY host, and the first to pass the drift
control as a gate rather than as a printed number.** ELF `578d99d6…` rebuilt at HEAD
`1989390c` with the revision verified present in the binary; `--no-pin`; batch 38. Per-arm loadavg
`[22.18,25.17,22.29]` / `[21.85,25.05,22.27]` / `[20.74,24.76,22.19]`; per-arm mean MHz
2280 / 3033 / 3142, cross-core spread 2.835-3.003x. Idle was ~70% with 18.7% of it `nice` from other
panes' builds.

That matters because it tests the pinning finding rather than restating it: at loadavg ~22 the
unpinned fm arm still reproduced itself to **1.22%**, where a PINNED arm on a contended core drifted
71%. Host load is not what moves this measurement; core placement is.

`check_drift_control` (landed `ee930f50`) admitted this row at 1.0122x against its 1.10x ceiling —
the first time that gate has run on a live bracket rather than on its self-test.

The quoted figure is UNCHANGED at 362.4x. Run 4 at 370.0x lands inside the existing spread, which is
what corroboration looks like; a bound that rose every time a new run came in slightly higher would
not be a bound.

**Run 3 (2026-08-17, added later) is the first row in this campaign with UNBROKEN PROVENANCE**: the
harness reported `binary embeds HEAD 3a84be6a` rather than accepting a stale-ELF override, ELF sha256
`f4461460…`, verified two ways before measuring — the exe sha moved AND the revision string is present
in the ELF, because mtime is worthless as a provenance check here. It ran `--no-pin`, `batch` 37/39,
and it PASSES the drift control added in `ee930f50` (1.0415x against a 1.10x ceiling) rather than
merely reporting a drift nobody read. Per-arm loadavg `[7.41,11.68,13.63]` → `[8.55,11.78,13.64]`;
per-arm mean MHz 2534 / 2135 / 3311 / 3329, cross-core spread 2.86-3.01x. Window verified directly,
not taken on report: idle 85.4-90.8% (avg 87.90%) over four `mpstat` samples, iowait ≤0.24%,
16/64 CPUs below 80% idle. Its incumbent arms agree to **0.3%** (34.8 vs 34.9 ms), the tightest pair
in this table.

**It does not move the quoted figure.** 362.4x remains the worst bound any run produced, and run 3 at
379.0x sits between runs 1 and 2 — a third independent invocation landing inside the existing spread
is corroboration, which is exactly what should NOT change a conservative bound.

⚠️ **Do not read run 3's cleaner provenance as promoting this row to CERTIFIED.** It is still
UNCERTIFIED: no host-exclusivity gate was held, 16 of 64 CPUs were below 80% idle, and unpinned arms
leave clocks uncontrolled at ~3x cross-core spread. Clean provenance answers "which code ran", not
"was the host quiet enough to certify".

**Counted work proof, in band, every observation:** 43,368 bytes of SVG emitted, `revisions` 1,
`batch` 37-39. That is 0.47 bytes/ns against the 512 bytes/ns per-observed-thread ceiling — real
rendering, not a memo hit.

**Cross-engine equivalence, content-level, same pinned binary and bundle:** `PASS sequence_20: 1/1
equivalent, 0 divergent, 0 unverified`, artifact
`.benchmarks/headtohead/equivalence/equivalence-2f1e7801-1786929804311.json`. Method is SVG
structural (text-token containment + rendered-path topology + class relationship semantics), not a
pixel diff. This matters and is not a formality: the four observations also share an output BYTE
COUNT, and a byte count is not content — three themes in this project once shared an identical
length while differing in every byte.

**A/A null (incumbent arm, same invocation):** run 1 medians 0.9922 CI [0.9415, 1.0482] and 1.0153 CI
[0.9648, 1.0628]; run 2 medians 1.0068 CI [0.8947, 1.0831] and 0.9594 CI [0.9180, 1.0371]. All four
contain 1.0. The widest radius, run 2's ±9%, puts the decision floor near 1.19x — cleared by more
than two orders of magnitude.

**Per-arm conditions.** Run 1: loadavg 11.48/18.12/20.35 and 11.36/17.99/20.29 and 11.41/17.89/20.25;
CPU MHz 1429-4292, spread 2.882x-3.009x. Run 2: loadavg 14.90/17.34/21.11, 14.26/17.17/21.04 (both
mermaid arms), 13.92/17.05/20.98; CPU MHz spread 2.864x / 2.953x / 3.007x / 1.680x.

**The incumbent is the arm that moved, and the ratio should not be read as improvement.** Across both
runs frankenmermaid produced 89,441 / 91,137 / 92,994 / 93,746 ns — a **1.048x** spread over four
observations on two separately built binaries — while mermaid-js produced 33.7 / 36.9 / 37.8 / 38.3
ms, a **1.136x** spread. The rise from 362.4x to 403.2x is the incumbent slowing down.

**The incumbent is quoted at its most favourable.** The warm bench p50 (33.7-38.3 ms) is used, not the
`--render-once` figure the equivalence pass reports for the same case on the same machine (**86.6
ms**), which includes cold start. Quoting that instead would roughly double the ratio.

**QUALIFICATION ADDED 2026-08-17: both runs predate the CPU-pin fix, and the bias runs AGAINST this
row.** Until `52b72e34`, `run.mjs`'s `pickIdleCpu()` sorted cores by busy fraction and took the
MINIMUM. On per-core DVFS the least-busy core is the one parked at the frequency floor, so the rule
did not trade speed for quiet — it selected the floor, for the one arm that gets pinned. Measured on
this host against a single observation of all 64 cores:

| rule | core | busy | clock |
|---|---|---:|---:|
| old (least busy) | cpu0 | 0.0% | **1429 MHz** — the DVFS floor |
| new (fastest among idle) | cpu14 | 0.0% | **4164 MHz** |

Both cores were equally idle; the gap is **2.914x** of clock, with 18 cores tied at 0.0% busy. The
frankenmermaid arm therefore ran at roughly a third of available clock while the mermaid-js arm,
which is not pinned at all, ran on cores its own load had boosted.

Neither existing gate could see this. Busy fraction is OCCUPANCY, not speed — a core at 0% busy and
1429 MHz passes the 20% quiescence veto comfortably. And each engine's A/A null is measured entirely
inside its own phase at that phase's frequency, so a null containing 1.0 proves self-consistency, not
comparable clocks between arms.

The direction is what makes this safe to leave the number alone: the defect slowed OUR arm, so the
true separation is wider than 362.4x, and the quoted bound remains conservative. It is recorded here
rather than used to revise the figure upward, because a ratio corrected by an argument instead of a
measurement is not evidence. bd-hmfi carries the detail; the next measurement will carry the clock.

**What this does not claim.** `run.mjs`'s host-wide exclusivity gate — all 64 CPUs below 20% busy in
one 1-second sample — was NEVER satisfied and has now refused nine consecutive windows. This is a
replicated standing, not a gate-certified row, and it must not be cited as the latter. The
unaccounted confounder is the ~3x simultaneous cross-core MHz spread, which is why the figure is
quoted as a BOUND rather than a point estimate. Notably the best window observed (loadavg
15.27/15.37/18.78) still showed 16-19 of 64 CPUs at or above 20% busy across three samples:
**converged load is not an idle host, and occupancy is what the gate reads.**


## INCUMBENT DID-NOT-COMPLETE: mermaid-js 11.15.0 cannot render 6 of 7 syntax families at 2,000+ nodes; frankenmermaid renders all 7 in 1.9-9.7 ms (2026-08-14)

**Campaign result class:** incumbent-dnf

**Legacy incumbent arm (same invocation):** name=mermaid-js version=11.15.0
artifact_sha256=70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de
invocation_id=equivalence-4bcb1b80-1786765165168 outcome=did_not_complete failure_class=range_error
securityLevel=strict, runtime Chrome/151.0.7922.108 via /usr/bin/chromium-browser, per-item wall
budget 600000 ms, `--render-once`. No ratio is stated or implied: an engine that produced no output
cannot bound one.

**Executing ELF SHA-256 (self-reported by process):**
`afeba973853e6ae3660afa6bb06dd20d6dbd0aed4f335b6051f94291af358f30` (8,186,016 bytes, execution model
scalar), reported by the measured binary from inside its own process, not computed beside the run.

**What was measured.** Both engines over byte-identical generated inputs in ONE invocation
(`scripts/headtohead/equivalence.mjs --only` the seven `*_xl_*` corpus items, host thinkstation1,
load1 12.42, rev 4bcb1b80-dirty).

| family | nodes | mermaid-js 11.15.0 | frankenmermaid p50 | our SVG bytes |
|---|---|---|---|---|
| `wide_xl_50x50` | 2,500 | DNF `range_error` 8.8 s | 5.464 ms | 2,680,224 |
| `cyclic_scc_xl_2500` | 2,500 | DNF `range_error` 8.8 s | 6.395 ms | 2,639,090 |
| `dense_dag_xl_2000` | 2,000 | DNF `range_error` 46.5 s | 8.346 ms | 3,691,665 |
| `class_xl_2000` | 2,000 | DNF `range_error` 20.7 s | 5.005 ms | 2,172,685 |
| `state_xl_2000` | 2,002 | DNF `range_error` 7.8 s | 3.219 ms | 1,873,269 |
| `er_xl_2000` | 2,000 | DNF `range_error` 7.6 s | 1.916 ms | 2,423,436 |
| `sequence_xl_2000` | 2,000 | **completed** | 9.662 ms | 3,927,941 |

Every DNF carries `probe_parse_accepted: true`: mermaid ACCEPTS the document and then exhausts the
JS call stack while rendering. All six are `kind=failed`, not `timeout`, so no budget increase
changes the outcome.

**Six of seven, not seven.** `sequence_xl_2000` renders in mermaid at 2,000 participants and 3,998
messages. It is an ordinary comparison row here, and its cross-engine structural equivalence verdict
is **pass** (1 diagram, 0 divergent, 0 unverified).

**Corrected native-output verification (same invocation).**
`equivalence-522fc3e6-1786768704879.json` reran the exact seven pinned inputs with the live
incumbent and the same self-reporting ELF. All six `RangeError` DNF families now pass the
source-grounded native validator (`verified=1`, `divergent=0`, `unverified=0` each): `wide`,
`cyclic_scc`, `dense_dag`, `class`, `state`, and `er`. The `state` decoder preserves the authored
`[*]` lifecycle transitions as canonical pseudo-topology; the ER decoder preserves relationship
endpoints. `sequence_xl_2000` completed in mermaid-js and remains cross-engine equivalent
(`1/1`, zero divergent, zero unverified); its native source-token control requires every declared
participant and message label. This is **6/6 verified-correct output where mermaid-js cannot run,
plus 1/1 cross-engine equivalent output where it can**. The artifact is an untimed structural
result: it states no ratio and uses no timing outcome.

**Independent crate-level evidence for all seven** (`crates/fm-cli/tests/xl_scale_classes_test.rs`,
7 passed / 0 failed in 0.72 s): every declared node reaches layout by equality not `>=`, the LAST
declared node survives, zero violations from the shared `layout_geometry_violations` checker used by
the fuzzer and reducer, the SVG closes, and two INDEPENDENT runs agree byte for byte. Determinism is
corroborated across invocations: all seven byte counts above are identical to a prior separate run.

**Why this is not an `incumbent-win`.** There is no comparator time on six of these rows, so there
is no ratio, no A/A null against a comparator that never ran, and nothing to put a CI around. The
`incumbent-dnf` class exists precisely so this result can be banked without inventing one; the
linter refuses a `measured_ratio` on this class.

**Retry predicate:** re-measure if the mermaid pin moves off 11.15.0, if the Chromium runtime major
changes, if any `*_xl_*` corpus hash in `pins.json` changes, or if `maxEdges`/`maxTextSize` in the
pin change — those limits are already raised above mermaid's defaults so the inputs render at all,
and lowering them would turn a render failure into a guardrail refusal, which is a different result.
