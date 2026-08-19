# PLAN_TO_PORT_FRANKENTUI_MERMAID_TO_RUST

## Goal

Reach behavioral parity for the Mermaid extraction/rendering surface that was
previously embedded in FrankenTUI, while keeping frankenmermaid Rust-native,
modular, deterministic, and free of compatibility shims.

This plan follows the `porting-to-rust` skill rule:

1. Extract spec from legacy/reference sources.
2. Implement from that spec.
3. Prove behavior with conformance tests and a parity ledger.

## Source Of Truth

Behavioral reference lives in the FrankenTUI extraction sources listed in
[`AGENTS.md`](/data/projects/frankenmermaid/AGENTS.md):

- `/dp/frankentui/crates/ftui-extras/src/mermaid.rs`
- `/dp/frankentui/crates/ftui-extras/src/mermaid_layout.rs`
- `/dp/frankentui/crates/ftui-extras/src/mermaid_render.rs`
- `/dp/frankentui/crates/ftui-extras/src/mermaid_diff.rs`
- `/dp/frankentui/crates/ftui-extras/src/mermaid_minimap.rs`
- `/dp/frankentui/crates/ftui-extras/src/diagram_layout.rs`
- `/dp/frankentui/crates/ftui-extras/src/diagram.rs`
- `/dp/frankentui/crates/ftui-extras/src/dot_parser.rs`
- `/dp/frankentui/crates/ftui-extras/src/canvas.rs`

`legacy_mermaid_code/` is reference corpus only, not a direct port target.

## Explicit Exclusions

These are out of scope for parity accounting unless the project owner later
decides otherwise:

- Bit-for-bit textual parity with mermaid-js internals
- Direct line-by-line translation from FrankenTUI or mermaid-js
- Reproducing FrankenTUI-only UI concerns unrelated to diagram behavior
- Preserving bugs that conflict with current frankenmermaid design goals
- Backwards compatibility shims for already-correct Rust APIs

## Current Phase

The repo is now in Phase 5: conformance testing and parity closure.

The standard port-spec documents already exist:

- `PLAN_TO_PORT_FRANKENTUI_MERMAID_TO_RUST.md`
- `EXISTING_FRANKENTUI_MERMAID_STRUCTURE.md`
- `PROPOSED_ARCHITECTURE.md`
- `FEATURE_PARITY.md`

That means the remaining blocker is no longer "missing plan/spec documents".
The blocker is that parity still is not proven end-to-end by a dedicated
FrankenTUI conformance harness and fixture corpus.

## Work Phases

### Phase 1: Scope And Ledger

- Create the parity plan and feature ledger.
- Map each legacy/reference surface to a frankenmermaid crate or gap.
- Mark claims from README against actual code paths.

### Phase 2: Exact Behavior Extraction

- Produce `EXISTING_FRANKENTUI_MERMAID_STRUCTURE.md`.
- Extract exact parser behaviors, layout rules, renderer semantics, and
  fallback/diagnostic behavior from the FrankenTUI reference sources.
- Record defaults, edge cases, and unsupported constructs precisely.

### Phase 3: Architecture Synthesis

- Produce `PROPOSED_ARCHITECTURE.md`.
- Map extracted behavior into current crate boundaries:
  `fm-parser`, `fm-core`, `fm-layout`, `fm-render-svg`,
  `fm-render-term`, `fm-render-canvas`, `fm-wasm`, `fm-cli`.
- Identify where direct parity is appropriate and where clean Rust redesign is
  better.

### Phase 4: Implementation

- Close the highest-value parity gaps first.
- Prioritize gaps that are currently detected but not actually implemented.
- Keep each implementation slice narrow, test-backed, and tied to the parity
  ledger.

### Phase 5: Conformance Testing

- Add fixture-based reference tests for parser/layout/render behavior.
- Compare frankenmermaid output against captured FrankenTUI reference behavior.
- Update `FEATURE_PARITY.md` with proved status, not aspiration.

## Immediate Priorities

Based on current code, the highest-value parity gaps are:

1. Build fixture-backed conformance coverage proving parser/layout/render
   behavior against the FrankenTUI reference surfaces.
2. Keep `FEATURE_PARITY.md` synchronized with actual code and tests rather than
   older implementation assumptions.
3. Close the remaining genuinely unproved render-surface gaps, especially
   terminal fidelity/overlay behaviors that existed in FrankenTUI and are not
   yet fully accounted for in the parity ledger.

## Success Criteria

We can only claim 100% feature parity when all of the following are true:

- Every in-scope feature has a documented reference behavior.
- Every documented feature is implemented in the corresponding Rust crate.
- Conformance tests prove parity for parser, layout, and rendering behavior.
- `FEATURE_PARITY.md` contains no open in-scope gaps.
