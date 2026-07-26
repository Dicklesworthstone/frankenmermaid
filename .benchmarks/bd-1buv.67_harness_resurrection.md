# bd-1buv.67 — bench-contract adoption, VOID resurrection, and frontier keep

**Campaign:** `perf-campaign-20260725`
**Lane:** cod / HARNESS+FRONTIER (`CreamGorge`)
**Date:** 2026-07-25
**Build policy:** strict remote only (`RCH_REQUIRE_REMOTE=1`), no local Cargo fallback

## 1. Meta-Lever 2 contract adopted

The touched performance harnesses now implement the first three mandatory contract parts:

1. hash and print the executing ELF from inside the process;
2. run an A/A null control and A/B in one invocation with alternating arm order; and
3. report bootstrap 95% CIs on the median and gate at a mandatory 2× A/A null-CI
   margin. CV and MAD remain report-only.

Adoption surface:

- `crates/fm-layout/benches/barycenter_sweep.rs`
- `crates/fm-layout/benches/harness_calibration.rs`
- `crates/fm-render-svg/src/attributes.rs`
- `crates/fm-render-canvas/src/renderer.rs`
- `docs/CROSS_REPO_RECOMMENDATION_bench_harness_contract.md`

The calibration selector was also corrected to publish
`min_decidable_2x = 1 + 2 * max(|ci_low - 1|, |ci_high - 1|)` rather than the old
single-half-width value.

## 2. Meta-Lever 1 VOID audit

The complete mechanical audit is `docs/LEDGER_RESURRECTION.md` plus
`docs/LEDGER_RESURRECTION_TABLE.md`:

- 667 ledger entries total;
- 251 reject-class entries;
- 19 VOID-A plus 43 VOID-B = **62 / 251 (24.7%) void**;
- only 22 / 251 recorded an A/A control;
- only 5 / 251 recorded a bench-binary SHA-256.

Historically, seven distinct VOID levers had already been re-run: four re-won and shipped, while
three were correctly closed. This turn re-certified the ranked top five on current code under the
new contract.

## 3. Ranked top-five resurrection

### Ranks 1–3: crossing-minimization harness

Final current-source run:

- worker: `ovh-a`;
- source HEAD at invocation: `470ca18821cf771c76f4bfb7924a2ed55733d0ff`;
- bench source SHA-256:
  `443efbc799c8996ffdcf217c106bc0df0fa013c7c176174c90d3d852b48bb795`;
- self-reported ELF SHA-256:
  `2439b3cad0ddd002ca7c697aa1d0ce6b21079b6c29038771dfa95705d2bd994c`
  (909,752 bytes);
- 41 rounds, 2 ms faster-arm calibration, minimum of three, exact result parity for every arm.

All values below are baseline/candidate speedups; larger than one is faster.

| Audit rank / comparison | SCC 100 | SCC 300 | SCC 800 |
|---|---:|---:|---:|
| #1 flat-CSR (`single_pass / flat_csr`) | **2.835×** [2.8322, 2.8365] | **4.348×** [4.3436, 4.3546] | **7.997×** [7.9928, 8.0030] |
| #2 VOID adjacency (`BTreeMap / flat_csr`) | **10.900×** [10.8277, 10.9419] | **19.981×** [19.8766, 20.0216] | **41.646×** [41.5758, 41.7799] |
| #3 packed crossings (`flat_csr / packed`) | **1.174×** [1.1727, 1.1747] | **1.074×** [1.0734, 1.0765] | **1.047×** [1.0458, 1.0481] |

Corresponding A/A 95% CIs:

| Comparison | SCC 100 | SCC 300 | SCC 800 |
|---|---:|---:|---:|
| flat-CSR | [0.9998, 1.0008] | [0.9993, 1.0001] | [0.9992, 1.0003] |
| VOID adjacency | [0.9999, 1.0006] | [0.9998, 1.0004] | [0.9996, 1.0009] |
| packed crossings | [1.0000, 1.0003] | [0.9988, 0.9998] | [1.0001, 1.0011] |

**Verdict:** ranks 1–3 are certified re-wins. The weakest effect, packed crossings at SCC 800,
still has its entire A/B CI above the mandatory 2× null floor.

**Retry / re-check predicate:** repeat only if the production crossing lineage changes, the
auto-selector stops routing the SCC fixture through Sugiyama, or a profile puts the named target
below 3% self-time. A rerun remains valid only with exact parity, a self-reported ELF hash, and the
same-invocation null-CI gate.

### Rank 4: `write_escaped_text` clean-label scan

- worker: `vmi1293453`;
- source SHA-256 before/after:
  `eec2f3d7df83b25e45e677bdc70c4e5f419fbdd42000258c08ab629386252f11`;
- self-reported ELF SHA-256:
  `08d4042140f76ef95071f8872a4826b63666e947305fbf27d4fa64f906630f8c`
  (10,890,200 bytes);
- A/A median **1.007906**, 95% CI **[0.961192, 1.048964]**;
- A/B median **1.387162**, 95% CI **[1.354079, 1.420410]**;
- exact parity across empty, ASCII-clean, escapable, CDATA, Unicode, and long-string cases;
- CV report-only: A/A 11.41%, A/B 16.09%.

**Verdict:** certified re-win. This row is direct evidence that CV is not a decidability gate:
both CVs exceed the old 5% threshold while the complete A/B CI clears twice the A/A floor.

**Retry / re-check predicate:** repeat if `write_escaped_text` or its short-clean dispatch changes,
or if a new profile no longer attributes at least 3% of representative SVG rendering to escaping.
Require the same clean/escaped/Unicode parity matrix.

### Rank 5: Canvas dotted-edge borrowed dash slice

- worker: `ovh-a`;
- source SHA-256 before/after:
  `7336437660468238e75f4dd7cb7cf9b87aeb1b2d75318c43cf0816eabeeade74`;
- self-reported ELF SHA-256:
  `7d9314ae65055046e7b138fd6c3ea62345a02f444caa9afbc09f6e9c59c4c014`
  (6,964,424 bytes);
- A/A median **0.999563**, 95% CI **[0.999427, 1.000666]**;
- A/B median **3.793855**, 95% CI **[3.792880, 3.796789]**;
- exact branch-mapping parity;
- CV report-only: A/A 1.42%, A/B 0.81%.

**Verdict:** certified re-win.

**Retry / re-check predicate:** repeat only if the Canvas stroke-dash representation or dotted-edge
branch mapping changes. The parity guard must continue to cover every arrow/dash branch.

## 4. Extra lineage result: dense-rank is decisively slower on current SCC inputs

The five-arm run also re-decided the historical `BTreeMap / dense_rank` stage:

| Input | A/A 95% CI | A/B speedup 95% CI | Verdict |
|---|---:|---:|---|
| SCC 100 | [0.9993, 1.0007] | **0.847×** [0.8467, 0.8491] | candidate slower |
| SCC 300 | [0.9994, 1.0030] | **0.821×** [0.8196, 0.8212] | candidate slower |
| SCC 800 | [0.9994, 1.0016] | **0.793×** [0.7925, 0.7936] | candidate slower |

The SCC-300 null CV was 5.54%, yet its null CI is tight and the slowdown is decisive. This is
ledgered as a current negative result in `docs/NEGATIVE_EVIDENCE.md`; no source lever was present to
revert because both historical arms are retained bench internals.

**Retry predicate:** do not propose dense-rank alone again on SCC graphs unless a production change
materially alters that arm and a profile-admitted non-SCC workload produces an A/B 95% CI wholly
above the mandatory 2× null threshold without regressing SCC 100/300/800.

For completeness, the following `dense_rank / single_pass` stage is strongly positive:
**4.583×**, **5.677×**, and **6.554×** at SCC 100/300/800.

## 5. Profile-attributed frontier keep: transformed theme-CSS cache

Profile basis: `.benchmarks/bd_1buv_68_workload_class_profiles.md`.

- `doc_build_40`: `render_svg_with_layout` **20.02% self**;
- theme-CSS post-pass `memchr::memmem::Finder::find_impl`: **9.00% self**;
- fixed strip/minify block with memmove: roughly 34% of the profile.

One lever caches the exact transformed `<style>` bytes under an exact raw-CSS plus
state/accent/body-var/live-marker feature key. It is thread-local, bounded at 32 entries, hits across
separate diagrams and label-only edits, bypasses unknown markers, and leaves the >100 KB path alone.
Full design and proof: `.benchmarks/render_theme_css_minify_memoization_CANDIDATE.md`.

Strict-remote release result on `ovh-a`, exact pinned 40-document corpus
`8badedbf69bc204d952af1ba780c07569b7eb1091ff5d0fdd400dd2e3f6b59d7`:

- self-reported ELF:
  `85a81ad5c196a27ed56503ca7756c6a68e4de2fa74ef7a30c4cab8624dbad786`
  (10,931,720 bytes);
- A/A median **0.991926**, 95% CI **[0.988105, 1.000418]**;
- A/B median **1.306114**, 95% CI **[1.302648, 1.318433]**;
- mandatory lower-bound threshold **1.030000**;
- CV report-only: baseline 3.27%, candidate 3.36%;
- exact parity for all 40 timed outputs plus **264/264** permanent miss/hit matrix comparisons.

**Verdict: KEEP — 30.61% faster at the median.**

**Retry / re-check predicate:** rerun the cold-per-batch probe if marker identities, any cached
post-pass, the 100 KB cap, or representative cache cardinality changes. Revert only if the A/B lower
CI crosses the mandatory 2× null threshold or any cached/uncached byte comparison fails.

## 6. Quality-gate disposition

- `cargo fmt --check`: pass.
- `git diff --check`: pass.
- strict-remote `cargo check --workspace --all-targets`: pass on `hz2`.
- strict-remote `cargo clippy --workspace --all-targets -- -D warnings`: pass on `ovh-a`.
  The first run exposed an existing `items_after_test_module` ordering error in
  `crates/fm-cli/src/bin/evidence.rs`; moving the unchanged `encode_hex` helper above the test
  module repaired the workspace gate without changing behavior.
- strict-remote `cargo test --workspace -j 2`: every suite passed until the repository's repeatedly
  documented standing `gantt_basic` SVG golden mismatch
  (`57da53e44f35e614` rendered versus `1e45b85306e2366c` checked in).
- strict-remote retry skipping only that named golden passed every subsequent suite until the
  legacy `test_tree_overflow` process aborted while laying out its 10,000-level mindmap. A focused
  retry on `hz2` still overflowed with `RUST_MIN_STACK=134217728`.
- strict-remote `cargo test --workspace -j 2 -- --skip svg_golden_snapshots_are_stable --skip
  test_tree_overflow`: pass on `ovh-a`, including all unit tests, 118 CLI integration tests,
  invariant/conformance suites, and doctests.

Neither standing red is attributed to this change and neither expected artifact was rewritten.
Follow-up bugs: `bd-0nr2` (review/refresh `gantt_basic`) and `bd-c6xf` (make the deep-tree test
non-aborting).

`ubs` was run on the exact intended commit surface. It exited nonzero on broad Rust heuristics:
bench/test `panic!` and `unwrap`, ordinary SVG marker-id equality misclassified as secret comparison,
and existing allocation/index inventories. Manual triage found no diff-attributable defect; the
scanner's own fmt, clippy, build, test-build, audit, and deny sub-gates were green. The existing
`bd-j4pv` owns broad `fm-render-svg` UBS heuristic triage.

Final pre-staging source SHA-256 recheck at HEAD `470ca18821cf771c76f4bfb7924a2ed55733d0ff`
matched the measured sources: barycenter harness `443efbc7…795`, calibration harness
`033ff9f7…899`, text-escape harness `eec2f3d7…f11`, Canvas harness `73364376…974`, and
theme-CSS implementation `618b0bca…7b4`.
