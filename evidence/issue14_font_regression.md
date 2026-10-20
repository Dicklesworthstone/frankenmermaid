# Issue #14: scaled-label and cluster geometry follow-up

Recorded October 19–20, 2026. The validated source candidate is
[a8ccad19305eff6bf3142940af4f962d4c646801](https://github.com/Dicklesworthstone/frankenmermaid/commit/a8ccad19305eff6bf3142940af4f962d4c646801),
based on `41afadc7a021ab6e75e5d4ec25656f6ec5696449`.

## Delivery status

The results below apply to that pre-integration candidate: 110 browser checks,
71 WASM API checks, and 1,041 native layout/SVG/WASM tests passed against its
rebuilt package.

Main subsequently advanced through `29536a1308acd1bae4da8b72b4cebffb39709ae0`.
This integration preserves all four concurrent circle/diamond, Canvas, ER, and
grouped-tree/sequence-note changes. Independent source review checked the shared
cluster bounds, every new tree-metric argument, Canvas typography alignment, and
retention of all upstream SVG/WASM changes.

An execution-host outage interrupted delivery. Automated workspace maintenance
subsequently removed the memory-backed checkout, build caches, and browser-report
directory. The source survived in GitHub objects, and the candidate WASM package
and detailed validation JSON survived in a partial checkpoint with matching
SHA-256 hashes. Execution access has returned; tooling and the merged checkout
are being restored.

**Integrated-tree validation and its matching package remain pending at this
integration checkpoint.** The measurements and digest pins below identify the
pre-integration candidate. Rebuild the combined source, audit its size and
native/WASM output, and rerun browser/native gates before updating the generated
package and reviewed pins. The previous package remains in this checkpoint;
deployment is not claimed. Two positive path-reuse test fixtures must also use
flat IR prefixes now that grouped paths correctly reject transplantation.

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

| Probe | Fresh unchanged main | Rebuilt candidate |
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

At approximately 30.63% and 95% zoom, the reported diagram retains
`data-detail-tier="compact"` and `data-label-detail="full"`, with all 24 node
labels, 24 edge labels, and two titles drawn. Text preservation is independent
of the presentation tier; CSS zoom is not claimed to change Compact to Rich.
Explicit opt-out and resource-pressure presentation remain covered.

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

## Rebuilt candidate identity and digest audit

These identities describe the successfully built and tested candidate, whose
binary transfer is still outstanding:

| Artifact | Measurement |
| --- | --- |
| Raw WASM | 2,006,518 bytes |
| Canonical gzip | 803,604 bytes |
| Ceiling | 803,840 bytes (785 KiB), 236 bytes headroom |
| WASM SHA-256 | `e71fff0f745d3eac55e07ae57408ae9856c3505c0265375466292020f2380813` |
| Source fingerprint | `3620b40964bf0201ef579aa3e3aa9999cd1e36e15f3c23175bf2362aa7456d59` |
| Five-artifact FNV-1a | `0x5b05c0d4c6024321` |
| Expected binary Git blob | `da88e9588f4492c8078990786135f9f9dc5b0369` |
| Expected package.json Git blob | `0d5584d3763e6e8c09097940f58a905663e4469c` |

Previous package gzip was 794,414 bytes. Fresh unchanged main was 802,138,
already 7,514 bytes beyond the old 776 KiB ceiling. This follow-up adds 1,466
bytes over that fresh baseline. The documented measured resource-budget
procedure raises the ceiling to the smallest whole KiB that fits.

Both baseline modules have zero custom sections, 89 imports, and 50 exports.
Across the eight prior source commits, code grew 22,017 raw bytes and data
shrunk 2,306. No metadata remains to strip. Previously measured extra optimizer
round trips grew output. Canonical -Oz/--converge, LTO, one codegen unit, and
the immediate-abort standard library remain. The metadata-only final rebuild
produced identical WASM bytes.

Before updating digest pins, every SVG element was audited against fresh
unchanged main. Flowchart and class outputs are byte-identical. Sequence and
state retain all elements and text; only four plate x/width pairs change.
Native output passes against these actual WASM digests:

- Flowchart: `0xafe8712eaa94163c`
- Sequence: `0x312f713e23dd8c2b`
- Class: `0x86fb0b6a73147bb7`
- State: `0xd5c3891b75b12cd9`

Test-only pins are excluded from the source fingerprint.

## Native gates and limitations

Default native features and dependencies are unchanged. Builds used the pinned
nightly toolchain, serial Cargo/CMake jobs, disabled native debug information,
no Rust flag override, and a shared-host resource guard. Sources remained
unchanged during each reported final run.

| Gate on the rebuilt candidate | Result |
| --- | --- |
| Workspace/all-targets cargo check | Pass |
| Workspace/all-targets strict Clippy, -D warnings | Pass |
| Native layout unit suite | 621 pass, 1 ignored |
| Native SVG unit suite | 353 pass, 2 ignored |
| Native WASM unit suite | 67 pass, 1 ignored |
| Package freshness smoke checks | 4/4 pass |
| Native/WASM comparison | 4/4 fixtures agree |
| git diff --check | Pass |
| cargo fmt --check | 90 preexisting hunks in 9 files; no new hunk bodies |

Two old fixtures failed setup after prior layout changes. The directed-path
fixture now starts at the certifier's real three-node minimum. The SVG
transplant fixture uses a certifiable WW label and deliberately wide SVG
calibration so its plate still collides with the changing suffix. All
label-change, fresh-output equality, eligibility, and exact reuse assertions
remain and pass.

Two small existing Clippy blockers were repaired without suppression:
quadrant cell dimensions are passed as a tuple, and validated lasso pairs use
typed chunks. Existing lasso correctness coverage passes. The latter also
resolves one old formatting hunk. All remaining 90 hunk bodies match the prior
report after ignoring line numbers.

The ignored layout test
`routed_edges_do_not_run_through_unrelated_nodes` remains the preexisting live
routing acceptance gap in bead `bd-h2ze`. The two ignored SVG tests and one
ignored WASM test are existing manual performance probes. A full-workspace
test run is not claimed.

## Reproduction and remaining delivery

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

The completed raw browser report SHA-256 is
`bc038c7c8e21c8e6fa1a39415365e83d82174b7066883ef43134f4c78b1bcdce`.
Its workspace path is
`/dev/shm/issue14-final-e71fff0f-browser/issue14-results.json`.
Native logs are in `/dev/shm/frankenmermaid-native-logs-20261019`.
The completed source/package workspace is
`/dev/shm/frankenmermaid-issue14-20261019`.
These paths are unavailable while the executor is disconnected; their contents
must not be assumed recovered until access and hashes are verified again.

A complete Cloudflare bundle was assembled before disconnection: 27 copied
files, generated headers, seven passing hosting-plan checks, and the same
tested WASM hash. Wrangler and Cloudflare credentials were unavailable, so
production deployment was not performed. Complete delivery requires:

1. Recover and verify the matching binary and generated package metadata, or
   rebuild them with the pinned canonical toolchain.
2. Commit those artifacts and the retained detailed validation JSON directly
   to main, then verify source freshness and package digest.
3. Deploy the complete distribution through Wrangler and verify the live asset.

No pull request, issue comment, or issue-state change is part of this work.
