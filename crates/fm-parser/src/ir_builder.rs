use fm_core::{
    ArrowType, Diagnostic, DiagnosticCategory, DiagramType, GraphDirection, IrAttributeKey,
    IrCluster, IrClusterId, IrEdge, IrEdgeKind, IrEndpoint, IrEntityAttribute, IrGraphCluster,
    IrGraphEdge, IrGraphNode, IrLabel, IrLabelId, IrNode, IrNodeId, IrNodeKind, IrSubgraph,
    IrSubgraphId, MermaidDiagramIr, MermaidError, MermaidWarning, MermaidWarningCode, NodeShape,
    Span,
};

use crate::ParseResult;

pub(crate) struct IrBuilder {
    ir: MermaidDiagramIr,
    node_index_by_id: std::collections::BTreeMap<String, IrNodeId>,
    warnings: Vec<String>,
    /// Track nodes that were auto-created (for dangling edge recovery)
    auto_created_nodes: Vec<IrNodeId>,
}

impl IrBuilder {
    pub(crate) fn new(diagram_type: DiagramType) -> Self {
        Self {
            ir: MermaidDiagramIr::empty(diagram_type),
            node_index_by_id: std::collections::BTreeMap::new(),
            warnings: Vec::new(),
            auto_created_nodes: Vec::new(),
        }
    }

    pub(crate) fn set_direction(&mut self, direction: GraphDirection) {
        self.ir.direction = direction;
        self.ir.meta.direction = direction;
    }

    pub(crate) fn set_block_beta_columns(&mut self, columns: usize) {
        self.ir.meta.block_beta_columns = Some(columns.max(1));
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

    pub(crate) fn set_init_flowchart_curve(&mut self, curve: String) {
        self.ir.meta.init.config.flowchart_curve = Some(curve);
    }

    pub(crate) fn set_init_sequence_mirror_actors(&mut self, mirror_actors: bool) {
        self.ir.meta.init.config.sequence_mirror_actors = Some(mirror_actors);
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

    /// Add a rich diagnostic to the IR.
    #[allow(dead_code)] // Will be used by recovery features
    pub(crate) fn add_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.ir.add_diagnostic(diagnostic);
    }

    /// Add an info-level recovery diagnostic.
    #[allow(dead_code)] // Will be used by recovery features
    pub(crate) fn add_recovery_info(&mut self, message: impl Into<String>, span: Option<Span>) {
        let mut diag = Diagnostic::info(message).with_category(DiagnosticCategory::Recovery);
        if let Some(s) = span {
            diag = diag.with_span(s);
        }
        self.ir.add_diagnostic(diag);
    }

    /// Add a warning-level recovery diagnostic.
    #[allow(dead_code)] // Will be used by recovery features
    pub(crate) fn add_recovery_warning(
        &mut self,
        message: impl Into<String>,
        span: Option<Span>,
        suggestion: Option<String>,
    ) {
        let mut diag = Diagnostic::warning(message).with_category(DiagnosticCategory::Recovery);
        if let Some(s) = span {
            diag = diag.with_span(s);
        }
        if let Some(sug) = suggestion {
            diag = diag.with_suggestion(sug);
        }
        self.ir.add_diagnostic(diag);
    }

    pub(crate) fn node_count(&self) -> usize {
        self.ir.nodes.len()
    }

    pub(crate) fn edge_count(&self) -> usize {
        self.ir.edges.len()
    }

    /// Get a node by its IrNodeId.
    pub(crate) fn get_node_by_id(&self, id: IrNodeId) -> Option<&IrNode> {
        self.ir.nodes.get(id.0)
    }

    /// Finish building the IR, applying semantic recovery.
    pub(crate) fn finish(
        mut self,
        confidence: f32,
        detection_method: crate::DetectionMethod,
    ) -> ParseResult {
        // Apply semantic recovery
        self.apply_semantic_recovery();

        ParseResult {
            ir: self.ir,
            warnings: self.warnings,
            confidence,
            detection_method,
        }
    }

    /// Apply semantic recovery strategies.
    fn apply_semantic_recovery(&mut self) {
        // Report auto-created placeholder nodes
        if !self.auto_created_nodes.is_empty() {
            let count = self.auto_created_nodes.len();
            let node_ids: Vec<String> = self
                .auto_created_nodes
                .iter()
                .filter_map(|id| self.ir.nodes.get(id.0).map(|n| n.id.clone()))
                .collect();
            let message = if count == 1 {
                format!(
                    "Auto-created placeholder node '{}' for dangling edge reference",
                    node_ids.first().unwrap_or(&String::new())
                )
            } else {
                format!(
                    "Auto-created {} placeholder nodes for dangling edge references: {}",
                    count,
                    node_ids.join(", ")
                )
            };
            self.ir.add_diagnostic(
                Diagnostic::info(message)
                    .with_category(DiagnosticCategory::Recovery)
                    .with_suggestion(
                        "Define these nodes explicitly for better diagram quality".to_string(),
                    ),
            );
        }

        // Check for unresolved edges and report them
        let unresolved_count = self
            .ir
            .edges
            .iter()
            .filter(|e| {
                matches!(e.from, IrEndpoint::Unresolved) || matches!(e.to, IrEndpoint::Unresolved)
            })
            .count();

        if unresolved_count > 0 {
            self.ir.add_diagnostic(
                Diagnostic::warning(format!(
                    "{} edge(s) have unresolved endpoints",
                    unresolved_count
                ))
                .with_category(DiagnosticCategory::Semantic),
            );
        }
    }

    /// Intern a node, optionally marking it as auto-created (for recovery).
    pub(crate) fn intern_node_auto(
        &mut self,
        id: &str,
        label: Option<&str>,
        shape: NodeShape,
        span: Span,
        is_auto_created: bool,
    ) -> Option<IrNodeId> {
        let normalized_id = id.trim();
        if normalized_id.is_empty() {
            self.add_warning("Encountered empty node identifier; skipped node");
            return None;
        }

        // Check if already exists
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

        // Create new node
        let label_id = clean_label(label).map(|value| self.intern_label(value, span));
        let node_id = IrNodeId(self.ir.nodes.len());
        let node = IrNode {
            id: normalized_id.to_string(),
            label: label_id,
            shape,
            classes: Vec::new(),
            href: None,
            span_primary: span,
            span_all: vec![span],
            implicit: is_auto_created,
            members: Vec::new(),
        };

        self.ir.nodes.push(node);
        self.ir.graph.nodes.push(IrGraphNode {
            node_id,
            kind: self.node_kind(),
            clusters: Vec::new(),
            subgraphs: Vec::new(),
        });
        self.node_index_by_id
            .insert(normalized_id.to_string(), node_id);

        if is_auto_created {
            self.auto_created_nodes.push(node_id);
        }

        Some(node_id)
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

        let title_id = clean_label(title).map(|value| self.intern_label(value, span));
        let cluster_index = self.ir.clusters.len();
        self.ir.clusters.push(IrCluster {
            id: IrClusterId(cluster_index),
            title: title_id,
            members: Vec::new(),
            span,
        });
        self.ir.graph.clusters.push(IrGraphCluster {
            cluster_id: IrClusterId(cluster_index),
            title: title_id,
            members: Vec::new(),
            subgraph: None,
            span,
        });
        Some(cluster_index)
    }

    pub(crate) fn add_node_to_cluster(&mut self, cluster_index: usize, node_id: IrNodeId) {
        let Some(cluster) = self.ir.clusters.get_mut(cluster_index) else {
            return;
        };
        if !cluster.members.contains(&node_id) {
            cluster.members.push(node_id);
        }
        if let Some(graph_cluster) = self.ir.graph.clusters.get_mut(cluster_index)
            && !graph_cluster.members.contains(&node_id)
        {
            graph_cluster.members.push(node_id);
        }
        if let Some(graph_node) = self.ir.graph.nodes.get_mut(node_id.0) {
            let cluster_id = IrClusterId(cluster_index);
            if !graph_node.clusters.contains(&cluster_id) {
                graph_node.clusters.push(cluster_id);
            }
        }
    }

    pub(crate) fn ensure_subgraph(
        &mut self,
        key: &str,
        title: Option<&str>,
        span: Span,
        parent: Option<usize>,
        cluster_index: Option<usize>,
    ) -> Option<usize> {
        let normalized_key = key.trim();
        if normalized_key.is_empty() {
            return None;
        }

        let title_id = clean_label(title).map(|value| self.intern_label(value, span));
        let subgraph_index = self.ir.graph.subgraphs.len();
        let parent_id = parent.map(IrSubgraphId);
        let cluster_id = cluster_index.map(IrClusterId);
        self.ir.graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(subgraph_index),
            key: normalized_key.to_string(),
            title: title_id,
            parent: parent_id,
            children: Vec::new(),
            members: Vec::new(),
            cluster: cluster_id,
            span,
        });
        if let Some(parent_index) = parent
            && let Some(parent_graph) = self.ir.graph.subgraphs.get_mut(parent_index)
        {
            parent_graph.children.push(IrSubgraphId(subgraph_index));
        }
        if let Some(cluster_index) = cluster_index
            && let Some(graph_cluster) = self.ir.graph.clusters.get_mut(cluster_index)
        {
            graph_cluster.subgraph = Some(IrSubgraphId(subgraph_index));
        }
        Some(subgraph_index)
    }

    pub(crate) fn add_node_to_subgraph(&mut self, subgraph_index: usize, node_id: IrNodeId) {
        let Some(subgraph) = self.ir.graph.subgraphs.get_mut(subgraph_index) else {
            return;
        };
        if !subgraph.members.contains(&node_id) {
            subgraph.members.push(node_id);
        }
        if let Some(graph_node) = self.ir.graph.nodes.get_mut(node_id.0) {
            let subgraph_id = IrSubgraphId(subgraph_index);
            if !graph_node.subgraphs.contains(&subgraph_id) {
                graph_node.subgraphs.push(subgraph_id);
            }
        }
    }

    pub(crate) fn intern_node(
        &mut self,
        id: &str,
        label: Option<&str>,
        shape: NodeShape,
        span: Span,
    ) -> Option<IrNodeId> {
        self.intern_node_auto(id, label, shape, span, false)
    }

    /// Intern a node as a placeholder (auto-created for dangling edge recovery).
    #[allow(dead_code)] // Will be used by recovery features
    pub(crate) fn intern_placeholder_node(&mut self, id: &str, span: Span) -> Option<IrNodeId> {
        self.intern_node_auto(id, Some(id), NodeShape::Rect, span, true)
    }

    pub(crate) fn add_class_to_node(&mut self, node_key: &str, class_name: &str, span: Span) {
        let normalized_class = class_name.trim();
        if normalized_class.is_empty() {
            return;
        }

        let Some(node_id) = self.intern_node(node_key, None, NodeShape::Rect, span) else {
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

    pub(crate) fn set_node_link(&mut self, node_key: &str, target: &str, span: Span) {
        let target = target.trim();
        if target.is_empty() {
            return;
        }

        let Some(node_id) = self.intern_node(node_key, None, NodeShape::Rect, span) else {
            return;
        };

        if let Some(node) = self.ir.nodes.get_mut(node_id.0) {
            node.href = Some(target.to_string());
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
        self.ir.graph.edges.push(IrGraphEdge {
            edge_id: self.ir.edges.len() - 1,
            kind: self.edge_kind(),
            from: IrEndpoint::Node(from),
            to: IrEndpoint::Node(to),
            span,
        });
    }

    fn intern_label(&mut self, text: String, span: Span) -> IrLabelId {
        let label_id = IrLabelId(self.ir.labels.len());
        self.ir.labels.push(IrLabel { text, span });
        label_id
    }
}

impl IrBuilder {
    fn node_kind(&self) -> IrNodeKind {
        match self.ir.diagram_type {
            DiagramType::Er => IrNodeKind::Entity,
            DiagramType::Sequence => IrNodeKind::Participant,
            DiagramType::State => IrNodeKind::State,
            DiagramType::Gantt => IrNodeKind::Task,
            DiagramType::Timeline | DiagramType::Journey => IrNodeKind::Event,
            DiagramType::GitGraph => IrNodeKind::Commit,
            DiagramType::Requirement => IrNodeKind::Requirement,
            DiagramType::Pie => IrNodeKind::Slice,
            DiagramType::QuadrantChart | DiagramType::XyChart => IrNodeKind::Point,
            _ => IrNodeKind::Generic,
        }
    }

    fn edge_kind(&self) -> IrEdgeKind {
        match self.ir.diagram_type {
            DiagramType::Er => IrEdgeKind::Relationship,
            DiagramType::Sequence => IrEdgeKind::Message,
            DiagramType::Timeline | DiagramType::Journey => IrEdgeKind::Timeline,
            DiagramType::Gantt => IrEdgeKind::Dependency,
            DiagramType::GitGraph => IrEdgeKind::Commit,
            _ => IrEdgeKind::Generic,
        }
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
