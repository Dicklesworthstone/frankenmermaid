# Conformance Test Coverage Matrix

This document tracks FrankenTUI/mermaid-js conformance test coverage for the frankenmermaid parser and renderer.

## Coverage Status

| Status | Meaning |
|--------|---------|
| Full | All MUST/SHOULD requirements tested with passing assertions |
| Partial | Core requirements tested, edge cases pending |
| Basic | Minimal happy-path test exists |
| None | No conformance tests yet |

## Diagram Type Coverage

| Diagram Type | Parser | Layout | Render | Test Cases | Status |
|--------------|--------|--------|--------|------------|--------|
| Flowchart | Yes | Yes | Yes | `flowchart_basic`, `click_link_tooltip`, `click_callback_tooltip` | Partial |
| Sequence | Yes | Yes | Yes | `sequence_basic`, `sequence_notes`, `sequence_fragments` | Partial |
| Class | Yes | Yes | Yes | `class_inheritance` | Basic |
| State | Yes | Yes | Yes | `state_transitions` | Basic |
| ER | Yes | Yes | Yes | `er_relationships` | Basic |
| Gantt | Yes | Yes | Yes | `gantt_project` | Basic |
| Pie | Yes | Yes | Yes | `pie_chart` | Basic |
| Mindmap | Yes | Yes | Yes | `mindmap_basic` | Basic |
| Journey | Yes | Yes | Yes | `journey_user` | Basic |
| Timeline | Yes | Yes | Yes | `timeline_events` | Basic |
| GitGraph | Yes | Yes | Yes | `gitgraph_basic` | Basic |
| Requirement | Yes | Yes | Yes | `requirement_basic` | Basic |
| QuadrantChart | Yes | Yes | Yes | `quadrant_basic` | Basic |
| Sankey | Yes | Yes | Yes | `sankey_links` | Partial |
| XyChart | Yes | Yes | Yes | `xychart_axes_series` | Partial |
| Block-Beta | Yes | Yes | Yes | `block_beta_nested_groups` | Partial |
| Packet-Beta | Yes | Yes | Yes | `packet_basic` | Basic |
| Architecture-Beta | Yes | Yes | Yes | `architecture_basic` | Basic |
| C4Context | Yes | Yes | Yes | `c4context_basic` | Basic |
| C4Container | Yes | Yes | Yes | `c4container_basic` | Basic |
| C4Component | Yes | Yes | Yes | `c4component_basic` | Basic |
| C4Dynamic | Yes | Yes | Yes | `c4dynamic_basic` | Basic |
| C4Deployment | Yes | Yes | Yes | `c4deployment_basic` | Basic |
| Kanban | Yes | Yes | Yes | `kanban_basic` | Basic |

## Feature Coverage Within Diagram Types

### Flowchart
| Feature | Tested | Notes |
|---------|--------|-------|
| Basic nodes and edges | Yes | `flowchart_basic` |
| Decision nodes (rhombus) | Yes | `flowchart_basic` |
| Click directives with URLs | Yes | `click_link_tooltip` |
| Click callbacks | Yes | `click_callback_tooltip` |
| Subgraphs | Yes | `flowchart_subgraph` |
| Direction overrides | No | Pending |
| linkStyle | No | Pending |
| classDef | No | Pending |

### Sequence
| Feature | Tested | Notes |
|---------|--------|-------|
| Participants | Yes | `sequence_basic` |
| Messages (sync/async) | Yes | `sequence_basic` |
| Notes | Yes | `sequence_notes` |
| Activations | No | Pending |
| Fragments (loop/alt/opt) | Yes | `sequence_fragments` |
| Box groups | No | Pending |

### Class
| Feature | Tested | Notes |
|---------|--------|-------|
| Classes | Yes | `class_inheritance` |
| Inheritance | Yes | `class_inheritance` |
| Methods/attributes | Partial | Labels only |
| Stereotypes | No | Pending |
| Generics | No | Pending |

### XyChart (Partial Coverage)
| Feature | Tested | Notes |
|---------|--------|-------|
| Title | Yes | `xychart_axes_series` |
| X-axis categories | Yes | `xychart_axes_series` |
| Y-axis range | Yes | `xychart_axes_series` |
| Bar series | Yes | `xychart_axes_series` |
| Line series | Yes | `xychart_axes_series` |

### Sankey (Partial Coverage)
| Feature | Tested | Notes |
|---------|--------|-------|
| Nodes | Yes | `sankey_links` |
| Flow edges with values | Yes | `sankey_links` |

### Block-Beta (Partial Coverage)
| Feature | Tested | Notes |
|---------|--------|-------|
| Columns | Yes | `block_beta_nested_groups` |
| Groups | Yes | `block_beta_nested_groups` |
| Column spans | Yes | `block_beta_nested_groups` |
| Space blocks | No | Pending |

## Test Statistics

- **Total test cases:** 26
- **Diagram types covered:** 24/24 (100%)
- **Average tests per type:** 1.08
- **Types with multiple tests:** 1 (Flowchart: 3)

## Coverage Gaps (Priority Order)

1. **High Priority:**
   - Flowchart subgraphs and direction overrides
   - Sequence fragments and activations
   - Class stereotypes and generics

2. **Medium Priority:**
   - linkStyle and classDef directives
   - State composite states and history
   - ER attribute types (PK/FK/UK)

3. **Low Priority:**
   - Theme directive parsing
   - Accessibility directives (accTitle/accDescr)
   - Init directive configuration

## How to Add Tests

1. Create a `.mmd` fixture in `fixtures/frankentui_conformance/`
2. Run `cargo run -p fm-cli -- parse <fixture>` to get expected counts
3. Run `cargo run -p fm-cli -- render <fixture>` to verify SVG contains expected text
4. Add entry to `frankentui_conformance_cases.json` with:
   - `id`: unique test identifier
   - `description`: what feature is being tested
   - `input_path`: relative path to fixture
   - `source_refs`: FrankenTUI code references (for traceability)
   - `expected`: diagram_type, warnings, counts, svg_contains

## Last Updated

2026-04-15 - Added all 24 diagram types with basic coverage
