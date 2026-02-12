use std::collections::BTreeMap;

use fm_core::{
    ArrowType, DiagramType, GraphDirection, IrEdge, IrEndpoint, IrLabel, IrLabelId, IrNode,
    IrNodeId, MermaidDiagramIr, NodeShape, Span,
};

use crate::ParseResult;

pub(crate) struct IrBuilder {
    ir: MermaidDiagramIr,
    node_index_by_id: BTreeMap<String, IrNodeId>,
    warnings: Vec<String>,
}

impl IrBuilder {
    pub(crate) fn new(diagram_type: DiagramType) -> Self {
        Self {
            ir: MermaidDiagramIr::empty(diagram_type),
            node_index_by_id: BTreeMap::new(),
            warnings: Vec::new(),
        }
    }

    pub(crate) fn set_direction(&mut self, direction: GraphDirection) {
        self.ir.direction = direction;
        self.ir.meta.direction = direction;
    }

    pub(crate) fn add_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }

    pub(crate) fn node_count(&self) -> usize {
        self.ir.nodes.len()
    }

    pub(crate) fn edge_count(&self) -> usize {
        self.ir.edges.len()
    }

    pub(crate) fn finish(self) -> ParseResult {
        ParseResult {
            ir: self.ir,
            warnings: self.warnings,
        }
    }

    pub(crate) fn intern_node(
        &mut self,
        id: &str,
        label: Option<&str>,
        shape: NodeShape,
        span: Span,
    ) -> Option<IrNodeId> {
        let normalized_id = id.trim();
        if normalized_id.is_empty() {
            self.add_warning("Encountered empty node identifier; skipped node");
            return None;
        }

        if let Some(existing_id) = self.node_index_by_id.get(normalized_id).copied() {
            let resolved_label = if self
                .ir
                .nodes
                .get(existing_id.0)
                .and_then(|node| node.label)
                .is_none()
            {
                clean_label(label).map(|cleaned_label| self.intern_label(cleaned_label, span))
            } else {
                None
            };

            if let Some(existing_node) = self.ir.nodes.get_mut(existing_id.0) {
                if existing_node.label.is_none() {
                    existing_node.label = resolved_label;
                }
                if existing_node.shape == NodeShape::Rect && shape != NodeShape::Rect {
                    existing_node.shape = shape;
                }
            }
            return Some(existing_id);
        }

        let label_id = clean_label(label).map(|value| self.intern_label(value, span));
        let node_id = IrNodeId(self.ir.nodes.len());
        let node = IrNode {
            id: normalized_id.to_string(),
            label: label_id,
            shape,
            classes: Vec::new(),
            span_primary: span,
            span_all: vec![span],
            implicit: false,
        };

        self.ir.nodes.push(node);
        self.node_index_by_id
            .insert(normalized_id.to_string(), node_id);
        Some(node_id)
    }

    pub(crate) fn push_edge(
        &mut self,
        from: IrNodeId,
        to: IrNodeId,
        arrow: ArrowType,
        label: Option<&str>,
        span: Span,
    ) {
        let label_id = clean_label(label).map(|value| self.intern_label(value, span));
        self.ir.edges.push(IrEdge {
            from: IrEndpoint::Node(from),
            to: IrEndpoint::Node(to),
            arrow,
            label: label_id,
            span,
        });
    }

    fn intern_label(&mut self, text: String, span: Span) -> IrLabelId {
        let label_id = IrLabelId(self.ir.labels.len());
        self.ir.labels.push(IrLabel { text, span });
        label_id
    }
}

fn clean_label(input: Option<&str>) -> Option<String> {
    let raw = input?;
    let cleaned = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('`')
        .trim();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.to_string())
    }
}
