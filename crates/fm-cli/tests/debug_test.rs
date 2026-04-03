use fm_core::{
    ArrowType, DiagramType, IrEdge, IrEndpoint, IrLabel, IrLabelId, IrNode, IrNodeId,
    MermaidDiagramIr, Span,
};
use fm_layout::*;
use std::sync::{Arc, Mutex};
use tracing_subscriber::Layer;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;

#[derive(Clone)]
struct CaptureWriter(Arc<Mutex<Vec<String>>>);

impl std::io::Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let s = String::from_utf8_lossy(buf).to_string();
        self.0.lock().unwrap().push(s);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CaptureWriter {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn labeled_graph_ir(n: usize, edges: &[(usize, usize)]) -> MermaidDiagramIr {
    let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);

    for i in 0..n {
        let label_id = IrLabelId(ir.labels.len());
        ir.labels.push(IrLabel {
            text: format!("Node {}", i),
            span: Span::default(),
        });
        ir.nodes.push(IrNode {
            id: format!("N{}", i),
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
fn test_debug_events() {
    let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let captured_clone = Arc::clone(&captured);

    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_writer(move || CaptureWriter(Arc::clone(&captured_clone)))
        .with_target(false)
        .with_level(true)
        .with_filter(LevelFilter::TRACE);

    let subscriber = tracing_subscriber::registry().with(fmt_layer);

    let ir = labeled_graph_ir(4, &[(0, 1), (1, 2), (2, 3)]);
    tracing::subscriber::with_default(subscriber, || {
        let mut engine = IncrementalLayoutEngine::default();
        let _first = engine.layout_diagram_traced_with_config_and_guardrails(
            &ir,
            LayoutAlgorithm::Auto,
            LayoutConfig::default(),
            LayoutGuardrails::default(),
        );
        let _second = engine.layout_diagram_traced_with_config_and_guardrails(
            &ir,
            LayoutAlgorithm::Auto,
            LayoutConfig::default(),
            LayoutGuardrails::default(),
        );
    });

    for line in captured.lock().unwrap().iter() {
        if line.contains("incremental.recompute") {
            println!("{}", line);
        }
    }
}
