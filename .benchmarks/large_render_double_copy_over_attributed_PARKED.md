# PARKED: large-diagram render — the double-copy is over-attributed; the real sub-frames, and one contained lever

**Date:** 2026-07-10 · **Agent:** cc_fm · **Base:** `0f9efd4` · **Status:** ANALYSIS ONLY. No code changed.
**Why parked:** see "Blocker" at the bottom. (`tests/artifacts/` is gitignored, so this tracked copy under
`.benchmarks/` is the canonical one; the working copy lives at `tests/artifacts/perf/`.) Nothing here has been measured as an A/B, because under the
current constraints there is **no valid A/B substrate** for a render-path lever.

---

## Ledger-grep first (three standing rejections on this exact frame)

1. **Large `to_string_with_body`** (2026-07-05) — rejected; lost the parallel/raw-parts shape.
2. **Raw-part body fusion** (`cod_fm`, 2026-07-10, `.benchmarks/render_large_raw_part_body_fusion_NEGATIVE.md`)
   — **+22.65% regression**, p=0.01, reverted. Its do-not-retry: *"Do not retry this pre-rendered-chunks-through-
   `to_string_with_body` shape. A future attempt needs a different output contract, such as a segmented/rope SVG
   result or caller-provided writer that avoids requiring one final contiguous `String`."*
3. **Empty between-child guards** (`cod_fm`, 2026-07-10) — flat / +0.85%, reverted. Targeted a 0.49% frame.

The frontier map (`DIG`, 2026-07-04) calls the large-diagram double-copy *"THE concrete remaining lever"* and
estimates **~3% on large-diagram render**. cod's profile of `large_wide_stages/render/40x80` reports
`__memmove_avx_unaligned_erms` at **14.82%** of the render loop.

## What the profile actually says (this is the correction)

The corpus tops out at 512 nodes — **below** the streaming gate at `lib.rs:2892`
(`edges < 4096 && nodes < 2048`), so no pinned corpus item exercises the slow path at all. I generated a
`wide_40x80` flowchart (**3200 nodes / 6201 edges / 3,475,207 B output**) and profiled it with the existing
symbolized binary. (That binary predates `bd-w5sn`, which is sound here: `strip_unused_state_css` early-returns
above 100 KB, so on this input it executes identical code — confirmed by the exact-`1.0000×` null controls in
`.benchmarks/postpass_single_pass_scan.md`.)

`perf record -F 2500 --call-graph=dwarf`, self-time, **as a share of the whole parse+layout+render pipeline**:

| bucket | self-time |
|---|---:|
| `fm_render_svg::*` | 21.79% |
| `fm_parser::*` | 14.67% |
| `fm_layout::*` | 10.41% |
| `sha2` (harness output hashing, not the engine) | 5.37% |
| `__memmove_avx` + `__memcpy_avx` | **2.54%** |

2.54% of pipeline ÷ 21.79% render = **≤11.7% of render**, consistent with cod's 14.82% on a render-only bench.
But the folded call-chains show that memmove is **not mostly the double copy**:

| chain | self-time (pipeline) | what it is |
|---|---:|---|
| unresolved kernel chain (`0xffffffff…`) | 0.72% | page-fault / zeroing on the multi-MB output buffer |
| `__memcpy_avx_unaligned_erms` (inlined, unattributed) | 0.72% | — |
| `write_common_node_fragment_into::<true>` → `String::write_fmt` | 0.37% | **streaming buffer growth**, not a copy |
| `alloc::str::join_generic_copy` | 0.37% | **copy 1** (parallel chunks → `node_svg`/`edge_svg`) |
| `Element::write_to_string` → `push_str` | 0.36% | **copy 2** (`raw_svg_parts` → final `String`) |

**The identifiable double-copy is ≈0.73% of pipeline ≈ 3.4% of render.** The rest of the memmove frame is
ordinary `String` growth inside the streaming writers plus kernel page-fault cost on a 3.47 MB allocation.

This retires the "one concrete remaining lever" framing and **explains all three rejections at once**: each
attacked a ~0.4% frame. A segmented/rope output contract — a public API change to `render_svg_with_layout`'s
return type — would buy at most ~3.4% of render on a 3200-node diagram. That is a bad trade, and it is why I am
**not** proposing it.

## The one genuinely different shape the profile does support

The slow path presizes via `finish_layout_svg_document` → `to_string_with_capacity(layout_svg_capacity_hint(..))`
(`lib.rs:3187`, `3202`), where the hint is `16 KiB + 768·nodes + 384·edges + 512·clusters + 192·aux`.

It does **not** under-reserve. It **over**-reserves, everywhere:

| item | nodes | edges | hint | actual | over-reserve |
|---|---:|---:|---:|---:|---:|
| flowchart_small_10 | 10 | 9 | 27,520 | 13,218 | **2.08×** |
| flowchart_medium_100 | 100 | 99 | 131,200 | 72,575 | 1.81× |
| flowchart_large_500 | 500 | 499 | 592,000 | 343,946 | 1.72× |
| wide_8x16 | 128 | 224 | 200,704 | 134,629 | 1.49× |
| wide_16x32 | 512 | 960 | 778,240 | 534,365 | 1.46× |
| dense_dag_200 | 200 | 790 | 473,344 | 355,447 | 1.33× |
| **wide_40x80** | 3200 | 6201 | 4,855,168 | 3,475,207 | **1.40×** |

On `wide_40x80` that is **1.38 MB of surplus pages** the kernel must map and zero — which is exactly where the
0.72% unresolved kernel chain (and the 2.01% `[k] 0xffffffff…` frame) comes from. So the contained, byte-identical
lever is **tighten `layout_svg_capacity_hint`**, not rewrite the output contract. Two sub-questions the data
raises but does not answer:

1. Are `NODE_BYTES = 768` / `EDGE_BYTES = 384` simply stale, or do they cover a worst case (long labels, inline
   styles, source spans) that the corpus never hits? A hint that is too *small* costs a realloc + full-buffer
   memmove — strictly worse than over-reserving. **This must be measured against a label-heavy corpus, not just
   the flowchart generators**, before any constant moves.
2. Does `mimalloc` even fault in the surplus? A `reserve` that never gets written may cost nothing beyond the
   `mmap`. The kernel frames say it costs *something*; they do not say how much.

**Do not** change the constants on the strength of this table alone. The measurement below is the gate.

## Blocker (why this is parked, not landed)

A render-path lever needs an A/B. Under today's constraints there is **no valid substrate** for one:

- **Local `cargo bench` is prohibited** (disk emergency: `/data` at 96%).
- **`rch exec` cannot carry a split A/B.** Per the `franken_networkx` `br-r37-c1-839yx` addendum, `rch exec`
  exposes no worker-pinning flag and selects workers non-deterministically, so ORIG and CAND measured in two
  invocations are not comparable. I confirmed this empirically: I passed `RCH_WORKER=hz1` for both arms of the
  `bd-w5sn` criterion A/B and **both were silently scheduled onto `hz2`** — same worker by luck, not by control.
- The prescribed substrate — **both arms in ONE binary and ONE invocation**, ORIG kept as a bench-only reference
  fn in an alternating criterion group — is not expressible for this lever today: `layout_svg_capacity_hint` is
  private and the capacity is not a parameter of any public entry point, so there is no way to register two arms.

**To unpark, do one of:**
1. Thread the capacity hint through a `#[doc(hidden)]` or `#[cfg(feature = "bench")]` parameter on
   `render_svg_with_layout`, then register `hint_current` vs `hint_tight` as two arms of one criterion group in
   `crates/fm-cli/benches/pipeline_bench.rs`, and run it in a single `rch exec -- cargo bench` invocation; or
2. Wait for the disk constraint to lift and run the standard same-machine `perf stat -e instructions:u`
   two-point A/B with a code-layout control (the substrate that carried `bd-w5sn`; it is machine-local,
   deterministic, and immune to both the worker-selection problem and the ±5% code-layout roulette).

Option 2 is preferred: instruction counts already proved able to resolve a 0.03% effect on this codebase.
Note that a capacity change is **byte-identical by construction** (it only affects `String::with_capacity`), so
the parity burden is trivial — the whole difficulty is measurement, not correctness.
