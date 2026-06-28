# Negative: remove the `Attributes::set` dedup retain — ~2-4% (probe was load-contaminated), REVERTED

**Crate:** `fm-render-svg` — **Date:** 2026-06-28 — **Agent:** BlackThrush
**Verdict:** marginal / noisy (~2-4%, mostly <3%) + a global-semantic correctness risk — reverted.

## Lever

`Attributes::set` runs `self.attrs.retain(|a| a.name != name)` before pushing — an O(k) dedup
scan on every attribute, ~6-7× per node element. For the common element the names are all
distinct, so the retain removes nothing. Tried making `set` a plain append (no dedup) with a
separate `set_replace` (retain) for the one caller that genuinely re-sets a name (the sequence
mirror-header `id` override, `Element::id_replace`).

## What the probe claimed vs reality

A first **ceiling probe** (skip the retain entirely, byte-identical on `gen_wide` since its node
attrs are distinct) measured ORIG 631 µs vs OPT 482 µs at 8x16 = **−24%** — looked like a huge
win. It was **load-contaminated**: on the real both-order A/B the same ORIG (retain) code
measured **495 µs**, not 631 — the probe's ORIG phase had hit a load spike on the shared box,
inflating the baseline. **Lesson: a single-direction A/B baseline can be silently inflated by a
mid-run load spike; always cross-check the ORIG absolute against a both-order run before trusting
a big number.**

## Real measurement

Same-worker both-order A/B, fresh dir `mermaid-bt10`, `wide_stages/render`, mt=4. All 225
fm-render-svg tests + conformance pass (byte-identical; only the mirror-header `id` re-set needed
the `set_replace` fix).

| bench | ORDER_A (ORIG vs opt) | ORDER_B (OPT vs orig) | geo-mean OPT/ORIG |
|---|---:|---:|---:|
| `…/8x16`  | −4.2% (ORIG faster) | −6.9% (OPT faster) | ~0.99 (**~1.4% faster**) |
| `…/12x24` | −0.6% (NS) | −8.0% (OPT faster) | ~0.96 (**~3.8% faster**) |
| `…/16x32` | +2.3% (NS) | −2.1% (NS) | ~0.98 (**~2.2% faster**) |

Net ~2-4%, noisy, mostly below the 3% bar. The `set` retain is NOT the ~8-24% cost earlier
notes/probes suggested — for the small per-element attr counts (k≈6-7) the O(k) scan is cheap.

## Why reverted (do-not-retry as a global change)

Making `set` no-dedup is a **global semantic change**: every diagram type's element building
loses the dedup safety net. The full test suite passing reduces but does not eliminate the risk
of an *untested* re-set in some diagram/config producing a duplicate attribute (invalid SVG) in
production. A noisy ~2-4% render gain does not justify trading away that safety. Render remains
byte-writing-bound; its per-element construction (including `set`) is hot-free-list cheap, as the
reverted full-node direct-byte (21203f3) and direct-stream edge (982bd3c) also found.
