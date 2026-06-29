# Root cause: golden_svg RED = edge fast path emits a bare `<path>`, dropping `<g>`/`<title>`

**Crate:** `fm-render-svg` — **Date:** 2026-06-29 — **Agent:** BlackThrush
**Severity:** byte-identity guard RED on main 7+ turns; this is the precise code-level cause (read-only
diagnosis — can't run golden_svg this turn, the highs-sys outage blocks all fm-cli builds).

## The divergence

`render_edge` has a fast path (lib.rs:6215-6232): for the common edge (`arrow == Arrow`, themed CSS,
no back-edge / animation / source-spans / marker-start / dasharray / inline-style, **no rendered
label**, with a marker-end) it returns `Element::raw_svg(build_common_edge_fragment(...))`.

`build_common_edge_fragment` (lib.rs:6011-6032) emits **only**:
```
<path d="…" stroke-width="…" class="fm-edge …" data-fm-edge-id="N" marker-end="…"/>
```
— a **bare `<path>`: no `<g class="fm-edge" … role="graphics-symbol" tabindex="0">` wrapper and no
`<title>`**. The slow path wraps the same edge in `<g …><path/><title>…</title></g>` (the goldens show
`<g id="fm-edge-1">…<title>Start connects to Line</title></g>` for edges that miss a fast-path
condition). So the fast path drops the group + a11y title → not byte-identical → golden mismatch on
`dense_flowchart_stress` (which has common solid-arrow edges that hit the fast path).

## Why CI didn't catch it: the pin test is insufficient

`edge_fast_fragment_matches_element` (lib.rs:6504) asserts the fragment equals a **hand-built bare**
`Element::path().d().stroke_width().class("fm-edge").class("fm-edge-solid").attr_int("data-fm-edge-id").marker_end()`
— it pins the fragment against a *bare path the author assumed the slow path produces*, never against
`render_edge`'s real output for an equivalent edge. So the unit test is a tautology (fragment == bare
path == fragment) and passes even though the slow path actually emits `<g>`/`<title>`. Contrast the
node fast path (66ff940 / `build_common_node_fragment`), which *does* emit `<title>` and is pinned by
`node_fast_fragment_matches_render`.

## Fix options (render owner)

1. **If the `<g>`/`<title>` is wanted (a11y kept):** make `build_common_edge_fragment` emit the
   `<g class="fm-edge …" role="graphics-symbol" tabindex="0">` wrapper + the describe_edge `<title>`,
   OR add `&& <no a11y title for this edge>` to the fast-path gate so titled edges use the slow path.
   Then re-bless golden_svg. **And fix the pin test** to compare against `render_edge(...)`'s real
   output, not a hand-built bare path — else this regresses again.
2. **If the bare edge is intended (part of the a11y/output reduction, ~12% — see
   `render_a11y_data_reduction_MEASURED.md`):** then the slow path should drop the edge `<g>`/`<title>`
   too (uniformly), and re-bless all goldens. But that is the contract decision (cod-b's comparator),
   not a silently-diverging fast path.

This corrects my earlier 90446ae hedge ("re-bless or fix"): it is a **fast-path-vs-slow-path
divergence**, not a stale golden — the fast path and slow path disagree, so re-blessing alone would
bake in inconsistent output (fast-path edges bare, slow-path edges titled).

## Update (71c1d76 follow-up): the divergence is BIGGER than just `<title>` — complete fix spec

Reading the full slow path (render_edge:6401-6440) this turn, the bare fast-path `<path>` is missing
MORE than the a11y title:
- **`id="fm-edge-N"` is missing for EVERY config** — the slow path always appends
  `elem = elem.id(&mermaid_edge_element_id(edge_index))` (line 6438), even for non-a11y edges. The
  fast-path `build_common_edge_fragment` never emits an `id`.
- **When `a11y.text_alternatives`** (default on): slow path wraps in
  `<g id="fm-edge-N" class="fm-edge" data-fm-edge-id="N" [role="graphics-symbol"] [tabindex="0"]>{path}<title>{describe_edge_labels(...)}</title></g>`
  (role gated on `a11y.aria_labels`, tabindex on `a11y.keyboard_nav`). Fast path emits none of it.

So the complete fix (preferred — keeps the direct-byte speed, the peer's node-fragment pattern
66ff940): `build_common_edge_fragment` must take the a11y flags + the `describe_edge` text and emit:
- a11y-off, no aria/kbd → `<path d=… stroke-width=… class="fm-edge {cls}" data-fm-edge-id="N" marker-end=… id="fm-edge-N"/>`
- a11y-on → the full `<g …>{path}<title>…</title></g>` above.
And **fix the tautological pin test** `edge_fast_fragment_matches_element` to assert
`build_common_edge_fragment(...) == ` the **real** `render_edge(...)` serialization for an unlabeled
edge across the a11y-flag matrix (not a hand-built bare `Element::path()`).

Alternatively gate the fast path on the a11y/id conditions — but that disables it for the default
(a11y-on) config, regressing the ~40%-of-render direct-byte path. This is render-owner work (their
fast path + their node-fragment expertise); it is the conformance half of the prerequisite to the
measurable ~12% render output-reduction win.
