# WIN: memchr newline search in ByteLines::next (bd-1buv) — 2026-07-23

Agent: CopperCliff (cc). Base: `c905a663`.

## Lever

`ByteLines::next` (`fm-parser/src/mermaid_parser.rs:1079`) found each line's terminating `\n` with a
**scalar** byte scan:

```rust
match bytes[self.start..].iter().position(|&b| b == b'\n') {
```

Replaced with the SIMD `memchr`:

```rust
match memchr::memchr(b'\n', &bytes[self.start..]) {
```

Byte-identical: `memchr::memchr(b'\n', s)` returns the index of the first `\n` in `s`, exactly what
`.iter().position(|&b| b == b'\n')` returns. `memchr` is already a `fm-parser` dependency (used at
lines 1391/8171). This is the un-mined remainder of the earlier `.lines()`→`byte_lines` conversion
(`4957e16`, which removed CharSearcher's char decoding but left the newline search scalar).

## Why fresh / not the documented "memchr loses on short haystacks" trap

Memory cautioned single-byte memchr can lose to scalar `.position` on SHORT haystacks. Measured here:
it WINS on the non-flowchart parsers (class/er/state/sequence) whose member/attribute lines are
~20-40 bytes and NUMEROUS, and is FLAT (no regression) on flowchart whose edge lines are ~8 bytes
("N0-->N1") where memchr ≈ scalar. Modern `memchr` has a cheap short-haystack path, so the crossover
is well below typical non-flowchart line length. The scan is a top parse frame (`ByteLines::next`
8.1% self + its `position` closure 6.3% ≈ 14% of class parse), and this is a COMPUTE win (byte
scanning) — it does NOT mimalloc-wash.

## Measured (interleaved one-binary A/B, per-arm target dirs, C/O/O/C, standard bench profile)

| workload | ORIG | CAND | Δ |
|---|---|---|---|
| `class_stages/parse/64`  | ~82.7µs | ~80.1µs | −3.1% |
| `class_stages/parse/256` | ~349.4µs | ~327.1µs | **−6.4%** |
| `class_stages/parse/512` | ~697.7µs | ~673.5µs | −3.5% |
| `er_stages/parse/64`  | ~39.9µs | ~38.5µs | −3.5% |
| `er_stages/parse/256` | ~157.3µs | ~153.9µs | −2.2% |
| `er_stages/parse/512` | ~325.0µs | ~309.1µs | −4.9% |
| `parse/flowchart/small_10` | ~4.57µs | ~4.51µs | flat |
| `parse/flowchart/medium_100` | ~28.0µs | ~28.0µs | flat |
| `parse/flowchart/large_1000` | ~270.5µs | ~271.4µs | flat (no regression) |

Every CAND run beat its paired ORIG run on class/er; CV<5% both arms. Load 6-9. `perf stat`
instruction counts trend lower for CAND (fixed-time, so not directly comparable, but not higher).

## Behavior proof

415/416 fm-parser lib tests pass; the one failure (`flowchart_parses_chained_edges_left_to_right`,
asserts `node.classes == ["sankey-node"]`) is **pre-existing on clean HEAD** — verified by running it
against `git show HEAD:...` (fails identically), unrelated to newline scanning (another agent's WIP /
broken test on main, not touched per AGENTS.md). clippy `-D warnings` + `cargo fmt --check` clean.

## Verdict: KEEP. One-line change; wins non-flowchart parse 2-6%, flat on flowchart.
