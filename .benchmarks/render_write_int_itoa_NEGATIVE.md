# Negative: direct itoa for integer attrs (write_int) — byte-identical, but sub-noise/unmeasurable, REVERTED

**Crate:** `fm-render-svg` — **Date:** 2026-06-29 — **Agent:** BlackThrush
**Verdict:** byte-identical + mechanistically win-or-neutral, but its signal is below the render bench
noise floor (±3-10%) so it is NOT a measurable win. Reverted per REVERT-~0-gain.

## The lever (uncovered — distinct from the write_fixed2 LUT 9f61618)

`AttributeValue::write_value` formats integer coords (`n.fract()==0`) and **every `Integer` attribute**
(`data-fm-edge-id` ×960/render, node accent/index, …) via `write!(out, "{}", i)` — the `fmt::Formatter`
/`Arguments` dispatch. `write_fixed2` (the fractional path) already avoids this with a direct
stack-buffer itoa; the integer paths did not. Added `write_int` (same direct itoa) and used it at both
sites. **Byte-identical — 226 fm-render-svg tests pass.** Mechanistically strictly less work than
`write!`, so win-or-neutral (can't regress).

## Why not landed: below the render bench noise floor

Both-order A/B on `wide_stages/render` (hw1, a cmake-healthy worker): **contradictory** — 8x16 ORDER_A
−11% (OPT slower) vs ORDER_B −12% (OPT faster); the ±12% swings are 4-phase load drift, not signal.
**Null A/B (identical OPT code both phases)** quantifies it: 8x16 +2.8% (NS), 12x24 +2.5% (NS), **16x32
+9.8% (p=0.00) — false-significant from identical code.** So the render bench noise floor is ±3-6%
(8x16/12x24) to ±10% (16x32) **even on a healthy worker** (load-drift-bound, not magnitude-bound). The
itoa's expected gain is ~0.5-2% (the fmt overhead over ~1500-2500 short-int writes; rustc already
compiles `write!("{}", i32)` fairly tight), well under the floor. Not a MEASURED win ⇒ reverted.

## Methodology finding (general)

**The large render bench (~0.5-3 ms) is as load-drift-bound as the small layout bench** — null A/B
±3-10% — so **render micro-levers (<5%) are unmeasurable on this fleet regardless of bench magnitude.**
Only a ≥10% render change clears the floor. The only such lever is the output reduction (a11y ~12% /
data-* ~7%, `render_a11y_data_reduction_MEASURED.md`), which is owner-decision-gated. Render is
effectively closed to measurable micro-optimization here; pursue parse-stage levers (large_1000 is
clean) or the owner-gated output reduction. (`write_int` is a safe byte-identical drop-in if a
future quiet/idle box can confirm it, but it is not worth landing unconfirmed.)
