# Fixture Provenance

This document records how conformance test fixtures were generated and how to regenerate them.

## Fixture Generation Method

Fixtures in this directory are **hand-crafted** Mermaid diagram files that represent minimal, focused test cases for each diagram type and feature. They are NOT generated from a reference implementation.

### Generation Process

1. **Fixture Creation:**
   - Fixtures are `.mmd` files written to test specific diagram features
   - Each fixture is a minimal example that exercises one capability
   - Syntax follows official Mermaid documentation

2. **Expected Value Extraction:**
   - Run `cargo run -p fm-cli -- parse <fixture.mmd>` to get IR statistics
   - Run `cargo run -p fm-cli -- render <fixture.mmd>` to get SVG output
   - Extract node/edge/cluster counts from parse output
   - Identify key strings that should appear in rendered SVG

3. **Test Manifest Update:**
   - Add entry to `frankentui_conformance_cases.json`
   - Include counts, svg_contains, and any warning expectations

## Fixture Inventory

| Fixture | Created | Source | Purpose |
|---------|---------|--------|---------|
| `flowchart_basic.mmd` | 2026-04-15 | Hand-crafted | Basic flowchart with decision node |
| `sequence_basic.mmd` | 2026-04-15 | Hand-crafted | Participant and message exchange |
| `class_inheritance.mmd` | 2026-04-15 | Hand-crafted | Class hierarchy with members |
| `state_transitions.mmd` | 2026-04-15 | Hand-crafted | State machine with transitions |
| `er_relationships.mmd` | 2026-04-15 | Hand-crafted | Entity-relationship diagram |
| `gantt_project.mmd` | 2026-04-15 | Hand-crafted | Gantt chart with tasks |
| `pie_chart.mmd` | 2026-04-15 | Hand-crafted | Pie chart with slices |
| `mindmap_basic.mmd` | 2026-04-15 | Hand-crafted | Mindmap with branches |
| `journey_user.mmd` | 2026-04-15 | Hand-crafted | User journey with sections |
| `timeline_events.mmd` | 2026-04-15 | Hand-crafted | Timeline with dated events |
| `gitgraph_basic.mmd` | 2026-04-15 | Hand-crafted | Git graph with branches/merges |
| `requirement_basic.mmd` | 2026-04-15 | Hand-crafted | Requirements with elements |
| `quadrant_basic.mmd` | 2026-04-15 | Hand-crafted | Quadrant chart with items |
| `sankey_links.mmd` | 2026-04-05 | Hand-crafted | Sankey with flow values |
| `xychart_axes_series.mmd` | 2026-04-05 | Hand-crafted | XY chart with bar/line series |
| `block_beta_nested_groups.mmd` | 2026-04-05 | Hand-crafted | Block-beta with spans |
| `packet_basic.mmd` | 2026-04-15 | Hand-crafted | Packet diagram with fields |
| `architecture_basic.mmd` | 2026-04-15 | Hand-crafted | Architecture with groups/services |
| `c4context_basic.mmd` | 2026-04-15 | Hand-crafted | C4 context diagram |
| `c4container_basic.mmd` | 2026-04-15 | Hand-crafted | C4 container diagram |
| `c4component_basic.mmd` | 2026-04-15 | Hand-crafted | C4 component diagram |
| `c4dynamic_basic.mmd` | 2026-04-15 | Hand-crafted | C4 dynamic diagram |
| `c4deployment_basic.mmd` | 2026-04-15 | Hand-crafted | C4 deployment diagram |
| `kanban_basic.mmd` | 2026-04-15 | Hand-crafted | Kanban board with columns |
| `click_link_tooltip.mmd` | 2026-04-05 | FrankenTUI | Click directive with URL |
| `click_callback_tooltip.mmd` | 2026-04-05 | FrankenTUI | Click callback directive |

## Reference Implementation Comparison

For fixtures that need validation against mermaid-js:

```bash
# 1. Render with frankenmermaid
cargo run -p fm-cli -- render fixture.mmd > fm_output.svg

# 2. Render with mermaid-js (requires Node.js + @mermaid-js/mermaid-cli)
npx mmdc -i fixture.mmd -o mermaid_output.svg

# 3. Compare
diff -u fm_output.svg mermaid_output.svg
```

## Test Execution

```bash
# Run all conformance tests
cargo test -p fm-cli --test frankentui_conformance_test

# Run with verbose output
cargo test -p fm-cli --test frankentui_conformance_test -- --nocapture
```

## Updating Expected Values

When parser/render behavior changes intentionally:

1. Run `cargo run -p fm-cli -- parse <fixture.mmd>` for new counts
2. Run `cargo run -p fm-cli -- render <fixture.mmd>` and check SVG
3. Update `frankentui_conformance_cases.json` with new expectations
4. Verify tests pass: `cargo test -p fm-cli --test frankentui_conformance_test`
5. Document changes in git commit message

## Environment

- **Rust version:** nightly (see `rust-toolchain.toml`)
- **frankenmermaid version:** workspace version from `Cargo.toml`
- **Generator command:** `cargo run -p fm-cli -- parse/render`

## Last Updated

2026-04-15 - Full fixture inventory documented
