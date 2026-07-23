# REJECT: minify_css bulk-copy non-whitespace runs (2026-07-23)

Agent: CopperCliff (cc). Base: `510c296b`. Lane: bd-1buv.

## Lever

`minify_css` (fm-render-svg/src/lib.rs ~698) copies non-whitespace CSS content one byte at a time in
its common `other` branch (`out.push(other); i += 1;`). Candidate replaced it with a run scan +
bulk `extend_from_slice` (memcpy):

```rust
_ => {
    let run_start = i;
    i += 1;
    while i < n && !matches!(b[i], b' ' | b'\t' | b'\n' | b'\r') { i += 1; }
    out.extend_from_slice(&b[run_start..i]);
}
```

Byte-identical (256/256 fm-render-svg tests incl `minify_css_is_whitespace_only_and_preserves_semantic_spaces`).

## Measured — REGRESSION on some diagrams (interleaved one-binary A/B, C/O/O/C)

`render_nonflowchart` (standard bench profile, load 6-15):
| workload | ORIG (push/byte) | CAND (bulk-copy) | Δ |
|---|---|---|---|
| er_40 | ~77.6µs | ~81.9µs | **+5.5% (slower)** |
| c4_40 | ~210µs | ~218µs | **+3.8% (slower)** |
| pie_40 | ~55.5µs | ~56.8µs | +2% |
| sequence_40 | ~70.9µs | ~71.3µs | flat |
| sankey_60 | ~91-98µs | ~91µs | ~flat (noisy) |

er_40 and c4_40 are direction-consistent regressions (both CAND runs > both ORIG runs).

## Why it regressed

The theme `<style>` CSS that `minify_css` processes is PRETTY-printed (2-space indent + newline per
line + spaces around tokens), so whitespace is FREQUENT and the non-whitespace runs between it are
SHORT (~4-12 byte tokens: property names, short values). For short runs, the added inner-loop
(second `matches!` scan to find the run end) + `extend_from_slice` setup (slice construction, bounds
check, memcpy call) cost MORE than the handful of byte pushes it replaces. Bulk-copy only wins on
LONG contiguous runs, which pretty CSS does not have. The simple per-byte push was already optimal
for this whitespace-dense input.

## Verdict: REJECT. Reverted to `510c296b`.

## Do Not Retry

Do not retry bulk-copying `minify_css`'s non-whitespace runs on the pretty theme CSS — its runs are
too short for `extend_from_slice` to beat per-byte push. Retry only if the theme CSS is first emitted
MINIFIED (long dense runs), which is the separate bd-dh1c effort — and even then `minify_css` on
already-minified input is a near-noop that shouldn't run (skip it instead). The `minify_css` byte
scan (~9.5% of small-diagram render) is at its floor for a within-function optimization; the only
real win is skipping it entirely for the invariant theme CSS, which bd-dh1c documents as blocked on
the strip-pass interleaving (no stable cacheable boundary).
