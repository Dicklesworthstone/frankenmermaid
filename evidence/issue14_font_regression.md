# Issue #14: browser labels, viewport, and font geometry

Recorded October 19–20, 2026. The integrated source improvements landed directly
on main in
[03fc62ed1cd058bc9b25ff98af4f6b8ff5f449e9](https://github.com/Dicklesworthstone/frankenmermaid/commit/03fc62ed1cd058bc9b25ff98af4f6b8ff5f449e9).
This package follow-up completes the matching canonical browser build and its
integrated validation record.

## Delivery status

The source integrates the issue-14 candidate `a8ccad19305eff6bf3142940af4f962d4c646801`
with main through `29536a1308acd1bae4da8b72b4cebffb39709ae0`. It preserves all four
concurrent circle/diamond, Canvas, ER, and grouped-tree/sequence-note changes.
Independent source review checked shared cluster bounds, tree metrics, Canvas
typography alignment, and retention of the upstream SVG/WASM work.

The matching runtime was rebuilt from an empty isolated target. A fresh browser
run passed **110/110 checks on the integrated runtime**, and the actual WASM API
suite passed **71/71**. Subsequent canonical rebuilds changed only the source
fingerprint in package metadata: the WASM, JavaScript, declarations, README, and
license are byte-identical to the tested package. The final native gate results
are recorded below. Production deployment is not claimed.

The detailed JSON retains the recovered pre-integration record in its original
top-level fields and adds current results under `integrated_main_validation`:
[issue14_font_validation_summary.json](issue14_font_validation_summary.json).
Its historical version at source commit `03fc62ed` has SHA-256
`f8013b4736641f5bad7306948ac5ef9e0ef2a7cfbfc103ea9d971af38e15ca75`.
That older record covers the candidate based on `41afadc7`; it is not substituted
for the new integrated runs.

Both positive path-reuse fixtures pass using independently parsed flat prefixes,
because grouped paths correctly reject transplantation. The helper checks complete
prefix node/edge objects, referenced labels and markup, direction, styles, and the
absence of groups, clusters, and ports before constructing its certificate.
Fresh-render equality, eligibility, changing label placement, and the original
reuse-count assertions remain. The focused two-test run and full SVG suite pass.

## Original report and reproduction

The [original issue](https://github.com/Dicklesworthstone/frankenmermaid/issues/14)
concerns the deployed browser playground, not an established CLI regression.
The unchanged fixture is
`crates/fm-render-svg/tests/fixtures/issue14_omnichannel.mmd`: 76 lines, 24 nodes,
43 edges, 24 labeled edges, and two titled subgraphs.

- Original source SHA-256:
  `3258bf1dd770ee1d7c9715069f4dc463bcd64faa119646645afe2792ee177b4c`.
- Captured SVG: 45,144 bytes, SHA-256
  `ccec4cd4463d1e0504131147d5ba01ef056a2d9c61eaef89bfcc16826c5dd446`.
  It contains 24 text elements, Compact detail, and no drawn edge/subgraph labels.
- Captured SVG URL:
  <https://github.com/user-attachments/files/33593339/frankenmermaid-observed.svg.txt>.

The viewport repair previously landed in
`2b1033f200aa7006349865dbf8dfd42ba39f3aaa`; full-label preservation and the earlier
package landed in `f85cf30d16918b5819f27e5eb1f962b324390c13`.
The [initial evidence](issue14_browser_regression.md) records that earlier work.

Eight layout/style/class/routing changes subsequently reached main without a
package rebuild. A canonical build of unchanged parent `41afadc7` reproduced
further custom-font defects. Its WASM SHA-256 is
`85af6ff9a368335f5a7e96fff78d2016f7cbbdea30ea4a92666035e17794dbed`.
It passes the original 62 browser checks but fails all 40 additional font/plate
checks in the retained 102-case negative control, with no setup failures.
The extended API gate also fails: its long edge plate stays 305.5 px wide at
font sizes 11, 22, and 30 despite preserving the text.

## What changed

### Font configuration and layout

WASM configuration merging scales absolute average-character calibration with
font size while preserving custom calibration. Initialization, repeated
initialization, nested overrides, direct SVG, decks, and worker requests share
this behavior. Invalid or overflowing derived widths cannot partially update
the configuration.

Sugiyama rank gaps use the caller's actual font metrics. Tree layout carries
those metrics through node sizing, its directed-path and branching rank gaps,
and cluster construction. Tests exercise LR/RL/TB/BT, branching, doubled metrics,
retained edges, stable unlabeled gaps, and explicit-default equality.

### Full text, backing plates, and export frames

Node and edge-label fitting accounts for proportional advances, wide W/M/w/m
glyphs, and measured font/weight variation. Both SVG writers share the fitting
geometry. Diamond labels use an inscribed rectangle, including icon displacement
and line height, rather than only the diamond's outer bounding box.

Authored multiline text is scaled as a block before adding wrapping when it can
remain readable. The report's original three-line node N remains three lines;
its assertion is unchanged. An intermediate candidate that changed it to four
lines failed this gate and was corrected. The wider node T wraps when required
while preserving all text.

Edge plates use the same full-text measurement for sizing and placement.
Anchors are projected into the frame before the no-collision fast return.
Placement checks nodes and earlier plates, preferring a clear in-frame position.
SVG and deck bounds include the actual resolved plate positions and expand when
necessary. Diagram title space is reserved even when a label moves above the
initial frame. Cached fragments still revalidate affected positions and offsets.

At font size 30, with the same actual browser font:

| Probe | Fresh `41afadc7` baseline | Rebuilt issue-14 runtime |
| --- | ---: | ---: |
| W20 backing width | 178 px | 703 px |
| W20 glyph width | 504.28 px | 504.28 px |
| W80 backing width | 628 px | 2,728 px |
| W80 glyph width | 2,017.09 px | 2,017.09 px |
| W80 node gap | 1,426 px | 2,812 px |

The final browser gate detects no glyph escape, tested opaque-fill occlusion, or
export-frame clipping in these cases.

### Cluster titles and compound frames

The additional minimal reproduction is:

```text
flowchart TD
subgraph X["Very long title"]
A[Alpha]
end
```

Before the cluster correction, the 60 px case drew the title at y=82–145 while
the child started at y=112. The later node fill obscured `Verylon`, and the title
extended about 22.18 px beyond the SVG frame. At size 30 it escaped its cluster
by 7.84 px.

Titles now receive semibold width allowance and header space for the baseline,
multiple lines, descenders, and a gap above children. Ordinary default one-line
headers keep the existing 36 px padding when sufficient. Compound allocation and
final reconstruction share a rectangle calculation, including nested insets.
Additional title width is centered around the contents.

| Measurement | Font size 30 | Font size 60 |
| --- | ---: | ---: |
| Actual title glyph width | 216.44 px | 432.88 px |
| Cluster width | 244.69 px | 473.38 px |
| Title bottom | 112.00 px | 145.00 px |
| Child node top | 121.10 px | 156.20 px |
| Clear space above child | 9.10 px | 11.20 px |

Both routes and independently reopened SVG downloads pass. CDP confirms
DejaVuSans-Bold draws these titles. Native tests additionally cover sizes
15/30/60, multiline headers, centering, nested containment, and all four parent
directions.

## Browser and API validation

The actual browser run used Chromium 141.0.7390.37 and Playwright 1.56.0, serving
both real showcase pages and the rebuilt package. Cached production Tailwind
and font responses were hash-verified. The default SVG system stack resolves
to DejaVu Sans on this Linux host; CDP records the actual drawing face.

All original 62 checks and tolerances remain. Forty more cover the original
diagram at sizes 22/30, default and explicitly requested Rich detail, W20 at
11/15/22/30, W80 at 22/30, scales 1/0.31/0.95, and independent exports.
Eight more cover the minimal cluster at 30/60 on both routes and exports.
A title-occlusion oracle checks later opaque node/edge fills. Ten controlled
SVGs validate its positive cases and exclusions for earlier, transparent,
and disjoint fills.

At approximately 31.47% and 95% zoom, the reported diagram retains
`data-detail-tier="compact"` and `data-label-detail="full"`, with all 24 node
labels, 24 edge labels, and two titles drawn. Text preservation is independent
of the presentation tier; CSS zoom is not claimed to change Compact to Rich.
Explicit opt-out and resource-pressure presentation remain covered.

As a negative control, Chromium independently reopened the reporter's unchanged
captured SVG at scales 0.31 and 0.95. Both views retain Compact detail, only 24
painted text elements, 23 inserted ellipses, and zero drawn edge/subgraph labels.
This measures the captured artifact; it does not claim a current-source failure.

At 1466 × 904, the maximized root measures exactly 1466 × 904. The pan container
is 855.16 × 635, from y=164.5 to y=799.5, inside the viewport. Zoom/fullscreen
controls remain reachable. The 1100×650, 390×844, 390×650, and 844×390 cases also
pass, including mobile editor/Compile/return, Fit, and Escape.

Both HTML files remain byte-identical, SHA-256
`e4e8f92489b7ab2fcdb22b655c08db2c1cf597416b14920e4ce4b4727c615f26`.
These checks retain the earlier viewport repair while testing the new renderer.

The 71-case API suite uses actual WASM for direct SVG, decks, workers,
initialization, and override precedence. It checks actual text, full-label
counts, authored line breaks, font-sensitive geometry, pressure, and opt-out.
No original correctness threshold was weakened.

Both routes recorded zero uncaught JavaScript errors. The optional Mermaid CDN
comparison failed certificate validation on this execution host, so this run
does not establish a live Mermaid comparison. FrankenMermaid, its actual WASM,
and 15 hash-verified production CSS/font resources loaded successfully.

## Final integrated package identity and digest audit

These identities describe the canonical integrated package in this follow-up:

| Artifact | Measurement |
| --- | --- |
| Raw WASM | 2,010,209 bytes |
| Canonical gzip | 805,290 bytes |
| Ceiling | 805,888 bytes (787 KiB), 598 bytes headroom |
| WASM SHA-256 | `512402ad4a81cb05d06bb8bf5dec831ea7f315aeb781c39bab047a627a538aa1` |
| Source fingerprint | `8ca904f5f1ed92114f5021c83d8953f9159604721743a6968cd3ada0d1b019b5` |
| Five-artifact FNV-1a | `0x6999f28e7ea66697` |
| Verified binary Git blob | `16a906f4b2ccd15b33f97182f18c811d99d4bdc1` |
| Verified package.json Git blob | `21ca6821ab71aa06c0e6ed2b47ef7b8447b8efdb` |

The unchanged integrated baseline `29536a1308acd1bae4da8b72b4cebffb39709ae0`
was compiled independently in a new empty target. It measures 2,007,365 raw
bytes and 803,958 gzip bytes, already 118 bytes beyond the interim 785 KiB
ceiling. The integrated issue-14 fixes add **2,844 raw bytes and 1,332 gzip bytes**.
The measured resource-budget procedure raises the ceiling to the smallest whole
KiB that fits, 787 KiB. Compiler, optimizer, and correctness settings are unchanged.

The raw delta comprises 2,810 code-section bytes, 24 type-section bytes, and
10 function-section bytes. Both modules have zero custom sections, 89 imports,
50 exports, and a 158,119-byte data section. Equal data-section size does not
assert byte identity. Canonical -Oz/--converge, LTO, one codegen unit, and the
immediate-abort standard library remain. The final package was independently
compiled from an empty target, then canonically rebuilt after the test-only
source-fingerprint change; both final builds pass freshness and size gates.

A first baseline attempt reused a shared Cargo target and incorrectly reused the
candidate binary. Its result was rejected and is not used in these measurements.
The retained valid comparison uses independently populated baseline/final targets.

Before retaining the render pins, all four fixture SVGs were generated through
the actual old and integrated WASM packages. They are byte-identical, including
all text, geometry, root attributes, and element structure. The reviewed WASM
output digests remain:

- Flowchart: `0xafe8712eaa94163c`
- Sequence: `0x312f713e23dd8c2b`
- Class: `0x86fb0b6a73147bb7`
- State: `0xd5c3891b75b12cd9`

The `fm-wasm` test module containing these pins is excluded from the source
fingerprint. The final native test separately checks package identity, source
freshness, and rendering equality, so a stale package cannot pass as a target match.

## Native gates and limitations

Default native features and dependencies are unchanged. Builds used the pinned
nightly toolchain, serial Cargo/CMake jobs, disabled native debug information,
no Rust flag override, and a shared-host resource guard. Sources remained
unchanged during each reported final run.

| Gate on the integrated package | Result |
| --- | --- |
| Workspace/all-targets cargo check | Stopped by shared-host memory guard before Rust errors; retry blocked by host capacity |
| Workspace/all-targets strict Clippy, -D warnings | Not run: required host memory/filesystem reserves unavailable |
| Native layout unit suite | 634 pass, 1 ignored |
| Native SVG unit suite | 354 pass, 2 ignored |
| Native WASM unit suite | 68 pass, 1 ignored |
| Package freshness smoke checks | 4/4 pass |
| Native/WASM comparison | 4/4 fixtures agree, with package/source freshness checked |
| git diff --check | Pass |
| cargo fmt --check | 78 preexisting hunks in 7 files; no new hunk bodies |

The integrated native suites total **1,056 passed tests and four existing ignored
tests**. Workspace-wide check/Clippy passes from the older candidate are retained
only as history and are not claimed for this integration. The fresh workspace
check reached 7,899,369,472 bytes of shared unreclaimable memory on the
8,589,934,592-byte host and was stopped by the unchanged 700 MB reserve guard,
without a Rust error diagnostic. The resumptive check could not start when the
available filesystem space fell below its unchanged 900 MB reserve. No other
jobs were stopped, files deleted, or guard thresholds lowered to finish these
gates. Rerun workspace check and strict Clippy when host capacity is available.

The directed-path fixture starts at the certifier's real three-node minimum.
The two SVG transplant fixtures use checked flat IR prefixes; the changing-label
case uses WW and deliberately wide SVG calibration so its plate still collides
with the changing suffix. All label-change, fresh-output equality, eligibility,
and exact reuse assertions remain and pass.

The integrated circle geometry also exposed an exact-equality assertion in the
gitgraph lane test: reconstructed centers were 293.404144287109375 and
293.40411376953125, exactly one f32 ULP apart. The unchanged production formula
places both at the common lane center. The test now permits at most one ULP for
finite nonnegative centers, explicitly requires unequal commit widths, and retains
diameter separation and strict chain ordering. Production coordinates are not
rounded to satisfy a test.

Two small existing Clippy blockers were repaired without suppression:
quadrant cell dimensions are passed as a tuple, and validated lasso pairs use
typed chunks. Existing lasso correctness coverage passes. The latter also
resolves one old formatting hunk. After the four upstream changes were integrated,
the remaining 78 formatting hunk bodies match unchanged `29536a`; the small new
compound-test import hunk was fixed. The final formatting audit finds no new hunk.

The ignored layout test
`routed_edges_do_not_run_through_unrelated_nodes` remains the preexisting live
routing acceptance gap in bead `bd-h2ze`. The two ignored SVG tests and one
ignored WASM test are existing manual performance probes. A full-workspace
test run is not claimed.

## Reproduction and deployment status

```bash
./build-wasm.sh
python scripts/playground_explorer_test.py --showcase-issue14 \
  --serve-root . --output-dir /tmp/issue14-final
node scripts/issue14_wasm_regression.mjs pkg /tmp/issue14-api.json
node scripts/check-wasm-freshness.mjs
node scripts/wasm_cross_target_digest.mjs
cargo test --locked -p fm-render-svg -p fm-layout -p fm-wasm --lib
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Set CHROMIUM if needed. The browser harness accepts --tailwind-script and
--resource-cache for hash-verified production resources.

The new integrated raw browser report SHA-256 is
`f37b57ec97259195af9cbc97014369c5e477e24c95dc60c0853309b5636ea649`.
Its run path is
`/dev/shm/frankenmermaid-browser-validation-20261020/runtime-512402ad/issue14-results.json`.
The authoritative source/package checkout is
`/dev/shm/frankenmermaid-issue14-integrated-20261020`.
The committed JSON embeds current browser measurements, API results, package/size
audits, and gate records so these transient run paths are not the only evidence.

A complete distribution was assembled from the final package at
`/dev/shm/frankenmermaid-site-integrated-final-20261020`: 27 copied files,
generated headers, seven passing hosting-plan checks, and the exact tested WASM
and package metadata. Wrangler and Cloudflare credentials are unavailable in this
session. Deployment through Wrangler and verification of the live asset remain
outstanding; a main-branch commit is not evidence of production deployment.

No pull request, issue comment, or issue-state change is part of this work.
