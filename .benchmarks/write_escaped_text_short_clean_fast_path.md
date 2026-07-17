# WIN: clean fast path for write_escaped_text short strings (bd-1buv.61)

**Date:** 2026-07-16 · **Agent:** BlackThrush · **File scope:** `crates/fm-render-svg/src/attributes.rs`
(`write_escaped_text`).

## Profile

Full-pipeline `perf record` (flowchart 300, default profile): `write_escaped_text::<String>` = **3.96%**
self-time — the 4th-hottest render frame. `perf annotate` shows the hot instructions are all in its
**per-byte scalar escape loop** (byte load `movzbl` 8.4%, `inc` 10.6%, loop-bound `cmp` 7.3%, and the three
`cmp $0x3e/$0x26/$0x3c` = `> & <` compares at ~3.5–5% each). That loop is the SHORT-string path taken by
node/edge labels + every `<title>` text (~600×/render for a 300-node flowchart) — NOT the CSS memchr2 path.

`write_escaped_attr` already had a vectorizable `.any()` clean fast path; `write_escaped_text` did not.

## The lever

Add the same clean fast path — a single `!bytes.iter().any(|&b| matches!(b, b'&' | b'<' | b'>'))` scan
(auto-vectorizes to SIMD) — **after** the length-gated CSS memchr2 path (so the ~5 KB `<style>` never pays
the extra scan) and **before** the per-byte loop. The common label with no special byte bulk-copies in one
`write_str`. Byte-identical: with no `&`/`<`/`>` the loop would emit `s` verbatim, and a `>` escapes only
inside `]]>` — which necessarily contains a `>`, so any `]]>` fails the check and falls through to the loop.

## Measured (non-LTO release opt=3, profharness `render`, `perf stat -e instructions:u`)

| shape | render instr ratio |
|-------|--------------------|
| flowchart (300 nodes) | **0.950x (−5.0%)** |
| er (100) | 0.972x (−2.8%) |
| seq (100) | 0.980x (−2.0%) |
| flowchart (100) | 0.981x (−1.9%) |
| mindmap (100) | 0.983x (−1.7%) |
| state (100) | 0.988x (−1.2%) |
| class (100) | 0.990x (−1.0%) |
| gantt (100) | 0.993x (−0.7%) |

The win scales with label/title count (more nodes → the fixed CSS is a smaller fraction). flowchart render
wall ~0.95–0.99x. Every shape benefits (all emit labels + `<title>`s through this function).

## Byte-identity

SHA-256 of full SVG dump matched baseline (HEAD build) across 12 shapes (flowchart/class/er/erattr/state/
seq/sankey/gantt/mindmap/requirement/journey/pie). 256 fm-render-svg tests + clippy `-D` green.
