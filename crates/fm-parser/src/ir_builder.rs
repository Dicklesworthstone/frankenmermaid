use std::collections::BTreeMap;

use fm_core::{
    ArrowType, DiagramType, GraphDirection, IrAttributeKey, IrCluster, IrClusterId, IrEdge,
    IrEndpoint, IrEntityAttribute, IrLabel, IrLabelId, IrNode, IrNodeId, MermaidDiagramIr,
    MermaidError, MermaidWarning, MermaidWarningCode, NodeShape, Span,
};

use crate::ParseResult;

pub(crate) struct IrBuilder {
    ir: MermaidDiagramIr,
    node_index_by_id: BTreeMap<String, IrNodeId>,
    cluster_index_by_key: BTreeMap<String, usize>,
    warnings: Vec<String>,
}

impl IrBuilder {
    pub(crate) fn new(diagram_type: DiagramType) -> Self {
        Self {
            ir: MermaidDiagramIr::empty(diagram_type),
            node_index_by_id: BTreeMap::new(),
            cluster_index_by_key: BTreeMap::new(),
            warnings: Vec::new(),
        }
    }

    pub(crate) fn set_direction(&mut self, direction: GraphDirection) {
        self.ir.direction = direction;
        self.ir.meta.direction = direction;
    }

    pub(crate) fn set_init_theme(&mut self, theme: String) {
        self.ir.meta.init.config.theme = Some(theme.clone());
        self.ir.meta.theme_overrides.theme = Some(theme);
    }

    pub(crate) fn insert_theme_variable(&mut self, key: String, value: String) {
        self.ir
            .meta
            .init
            .config
            .theme_variables
            .insert(key.clone(), value.clone());
        self.ir
            .meta
            .theme_overrides
            .theme_variables
            .insert(key, value);
    }

    pub(crate) fn set_init_flowchart_direction(&mut self, direction: GraphDirection) {
        self.ir.meta.init.config.flowchart_direction = Some(direction);
    }

    pub(crate) fn add_init_warning(&mut self, message: impl Into<String>, span: Span) {
        self.ir.meta.init.warnings.push(MermaidWarning {
            code: MermaidWarningCode::ParseRecovery,
            message: message.into(),
            span,
        });
    }

    pub(crate) fn add_init_error(&mut self, message: impl Into<String>, span: Span) {
        self.ir.meta.init.errors.push(MermaidError::Parse {
            message: message.into(),
            span,
            expected: vec!["a valid Mermaid init JSON object".to_string()],
        });
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

    pub(crate) fn ensure_cluster(
        &mut self,
        key: &str,
        title: Option<&str>,
        span: Span,
    ) -> Option<usize> {
        let normalized_key = key.trim();
        if normalized_key.is_empty() {
            return None;
        }

        if let Some(existing_index) = self.cluster_index_by_key.get(normalized_key).copied() {
            if let Some(cleaned_title) = clean_label(title) {
                let label_id = self.intern_label(cleaned_title, span);
                if let Some(existing_cluster) = self.ir.clusters.get_mut(existing_index) {
                    if existing_cluster.title.is_none() {
                        existing_cluster.title = Some(label_id);
                    }
                }
            }
            return Some(existing_index);
        }

        let title_id = clean_label(title).map(|value| self.intern_label(value, span));
        let cluster_index = self.ir.clusters.len();
        self.ir.clusters.push(IrCluster {
            id: IrClusterId(cluster_index),
            title: title_id,
            members: Vec::new(),
            span,
        });
        self.cluster_index_by_key
            .insert(normalized_key.to_string(), cluster_index);
        Some(cluster_index)
    }

    pub(crate) fn add_node_to_cluster(&mut self, cluster_index: usize, node_id: IrNodeId) {
        let Some(cluster) = self.ir.clusters.get_mut(cluster_index) else {
            return;
        };
        if !cluster.members.contains(&node_id) {
            cluster.members.push(node_id);
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
            members: Vec::new(),
        };

        self.ir.nodes.push(node);
        self.node_index_by_id
            .insert(normalized_id.to_string(), node_id);
        Some(node_id)
    }

    pub(crate) fn add_class_to_node(&mut self, node_key: &str, class_name: &str, span: Span) {
        let normalized_class = class_name.trim();
        if normalized_class.is_empty() {
            return;
        }

        let Some(node_id) = self.intern_node(node_key, Some(node_key), NodeShape::Rect, span)
        else {
            return;
        };

        let Some(node) = self.ir.nodes.get_mut(node_id.0) else {
            return;
        };
        if !node
            .classes
            .iter()
            .any(|existing| existing == normalized_class)
        {
            node.classes.push(normalized_class.to_string());
        }
    }

    /// Add an entity attribute to a node (for ER diagrams).
    pub(crate) fn add_entity_attribute(
        &mut self,
        node_id: IrNodeId,
        data_type: &str,
        name: &str,
        key: IrAttributeKey,
        comment: Option<&str>,
    ) {
        let Some(node) = self.ir.nodes.get_mut(node_id.0) else {
            return;
        };

        node.members.push(IrEntityAttribute {
            data_type: data_type.to_string(),
            name: name.to_string(),
            key,
            comment: comment.map(|s| s.to_string()),
        });
    }

    /// Get a node ID by key if it exists.
    pub(crate) fn get_node_id(&self, key: &str) -> Option<IrNodeId> {
        self.node_index_by_id.get(key.trim()).copied()
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
