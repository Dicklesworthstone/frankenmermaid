use std::collections::{BTreeMap, HashMap};

use fm_core::{
    ArrowType, ClassMemberKind, ClassStereotype, Diagnostic, DiagnosticCategory, DiagramType,
    FragmentAlternative, FragmentKind, GraphDirection, IrActivation, IrAttributeKey, IrClassMember,
    IrClassNodeMeta, IrCluster, IrClusterId, IrEdge, IrEdgeKind, IrEndpoint, IrEntityAttribute,
    IrGanttMeta, IrGraphCluster, IrGraphEdge, IrGraphNode, IrLabel, IrLabelId, IrLifecycleEvent,
    IrNode, IrNodeId, IrNodeKind, IrParticipantGroup, IrSequenceFragment, IrSequenceMeta,
    IrSequenceNote, IrStyleRef, IrStyleTarget, IrSubgraph, IrSubgraphId, LifecycleEventKind,
    MermaidDiagramIr, MermaidError, MermaidParseMode, MermaidWarning, MermaidWarningCode,
    NodeShape, NotePosition, Span,
};

use crate::ParseResult;
use crate::mermaid_parser::normalize_identifier;

/// Open fragment entry: (kind, label, start_edge, alternatives, child_fragment_indices).
type OpenFragment = (
    FragmentKind,
    String,
    usize,
    Vec<FragmentAlternative>,
    Vec<usize>,
);

#[derive(Debug, Clone)]
struct StateCompositeContext {
    lookup_key: String,
    cluster_index: usize,
    subgraph_index: usize,
    region_count: usize,
    current_region_subgraph: Option<usize>,
    pending_region_members: Vec<IrNodeId>,
}

pub(crate) struct IrBuilder {
    ir: MermaidDiagramIr,
    // Lookups for uniqueness
    node_index_by_id: HashMap<String, IrNodeId>,
    cluster_index_by_key: HashMap<String, usize>,
    subgraph_index_by_key: HashMap<String, usize>,
    label_index_by_text: HashMap<String, IrLabelId>,

    warnings: Vec<String>,
    /// Track nodes that were auto-created (for dangling edge recovery)
    auto_created_nodes: Vec<IrNodeId>,
    /// Stack of open activations per participant name: (node_id, start_edge_index, depth)
    activation_stacks: BTreeMap<String, Vec<(IrNodeId, usize)>>,
    /// Currently open participant group (label, color, collected participant names)
    current_participant_group: Option<(String, Option<String>, Vec<String>)>,
    /// Stack of open fragments
    fragment_stack: Vec<OpenFragment>,
    /// Currently open class block (for member accumulation)
    current_class: Option<String>,
    /// Stack of open composite states for state diagrams.
    state_stack: Vec<StateCompositeContext>,
}

impl IrBuilder {
    pub(crate) fn new(diagram_type: DiagramType) -> Self {
        Self {
            ir: MermaidDiagramIr::empty(diagram_type),
            node_index_by_id: HashMap::new(),
            cluster_index_by_key: HashMap::new(),
            subgraph_index_by_key: HashMap::new(),
            label_index_by_text: HashMap::new(),
            warnings: Vec::new(),
            auto_created_nodes: Vec::new(),
            activation_stacks: BTreeMap::new(),
            current_participant_group: None,
            fragment_stack: Vec::new(),
            current_class: None,
            state_stack: Vec::new(),
        }
    }

    pub(crate) fn set_direction(&mut self, direction: GraphDirection) {
        self.ir.direction = direction;
        self.ir.meta.direction = direction;
    }

    pub(crate) fn set_parse_mode(&mut self, parse_mode: MermaidParseMode) {
        self.ir.meta.parse_mode = parse_mode;
    }

    pub(crate) fn set_block_beta_columns(&mut self, columns: usize) {
        self.ir.meta.block_beta_columns = Some(columns.max(1));
    }

    pub(crate) fn set_gantt_meta(&mut self, gantt_meta: IrGanttMeta) {
        self.ir.gantt_meta = Some(gantt_meta);
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

    pub(crate) fn enable_autonumber(&mut self) {
        self.ir
            .sequence_meta
            .get_or_insert_with(IrSequenceMeta::default)
            .autonumber = true;
    }

    pub(crate) fn add_sequence_note(
        &mut self,
        position: NotePosition,
        participant_names: &[String],
        text: String,
    ) {
        // Resolve participant names to node IDs
        let participants: Vec<IrNodeId> = participant_names
            .iter()
            .filter_map(|name| {
                let normalized = normalize_identifier(name);
                self.node_index_by_id.get(&normalized).copied()
            })
            .collect();

        self.ir
            .sequence_meta
            .get_or_insert_with(IrSequenceMeta::default)
            .notes
            .push(IrSequenceNote {
                position,
                participants,
                text,
            });
    }

    pub(crate) fn activate_participant(&mut self, name: &str) {
        let normalized = normalize_identifier(name);
        let Some(&node_id) = self.node_index_by_id.get(&normalized) else {
            return;
        };
        let edge_index = self.ir.edges.len().saturating_sub(1);
        self.activation_stacks
            .entry(normalized)
            .or_default()
            .push((node_id, edge_index));
    }

    pub(crate) fn deactivate_participant(&mut self, name: &str) {
        let normalized = normalize_identifier(name);
        let Some(stack) = self.activation_stacks.get_mut(&normalized) else {
            return;
        };
        let Some((node_id, start_edge)) = stack.pop() else {
            return;
        };
        let end_edge = self.ir.edges.len().saturating_sub(1);
        let depth = stack.len(); // remaining stack depth = nesting level

        self.ir
            .sequence_meta
            .get_or_insert_with(IrSequenceMeta::default)
            .activations
            .push(IrActivation {
                participant: node_id,
                start_edge,
                end_edge,
                depth,
            });
    }

    pub(crate) fn begin_participant_group(&mut self, label: String, color: Option<String>) {
        // If there's already an open group, auto-close it
        self.end_participant_group();
        self.current_participant_group = Some((label, color, Vec::new()));
    }

    pub(crate) fn end_participant_group(&mut self) {
        if let Some((label, color, names)) = self.current_participant_group.take() {
            let participants: Vec<IrNodeId> = names
                .iter()
                .filter_map(|name| self.node_index_by_id.get(name).copied())
                .collect();

            if !participants.is_empty() {
                self.ir
                    .sequence_meta
                    .get_or_insert_with(IrSequenceMeta::default)
                    .participant_groups
                    .push(IrParticipantGroup {
                        label,
                        color,
                        participants,
                    });
            }
        }
    }

    /// Record that a participant declared inside a box group should be tracked.
    pub(crate) fn track_participant_in_group(&mut self, name: &str) {
        if let Some((_, _, ref mut names)) = self.current_participant_group {
            let normalized = normalize_identifier(name);
            if !normalized.is_empty() {
                names.push(normalized);
            }
        }
    }

    pub(crate) fn add_lifecycle_create(&mut self, name: &str) {
        let normalized = normalize_identifier(name);
        let Some(&node_id) = self.node_index_by_id.get(&normalized) else {
            return;
        };
        let at_edge = self.ir.edges.len();
        self.ir
            .sequence_meta
            .get_or_insert_with(IrSequenceMeta::default)
            .lifecycle_events
            .push(IrLifecycleEvent {
                kind: LifecycleEventKind::Create,
                participant: node_id,
                at_edge,
            });
    }

    pub(crate) fn add_lifecycle_destroy(&mut self, name: &str) {
        let normalized = normalize_identifier(name);
        let Some(&node_id) = self.node_index_by_id.get(&normalized) else {
            return;
        };
        let at_edge = self.ir.edges.len().saturating_sub(1);
        self.ir
            .sequence_meta
            .get_or_insert_with(IrSequenceMeta::default)
            .lifecycle_events
            .push(IrLifecycleEvent {
                kind: LifecycleEventKind::Destroy,
                participant: node_id,
                at_edge,
            });
    }

    pub(crate) fn set_current_class(&mut self, name: &str) {
        self.current_class = Some(name.to_string());
    }

    pub(crate) fn clear_current_class(&mut self) {
        self.current_class = None;
    }

    pub(crate) fn add_class_member(&mut self, member: IrClassMember) {
        let Some(class_name) = self.current_class.as_ref() else {
            return;
        };
        let Some(&node_id) = self.node_index_by_id.get(class_name) else {
            return;
        };
        let Some(node) = self.ir.nodes.get_mut(node_id.0) else {
            return;
        };
        let meta = node.class_meta.get_or_insert_with(IrClassNodeMeta::default);
        match member.kind {
            ClassMemberKind::Attribute => meta.attributes.push(member),
            ClassMemberKind::Method => meta.methods.push(member),
        }
    }

    pub(crate) fn set_class_stereotype(&mut self, class_name: &str, stereotype: ClassStereotype) {
        let Some(&node_id) = self.node_index_by_id.get(class_name) else {
            return;
        };
        let Some(node) = self.ir.nodes.get_mut(node_id.0) else {
            return;
        };
        node.class_meta
            .get_or_insert_with(IrClassNodeMeta::default)
            .stereotype = Some(stereotype);
    }

    pub(crate) fn set_class_generics(&mut self, class_name: &str, generics: Vec<String>) {
        let Some(&node_id) = self.node_index_by_id.get(class_name) else {
            return;
        };
        let Some(node) = self.ir.nodes.get_mut(node_id.0) else {
            return;
        };
        node.class_meta
            .get_or_insert_with(IrClassNodeMeta::default)
            .generics = generics;
    }

    pub(crate) fn begin_state_cluster(&mut self, name: &str, title: Option<&str>, span: Span) {
        let parent_subgraph = self
            .state_stack
            .last()
            .map(|context| context.subgraph_index);
        let lookup_key = self
            .state_stack
            .last()
            .map(|context| format!("{}/{}", context.lookup_key, name))
            .unwrap_or_else(|| format!("state/{name}"));

        let Some(cluster_index) = self.ensure_cluster(&lookup_key, title.or(Some(name)), span)
        else {
            return;
        };
        let Some(subgraph_index) = self.ensure_subgraph(
            &lookup_key,
            name,
            title.or(Some(name)),
            span,
            parent_subgraph,
            Some(cluster_index),
        ) else {
            return;
        };

        self.state_stack.push(StateCompositeContext {
            lookup_key,
            cluster_index,
            subgraph_index,
            region_count: 0,
            current_region_subgraph: None,
            pending_region_members: Vec::new(),
        });
    }

    pub(crate) fn end_state_cluster(&mut self) -> bool {
        self.state_stack.pop().is_some()
    }

    pub(crate) fn advance_state_region(&mut self, span: Span) -> bool {
        let Some(mut context) = self.state_stack.pop() else {
            return false;
        };

        if context.region_count == 0 {
            let Some(first_region_subgraph) = self.ensure_subgraph(
                &format!("{}/__region_1", context.lookup_key),
                "__state_region_1",
                None,
                span,
                Some(context.subgraph_index),
                None,
            ) else {
                self.state_stack.push(context);
                return false;
            };
            for node_id in context.pending_region_members.iter().copied() {
                self.add_node_to_subgraph(first_region_subgraph, node_id);
            }
        }

        let next_region_number = context.region_count + 2;
        let Some(next_region_subgraph) = self.ensure_subgraph(
            &format!("{}/__region_{next_region_number}", context.lookup_key),
            &format!("__state_region_{next_region_number}"),
            None,
            span,
            Some(context.subgraph_index),
            None,
        ) else {
            self.state_stack.push(context);
            return false;
        };

        context.region_count += 1;
        let total_regions = context.region_count + 1;
        self.set_cluster_grid_span(context.cluster_index, total_regions);
        self.set_subgraph_grid_span(context.subgraph_index, total_regions);
        context.current_region_subgraph = Some(next_region_subgraph);
        context.pending_region_members.clear();
        self.state_stack.push(context);
        true
    }

    pub(crate) fn attach_state_node(&mut self, node_id: IrNodeId) {
        for context_index in 0..self.state_stack.len() {
            let (cluster_index, subgraph_index, current_region_subgraph, should_track_member) = {
                let context = &self.state_stack[context_index];
                (
                    context.cluster_index,
                    context.subgraph_index,
                    context.current_region_subgraph,
                    !context.pending_region_members.contains(&node_id),
                )
            };

            self.add_node_to_cluster(cluster_index, node_id);
            self.add_node_to_subgraph(subgraph_index, node_id);
            if let Some(region_subgraph_index) = current_region_subgraph {
                self.add_node_to_subgraph(region_subgraph_index, node_id);
            }

            if should_track_member && let Some(context) = self.state_stack.get_mut(context_index) {
                context.pending_region_members.push(node_id);
            }
        }
    }

    pub(crate) fn begin_fragment(&mut self, kind: FragmentKind, label: String) {
        let start_edge = self.ir.edges.len();
        self.fragment_stack
            .push((kind, label, start_edge, Vec::new(), Vec::new()));
    }

    pub(crate) fn add_fragment_alternative(&mut self, label: String) {
        if let Some((_, _, _, alternatives, _)) = self.fragment_stack.last_mut() {
            let start_edge = self.ir.edges.len();
            // Close the previous section's end_edge
            // The alternative starts at the current edge index
            alternatives.push(FragmentAlternative {
                label,
                start_edge,
                end_edge: start_edge, // will be updated when the next else/end arrives
            });
        }
    }

    /// Close the innermost open fragment. Returns true if a fragment was closed.
    pub(crate) fn end_fragment(&mut self) -> bool {
        let Some((kind, label, start_edge, mut alternatives, children)) = self.fragment_stack.pop()
        else {
            return false;
        };

        let end_edge = self.ir.edges.len().saturating_sub(1);

        // Update the end_edge of the last alternative
        if let Some(last_alt) = alternatives.last_mut() {
            last_alt.end_edge = end_edge;
        }

        let meta = self
            .ir
            .sequence_meta
            .get_or_insert_with(IrSequenceMeta::default);
        let fragment_index = meta.fragments.len();
        meta.fragments.push(IrSequenceFragment {
            kind,
            label,
            start_edge,
            end_edge,
            alternatives,
            children,
        });

        // Register as a child of the parent fragment, if any
        if let Some((_, _, _, _, parent_children)) = self.fragment_stack.last_mut() {
            parent_children.push(fragment_index);
        }

        true
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

    /// Look up a node ID by its string key (as used in the diagram source).
    pub(crate) fn node_id_by_key(&self, key: &str) -> Option<&IrNodeId> {
        self.node_index_by_id.get(key)
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
        // Close any remaining open fragments, activations, and participant groups
        while self.end_fragment() {}
        self.flush_open_activations();
        self.end_participant_group();

        // Apply semantic recovery
        self.apply_semantic_recovery();

        ParseResult {
            ir: self.ir,
            warnings: self.warnings,
            confidence,
            detection_method,
        }
    }

    /// Close any remaining open activations (auto-close at end of diagram).
    fn flush_open_activations(&mut self) {
        let end_edge = self.ir.edges.len().saturating_sub(1);
        let stacks = std::mem::take(&mut self.activation_stacks);
        for (_name, stack) in stacks {
            for (idx, (node_id, start_edge)) in stack.into_iter().enumerate() {
                self.ir
                    .sequence_meta
                    .get_or_insert_with(IrSequenceMeta::default)
                    .activations
                    .push(IrActivation {
                        participant: node_id,
                        start_edge,
                        end_edge,
                        depth: idx,
                    });
            }
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
            class_meta: None,
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
        lookup_key: &str,
        title: Option<&str>,
        span: Span,
    ) -> Option<usize> {
        let normalized_key = lookup_key.trim();
        if normalized_key.is_empty() {
            return None;
        }

        if let Some(&existing_index) = self.cluster_index_by_key.get(normalized_key) {
            // If the re-opened cluster has a title but the existing one doesn't,
            // update it.
            if let Some(title_text) = clean_label(title) {
                let existing_title = self.ir.clusters.get(existing_index).and_then(|c| c.title);
                let graph_title = self
                    .ir
                    .graph
                    .clusters
                    .get(existing_index)
                    .and_then(|c| c.title);

                if existing_title.is_none() || graph_title.is_none() {
                    let label_id = self.intern_label(title_text, span);
                    if let Some(cluster) = self.ir.clusters.get_mut(existing_index)
                        && cluster.title.is_none()
                    {
                        cluster.title = Some(label_id);
                    }
                    if let Some(graph_cluster) = self.ir.graph.clusters.get_mut(existing_index)
                        && graph_cluster.title.is_none()
                    {
                        graph_cluster.title = Some(label_id);
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
            grid_span: 1,
            span,
        });
        self.ir.graph.clusters.push(IrGraphCluster {
            cluster_id: IrClusterId(cluster_index),
            title: title_id,
            members: Vec::new(),
            subgraph: None,
            grid_span: 1,
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
        lookup_key: &str,
        public_key: &str,
        title: Option<&str>,
        span: Span,
        parent: Option<usize>,
        cluster_index: Option<usize>,
    ) -> Option<usize> {
        let normalized_lookup_key = lookup_key.trim();
        let normalized_public_key = public_key.trim();
        if normalized_lookup_key.is_empty() || normalized_public_key.is_empty() {
            return None;
        }

        if let Some(&existing_index) = self.subgraph_index_by_key.get(normalized_lookup_key) {
            // Update title if needed
            if let Some(title_text) = clean_label(title) {
                let existing_title = self
                    .ir
                    .graph
                    .subgraphs
                    .get(existing_index)
                    .and_then(|s| s.title);
                if existing_title.is_none() {
                    let label_id = self.intern_label(title_text, span);
                    if let Some(subgraph) = self.ir.graph.subgraphs.get_mut(existing_index) {
                        subgraph.title = Some(label_id);
                    }
                }
            }
            return Some(existing_index);
        }

        let title_id = clean_label(title).map(|value| self.intern_label(value, span));
        let subgraph_index = self.ir.graph.subgraphs.len();
        let parent_id = parent.map(IrSubgraphId);
        let cluster_id = cluster_index.map(IrClusterId);
        self.ir.graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(subgraph_index),
            key: normalized_public_key.to_string(),
            title: title_id,
            parent: parent_id,
            children: Vec::new(),
            members: Vec::new(),
            cluster: cluster_id,
            grid_span: 1,
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
        self.subgraph_index_by_key
            .insert(normalized_lookup_key.to_string(), subgraph_index);
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

    pub(crate) fn set_cluster_grid_span(&mut self, cluster_index: usize, grid_span: usize) {
        let grid_span = grid_span.max(1);
        if let Some(cluster) = self.ir.clusters.get_mut(cluster_index) {
            cluster.grid_span = grid_span;
        }
        if let Some(graph_cluster) = self.ir.graph.clusters.get_mut(cluster_index) {
            graph_cluster.grid_span = grid_span;
        }
    }

    pub(crate) fn set_subgraph_grid_span(&mut self, subgraph_index: usize, grid_span: usize) {
        let grid_span = grid_span.max(1);
        if let Some(subgraph) = self.ir.graph.subgraphs.get_mut(subgraph_index) {
            subgraph.grid_span = grid_span;
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

    pub(crate) fn push_style_ref(&mut self, target: IrStyleTarget, style: String, span: Span) {
        self.ir.style_refs.push(IrStyleRef {
            target,
            style,
            span,
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
        if let Some(&existing_id) = self.label_index_by_text.get(&text) {
            return existing_id;
        }

        let label_id = IrLabelId(self.ir.labels.len());
        self.ir.labels.push(IrLabel {
            text: text.clone(),
            span,
        });
        self.label_index_by_text.insert(text, label_id);
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

#[cfg(test)]
mod tests {
    use super::IrBuilder;
    use fm_core::{DiagramType, NodeShape, Span};

    #[test]
    fn intern_node_reuses_existing_lookup_entry() {
        let mut builder = IrBuilder::new(DiagramType::Flowchart);
        let span = Span::default();

        let first = builder
            .intern_node("A", None, NodeShape::Rect, span)
            .expect("first node should be created");
        let second = builder
            .intern_node("A", Some("Alpha"), NodeShape::Diamond, span)
            .expect("existing node should be reused");

        assert_eq!(first, second);

        let node = builder.get_node_by_id(first).expect("node should exist");
        assert_eq!(node.shape, NodeShape::Diamond);
        assert!(
            node.label.is_some(),
            "missing label should be upgraded in place"
        );
    }

    #[test]
    fn finish_flushes_activation_stacks_in_name_order() {
        let mut builder = IrBuilder::new(DiagramType::Sequence);
        let span = Span::default();

        let _ = builder.intern_node("beta", Some("beta"), NodeShape::Rect, span);
        let _ = builder.intern_node("alpha", Some("alpha"), NodeShape::Rect, span);

        builder.activate_participant("beta");
        builder.activate_participant("alpha");

        let result = builder.finish(1.0, crate::DetectionMethod::ExactKeyword);
        let activations = &result
            .ir
            .sequence_meta
            .expect("sequence metadata should exist")
            .activations;

        assert_eq!(activations.len(), 2);
        assert_eq!(activations[0].participant.0, 1);
        assert_eq!(activations[1].participant.0, 0);
    }
}
