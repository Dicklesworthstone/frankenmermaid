# Negative: 2-digit "00".."99" LUT for `write_fixed2` integer part — ~0/regression at 16x32, REVERTED

**Crate:** `fm-render-svg` — **Date:** 2026-06-28 — **Agent:** BlackThrush
**Verdict:** direction-inconsistent / regression at the headline size — reverted. Do not retry.

## Lever

`write_fixed2` (the per-coordinate fixed-2-decimal formatter, ~7000 calls on wide render —
edge `d`-strings + node coords) extracts the integer part digit-by-digit (`% 10` / `/= 10` per
digit). Tried the classic itoa **two-digits-at-a-time** technique: a 200-byte `b"0001…99"` table,
emitting two integer digits per `i64 /100` plus writing both frac digits straight from the table.
Genuinely different formatting primitive.

## Correctness

Byte-identical: 225 render tests (incl. `write_fixed2_byte_identical_to_std_format`, a ~6M-value
sweep vs `{:.2}`) + conformance pass.

## Measurement

Same-worker both-order A/B, fresh dir, `wide_stages/render`, mt=4. Bias-corrected geo-mean
OPT/ORIG:

| bench | ORDER_A | ORDER_B | geo-mean |
|---|---:|---:|---:|
| `…/8x16`  | −0.3% (NS) | −2.7% | ~1.3% faster |
| `…/12x24` | +8.2% (OPT faster) | −0.2% (NS) | ~4% faster |
| `…/16x32` | −2.8% (ORIG faster) | +5.6% (OPT slower) | **~4% SLOWER** |

**Direction-inconsistent across sizes**, and OPT is SLOWER in **both** orders at 16x32 (the
biggest, most-stable bench, most write_fixed2 calls). 12x24 "faster" is the noisy one.

## Why it does not pay (do-not-retry)

Coordinates are short — 2-4 integer digits — so the digit-by-digit loop is only ~2-4 cheap
integer divides, already fast. The 200-byte table adds a cache-line load + an index computation
per call that is NOT amortized at that length; the LUT only wins for long integers (10+ digits),
which coordinates never are. Same class as the reverted `write_int` (0a... / 376056f) and the
`classify`-table (bd-9e7c): the "table beats arithmetic" assumption fails when the data is short
and the arithmetic path is already lowered well. `write_fixed2` is at its floor; render is
byte-writing-bound.
