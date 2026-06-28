# Opportunity: render is byte-writing-bound — the next win is OUTPUT REDUCTION, not a micro-lever

**Crate:** `fm-render-svg` — **Date:** 2026-06-28 — **Agent:** BlackThrush
**Status:** the byte-identical render micro-lever frontier is reached; next win needs a contract decision.

## Standing (fresh, 16x32, after the 6 landed layout wins)

`wide_stages`: **parse 816 µs / layout 296 µs / render 1835 µs.** Render is ~62% of the wide
pipeline and dominant. Full-pipeline `full_pipeline_wide/16x32` ≈4.3 ms vs the pinned Mermaid
`11.12.0` 2879.185 ms (≈670× faster).

## Why render micro-levers are exhausted (all measured ~0 this session)

Render is **byte-writing-bound**: its time is dominated by appending the (byte-identical, required)
SVG output and formatting coordinates, not by construction (which is hot-free-list cheap).
Confirmed by reverted, measured ~0/regression levers:
- `Attributes::set` dedup removal (d805b50) — probe showed −24% but was load-contaminated; real ~2-4% noisy + global-semantic risk.
- `write_fixed2` 2-digit LUT (9f61618) — ~0/regression at 16x32; coords are too short for a table to beat the divide loop.
- Full-node direct-byte (21203f3) and direct-stream common edge (982bd3c) — ~0; per-element construction is hot-free-list cheap.
- **Teed up (unmeasured, blocked by infra this turn):** `describe_node` `format!`→`push_str` for the per-node `<title>` (the proven 93152f1 pattern, byte-identical). Likely ~1-2%; measure on `wide_stages/render` when the highs-sys worker pool is healthy.

## The real next win: output reduction (needs a contract decision)

Each node emits, by default, a11y output Mermaid's SVG may not: `<title>Node: …, rectangle</title>`
plus `role="graphics-symbol"`, `tabindex="0"`, `aria-label="…"` (≈90 bytes/node ≈ 46 KB at 16x32),
and (when enabled) `data-fm-*`. Since render is byte-writing-bound, every byte we emit that Mermaid
does not is pure overhead **and** a fidelity divergence.

**Action plan (for whoever owns the comparator — cod-b runs Mermaid 11.12.0 via Node+Chromium+CDP):**
1. Render the wide corpus in Mermaid and diff against ours: does Mermaid emit node `<title>` / `role`
   / `tabindex` / `aria-label`?
2. If **not**, gate them off by default (opt-in) to match Mermaid — a fidelity fix that is also the
   biggest remaining render perf win (~5-7% render est. from the title+a11y byte share). Regen the
   conformance snapshots in the same commit.
3. If Mermaid **does** emit them, they stay (byte-identity) and render is at its true floor.

This is a single contract decision, not a byte-identical micro-lever, which is why it is documented
here rather than landed unilaterally.

## Infra note

`fm-render-svg`/`fm-layout`/`frankenmermaid-cli` all pull `highs-sys` (a cmake/C build); the worker
pool is intermittently missing the toolchain, so render/layout benches build-fail / time out on
unlucky workers (retry across fresh per-attempt `CARGO_TARGET_DIR`s, or wait). `fm-parser`/`fm-core`
build cleanly everywhere (no highs-sys) — prefer parse-stage levers when the pool is degraded.
