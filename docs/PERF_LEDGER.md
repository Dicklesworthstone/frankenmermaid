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
