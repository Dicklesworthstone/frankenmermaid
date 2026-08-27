use std::collections::BTreeMap;
use std::collections::hash_map::Entry;

use rustc_hash::{FxHashMap, FxHashSet};

use fm_core::{
    ArchitectureSide, ArrowType, C4RelationshipDirection, ClassMemberKind, ClassStereotype,
    Diagnostic, DiagnosticCategory, DiagramType, EdgeAnimation, FragmentAlternative, FragmentKind,
    GraphDirection, IrActivation, IrAttributeKey, IrC4NodeMeta, IrClassMember, IrClassNodeMeta,
    IrCluster, IrClusterId, IrConstraint, IrEdge, IrEdgeKind, IrEndpoint, IrEntityAttribute,
    IrGanttMeta, IrGraphCluster, IrGraphEdge, IrGraphNode, IrLabel, IrLabelId, IrLabelSegment,
    IrLifecycleEvent, IrNode, IrNodeId, IrNodeKind, IrParticipantGroup, IrSequenceAutonumberRange,
    IrSequenceFragment, IrSequenceMeta, IrSequenceNote, IrStyleRef, IrStyleTarget, IrSubgraph,
    IrSubgraphId, IrXyChartMeta, LifecycleEventKind, MermaidDiagramIr, MermaidError,
    MermaidParseMode, MermaidSanitizeMode, MermaidWarning, MermaidWarningCode, NodeShape,
    NotePosition, Span,
};

use crate::mermaid_parser::trim_fast;
use crate::{ParseResult, ParserConfig, normalize_identifier, normalize_identifier_cow};

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
#[derive(Clone, Default)]
struct NodeIdIndex {
    buckets: FxHashMap<u64, NodeIdBucket>,
}

#[derive(Clone)]
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
#[derive(Clone, Default)]
struct LabelIndex {
    buckets: FxHashMap<u64, LabelBucket>,
}

#[derive(Clone)]
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

#[derive(Clone)]
pub struct IrBuilder {
    ir: MermaidDiagramIr,
    // Lookups for uniqueness. These are read by key only (never iterated), so a hash
    // map is both faster and determinism-safe — IR output order comes from the `ir`
    // vectors, not from map iteration.
    node_id_index: NodeIdIndex,
    /// Mermaid edge IDs are lookup keys for directives such as `class edgeId alert`.
    /// Store the first match because Mermaid's `setClass` uses `edges.find(...)`.
    edge_index_by_id: FxHashMap<String, usize>,
    cluster_index_by_key: FxHashMap<String, usize>,
    subgraph_index_by_key: FxHashMap<String, usize>,
    /// Flowchart only: subgraph public id -> the node id an endpoint naming it resolves to, for
    /// subgraphs declared LATER in the document than the edge naming them (bd-dw2a9). Empty for
    /// every other diagram type, so their paths pay one hash miss.
    flow_forward_subgraph_members: FxHashMap<String, String>,
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
    reusable_prefix_guard: Option<ReusablePrefixGuard>,
}

#[derive(Clone, Copy)]
struct ReusablePrefixGuard {
    node_count: usize,
    edge_count: usize,
    cluster_count: usize,
    subgraph_count: usize,
    unchanged: bool,
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

fn clone_vec_reusing<T: Clone>(
    target: &mut Vec<T>,
    source: &[T],
    mut clone_one: impl FnMut(&mut T, &T),
) {
    let shared_len = target.len().min(source.len());
    for index in 0..shared_len {
        clone_one(&mut target[index], &source[index]);
    }
    if target.len() > source.len() {
        target.truncate(source.len());
    } else {
        target.extend(source[shared_len..].iter().cloned());
    }
}

fn clone_node_reusing(target: &mut IrNode, source: &IrNode) {
    target.id.clone_from(&source.id);
    target.label = source.label;
    target.shape = source.shape;
    target.classes.clone_from(&source.classes);
    target.interaction.clone_from(&source.interaction);
    target.menu_links.clone_from(&source.menu_links);
    target.span_primary = source.span_primary;
    target.implicit = source.implicit;
    target.members.clone_from(&source.members);
    target.class_meta.clone_from(&source.class_meta);
    target.requirement_meta.clone_from(&source.requirement_meta);
    target.c4_meta.clone_from(&source.c4_meta);
    target.inline_style.clone_from(&source.inline_style);
}

fn clone_label_reusing(target: &mut IrLabel, source: &IrLabel) {
    target.text.clone_from(&source.text);
    target.span = source.span;
}

fn clone_cluster_reusing(target: &mut IrCluster, source: &IrCluster) {
    target.id = source.id;
    target.title = source.title;
    target.members.clone_from(&source.members);
    target.grid_span = source.grid_span;
    target.span = source.span;
}

fn clone_graph_node_reusing(target: &mut IrGraphNode, source: &IrGraphNode) {
    target.node_id = source.node_id;
    target.kind = source.kind;
    target.clusters.clone_from(&source.clusters);
    target.subgraphs.clone_from(&source.subgraphs);
}

fn clone_graph_cluster_reusing(target: &mut IrGraphCluster, source: &IrGraphCluster) {
    target.cluster_id = source.cluster_id;
    target.title = source.title;
    target.members.clone_from(&source.members);
    target.subgraph = source.subgraph;
    target.grid_span = source.grid_span;
    target.span = source.span;
}

fn clone_subgraph_reusing(target: &mut IrSubgraph, source: &IrSubgraph) {
    target.id = source.id;
    target.key.clone_from(&source.key);
    target.title = source.title;
    target.parent = source.parent;
    target.children.clone_from(&source.children);
    target.members.clone_from(&source.members);
    target.cluster = source.cluster;
    target.grid_span = source.grid_span;
    target.span = source.span;
    target.direction = source.direction;
}

fn clone_ir_reusing(target: &mut MermaidDiagramIr, source: &MermaidDiagramIr) {
    target.diagram_type = source.diagram_type;
    target.direction = source.direction;
    clone_vec_reusing(&mut target.nodes, &source.nodes, clone_node_reusing);
    target.edges.clone_from(&source.edges);
    target.ports.clone_from(&source.ports);
    clone_vec_reusing(
        &mut target.clusters,
        &source.clusters,
        clone_cluster_reusing,
    );
    clone_vec_reusing(
        &mut target.graph.nodes,
        &source.graph.nodes,
        clone_graph_node_reusing,
    );
    target.graph.edges.clone_from(&source.graph.edges);
    clone_vec_reusing(
        &mut target.graph.clusters,
        &source.graph.clusters,
        clone_graph_cluster_reusing,
    );
    clone_vec_reusing(
        &mut target.graph.subgraphs,
        &source.graph.subgraphs,
        clone_subgraph_reusing,
    );
    clone_vec_reusing(&mut target.labels, &source.labels, clone_label_reusing);
    target.label_markup.clone_from(&source.label_markup);
    target.constraints.clone_from(&source.constraints);
    target.style_refs.clone_from(&source.style_refs);
    target.style_defs.clone_from(&source.style_defs);
    target.meta.clone_from(&source.meta);
    target.sequence_meta.clone_from(&source.sequence_meta);
    target.gantt_meta.clone_from(&source.gantt_meta);
    target.xy_chart_meta.clone_from(&source.xy_chart_meta);
    target.pie_meta.clone_from(&source.pie_meta);
    target.quadrant_meta.clone_from(&source.quadrant_meta);
    target.packet_meta.clone_from(&source.packet_meta);
    target.git_graph_meta.clone_from(&source.git_graph_meta);
    target.state_notes.clone_from(&source.state_notes);
    target.diagnostics.clone_from(&source.diagnostics);
}

impl IrBuilder {
    pub(crate) fn begin_reusable_suffix(&mut self, source: &Self) {
        self.reusable_prefix_guard = Some(ReusablePrefixGuard {
            node_count: source.ir.nodes.len(),
            edge_count: source.ir.edges.len(),
            cluster_count: source.ir.clusters.len(),
            subgraph_count: source.ir.graph.subgraphs.len(),
            unchanged: true,
        });
    }

    #[inline]
    fn mark_reusable_prefix_dirty(&mut self) {
        if let Some(guard) = self.reusable_prefix_guard.as_mut() {
            guard.unchanged = false;
        }
    }

    #[inline]
    fn mark_reusable_prefix_node_dirty(&mut self, node_id: IrNodeId) {
        if let Some(guard) = self.reusable_prefix_guard.as_mut()
            && node_id.0 < guard.node_count
        {
            guard.unchanged = false;
        }
    }

    #[inline]
    fn mark_reusable_prefix_edge_dirty(&mut self, edge_index: usize) {
        if let Some(guard) = self.reusable_prefix_guard.as_mut()
            && edge_index < guard.edge_count
        {
            guard.unchanged = false;
        }
    }

    #[inline]
    fn mark_reusable_prefix_cluster_dirty(&mut self, cluster_index: usize) {
        if let Some(guard) = self.reusable_prefix_guard.as_mut()
            && cluster_index < guard.cluster_count
        {
            guard.unchanged = false;
        }
    }

    #[inline]
    fn mark_reusable_prefix_subgraph_dirty(&mut self, subgraph_index: usize) {
        if let Some(guard) = self.reusable_prefix_guard.as_mut()
            && subgraph_index < guard.subgraph_count
        {
            guard.unchanged = false;
        }
    }

    /// Restore a builder whose last suffix left the compiled prefix byte-for-byte unchanged.
    ///
    /// [`Self::reusable_prefix_unchanged`] is the proof obligation for this path. Once it succeeds,
    /// every large IR sequence starts with the immutable compiled snapshot, so restoring the slot
    /// only needs to discard the appended suffix. The lookup indexes are still refreshed from the
    /// snapshot because hash collisions can mix prefix and suffix IDs in one bucket; those maps are
    /// small compared with cloning every prefix node, edge, label, and nested membership vector.
    pub(crate) fn reset_reusable_suffix_from(&mut self, source: &Self) {
        debug_assert!(self.reusable_prefix_unchanged(source));

        self.ir.nodes.truncate(source.ir.nodes.len());
        self.ir.edges.truncate(source.ir.edges.len());
        self.ir.clusters.truncate(source.ir.clusters.len());
        self.ir.graph.nodes.truncate(source.ir.graph.nodes.len());
        self.ir.graph.edges.truncate(source.ir.graph.edges.len());
        self.ir
            .graph
            .clusters
            .truncate(source.ir.graph.clusters.len());
        self.ir
            .graph
            .subgraphs
            .truncate(source.ir.graph.subgraphs.len());
        self.ir.labels.truncate(source.ir.labels.len());
        self.ir
            .label_markup
            .retain(|label, _| label.0 < source.ir.labels.len());

        // These fields are outside the certified flowchart-prefix equality surface. The admitted
        // flowchart subset leaves them empty, but copying the snapshot here keeps the reset exact if
        // that subset grows later without putting the large graph vectors back on the copy path.
        self.ir.ports.clone_from(&source.ir.ports);
        self.ir.constraints.clone_from(&source.ir.constraints);
        self.ir.sequence_meta.clone_from(&source.ir.sequence_meta);
        self.ir.gantt_meta.clone_from(&source.ir.gantt_meta);
        self.ir.xy_chart_meta.clone_from(&source.ir.xy_chart_meta);
        self.ir.pie_meta.clone_from(&source.ir.pie_meta);
        self.ir.quadrant_meta.clone_from(&source.ir.quadrant_meta);
        self.ir.packet_meta.clone_from(&source.ir.packet_meta);
        self.ir.git_graph_meta.clone_from(&source.ir.git_graph_meta);
        self.ir.state_notes.clone_from(&source.ir.state_notes);
        self.ir.diagnostics.clone_from(&source.ir.diagnostics);

        self.node_id_index
            .buckets
            .clone_from(&source.node_id_index.buckets);
        self.edge_index_by_id.clone_from(&source.edge_index_by_id);
        self.cluster_index_by_key
            .clone_from(&source.cluster_index_by_key);
        self.subgraph_index_by_key
            .clone_from(&source.subgraph_index_by_key);
        self.cluster_member_set
            .clone_from(&source.cluster_member_set);
        self.subgraph_member_set
            .clone_from(&source.subgraph_member_set);
        self.label_index
            .buckets
            .clone_from(&source.label_index.buckets);
        self.warnings.clone_from(&source.warnings);
        self.auto_created_nodes
            .clone_from(&source.auto_created_nodes);
        self.activation_stacks.clone_from(&source.activation_stacks);
        self.current_participant_group
            .clone_from(&source.current_participant_group);
        self.fragment_stack.clone_from(&source.fragment_stack);
        self.current_class_node_id = source.current_class_node_id;
        self.state_stack.clone_from(&source.state_stack);
        self.parser_config = source.parser_config;
        self.reusable_prefix_guard = None;
    }

    pub(crate) fn reset_from(&mut self, source: &Self) {
        clone_ir_reusing(&mut self.ir, &source.ir);
        self.node_id_index
            .buckets
            .clone_from(&source.node_id_index.buckets);
        self.edge_index_by_id.clone_from(&source.edge_index_by_id);
        self.cluster_index_by_key
            .clone_from(&source.cluster_index_by_key);
        self.subgraph_index_by_key
            .clone_from(&source.subgraph_index_by_key);
        self.cluster_member_set
            .clone_from(&source.cluster_member_set);
        self.subgraph_member_set
            .clone_from(&source.subgraph_member_set);
        self.label_index
            .buckets
            .clone_from(&source.label_index.buckets);
        self.warnings.clone_from(&source.warnings);
        self.auto_created_nodes
            .clone_from(&source.auto_created_nodes);
        self.activation_stacks.clone_from(&source.activation_stacks);
        self.current_participant_group
            .clone_from(&source.current_participant_group);
        self.fragment_stack.clone_from(&source.fragment_stack);
        self.current_class_node_id = source.current_class_node_id;
        self.state_stack.clone_from(&source.state_stack);
        self.parser_config = source.parser_config;
        self.reusable_prefix_guard = None;
    }

    pub(crate) fn new(diagram_type: DiagramType) -> Self {
        Self {
            ir: MermaidDiagramIr::empty(diagram_type),
            node_id_index: NodeIdIndex::default(),
            edge_index_by_id: FxHashMap::default(),
            cluster_index_by_key: FxHashMap::default(),
            subgraph_index_by_key: FxHashMap::default(),
            flow_forward_subgraph_members: FxHashMap::default(),
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
            reusable_prefix_guard: None,
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
            edge_index_by_id: FxHashMap::default(),
            cluster_index_by_key: FxHashMap::default(),
            subgraph_index_by_key: FxHashMap::default(),
            flow_forward_subgraph_members: FxHashMap::default(),
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
            reusable_prefix_guard: None,
        }
    }

    pub(crate) fn set_direction(&mut self, direction: GraphDirection) {
        if self.ir.direction != direction || self.ir.meta.direction != direction {
            self.mark_reusable_prefix_dirty();
        }
        self.ir.direction = direction;
        self.ir.meta.direction = direction;
    }

    pub(crate) fn set_subgraph_direction(
        &mut self,
        subgraph_index: usize,
        direction: GraphDirection,
    ) {
        let changes_prefix = self
            .ir
            .graph
            .subgraphs
            .get(subgraph_index)
            .is_some_and(|subgraph| subgraph.direction != Some(direction));
        if changes_prefix {
            self.mark_reusable_prefix_subgraph_dirty(subgraph_index);
        }
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

    pub(crate) fn set_packet_meta(&mut self, packet_meta: fm_core::IrPacketMeta) {
        self.ir.packet_meta = Some(packet_meta);
    }

    pub(crate) fn set_git_graph_meta(&mut self, git_graph_meta: fm_core::IrGitGraphMeta) {
        self.ir.git_graph_meta = Some(git_graph_meta);
    }

    pub(crate) fn set_acc_title(&mut self, title: String) {
        self.ir.meta.acc_title = Some(title);
    }

    pub(crate) fn set_title(&mut self, title: String) {
        self.ir.meta.title = Some(title);
    }

    /// Whether no diagram title has been recorded yet.
    ///
    /// The post-parse generic title extractor uses this so it can never clobber a title a
    /// type-specific parser already set (journey, timeline, gantt, pie, quadrant, xychart all call
    /// `set_title` themselves, several also storing it in their own meta).
    pub(crate) const fn title_is_unset(&self) -> bool {
        self.ir.meta.title.is_none()
    }

    /// Record how the source asked for edges to be routed.
    pub(crate) const fn set_edge_routing_hint(&mut self, hint: fm_core::MermaidEdgeRoutingHint) {
        self.ir.meta.edge_routing = Some(hint);
    }

    /// Record a source-level minimum in-rank node gap, in layout units.
    pub(crate) const fn set_node_spacing(&mut self, units: u32) {
        self.ir.meta.node_spacing = Some(units);
    }

    /// Record a source-level minimum rank gap, in layout units.
    pub(crate) const fn set_rank_spacing(&mut self, units: u32) {
        self.ir.meta.rank_spacing = Some(units);
    }

    /// Apply `flowchart.nodeSpacing` from an init directive: to the layout hint AND to the init
    /// record, so `parse --json` still reports the directive that was given.
    pub(crate) const fn set_init_node_spacing(&mut self, units: u32) {
        self.ir.meta.init.config.node_spacing = Some(units);
        self.ir.meta.node_spacing = Some(units);
    }

    /// Apply `flowchart.rankSpacing` from an init directive. See [`Self::set_init_node_spacing`].
    pub(crate) const fn set_init_rank_spacing(&mut self, units: u32) {
        self.ir.meta.init.config.rank_spacing = Some(units);
        self.ir.meta.rank_spacing = Some(units);
    }

    /// Set an already-created cluster's title, for syntaxes that name a group from INSIDE its body.
    ///
    /// DOT does exactly that (`subgraph cluster_0 { label="Backend"; … }`), so the cluster exists
    /// before its name is known and [`Self::ensure_cluster`]'s creation-time title cannot carry it.
    /// An explicit label statement overwrites any earlier title, because the attribute is the
    /// authoritative name of the group; `ensure_cluster` deliberately only fills a vacant one, which
    /// is right for re-opening a cluster but wrong here.
    pub(crate) fn set_cluster_title(&mut self, cluster_index: usize, title: &str, span: Span) {
        let Some(title_text) = clean_label(Some(title)) else {
            return;
        };
        let label = ParsedLabel::plain(title_text);
        let label_id = self.intern_label(&label, span);
        self.mark_reusable_prefix_cluster_dirty(cluster_index);
        if let Some(cluster) = self.ir.clusters.get_mut(cluster_index) {
            cluster.title = Some(label_id);
        }
        // The graph view mirrors clusters by the same index, so both must move together or a
        // renderer reading one would disagree with a renderer reading the other.
        if let Some(graph_cluster) = self.ir.graph.clusters.get_mut(cluster_index) {
            graph_cluster.title = Some(label_id);
        }
    }

    /// Set an already-created subgraph's title, for the same reason as [`Self::set_cluster_title`].
    pub(crate) fn set_subgraph_title(&mut self, subgraph_index: usize, title: &str, span: Span) {
        let Some(title_text) = clean_label(Some(title)) else {
            return;
        };
        let label = ParsedLabel::plain(title_text);
        let label_id = self.intern_label(&label, span);
        if let Some(subgraph) = self.ir.graph.subgraphs.get_mut(subgraph_index) {
            subgraph.title = Some(label_id);
        }
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

    pub(crate) const fn set_init_gantt_top_axis(&mut self, top_axis: bool) {
        self.ir.meta.init.config.gantt_top_axis = Some(top_axis);
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

    /// Turn sequence autonumbering OFF, as `autonumber off` does in mermaid.
    ///
    /// A distinct entry point rather than `enable_autonumber_with(0, 0)`: the start and increment are
    /// meaningless when numbering is off, and writing zeroes into them would make a later bare
    /// `autonumber` (which restores the default 1/1) indistinguishable from a corrupted state.
    pub(crate) fn disable_autonumber(&mut self) {
        let edge_index = self.ir.edges.len();
        let meta = self
            .ir
            .sequence_meta
            .get_or_insert_with(IrSequenceMeta::default);
        if let Some(range) = meta
            .autonumber_ranges
            .last_mut()
            .filter(|range| range.end_edge.is_none())
        {
            range.end_edge = Some(edge_index);
        }
        meta.autonumber = false;
    }

    pub(crate) fn enable_autonumber_with(&mut self, start: u32, increment: u32) {
        let edge_index = self.ir.edges.len();
        let meta = self
            .ir
            .sequence_meta
            .get_or_insert_with(IrSequenceMeta::default);
        if let Some(range) = meta
            .autonumber_ranges
            .last_mut()
            .filter(|range| range.end_edge.is_none())
        {
            range.end_edge = Some(edge_index);
        }
        meta.autonumber_ranges.push(IrSequenceAutonumberRange {
            start_edge: edge_index,
            end_edge: None,
            start,
            increment,
        });
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
                // Lookup only: the owned String was allocated purely to be borrowed here.
                let normalized = normalize_identifier_cow(name);
                self.node_id_index.get(normalized.as_ref(), &self.ir.nodes)
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
        // NOT converted to the borrowing form on purpose: `entry` below takes an OWNED key, so this
        // site genuinely needs a String and a Cow would only add an `into_owned()` for the same one
        // allocation. The borrowing form pays off where the value is a pure lookup and is dropped.
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
        // Lookup only. `.as_ref()` is required here, not merely tidy: HashMap::get_mut's
        // Borrow bound does not accept `&Cow<str>`.
        let normalized = normalize_identifier_cow(name);
        let Some(stack) = self.activation_stacks.get_mut(normalized.as_ref()) else {
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
        // Lookup only: the owned String was allocated purely to be borrowed here.
        let normalized = normalize_identifier_cow(name);
        let Some(node_id) = self.node_id_index.get(normalized.as_ref(), &self.ir.nodes) else {
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
        // Lookup only: the owned String was allocated purely to be borrowed here.
        let normalized = normalize_identifier_cow(name);
        let Some(node_id) = self.node_id_index.get(normalized.as_ref(), &self.ir.nodes) else {
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
        // FIRST ANNOTATION WINS (bd-dezf6). mermaid keeps every annotation in an array but its
        // class renderer draws only `annotations[0]`, so on `<<interface>> Foo` followed by
        // `<<abstract>> Foo` it shows `interface`. Overwriting here made us show `abstract` — the
        // wrong one of the two, not merely one of two.
        let meta = node
            .class_meta
            .get_or_insert_with(|| Box::new(IrClassNodeMeta::default()));
        if meta.stereotype.is_none() {
            meta.stereotype = Some(stereotype);
        }
    }

    /// Set the stereotype of the class block currently open.
    ///
    /// The in-block annotation spelling names no class, so it resolves through the same
    /// `current_class_node_id` that members already use rather than re-looking-up a name.
    pub(crate) fn set_current_class_stereotype(&mut self, stereotype: ClassStereotype) {
        let Some(node_id) = self.current_class_node_id else {
            return;
        };
        let Some(node) = self.ir.nodes.get_mut(node_id.0) else {
            return;
        };
        // First annotation wins here too (bd-dezf6) — the in-block spelling reaches the same field.
        let meta = node
            .class_meta
            .get_or_insert_with(|| Box::new(IrClassNodeMeta::default()));
        if meta.stereotype.is_none() {
            meta.stereotype = Some(stereotype);
        }
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

    /// Lookup key of the composite state currently being parsed, or `None` at the top level.
    ///
    /// `[*]` is a pseudo-state, and its identity is SCOPED: the `[*]` inside `state Processing { … }`
    /// is a different pseudo-state from the diagram's own. Interning both under one global id merged
    /// them into a single node that then emitted every start transition from both scopes (bd-w5j5).
    /// Nested composites need the full path, not just the innermost name, so this returns the same
    /// key `begin_state_cluster` builds its cluster from.
    pub(crate) fn state_scope_key(&self) -> Option<&str> {
        self.state_stack
            .last()
            .map(|context| context.lookup_key.as_str())
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

    /// Record a layout constraint for the constraint solver in `fm-layout`.
    ///
    /// Duplicates are dropped: DOT lets the same `{ rank=same; … }` group be written twice, and the
    /// solver counts applied constraints, so a repeat would inflate that count without changing the
    /// solution.
    pub(crate) fn add_constraint(&mut self, constraint: IrConstraint) {
        if !self.ir.constraints.contains(&constraint) {
            self.ir.constraints.push(constraint);
        }
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

    pub(crate) const fn ir(&self) -> &MermaidDiagramIr {
        &self.ir
    }

    pub(crate) fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub(crate) const fn node_count(&self) -> usize {
        self.ir.nodes.len()
    }

    /// The first edge running `from` -> `to`, if one exists (bd-ww46 follow-up).
    ///
    /// FIRST match, deliberately: mermaid's own `updateRelStyle` does
    /// `edges.find(e => e.from === t && e.to === r)` and styles that one, so a diagram with two
    /// relationships between the same pair styles the earlier one in both engines.
    ///
    /// Endpoints are compared through `matches!` rather than `==` so a `Port` endpoint can never
    /// alias a `Node` with the same index.
    pub(crate) fn edge_index_by_endpoints(&self, from: IrNodeId, to: IrNodeId) -> Option<usize> {
        self.ir.edges.iter().position(|edge| {
            matches!(edge.from, IrEndpoint::Node(id) if id == from)
                && matches!(edge.to, IrEndpoint::Node(id) if id == to)
        })
    }

    /// The first edge carrying Mermaid's source-level `edgeId@` prefix.
    pub(crate) fn edge_index_by_id(&self, edge_id: &str) -> Option<usize> {
        self.edge_index_by_id.get(edge_id).copied()
    }

    pub(crate) const fn edge_count(&self) -> usize {
        self.ir.edges.len()
    }

    /// Look up a node ID by its string key (as used in the diagram source).
    /// The CSS a `classDef` declared for `class_name`, joined in declaration order.
    ///
    /// Reads the SAME store `push_style_ref` writes, so a lookup cannot disagree with what was
    /// recorded. Joined with `;` rather than last-wins-by-key because that is what a CSS declaration
    /// block already means: a later property of the same name overrides an earlier one, and joining
    /// preserves that without this function having to reimplement the cascade.
    ///
    /// Returns `None` when no `classDef` of that name was seen, so a caller can tell "no such class"
    /// from "a class that declared nothing".
    pub(crate) fn class_style_css(&self, class_name: &str) -> Option<String> {
        let joined = self
            .ir
            .style_refs
            .iter()
            .filter(|style_ref| {
                matches!(&style_ref.target, IrStyleTarget::Class(name) if name == class_name)
            })
            .map(|style_ref| style_ref.style.as_str())
            .collect::<Vec<_>>()
            .join(";");
        (!joined.is_empty()).then_some(joined)
    }

    /// The cluster a `style`/`class` target names, if it names one (bd-xfmm).
    ///
    /// The same normalisation `ensure_cluster` applies when it INSERTS the key, so a lookup cannot
    /// miss an entry the insert created. Two different trim rules here would make a subgraph
    /// styleable or not depending on the author's whitespace.
    pub(crate) fn cluster_index_by_key(&self, key: &str) -> Option<usize> {
        let key = key.trim();
        if let Some(&index) = self.cluster_index_by_key.get(key) {
            return Some(index);
        }

        // ⚠️ A FLOWCHART SUBGRAPH IS NOT KEYED BY ITS ID, which is what made every subgraph-styling
        // test fail the moment they were first compiled. `flow_subgraph_lookup_key` builds a
        // COMPOSITE key — `subgraph one[One]` is stored as `one@title:One`, so two subgraphs sharing
        // an id but not a title stay distinct — and a `style one …` or `class one …` directive names
        // only `one`. The direct map lookup above can never hit for those.
        //
        // `IrSubgraph` already carries both halves: `key` is the RAW public id and `cluster` is the
        // cluster it created. So the id resolves without inventing a second index.
        //
        // Deliberately a SCAN rather than a new `FxHashMap` field. Two `clone_from` blocks in this
        // file copy every index map into the reusable-prefix builder; a new map means remembering
        // both, and forgetting one would be an incremental-parse bug that only shows up on a reused
        // prefix. The list is subgraph-sized (tens), and this runs once per style DIRECTIVE line,
        // never per node or per edge.
        self.ir
            .graph
            .subgraphs
            .iter()
            .find(|subgraph| subgraph.key == key)
            .and_then(|subgraph| subgraph.cluster)
            .map(|cluster| cluster.0)
    }

    pub(crate) fn node_id_by_key(&self, key: &str) -> Option<IrNodeId> {
        self.node_id_index.get(key, &self.ir.nodes)
    }

    fn finalize(&mut self) {
        // Close any remaining open fragments, activations, and participant groups
        while self.end_fragment() {}
        self.flush_open_activations();
        self.end_participant_group();

        // Apply semantic recovery
        self.apply_semantic_recovery();

        // Populate structured style types from raw style_refs.
        self.ir.populate_structured_styles();
    }

    pub(crate) fn finish_reusable(&mut self) {
        self.finalize();
    }

    pub(crate) fn reusable_prefix_unchanged(&self, _source: &Self) -> bool {
        let unchanged = self
            .reusable_prefix_guard
            .is_some_and(|guard| guard.unchanged);

        #[cfg(debug_assertions)]
        {
            let exact = self.ir.direction == _source.ir.direction
                && self.ir.meta == _source.ir.meta
                && self.ir.style_refs == _source.ir.style_refs
                && self.ir.style_defs == _source.ir.style_defs
                && self.ir.nodes.starts_with(&_source.ir.nodes)
                && self.ir.edges.starts_with(&_source.ir.edges)
                && self.ir.clusters.starts_with(&_source.ir.clusters)
                && self.ir.labels.starts_with(&_source.ir.labels)
                && self.ir.graph.nodes.starts_with(&_source.ir.graph.nodes)
                && self.ir.graph.edges.starts_with(&_source.ir.graph.edges)
                && self
                    .ir
                    .graph
                    .clusters
                    .starts_with(&_source.ir.graph.clusters)
                && self
                    .ir
                    .graph
                    .subgraphs
                    .starts_with(&_source.ir.graph.subgraphs)
                && _source
                    .ir
                    .label_markup
                    .iter()
                    .all(|(label, markup)| self.ir.label_markup.get(label) == Some(markup))
                && self
                    .ir
                    .label_markup
                    .keys()
                    .filter(|label| label.0 < _source.ir.labels.len())
                    .all(|label| _source.ir.label_markup.contains_key(label));
            debug_assert_eq!(
                unchanged, exact,
                "reusable-prefix mutation tracking drifted"
            );
        }

        unchanged
    }

    /// Finish building the IR, applying semantic recovery.
    pub(crate) fn finish(
        mut self,
        confidence: f32,
        detection_method: crate::DetectionMethod,
    ) -> ParseResult {
        self.finalize();

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

            let mut existing_node_changed = false;
            if let Some(existing_node) = self.ir.nodes.get_mut(existing_id.0) {
                if existing_node.label.is_none() && resolved_label.is_some() {
                    existing_node.label = resolved_label;
                    existing_node_changed = true;
                }
                if existing_node.shape == NodeShape::Rect && shape != NodeShape::Rect {
                    existing_node.shape = shape;
                    existing_node_changed = true;
                }

                // `span_all` is write-only dead data (no workspace reader); do not accumulate
                // a `Span` per node reference. See the node-construction site.

                // If this call is NOT auto-created but the existing node IS,
                // "upgrade" it to an explicit node and remove from tracking.
                if !is_auto_created && existing_node.implicit {
                    existing_node.implicit = false;
                    self.auto_created_nodes.retain(|&id| id != existing_id);
                    existing_node_changed = true;
                }
            }
            if existing_node_changed {
                self.mark_reusable_prefix_node_dirty(existing_id);
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
            journey_meta: None,
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
            self.cluster_member_set
                .reserve(self.ir.nodes.capacity().max(4));
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
                    self.mark_reusable_prefix_cluster_dirty(existing_index);
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
            // Set afterwards by the C4 boundary path; an ordinary subgraph has no boundary type.
            c4_boundary_type: None,
            // Filled afterwards by `add_class_to_cluster`, once `classDef`s are resolved (bd-6cdzy).
            classes: Vec::new(),
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
        if self.ir.clusters.get(cluster_index).is_none() {
            return;
        }
        let cluster_id = IrClusterId(cluster_index);
        // The membership index is already here: `ir.graph.nodes[i].clusters` is appended ONLY below,
        // starts empty, and grows on exactly the calls that append to `clusters[i].members`. So the
        // per-node list answers "is this node already in this cluster" without a second structure —
        // and it answers it from ~1 element (a node is in its own group plus its ancestors) instead
        // of a hash probe into a set sized to the whole diagram. See `add_node_to_subgraph` for the
        // mirrored site; both were paying this, and on a subgraph-heavy diagram the shared
        // `FxHashSet<(usize, IrNodeId)>::insert` was the single largest self-time frame (8.73% on
        // `arch_100x50`, 5,000 nodes in 100 subgraphs).
        let already = match self.ir.graph.nodes.get(node_id.0) {
            Some(graph_node) => graph_node.clusters.contains(&cluster_id),
            // No mirror to consult. Unreachable via the interner -- `ir.nodes` and `ir.graph.nodes`
            // are pushed in lockstep, so an id is never handed out before its graph node exists --
            // but keep the original set-based dedup rather than assume it. The two paths partition
            // node ids (a given id either has a graph node on every call or on none), so a node can
            // never dedup against the wrong one.
            None => !self.cluster_member_set.insert((cluster_index, node_id)),
        };
        if already {
            return;
        }
        self.mark_reusable_prefix_cluster_dirty(cluster_index);
        self.mark_reusable_prefix_node_dirty(node_id);
        if let Some(cluster) = self.ir.clusters.get_mut(cluster_index) {
            cluster.members.push(node_id);
        }
        if let Some(graph_cluster) = self.ir.graph.clusters.get_mut(cluster_index) {
            graph_cluster.members.push(node_id);
        }
        if let Some(graph_node) = self.ir.graph.nodes.get_mut(node_id.0) {
            graph_node.clusters.push(cluster_id);
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
            self.subgraph_member_set
                .reserve(self.ir.nodes.capacity().max(4));
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
                    self.mark_reusable_prefix_subgraph_dirty(existing_index);
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
        if let Some(parent_index) = parent {
            self.mark_reusable_prefix_subgraph_dirty(parent_index);
            if let Some(parent_graph) = self.ir.graph.subgraphs.get_mut(parent_index) {
                parent_graph.children.push(IrSubgraphId(subgraph_index));
            }
        }
        if let Some(cluster_index) = cluster_index {
            self.mark_reusable_prefix_cluster_dirty(cluster_index);
            if let Some(graph_cluster) = self.ir.graph.clusters.get_mut(cluster_index) {
                graph_cluster.subgraph = Some(IrSubgraphId(subgraph_index));
            }
        }
        self.subgraph_index_by_key
            .insert(normalized_lookup_key.to_string(), subgraph_index);
        Some(subgraph_index)
    }

    pub(crate) fn add_node_to_subgraph(&mut self, subgraph_index: usize, node_id: IrNodeId) {
        if self.ir.graph.subgraphs.get(subgraph_index).is_none() {
            return;
        }
        let subgraph_id = IrSubgraphId(subgraph_index);
        // Mirrors `add_node_to_cluster`: `ir.graph.nodes[i].subgraphs` is appended only below, starts
        // empty, and grows on exactly the calls that append to `subgraphs[i].members`, so it already
        // is the membership index and the parallel hash set is redundant on this path.
        let already = match self.ir.graph.nodes.get(node_id.0) {
            Some(graph_node) => graph_node.subgraphs.contains(&subgraph_id),
            None => !self.subgraph_member_set.insert((subgraph_index, node_id)),
        };
        if already {
            return;
        }
        self.mark_reusable_prefix_subgraph_dirty(subgraph_index);
        self.mark_reusable_prefix_node_dirty(node_id);
        if let Some(subgraph) = self.ir.graph.subgraphs.get_mut(subgraph_index) {
            subgraph.members.push(node_id);
        }
        if let Some(graph_node) = self.ir.graph.nodes.get_mut(node_id.0) {
            graph_node.subgraphs.push(subgraph_id);
        }
    }

    pub(crate) fn set_cluster_grid_span(&mut self, cluster_index: usize, grid_span: usize) {
        let grid_span = grid_span.max(1);
        let changes_prefix = self
            .ir
            .clusters
            .get(cluster_index)
            .is_some_and(|cluster| cluster.grid_span != grid_span)
            || self
                .ir
                .graph
                .clusters
                .get(cluster_index)
                .is_some_and(|cluster| cluster.grid_span != grid_span);
        if changes_prefix {
            self.mark_reusable_prefix_cluster_dirty(cluster_index);
        }
        if let Some(cluster) = self.ir.clusters.get_mut(cluster_index) {
            cluster.grid_span = grid_span;
        }
        if let Some(graph_cluster) = self.ir.graph.clusters.get_mut(cluster_index) {
            graph_cluster.grid_span = grid_span;
        }
    }

    pub(crate) fn set_subgraph_grid_span(&mut self, subgraph_index: usize, grid_span: usize) {
        let grid_span = grid_span.max(1);
        let changes_prefix = self
            .ir
            .graph
            .subgraphs
            .get(subgraph_index)
            .is_some_and(|subgraph| subgraph.grid_span != grid_span);
        if changes_prefix {
            self.mark_reusable_prefix_subgraph_dirty(subgraph_index);
        }
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
    /// The member an edge endpoint naming a SUBGRAPH should attach to (bd-pfibz).
    ///
    /// ⚠️ IT LOOKS THE ID UP IN `graph.subgraphs`, NOT IN `cluster_index_by_key`, AND THAT IS THE
    /// WHOLE BUG. Two earlier attempts at this fix were placed correctly and looked up the wrong
    /// key: a cluster is registered under `flow_subgraph_lookup_key`, which is `"{id}@title:{title}"`
    /// whenever a title exists — and `subgraph s1` DEFAULTS its title to its own id (bd-ka77), so
    /// the key is `s1@title:s1` and never `s1`. `IrSubgraph::key` is the public id, so this is the
    /// map that can answer the question the author asked.
    ///
    /// ⚠️ AND IT RESOLVES TO A MEMBER, NOT TO THE SUBGRAPH, WHICH IS A STATED LIMIT. `IrEndpoint`
    /// has no cluster or subgraph variant, and adding one means exhaustive matches through fm-layout
    /// and all three renderers — the cost the realization dash declined for the same reason. An edge
    /// to the first member draws between the right two regions rather than between their boundaries:
    /// geometry that differs from the reference, where inventing a box differs from the AUTHOR.
    ///
    /// An empty subgraph has no member to stand in for it, so it falls through to the old behaviour.
    pub(crate) fn subgraph_endpoint_member(&self, id: &str) -> Option<IrNodeId> {
        // A subgraph-free diagram pays one `is_empty` check; the scan below is over subgraphs, of
        // which a diagram has a handful, not over nodes.
        if self.ir.graph.subgraphs.is_empty() {
            return None;
        }
        let key = id.trim();
        self.ir
            .graph
            .subgraphs
            .iter()
            .find(|subgraph| subgraph.key == key)
            .and_then(|subgraph| subgraph.members.first().copied())
    }

    /// Seed the forward-reference map for one flowchart document, before any of it is lowered.
    ///
    /// Built by the parser from the already-parsed item tree, which is why this is a plain setter:
    /// the whole point of bd-dw2a9 is that the answer is knowable BEFORE lowering starts.
    pub(crate) fn set_flow_forward_subgraph_members(&mut self, map: FxHashMap<String, String>) {
        self.flow_forward_subgraph_members = map;
    }

    /// The node an edge endpoint naming a subgraph resolves to, whichever order they were written in.
    ///
    /// ⚠️ THIS IS THE HALF `subgraph_endpoint_member` CANNOT ANSWER (bd-dw2a9). That one reads
    /// `graph.subgraphs`, so it only sees subgraphs ALREADY lowered — a forward reference names one
    /// that does not exist yet, and fell through to interning a phantom box. The second lookup uses
    /// the pre-scan map and interns the member itself, which is legal precisely because the member
    /// would be interned by the subgraph body a moment later anyway: the edge only moves it earlier.
    ///
    /// This is why this bead's own "BLOCKED ON MISSING INFRASTRUCTURE" note was wrong. It assumed
    /// the only route was a post-lowering pass that REMOVED the phantom, which would mean remapping
    /// every `IrNodeId` in edges, cluster and subgraph members and both id maps. Nothing is removed
    /// here because nothing wrong is ever created.
    pub(crate) fn resolve_subgraph_endpoint(&mut self, id: &str, span: Span) -> Option<IrNodeId> {
        if let Some(member) = self.subgraph_endpoint_member(id) {
            return Some(member);
        }
        let target = self.flow_forward_subgraph_members.get(id.trim())?.clone();
        self.intern_node_auto_normalized(&target, None, NodeShape::Rect, span, false)
    }

    pub(crate) fn intern_edge_endpoint_pretrimmed(
        &mut self,
        id: &str,
        span: Span,
    ) -> Option<IrNodeId> {
        // ⚠️ THE GUARD SITS ON THE PATH THAT ACTUALLY RUNS. `s1 --> s2` is a "simple" edge and comes
        // through here, not through `intern_flow_ast_node`; guarding only that one leaves the
        // phantom exactly where it was while every slow-path test passes.
        if let Some(member) = self.resolve_subgraph_endpoint(id, span) {
            return Some(member);
        }
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
        self.intern_node_auto_normalized(
            id,
            label.map(NodeLabelInput::ParsedOwned),
            shape,
            span,
            false,
        )
    }

    /// Attach a state-diagram description to `id`, APPENDING to any label it already carries.
    ///
    /// `intern_node` deliberately fills a label only when the node has none — first writer wins —
    /// which is right for node tokens and wrong for descriptions: mermaid accumulates them, so
    /// `state "Desc" as s1` followed by `s1 : more` is `["Desc", "more"]` and draws two lines.
    /// Routing descriptions through `intern_node` silently dropped every line after the first
    /// (bd-xm62h). Joined with `\n`, which `fm_render_svg::wrap_node_label_lines` already splits on.
    pub(crate) fn append_state_description(&mut self, id: &str, text: &str, span: Span) {
        // ⚠️ THE THIRD BARE LABEL PATH (bd-j06n2). Node labels went through the parser's normalizer,
        // edge labels went through a same-named copy that only trimmed quotes, and a state
        // description went through NEITHER — `s1 : one<br/>two` kept the tag and `&amp;` stayed
        // encoded, while the identical text in a flowchart node came out right.
        let text = &clean_label(Some(text)).unwrap_or_else(|| text.to_owned());
        let Some(node_id) = self.intern_node(id, None, NodeShape::Rounded, span) else {
            return;
        };
        let existing = self
            .ir
            .nodes
            .get(node_id.0)
            .and_then(|node| node.label)
            .and_then(|label_id| self.ir.labels.get(label_id.0))
            .map(|label| label.text.clone());
        let combined = match existing {
            Some(existing) if !existing.is_empty() => {
                let mut combined = String::with_capacity(existing.len() + 1 + text.len());
                combined.push_str(&existing);
                combined.push('\n');
                combined.push_str(text);
                combined
            }
            _ => text.to_owned(),
        };
        let label_id = self.intern_plain_label_owned(combined, span);
        if let Some(node) = self.ir.nodes.get_mut(node_id.0) {
            node.label = Some(label_id);
        }
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

            let mut existing_node_changed = false;
            if let Some(existing_node) = self.ir.nodes.get_mut(existing_id.0) {
                if existing_node.label.is_none() && resolved_label.is_some() {
                    existing_node.label = resolved_label;
                    existing_node_changed = true;
                }
                if existing_node.shape == NodeShape::Rect && shape != NodeShape::Rect {
                    existing_node.shape = shape;
                    existing_node_changed = true;
                }
                if existing_node.implicit {
                    existing_node.implicit = false;
                    self.auto_created_nodes.retain(|&id| id != existing_id);
                    existing_node_changed = true;
                }
            }
            if existing_node_changed {
                self.mark_reusable_prefix_node_dirty(existing_id);
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
            journey_meta: None,
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

        let should_add = self.ir.nodes.get(node_id.0).is_some_and(|node| {
            !node
                .classes
                .iter()
                .any(|existing| existing == normalized_class)
        });
        if !should_add {
            return;
        }
        self.mark_reusable_prefix_node_dirty(node_id);
        if let Some(node) = self.ir.nodes.get_mut(node_id.0) {
            node.classes.push(normalized_class.to_string());
        }
    }

    /// Record a `classDef` name applied to a CLUSTER by `class <subgraph> <name>` (bd-6cdzy).
    ///
    /// The cluster twin of [`Self::add_class_to_node`], deduping the same way, because `class one
    /// hot` twice must not emit the marker twice. Takes the cluster INDEX rather than a key: the
    /// caller has already resolved it through `cluster_index_by_key`, and re-resolving here would be
    /// a second lookup that could disagree with the one that produced the style ref.
    pub(crate) fn add_class_to_cluster(&mut self, cluster_index: usize, class_name: &str) {
        let normalized_class = trim_fast(class_name);
        if normalized_class.is_empty() {
            return;
        }
        if let Some(cluster) = self.ir.clusters.get_mut(cluster_index)
            && !cluster
                .classes
                .iter()
                .any(|existing| existing == normalized_class)
        {
            cluster.classes.push(normalized_class.to_string());
        }
    }

    /// Record a journey step's actors as the author wrote them (bd-mq273).
    ///
    /// Separate from the `journey-actor-*` classes beside it: those are a STYLING hook and are
    /// normalized for CSS, so they cannot round-trip a name containing a space.
    pub(crate) fn set_journey_actors(&mut self, node_id: IrNodeId, actors: Vec<String>) {
        if actors.is_empty() {
            return;
        }
        if let Some(node) = self.ir.nodes.get_mut(node_id.0) {
            node.journey_meta = Some(Box::new(fm_core::IrJourneyNodeMeta { actors }));
        }
    }

    /// Record the C4 boundary type mermaid draws beneath a boundary's label.
    ///
    /// Stored as mermaid's own token; the renderer adds the `[…]` mermaid wraps it in, exactly as
    /// mermaid does — its `drawInsideBoundary` brackets the string and `drawBoundary` draws it.
    pub(crate) fn set_cluster_c4_boundary_type(
        &mut self,
        cluster_index: usize,
        boundary_type: &str,
    ) {
        if boundary_type.is_empty() {
            return;
        }
        if let Some(cluster) = self.ir.clusters.get_mut(cluster_index) {
            cluster.c4_boundary_type = Some(boundary_type.to_string());
        }
    }
    pub(crate) fn add_class_to_node_id(&mut self, node_id: IrNodeId, class_name: &str) {
        let normalized_class = trim_fast(class_name);
        if normalized_class.is_empty() {
            return;
        }

        let should_add = self.ir.nodes.get(node_id.0).is_some_and(|node| {
            !node
                .classes
                .iter()
                .any(|existing| existing == normalized_class)
        });
        if !should_add {
            return;
        }
        self.mark_reusable_prefix_node_dirty(node_id);
        if let Some(node) = self.ir.nodes.get_mut(node_id.0) {
            node.classes.push(normalized_class.to_string());
        }
    }

    pub(crate) fn set_node_icon(&mut self, node_id: IrNodeId, icon: &str) {
        let icon = icon.trim();
        if icon.is_empty() {
            return;
        }
        self.mark_reusable_prefix_node_dirty(node_id);
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

        self.mark_reusable_prefix_node_dirty(node_id);
        if let Some(node) = self.ir.nodes.get_mut(node_id.0) {
            node.interaction_mut().href = Some(target.to_string());
        }
    }

    /// Record the browser target a `click` directive declared for this node (bd-vn7s).
    ///
    /// Deliberately does NOT intern: this only ever runs directly after `set_node_link` for the
    /// same key, so the node exists, and interning here would give a misspelled alias a phantom
    /// node the way bd-xfmm did.
    pub(crate) fn set_node_link_target(&mut self, node_key: &str, link_target: &str) {
        let link_target = link_target.trim();
        if link_target.is_empty() {
            return;
        }

        let Some(node_id) = self.node_id_by_key(node_key) else {
            return;
        };

        self.mark_reusable_prefix_node_dirty(node_id);
        if let Some(node) = self.ir.nodes.get_mut(node_id.0) {
            node.interaction_mut().link_target = Some(link_target.to_string());
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

        self.mark_reusable_prefix_node_dirty(node_id);
        if let Some(node) = self.ir.nodes.get_mut(node_id.0) {
            node.interaction_mut().callback = Some(callback.to_string());
        }
    }

    pub(crate) fn set_node_tooltip(&mut self, node_key: &str, tooltip: &str, span: Span) {
        let Some(node_id) = self.intern_node(node_key, None, NodeShape::Rect, span) else {
            return;
        };
        self.mark_reusable_prefix_node_dirty(node_id);
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
        self.mark_reusable_prefix_node_dirty(node_id);
    }

    pub(crate) fn node_mut(&mut self, node_id: IrNodeId) -> Option<&mut fm_core::IrNode> {
        self.mark_reusable_prefix_node_dirty(node_id);
        self.ir.nodes.get_mut(node_id.0)
    }

    pub(crate) fn set_c4_node_meta(&mut self, node_id: IrNodeId, meta: IrC4NodeMeta) {
        self.mark_reusable_prefix_node_dirty(node_id);
        let Some(node) = self.ir.nodes.get_mut(node_id.0) else {
            return;
        };
        node.c4_meta = Some(Box::new(meta));
    }

    /// Add an entity attribute to a node (for ER diagrams). `keys` carries every key modifier
    /// in source order — empty when the attribute has none (bd-nryyc list semantics).
    pub(crate) fn add_entity_attribute(
        &mut self,
        node_id: IrNodeId,
        data_type: &str,
        name: &str,
        keys: Vec<IrAttributeKey>,
        comment: Option<&str>,
    ) {
        self.mark_reusable_prefix_node_dirty(node_id);
        let Some(node) = self.ir.nodes.get_mut(node_id.0) else {
            return;
        };

        node.members.push(IrEntityAttribute {
            data_type: data_type.to_string(),
            name: name.to_string(),
            keys,
            comment: comment.map(std::string::ToString::to_string),
        });
    }

    pub(crate) fn push_style_ref(&mut self, target: IrStyleTarget, style: String, span: Span) {
        self.mark_reusable_prefix_dirty();
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

    /// Associate a source-level Mermaid edge ID with the edge just lowered.
    pub(crate) fn set_last_edge_id(&mut self, edge_id: &str) {
        let Some(edge_index) = self.ir.edges.len().checked_sub(1) else {
            return;
        };
        self.edge_index_by_id
            .entry(edge_id.to_string())
            .or_insert(edge_index);
    }

    /// Attach a march speed to the edge previously registered under `edge_id`.
    ///
    /// A miss is a NO-OP, and that is upstream's behaviour, not a convenience: measured against
    /// the pinned 11.15.0 bundle, `zz@{ animate: true }` naming no declared edge renders an
    /// ordinary diagram with no animation and no error. Two further measured properties fall out
    /// of routing through `edge_index_by_id`, which stores the FIRST edge to claim an id:
    ///
    /// * `e1@{ … }` written BEFORE `A e1@--> B` animates nothing — the map has no entry yet.
    /// * when two edges both declare `e1`, only the first is animated.
    ///
    /// Both were measured upstream, and both are consequences of the existing first-match map
    /// rather than rules restated here, which is why this function has no ordering logic of its own.
    pub(crate) fn set_edge_animation(&mut self, edge_id: &str, animation: EdgeAnimation) {
        let Some(edge_index) = self.edge_index_by_id(edge_id) else {
            return;
        };
        self.mark_reusable_prefix_edge_dirty(edge_index);
        if let Some(edge) = self.ir.edges.get_mut(edge_index) {
            edge.extras_mut().animation = Some(animation);
        }
    }

    /// Set the ER cardinality notation on the last-pushed edge.
    pub(crate) fn set_last_edge_er_notation(&mut self, notation: &str) {
        if let Some(edge_index) = self.ir.edges.len().checked_sub(1) {
            self.mark_reusable_prefix_edge_dirty(edge_index);
        }
        if let Some(edge) = self.ir.edges.last_mut() {
            edge.extras_mut().er_notation = Some(Box::from(notation));
        }
    }

    /// Set cardinality labels on the most recently pushed edge.
    /// Attach an inline style to the edge just pushed (bd-u9hcc).
    ///
    /// Mirrors `set_last_edge_cardinality`: the class path pushes the edge and then decorates it,
    /// so this reaches back for the last one rather than threading an index through the AST.
    ///
    /// MERGES rather than replaces, because an edge can already carry a style from another channel
    /// and silently discarding it would be the same class of drop this bead is about. Later keys
    /// win, which is what a CSS declaration block already means.
    pub(crate) fn set_last_edge_inline_style(&mut self, style: &str) {
        let parsed = fm_core::parse_style_string(style);
        if parsed.properties.is_empty() {
            return;
        }
        let Some(edge_index) = self.ir.edges.len().checked_sub(1) else {
            return;
        };
        self.mark_reusable_prefix_edge_dirty(edge_index);
        if let Some(edge) = self.ir.edges.get_mut(edge_index) {
            match edge.inline_style.as_mut() {
                Some(existing) => existing.properties.extend(parsed.properties),
                None => edge.inline_style = Some(Box::new(parsed)),
            }
        }
    }

    pub(crate) fn set_last_edge_cardinality(&mut self, source: Option<&str>, target: Option<&str>) {
        if let Some(edge_index) = self.ir.edges.len().checked_sub(1) {
            self.mark_reusable_prefix_edge_dirty(edge_index);
        }
        if let Some(edge) = self.ir.edges.last_mut() {
            if let Some(s) = source {
                edge.extras_mut().source_cardinality = Some(Box::from(s));
            }
            if let Some(t) = target {
                edge.extras_mut().target_cardinality = Some(Box::from(t));
            }
        }
    }

    /// Record a C4 relationship's technology on the most recently pushed edge.
    ///
    /// Kept off the label so the renderer can draw it as the separate italic row mermaid draws.
    pub(crate) fn set_last_edge_technology(&mut self, technology: &str) {
        if technology.is_empty() {
            return;
        }
        if let Some(edge_index) = self.ir.edges.len().checked_sub(1) {
            self.mark_reusable_prefix_edge_dirty(edge_index);
        }
        if let Some(edge) = self.ir.edges.last_mut() {
            edge.extras_mut().technology = Some(Box::from(technology));
        }
    }

    /// Record the placement direction carried by a C4 directional relationship macro.
    pub(crate) fn set_last_edge_c4_direction(&mut self, direction: C4RelationshipDirection) {
        if let Some(edge_index) = self.ir.edges.len().checked_sub(1) {
            self.mark_reusable_prefix_edge_dirty(edge_index);
        }
        if let Some(edge) = self.ir.edges.last_mut() {
            edge.extras_mut().c4_direction = Some(direction);
        }
    }

    /// Set the declared architecture-beta placement sides on the most recently pushed edge.
    ///
    /// The `mark_reusable_prefix_edge_dirty` call is not decoration: every other edge mutator here
    /// makes it, and without it an incremental reparse can serve the pre-edit edge — i.e. changing
    /// `a:R --> L:b` to `a:B --> T:b` would leave the old direction in place.
    pub(crate) fn set_last_edge_architecture_sides(
        &mut self,
        source: Option<ArchitectureSide>,
        target: Option<ArchitectureSide>,
    ) {
        if source.is_none() && target.is_none() {
            return;
        }
        if let Some(edge_index) = self.ir.edges.len().checked_sub(1) {
            self.mark_reusable_prefix_edge_dirty(edge_index);
        }
        if let Some(edge) = self.ir.edges.last_mut() {
            if source.is_some() {
                edge.extras_mut().source_side = source;
            }
            if target.is_some() {
                edge.extras_mut().target_side = target;
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

/// Delegates to the parser's normalizer (bd-j06n2).
///
/// ⚠️ THIS USED TO BE A SECOND, DIFFERENT `clean_label`, and the name collision is what hid it. It
/// trimmed quotes and stopped there, so every caller below — edge labels and five diagram-title
/// sites — silently lost the `<br>` conversion, the HTML entity decode and the `#nn;` numeric
/// decode that node labels got from the parser's version. `A -->|"one<br/>two"| B` drew the tag;
/// `s1 : a &amp; b` came out as `a &amp`, truncated.
///
/// Kept as a wrapper rather than replaced at seven call sites: the point is that there is now ONE
/// implementation, not that the name disappears.
fn clean_label(input: Option<&str>) -> Option<String> {
    crate::mermaid_parser::clean_label(input)
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
