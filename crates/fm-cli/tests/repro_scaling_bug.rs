use fm_core::{DiagramType, GraphDirection, IrLabel, IrLabelId, IrNode, MermaidDiagramIr};
use fm_render_term::{TermRenderConfig, render_term_with_config};

#[test]
fn test_scaling_bug_repro() {
    let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
    ir.direction = GraphDirection::TB;

    // Create two nodes far apart.
    // In layout-space, they might be at y=0 and y=1000.
    // With scale 0.25, they would be at y=0 and y=250.
    // If rows=40, y=250 is way off-screen.

    ir.labels.push(IrLabel {
        text: "NodeA".to_string(),
        ..Default::default()
    });
    ir.labels.push(IrLabel {
        text: "NodeB".to_string(),
        ..Default::default()
    });

    ir.nodes.push(IrNode {
        id: "A".to_string(),
        label: Some(IrLabelId(0)),
        ..Default::default()
    });
    ir.nodes.push(IrNode {
        id: "B".to_string(),
        label: Some(IrLabelId(1)),
        ..Default::default()
    });

    // We need an edge to force them apart in layout, or just many nodes.
    // Let's just use many nodes to force a large layout.
    use fm_core::{ArrowType, IrEdge, IrEndpoint, IrNodeId};
    // ...
    for i in 2..50 {
        ir.labels.push(IrLabel {
            text: format!("Node{i}"),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: format!("N{i}"),
            label: Some(IrLabelId(i)),
            ..Default::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(i - 1)),
            to: IrEndpoint::Node(IrNodeId(i)),
            arrow: ArrowType::Arrow,
            ..Default::default()
        });
    }

    let config = TermRenderConfig::default();
    let cols = 80;
    let rows = 24;

    let result = render_term_with_config(&ir, &config, cols, rows);

    // If the scaling bug exists, the "Node49" (or similar) will be missing from the output
    // because its cell-position was calculated with the hardcoded scale 0.25
    // instead of a fitted scale.

    println!("Width: {}, Height: {}", result.width, result.height);
    println!("Output length: {}", result.output.len());

    // Check if the last node is present.
    assert!(
        result.output.contains("Node49"),
        "Last node should be visible in the output even in small terminal"
    );
}
