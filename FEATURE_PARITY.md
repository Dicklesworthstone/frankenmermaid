# FEATURE_PARITY

## Meaning Of This Document

This file tracks actual parity status against the FrankenTUI Mermaid extraction
reference, not marketing claims and not aspirational support.

Status meanings:

- `Complete`: implemented and verified in current Rust code
- `Partial`: implemented for a meaningful subset, but not yet parity-complete
- `Fallback`: detected or acknowledged, but routed through generic/fallback
  behavior rather than a real implementation
- `Missing`: no meaningful implementation yet

## Evidence Sources

Current status in this file is grounded in:

- parser dispatch in [`crates/fm-parser/src/mermaid_parser.rs`](/data/projects/frankenmermaid/crates/fm-parser/src/mermaid_parser.rs)
- type detection in [`crates/fm-parser/src/lib.rs`](/data/projects/frankenmermaid/crates/fm-parser/src/lib.rs)
- CLI support reporting in [`crates/fm-cli/src/main.rs`](/data/projects/frankenmermaid/crates/fm-cli/src/main.rs)
- layout specialization in [`crates/fm-layout/src/lib.rs`](/data/projects/frankenmermaid/crates/fm-layout/src/lib.rs)
- behavioral reference paths listed in [`AGENTS.md`](/data/projects/frankenmermaid/AGENTS.md)

## Current Baseline

### Parser Families

| Diagram family | Detection | Dedicated parser | Current status | Notes |
|---|---|---|---|---|
| Flowchart | Yes | Yes | Partial | Most advanced parser path; recursive document AST work in progress |
| Sequence | Yes | Yes | Partial | Dedicated parser exists |
| Class | Yes | Yes | Partial | Dedicated parser exists |
| State | Yes | Yes | Partial | Dedicated parser exists |
| ER | Yes | Yes | Partial | Dedicated parser exists |
| Requirement | Yes | Yes | Partial | Dedicated parser exists |
| Mindmap | Yes | Yes | Partial | Dedicated parser exists |
| Journey | Yes | Yes | Partial | Dedicated parser exists |
| Timeline | Yes | Yes | Partial | Dedicated parser exists |
| Packet Beta | Yes | Yes | Partial | Dedicated parser exists |
| Gantt | Yes | Yes | Partial | Dedicated parser exists |
| Pie | Yes | Yes | Partial | Dedicated parser exists |
| Quadrant Chart | Yes | Yes | Partial | Dedicated parser exists |
| Git Graph | Yes | Yes | Partial | Dedicated parser exists; CLI still marks it unsupported |
| Sankey | Yes | No | Fallback | Detected but routed through generic flowchart fallback |
| XY Chart | Yes | No | Fallback | Detected but routed through generic flowchart fallback |
| Block Beta | Yes | No | Fallback | Detected but routed through generic flowchart fallback |
| Architecture Beta | Yes | No | Fallback | Detected but routed through generic flowchart fallback |
| C4 family | Yes | No | Fallback | Detected but routed through generic flowchart fallback |
| DOT bridge | Yes | Yes | Partial | Dedicated parser exists via `dot_parser` |

### Layout And Rendering

| Surface | Current status | Notes |
|---|---|---|
| Shared IR pipeline | Partial | Strong base exists, including graph/subgraph IR work |
| Deterministic layout | Partial | Multiple specialized paths exist; parity still unproved |
| SVG renderer | Partial | Mature surface, but no parity proof against FrankenTUI yet |
| Terminal renderer | Partial | Present, but no conformance ledger yet |
| Canvas/WASM | Partial | Present, but no parity ledger yet |
| Diff/minimap parity | Missing | Legacy reference files exist, but parity extraction has not been documented yet |

### CLI/User-Facing Claims

Current CLI support reporting in [`crates/fm-cli/src/main.rs`](/data/projects/frankenmermaid/crates/fm-cli/src/main.rs) already contradicts any blanket "100% parity" claim:

- `Flowchart`: `full`
- `Sequence`, `Class`, `State`, `ER`: `partial`
- `Pie`, `Gantt`, `Journey`, `Mindmap`, `Timeline`, `QuadrantChart`, `Requirement`, `PacketBeta`: `basic`
- `GitGraph`, `Sankey`, `XYChart`, `BlockBeta`, `ArchitectureBeta`, all `C4*`: `unsupported`

That is the current hard baseline until implementation and tests prove otherwise.

## Highest-Value Gaps

1. Parser parity for detected-but-fallback families:
   `Sankey`, `XYChart`, `BlockBeta`, `ArchitectureBeta`, `C4*`
2. CLI support metadata drift:
   `GitGraph` has a dedicated parser but is still reported as unsupported
3. Missing reference-spec documents:
   no `EXISTING_*_STRUCTURE.md` or `PROPOSED_ARCHITECTURE.md` yet
4. Missing conformance infrastructure:
   no fixture-based parity harness against the FrankenTUI reference surfaces

## Required Next Documents

- `EXISTING_FRANKENTUI_MERMAID_STRUCTURE.md`
- `PROPOSED_ARCHITECTURE.md`

Those two documents are required before any honest claim of 100% parity.

## Exit Condition For 100%

This file can only move to "100% feature parity" when:

- every in-scope legacy/reference feature is documented,
- every documented feature is implemented in Rust,
- every implementation is backed by conformance tests,
- no row above remains `Partial`, `Fallback`, or `Missing`.
