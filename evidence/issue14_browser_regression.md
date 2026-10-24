# Issue #14: browser playground regression evidence

Recorded 2026-10-17–18 for [BleedingDev's report](https://github.com/Dicklesworthstone/frankenmermaid/issues/14).
This concerns the deployed browser playground; it does not establish a CLI regression.
Baseline: `4e6dea858bf441156beab63f0edd301645856e76`.
Viewport/camera fix committed to main: `2b1033f200aa7006349865dbf8dfd42ba39f3aaa`.

The final production package passes **62/62 browser checks**, **38/38 actual WASM API
checks**, and **4/4 existing WASM freshness checks**. The complete check-outcome matrix,
font asset hashes, size measurements, and byte audit are retained in
[`issue14_validation_summary.json`](issue14_validation_summary.json).

## Original inputs and artifact identity

The unchanged fixture is `crates/fm-render-svg/tests/fixtures/issue14_omnichannel.mmd`:
76 lines, 24 nodes, 43 edges, 24 labeled edges, and two titled subgraphs.
The browser oracle extracts all 50 expected labels from the pinned source independently of FrankenMermaid's parser.

| Input/artifact | SHA-256 |
| --- | --- |
| Original Mermaid source | `3258bf1dd770ee1d7c9715069f4dc463bcd64faa119646645afe2792ee177b4c` |
| Reporter's captured SVG | `ccec4cd4463d1e0504131147d5ba01ef056a2d9c61eaef89bfcc16826c5dd446` |
| Baseline `pkg/frankenmermaid_bg.wasm` | `b7f8162254c93af259e51640d8c4fb7231b78683a73ebe2f0c22e0425fbb848a` |
| Baseline root HTML (both copies) | `da0a41d700e841ee731ca392df2a30e455ad647fb69b0969fb1f7829188fec1c` |
| Fixed root HTML (both copies) | `e4e8f92489b7ab2fcdb22b655c08db2c1cf597416b14920e4ce4b4727c615f26` |
| Cached production Tailwind script | `176e894661aa9cdc9a5cba6c720044cbbf7b8bd80d1c9a142a7c24b1b6c50d15` |

## Reproduction and completed browser sizing validation

The report records `data-detail-tier="compact"` near 31% and 95% zoom. Local baseline
rendering likewise remains Compact: 24 node text elements, 23 shortened/altered labels,
zero visible edge labels, and zero subgraph titles. The short decision label already
survives. Zooming cannot recover text absent from the generated SVG; export also loses it.

At 1466 × 904, the baseline pan container measured 1731.6875 pixels high and ended at
y=1897.1875; its visualizer measured 1772.1875 pixels high. The fixed minimum-size chain,
viewport sizing, short-window chrome, and camera fitting passed on both root routes:

| Viewport | Baseline pan height | Fixed pan height |
| --- | ---: | ---: |
| 1466 × 904 | 1731.6875 px | 635 px |
| 1100 × 650 | 1731.6875 px | 461 px |
| 390 × 844 | 619.5 px | 369.5 px |
| 390 × 650 | Not measured | 387 px |
| 844 × 390 | Not measured | 182 px |

Mobile editor heights were 243.5, 285, and 134 pixels at the final three sizes.
The tests checked viewport containment, usable pane area, Fit bounds, approximately
95% zoom through actual controls, control-center hit testing, mobile editor focus and
Compile/return, and Escape restoration. Screenshots confirmed the short landscape view.
These sizing checks used the old WASM package: label failures remained expected, so
the overall browser suite was **not** reported green for that run.

Evidence: `/tmp/issue14-baseline-real/issue14-results.json` and
`/tmp/issue14-current-short-fixed/issue14-results.json`, with adjacent SVGs/screenshots.
The browser loaded real WASM and production Tailwind. A CDN certificate failure was
handled by routing the exact cached production script above, not a substitute stylesheet.

## Final rendering behavior

Browser/WASM `preserveLabels` now defaults to true. Full node text,
edge labels, and subgraph titles survive Compact/Normal/Rich tiers and pressure-driven
decoration reductions. `data-label-detail="full"` records that text policy separately
from `data-detail-tier`; CSS pan/zoom does not need to regenerate missing labels.
Explicit `preserveLabels: false` remains available, and native `SvgRenderConfig` keeps
its existing default. Top-level and nested SVG overrides are covered by focused tests.

The default pressure state lacks telemetry and selects the conservative Compact fallback.
CSS pan/zoom does not rerun tier selection. The fix therefore retains truthful detail-tier
metadata and separates text preservation from typography and decoration; it does not
pretend that zoom has selected Rich. Explicit nominal-pressure Compact, Normal, and Rich
requests are independently exercised by the actual WASM tests.

Full text alone was insufficient. Edge 24, “U uživatelek aplikace,” was initially painted
under node N, and the opaque background of edge 25 covered edge 16, “Ano.” Preserved
flowchart labels are now resolved once in actual edge paint order, avoiding every node
and earlier label plate. Streaming and Element writers share those positions. Cached
edge fragments retain the resolved coordinates and compare them before prefix reuse,
including the optimized path that transplants a changed layout suffix in place.

The placement algorithm merges obstacle intervals and considers four finite axis moves
inside the existing padded SVG frame. It is deterministic local placement, not a global
packing guarantee: if all four candidates are outside the frame, the original routed
anchor remains. Native/adaptive defaults and other diagram families keep their previous
edge geometry. Tests cover paint order differing from IR indices, invisible/bundled
paths, changing unrelated obstacles, frame changes, and optimized prefix transplantation.

### Zoom-sensitive font metrics

At very small mobile Fit scales, Chromium 141's `optimizeLegibility` produced a
24.4926-SVG-unit text height at 4.08288% zoom, corresponding to one device pixel, while
the configured font size remained 10.36 SVG units. Changing only the transform back to
1 retained the distorted bounds. A controlled same-SVG, same-font experiment changing
only `text-rendering` to `geometricPrecision` restored the 12-unit glyph bounds and
produced zero geometry errors at scales 1, 0.0408288, 0.105424, 0.31, and 0.95.

Preserved SVGs now append this rule after the theme's text rules in both rendering backends:

```css
[data-label-detail="full"] text { text-rendering: geometricPrecision; }
```

Preserved Scene roots receive the matching marker. The attribute selector scopes the rule
to preserved diagrams, including exports, without altering an adaptive sibling SVG.
The native test checks the opening root tag independently of the identical string in CSS,
both backends, cascade order, and explicit opt-out. The browser tolerances were unchanged.

## Final browser and API validation

The real-showcase suite passed all **62 checks** on both `index.html` and
`frankenmermaid_demo_showcase.html`, using actual WASM and loaded production Inter and
JetBrains Mono fonts. It checks all 50 painted labels, authored line breaks, text/shape
containment, diamond corners, edge-label plates, later opaque node/label fills, SVG
bounds, Fit, native 31% and 95% zoom controls, full-label downloads, all five maximized
viewports, control reachability, mobile editor/Compile/return, Escape, and uncaught errors.
The exact zoom scales reached in the suite were 0.309176 and 0.945777.

A separate native-input closeup reached exactly 0.95, with zero text/geometry errors.
CDP confirmed custom Inter-Medium for node N and edges 24/25. Visual inspection confirmed
the complete three-line Czech node text and separately visible “U uživatelek aplikace,”
“Ano,” and “Odpověď 1 / 2 / 3.” The five pan/editor measurements above also hold with
the final package and production fonts. Offscreen content during a 95% closeup is normal
camera clipping; Fit bounds and exported content are tested separately.

The oracle was checked against the retained intermediate SVG with known occlusions. It
still detected all three expected failures: the hidden “Ano” glyphs, overlapping edge
16/25 plates, and overlapping edge 28/29 plates. The precision fix did not hide these
failures by loosening tolerances.

The durable `scripts/issue14_wasm_regression.mjs` passed 38 cases across direct `renderSvg`,
`renderDeck`, worker messages, and initialization. Each of the 34 full-policy cases draws
24 node labels, 24 edge labels, and two titles; all four explicit opt-out cases retain the
previous Compact 24/0/0 behavior. It uses the pinned original fixture and an independent
multiset of actual `<text>/<tspan>` contents, not accessibility metadata. Pressure still
reduces decoration, nested overrides take precedence, nominal telemetry allows each
requested presentation tier, and exported precision styling is checked.

The existing offline explorer suite remains **38/39** on this Chromium, with its prior
`wheel-zoom-keeps-pointer-anchor` discrepancy (about 0.162 SVG units against a 0.002
tolerance). Its original `main()` AST and tolerance are unchanged. This is separate
from the new 62/62 production-page regression suite.

## Rebuilt package, size attribution, and digest audit

| Artifact | Measurement |
| --- | --- |
| Final raw WASM | 1,983,662 bytes |
| Final canonical gzip | 794,414 bytes |
| Final size ceiling | 794,624 bytes (776 KiB), 210 bytes headroom |
| Final WASM SHA-256 | `c2ba665b60abdd57d9c357fc6e28891b4bed7b62bcfc75d714fb059c87af3ea1` |
| Final source fingerprint (85 inputs) | `b41ce83803d928375eb5950f8a01116f42d7c4b4a000f34bcd8c78ef668b5ae7` |
| Five-file package FNV-1a | `0x044c702458aaf4e4` |

The last committed package was generated at `01666a65023b18ae334faf47ae7d002fd89d9207`.
Its 83-input fingerprint was checked against that revision. A fresh build of unchanged
baseline `4e6dea85` actually recompiled all six local crates, using the same pinned
toolchain and canonical optimization settings as the candidate. An earlier stale-mtime
cache reuse was rejected and is not used for this attribution.

| Canonical gzip measurement | Bytes | Attributed change |
| --- | ---: | --- |
| Last committed package | 771,021 | Only 51 bytes below the old 753 KiB ceiling |
| Fresh unchanged baseline | 790,765 | +19,744 from source accumulated since that package |
| Same baseline with inline producer stripping | 790,696 | −69 |
| Full labels and node/label-plate clearance | 794,451 | +3,755 over the same-flags baseline |
| Final scoped geometric precision | 794,414 | −37; total issue-related growth +3,718 |

The preexisting source growth is associated with retained Canvas selection/session
bindings added since the last package build; no parser/layout/SVG dependency or profile
change explains that baseline delta. A paired optimizer experiment added only
`--strip-producers` inside the existing canonical pass: it removed 74 raw/69 gzip bytes,
with every noncustom section byte-identical. A separate strip-only optimizer round trip
was rejected because it grew gzip by 3,012 bytes. Existing `-Oz --converge`, LTO, one
codegen unit, and immediate-abort standard-library settings remain in force.

The ledger preflight returned generic-token matches; all eight matching rejected rows
were read and concerned unrelated SVG/parser/layout levers. None covered producer custom
metadata. The exact preflight wording and applicability review were retained; the query
was not reworded to evade the ledger. The size ceiling follows the script's documented
measured resource-ratchet procedure and is the smallest whole-KiB ceiling that fits.
No correctness gate was relaxed, and the old ceiling's failure is recorded above.

Four cross-target SVG fixtures were audited byte-for-byte against the old package.
Removing only the new full-label root attribute and scoped precision rule restores each
old SVG exactly. Geometry and painted text are unchanged. The reviewed final FNV-1a pins
are flowchart `0x4628cd9c352eb4d7`, sequence `0xd3bcce11232c5c5e`, class
`0xae81b7ad5f775612`, and state `0x753b63643e69e453`. The package digest and all four
pins were updated together. The WASM test module is explicitly excluded from the source
fingerprint, so these test-only pin updates do not stale the rebuilt package.

## Reproducible commands

Run from the repository root with its pinned Rust toolchain and browser prerequisites:

```bash
./build-wasm.sh
python scripts/playground_explorer_test.py --showcase-issue14 \
  --serve-root . --output-dir /tmp/issue14-regression
python scripts/playground_explorer_test.py
node scripts/issue14_wasm_regression.mjs
node scripts/check-wasm-freshness.mjs
node scripts/wasm_cross_target_digest.mjs
cargo check --workspace --all-targets --locked -j1
cargo test -p fm-render-svg --lib --locked -j1
cargo test -p fm-wasm --lib --locked -j1
```

Set `CHROMIUM` to an installed browser if needed. For the captured CDN failure, add
`--tailwind-script /tmp/issue14-tailwind.js` to the issue-specific browser command.
The resource-cache option `--resource-cache /path/to/manifest.json` accepts hash-verified
production font responses when a CDN certificate problem prevents normal loading. The
recorded run used the exact Tailwind/Google Fonts assets whose hashes appear in the JSON
evidence, not substitute styles or an engine stub. The canonical build must finish before
package provenance or render digests are updated.

## Native gate evidence and limits

Before workspace recovery, `cargo check --workspace --all-targets --locked -j1` passed
(`check_serial=0`; `/tmp/frankenmermaid-native-check-serial.log`). That successful check
predates the final placement/precision changes and is not claimed as final-source proof.
Earlier parallel attempts encountered OOM/ENOSPC; they are not source-regression evidence.

Existing strict-Clippy blockers were `chunks_exact_to_as_chunks` in
`fm-wasm/src/canvas_selection.rs:98` and `too_many_arguments` in
`fm-render-term/src/renderer.rs:4385`. Formatting differences were traced to eight
baseline files, including unchanged formatting hunks in `fm-wasm/src/lib.rs`;
`/tmp/frankenmermaid-fmt-baseline-report.json` records that comparison.

The final native command was `cargo test -p fm-render-svg --lib --locked -j1 --
--test-threads=1`, using default features, debug=0/test-debug=0, no RUSTFLAGS override,
one Cargo/CMake job, and disabled incremental output. After completing the retained
HiGHS build/bindings, it was stopped by the resource watchdog during dependency
`wide v1.7.1`, before renderer compilation or test execution: exit −15 after 116.55 s.
The 8 GiB cgroup had reached 8,377,737,216 / 8,589,934,592 bytes, below the reserved
220 MB headroom; shared memory still had 186,732,544 bytes free. OOM counters did not
increase. The final SVG source hash was identical before and after the attempt.

Native SVG/WASM unit tests and the native side of the cross-target comparison therefore
remain **unrun**. No feature, dependency, expected output, or correctness threshold was
changed to bypass this limitation. Actual WASM compilation and the browser/API checks
above cover the shipped browser artifact; they do not establish a CLI regression result.

The final required `cargo fmt --all -- --check` exits 1 with exactly the same 83 hunks in
eight baseline files. Normalizing only absolute paths and line numbers makes its complete
diff byte-identical to the retained baseline formatter output. The final SVG file passes
standalone rustfmt; the four WASM formatting hunks are preexisting. Final all-workspace
Clippy/check and native cross-target execution are not claimed green.

## Deployment status

The final Cloudflare Pages bundle contains all 27 required files, including both corrected
playground pages, the regenerated package, lazy authoring/deck modules, capability data,
and the PNG share image. The seven hosting-plan checks and ten route-integrity checks
passed locally. The generated headers retain the repository's revalidation policy.

The repository's `AGENTS.md` requires deployment through Cloudflare Pages/Wrangler.
This session has no authorized Cloudflare credentials or account context, and no
available Cloudflare connector was found. No deployment or live-site verification is
claimed. The existing project can be deployed from an authorized environment with:

```bash
python scripts/cloudflare_pages_ops.py stage-bundle --repo-root . --output-dir dist/site
wrangler pages deploy dist/site --project-name frankenmermaid --branch main
```

Post-deploy root headers, package freshness, and the share image's `image/png` response
still require live verification. No GitHub Actions workflow was invoked, no PR was
opened, and the issue was not closed or commented on by this work.
