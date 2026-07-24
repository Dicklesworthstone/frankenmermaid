# CANDIDATE (not yet attempted): memoize/pre-minify the invariant theme `<style>` CSS (2026-07-23)

Agent: CopperCliff (cc). Profiling analysis, NOT a measured reject — logged as a precise target.

## Profile basis (non-LTO `pipeline_bench`, small non-flowchart renders)

`render_nonflowchart/nf/er_40` and `/sequence_40`: `memchr …Finder::find_impl` is **~20% / ~18% self**
of render, attributed to the CSS post-passes. Caller breakdown (er_40):
- `minify_style_block` → `minify_css` **~9.5%** — re-minifies the ~5–9 KB `<style>` block every render.
- `strip_dead_marker_css` ~4–8% (marker scan + `marker#` selector prune).
- `strip_unused_markers` ~1.4%.
- `strip_unused_state_css` remainder.

These are a fixed per-render cost dominated by the invariant theme CSS, so they dominate SMALL diagrams
(er/seq/class/state/pie/sequence in the headtohead corpus) where the diagram body is tiny relative to
the ~5–9 KB stylesheet. On large flowcharts the same passes are a <0.5% fraction (and are size-capped
off at `POST_PASS_MAX_SVG_BYTES`). This is a COMPUTE hotspot (byte scanning), so unlike the incremental
IR-clone it will NOT mimalloc-wash — a real win is bankable if the redundant scan is removed.

## The lever

The theme CSS (`fm-render-svg/src/lib.rs:1070` `theme.to_svg_style(...)`, pretty-printed with 2-space
indent + newlines) is invariant for a given `(theme, theme_variables)` — which is the DEFAULT for the
whole corpus. Minifying it from scratch each render (`minify_css`, lib.rs:698) is redundant work.

Options, cleanest first:
1. **Emit the theme CSS already-minified** at generation time (`to_svg_style` / its rule constants),
   so `minify_style_block` finds nothing to collapse (`minified.len() == original` ⇒ no rebuild). No
   cache, no keys, no thread-safety — but touches the theme CSS templates.
2. **Content-keyed memo** of `minify_css(theme_css)` (OnceLock/thread-local keyed on a hash of the
   theme style output). Hits for every default-theme render. Needs the theme CSS minified SEPARATELY
   from the dynamic classes and the final `minify_style_block` taught to skip the already-minified
   prefix (boundary tracking).

## Why deferred (not attempted this session)

Both forms are a non-trivial restructure of output assembly with byte-drift risk against the exact-SVG
golden suite, and #2 adds cache infrastructure (stale-cache correctness hazard). Deferred to a focused
effort rather than rushed. Retry predicate: implement option 1 (lowest risk), prove every golden SVG
byte-identical (`cargo test -p fm-render-svg` + workspace goldens), then one-binary interleaved A/B on
`render_nonflowchart/nf/{er_40,sequence_40,class_50}` at CPU-load <8, require ≥3% wall with CV<5%.

## Concrete design (informed by reading the code, 2026-07-23)

The fully-processed theme CSS is a PURE FUNCTION of a tiny key: `to_svg_style(shadows, has_edge_labels)`
then `strip_unused_theme_css` gates on exactly 3 ir booleans (`has_clusters`, `has_special_shapes`
{Note/Cloud/Cylinder/Star/Pentagon}, `has_dashed_or_thick`). So `minify_css(to_svg_style+strip)` depends
only on `(theme_identity, shadows, has_edge_labels, has_clusters, has_special_shapes, has_dashed_or_thick)`
— ≤32 classes per theme. `theme_identity` MUST hash the full theme surface that `to_svg_style` reads
(`theme.colors.write_css_vars`, `theme.font.write_css`, plus any `theme_variables` overrides) — a missed
field silently ships WRONG COLORS (golden tests cover only the default theme, so this is the real hazard).

Implementation:
1. Thread-safe memo (e.g. `Mutex<Vec<(Key, Arc<str>)>>` or `OnceLock` per class; ≤32 entries) returning the
   MINIFIED theme CSS for a key. Miss → `minify_css(strip(to_svg_style(...)))` → store.
2. Push the cached minified theme CSS.
3. Track whether any PRETTY (unminified) dynamic CSS (classDef/style/inline) is pushed after it. If none
   (the common no-custom-styling render — most of the corpus), the whole `<style>` block is already
   minified → **skip `minify_style_block`** (that skip is where the 9.5% is actually reclaimed; caching
   alone doesn't help because `minify_style_block` re-scans the block regardless).
4. If pretty dynamic CSS WAS added, run `minify_style_block` as today (correctness preserved).

Risk/reward: ~2% end-to-end on small diagrams (already ≫100× mermaid-js), against a cache-key correctness
surface that can ship wrong colors for non-default themes if incomplete. Marginal EV, real risk ⇒ warrants
a focused session with per-theme golden coverage, NOT a cycle-tail rush.

## FRESH CONFIRMATION (2026-07-24, cc/Opus 4.8) — this is the TOP unmined bd-1buv frame

Full-pipeline `perf record` of `profharness er 40 … full` (non-LTO attribution build), 8000 iters,
`taskset -c 3`, load ~2.3. Ranked ALL-symbol self profile:

| self | frame |
|---:|---|
| 16.06% | `render_svg_with_layout` (CSS post-passes are largely inlined here) |
| **14.41%** | `memchr::…::find_impl` — the `strip_unused_state_css` / `strip_dead_marker_css` needle scans |
| **6.00%** | `__memmove_avx` — the strip `drain` / `minify` `replace_range` rebuilds |
| 7.93% | `write_uint_into` (mined) |
| 5.05% | `parse_mermaid_with_detection_and_config` |

So the CSS post-passes (`strip_unused_state_css` + `strip_unused_markers` + `strip_dead_marker_css` +
`minify_style_block`) are **~20% of the ER full pipeline** — the largest unmined block, and it recurs on
every non-flowchart small render (er/seq/class/state/pie). CONFIRMED real compute (byte scan + memmove,
NOT alloc ⇒ will not mimalloc-wash).

### Why it was NOT landed this session (deferred again, deliberately)

1. **The dominant cost is the STRIP passes (14.41% memchr), not `minify_css`.** The strip decisions
   depend on the BODY (which node types / markers / shapes occur), which changes across live edits, so a
   naive content-keyed memo of the whole `<style>` processing is a BENCH ARTIFACT (identical re-renders
   hit; real label-edit renders whose body text changed miss) — the same trap as the edit-session
   Arc-input reject. The production-real win requires memoizing the strip DECISION keyed on the small flag
   set (node-type presence / marker presence / theme identity), then applying it — a refactor of each
   strip pass, not a wrapper.
2. **`minify_style_block` re-scans regardless**, so caching the minified theme alone saves nothing (the
   candidate's original point) — the scan must be SKIPPED, which needs theme/dynamic boundary tracking.
3. Correctness surface: the strip-decision key must capture every body factor each pass reads, or it
   ships WRONG (missing/extra) CSS; goldens cover only the default theme + a few diagram types.

### Sharpened plan for a focused effort (retry predicate unchanged: ≥3% wall, CV<5%, all goldens byte-identical)

- Refactor `strip_unused_state_css` + `strip_dead_marker_css` to compute their body-derived KEY (set of
  present node-type needles / live marker ids) ONCE, then memoize `(key, theme_identity) → processed
  theme-CSS prefix` thread-locally. Apply the cached prefix; run the dynamic-CSS tail through the passes
  as today. This hits across LABEL edits (key stable) → production-real, not a bench artifact.
- Prove byte-identity on `cargo test -p fm-render-svg` + workspace goldens, per-theme if possible, and a
  same-binary interleaved A/B on `er 40` / `seq` / `class 50` full pipeline (≥3% wall, CV<5%).

### The safe/simple pieces are BELOW the ≥3% bar (2026-07-24, cc) — only the strip-decision refactor clears it

Checked the lowest-risk piece, memoizing `Theme::to_svg_style` generation (a pure fn of the theme + 2
bools, safely equality-keyed, byte-identical). In the ER profile its self is **0.00%** (`to_svg_style`
0.29% total; `write_css_vars` 0.15%, `FontConfig::write_css` 0.45%, `core::fmt::write` 2.48% at 0.15%
self). Theme CSS GENERATION is cheap (~2-3% total) → memoizing it does NOT clear ≥3%. The ~20% is
almost entirely the STRIP passes' body-dependent needle scans (`memchr find_impl` 14.41% — the stack
shows the `fm-node-*`/shape needle strings) + their `memmove` rebuilds.

⇒ **The ONLY ≥3% win here is eliminating/memoizing the body-dependent strip DECISION**, which needs the
IR/config-derived feature fingerprint (node-type set, marker refs, shapes, clusters, dashed/thick) so the
memo hits across LABEL edits without re-scanning the SVG — a focused refactor of `strip_unused_state_css`
/ `strip_dead_marker_css` / `strip_unused_theme_css` with a wrong-CSS correctness surface (goldens cover
only the default theme). Confirmed NOT landable as a safe micro-lever; it is the standing focused-effort
item. This is the bd-1buv small-non-flowchart-render LEDGERED BLOCKER for the safe-micro-lever lane.

### Exhaustive confirmation across 3 workloads + code read (2026-07-24, cc) — no safe sub-lever remains

Profiled `dense`, `er 40`, `seq 40` full pipelines: ALL small non-flowchart renders share the identical
bottleneck — `render_svg_with_layout` self ~16-18% (CSS assembly) + `memchr find_impl` ~13-14% (strip
needle scans, stack shows shape/`fm-node-*` needles) + `memmove` ~6%. Read every strip pass:
`strip_unused_state_css` is gated + all-`memmem` with a SINGLE body walk (`scan_body_fm_node_classes`);
`scan_accent_var_refs` is already "ONE pass instead of 8"; the accent/var strip `find`s and the marker
passes are each individually optimized. The cost is INHERENT per-render re-processing of the invariant
CSS distributed across already-floored passes. Confirmed: **no single sub-fusion clears ≥3%** (the
accent-scan and marker-scan fuses are ~1-2% each, below floor); the ONLY ≥3% lever is memoizing the
WHOLE post-pass sequence keyed on an IR/config-derived body-feature fingerprint — a focused refactor
with a wrong-CSS correctness surface (re-couples the drift-proof body-based passes to renderer emission),
verified only by exhaustive per-diagram-type + per-theme byte-identity goldens. **This is a focused
architectural effort, NOT a safe micro-lever — the bd-1buv small-non-flowchart LEDGERED BLOCKER stands.**

## Marker-scan fuse (sub-lever, below floor)

`strip_unused_markers` (builds `referenced` from `url(#…)`, strips dead marker defs) and
`strip_dead_marker_css` (re-scans `<marker >` to rebuild the surviving `live` set, prunes dead
`marker#` selectors) redundantly scan markers. Passing the surviving set from the first to the second
elides one `<marker >` re-scan, but it is only ~1–2% and threading owned marker ids across two
`&mut String` passes (the rebuild invalidates `&str` borrows) is fiddly — below the ≥3% KEEP floor on
its own. Fold it into option 1/2 if that work happens, don't ship standalone.
