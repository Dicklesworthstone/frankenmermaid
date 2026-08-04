# Negative: direct-stream the common-edge fragment (skip per-edge String) — ~0/regression, REVERTED

**Crate:** `fm-render-svg` — **Date:** 2026-06-28 — **Agent:** BlackThrush
**Verdict:** ~0 / slight regression — reverted (uncommitted), do not retry.

## Lever (a DIFFERENT primitive than the prior render wins)

The common-edge fast path serializes via `Element::raw_svg(build_common_edge_fragment(..))`,
where `build_common_edge_fragment` allocates a per-edge `String f` (`with_capacity(path_str.len()
+ 96)`), builds the `<path>` fragment into it, and the edge loop then `write_to_string`s `f` into
the shared `edge_svg` buffer and drops it — i.e. a per-edge alloc + a second copy of the fragment
bytes (~960× at 16x32). The lever wrote the fragment **directly into the shared `edge_svg`
buffer** (`write_common_edge_fragment(&mut edge_svg, ..)`), returning an empty `raw_svg` no-op —
eliminating the intermediate `String` and the second byte copy. This is a genuinely different
primitive from the prior streaming/direct-byte edge wins (a4f6cff/41d3a1b), which removed the
Element/Attributes tree but still allocated `f`.

## Correctness

Byte-identical (same bytes, just written once into the shared buffer instead of via an
intermediate String): 225 fm-render-svg tests + `frankentui_conformance_test` pass.

## Measurement

Same-worker both-order stash-swap A/B, fresh dir `mermaid-bt4`, `wide_stages/render`, mt=4.
Signs flipped between orders (the recompiled second phase runs ~warm/faster), so the directional
`change:` is dominated by run-order bias. Bias-corrected (geometric mean of OPT/ORIG across both
orders):

| bench | ORDER_A (ORIG vs opt) | ORDER_B (OPT vs orig) | bias-corrected |
|---|---:|---:|---:|
| `wide_stages/render/8x16`  | −0.05% (NS) | −3.7% | ~neutral / ~1.8% faster |
| `wide_stages/render/12x24` | −10.8% (ORIG faster) | −4.2% (OPT faster) | **~3.6% SLOWER** |
| `wide_stages/render/16x32` | −3.4% (ORIG faster) | −0.5% (NS) | ~1.5% slower |

Absolute P1(OPT)-vs-P2(ORIG) agreed: 12x24 OPT 1.2795 ms vs ORIG 1.1186 ms (+14%), with ORIG
running second (penalized) — so OPT is genuinely the slower side. Net: **~0 to slight regression.**

## Why it does not pay (do-not-retry)

Same lesson as the reverted per-line `line_items` Vec elision (5d1ccbc) and the streaming model
generally: a small buffer that is **alloc'd + freed every loop iteration is recycled hot from
the allocator free list**, so removing it saves ~nothing — and writing into the *large* shared
`edge_svg` buffer (with a per-edge `reserve`) has slightly worse locality / call overhead than
building each fragment in its own small cache-hot `String` and doing one bulk `write_str`. The
per-edge `String` is NOT a real cost post-streaming; render remains byte-writing-bound. Do not
retry "eliminate the per-edge fragment String" for nodes or edges — the hot free list already
makes it free.
