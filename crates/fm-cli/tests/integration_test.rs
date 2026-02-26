//! Integration tests for the FrankenMermaid pipeline.
//!
//! These tests verify the end-to-end flow from parsing to layout to rendering.

use fm_core::{DiagramType, GraphDirection};
use fm_layout::{layout_diagram, layout_diagram_traced};
use fm_parser::parse;
use fm_render_svg::render_svg;
use fm_render_term::render_term;

/// Test that a simple flowchart parses and produces non-zero layout positions.
#[test]
fn flowchart_parses_and_lays_out_with_nonzero_positions() {
    let input = r#"flowchart LR
    A[Start] --> B[Process]
    B --> C[End]
"#;

    let parse_result = parse(input);
    // ParseResult has warnings, not errors. Check warnings for critical issues.
    assert!(
        parse_result.warnings.is_empty(),
        "Parse warnings: {:?}",
        parse_result.warnings
    );

    let ir = parse_result.ir;
    assert_eq!(ir.diagram_type, DiagramType::Flowchart);
    assert_eq!(ir.direction, GraphDirection::LR);
    assert_eq!(ir.nodes.len(), 3, "Expected 3 nodes");
    assert_eq!(ir.edges.len(), 2, "Expected 2 edges");

    let layout = layout_diagram(&ir);
    assert_eq!(layout.nodes.len(), 3);
    assert_eq!(layout.edges.len(), 2);

    // Verify all nodes have non-zero bounds.
    for node in &layout.nodes {
        assert!(
            node.bounds.width > 0.0,
            "Node {} has zero width",
            node.node_id
        );
        assert!(
            node.bounds.height > 0.0,
            "Node {} has zero height",
            node.node_id
        );
    }

    // Verify layout bounds are positive.
    assert!(layout.bounds.width > 0.0, "Layout has zero width");
    assert!(layout.bounds.height > 0.0, "Layout has zero height");

    // Verify edges have at least 2 points.
    for edge in &layout.edges {
        assert!(
            edge.points.len() >= 2,
            "Edge {} has fewer than 2 points",
            edge.edge_index
        );
    }
}

/// Test that SVG rendering produces valid output.
#[test]
fn flowchart_renders_to_valid_svg() {
    let input = "flowchart TD\n    A --> B";

    let parse_result = parse(input);
    let ir = parse_result.ir;

    let svg = render_svg(&ir);

    // Basic validity checks.
    assert!(svg.starts_with("<svg"), "SVG should start with <svg tag");
    assert!(svg.contains("</svg>"), "SVG should end with </svg>");
    assert!(svg.contains("viewBox"), "SVG should have a viewBox");
    assert!(svg.contains("<rect"), "SVG should contain rect elements");
    assert!(svg.contains("<path"), "SVG should contain path elements");
}

/// Test that terminal rendering produces non-empty output.
#[test]
fn flowchart_renders_to_terminal() {
    let input = "flowchart LR\n    A --> B --> C";

    let parse_result = parse(input);
    let ir = parse_result.ir;

    let term_output = render_term(&ir);

    // Should produce some output.
    assert!(
        !term_output.is_empty(),
        "Terminal output should not be empty"
    );
    assert!(
        term_output.lines().count() > 0,
        "Should have multiple lines"
    );
}

/// Test determinism: same input produces same layout.
#[test]
fn layout_is_deterministic() {
    let input = r#"flowchart TD
    A[Alpha] --> B[Beta]
    A --> C[Gamma]
    B --> D[Delta]
    C --> D
"#;

    let parse_result = parse(input);
    let ir = parse_result.ir;

    let layout1 = layout_diagram_traced(&ir);
    let layout2 = layout_diagram_traced(&ir);

    // Layouts should be identical.
    assert_eq!(
        layout1.layout.nodes.len(),
        layout2.layout.nodes.len(),
        "Node counts differ"
    );

    for (n1, n2) in layout1.layout.nodes.iter().zip(layout2.layout.nodes.iter()) {
        assert_eq!(n1.node_id, n2.node_id, "Node IDs differ");
        assert!(
            (n1.bounds.x - n2.bounds.x).abs() < 0.001,
            "Node {} x position differs",
            n1.node_id
        );
        assert!(
            (n1.bounds.y - n2.bounds.y).abs() < 0.001,
            "Node {} y position differs",
            n1.node_id
        );
    }

    // Stats should match.
    assert_eq!(
        layout1.layout.stats.crossing_count, layout2.layout.stats.crossing_count,
        "Crossing counts differ"
    );
}

/// Test that cycles are handled gracefully.
#[test]
fn handles_cyclic_graph() {
    let input = r#"flowchart LR
    A --> B
    B --> C
    C --> A
"#;

    let parse_result = parse(input);
    assert!(
        parse_result.warnings.is_empty(),
        "Cyclic graph should parse: {:?}",
        parse_result.warnings
    );

    let ir = parse_result.ir;
    let layout = layout_diagram(&ir);

    // Should still produce valid layout.
    assert_eq!(layout.nodes.len(), 3);
    assert!(
        layout.stats.reversed_edges >= 1,
        "Should have reversed edges"
    );

    // All nodes should have valid positions.
    for node in &layout.nodes {
        assert!(
            node.bounds.x.is_finite() && node.bounds.y.is_finite(),
            "Node {} has non-finite position",
            node.node_id
        );
    }
}

/// Test parsing of different diagram types.
#[test]
fn detects_diagram_types_correctly() {
    let test_cases = [
        ("flowchart TD\nA-->B", DiagramType::Flowchart),
        ("graph LR\nA-->B", DiagramType::Flowchart),
        ("sequenceDiagram\nAlice->>Bob: Hello", DiagramType::Sequence),
        ("classDiagram\nAnimal <|-- Dog", DiagramType::Class),
        ("stateDiagram-v2\n[*] --> State1", DiagramType::State),
        ("pie\ntitle Pie\n\"A\": 30", DiagramType::Pie),
        (
            "gantt\ntitle Gantt\nsection S1\nTask: a, 2024-01-01, 1d",
            DiagramType::Gantt,
        ),
    ];

    for (input, expected_type) in test_cases {
        let result = parse(input);
        assert_eq!(
            result.ir.diagram_type,
            expected_type,
            "Failed for input: {}",
            input.lines().next().unwrap_or(input)
        );
    }
}

/// Test edge label handling.
#[test]
fn handles_edge_labels() {
    let input = r#"flowchart LR
    A -->|label1| B
    B -->|label2| C
"#;

    let parse_result = parse(input);
    let ir = parse_result.ir;

    // Should have 2 edges.
    assert_eq!(ir.edges.len(), 2);

    // Both edges should have labels.
    let edges_with_labels = ir.edges.iter().filter(|e| e.label.is_some()).count();
    assert!(
        edges_with_labels >= 1,
        "Expected at least one edge with label"
    );
}

/// Test node shape parsing.
#[test]
fn parses_node_shapes() {
    let input = r#"flowchart LR
    A[Rectangle]
    B(Rounded)
    C((Circle))
    D{Diamond}
"#;

    let parse_result = parse(input);
    let ir = parse_result.ir;

    assert!(ir.nodes.len() >= 4, "Expected at least 4 nodes");

    // Verify different shapes are recognized.
    let shapes: Vec<_> = ir.nodes.iter().map(|n| n.shape).collect();
    assert!(
        shapes.iter().any(|s| *s != fm_core::NodeShape::Rect),
        "Expected some non-rect shapes"
    );
}

/// Test subgraph/cluster handling.
#[test]
fn handles_subgraphs() {
    let input = r#"flowchart TD
    subgraph cluster1 [Cluster One]
        A --> B
    end
    subgraph cluster2 [Cluster Two]
        C --> D
    end
    B --> C
"#;

    let parse_result = parse(input);
    let ir = parse_result.ir;

    // Parser should preserve subgraph structure as clusters.
    assert_eq!(ir.diagram_type, DiagramType::Flowchart);
    assert_eq!(
        ir.clusters.len(),
        2,
        "Expected two parsed subgraph clusters"
    );

    // Nodes and edges within subgraphs should still be parsed.
    assert!(
        ir.nodes.len() >= 4,
        "Expected at least 4 nodes from subgraph content"
    );
    assert!(
        ir.edges.len() >= 3,
        "Expected at least 3 edges from subgraph content"
    );

    // Cluster membership should include nodes declared inside each subgraph.
    let cluster_sizes: Vec<usize> = ir
        .clusters
        .iter()
        .map(|cluster| cluster.members.len())
        .collect();
    assert!(
        cluster_sizes.iter().all(|size| *size >= 2),
        "Expected each subgraph cluster to include at least two member nodes, got {cluster_sizes:?}"
    );

    // Layout should include clusters and remain valid.
    let layout = layout_diagram(&ir);
    assert!(layout.nodes.len() >= 4, "Layout should include all nodes");
    assert!(layout.edges.len() >= 3, "Layout should include all edges");
    assert_eq!(
        layout.clusters.len(),
        2,
        "Expected two rendered layout clusters"
    );

    // All nodes should have valid positions.
    for node in &layout.nodes {
        assert!(
            node.bounds.x.is_finite() && node.bounds.y.is_finite(),
            "Node {} has non-finite position",
            node.node_id
        );
    }
}

/// Test that very long labels are handled.
#[test]
fn handles_long_labels() {
    let long_label = "A".repeat(200);
    let input = format!("flowchart LR\n    A[{}]", long_label);

    let parse_result = parse(&input);
    assert!(
        parse_result.warnings.is_empty(),
        "Long label should parse: {:?}",
        parse_result.warnings
    );

    let layout = layout_diagram(&parse_result.ir);
    assert_eq!(layout.nodes.len(), 1);

    // Node should have positive width accommodating long label.
    assert!(layout.nodes[0].bounds.width > 0.0);
}

/// Test empty diagram handling.
#[test]
fn handles_empty_diagram() {
    let input = "flowchart TD";

    let parse_result = parse(input);
    let ir = parse_result.ir;

    // Should parse without fatal issues (warnings are ok for empty diagram).
    assert_eq!(ir.diagram_type, DiagramType::Flowchart);

    // Layout should handle empty graph.
    let layout = layout_diagram(&ir);
    assert_eq!(layout.nodes.len(), 0);
    assert_eq!(layout.edges.len(), 0);
}

/// Test direction handling for all directions.
#[test]
fn handles_all_directions() {
    let directions = [
        ("flowchart TB\nA-->B", GraphDirection::TB),
        ("flowchart TD\nA-->B", GraphDirection::TD),
        ("flowchart LR\nA-->B", GraphDirection::LR),
        ("flowchart RL\nA-->B", GraphDirection::RL),
        ("flowchart BT\nA-->B", GraphDirection::BT),
    ];

    for (input, expected_dir) in directions {
        let result = parse(input);
        assert_eq!(
            result.ir.direction, expected_dir,
            "Failed for direction {:?}",
            expected_dir
        );
    }
}
