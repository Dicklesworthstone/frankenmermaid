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

## Marker-scan fuse (sub-lever, below floor)

`strip_unused_markers` (builds `referenced` from `url(#…)`, strips dead marker defs) and
`strip_dead_marker_css` (re-scans `<marker >` to rebuild the surviving `live` set, prunes dead
`marker#` selectors) redundantly scan markers. Passing the surviving set from the first to the second
elides one `<marker >` re-scan, but it is only ~1–2% and threading owned marker ids across two
`&mut String` passes (the rebuild invalidates `&str` borrows) is fiddly — below the ≥3% KEEP floor on
its own. Fold it into option 1/2 if that work happens, don't ship standalone.
