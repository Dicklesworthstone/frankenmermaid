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

## SEMANTIC ADMISSION ONLY: four demoted class-mixed whole jobs are equivalence-clean (2026-07-31)

**Bead:** `bd-jqko`. **Lane:** cod (`LavenderMill`).
**Executing ELF SHA-256 (self-reported by process):**
`499e4bf6b23ef6bfe4fccf9df4a086274d51f42ebe28b2f4ae5cc0c249a4eb47` (7,897,696 bytes).
**A/A null control (same invocation):** incomplete by construction and therefore not a performance
gate. The scalar Rust dump arms reported medians `1.016480` (`doc_build_40`), `0.989018`
(`ci_batch_500`), `0.994573` (`docs_site_50`), and `1.009611` (`docs_site_200`), each inside the
corrected `[0.98, 1.02]` clause. The untimed `--render-once` incumbent dump arm did not collect
mermaid-js A/A samples, so no ratio, win, loss, or corrected-null performance verdict exists. Any
future timed invocation must independently keep every arm's null median in `[0.98, 1.02]` and pass
the bootstrap-CI and 2x-null-margin clauses; CV remains provenance only.

- **Why this rerun was obligatory.** The older artifacts for `doc_build_40` and `ci_batch_500`
  predated class Tier-2 adjudication, while the historical numeric rows for all four jobs were
  demoted after the exact-output gate exposed missing class/state work. The class-member,
  relationship-kind, state-label, and inheritance retry predicates are now closed, so the exact
  named corpora—not filtered substitutes—were rerun through the current oracle.
- **Exact result.** `doc_build_40` is **40/40 equivalent**, `ci_batch_500` **500/500**,
  `docs_site_50` **50/50**, and `docs_site_200` **200/200**, with zero divergent and zero
  unverified diagrams in every row. All 129 class diagrams received Tier-2 relationship
  adjudication and passed. The pinned input SHA-256 values are respectively
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
  Every engine dumped every expected revision, and each concatenated dump hash exactly matches
  that engine's self-reported output SHA-256. The live incumbent is mermaid-js `11.15.0`, bundle
  SHA-256 `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`,
  through `/usr/bin/chromium-browser` (`Chrome/150.0.7871.128`).
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
