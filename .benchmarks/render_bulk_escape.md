# Perf win: bulk-write XML escaping (30–60% faster render)

**Crate:** `fm-render-svg` · **Date:** 2026-06-24 · **Agent:** frankenmermaid-cc
**Verdict:** kept — 30–60% render speedup (p<0.05), byte-identical output.

## What changed

Every string attribute value (`AttributeValue::Display`) and every text-content node
(`Element::write_to_string`) was escaped by `escape_xml_attr` / `escape_xml_text`,
which: allocate a fresh `String`, iterate `s.chars()` (per-`char` UTF-8 decode), push
each character, and — for attributes — were then re-copied into the output by
`write!(f, "{}", escaped)`. A probe (skip escaping) showed this is **~34% of render**
(medium +51% / large +52% slower with escaping on).

Replaced with `write_escaped_attr` / `write_escaped_text`: a byte scan that bulk-copies
unescaped runs straight into the output (`f.write_str(&s[start..i])`) and writes the
entity for each special byte — no per-`char` decode, no intermediate allocation, no
double copy. Every escaped character (`& < > " '`, and `]` for the `]]>` look-back) is
ASCII, so byte scanning never splits a multi-byte UTF-8 sequence and slices land on
char boundaries.

## Correctness — proven byte-identical

A differential test (`bulk_escape_byte_identical_to_charwise`) compares the bulk
versions to the original char-by-char logic across special chars, the `]]>` /
`div > p` edge cases, multi-byte UTF-8 (`café ☕`, `résumé < β`), and emoji boundaries
(`🚀]]>🚀`). All **215 fm-render-svg tests pass** (snapshot/render tests unchanged) →
SVG output is byte-for-byte identical, conformance GREEN; clippy clean.

## Measurement — same-worker A/B (stash-swap, measurement-time 4)

| `render_svg/flowchart` | bulk faster by | p |
|------------------------|----------------|---|
| small_10  | **+60.5%** | <0.05 ✓ |
| medium_100| **+40.1%** | <0.05 ✓ |
| large_500 | **+29.8%** | <0.05 ✓ |

Escaping was the single largest render cost (more than float formatting); bulk-write
captures essentially all of it. This is the biggest render-stage lever found so far.
Combined with the float-format win (95e0150), SVG serialization is now dramatically
cheaper without any output change.
