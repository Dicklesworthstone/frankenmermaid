use std::collections::BTreeMap;
use std::collections::hash_map::Entry;

use rustc_hash::{FxHashMap, FxHashSet};

use fm_core::{
    ArrowType, ClassMemberKind, ClassStereotype, Diagnostic, DiagnosticCategory, DiagramType,
    FragmentAlternative, FragmentKind, GraphDirection, IrActivation, IrAttributeKey, IrC4NodeMeta,
    IrClassMember, IrClassNodeMeta, IrCluster, IrClusterId, IrEdge, IrEdgeKind, IrEndpoint,
    IrEntityAttribute, IrGanttMeta, IrGraphCluster, IrGraphEdge, IrGraphNode, IrLabel, IrLabelId,
    IrLabelSegment, IrLifecycleEvent, IrNode, IrNodeId, IrNodeKind, IrParticipantGroup,
    IrSequenceFragment, IrSequenceMeta, IrSequenceNote, IrStyleRef, IrStyleTarget, IrSubgraph,
    IrSubgraphId, IrXyChartMeta, LifecycleEventKind, MermaidDiagramIr, MermaidError,
    MermaidParseMode, MermaidSanitizeMode, MermaidWarning, MermaidWarningCode, NodeShape,
    NotePosition, Span,
};

use crate::mermaid_parser::trim_fast;
use crate::{ParseResult, ParserConfig, normalize_identifier};

/// Open fragment entry: (kind, label, `start_edge`, alternatives, `child_fragment_indices`).
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

/// Node-id → `IrNodeId` lookup that keys by the FxHash of the id rather than storing an owned `String`
/// key. The id is already owned once in `ir.nodes[id].id`; the previous `FxHashMap<String, _>` cloned it
/// a SECOND time per node purely for the map key (the keys accumulate through lowering, so they are not
/// allocator-recycled — a real per-node allocation on every diagram). Keying by `u64` removes that clone;
/// lookups verify the candidate against `ir.nodes[..].id` so a hash collision can never resolve to the
/// wrong node (collisions land in `Many`). The map is never iterated (IR order comes from `ir.nodes`), so
/// keying by hash is determinism-safe.
#[derive(Default)]
struct NodeIdIndex {
    buckets: FxHashMap<u64, NodeIdBucket>,
}

enum NodeIdBucket {
    One(IrNodeId),
    Many(Vec<IrNodeId>),
}

impl NodeIdIndex {
    /// Pre-size the bucket map so a large diagram's node interning doesn't rehash ~log2(N) times
    /// (measured as `RawTable::reserve_rehash` on the hot parse path). Capacity-only, behavior-identical.
    fn with_capacity(capacity: usize) -> Self {
        Self {
            buckets: FxHashMap::with_capacity_and_hasher(capacity, rustc_hash::FxBuildHasher),
        }
    }

    fn hash_key(id: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = rustc_hash::FxHasher::default();
        id.hash(&mut hasher);
        hasher.finish()
    }

    fn get(&self, id: &str, nodes: &[IrNode]) -> Option<IrNodeId> {
        self.get_with_hash(Self::hash_key(id), id, nodes)
    }

    /// Like [`Self::get`] but with a caller-precomputed `hash` (from [`Self::hash_key`]). Lets the
    /// intern hot path (`intern_node_auto`) hash the id ONCE for its get+insert pair instead of
    /// twice (a full `FxHasher` run per new node was redundant). Behaviour-identical to `get`.
    fn get_with_hash(&self, hash: u64, id: &str, nodes: &[IrNode]) -> Option<IrNodeId> {
        let matches = |nid: &IrNodeId| nodes.get(nid.0).is_some_and(|node| node.id == id);
        match self.buckets.get(&hash)? {
            NodeIdBucket::One(nid) => matches(nid).then_some(*nid),
            NodeIdBucket::Many(candidates) => candidates.iter().copied().find(|nid| matches(nid)),
        }
    }

    /// Record `node_id` under a caller-precomputed `hash` (from [`Self::hash_key`]). Callers
    /// guarantee the id is not already present (`intern_node_auto` checks `get_with_hash` first),
    /// so an occupied slot here is always a hash COLLISION between distinct ids.
    fn insert_with_hash(&mut self, hash: u64, node_id: IrNodeId) {
        // `entry` locates the slot in ONE probe; the old `get_mut(&hash)` + `insert(hash, ..)` pair
        // probed the bucket map twice on the common vacant path (every distinct node id). The bucket
        // transitions are identical, so this is behaviour-identical.
        match self.buckets.entry(hash) {
            Entry::Vacant(slot) => {
                slot.insert(NodeIdBucket::One(node_id));
            }
            Entry::Occupied(mut slot) => {
                let bucket = slot.get_mut();
                match bucket {
                    NodeIdBucket::One(existing) => {
                        *bucket = NodeIdBucket::Many(vec![*existing, node_id]);
                    }
                    NodeIdBucket::Many(candidates) => candidates.push(node_id),
                }
            }
        }
    }
}

/// Label-dedup index that keys by the FxHash of `(text, segments)` instead of storing an owned
/// `(String, Vec<IrLabelSegment>)` key. The label text is already owned in `ir.labels[id].text` and the
/// segments in `ir.label_markup[id]`; the previous `FxHashMap<(String, Vec<_>), _>` cloned BOTH a second
/// time per distinct label purely for the dedup key. Keying by hash removes those clones; lookups verify
/// the candidate against `ir.labels`/`ir.label_markup` so a hash collision can never dedup two distinct
/// labels together (collisions land in `Many`). Never iterated, so hash-keying is determinism-safe.
#[derive(Default)]
struct LabelIndex {
    buckets: FxHashMap<u64, LabelBucket>,
}

enum LabelBucket {
    One(IrLabelId),
    Many(Vec<IrLabelId>),
}

impl LabelIndex {
    /// Pre-size the bucket map — see [`NodeIdIndex::with_capacity`]. Capacity-only, behavior-identical.
    fn with_capacity(capacity: usize) -> Self {
        Self {
            buckets: FxHashMap::with_capacity_and_hasher(capacity, rustc_hash::FxBuildHasher),
        }
    }

    fn hash_key(text: &str, segments: &[IrLabelSegment]) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = rustc_hash::FxHasher::default();
        text.hash(&mut hasher);
        segments.hash(&mut hasher);
        hasher.finish()
    }

    /// Look up `(text, segments)` under a caller-precomputed `hash` (from [`Self::hash_key`]). Lets
    /// `intern_label` hash the pair ONCE for its get+insert pair instead of twice per new label.
    fn get_with_hash(
        &self,
        hash: u64,
        text: &str,
        segments: &[IrLabelSegment],
        labels: &[IrLabel],
        markup: &BTreeMap<IrLabelId, Vec<IrLabelSegment>>,
    ) -> Option<IrLabelId> {
        let matches = |lid: &IrLabelId| {
            labels.get(lid.0).is_some_and(|label| label.text == text)
                && markup.get(lid).map_or(&[][..], Vec::as_slice) == segments
        };
        match self.buckets.get(&hash)? {
            LabelBucket::One(lid) => matches(lid).then_some(*lid),
            LabelBucket::Many(candidates) => candidates.iter().copied().find(|lid| matches(lid)),
        }
    }

    /// Record `label_id` under a caller-precomputed `hash` (from [`Self::hash_key`]). Callers
    /// guarantee the pair is not already present (`intern_label` checks `get_with_hash` first),
    /// so an occupied slot is always a hash COLLISION.
    fn insert_with_hash(&mut self, hash: u64, label_id: IrLabelId) {
        // One-probe `entry` in place of `get_mut` + `insert` (two probes on the vacant path, hit for
        // every distinct label). Bucket transitions identical — behaviour-identical. See
        // `NodeIdIndex::insert_with_hash`.
        match self.buckets.entry(hash) {
            Entry::Vacant(slot) => {
                slot.insert(LabelBucket::One(label_id));
            }
            Entry::Occupied(mut slot) => {
                let bucket = slot.get_mut();
                match bucket {
                    LabelBucket::One(existing) => {
                        *bucket = LabelBucket::Many(vec![*existing, label_id]);
                    }
                    LabelBucket::Many(candidates) => candidates.push(label_id),
                }
            }
        }
    }
}

pub struct IrBuilder {
    ir: MermaidDiagramIr,
    // Lookups for uniqueness. These are read by key only (never iterated), so a hash
    // map is both faster and determinism-safe — IR output order comes from the `ir`
    // vectors, not from map iteration.
    node_id_index: NodeIdIndex,
    cluster_index_by_key: FxHashMap<String, usize>,
    subgraph_index_by_key: FxHashMap<String, usize>,
    /// O(1) membership dedup for `(cluster_index, node_id)` / `(subgraph_index, node_id)` — the
    /// `cluster.members`/`subgraph.members` Vecs are append-only and grow to the subgraph size, so
    /// the old `members.contains(&id)` linear dedup-on-insert was O(subgraph²) (measured ~58% of a
    /// big-subgraph parse). These sets mirror those Vecs exactly (both start empty, both are only
    /// appended here), so gating the push on the set is byte-identical.
    cluster_member_set: FxHashSet<(usize, IrNodeId)>,
    subgraph_member_set: FxHashSet<(usize, IrNodeId)>,
    label_index: LabelIndex,

    warnings: Vec<String>,
    /// Track nodes that were auto-created (for dangling edge recovery)
    auto_created_nodes: Vec<IrNodeId>,
    /// Stack of open activations per participant name: (`node_id`, `start_edge_index`, depth)
    activation_stacks: BTreeMap<String, Vec<(IrNodeId, usize)>>,
    /// Currently open participant group (label, color, collected participant names)
    current_participant_group: Option<(String, Option<String>, Vec<String>)>,
    /// Stack of open fragments
    fragment_stack: Vec<OpenFragment>,
    /// Node id of the currently open class block, resolved once when the block opens so each member add
    /// skips a `NodeIdIndex` hash+lookup+id-compare (the class name is invariant across a block's members).
    current_class_node_id: Option<IrNodeId>,
    /// Stack of open composite states for state diagrams.
    state_stack: Vec<StateCompositeContext>,
    parser_config: ParserConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLabel {
    pub(crate) text: String,
    pub(crate) segments: Vec<IrLabelSegment>,
}

#[derive(Clone)]
enum NodeLabelInput<'a> {
    Parsed(&'a ParsedLabel),
    Plain(&'a str),
    /// Owned label the caller hands over by value (moved, not cloned, into the IR on the create
    /// path). Used by the flowchart lowering pass to consume its `FastNode` label.
    ParsedOwned(ParsedLabel),
}

impl ParsedLabel {
    pub(crate) fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            segments: Vec::new(),
        }
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.text
    }
}

impl IrBuilder {
    pub(crate) fn new(diagram_type: DiagramType) -> Self {
        Self {
            ir: MermaidDiagramIr::empty(diagram_type),
            node_id_index: NodeIdIndex::default(),
            cluster_index_by_key: FxHashMap::default(),
            subgraph_index_by_key: FxHashMap::default(),
            cluster_member_set: FxHashSet::default(),
            subgraph_member_set: FxHashSet::default(),
            label_index: LabelIndex::default(),
            warnings: Vec::new(),
            auto_created_nodes: Vec::new(),
            activation_stacks: BTreeMap::new(),
            current_participant_group: None,
            fragment_stack: Vec::new(),
            current_class_node_id: None,
            state_stack: Vec::new(),
            parser_config: ParserConfig::default(),
        }
    }

    /// Create a builder with pre-sized IR vectors based on input line count.
    ///
    /// Heuristic: each non-empty input line produces ~0.5 nodes and ~0.3 edges.
    pub(crate) fn with_capacity_hint(diagram_type: DiagramType, input_lines: usize) -> Self {
        // Timeline creates a period AND event node per data line, so it needs ~`2 * input_lines` nodes/labels.
        // Other NODE-PER-LINE diagrams (journey/gantt/mindmap/kanban/pie/xychart) need ~`input_lines`, while
        // EDGE-HEAVY diagrams (flowchart/er/state/class) need ~`input_lines/2`. Sizing the
        // node & label indexes per-type shrinks the `NodeIdIndex`/`LabelIndex`/member-set `reserve_rehash`
        // (the `/2` estimate was ~2-4× short — timeline builds a year + event node per line) WITHOUT
        // over-reserving the edge-heavy common case (the `_` arm is byte-for-byte unchanged: flowchart/er
        // NEUTRAL). `with_capacity_hint` runs once per parse (cold) ⇒ no hot-path codegen change.
        // Capacity-only ⇒ behavior-identical.
        let estimated_nodes = match diagram_type {
            DiagramType::Timeline => input_lines.saturating_mul(2).max(4),
            DiagramType::Journey
            | DiagramType::Gantt
            | DiagramType::Mindmap
            | DiagramType::Kanban
            | DiagramType::Pie
            | DiagramType::XyChart => input_lines.max(4),
            _ => (input_lines / 2).max(4),
        };
        let estimated_edges = (input_lines / 3).max(2);
        let estimated_labels = estimated_nodes;
        let mut ir = MermaidDiagramIr::empty(diagram_type);
        ir.reserve_capacity(estimated_nodes, estimated_edges, estimated_labels);
        Self {
            ir,
            node_id_index: NodeIdIndex::with_capacity(estimated_nodes),
            cluster_index_by_key: FxHashMap::default(),
            subgraph_index_by_key: FxHashMap::default(),
            cluster_member_set: FxHashSet::default(),
            subgraph_member_set: FxHashSet::default(),
            label_index: LabelIndex::with_capacity(estimated_labels),
            warnings: Vec::new(),
            auto_created_nodes: Vec::new(),
            activation_stacks: BTreeMap::new(),
            current_participant_group: None,
            fragment_stack: Vec::new(),
            current_class_node_id: None,
            state_stack: Vec::new(),
            parser_config: ParserConfig::default(),
        }
    }

    pub(crate) const fn set_direction(&mut self, direction: GraphDirection) {
        self.ir.direction = direction;
        self.ir.meta.direction = direction;
    }

    pub(crate) fn set_subgraph_direction(
        &mut self,
        subgraph_index: usize,
        direction: GraphDirection,
    ) {
        if let Some(subgraph) = self.ir.graph.subgraphs.get_mut(subgraph_index) {
            subgraph.direction = Some(direction);
        }
    }

    pub(crate) const fn set_parse_mode(&mut self, parse_mode: MermaidParseMode) {
        self.ir.meta.parse_mode = parse_mode;
    }

    pub(crate) const fn set_parser_config(&mut self, parser_config: ParserConfig) {
        self.parser_config = parser_config;
    }

    pub(crate) const fn parser_config(&self) -> &ParserConfig {
        &self.parser_config
    }

    pub(crate) fn set_block_beta_columns(&mut self, columns: usize) {
        self.ir.meta.block_beta_columns = Some(columns.max(1));
    }

    pub(crate) fn set_gantt_meta(&mut self, gantt_meta: IrGanttMeta) {
        self.ir.gantt_meta = Some(gantt_meta);
    }

    pub(crate) fn set_xy_chart_meta(&mut self, xy_chart_meta: IrXyChartMeta) {
        self.ir.xy_chart_meta = Some(xy_chart_meta);
    }

    pub(crate) fn set_pie_meta(&mut self, pie_meta: fm_core::IrPieMeta) {
        self.ir.pie_meta = Some(pie_meta);
    }

    pub(crate) fn set_quadrant_meta(&mut self, quadrant_meta: fm_core::IrQuadrantMeta) {
        self.ir.quadrant_meta = Some(quadrant_meta);
    }

    pub(crate) fn set_acc_title(&mut self, title: String) {
        self.ir.meta.acc_title = Some(title);
    }

    pub(crate) fn set_title(&mut self, title: String) {
        self.ir.meta.title = Some(title);
    }

    pub(crate) fn set_acc_descr(&mut self, descr: String) {
        self.ir.meta.acc_descr = Some(descr);
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

    pub(crate) const fn set_init_flowchart_direction(&mut self, direction: GraphDirection) {
        self.ir.meta.init.config.flowchart_direction = Some(direction);
    }

    pub(crate) fn set_init_flowchart_curve(&mut self, curve: String) {
        self.ir.meta.init.config.flowchart_curve = Some(curve);
    }

    pub(crate) const fn set_init_sequence_mirror_actors(&mut self, mirror_actors: bool) {
        self.ir.meta.init.config.sequence_mirror_actors = Some(mirror_actors);
    }

    pub(crate) fn set_init_sequence_show_sequence_numbers(&mut self, show_numbers: bool) {
        self.ir.meta.init.config.sequence_show_sequence_numbers = Some(show_numbers);
        if self.ir.diagram_type == DiagramType::Sequence && show_numbers {
            self.enable_autonumber();
        }
    }

    pub(crate) const fn set_init_sanitize_mode(&mut self, sanitize_mode: MermaidSanitizeMode) {
        self.ir.meta.init.config.sanitize_mode = sanitize_mode;
    }

    pub(crate) const fn sanitize_mode(&self) -> MermaidSanitizeMode {
        self.ir.meta.init.config.sanitize_mode
    }

    pub(crate) const fn set_c4_show_legend(&mut self, show_legend: bool) {
        self.ir.meta.c4_show_legend = show_legend;
    }

    pub(crate) fn enable_autonumber(&mut self) {
        self.enable_autonumber_with(1, 1);
    }

    pub(crate) fn enable_autonumber_with(&mut self, start: u32, increment: u32) {
        let meta = self
            .ir
            .sequence_meta
            .get_or_insert_with(IrSequenceMeta::default);
        meta.autonumber = true;
        meta.autonumber_start = start;
        meta.autonumber_increment = increment;
    }

    pub(crate) fn hide_sequence_footbox(&mut self) {
        self.ir
            .sequence_meta
            .get_or_insert_with(IrSequenceMeta::default)
            .hide_footbox = true;
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
                self.node_id_index.get(&normalized, &self.ir.nodes)
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
                after_edge: self.ir.edges.len().saturating_sub(1),
            });
    }

    pub(crate) fn activate_participant(&mut self, name: &str) {
        let normalized = normalize_identifier(name);
        let Some(node_id) = self.node_id_index.get(&normalized, &self.ir.nodes) else {
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
                .filter_map(|name| self.node_id_index.get(name, &self.ir.nodes))
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
        let Some(node_id) = self.node_id_index.get(&normalized, &self.ir.nodes) else {
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
        let Some(node_id) = self.node_id_index.get(&normalized, &self.ir.nodes) else {
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
        // Callers intern the class node immediately before this (see `lower_class_statement`'s
        // `BlockStart` arm), and node ids are stable append indices, so resolving here is identical to
        // resolving per member — and lets `add_class_member` skip the lookup entirely.
        self.current_class_node_id = self.node_id_index.get(name, &self.ir.nodes);
    }

    pub(crate) fn clear_current_class(&mut self) {
        self.current_class_node_id = None;
    }

    pub(crate) fn add_class_member(&mut self, member: IrClassMember) {
        // `current_class_node_id` was resolved once in `set_current_class` — same node the per-member
        // `node_id_index.get(class_name)` would return, without re-hashing the class name each member.
        let Some(node_id) = self.current_class_node_id else {
            return;
        };
        let Some(node) = self.ir.nodes.get_mut(node_id.0) else {
            return;
        };
        let meta = node
            .class_meta
            .get_or_insert_with(|| Box::new(IrClassNodeMeta::default()));
        match member.kind {
            ClassMemberKind::Attribute => meta.attributes.push(member),
            ClassMemberKind::Method => meta.methods.push(member),
        }
    }

    pub(crate) fn set_class_stereotype(&mut self, class_name: &str, stereotype: ClassStereotype) {
        let Some(node_id) = self.node_id_index.get(class_name, &self.ir.nodes) else {
            return;
        };
        let Some(node) = self.ir.nodes.get_mut(node_id.0) else {
            return;
        };
        node.class_meta
            .get_or_insert_with(|| Box::new(IrClassNodeMeta::default()))
            .stereotype = Some(stereotype);
    }

    pub(crate) fn set_class_generics(&mut self, class_name: &str, generics: Vec<String>) {
        let Some(node_id) = self.node_id_index.get(class_name, &self.ir.nodes) else {
            return;
        };
        let Some(node) = self.ir.nodes.get_mut(node_id.0) else {
            return;
        };
        node.class_meta
            .get_or_insert_with(|| Box::new(IrClassNodeMeta::default()))
            .generics = generics;
    }

    pub(crate) fn begin_state_cluster(&mut self, name: &str, title: Option<&str>, span: Span) {
        let parent_subgraph = self
            .state_stack
            .last()
            .map(|context| context.subgraph_index);
        let lookup_key = self.state_stack.last().map_or_else(
            || format!("state/{name}"),
            |context| format!("{}/{}", context.lookup_key, name),
        );

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

    pub(crate) fn begin_fragment(
        &mut self,
        kind: FragmentKind,
        label: String,
        color: Option<String>,
    ) {
        let start_edge = self.ir.edges.len();
        self.fragment_stack
            .push((kind, label, start_edge, Vec::new(), Vec::new()));
        if let Some((stored_kind, stored_label, _, _, _)) = self.fragment_stack.last_mut()
            && *stored_kind == FragmentKind::Rect
            && let Some(color) = color
        {
            *stored_label = color;
        }
    }

    pub(crate) fn add_fragment_alternative(&mut self, label: String) {
        if let Some((_, _, _, alternatives, _)) = self.fragment_stack.last_mut() {
            let start_edge = self.ir.edges.len();
            // Close the previous section's end_edge
            if let Some(last_alt) = alternatives.last_mut() {
                last_alt.end_edge = start_edge.saturating_sub(1);
            }
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
            label: if kind == FragmentKind::Rect {
                String::new()
            } else {
                label.clone()
            },
            color: (kind == FragmentKind::Rect).then_some(label),
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

    /// Mutable access to the IR for direct field manipulation.
    pub(crate) const fn ir_mut(&mut self) -> &mut MermaidDiagramIr {
        &mut self.ir
    }

    pub(crate) const fn node_count(&self) -> usize {
        self.ir.nodes.len()
    }

    pub(crate) const fn edge_count(&self) -> usize {
        self.ir.edges.len()
    }

    /// Look up a node ID by its string key (as used in the diagram source).
    pub(crate) fn node_id_by_key(&self, key: &str) -> Option<IrNodeId> {
        self.node_id_index.get(key, &self.ir.nodes)
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

        // Populate structured style types from raw style_refs.
        self.ir.populate_structured_styles();

        ParseResult {
            ir: self.ir,
            warnings: self.warnings,
            confidence,
            detection_method,
            format_complement: crate::MermaidFormatComplement::default(),
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
                    node_ids.first().map_or("", String::as_str)
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
                    "{unresolved_count} edge(s) have unresolved endpoints"
                ))
                .with_category(DiagnosticCategory::Semantic),
            );
        }
    }

    /// Intern a node, optionally marking it as auto-created (for recovery).
    fn intern_node_auto(
        &mut self,
        id: &str,
        label: Option<NodeLabelInput<'_>>,
        shape: NodeShape,
        span: Span,
        is_auto_created: bool,
    ) -> Option<IrNodeId> {
        // `trim_fast` == `str::trim` byte-for-byte (ASCII byte scan, Unicode fallback only when a
        // non-ASCII byte sits at a trimmed boundary) but skips the `char::is_whitespace` CharSearcher.
        // Normalize a possibly-untrimmed id, then delegate to the normalized core.
        self.intern_node_auto_normalized(trim_fast(id), label, shape, span, is_auto_created)
    }

    /// Core of [`Self::intern_node_auto`] taking an ALREADY-trimmed `normalized_id`. The flowchart
    /// fast paths (`parse_fast_simple_flowchart_node_borrowed` / `_edge_parts`) hand in ids that are
    /// already `trim_ascii`'d AND validated as pure-ASCII `is_fast_flow_identifier`s (no whitespace),
    /// so `trim_fast(id) == id` there — they intern through this directly to skip the redundant
    /// per-intern trim (~2400 interns per flowchart/800 parse).
    fn intern_node_auto_normalized(
        &mut self,
        normalized_id: &str,
        label: Option<NodeLabelInput<'_>>,
        shape: NodeShape,
        span: Span,
        is_auto_created: bool,
    ) -> Option<IrNodeId> {
        if normalized_id.is_empty() {
            self.add_warning("Encountered empty node identifier; skipped node");
            return None;
        }

        // Hash the id ONCE for the get+insert pair below (a new node was hashed twice: once here
        // and again in the insert on the create path). Byte-identical; monotonically fewer hashes.
        let id_hash = NodeIdIndex::hash_key(normalized_id);

        // Check if already exists
        if let Some(existing_id) =
            self.node_id_index
                .get_with_hash(id_hash, normalized_id, &self.ir.nodes)
        {
            let resolved_label = if self
                .ir
                .nodes
                .get(existing_id.0)
                .and_then(|node| node.label)
                .is_none()
            {
                label.map(|value| self.intern_node_label_input(value, span))
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

                // `span_all` is write-only dead data (no workspace reader); do not accumulate
                // a `Span` per node reference. See the node-construction site.

                // If this call is NOT auto-created but the existing node IS,
                // "upgrade" it to an explicit node and remove from tracking.
                if !is_auto_created && existing_node.implicit {
                    existing_node.implicit = false;
                    self.auto_created_nodes.retain(|&id| id != existing_id);
                }
            }
            return Some(existing_id);
        }

        // Create new node
        let label_id = label.map(|value| self.intern_node_label_input(value, span));
        let node_id = IrNodeId(self.ir.nodes.len());
        let node = IrNode {
            id: normalized_id.to_string(),
            label: label_id,
            shape,
            classes: Vec::new(),
            interaction: None,
            span_primary: span,
            implicit: is_auto_created,
            members: Vec::new(),
            menu_links: Vec::new(),
            class_meta: None,
            requirement_meta: None,
            c4_meta: None,
            inline_style: None,
        };

        self.ir.nodes.push(node);
        self.ir.graph.nodes.push(IrGraphNode {
            node_id,
            kind: self.node_kind(),
            clusters: Vec::new(),
            subgraphs: Vec::new(),
        });
        self.node_id_index.insert_with_hash(id_hash, node_id);

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

        // Reserve the `add_node_to_cluster` dedup set once, when the FIRST cluster is created — it fills
        // to ~node-count as members accumulate, so this skips the geometric `reserve_rehash` (~5.8% of
        // section-heavy parse: timeline −0.98%, journey −1.19%). Done here (per-section, cold) rather than
        // in the per-node `add_node_to_cluster` so the flowchart node hot path is byte-for-byte unchanged
        // (moving it into the hot path regressed flowchart +0.11% via inlining), and a subgraph-free diagram
        // never reaches here so pays no unused-map allocation. Capacity-only ⇒ behavior-identical.
        if self.cluster_member_set.capacity() == 0 {
            self.cluster_member_set.reserve(self.ir.nodes.capacity().max(4));
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
                    let label = ParsedLabel::plain(title_text);
                    let label_id = self.intern_label(&label, span);
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

        let title_label = clean_label(title).map(ParsedLabel::plain);
        let title_id = title_label
            .as_ref()
            .map(|value| self.intern_label(value, span));
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
        // O(1) dedup via `cluster_member_set` instead of the O(members) `contains` scans (this was
        // O(subgraph²) on big clusters). `ir.clusters[i].members` and `ir.graph.clusters[i].members`
        // are created together and only appended here in lockstep, so one set key gates both — the
        // set is empty exactly when both Vecs lack `node_id`. Byte-identical.
        if self.cluster_member_set.insert((cluster_index, node_id)) {
            cluster.members.push(node_id);
            if let Some(graph_cluster) = self.ir.graph.clusters.get_mut(cluster_index) {
                graph_cluster.members.push(node_id);
            }
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

        // See `ensure_cluster`: one-time member-set reserve on first subgraph, off the per-node hot path.
        if self.subgraph_member_set.capacity() == 0 {
            self.subgraph_member_set.reserve(self.ir.nodes.capacity().max(4));
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
                    let label = ParsedLabel::plain(title_text);
                    let label_id = self.intern_label(&label, span);
                    if let Some(subgraph) = self.ir.graph.subgraphs.get_mut(existing_index) {
                        subgraph.title = Some(label_id);
                    }
                }
            }
            return Some(existing_index);
        }

        let title_label = clean_label(title).map(ParsedLabel::plain);
        let title_id = title_label
            .as_ref()
            .map(|value| self.intern_label(value, span));
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
            direction: None,
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
        // O(1) dedup (was O(subgraph²) — see `add_node_to_cluster`). `subgraph_member_set` mirrors
        // `subgraph.members` exactly (empty start, appended only here). Byte-identical.
        if self.subgraph_member_set.insert((subgraph_index, node_id)) {
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

    pub(crate) fn intern_node_label(
        &mut self,
        id: &str,
        label: Option<&ParsedLabel>,
        shape: NodeShape,
        span: Span,
    ) -> Option<IrNodeId> {
        self.intern_node_auto(id, label.map(NodeLabelInput::Parsed), shape, span, false)
    }

    /// Intern a flowchart fast-path edge endpoint (label-less Rect node) whose id is already
    /// `trim_ascii`'d and `is_fast_flow_identifier`-validated (pure ASCII, no whitespace) — so
    /// `trim_fast(id) == id`. Interns through the normalized core to skip that redundant trim.
    pub(crate) fn intern_edge_endpoint_pretrimmed(
        &mut self,
        id: &str,
        span: Span,
    ) -> Option<IrNodeId> {
        self.intern_node_auto_normalized(id, None, NodeShape::Rect, span, false)
    }

    /// Like [`Self::intern_node_label`] but consumes an owned label, moving it into the IR instead of
    /// cloning (see [`Self::intern_label_owned`]). For the flowchart lowering pass's `FastNode`, whose
    /// id is already `trim_ascii`'d + `is_fast_flow_identifier`-validated — so intern through the
    /// normalized core to skip the redundant `trim_fast`.
    pub(crate) fn intern_node_label_owned(
        &mut self,
        id: &str,
        label: Option<ParsedLabel>,
        shape: NodeShape,
        span: Span,
    ) -> Option<IrNodeId> {
        self.intern_node_auto_normalized(id, label.map(NodeLabelInput::ParsedOwned), shape, span, false)
    }

    pub(crate) fn intern_node(
        &mut self,
        id: &str,
        label: Option<&str>,
        shape: NodeShape,
        span: Span,
    ) -> Option<IrNodeId> {
        self.intern_node_auto(id, label.map(NodeLabelInput::Plain), shape, span, false)
    }

    /// Intern a generated node whose id is known fresh by the caller, consuming the
    /// owned id and plain label instead of cloning them through the generic path.
    pub(crate) fn intern_fresh_node_owned_label(
        &mut self,
        id: String,
        label: String,
        shape: NodeShape,
        span: Span,
    ) -> Option<IrNodeId> {
        // Byte-exact `trim_fast` for the same reason as `intern_node_auto` — normalizes the owned
        // generated id without the Unicode `char::is_whitespace` CharSearcher. Byte-identical.
        let normalized_id = trim_fast(&id);
        if normalized_id.is_empty() {
            self.add_warning("Encountered empty node identifier; skipped node");
            return None;
        }
        if normalized_id.len() != id.len() {
            return self.intern_node(normalized_id, Some(&label), shape, span);
        }

        let id_hash = NodeIdIndex::hash_key(&id);
        if let Some(existing_id) = self
            .node_id_index
            .get_with_hash(id_hash, &id, &self.ir.nodes)
        {
            let resolved_label = if self
                .ir
                .nodes
                .get(existing_id.0)
                .and_then(|node| node.label)
                .is_none()
            {
                Some(self.intern_plain_label_owned(label, span))
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
                if existing_node.implicit {
                    existing_node.implicit = false;
                    self.auto_created_nodes.retain(|&id| id != existing_id);
                }
            }
            return Some(existing_id);
        }

        let label_id = self.intern_plain_label_owned(label, span);
        let node_id = IrNodeId(self.ir.nodes.len());
        self.ir.nodes.push(IrNode {
            id,
            label: Some(label_id),
            shape,
            classes: Vec::new(),
            interaction: None,
            span_primary: span,
            implicit: false,
            members: Vec::new(),
            menu_links: Vec::new(),
            class_meta: None,
            requirement_meta: None,
            c4_meta: None,
            inline_style: None,
        });
        self.ir.graph.nodes.push(IrGraphNode {
            node_id,
            kind: self.node_kind(),
            clusters: Vec::new(),
            subgraphs: Vec::new(),
        });
        self.node_id_index.insert_with_hash(id_hash, node_id);
        Some(node_id)
    }

    /// Intern a node as a placeholder (auto-created for dangling edge recovery).
    #[allow(dead_code)] // Will be used by recovery features
    pub(crate) fn intern_placeholder_node(&mut self, id: &str, span: Span) -> Option<IrNodeId> {
        let label = ParsedLabel::plain(id);
        self.intern_node_auto(
            id,
            Some(NodeLabelInput::Parsed(&label)),
            NodeShape::Rect,
            span,
            true,
        )
    }

    pub(crate) fn add_class_to_node(&mut self, node_key: &str, class_name: &str, span: Span) {
        let normalized_class = trim_fast(class_name);
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

    pub(crate) fn add_class_to_node_id(&mut self, node_id: IrNodeId, class_name: &str) {
        let normalized_class = trim_fast(class_name);
        if normalized_class.is_empty() {
            return;
        }

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

    pub(crate) fn set_node_icon(&mut self, node_id: IrNodeId, icon: &str) {
        let icon = icon.trim();
        if icon.is_empty() {
            return;
        }
        if let Some(node) = self.ir.nodes.get_mut(node_id.0) {
            node.interaction_mut().icon = Some(icon.to_string());
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
            node.interaction_mut().href = Some(target.to_string());
        }
    }

    pub(crate) fn set_node_callback(&mut self, node_key: &str, callback: &str, span: Span) {
        let callback = callback.trim();
        if callback.is_empty() {
            return;
        }

        let Some(node_id) = self.intern_node(node_key, None, NodeShape::Rect, span) else {
            return;
        };

        if let Some(node) = self.ir.nodes.get_mut(node_id.0) {
            node.interaction_mut().callback = Some(callback.to_string());
        }
    }

    pub(crate) fn set_node_tooltip(&mut self, node_key: &str, tooltip: &str, span: Span) {
        let Some(node_id) = self.intern_node(node_key, None, NodeShape::Rect, span) else {
            return;
        };
        if let Some(node) = self.ir.nodes.get_mut(node_id.0) {
            node.interaction_mut().tooltip = Some(tooltip.to_string());
        }
    }

    pub(crate) fn add_node_menu_link(
        &mut self,
        node_key: &str,
        label: &str,
        url: &str,
        span: Span,
    ) {
        let Some(node_id) = self.intern_node(node_key, None, NodeShape::Rect, span) else {
            return;
        };
        let Some(node) = self.ir.nodes.get_mut(node_id.0) else {
            return;
        };
        if node
            .menu_links
            .iter()
            .any(|entry| entry.label == label && entry.url == url)
        {
            return;
        }
        node.menu_links.push(fm_core::IrMenuLink {
            label: label.to_string(),
            url: url.to_string(),
        });
    }

    pub(crate) fn node_mut(&mut self, node_id: IrNodeId) -> Option<&mut fm_core::IrNode> {
        self.ir.nodes.get_mut(node_id.0)
    }

    pub(crate) fn set_c4_node_meta(&mut self, node_id: IrNodeId, meta: IrC4NodeMeta) {
        let Some(node) = self.ir.nodes.get_mut(node_id.0) else {
            return;
        };
        node.c4_meta = Some(Box::new(meta));
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
            comment: comment.map(std::string::ToString::to_string),
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
        let parsed_label = clean_label(label).map(ParsedLabel::plain);
        let label_id = parsed_label
            .as_ref()
            .map(|value| self.intern_label(value, span));
        self.ir.edges.push(IrEdge {
            from: IrEndpoint::Node(from),
            to: IrEndpoint::Node(to),
            arrow,
            label: label_id,
            span,
            extras: None,
            inline_style: None,
        });
        self.ir.graph.edges.push(IrGraphEdge {
            edge_id: self.ir.edges.len() - 1,
            kind: self.edge_kind(),
            from: IrEndpoint::Node(from),
            to: IrEndpoint::Node(to),
            span,
        });
    }

    /// Set the ER cardinality notation on the last-pushed edge.
    pub(crate) fn set_last_edge_er_notation(&mut self, notation: &str) {
        if let Some(edge) = self.ir.edges.last_mut() {
            edge.extras_mut().er_notation = Some(Box::from(notation));
        }
    }

    /// Set cardinality labels on the most recently pushed edge.
    pub(crate) fn set_last_edge_cardinality(&mut self, source: Option<&str>, target: Option<&str>) {
        if let Some(edge) = self.ir.edges.last_mut() {
            if let Some(s) = source {
                edge.extras_mut().source_cardinality = Some(Box::from(s));
            }
            if let Some(t) = target {
                edge.extras_mut().target_cardinality = Some(Box::from(t));
            }
        }
    }

    fn intern_label(&mut self, label: &ParsedLabel, span: Span) -> IrLabelId {
        // Hash the (text, segments) pair ONCE for the get+insert pair below (a new label was
        // hashed twice). Byte-identical; monotonically fewer hashes.
        let label_hash = LabelIndex::hash_key(&label.text, &label.segments);
        if let Some(existing_id) = self.label_index.get_with_hash(
            label_hash,
            &label.text,
            &label.segments,
            &self.ir.labels,
            &self.ir.label_markup,
        ) {
            return existing_id;
        }

        let label_id = IrLabelId(self.ir.labels.len());
        self.ir.labels.push(IrLabel {
            text: label.text.clone(),
            span,
        });
        if !label.segments.is_empty() {
            self.ir
                .label_markup
                .insert(label_id, label.segments.clone());
        }
        self.label_index.insert_with_hash(label_hash, label_id);
        label_id
    }

    fn intern_node_label_input(&mut self, label: NodeLabelInput<'_>, span: Span) -> IrLabelId {
        match label {
            NodeLabelInput::Parsed(label) => self.intern_label(label, span),
            NodeLabelInput::Plain(text) => self.intern_plain_label(text, span),
            NodeLabelInput::ParsedOwned(label) => self.intern_label_owned(label, span),
        }
    }

    /// Owned-label variant of [`Self::intern_label`]: consumes the `ParsedLabel` and MOVES its text
    /// and segments into the IR on the create path instead of cloning them. Byte-identical to
    /// `intern_label` (same hash, same dedup, same insertion order); on a dedup hit the owned label is
    /// dropped — exactly what happens to the borrowed form's owner. Lets the flowchart lowering pass
    /// hand its owned `FlowDocumentItem::FastNode` label straight in, avoiding a `String` clone (and
    /// that clone's later free when the document `Vec` drops) per distinct node label.
    fn intern_label_owned(&mut self, label: ParsedLabel, span: Span) -> IrLabelId {
        let label_hash = LabelIndex::hash_key(&label.text, &label.segments);
        if let Some(existing_id) = self.label_index.get_with_hash(
            label_hash,
            &label.text,
            &label.segments,
            &self.ir.labels,
            &self.ir.label_markup,
        ) {
            return existing_id;
        }

        let label_id = IrLabelId(self.ir.labels.len());
        let ParsedLabel { text, segments } = label;
        let has_segments = !segments.is_empty();
        self.ir.labels.push(IrLabel { text, span });
        if has_segments {
            self.ir.label_markup.insert(label_id, segments);
        }
        self.label_index.insert_with_hash(label_hash, label_id);
        label_id
    }

    fn intern_plain_label(&mut self, text: &str, span: Span) -> IrLabelId {
        let label_hash = LabelIndex::hash_key(text, &[]);
        if let Some(existing_id) = self.label_index.get_with_hash(
            label_hash,
            text,
            &[],
            &self.ir.labels,
            &self.ir.label_markup,
        ) {
            return existing_id;
        }

        let label_id = IrLabelId(self.ir.labels.len());
        self.ir.labels.push(IrLabel {
            text: text.to_owned(),
            span,
        });
        self.label_index.insert_with_hash(label_hash, label_id);
        label_id
    }

    fn intern_plain_label_owned(&mut self, text: String, span: Span) -> IrLabelId {
        let label_hash = LabelIndex::hash_key(&text, &[]);
        if let Some(existing_id) = self.label_index.get_with_hash(
            label_hash,
            &text,
            &[],
            &self.ir.labels,
            &self.ir.label_markup,
        ) {
            return existing_id;
        }

        let label_id = IrLabelId(self.ir.labels.len());
        self.ir.labels.push(IrLabel { text, span });
        self.label_index.insert_with_hash(label_hash, label_id);
        label_id
    }
}

impl IrBuilder {
    const fn node_kind(&self) -> IrNodeKind {
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

    const fn edge_kind(&self) -> IrEdgeKind {
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

        let node = builder.ir.nodes.get(first.0).expect("node should exist");
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

    #[test]
    fn hide_sequence_footbox_sets_sequence_meta_flag() {
        let mut builder = IrBuilder::new(DiagramType::Sequence);

        builder.hide_sequence_footbox();

        let result = builder.finish(1.0, crate::DetectionMethod::ExactKeyword);
        assert!(
            result
                .ir
                .sequence_meta
                .expect("sequence metadata should exist")
                .hide_footbox
        );
    }

    #[test]
    fn enable_autonumber_with_sets_sequence_numbering_parameters() {
        let mut builder = IrBuilder::new(DiagramType::Sequence);

        builder.enable_autonumber_with(10, 5);

        let meta = builder
            .ir
            .sequence_meta
            .expect("sequence_meta should be set");
        assert!(meta.autonumber);
        assert_eq!(meta.autonumber_start, 10);
        assert_eq!(meta.autonumber_increment, 5);
    }
}
