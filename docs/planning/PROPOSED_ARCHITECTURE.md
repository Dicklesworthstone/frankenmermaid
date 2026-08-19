# PROPOSED_ARCHITECTURE

## Goal

Map the extracted FrankenTUI Mermaid reference behavior into frankenmermaid’s
existing crate structure without cargo-culting the legacy implementation.

This document is the implementation bridge between:

- [`EXISTING_FRANKENTUI_MERMAID_STRUCTURE.md`](/data/projects/frankenmermaid/EXISTING_FRANKENTUI_MERMAID_STRUCTURE.md)
- the current workspace architecture in [`README.md`](/data/projects/frankenmermaid/README.md)

## Non-Goals

- Rebuilding FrankenTUI’s monolithic module layout inside frankenmermaid
- Preserving legacy naming if current Rust naming is already clearer
- Introducing compatibility shims to mimic old APIs exactly
- Porting terminal-only UI concerns that do not belong in the shared engine

## Crate Mapping

### `fm-parser`

Owns:

- type detection
- line/statement parsing
- family-specific parsing for Mermaid and DOT input
- init/front-matter extraction
- warning/error recovery
- normalization into shared IR

Reference sources mapped here:

- `mermaid.rs` parser helpers and `parse_with_diagnostics`
- `dot_parser.rs`

### `fm-core`

Owns:

- shared IR
- diagnostics and metadata
- theme/style/link contracts
- support/parity metadata structures when they become productized

Reference sources mapped here:

- `MermaidDiagramIr`
- style/link/constraint types in `mermaid.rs`
- compatibility/support enums that belong to the shared contract surface

### `fm-layout`

Owns:

- deterministic generic graph layout
- specialized layout families
- layout trace/stats/degradation plans

Reference sources mapped here:

- `mermaid_layout.rs`

### `fm-render-svg`

Owns:

- accessible SVG generation
- family-specific SVG rendering choices
- metadata and responsive sizing

Reference sources mapped conceptually here:

- visual behavior from `mermaid_render.rs`

### `fm-render-term`

Owns:

- terminal render plans
- fidelity/degradation choices for terminal output
- minimap/diff integration when ported

Reference sources mapped here:

- `mermaid_render.rs`
- `mermaid_minimap.rs`
- `mermaid_diff.rs`

### `fm-render-canvas`

Owns:

- canvas-backed rendering primitives and family-specific drawing

Reference sources mapped here:

- canvas-backed parts of `mermaid_render.rs`
- `canvas.rs`

### `fm-cli`

Owns:

- user-facing support reporting
- detect/parse/render/validate command behavior
- parity-report and conformance-facing command hooks if added later

### `fm-wasm`

Owns:

- browser/WASM API contract only
- no family-specific parsing or layout logic beyond delegation

## Recommended Parity Layers

### Layer 1: Parse Parity

First objective for each family:

- detect header correctly
- parse family-specific statements without falling back to generic flowchart
- preserve family-specific metadata in IR

### Layer 2: IR Parity

Second objective:

- stop flattening family-specific semantics into generic nodes/edges where the
  reference has dedicated typed fields

### Layer 3: Layout Parity

Third objective:

- decide whether the family should use generic graph layout or a specialized
  layout path
- preserve deterministic output and trace hooks

### Layer 4: Render Parity

Fourth objective:

- implement family-specific visuals only after parser/IR semantics are sound
- keep SVG, terminal, and canvas behavior aligned from the same IR

### Layer 5: Conformance

Final objective:

- prove behavior against captured reference fixtures
- update `FEATURE_PARITY.md` only after tests exist

## Immediate Priority Order

### 1. Conformance Harness

Why first:

- dedicated parsers and layouts now exist for the previously called-out
  families (`BlockBeta`, `Sankey`, `XyChart`, `ArchitectureBeta`, `C4*`)
- parity claims without captured reference fixtures are still unproved
- `FEATURE_PARITY.md` can only be trusted if it is tied to executable evidence

Implementation target:

- add fixture-backed parser/layout/render comparisons against the FrankenTUI
  reference surface
- cover both happy-path and edge-case diagrams per family
- use the resulting fixtures to drive future parity updates

### 2. Terminal Parity Audit

Why second:

- FrankenTUI had fidelity-tier selection and overlay-oriented terminal behavior
- frankenmermaid already implements several terminal specializations, but the
  exact parity surface still needs an evidence-backed ledger

### 3. Surface Truthfulness

Why third:

- README / support metadata / parity docs must reflect actual implementation
  status and tests
- false-negative parity docs are as harmful as false-positive marketing claims

### 4. Family-Specific Gap Closure

Why fourth:

- after conformance fixtures exist, the remaining family-specific gaps will be
  objective rather than inferred
- only then is it worth doing another dedicated parser/layout/render wave

## Proposed Implementation Rules

1. Do not implement from FrankenTUI directly once the spec is extracted.
2. One family at a time, end-to-end: parser -> IR -> layout -> render -> tests.
3. Add dedicated tests before claiming a family moved from `Fallback` to
   `Partial`.
4. Update CLI support reporting only when the underlying pipeline actually
   changed.
5. Keep `FEATURE_PARITY.md` as the truth source for current status.

## First Concrete Next Slice

The next implementation slice should be:

1. Add the first fixture-backed conformance cases for already-implemented
   families rather than speculating about fallback gaps that no longer exist.
2. Start with parser + render surfaces that are easy to compare deterministically:
   - click/link directives
   - block-beta structure
   - sankey record parsing
   - xychart axis/series parsing
3. Wire those fixtures into Rust tests and use them to correct
   `FEATURE_PARITY.md`.
4. Re-run targeted tests plus workspace quality gates.

That is the smallest honest step that materially reduces the parity gap now.
