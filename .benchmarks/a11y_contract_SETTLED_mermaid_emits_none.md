# SETTLED: mermaid emits **zero** per-element a11y — so keep our a11y-full default, and fix the lean path instead

**Date:** 2026-07-09 · **Agent:** cc_fm · **HEAD:** `59b237b` · **Bead:** bd-b2b6
**Supersedes the owner-decision framing in `edge_a11y_is_19pct_render_MEASURED.md` and
`BLOCKER_render_perf_double_gated.md`.** Both said the a11y contract decision was blocked on "cod-b's
Mermaid comparator" confirming whether mermaid emits per-element `<title>`/`role`/`aria`. The
comparator now exists (`scripts/headtohead/`, bd-1buv.1). The answer is measured, not assumed.

## The measurement

Same graph (`wide_8x16`: 128 nodes, 224 edges), byte-identical input, mermaid 11.15.0 pinned bundle
vs frankenmermaid at `59b237b`. Dumped with `mermaid_bench.mjs --dump-svg` and
`headtohead <corpus.json> <dump-dir>`.

| output | bytes | `role=` | `tabindex` | `aria-label` | `<title>` | `<desc>` |
|---|---:|---:|---:|---:|---:|---:|
| mermaid 11.15.0 | 292,024 | **1** | **0** | **0** | **0** | **0** |
| frankenmermaid default | 134,629 | 353 | 352 | 128 | 353 | 1 |
| frankenmermaid lean (`A11yConfig::none()`) | 93,077 | 0 | 0 | 0 | 0 | 0 |

mermaid's single `role` is the root `role="graphics-document document"` (plus one
`aria-roledescription="flowchart-v2"`). It emits **no** `<desc>`, `aria-describedby`,
`aria-labelledby`, `accTitle` or `focusable` either. Our 353/352/128/353 are exactly
`1 root + 128 nodes + 224 edges`, `128 + 224`, nodes-only, and `1 + 128 + 224` — internally consistent.

Per-element a11y costs **41,552 bytes = 30.9% of our default output**.

## Why this settles the decision the other way

The recorded framing offered two directions and called dropping a11y "the biggest measurable render win
on the board" (−19% render). That framing is now wrong on its premise:

1. **We are already 2.17x smaller than mermaid *with* full per-element a11y**, and 2174x faster on this
   item. Output-size dominance does not require dropping anything.
2. **Our default is strictly more accessible than the original.** Dropping per-element
   `role`/`tabindex`/`aria-label`/`<title>` to reach "mermaid parity" would trade a real, shipped
   accessibility advantage for a 19% render win on a path that already beats the comparator by three
   orders of magnitude. That is a bad trade at any exchange rate.
3. Mermaid-parity output is still available today via `A11yConfig::none()` for users who want it.

**Recommendation to the owner: keep `A11yConfig::full()` as the default.** Do not pursue the uniform
bare-edge reduction. The a11y `<g>`/`<title>` is a differentiator, not dead weight.

## The real lever this exposes (bd-b2b6)

The lean profile is **1.5–2.02x SLOWER than our own default** on 11/13 corpus items, because the
streaming node/edge fast paths in `crates/fm-render-svg/src/lib.rs` are gated on
`a11y.aria_labels && a11y.keyboard_nav && a11y.text_alternatives` (~5746, 5798, 5843, 5902, 6063).
With a11y off, every element falls back to the per-element `Element` builder. **Less output currently
costs more work** — which is backwards.

Fix: emit lean fragments from the fast path and relax the gate to "a11y uniformly on **or** uniformly
off". `SvgRenderConfig::default()` keeps `A11yConfig::full()`, so **default output is unchanged and no
golden needs re-blessing**.

**Verification oracle (already built):** `headtohead <corpus.json> <dump-dir>` writes
`<id>.lean.svg` per item. Capture the lean SHA-256 before the change and require it to be unchanged
after — that proves the fast path reproduces the slow path's lean bytes exactly. Then re-run
`node scripts/headtohead/run.mjs` and require `lean_slowdown <= 1.0` on every row.

## Do-not-retry

- Do not "complete the reduction" by stripping the slow path's `<g>`/`<title>` and re-blessing the 37
  goldens. The comparator says there is nothing to catch up to.
- Do not treat the 30.9% byte share as a render-time share: render is not purely byte-bound (the
  `data-fm-source-*` entry measured 35% bytes but only ~12% render).
