use fm_core::{
    ArrowType, DiagramType, IrEdge, IrEndpoint, IrLabel, IrLabelId, IrNode, IrNodeId,
    MermaidDiagramIr, Span,
};
use fm_layout::spectral::*;

fn make_ir(n: usize, edges: &[(usize, usize)]) -> MermaidDiagramIr {
    let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);

    for i in 0..n {
        let label_id = IrLabelId(ir.labels.len());
        ir.labels.push(IrLabel {
            text: format!("N{i}"),
            span: Span::default(),
        });
        ir.nodes.push(IrNode {
            id: format!("node_{i}"),
            label: Some(label_id),
            ..IrNode::default()
        });
    }

    for &(from, to) in edges {
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(from)),
            to: IrEndpoint::Node(IrNodeId(to)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
    }

    ir
}

#[test]
fn test_disconnected_partition() {
    let ir = make_ir(4, &[(0, 1), (2, 3)]);
    let result = spectral_bisect_graph(&ir);

    // If it returns trivial = true, it failed to partition a disconnected graph
    assert!(!result.trivial, "Disconnected graph was not partitioned!");
    assert_eq!(result.partitions.len(), 2);
}
