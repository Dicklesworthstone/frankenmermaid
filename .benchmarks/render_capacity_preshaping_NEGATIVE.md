# REJECTED: capacity pre-shaping (`bd-6rxj`) — the over-reserved pages are never touched

**Date:** 2026-07-10 · **Agent:** cc_fm · **Base:** `00c5ca5` · **Verdict:** REJECTED, not attempted.
**Method:** analysis only — no builds (disk constraint). Existing symbolized binary + `perf stat`.

This also **self-corrects a wrong attribution I published earlier today** in `6f36e0a`.

---

## What I claimed this morning (and got wrong)

In the `DIG / NO-SHIP` entry for the large-diagram double copy I wrote that a 0.72% unresolved kernel chain was
*"page-fault / zeroing on the multi-MB output buffer"*, and that `layout_svg_capacity_hint`'s over-reservation
meant **"1.38 MB of surplus pages the kernel maps and zeroes — precisely the 0.72% kernel chain."** I filed
`bd-6rxj` to tighten the hint on that basis.

**That was inference, not measurement.** I never checked whether the surplus is touched.

## The measurement

`wide_40x80` (3200 nodes / 6201 edges / **3,475,207 B** output → the large slow path), `FM_H2H_FORCE_PROFILE=default`,
`perf stat` two-point delta (`reps=36` − `reps=6`, `warmup=2`), `taskset -c 11`, 60 renders per delta:

| event | delta | **per render** |
|---|---:|---:|
| `page-faults` | 886 | **14.8** |
| `minor-faults` | 917 | **15.3** |
| `instructions:u` | 4,065,008,919 | 67,750,148 |

Now compare against what the buffer would cost if it were faulted:

| quantity | bytes | 4 KiB pages |
|---|---:|---:|
| output actually written | 3,475,207 | **848** |
| `layout_svg_capacity_hint` reservation | 4,855,168 | 1,185 |
| **surplus (the thing `bd-6rxj` would remove)** | 1,379,961 | **336** |

**14.8 faults per render — not 848, and nowhere near 1,185.**

## Why, mechanically

1. `String::with_capacity(n)` calls `malloc(n)`. It **does not touch the memory.** Pages are faulted lazily on
   first *write*. We write 3.47 MB of the 4.85 MB reserved, so the 336-page surplus is **never touched** and
   costs nothing beyond a slightly larger `mmap` request.
2. Across iterations the 3.47 MB buffer is **recycled by mimalloc's free list** — it is not `munmap`'d and
   re-faulted per render. That is why the per-render fault count (14.8) is two orders of magnitude below the
   written-page count (848): the faults happen once, at the first allocation, and the two-point delta cancels them.

So the over-reservation is **free**, and tightening `NODE_BYTES` / `EDGE_BYTES` can only:
- save nothing on the upside (the surplus never faults), and
- risk a realloc + **whole-buffer memmove** on the downside if any diagram ever exceeds the tightened hint.

An under-reserving hint is strictly worse than an over-reserving one. **`bd-6rxj` is rejected.**

## Self-time of the frames involved (ledger-integrity rule)

`perf record -F 2500 --call-graph=dwarf`, `wide_40x80`, self-time as % of sampled pipeline:

| frame | self-time | what it really is |
|---|---:|---|
| `__memmove_avx` + `__memcpy_avx` | **2.54%** | see re-attribution below |
| ↳ via `String::write_fmt` / `push_str` → `copy_nonoverlapping` | dominant chain | **buffer append** — writing the output bytes |
| ↳ `alloc::str::join_generic_copy` | 0.37% | copy 1 (parallel chunks → `node_svg`/`edge_svg`) |
| ↳ `Element::write_to_string` → `push_str` | 0.36% | copy 2 (`raw_svg_parts` → final `String`) |
| kernel frames (`0xffffffff…`) | 18.27% | **not** output-buffer page faults (14.8/render). `--call-graph=dwarf` sampling overhead is the plausible bulk; unattributed. |

**Corrected reading:** the memmove frame is mostly the *unavoidable act of appending bytes to the output*, not a
redundant copy. The identifiable double copy is **0.73% of pipeline ≈ 3.4% of render** — which is what already
made the rope/arena output contract a NO-SHIP.

## Bonus finding: the double-copy frame provably did NOT move

Both of this session's render landings are structural no-ops on the large slow path — the lean edge fragment
only changes the a11y-off profile, and `bd-w5sn`'s post-pass early-returns above `POST_PASS_MAX_SVG_BYTES`.
Direct confirmation, `wide_40x80`, `perf stat -e instructions:u` two-point delta:

| binary | default profile | lean profile |
|---|---:|---:|
| `830d672` (pre-both) | 1.0000× | 1.0000× |
| `bc56f72` (lean edge fragment) | 0.9984× | **0.7274×** |
| `0f9efd4` (+ single-pass CSS) | 0.9984× | **0.7274×** |

Default moves **−0.16%**; `bd-w5sn` adds exactly its early-return check and nothing else. So the double-copy
frame could not have moved, and did not.

The lean column is the striking number: **−27.3% pipeline instructions on a 3200-node graph**, the largest
measured effect of the edge-fragment lever anywhere — larger than any corpus item (`wide_16x32` was 0.7359×).

## Where the large-diagram render time actually goes

| frame | self-time |
|---|---:|
| `attributes::write_uint_into` | 5.31% |
| `attributes::write_fixed2` | 3.61% |
| `path::build_smooth_path_by_into` | 2.54% |
| `attributes::write_escaped_attr` | 2.51% |
| `path::FmtNum::write_into` | 1.82% |
| `attributes::AttributeValue::write_value` | 1.47% |
| `attributes::write_escaped_text` | 1.44% |
| **total number-format + escape + path-emit** | **18.70%** |
| `__memmove_avx` + `__memcpy_avx` (all of it) | 2.54% |

**Number formatting is 7.4× the entire memmove frame** and accounts for 86% of `fm_render_svg`'s 21.79% self-time.

And that vein is already mature — the ledger says so explicitly: `write_uint_into` is digit-table optimized
(`e79a7bd`); the `itoa` crate was **rejected as a regression** (2026-07-02); removing its `&DIGIT_PAIRS[d..d+2]`
char-boundary checks measured **~30% SLOWER**; and a prior entry already classifies
`write_fixed2` + `write_uint_into` + `FmtNum` as *"the inherent byte production"*.

**Conclusion:** large-diagram render is at its byte-production floor. The remaining lever is not *how fast we
write the bytes* but *how many bytes we write* — which is the output-profile decision now sitting with the owner
(`docs/PROPOSAL_default_output_profile.md`). The lean column above is exactly that lever, already measured at
−27.3% on this graph.
