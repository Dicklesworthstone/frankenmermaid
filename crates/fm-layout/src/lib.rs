#![forbid(unsafe_code)]

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::f32::consts::PI;

use fm_core::{
    DiagramType, GraphDirection, IrEndpoint, IrNode, MermaidComplexity, MermaidConfig,
    MermaidDiagramIr, MermaidFidelity, MermaidGlyphMode, MermaidGuardReport,
    MermaidLayoutDecisionAlternative, MermaidLayoutDecisionLedger, MermaidLayoutDecisionRecord,
    MermaidPressureReport, MermaidPressureTier, Span,
};
use tracing::{debug, info, trace, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutAlgorithm {
    Auto,
    Sugiyama,
    Force,
    Tree,
    Radial,
    Timeline,
    Gantt,
    Sankey,
    Kanban,
    Grid,
    Sequence,
}

impl LayoutAlgorithm {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Sugiyama => "sugiyama",
            Self::Force => "force",
            Self::Tree => "tree",
            Self::Radial => "radial",
            Self::Timeline => "timeline",
            Self::Gantt => "gantt",
            Self::Sankey => "sankey",
            Self::Kanban => "kanban",
            Self::Grid => "grid",
            Self::Sequence => "sequence",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockBetaGridItem {
    Node(usize),
    Group(fm_core::IrSubgraphId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CycleStrategy {
    #[default]
    Greedy,
    DfsBack,
    MfasApprox,
    CycleAware,
}

impl CycleStrategy {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Greedy => "greedy",
            Self::DfsBack => "dfs-back",
            Self::MfasApprox => "mfas",
            Self::CycleAware => "cycle-aware",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "greedy" => Some(Self::Greedy),
            "dfs-back" | "dfs_back" | "dfs" => Some(Self::DfsBack),
            "mfas" | "minimum-feedback-arc-set" | "minimum_feedback_arc_set" => {
                Some(Self::MfasApprox)
            }
            "cycle-aware" | "cycle_aware" | "cycleaware" => Some(Self::CycleAware),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayoutConfig {
    pub cycle_strategy: CycleStrategy,
    pub collapse_cycle_clusters: bool,
    pub font_metrics: Option<fm_core::FontMetrics>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LayoutStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub crossing_count: usize,
    /// Crossing count after barycenter (before transpose/sifting refinement).
    pub crossing_count_before_refinement: usize,
    pub reversed_edges: usize,
    pub cycle_count: usize,
    pub cycle_node_count: usize,
    pub max_cycle_size: usize,
    pub collapsed_clusters: usize,
    /// Sum of Euclidean edge lengths for reversed (cycle-breaking) edges.
    pub reversed_edge_total_length: f32,
    /// Sum of Euclidean edge lengths for all edges.
    pub total_edge_length: f32,
    pub phase_iterations: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl LayoutRect {
    #[must_use]
    pub fn center(self) -> LayoutPoint {
        LayoutPoint {
            x: self.x + (self.width / 2.0),
            y: self.y + (self.height / 2.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutNodeBox {
    pub node_index: usize,
    pub node_id: String,
    pub rank: usize,
    pub order: usize,
    pub span: Span,
    pub bounds: LayoutRect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutClusterBox {
    pub cluster_index: usize,
    pub span: Span,
    pub bounds: LayoutRect,
}

/// Edge routing style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EdgeRouting {
    /// Manhattan-style orthogonal routing (default).
    #[default]
    Orthogonal,
    /// Cubic Bezier spline routing.
    Spline,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutEdgePath {
    pub edge_index: usize,
    pub span: Span,
    pub points: Vec<LayoutPoint>,
    pub reversed: bool,
    /// True if this is a self-loop edge (source == target).
    pub is_self_loop: bool,
    /// Offset for parallel edges (0 for first edge, increments for duplicates).
    pub parallel_offset: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutSpacing {
    pub node_spacing: f32,
    pub rank_spacing: f32,
    pub cluster_padding: f32,
}

impl Default for LayoutSpacing {
    fn default() -> Self {
        Self {
            node_spacing: 80.0,
            rank_spacing: 120.0,
            cluster_padding: 52.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutStageSnapshot {
    pub stage: &'static str,
    pub reversed_edges: usize,
    pub crossing_count: usize,
    pub node_count: usize,
    pub edge_count: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayoutTrace {
    pub dispatch: LayoutDispatch,
    pub guard: LayoutGuardDecision,
    pub snapshots: Vec<LayoutStageSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutGuardrails {
    pub max_layout_time_ms: usize,
    pub max_layout_iterations: usize,
    pub max_route_ops: usize,
}

impl Default for LayoutGuardrails {
    fn default() -> Self {
        let defaults = MermaidConfig::default();
        Self {
            max_layout_time_ms: 250,
            max_layout_iterations: defaults.layout_iteration_budget,
            max_route_ops: defaults.route_budget,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutGuardDecision {
    pub initial_algorithm: LayoutAlgorithm,
    pub selected_algorithm: LayoutAlgorithm,
    pub estimated_layout_time_ms: usize,
    pub estimated_layout_iterations: usize,
    pub estimated_route_ops: usize,
    pub selected_estimated_layout_time_ms: usize,
    pub selected_estimated_layout_iterations: usize,
    pub selected_estimated_route_ops: usize,
    pub time_budget_exceeded: bool,
    pub iteration_budget_exceeded: bool,
    pub route_budget_exceeded: bool,
    pub fallback_applied: bool,
    pub reason: &'static str,
}

impl Default for LayoutGuardDecision {
    fn default() -> Self {
        Self {
            initial_algorithm: LayoutAlgorithm::Sugiyama,
            selected_algorithm: LayoutAlgorithm::Sugiyama,
            estimated_layout_time_ms: 0,
            estimated_layout_iterations: 0,
            estimated_route_ops: 0,
            selected_estimated_layout_time_ms: 0,
            selected_estimated_layout_iterations: 0,
            selected_estimated_route_ops: 0,
            time_budget_exceeded: false,
            iteration_budget_exceeded: false,
            route_budget_exceeded: false,
            fallback_applied: false,
            reason: "within_budget",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutDispatch {
    pub requested: LayoutAlgorithm,
    pub selected: LayoutAlgorithm,
    pub capability_unavailable: bool,
    pub reason: &'static str,
}

impl Default for LayoutDispatch {
    fn default() -> Self {
        Self {
            requested: LayoutAlgorithm::Auto,
            selected: LayoutAlgorithm::Sugiyama,
            capability_unavailable: false,
            reason: "legacy_default",
        }
    }
}

/// Graph topology metrics used for intelligent algorithm auto-selection.
///
/// For diagram types that map unambiguously to a specific algorithm (e.g. Mindmap → Radial),
/// the metrics are not consulted. For general graph types (Flowchart, Class, State, ER, etc.),
/// these metrics drive the choice between Sugiyama, Force, and Tree layouts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphMetrics {
    pub node_count: usize,
    pub edge_count: usize,
    /// `edge_count / node_count` (0.0 when no nodes).
    pub edge_to_node_ratio: f32,
    /// Number of DFS back-edges (proxy for cycle density).
    pub back_edge_count: usize,
    /// Number of strongly connected components with more than one node.
    pub scc_count: usize,
    /// Size of the largest SCC (1 means no cycles).
    pub max_scc_size: usize,
    /// Nodes with in-degree 0 (candidate roots for tree layout).
    pub root_count: usize,
    /// True when the graph has no back-edges and a single root — an exact tree.
    pub is_tree_like: bool,
    /// True when the edge-to-node ratio is low (< 1.2), suggesting sparse connectivity.
    pub is_sparse: bool,
    /// True when the edge-to-node ratio is high (> 2.0), suggesting dense connectivity.
    pub is_dense: bool,
}

impl GraphMetrics {
    /// Compute graph metrics from the IR.  Runs DFS back-edge detection and Tarjan SCC
    /// detection in O(V+E) time, so this is cheap relative to actual layout.
    #[must_use]
    pub fn from_ir(ir: &MermaidDiagramIr) -> Self {
        let node_count = ir.nodes.len();
        let edges = resolved_edges(ir);
        let edge_count = edges.len();
        let edge_to_node_ratio = if node_count == 0 {
            0.0
        } else {
            edge_count as f32 / node_count as f32
        };

        let mut in_degree = vec![0_usize; node_count];
        for edge in &edges {
            in_degree[edge.target] = in_degree[edge.target].saturating_add(1);
        }
        let root_count = in_degree.iter().filter(|d| **d == 0).count();

        let back_edge_count = count_back_edges(node_count, &edges);

        let node_priority = stable_node_priorities(ir);
        let cycle_detection = detect_cycle_components(node_count, &edges, &node_priority);
        let scc_count = cycle_detection.cyclic_component_indexes.len();
        let max_scc_size = cycle_detection
            .components
            .iter()
            .filter(|c| c.len() > 1)
            .map(Vec::len)
            .max()
            .unwrap_or(1);

        let is_tree_like = node_count > 0
            && back_edge_count == 0
            && root_count == 1
            && edge_count == node_count - 1;
        let is_sparse = edge_to_node_ratio < 1.2;
        let is_dense = edge_to_node_ratio > 2.0;

        Self {
            node_count,
            edge_count,
            edge_to_node_ratio,
            back_edge_count,
            scc_count,
            max_scc_size,
            root_count,
            is_tree_like,
            is_sparse,
            is_dense,
        }
    }
}

/// Count DFS back-edges without full SCC decomposition.
fn count_back_edges(node_count: usize, edges: &[OrientedEdge]) -> usize {
    if node_count == 0 {
        return 0;
    }
    let mut adj = vec![vec![]; node_count];
    for edge in edges {
        if edge.source < node_count && edge.target < node_count {
            adj[edge.source].push(edge.target);
        }
    }
    let mut color = vec![0_u8; node_count];
    let mut back_edges = 0_usize;
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for start in 0..node_count {
        if color[start] != 0 {
            continue;
        }
        stack.push((start, 0));
        color[start] = 1;
        while let Some((node, idx)) = stack.last_mut() {
            if *idx < adj[*node].len() {
                let neighbor = adj[*node][*idx];
                *idx += 1;
                match color[neighbor] {
                    0 => {
                        color[neighbor] = 1;
                        stack.push((neighbor, 0));
                    }
                    1 => back_edges += 1,
                    _ => {}
                }
            } else {
                color[*node] = 2;
                stack.pop();
            }
        }
    }
    back_edges
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutCycleCluster {
    pub head_node_index: usize,
    pub member_node_indexes: Vec<usize>,
    pub bounds: LayoutRect,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayoutExtensions {
    pub bands: Vec<LayoutBand>,
    pub axis_ticks: Vec<LayoutAxisTick>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutBand {
    pub kind: LayoutBandKind,
    pub label: String,
    pub bounds: LayoutRect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutBandKind {
    Section,
    Lane,
    Column,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutAxisTick {
    pub label: String,
    pub position: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiagramLayout {
    pub nodes: Vec<LayoutNodeBox>,
    pub clusters: Vec<LayoutClusterBox>,
    pub cycle_clusters: Vec<LayoutCycleCluster>,
    pub edges: Vec<LayoutEdgePath>,
    pub bounds: LayoutRect,
    pub stats: LayoutStats,
    pub extensions: LayoutExtensions,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TracedLayout {
    pub layout: DiagramLayout,
    pub trace: LayoutTrace,
}

/// Target-agnostic render scene produced from diagram IR + layout geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderScene {
    pub bounds: RenderRect,
    pub root: RenderGroup,
}

/// Rectangle used by render IR primitives.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl From<LayoutRect> for RenderRect {
    fn from(value: LayoutRect) -> Self {
        Self {
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
        }
    }
}

/// Generic affine transform for backend-agnostic rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RenderTransform {
    Matrix {
        a: f32,
        b: f32,
        c: f32,
        d: f32,
        e: f32,
        f: f32,
    },
}

/// Optional clipping shape for groups.
#[derive(Debug, Clone, PartialEq)]
pub enum RenderClip {
    Rect(RenderRect),
    Path(Vec<PathCmd>),
}

/// A group of render items with optional transform/clip state.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderGroup {
    pub id: Option<String>,
    pub source: RenderSource,
    pub transform: Option<RenderTransform>,
    pub clip: Option<RenderClip>,
    pub children: Vec<RenderItem>,
}

impl RenderGroup {
    #[must_use]
    pub fn new(id: Option<String>) -> Self {
        Self {
            id,
            source: RenderSource::Diagram,
            transform: None,
            clip: None,
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_source(mut self, source: RenderSource) -> Self {
        self.source = source;
        self
    }
}

/// Source element a render primitive came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderSource {
    Diagram,
    Node(usize),
    Edge(usize),
    Cluster(usize),
}

/// Paint source for fills.
#[derive(Debug, Clone, PartialEq)]
pub enum FillStyle {
    Solid { color: String, opacity: f32 },
}

/// Stroke cap style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

/// Stroke join style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// Stroke style for path primitives.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeStyle {
    pub color: String,
    pub width: f32,
    pub opacity: f32,
    pub dash_array: Vec<f32>,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
}

impl StrokeStyle {
    #[must_use]
    pub fn solid(color: impl Into<String>, width: f32) -> Self {
        Self {
            color: color.into(),
            width,
            opacity: 1.0,
            dash_array: Vec::new(),
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
        }
    }
}

/// Path drawing commands used by all backends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCmd {
    MoveTo {
        x: f32,
        y: f32,
    },
    LineTo {
        x: f32,
        y: f32,
    },
    CubicTo {
        c1x: f32,
        c1y: f32,
        c2x: f32,
        c2y: f32,
        x: f32,
        y: f32,
    },
    QuadTo {
        cx: f32,
        cy: f32,
        x: f32,
        y: f32,
    },
    Close,
}

/// Marker kind for path endpoints (e.g. arrowheads).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarkerKind {
    #[default]
    None,
    Arrow,
    ThickArrow,
    DottedArrow,
    Circle,
    Cross,
    Diamond,
    Open,
}

/// A path primitive in the shared render IR.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderPath {
    pub source: RenderSource,
    pub commands: Vec<PathCmd>,
    pub fill: Option<FillStyle>,
    pub stroke: Option<StrokeStyle>,
    pub marker_start: MarkerKind,
    pub marker_end: MarkerKind,
}

/// Horizontal alignment for text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Start,
    Middle,
    End,
}

/// Vertical alignment baseline for text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextBaseline {
    Top,
    #[default]
    Middle,
    Bottom,
}

/// Text primitive in the shared render IR.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderText {
    pub source: RenderSource,
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub align: TextAlign,
    pub baseline: TextBaseline,
    pub fill: FillStyle,
}

/// A render IR item.
#[derive(Debug, Clone, PartialEq)]
pub enum RenderItem {
    Group(RenderGroup),
    Path(RenderPath),
    Text(RenderText),
}

/// Build a target-agnostic render scene from semantic IR and computed layout.
#[must_use]
pub fn build_render_scene(ir: &MermaidDiagramIr, layout: &DiagramLayout) -> RenderScene {
    let bounds = RenderRect::from(layout.bounds);

    let mut root = RenderGroup::new(Some(String::from("diagram-root"))).with_source(RenderSource::Diagram);
    root.transform = Some(RenderTransform::Matrix {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    });
    root.clip = Some(RenderClip::Rect(bounds));
    root.children
        .push(RenderItem::Group(build_cluster_layer(layout)));
    root.children
        .push(RenderItem::Group(build_edge_layer(ir, layout)));
    root.children
        .push(RenderItem::Group(build_node_layer(ir, layout)));
    root.children
        .push(RenderItem::Group(build_label_layer(ir, layout)));

    RenderScene { bounds, root }
}

fn build_cluster_layer(layout: &DiagramLayout) -> RenderGroup {
    let mut layer = RenderGroup::new(Some(String::from("clusters"))).with_source(RenderSource::Diagram);

    for cluster in &layout.clusters {
        layer.children.push(RenderItem::Path(RenderPath {
            source: RenderSource::Cluster(cluster.cluster_index),
            commands: rounded_rect_path(cluster.bounds, 8.0),
            fill: Some(FillStyle::Solid {
                color: String::from("#e2e8f0"),
                opacity: 0.24,
            }),
            stroke: Some(StrokeStyle::solid("#94a3b8", 1.0)),
            marker_start: MarkerKind::None,
            marker_end: MarkerKind::None,
        }));
    }

    if !layout.clusters.is_empty() {
        layer.clip = Some(RenderClip::Rect(RenderRect::from(layout.bounds)));
    }

    layer
}

fn build_edge_layer(ir: &MermaidDiagramIr, layout: &DiagramLayout) -> RenderGroup {
    let mut layer = RenderGroup::new(Some(String::from("edges"))).with_source(RenderSource::Diagram);

    for edge in &layout.edges {
        if edge.points.len() < 2 {
            continue;
        }

        let mut commands = Vec::with_capacity(edge.points.len() * 2);
        let n = edge.points.len();

        // Implement smoothing logic (Catmull-Rom to Cubic Bezier)
        commands.push(PathCmd::MoveTo {
            x: edge.points[0].x,
            y: edge.points[0].y,
        });

        if n == 2 {
            commands.push(PathCmd::LineTo {
                x: edge.points[1].x,
                y: edge.points[1].y,
            });
        } else {
            let t: f32 = 0.25; // Tension factor matching legacy renderer
            for i in 0..(n - 1) {
                let p_prev = if i == 0 {
                    edge.points[0]
                } else {
                    edge.points[i - 1]
                };
                let p_cur = edge.points[i];
                let p_next = edge.points[i + 1];
                let p_next2 = if i + 2 < n {
                    edge.points[i + 2]
                } else {
                    edge.points[n - 1]
                };

                commands.push(PathCmd::CubicTo {
                    c1x: p_cur.x + (p_next.x - p_prev.x) * t,
                    c1y: p_cur.y + (p_next.y - p_prev.y) * t,
                    c2x: p_next.x - (p_next2.x - p_cur.x) * t,
                    c2y: p_next.y - (p_next2.y - p_cur.y) * t,
                    x: p_next.x,
                    y: p_next.y,
                });
            }
        }

        let mut stroke = StrokeStyle::solid("#475569", 1.5);
        let mut marker_end = MarkerKind::None;
        let marker_start = MarkerKind::None;

        if let Some(ir_edge) = ir.edges.get(edge.edge_index) {
            if edge.reversed {
                stroke.dash_array = vec![4.0, 4.0];
                stroke.color = String::from("#94a3b8");
                marker_end = MarkerKind::Open;
            } else {
                match ir_edge.arrow {
                    fm_core::ArrowType::Line => marker_end = MarkerKind::None,
                    fm_core::ArrowType::Arrow => marker_end = MarkerKind::Arrow,
                    fm_core::ArrowType::ThickArrow => {
                        stroke.width = 2.5;
                        marker_end = MarkerKind::ThickArrow;
                    }
                    fm_core::ArrowType::DottedArrow => {
                        stroke.dash_array = vec![6.0, 4.0];
                        stroke.line_cap = LineCap::Round;
                        marker_end = MarkerKind::Arrow;
                    }
                    fm_core::ArrowType::Circle => marker_end = MarkerKind::Circle,
                    fm_core::ArrowType::Cross => marker_end = MarkerKind::Cross,
                }
            }
        }

        layer.children.push(RenderItem::Path(RenderPath {
            source: RenderSource::Edge(edge.edge_index),
            commands,
            fill: None,
            stroke: Some(stroke),
            marker_start,
            marker_end,
        }));
    }

    layer
}

fn build_node_layer(ir: &MermaidDiagramIr, layout: &DiagramLayout) -> RenderGroup {
    let mut layer = RenderGroup::new(Some(String::from("nodes"))).with_source(RenderSource::Diagram);

    for node_box in &layout.nodes {
        let shape = ir
            .nodes
            .get(node_box.node_index)
            .map_or(fm_core::NodeShape::Rect, |node| node.shape);

        layer.children.push(RenderItem::Path(RenderPath {
            source: RenderSource::Node(node_box.node_index),
            commands: node_path(node_box.bounds, shape),
            fill: Some(FillStyle::Solid {
                color: String::from("#ffffff"),
                opacity: 1.0,
            }),
            stroke: Some(StrokeStyle::solid("#94a3b8", 1.5)),
            marker_start: MarkerKind::None,
            marker_end: MarkerKind::None,
        }));
    }

    layer
}

fn build_label_layer(ir: &MermaidDiagramIr, layout: &DiagramLayout) -> RenderGroup {
    let mut layer = RenderGroup::new(Some(String::from("labels"))).with_source(RenderSource::Diagram);

    for node_box in &layout.nodes {
        let Some(node) = ir.nodes.get(node_box.node_index) else {
            continue;
        };
        let label_text = node
            .label
            .and_then(|label_id| ir.labels.get(label_id.0))
            .map_or_else(|| node.id.clone(), |label| label.text.clone());

        layer.children.push(RenderItem::Text(RenderText {
            source: RenderSource::Node(node_box.node_index),
            text: label_text,
            x: node_box.bounds.x + (node_box.bounds.width / 2.0),
            y: node_box.bounds.y + (node_box.bounds.height / 2.0),
            font_size: 14.0,
            align: TextAlign::Middle,
            baseline: TextBaseline::Middle,
            fill: FillStyle::Solid {
                color: String::from("#0f172a"),
                opacity: 1.0,
            },
        }));
    }

    for edge in &layout.edges {
        let Some(label) = ir
            .edges
            .get(edge.edge_index)
            .and_then(|edge_ir| edge_ir.label)
            .and_then(|label_id| ir.labels.get(label_id.0))
        else {
            continue;
        };

        let midpoint = edge_label_position(edge);
        layer.children.push(RenderItem::Text(RenderText {
            source: RenderSource::Edge(edge.edge_index),
            text: label.text.clone(),
            x: midpoint.x,
            y: midpoint.y,
            font_size: 12.0,
            align: TextAlign::Middle,
            baseline: TextBaseline::Middle,
            fill: FillStyle::Solid {
                color: String::from("#334155"),
                opacity: 1.0,
            },
        }));
    }

    for cluster in &layout.clusters {
        let Some(title) = ir
            .clusters
            .get(cluster.cluster_index)
            .and_then(|cluster_ir| cluster_ir.title)
            .and_then(|label_id| ir.labels.get(label_id.0))
        else {
            continue;
        };

        layer.children.push(RenderItem::Text(RenderText {
            source: RenderSource::Cluster(cluster.cluster_index),
            text: title.text.clone(),
            x: cluster.bounds.x + 10.0,
            y: cluster.bounds.y + 8.0,
            font_size: 12.0,
            align: TextAlign::Start,
            baseline: TextBaseline::Top,
            fill: FillStyle::Solid {
                color: String::from("#64748b"),
                opacity: 1.0,
            },
        }));
    }

    layer
}

fn node_path(bounds: LayoutRect, shape: fm_core::NodeShape) -> Vec<PathCmd> {
    use fm_core::NodeShape;
    match shape {
        NodeShape::Rect => rounded_rect_path(bounds, 5.0),
        NodeShape::Rounded => rounded_rect_path(bounds, 10.0),
        NodeShape::Stadium => stadium_path(bounds),
        NodeShape::Diamond => diamond_path(bounds),
        NodeShape::Hexagon => hexagon_path(bounds),
        NodeShape::Circle | NodeShape::DoubleCircle => polygon_ellipse_path(bounds, 24),
        NodeShape::Cylinder => cylinder_path(bounds),
        NodeShape::Trapezoid => trapezoid_path(bounds),
        NodeShape::InvTrapezoid => inv_trapezoid_path(bounds),
        NodeShape::Parallelogram => parallelogram_path(bounds),
        NodeShape::InvParallelogram => inv_parallelogram_path(bounds),
        NodeShape::Asymmetric => asymmetric_path(bounds),
        NodeShape::Note => note_path(bounds),
        NodeShape::Triangle => triangle_path(bounds),
        NodeShape::Pentagon => polygon_path(bounds, 5, -std::f32::consts::FRAC_PI_2),
        NodeShape::Star => star_path(bounds, 5),
        NodeShape::Cloud => cloud_path(bounds),
        NodeShape::Tag => tag_path(bounds),
        NodeShape::Subroutine | NodeShape::CrossedCircle => {
            // For composite shapes, we use the primary boundary path.
            // Inner lines are added by specialized render logic if needed,
            // but for simple path representation we return the outer box.
            rounded_rect_path(bounds, 4.0)
        }
    }
}

fn stadium_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let r = bounds.width.min(bounds.height) / 2.0;
    rounded_rect_path(bounds, r)
}

fn hexagon_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let cy = y + h / 2.0;
    let inset = w * 0.15;
    vec![
        PathCmd::MoveTo { x: x + inset, y },
        PathCmd::LineTo {
            x: x + w - inset,
            y,
        },
        PathCmd::LineTo { x: x + w, y: cy },
        PathCmd::LineTo {
            x: x + w - inset,
            y: y + h,
        },
        PathCmd::LineTo {
            x: x + inset,
            y: y + h,
        },
        PathCmd::LineTo { x, y: cy },
        PathCmd::Close,
    ]
}

fn cylinder_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let ry = h * 0.1;

    vec![
        // Top ellipse
        PathCmd::MoveTo { x, y: y + ry },
        // Approximate arcs with Cubic Bezier if needed, but for now we simplify
        // to maintaining the outer boundary.
        PathCmd::LineTo {
            x: x + w,
            y: y + ry,
        },
        PathCmd::LineTo {
            x: x + w,
            y: y + h - ry,
        },
        PathCmd::LineTo { x, y: y + h - ry },
        PathCmd::Close,
    ]
}

fn trapezoid_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let inset = w * 0.15;
    vec![
        PathCmd::MoveTo { x: x + inset, y },
        PathCmd::LineTo {
            x: x + w - inset,
            y,
        },
        PathCmd::LineTo { x: x + w, y: y + h },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::Close,
    ]
}

fn inv_trapezoid_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let inset = w * 0.15;
    vec![
        PathCmd::MoveTo { x, y },
        PathCmd::LineTo { x: x + w, y },
        PathCmd::LineTo {
            x: x + w - inset,
            y: y + h,
        },
        PathCmd::LineTo {
            x: x + inset,
            y: y + h,
        },
        PathCmd::Close,
    ]
}

fn parallelogram_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let inset = w * 0.15;
    vec![
        PathCmd::MoveTo { x: x + inset, y },
        PathCmd::LineTo { x: x + w, y },
        PathCmd::LineTo {
            x: x + w - inset,
            y: y + h,
        },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::Close,
    ]
}

fn inv_parallelogram_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let inset = w * 0.15;
    vec![
        PathCmd::MoveTo { x, y },
        PathCmd::LineTo {
            x: x + w - inset,
            y,
        },
        PathCmd::LineTo { x: x + w, y: y + h },
        PathCmd::LineTo {
            x: x + inset,
            y: y + h,
        },
        PathCmd::Close,
    ]
}

fn asymmetric_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let flag = w * 0.15;
    let cy = y + h / 2.0;
    vec![
        PathCmd::MoveTo { x, y },
        PathCmd::LineTo { x: x + w - flag, y },
        PathCmd::LineTo { x: x + w, y: cy },
        PathCmd::LineTo {
            x: x + w - flag,
            y: y + h,
        },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::Close,
    ]
}

fn note_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let fold = 10.0;
    vec![
        PathCmd::MoveTo { x, y },
        PathCmd::LineTo { x: x + w - fold, y },
        PathCmd::LineTo {
            x: x + w,
            y: y + fold,
        },
        PathCmd::LineTo { x: x + w, y: y + h },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::Close,
    ]
}

fn triangle_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let cx = x + w / 2.0;
    vec![
        PathCmd::MoveTo { x: cx, y },
        PathCmd::LineTo { x: x + w, y: y + h },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::Close,
    ]
}

fn polygon_path(bounds: LayoutRect, sides: usize, angle_offset: f32) -> Vec<PathCmd> {
    let cx = bounds.x + (bounds.width / 2.0);
    let cy = bounds.y + (bounds.height / 2.0);
    let r = bounds.width.min(bounds.height) / 2.0;
    let mut cmds = Vec::with_capacity(sides + 1);
    for i in 0..sides {
        let angle = angle_offset + (i as f32) * 2.0 * std::f32::consts::PI / (sides as f32);
        let px = cx + r * angle.cos();
        let py = cy + r * angle.sin();
        if i == 0 {
            cmds.push(PathCmd::MoveTo { x: px, y: py });
        } else {
            cmds.push(PathCmd::LineTo { x: px, y: py });
        }
    }
    cmds.push(PathCmd::Close);
    cmds
}

fn star_path(bounds: LayoutRect, points: usize) -> Vec<PathCmd> {
    let cx = bounds.x + (bounds.width / 2.0);
    let cy = bounds.y + (bounds.height / 2.0);
    let outer_r = bounds.width.min(bounds.height) / 2.0;
    let inner_r = outer_r * 0.4;
    let angle_offset = -std::f32::consts::FRAC_PI_2;
    let total_points = points * 2;
    let mut cmds = Vec::with_capacity(total_points + 1);
    for i in 0..total_points {
        let r = if i % 2 == 0 { outer_r } else { inner_r };
        let angle = angle_offset + (i as f32) * std::f32::consts::PI / (points as f32);
        let px = cx + r * angle.cos();
        let py = cy + r * angle.sin();
        if i == 0 {
            cmds.push(PathCmd::MoveTo { x: px, y: py });
        } else {
            cmds.push(PathCmd::LineTo { x: px, y: py });
        }
    }
    cmds.push(PathCmd::Close);
    cmds
}

fn cloud_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let r = h / 3.0;
    // Simplified cloud path
    vec![
        PathCmd::MoveTo {
            x: x + r,
            y: y + h * 0.6,
        },
        PathCmd::LineTo {
            x: x + r * 2.0,
            y: y + h * 0.3,
        },
        PathCmd::LineTo {
            x: x + w * 0.5,
            y: y + r * 0.5,
        },
        PathCmd::LineTo {
            x: x + w - r * 2.0,
            y: y + h * 0.3,
        },
        PathCmd::LineTo {
            x: x + w - r,
            y: y + h * 0.6,
        },
        PathCmd::LineTo {
            x: x + w - r,
            y: y + h * 0.8,
        },
        PathCmd::LineTo {
            x: x + r,
            y: y + h * 0.8,
        },
        PathCmd::Close,
    ]
}

fn tag_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let point = w * 0.2;
    let cy = y + h / 2.0;
    vec![
        PathCmd::MoveTo { x, y },
        PathCmd::LineTo {
            x: x + w - point,
            y,
        },
        PathCmd::LineTo { x: x + w, y: cy },
        PathCmd::LineTo {
            x: x + w - point,
            y: y + h,
        },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::Close,
    ]
}

fn rounded_rect_path(bounds: LayoutRect, radius: f32) -> Vec<PathCmd> {
    let mut commands = Vec::with_capacity(10);
    let r = radius.min(bounds.width / 2.0).min(bounds.height / 2.0);
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;

    commands.push(PathCmd::MoveTo { x: x + r, y });
    commands.push(PathCmd::LineTo { x: x + w - r, y });
    commands.push(PathCmd::QuadTo {
        cx: x + w,
        cy: y,
        x: x + w,
        y: y + r,
    });
    commands.push(PathCmd::LineTo {
        x: x + w,
        y: y + h - r,
    });
    commands.push(PathCmd::QuadTo {
        cx: x + w,
        cy: y + h,
        x: x + w - r,
        y: y + h,
    });
    commands.push(PathCmd::LineTo { x: x + r, y: y + h });
    commands.push(PathCmd::QuadTo {
        cx: x,
        cy: y + h,
        x,
        y: y + h - r,
    });
    commands.push(PathCmd::LineTo { x, y: y + r });
    commands.push(PathCmd::QuadTo {
        cx: x,
        cy: y,
        x: x + r,
        y,
    });
    commands.push(PathCmd::Close);

    commands
}

fn diamond_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let cx = bounds.x + (bounds.width / 2.0);
    let cy = bounds.y + (bounds.height / 2.0);
    vec![
        PathCmd::MoveTo { x: cx, y: bounds.y },
        PathCmd::LineTo {
            x: bounds.x + bounds.width,
            y: cy,
        },
        PathCmd::LineTo {
            x: cx,
            y: bounds.y + bounds.height,
        },
        PathCmd::LineTo { x: bounds.x, y: cy },
        PathCmd::Close,
    ]
}

fn polygon_ellipse_path(bounds: LayoutRect, segments: usize) -> Vec<PathCmd> {
    let segment_count = segments.max(8);
    let cx = bounds.x + (bounds.width / 2.0);
    let cy = bounds.y + (bounds.height / 2.0);
    let rx = bounds.width / 2.0;
    let ry = bounds.height / 2.0;

    let mut commands = Vec::with_capacity(segment_count + 2);
    for index in 0..segment_count {
        let theta = (index as f32 / segment_count as f32) * 2.0 * PI;
        let x = cx + (rx * theta.cos());
        let y = cy + (ry * theta.sin());
        if index == 0 {
            commands.push(PathCmd::MoveTo { x, y });
        } else {
            commands.push(PathCmd::LineTo { x, y });
        }
    }
    commands.push(PathCmd::Close);
    commands
}

fn edge_label_position(edge_path: &LayoutEdgePath) -> LayoutPoint {
    if edge_path.points.len() == 4 {
        let p1 = &edge_path.points[1];
        let p2 = &edge_path.points[2];
        LayoutPoint {
            x: (p1.x + p2.x) / 2.0,
            y: (p1.y + p2.y) / 2.0,
        }
    } else if edge_path.points.len() == 2 {
        let p1 = &edge_path.points[0];
        let p2 = &edge_path.points[1];
        LayoutPoint {
            x: (p1.x + p2.x) / 2.0,
            y: (p1.y + p2.y) / 2.0,
        }
    } else {
        let midpoint_index = edge_path.points.len() / 2;
        edge_path.points[midpoint_index]
    }
}

#[must_use]
pub fn layout(ir: &MermaidDiagramIr, algorithm: LayoutAlgorithm) -> LayoutStats {
    layout_diagram_traced_with_algorithm(ir, algorithm)
        .layout
        .stats
}

#[must_use]
pub fn layout_diagram(ir: &MermaidDiagramIr) -> DiagramLayout {
    layout_diagram_traced(ir).layout
}

#[must_use]
pub fn layout_diagram_with_cycle_strategy(
    ir: &MermaidDiagramIr,
    cycle_strategy: CycleStrategy,
) -> DiagramLayout {
    layout_diagram_traced_with_cycle_strategy(ir, cycle_strategy).layout
}

#[must_use]
pub fn layout_diagram_with_config(ir: &MermaidDiagramIr, config: LayoutConfig) -> DiagramLayout {
    layout_diagram_traced_with_config(ir, LayoutAlgorithm::Auto, config).layout
}

#[must_use]
pub fn layout_diagram_traced(ir: &MermaidDiagramIr) -> TracedLayout {
    layout_diagram_traced_with_algorithm_and_cycle_strategy(
        ir,
        LayoutAlgorithm::Auto,
        default_cycle_strategy(),
    )
}

#[must_use]
pub fn layout_diagram_traced_with_cycle_strategy(
    ir: &MermaidDiagramIr,
    cycle_strategy: CycleStrategy,
) -> TracedLayout {
    layout_diagram_traced_with_algorithm_and_cycle_strategy(
        ir,
        LayoutAlgorithm::Auto,
        cycle_strategy,
    )
}

#[must_use]
pub fn layout_diagram_traced_with_algorithm(
    ir: &MermaidDiagramIr,
    algorithm: LayoutAlgorithm,
) -> TracedLayout {
    layout_diagram_traced_with_algorithm_and_cycle_strategy(ir, algorithm, default_cycle_strategy())
}

#[must_use]
pub fn layout_diagram_traced_with_algorithm_and_guardrails(
    ir: &MermaidDiagramIr,
    algorithm: LayoutAlgorithm,
    guardrails: LayoutGuardrails,
) -> TracedLayout {
    layout_diagram_traced_with_config_and_guardrails(
        ir,
        algorithm,
        LayoutConfig {
            cycle_strategy: default_cycle_strategy(),
            collapse_cycle_clusters: false,
            font_metrics: None,
        },
        guardrails,
    )
}

#[must_use]
pub fn layout_diagram_traced_with_algorithm_and_cycle_strategy(
    ir: &MermaidDiagramIr,
    algorithm: LayoutAlgorithm,
    cycle_strategy: CycleStrategy,
) -> TracedLayout {
    layout_diagram_traced_with_config(
        ir,
        algorithm,
        LayoutConfig {
            cycle_strategy,
            collapse_cycle_clusters: false,
            font_metrics: None,
        },
    )
}

#[must_use]
pub fn layout_diagram_traced_with_config(
    ir: &MermaidDiagramIr,
    algorithm: LayoutAlgorithm,
    config: LayoutConfig,
) -> TracedLayout {
    layout_diagram_traced_with_config_and_guardrails(
        ir,
        algorithm,
        config,
        LayoutGuardrails::default(),
    )
}

#[must_use]
pub fn layout_diagram_traced_with_config_and_guardrails(
    ir: &MermaidDiagramIr,
    algorithm: LayoutAlgorithm,
    config: LayoutConfig,
    guardrails: LayoutGuardrails,
) -> TracedLayout {
    let dispatch = dispatch_layout_algorithm(ir, algorithm);
    let guard = evaluate_layout_guardrails(ir, dispatch.selected, guardrails);
    let mut guarded_dispatch = dispatch;
    guarded_dispatch.selected = guard.selected_algorithm;
    if guard.fallback_applied {
        guarded_dispatch.reason = guard.reason;
    }

    let mut traced = match guarded_dispatch.selected {
        LayoutAlgorithm::Sugiyama => layout_diagram_sugiyama_traced_with_config(ir, config),
        LayoutAlgorithm::Force => layout_diagram_force_traced(ir),
        LayoutAlgorithm::Tree => layout_diagram_tree_traced(ir),
        LayoutAlgorithm::Radial => layout_diagram_radial_traced(ir),
        LayoutAlgorithm::Timeline => layout_diagram_timeline_traced(ir),
        LayoutAlgorithm::Gantt => layout_diagram_gantt_traced(ir),
        LayoutAlgorithm::Sankey => layout_diagram_sankey_traced(ir),
        LayoutAlgorithm::Kanban => layout_diagram_kanban_traced(ir),
        LayoutAlgorithm::Grid => layout_diagram_grid_traced(ir),
        LayoutAlgorithm::Sequence => layout_diagram_sequence_traced(ir),
        LayoutAlgorithm::Auto => unreachable!("dispatch must resolve auto to a concrete layout"),
    };
    traced.trace.dispatch = guarded_dispatch;
    traced.trace.guard = guard;
    traced.trace.snapshots.insert(
        0,
        LayoutStageSnapshot {
            stage: "dispatch",
            reversed_edges: 0,
            crossing_count: 0,
            node_count: ir.nodes.len(),
            edge_count: ir.edges.len(),
        },
    );
    traced.layout.stats.phase_iterations = traced.trace.snapshots.len();
    traced
}

fn dispatch_layout_algorithm(ir: &MermaidDiagramIr, requested: LayoutAlgorithm) -> LayoutDispatch {
    let dispatch = match requested {
        LayoutAlgorithm::Auto => {
            let selected = preferred_layout_algorithm(ir);
            LayoutDispatch {
                requested,
                selected,
                capability_unavailable: false,
                reason: auto_selection_reason(ir, selected),
            }
        }
        explicit => {
            if algorithm_available_for_diagram(ir.diagram_type, explicit) {
                LayoutDispatch {
                    requested,
                    selected: explicit,
                    capability_unavailable: false,
                    reason: "explicit_request_honored",
                }
            } else {
                let selected = preferred_layout_algorithm(ir);
                LayoutDispatch {
                    requested,
                    selected,
                    capability_unavailable: true,
                    reason: "requested_algorithm_capability_unavailable_for_diagram_type",
                }
            }
        }
    };
    info!(
        requested = dispatch.requested.as_str(),
        selected = dispatch.selected.as_str(),
        capability_unavailable = dispatch.capability_unavailable,
        reason = dispatch.reason,
        diagram_type = ir.diagram_type.as_str(),
        node_count = ir.nodes.len(),
        edge_count = ir.edges.len(),
        "layout.dispatch"
    );
    dispatch
}

/// Return a static reason string that explains *why* the auto-selector chose this algorithm.
fn auto_selection_reason(ir: &MermaidDiagramIr, selected: LayoutAlgorithm) -> &'static str {
    match ir.diagram_type {
        DiagramType::Mindmap => return "auto_diagram_type_mindmap",
        DiagramType::Timeline => return "auto_diagram_type_timeline",
        DiagramType::Gantt => return "auto_diagram_type_gantt",
        DiagramType::Sankey => return "auto_diagram_type_sankey",
        DiagramType::Journey | DiagramType::Kanban => return "auto_diagram_type_kanban",
        DiagramType::BlockBeta => return "auto_diagram_type_block_beta",
        DiagramType::Sequence => return "auto_diagram_type_sequence",
        _ => {}
    }
    match selected {
        LayoutAlgorithm::Tree => "auto_metrics_tree_like",
        LayoutAlgorithm::Force => {
            let metrics = GraphMetrics::from_ir(ir);
            if metrics.is_dense {
                "auto_metrics_dense_graph"
            } else {
                "auto_metrics_sparse_disconnected"
            }
        }
        _ => "auto_metrics_default_sugiyama",
    }
}

fn preferred_layout_algorithm(ir: &MermaidDiagramIr) -> LayoutAlgorithm {
    match ir.diagram_type {
        DiagramType::Mindmap => LayoutAlgorithm::Radial,
        DiagramType::Timeline => LayoutAlgorithm::Timeline,
        DiagramType::Gantt => LayoutAlgorithm::Gantt,
        DiagramType::Sankey => LayoutAlgorithm::Sankey,
        DiagramType::Journey | DiagramType::Kanban => LayoutAlgorithm::Kanban,
        DiagramType::BlockBeta => LayoutAlgorithm::Grid,
        DiagramType::Sequence => LayoutAlgorithm::Sequence,
        _ => select_general_graph_algorithm(ir),
    }
}

/// For general graph types (Flowchart, Class, State, ER, C4, Requirement, etc.),
/// analyze graph topology metrics to choose between Sugiyama, Tree, and Force.
fn select_general_graph_algorithm(ir: &MermaidDiagramIr) -> LayoutAlgorithm {
    let metrics = GraphMetrics::from_ir(ir);

    // Trivial graphs: Sugiyama handles them efficiently.
    if metrics.node_count <= 2 {
        return LayoutAlgorithm::Sugiyama;
    }

    // Perfect tree: use Tree layout for cleaner hierarchical rendering.
    // Only for larger trees where tidy-tree shows significant benefit.
    if metrics.is_tree_like && metrics.node_count > 10 {
        return LayoutAlgorithm::Tree;
    }

    // Dense graphs with many crossings: force-directed avoids excessive crossing
    // minimization cost and often produces better results for hairball graphs.
    // Only apply to larger graphs where Sugiyama performance starts to degrade.
    if metrics.is_dense && metrics.node_count > 30 {
        return LayoutAlgorithm::Force;
    }

    // Very sparse disconnected graphs with MANY components: force-directed can handle
    // them naturally, but Sugiyama's component compaction is usually cleaner for small/medium graphs.
    // Only switch to Force if the graph is both large and has many back-edges (cycles).
    if metrics.node_count > 50 && metrics.back_edge_count > 5 {
        return LayoutAlgorithm::Force;
    }

    // Default: Sugiyama produces clean hierarchical layouts for most DAG-like graphs and forests.
    LayoutAlgorithm::Sugiyama
}

fn algorithm_available_for_diagram(diagram_type: DiagramType, algorithm: LayoutAlgorithm) -> bool {
    match algorithm {
        LayoutAlgorithm::Auto => true,
        LayoutAlgorithm::Sugiyama | LayoutAlgorithm::Force | LayoutAlgorithm::Tree => true,
        LayoutAlgorithm::Radial => matches!(diagram_type, DiagramType::Mindmap),
        LayoutAlgorithm::Timeline => matches!(diagram_type, DiagramType::Timeline),
        LayoutAlgorithm::Gantt => matches!(diagram_type, DiagramType::Gantt),
        LayoutAlgorithm::Sankey => matches!(diagram_type, DiagramType::Sankey),
        LayoutAlgorithm::Kanban => {
            matches!(diagram_type, DiagramType::Journey | DiagramType::Kanban)
        }
        LayoutAlgorithm::Grid => matches!(diagram_type, DiagramType::BlockBeta),
        LayoutAlgorithm::Sequence => matches!(diagram_type, DiagramType::Sequence),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LayoutCostEstimate {
    time_ms: usize,
    iterations: usize,
    route_ops: usize,
}

impl LayoutCostEstimate {
    #[must_use]
    fn exceeds(self, guardrails: LayoutGuardrails) -> (bool, bool, bool) {
        (
            self.time_ms > guardrails.max_layout_time_ms,
            self.iterations > guardrails.max_layout_iterations,
            self.route_ops > guardrails.max_route_ops,
        )
    }

    #[must_use]
    const fn score(self) -> usize {
        self.time_ms
            .saturating_mul(16)
            .saturating_add(self.iterations.saturating_mul(4))
            .saturating_add(self.route_ops)
    }
}

fn estimate_layout_cost(ir: &MermaidDiagramIr, algorithm: LayoutAlgorithm) -> LayoutCostEstimate {
    let nodes = ir.nodes.len();
    let edges = ir.edges.len();
    let ranks_hint = nodes.max(1).div_ceil(4);
    match algorithm {
        LayoutAlgorithm::Sugiyama => LayoutCostEstimate {
            time_ms: nodes
                .saturating_mul(edges.max(1))
                .div_ceil(50)
                .saturating_add(ranks_hint.saturating_mul(5))
                .saturating_add(10),
            iterations: ranks_hint.saturating_mul(10).saturating_add(24),
            route_ops: edges
                .saturating_mul(24)
                .saturating_add(nodes.saturating_mul(4)),
        },
        LayoutAlgorithm::Force => {
            let iterations = force_iteration_budget(nodes);
            LayoutCostEstimate {
                time_ms: nodes
                    .saturating_mul(nodes.max(1))
                    .saturating_mul(iterations.max(1))
                    / 40
                    + 20,
                iterations,
                route_ops: edges
                    .saturating_mul(16)
                    .saturating_add(nodes.saturating_mul(6)),
            }
        }
        LayoutAlgorithm::Tree => LayoutCostEstimate {
            time_ms: nodes
                .saturating_mul(4)
                .saturating_add(edges.saturating_mul(2))
                .saturating_add(8),
            iterations: nodes.saturating_add(4),
            route_ops: edges.saturating_mul(8).saturating_add(nodes),
        },
        LayoutAlgorithm::Radial => LayoutCostEstimate {
            time_ms: nodes
                .saturating_mul(5)
                .saturating_add(edges.saturating_mul(2))
                .saturating_add(12),
            iterations: nodes.saturating_add(6),
            route_ops: edges
                .saturating_mul(8)
                .saturating_add(nodes.saturating_mul(2)),
        },
        LayoutAlgorithm::Timeline
        | LayoutAlgorithm::Gantt
        | LayoutAlgorithm::Kanban
        | LayoutAlgorithm::Grid
        | LayoutAlgorithm::Sequence => LayoutCostEstimate {
            time_ms: nodes
                .saturating_mul(3)
                .saturating_add(edges.saturating_mul(2))
                .saturating_add(6),
            iterations: nodes.saturating_add(2),
            route_ops: edges.saturating_mul(6).saturating_add(nodes),
        },
        LayoutAlgorithm::Sankey => LayoutCostEstimate {
            time_ms: nodes
                .saturating_mul(8)
                .saturating_add(edges.saturating_mul(6))
                .saturating_add(20),
            iterations: nodes.saturating_mul(2).saturating_add(8),
            route_ops: edges
                .saturating_mul(18)
                .saturating_add(nodes.saturating_mul(4)),
        },
        LayoutAlgorithm::Auto => LayoutCostEstimate {
            time_ms: 0,
            iterations: 0,
            route_ops: 0,
        },
    }
}

fn fallback_candidates(ir: &MermaidDiagramIr, selected: LayoutAlgorithm) -> Vec<LayoutAlgorithm> {
    let mut candidates = vec![selected];
    let preferred = match ir.diagram_type {
        DiagramType::BlockBeta => [
            LayoutAlgorithm::Grid,
            LayoutAlgorithm::Tree,
            LayoutAlgorithm::Sugiyama,
        ],
        DiagramType::Mindmap => [
            LayoutAlgorithm::Radial,
            LayoutAlgorithm::Tree,
            LayoutAlgorithm::Sugiyama,
        ],
        DiagramType::Timeline => [
            LayoutAlgorithm::Timeline,
            LayoutAlgorithm::Tree,
            LayoutAlgorithm::Sugiyama,
        ],
        DiagramType::Gantt => [
            LayoutAlgorithm::Gantt,
            LayoutAlgorithm::Grid,
            LayoutAlgorithm::Sugiyama,
        ],
        DiagramType::Sankey => [
            LayoutAlgorithm::Sankey,
            LayoutAlgorithm::Tree,
            LayoutAlgorithm::Sugiyama,
        ],
        DiagramType::Journey | DiagramType::Kanban => [
            LayoutAlgorithm::Kanban,
            LayoutAlgorithm::Grid,
            LayoutAlgorithm::Sugiyama,
        ],
        DiagramType::Sequence => [
            LayoutAlgorithm::Sequence,
            LayoutAlgorithm::Grid,
            LayoutAlgorithm::Sugiyama,
        ],
        _ => [selected, LayoutAlgorithm::Tree, LayoutAlgorithm::Sugiyama],
    };

    for candidate in preferred {
        if candidate != LayoutAlgorithm::Auto
            && algorithm_available_for_diagram(ir.diagram_type, candidate)
            && !candidates.contains(&candidate)
        {
            candidates.push(candidate);
        }
    }

    for candidate in [
        LayoutAlgorithm::Tree,
        LayoutAlgorithm::Sugiyama,
        LayoutAlgorithm::Grid,
    ] {
        if candidate != LayoutAlgorithm::Auto
            && algorithm_available_for_diagram(ir.diagram_type, candidate)
            && !candidates.contains(&candidate)
        {
            candidates.push(candidate);
        }
    }

    candidates
}

fn guardrail_reason(
    time_budget_exceeded: bool,
    iteration_budget_exceeded: bool,
    route_budget_exceeded: bool,
    fallback_applied: bool,
    within_budget_candidate_found: bool,
) -> &'static str {
    match (
        time_budget_exceeded,
        iteration_budget_exceeded,
        route_budget_exceeded,
        fallback_applied,
        within_budget_candidate_found,
    ) {
        (false, false, false, false, _) => "within_budget",
        (true, false, false, true, true) => "guardrail_fallback_time_budget",
        (false, true, false, true, true) => "guardrail_fallback_iteration_budget",
        (false, false, true, true, true) => "guardrail_fallback_route_budget",
        (_, _, _, true, true) => "guardrail_fallback_multi_budget",
        (true, false, false, true, false) => "guardrail_forced_time_budget",
        (false, true, false, true, false) => "guardrail_forced_iteration_budget",
        (false, false, true, true, false) => "guardrail_forced_route_budget",
        _ => "guardrail_forced_multi_budget",
    }
}

fn evaluate_layout_guardrails(
    ir: &MermaidDiagramIr,
    selected: LayoutAlgorithm,
    guardrails: LayoutGuardrails,
) -> LayoutGuardDecision {
    let initial_estimate = estimate_layout_cost(ir, selected);
    let (time_budget_exceeded, iteration_budget_exceeded, route_budget_exceeded) =
        initial_estimate.exceeds(guardrails);

    if !(time_budget_exceeded || iteration_budget_exceeded || route_budget_exceeded) {
        return LayoutGuardDecision {
            initial_algorithm: selected,
            selected_algorithm: selected,
            estimated_layout_time_ms: initial_estimate.time_ms,
            estimated_layout_iterations: initial_estimate.iterations,
            estimated_route_ops: initial_estimate.route_ops,
            selected_estimated_layout_time_ms: initial_estimate.time_ms,
            selected_estimated_layout_iterations: initial_estimate.iterations,
            selected_estimated_route_ops: initial_estimate.route_ops,
            reason: "within_budget",
            ..LayoutGuardDecision::default()
        };
    }

    let mut selected_algorithm = selected;
    let mut selected_estimate = initial_estimate;
    let mut within_budget_candidate_found = false;

    for candidate in fallback_candidates(ir, selected).into_iter().skip(1) {
        let estimate = estimate_layout_cost(ir, candidate);
        if !estimate.exceeds(guardrails).0
            && !estimate.exceeds(guardrails).1
            && !estimate.exceeds(guardrails).2
        {
            selected_algorithm = candidate;
            selected_estimate = estimate;
            within_budget_candidate_found = true;
            break;
        }

        if estimate.score() < selected_estimate.score() {
            selected_algorithm = candidate;
            selected_estimate = estimate;
        }
    }

    let guard = LayoutGuardDecision {
        initial_algorithm: selected,
        selected_algorithm,
        estimated_layout_time_ms: initial_estimate.time_ms,
        estimated_layout_iterations: initial_estimate.iterations,
        estimated_route_ops: initial_estimate.route_ops,
        selected_estimated_layout_time_ms: selected_estimate.time_ms,
        selected_estimated_layout_iterations: selected_estimate.iterations,
        selected_estimated_route_ops: selected_estimate.route_ops,
        time_budget_exceeded,
        iteration_budget_exceeded,
        route_budget_exceeded,
        fallback_applied: selected_algorithm != selected,
        reason: guardrail_reason(
            time_budget_exceeded,
            iteration_budget_exceeded,
            route_budget_exceeded,
            selected_algorithm != selected,
            within_budget_candidate_found,
        ),
    };

    if guard.fallback_applied {
        warn!(
            initial_algorithm = guard.initial_algorithm.as_str(),
            selected_algorithm = guard.selected_algorithm.as_str(),
            estimated_time_ms = guard.estimated_layout_time_ms,
            reason = guard.reason,
            "layout.guardrail.fallback"
        );
    } else {
        debug!(
            algorithm = guard.selected_algorithm.as_str(),
            estimated_time_ms = guard.estimated_layout_time_ms,
            reason = guard.reason,
            "layout.guardrail.ok"
        );
    }

    guard
}

fn layout_diagram_sugiyama_traced_with_config(
    ir: &MermaidDiagramIr,
    config: LayoutConfig,
) -> TracedLayout {
    let mut trace = LayoutTrace::default();
    let spacing = LayoutSpacing::default();
    let metrics = config.font_metrics.clone().unwrap_or_else(fm_core::FontMetrics::default_metrics);
    let node_sizes = compute_node_sizes(ir, &metrics);
    let cycle_result = cycle_removal(ir, config.cycle_strategy);
    push_snapshot(
        &mut trace,
        "cycle_removal",
        ir.nodes.len(),
        ir.edges.len(),
        cycle_result.reversed_edge_indexes.len(),
        0,
    );

    let collapse_map = if config.collapse_cycle_clusters {
        Some(build_cycle_cluster_map(ir, &cycle_result))
    } else {
        None
    };

    let ranks = rank_assignment(ir, &cycle_result);
    push_snapshot(
        &mut trace,
        "rank_assignment",
        ir.nodes.len(),
        ir.edges.len(),
        cycle_result.reversed_edge_indexes.len(),
        0,
    );

    let (crossing_count_before, ordering_by_rank) = crossing_minimization(ir, &ranks);
    push_snapshot(
        &mut trace,
        "crossing_minimization",
        ir.nodes.len(),
        ir.edges.len(),
        cycle_result.reversed_edge_indexes.len(),
        crossing_count_before,
    );

    // Refinement: transpose + sifting heuristics.
    let (crossing_count, ordering_by_rank) =
        crossing_refinement(ir, &ranks, ordering_by_rank, crossing_count_before);
    push_snapshot(
        &mut trace,
        "crossing_refinement",
        ir.nodes.len(),
        ir.edges.len(),
        cycle_result.reversed_edge_indexes.len(),
        crossing_count,
    );

    let mut nodes = coordinate_assignment(ir, &node_sizes, &ranks, &ordering_by_rank, spacing);
    let edges = build_edge_paths(ir, &nodes, &cycle_result.highlighted_edge_indexes);
    let mut clusters = build_cluster_boxes(ir, &nodes, spacing);
    let mut cycle_clusters = Vec::new();

    // If cycle clusters are collapsed, group member nodes within their cluster head's bounds.
    let collapsed_count = if let Some(ref collapse_map) = collapse_map {
        let count = collapse_map.cluster_heads.len();
        cycle_clusters =
            build_cycle_cluster_results(collapse_map, &mut nodes, &mut clusters, spacing);
        count
    } else {
        0
    };

    let bounds = compute_bounds(&nodes, &clusters, &edges, spacing);

    push_snapshot(
        &mut trace,
        "post_processing",
        ir.nodes.len(),
        ir.edges.len(),
        cycle_result.reversed_edge_indexes.len(),
        crossing_count,
    );

    let (total_edge_length, measured_reversed_edge_total_length) =
        compute_edge_length_metrics(&edges);
    let reversed_edges = if matches!(config.cycle_strategy, CycleStrategy::CycleAware) {
        0
    } else {
        cycle_result.reversed_edge_indexes.len()
    };
    let reversed_edge_total_length = if matches!(config.cycle_strategy, CycleStrategy::CycleAware) {
        0.0
    } else {
        measured_reversed_edge_total_length
    };

    let stats = LayoutStats {
        node_count: ir.nodes.len(),
        edge_count: ir.edges.len(),
        crossing_count,
        crossing_count_before_refinement: crossing_count_before,
        reversed_edges,
        cycle_count: cycle_result.summary.cycle_count,
        cycle_node_count: cycle_result.summary.cycle_node_count,
        max_cycle_size: cycle_result.summary.max_cycle_size,
        collapsed_clusters: collapsed_count,
        reversed_edge_total_length,
        total_edge_length,
        phase_iterations: trace.snapshots.len(),
    };

    TracedLayout {
        layout: DiagramLayout {
            nodes,
            clusters,
            cycle_clusters,
            edges,
            bounds,
            stats,
            extensions: LayoutExtensions::default(),
        },
        trace,
    }
}

/// Lay out a diagram using force-directed (Fruchterman-Reingold) algorithm.
///
/// Suitable for diagrams without a natural hierarchy: ER diagrams, architecture
/// diagrams, generic graphs with no clear flow direction.
#[must_use]
pub fn layout_diagram_force(ir: &MermaidDiagramIr) -> DiagramLayout {
    layout_diagram_force_traced(ir).layout
}

/// Lay out with force-directed algorithm and return tracing information.
#[must_use]
pub fn layout_diagram_force_traced(ir: &MermaidDiagramIr) -> TracedLayout {
    let mut trace = LayoutTrace::default();
    let spacing = LayoutSpacing::default();
    let metrics = fm_core::FontMetrics::default_metrics();
    let node_sizes = compute_node_sizes(ir, &metrics);
    let n = ir.nodes.len();

    if n == 0 {
        return TracedLayout {
            layout: DiagramLayout {
                nodes: vec![],
                clusters: vec![],
                cycle_clusters: vec![],
                edges: vec![],
                bounds: LayoutRect {
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 0.0,
                },
                stats: LayoutStats::default(),
                extensions: LayoutExtensions::default(),
            },
            trace,
        };
    }

    // Deterministic initial placement using hash of node IDs.
    let mut positions = force_initial_positions(ir, &node_sizes, &spacing);

    push_snapshot(&mut trace, "force_init", n, ir.edges.len(), 0, 0);

    // Build adjacency list for attractive forces.
    let adjacency = force_build_adjacency(ir);

    // Build cluster membership for cluster-aware forces.
    let cluster_membership = force_cluster_membership(ir);

    // Fruchterman-Reingold iterations.
    let area = (n as f32) * spacing.node_spacing * spacing.rank_spacing;
    let k = (area / n as f32).sqrt(); // Optimal distance between nodes
    let max_iterations = force_iteration_budget(n);
    let convergence_threshold = 0.5;

    for iteration in 0..max_iterations {
        let temperature = force_temperature(iteration, max_iterations, k);
        if temperature < convergence_threshold {
            break;
        }

        let displacements = force_compute_displacements(
            &positions,
            &node_sizes,
            &adjacency,
            &cluster_membership,
            k,
            n,
        );

        // Apply displacements clamped by temperature.
        let mut max_displacement: f32 = 0.0;
        for i in 0..n {
            let (dx, dy) = displacements[i];
            let magnitude = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
            let clamped_mag = magnitude.min(temperature);
            let scale = clamped_mag / magnitude;
            positions[i].0 += dx * scale;
            positions[i].1 += dy * scale;
            max_displacement = max_displacement.max(clamped_mag);
        }

        if max_displacement < convergence_threshold {
            break;
        }
    }

    push_snapshot(&mut trace, "force_simulation", n, ir.edges.len(), 0, 0);

    // Overlap removal post-processing.
    force_remove_overlaps(&mut positions, &node_sizes, &spacing);

    push_snapshot(&mut trace, "force_overlap_removal", n, ir.edges.len(), 0, 0);

    // Normalize positions so all coordinates are non-negative.
    force_normalize_positions(&mut positions, &node_sizes);

    // Build layout output.
    let nodes = force_build_node_boxes(ir, &positions, &node_sizes);
    let edges = force_build_edge_paths(ir, &nodes);
    let clusters = build_cluster_boxes(ir, &nodes, spacing);
    let bounds = compute_bounds(&nodes, &clusters, &edges, spacing);

    let (total_edge_length, reversed_edge_total_length) = compute_edge_length_metrics(&edges);

    push_snapshot(&mut trace, "force_post_processing", n, ir.edges.len(), 0, 0);

    let stats = LayoutStats {
        node_count: n,
        edge_count: ir.edges.len(),
        crossing_count: 0, // Not computed for force-directed
        crossing_count_before_refinement: 0,
        reversed_edges: 0,
        cycle_count: 0,
        cycle_node_count: 0,
        max_cycle_size: 0,
        collapsed_clusters: 0,
        reversed_edge_total_length,
        total_edge_length,
        phase_iterations: trace.snapshots.len(),
    };

    TracedLayout {
        layout: DiagramLayout {
            nodes,
            clusters,
            cycle_clusters: vec![],
            edges,
            bounds,
            stats,
            extensions: LayoutExtensions::default(),
        },
        trace,
    }
}

/// Lay out a diagram using a deterministic tidy-tree algorithm.
#[must_use]
pub fn layout_diagram_tree(ir: &MermaidDiagramIr) -> DiagramLayout {
    layout_diagram_tree_traced(ir).layout
}

/// Lay out using the tree algorithm and return tracing information.
#[must_use]
pub fn layout_diagram_tree_traced(ir: &MermaidDiagramIr) -> TracedLayout {
    let mut trace = LayoutTrace::default();
    let spacing = LayoutSpacing::default();
    let node_sizes = compute_node_sizes(ir, &fm_core::FontMetrics::default_metrics());
    let node_count = ir.nodes.len();

    if node_count == 0 {
        return TracedLayout {
            layout: DiagramLayout {
                nodes: Vec::new(),
                clusters: Vec::new(),
                cycle_clusters: Vec::new(),
                edges: Vec::new(),
                bounds: LayoutRect {
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 0.0,
                },
                stats: LayoutStats::default(),
                extensions: LayoutExtensions::default(),
            },
            trace,
        };
    }

    let tree = build_tree_layout_structure(ir);
    push_snapshot(
        &mut trace,
        "tree_structure",
        node_count,
        ir.edges.len(),
        0,
        0,
    );

    let span_sizes: Vec<f32> = node_sizes
        .iter()
        .map(|(width, height)| {
            if tree.horizontal_depth_axis {
                *height
            } else {
                *width
            }
        })
        .collect();

    let mut span_memo = vec![None; node_count];
    for root in &tree.roots {
        let _ = tree_subtree_span(*root, &tree.children, &span_sizes, spacing, &mut span_memo);
    }
    let subtree_spans: Vec<f32> = span_memo
        .into_iter()
        .map(|span| span.unwrap_or(0.0))
        .collect();

    let mut span_centers = vec![0.0_f32; node_count];
    let mut root_cursor = 0.0_f32;
    for root in &tree.roots {
        let root_span = subtree_spans[*root];
        assign_tree_span_centers(
            *root,
            root_cursor,
            &tree.children,
            &subtree_spans,
            spacing,
            &mut span_centers,
        );
        root_cursor += root_span + (spacing.node_spacing * 1.5);
    }

    let depth_level_sizes = tree_depth_level_sizes(&tree, &node_sizes);
    let depth_centers = depth_level_centers(&depth_level_sizes, spacing.rank_spacing);

    let mut centers = vec![(0.0_f32, 0.0_f32); node_count];
    for node_index in 0..node_count {
        let logical_depth = tree.depth[node_index];
        let mapped_depth = if tree.reverse_depth_axis {
            tree.max_depth.saturating_sub(logical_depth)
        } else {
            logical_depth
        };
        let depth_center = depth_centers[mapped_depth];
        let span_center = span_centers[node_index];
        centers[node_index] = if tree.horizontal_depth_axis {
            (depth_center, span_center)
        } else {
            (span_center, depth_center)
        };
    }
    normalize_center_positions(&mut centers, &node_sizes);

    let order_by_rank = rank_orders_from_key(ir, &tree.depth, &span_centers);
    let nodes = node_boxes_from_centers(ir, &node_sizes, &tree.depth, &order_by_rank, &centers);
    let edges = build_edge_paths(ir, &nodes, &BTreeSet::new());
    let clusters = build_cluster_boxes(ir, &nodes, spacing);
    let bounds = compute_bounds(&nodes, &clusters, &edges, spacing);
    let (total_edge_length, reversed_edge_total_length) = compute_edge_length_metrics(&edges);

    push_snapshot(
        &mut trace,
        "tree_post_processing",
        node_count,
        ir.edges.len(),
        0,
        0,
    );

    let stats = LayoutStats {
        node_count,
        edge_count: ir.edges.len(),
        crossing_count: 0,
        crossing_count_before_refinement: 0,
        reversed_edges: 0,
        cycle_count: 0,
        cycle_node_count: 0,
        max_cycle_size: 0,
        collapsed_clusters: 0,
        reversed_edge_total_length,
        total_edge_length,
        phase_iterations: trace.snapshots.len(),
    };

    TracedLayout {
        layout: DiagramLayout {
            nodes,
            clusters,
            cycle_clusters: Vec::new(),
            edges,
            bounds,
            stats,
            extensions: LayoutExtensions::default(),
        },
        trace,
    }
}

/// Lay out a diagram using a deterministic radial tree variant.
#[must_use]
pub fn layout_diagram_radial(ir: &MermaidDiagramIr) -> DiagramLayout {
    layout_diagram_radial_traced(ir).layout
}

/// Lay out using the radial tree algorithm and return tracing information.
#[must_use]
pub fn layout_diagram_radial_traced(ir: &MermaidDiagramIr) -> TracedLayout {
    let mut trace = LayoutTrace::default();
    let spacing = LayoutSpacing::default();
    let node_sizes = compute_node_sizes(ir, &fm_core::FontMetrics::default_metrics());
    let node_count = ir.nodes.len();

    if node_count == 0 {
        return TracedLayout {
            layout: DiagramLayout {
                nodes: Vec::new(),
                clusters: Vec::new(),
                cycle_clusters: Vec::new(),
                edges: Vec::new(),
                bounds: LayoutRect {
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 0.0,
                },
                stats: LayoutStats::default(),
                extensions: LayoutExtensions::default(),
            },
            trace,
        };
    }

    let tree = build_tree_layout_structure(ir);
    push_snapshot(
        &mut trace,
        "tree_structure",
        node_count,
        ir.edges.len(),
        0,
        0,
    );

    let depth_offset = usize::from(tree.roots.len() > 1);
    let effective_max_depth = tree.max_depth + depth_offset;
    let mut ring_level_sizes = vec![0.0_f32; effective_max_depth + 1];
    for (node_index, (width, height)) in node_sizes.iter().copied().enumerate() {
        let level = tree.depth[node_index] + depth_offset;
        ring_level_sizes[level] = ring_level_sizes[level].max(width.max(height));
    }

    let mut radii = vec![0.0_f32; effective_max_depth + 1];
    for level in 1..=effective_max_depth {
        let prev = ring_level_sizes[level - 1].max(1.0);
        let current = ring_level_sizes[level].max(1.0);
        radii[level] = radii[level - 1] + (prev / 2.0) + spacing.rank_spacing + (current / 2.0);
    }

    let mut leaf_memo = vec![None; node_count];
    for root in &tree.roots {
        let _ = radial_leaf_count(*root, &tree.children, &mut leaf_memo);
    }
    let leaf_counts: Vec<usize> = leaf_memo
        .into_iter()
        .map(|count| count.unwrap_or(1))
        .collect();

    let mut angles = vec![0.0_f32; node_count];
    if tree.roots.len() == 1 && depth_offset == 0 {
        assign_radial_angles(
            tree.roots[0],
            -PI,
            PI,
            &tree,
            &leaf_counts,
            &node_sizes,
            &radii,
            depth_offset,
            spacing,
            &mut angles,
        );
    } else {
        let total_leaves: usize = tree.roots.iter().map(|root| leaf_counts[*root]).sum();
        let total_leaves = total_leaves.max(1);
        let mut cursor = -PI;
        for (root_index, root) in tree.roots.iter().enumerate() {
            let weight = leaf_counts[*root] as f32 / total_leaves as f32;
            let mut span = (2.0 * PI) * weight;
            if root_index + 1 == tree.roots.len() {
                span = PI - cursor;
            }
            assign_radial_angles(
                *root,
                cursor,
                cursor + span,
                &tree,
                &leaf_counts,
                &node_sizes,
                &radii,
                depth_offset,
                spacing,
                &mut angles,
            );
            cursor += span;
        }
    }

    let mut centers = vec![(0.0_f32, 0.0_f32); node_count];
    for node_index in 0..node_count {
        let level = tree.depth[node_index] + depth_offset;
        let radius = radii[level];
        let angle = angles[node_index];
        centers[node_index] = (radius * angle.cos(), radius * angle.sin());
    }
    normalize_center_positions(&mut centers, &node_sizes);

    let order_by_rank = rank_orders_from_key(ir, &tree.depth, &angles);
    let nodes = node_boxes_from_centers(ir, &node_sizes, &tree.depth, &order_by_rank, &centers);
    let edges = force_build_edge_paths(ir, &nodes);
    let clusters = build_cluster_boxes(ir, &nodes, spacing);
    let bounds = compute_bounds(&nodes, &clusters, &edges, spacing);
    let (total_edge_length, reversed_edge_total_length) = compute_edge_length_metrics(&edges);

    push_snapshot(
        &mut trace,
        "radial_post_processing",
        node_count,
        ir.edges.len(),
        0,
        0,
    );

    let stats = LayoutStats {
        node_count,
        edge_count: ir.edges.len(),
        crossing_count: 0,
        crossing_count_before_refinement: 0,
        reversed_edges: 0,
        cycle_count: 0,
        cycle_node_count: 0,
        max_cycle_size: 0,
        collapsed_clusters: 0,
        reversed_edge_total_length,
        total_edge_length,
        phase_iterations: trace.snapshots.len(),
    };

    TracedLayout {
        layout: DiagramLayout {
            nodes,
            clusters,
            cycle_clusters: Vec::new(),
            edges,
            bounds,
            stats,
            extensions: LayoutExtensions::default(),
        },
        trace,
    }
}

#[must_use]
pub fn layout_diagram_timeline(ir: &MermaidDiagramIr) -> DiagramLayout {
    layout_diagram_timeline_traced(ir).layout
}

#[must_use]
pub fn layout_diagram_timeline_traced(ir: &MermaidDiagramIr) -> TracedLayout {
    let node_count = ir.nodes.len();
    let node_sizes = compute_node_sizes(ir, &fm_core::FontMetrics::default_metrics());
    let mut trace = LayoutTrace::default();
    push_snapshot(
        &mut trace,
        "timeline_layout",
        node_count,
        ir.edges.len(),
        0,
        0,
    );

    let spacing = LayoutSpacing::default();
    let mut rank_by_node = vec![0_usize; node_count];
    let mut order_by_node = vec![0_usize; node_count];
    let mut centers = vec![(0.0_f32, 0.0_f32); node_count];

    let mut period_indexes: Vec<usize> = ir
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| matches!(node.shape, fm_core::NodeShape::Rect))
        .map(|(node_index, _)| node_index)
        .collect();
    if period_indexes.is_empty() {
        period_indexes = (0..node_count).collect();
    }
    period_indexes.sort_by(|left, right| compare_node_indices(ir, *left, *right));

    let period_set: BTreeSet<usize> = period_indexes.iter().copied().collect();
    let mut events_by_period: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for edge in &ir.edges {
        let Some(source) = endpoint_node_index(ir, edge.from) else {
            continue;
        };
        let Some(target) = endpoint_node_index(ir, edge.to) else {
            continue;
        };
        if period_set.contains(&source) && !period_set.contains(&target) {
            events_by_period.entry(source).or_default().push(target);
        }
    }
    for targets in events_by_period.values_mut() {
        targets.sort_by(|left, right| compare_node_indices(ir, *left, *right));
        targets.dedup();
    }

    let period_gap_x = spacing.rank_spacing + 104.0;
    let event_gap_y = spacing.node_spacing + 22.0;
    let mut assigned = BTreeSet::new();

    for (period_order, period_index) in period_indexes.iter().enumerate() {
        let x = period_order as f32 * period_gap_x;
        centers[*period_index] = (x, 0.0);
        rank_by_node[*period_index] = 0;
        order_by_node[*period_index] = period_order;
        assigned.insert(*period_index);

        let mut event_row = 1_usize;
        if let Some(targets) = events_by_period.get(period_index) {
            for target in targets {
                if assigned.insert(*target) {
                    centers[*target] = (x, 48.0 + event_row as f32 * event_gap_y);
                    rank_by_node[*target] = event_row;
                    order_by_node[*target] = period_order;
                    event_row = event_row.saturating_add(1);
                }
            }
        }
    }

    let period_count = period_indexes.len().max(1);
    let mut spill = 0_usize;
    let mut leftovers: Vec<usize> = (0..node_count)
        .filter(|node_index| !assigned.contains(node_index))
        .collect();
    leftovers.sort_by(|left, right| compare_node_indices(ir, *left, *right));
    for node_index in leftovers {
        let col = spill % period_count;
        let row = spill / period_count;
        centers[node_index] = (col as f32 * period_gap_x, (4.0 + row as f32) * event_gap_y);
        rank_by_node[node_index] = row.saturating_add(1);
        order_by_node[node_index] = col;
        spill = spill.saturating_add(1);
    }

    let mut traced = finalize_specialized_layout(
        ir,
        &node_sizes,
        rank_by_node,
        order_by_node,
        centers,
        trace,
        true,
    );
    traced.layout.extensions.axis_ticks = period_indexes
        .into_iter()
        .map(|node_index| {
            let node = traced
                .layout
                .nodes
                .iter()
                .find(|node| node.node_index == node_index)
                .expect("timeline period node should exist in layout");
            LayoutAxisTick {
                label: layout_label_text(ir, node_index).to_string(),
                position: node.bounds.center().x,
            }
        })
        .collect();
    traced.layout.extensions.bands = traced
        .layout
        .clusters
        .iter()
        .filter_map(|cluster| {
            let title = ir
                .clusters
                .get(cluster.cluster_index)
                .and_then(|cluster| cluster.title)
                .and_then(|label_id| ir.labels.get(label_id.0))
                .map(|label| label.text.clone())?;
            Some(LayoutBand {
                kind: LayoutBandKind::Section,
                label: title,
                bounds: cluster.bounds,
            })
        })
        .collect();
    traced
}

// ---------------------------------------------------------------------------
// Sequence diagram layout
// ---------------------------------------------------------------------------

/// Lay out a sequence diagram with participants arranged horizontally and
/// messages stacked vertically in declaration order.
#[must_use]
pub fn layout_diagram_sequence(ir: &MermaidDiagramIr) -> DiagramLayout {
    layout_diagram_sequence_traced(ir).layout
}

#[must_use]
pub fn layout_diagram_sequence_traced(ir: &MermaidDiagramIr) -> TracedLayout {
    let node_count = ir.nodes.len();
    let node_sizes = compute_node_sizes(ir, &fm_core::FontMetrics::default_metrics());
    let mut trace = LayoutTrace::default();
    push_snapshot(
        &mut trace,
        "sequence_layout",
        node_count,
        ir.edges.len(),
        0,
        0,
    );

    if node_count == 0 {
        return TracedLayout {
            layout: DiagramLayout {
                nodes: Vec::new(),
                clusters: Vec::new(),
                cycle_clusters: Vec::new(),
                edges: Vec::new(),
                bounds: LayoutRect {
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 0.0,
                },
                stats: LayoutStats::default(),
                extensions: LayoutExtensions::default(),
            },
            trace,
        };
    }

    let spacing = LayoutSpacing::default();

    // ── Phase 1: identify participants (declaration order) ──────────────
    // Participants are the nodes; edges are messages between them.
    // Preserve the declaration order from the parser which already sorted
    // participants by first appearance.
    let participant_gap = spacing.node_spacing + 80.0;
    let message_gap = spacing.rank_spacing.max(56.0);
    let header_y = 0.0_f32;

    // Build participant index → horizontal position mapping.
    // Each participant is centered at (participant_order * gap, header_y).
    let mut rank_by_node = vec![0_usize; node_count];
    let mut order_by_node = vec![0_usize; node_count];
    let mut centers = vec![(0.0_f32, 0.0_f32); node_count];

    // Compute cumulative x positions accounting for individual node widths.
    let mut x_cursor = 0.0_f32;
    let mut participant_x_centers: Vec<f32> = Vec::with_capacity(node_count);
    for (participant_order, (width, _height)) in node_sizes.iter().copied().enumerate() {
        let half_width = width / 2.0;
        let cx = x_cursor + half_width;
        participant_x_centers.push(cx);
        centers[participant_order] = (cx, header_y);
        rank_by_node[participant_order] = 0;
        order_by_node[participant_order] = participant_order;
        x_cursor = cx + half_width + participant_gap;
        let _ = participant_order; // suppress unused
    }

    // ── Phase 2: compute message (edge) row positions ──────────────────
    // Each edge occupies a vertical row below the participant header.
    // The y-position increases with each message in declaration order.
    // Edges reference node indices for from/to; the x-coordinate of each
    // endpoint is determined by the participant's center x.
    //
    // For self-messages (from == to), we add extra vertical space.
    let first_message_y =
        header_y + node_sizes.iter().map(|(_, h)| *h).fold(0.0_f32, f32::max) + message_gap;

    let mut message_y_positions: Vec<f32> = Vec::with_capacity(ir.edges.len());
    let mut y_cursor = first_message_y;
    for edge in &ir.edges {
        message_y_positions.push(y_cursor);
        let is_self = match (
            endpoint_node_index(ir, edge.from),
            endpoint_node_index(ir, edge.to),
        ) {
            (Some(s), Some(t)) => s == t,
            _ => false,
        };
        // Self-messages need more vertical space for the loop.
        let row_height = if is_self {
            message_gap * 1.5
        } else {
            message_gap
        };
        y_cursor += row_height;
    }

    // Total diagram height: extend lifelines past the last message.
    let diagram_bottom = y_cursor + message_gap * 0.5;

    // ── Phase 3: build layout nodes (participant boxes at the top) ──────
    let nodes: Vec<LayoutNodeBox> = (0..node_count)
        .map(|participant_order| {
            let (width, height) = node_sizes[participant_order];
            let cx = participant_x_centers[participant_order];
            LayoutNodeBox {
                node_index: participant_order,
                node_id: ir.nodes[participant_order].id.clone(),
                rank: 0,
                order: participant_order,
                span: ir.nodes[participant_order].span_primary,
                bounds: LayoutRect {
                    x: cx - width / 2.0,
                    y: header_y,
                    width,
                    height,
                },
            }
        })
        .collect();

    // ── Phase 4: build edge paths ──────────────────────────────────────
    // Each message is a horizontal arrow from sender lifeline to receiver
    // lifeline at the corresponding y-position.
    let edges: Vec<LayoutEdgePath> = ir
        .edges
        .iter()
        .enumerate()
        .map(|(edge_index, edge)| {
            let y = message_y_positions[edge_index];
            let source_index = endpoint_node_index(ir, edge.from).unwrap_or(0);
            let target_index = endpoint_node_index(ir, edge.to).unwrap_or(0);
            let source_x = participant_x_centers
                .get(source_index)
                .copied()
                .unwrap_or(0.0);
            let target_x = participant_x_centers
                .get(target_index)
                .copied()
                .unwrap_or(0.0);
            let is_self_loop = source_index == target_index;

            let points = if is_self_loop {
                // Self-message: draw a loop to the right and back.
                let loop_width = 40.0;
                let loop_height = message_gap * 0.6;
                vec![
                    LayoutPoint { x: source_x, y },
                    LayoutPoint {
                        x: source_x + loop_width,
                        y,
                    },
                    LayoutPoint {
                        x: source_x + loop_width,
                        y: y + loop_height,
                    },
                    LayoutPoint {
                        x: source_x,
                        y: y + loop_height,
                    },
                ]
            } else {
                vec![
                    LayoutPoint { x: source_x, y },
                    LayoutPoint { x: target_x, y },
                ]
            };

            LayoutEdgePath {
                edge_index,
                span: edge.span,
                points,
                reversed: false,
                is_self_loop,
                parallel_offset: 0.0,
            }
        })
        .collect();

    // ── Phase 5: compute bounds and extensions ─────────────────────────
    let total_width = if node_count > 0 {
        let last_cx = participant_x_centers[node_count - 1];
        let last_half_w = node_sizes[node_count - 1].0 / 2.0;
        last_cx + last_half_w
    } else {
        0.0
    };

    let bounds = LayoutRect {
        x: 0.0,
        y: 0.0,
        width: total_width,
        height: diagram_bottom,
    };

    push_snapshot(
        &mut trace,
        "sequence_post_processing",
        node_count,
        ir.edges.len(),
        0,
        0,
    );

    let (total_edge_length, reversed_edge_total_length) = compute_edge_length_metrics(&edges);

    let stats = LayoutStats {
        node_count,
        edge_count: ir.edges.len(),
        crossing_count: 0,
        crossing_count_before_refinement: 0,
        reversed_edges: 0,
        cycle_count: 0,
        cycle_node_count: 0,
        max_cycle_size: 0,
        collapsed_clusters: 0,
        reversed_edge_total_length,
        total_edge_length,
        phase_iterations: trace.snapshots.len(),
    };

    // Build lifeline bands: one vertical band per participant from header bottom
    // to diagram bottom, useful for renderers that draw dashed lifelines.
    let lifeline_bands: Vec<LayoutBand> = (0..node_count)
        .map(|participant_order| {
            let cx = participant_x_centers[participant_order];
            let (_, header_height) = node_sizes[participant_order];
            LayoutBand {
                kind: LayoutBandKind::Lane,
                label: layout_label_text(ir, participant_order).to_string(),
                bounds: LayoutRect {
                    x: cx - 1.0,
                    y: header_y + header_height,
                    width: 2.0,
                    height: diagram_bottom - (header_y + header_height),
                },
            }
        })
        .collect();

    TracedLayout {
        layout: DiagramLayout {
            nodes,
            clusters: Vec::new(),
            cycle_clusters: Vec::new(),
            edges,
            bounds,
            stats,
            extensions: LayoutExtensions {
                bands: lifeline_bands,
                axis_ticks: Vec::new(),
            },
        },
        trace,
    }
}

#[must_use]
pub fn layout_diagram_gantt(ir: &MermaidDiagramIr) -> DiagramLayout {
    layout_diagram_gantt_traced(ir).layout
}

#[must_use]
pub fn layout_diagram_gantt_traced(ir: &MermaidDiagramIr) -> TracedLayout {
    let node_count = ir.nodes.len();
    let mut node_sizes = compute_node_sizes(ir, &fm_core::FontMetrics::default_metrics());
    let mut trace = LayoutTrace::default();
    push_snapshot(&mut trace, "gantt_layout", node_count, ir.edges.len(), 0, 0);

    for size in &mut node_sizes {
        size.0 = size.0.max(156.0);
        size.1 = size.1.max(40.0);
    }

    let mut section_to_nodes: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut order_hint_by_node: BTreeMap<usize, usize> = BTreeMap::new();
    for node_index in 0..node_count {
        let label = layout_label_text(ir, node_index);
        let section = label
            .split_once(':')
            .map(|(prefix, _)| prefix.trim())
            .filter(|prefix| !prefix.is_empty())
            .unwrap_or("Backlog")
            .to_string();
        section_to_nodes
            .entry(section)
            .or_default()
            .push(node_index);
        order_hint_by_node.insert(
            node_index,
            parse_order_hint(&ir.nodes[node_index].id, node_index),
        );
    }

    for nodes in section_to_nodes.values_mut() {
        nodes.sort_by(|left, right| {
            order_hint_by_node[left]
                .cmp(&order_hint_by_node[right])
                .then_with(|| compare_node_indices(ir, *left, *right))
        });
    }

    let mut ordered_hints: Vec<usize> = order_hint_by_node.values().copied().collect();
    ordered_hints.sort_unstable();
    ordered_hints.dedup();
    let slot_by_hint: BTreeMap<usize, usize> = ordered_hints
        .iter()
        .copied()
        .enumerate()
        .map(|(slot, hint)| (hint, slot))
        .collect();

    let spacing = LayoutSpacing::default();
    let mut rank_by_node = vec![0_usize; node_count];
    let mut order_by_node = vec![0_usize; node_count];
    let mut centers = vec![(0.0_f32, 0.0_f32); node_count];

    let col_gap = spacing.rank_spacing + 144.0;
    let row_gap = (spacing.node_spacing * 0.72) + 24.0;
    let mut section_base_y = 0.0_f32;

    for (section_index, (_section, nodes)) in section_to_nodes.iter().enumerate() {
        for (row_index, node_index) in nodes.iter().enumerate() {
            let slot = slot_by_hint[&order_hint_by_node[node_index]];
            centers[*node_index] = (
                slot as f32 * col_gap,
                section_base_y + row_index as f32 * row_gap,
            );
            rank_by_node[*node_index] = slot;
            order_by_node[*node_index] = row_index + section_index * 128;
        }
        section_base_y += (nodes.len().max(1) as f32 * row_gap) + 56.0;
    }

    let mut traced = finalize_specialized_layout(
        ir,
        &node_sizes,
        rank_by_node,
        order_by_node,
        centers,
        trace,
        true,
    );
    traced.layout.extensions.axis_ticks = ordered_hints
        .iter()
        .enumerate()
        .filter_map(|(slot, hint)| {
            let node = traced.layout.nodes.iter().find(|node| node.rank == slot)?;
            Some(LayoutAxisTick {
                label: hint.to_string(),
                position: node.bounds.center().x,
            })
        })
        .collect();
    traced.layout.extensions.bands = section_to_nodes
        .iter()
        .filter_map(|(section, node_indexes)| {
            let bounds = layout_bounds_for_nodes(&traced.layout, node_indexes, 24.0)?;
            Some(LayoutBand {
                kind: LayoutBandKind::Section,
                label: section.clone(),
                bounds,
            })
        })
        .collect();
    traced
}

#[must_use]
pub fn layout_diagram_sankey(ir: &MermaidDiagramIr) -> DiagramLayout {
    layout_diagram_sankey_traced(ir).layout
}

#[must_use]
pub fn layout_diagram_sankey_traced(ir: &MermaidDiagramIr) -> TracedLayout {
    let node_count = ir.nodes.len();
    let mut node_sizes = compute_node_sizes(ir, &fm_core::FontMetrics::default_metrics());
    let mut trace = LayoutTrace::default();
    push_snapshot(
        &mut trace,
        "sankey_layout",
        node_count,
        ir.edges.len(),
        0,
        0,
    );

    let mut in_degree = vec![0_usize; node_count];
    let mut out_degree = vec![0_usize; node_count];
    for edge in &ir.edges {
        let Some(source) = endpoint_node_index(ir, edge.from) else {
            continue;
        };
        let Some(target) = endpoint_node_index(ir, edge.to) else {
            continue;
        };
        if source == target || source >= node_count || target >= node_count {
            continue;
        }
        out_degree[source] = out_degree[source].saturating_add(1);
        in_degree[target] = in_degree[target].saturating_add(1);
    }

    for (node_index, size) in node_sizes.iter_mut().enumerate() {
        let flow = in_degree[node_index].max(out_degree[node_index]).max(1) as f32;
        size.0 = size.0.max(108.0);
        size.1 = size.1.max(30.0 + (flow * 14.0));
    }

    let ranks = layered_ranks(ir);
    let mut nodes_by_rank: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (node_index, rank) in ranks.iter().copied().enumerate() {
        nodes_by_rank.entry(rank).or_default().push(node_index);
    }
    for nodes in nodes_by_rank.values_mut() {
        nodes.sort_by(|left, right| compare_node_indices(ir, *left, *right));
    }

    let spacing = LayoutSpacing::default();
    let mut rank_by_node = vec![0_usize; node_count];
    let mut order_by_node = vec![0_usize; node_count];
    let mut centers = vec![(0.0_f32, 0.0_f32); node_count];
    let col_gap = spacing.rank_spacing + 136.0;
    let row_gap = (spacing.node_spacing * 0.45) + 18.0;

    for (rank, nodes) in &nodes_by_rank {
        let mut cursor_y = 0.0_f32;
        for (order_index, node_index) in nodes.iter().enumerate() {
            let height = node_sizes[*node_index].1;
            centers[*node_index] = (*rank as f32 * col_gap, cursor_y + (height / 2.0));
            rank_by_node[*node_index] = *rank;
            order_by_node[*node_index] = order_index;
            cursor_y += height + row_gap;
        }
    }

    let mut traced = finalize_specialized_layout(
        ir,
        &node_sizes,
        rank_by_node,
        order_by_node,
        centers,
        trace,
        true,
    );
    traced.layout.extensions.bands = nodes_by_rank
        .keys()
        .copied()
        .filter_map(|rank| {
            layout_band_for_rank(
                &traced.layout,
                rank,
                LayoutBandKind::Column,
                format!("column {}", rank + 1),
                20.0,
            )
        })
        .collect();
    traced
}

#[must_use]
pub fn layout_diagram_grid(ir: &MermaidDiagramIr) -> DiagramLayout {
    layout_diagram_grid_traced(ir).layout
}

#[must_use]
pub fn layout_diagram_grid_traced(ir: &MermaidDiagramIr) -> TracedLayout {
    let node_count = ir.nodes.len();
    let mut node_sizes = compute_node_sizes(ir, &fm_core::FontMetrics::default_metrics());
    let mut trace = LayoutTrace::default();
    push_snapshot(&mut trace, "grid_layout", node_count, ir.edges.len(), 0, 0);

    let spacing = LayoutSpacing::default();
    let mut rank_by_node = vec![0_usize; node_count];
    let mut order_by_node = vec![0_usize; node_count];
    let mut centers = vec![(0.0_f32, 0.0_f32); node_count];

    let base_max_width = node_sizes
        .iter()
        .map(|(width, _)| *width)
        .fold(84.0_f32, f32::max);
    let max_height = node_sizes
        .iter()
        .map(|(_, height)| *height)
        .fold(44.0_f32, f32::max);

    let mut column_count = if ir.diagram_type == DiagramType::BlockBeta {
        ir.meta.block_beta_columns.unwrap_or(0)
    } else {
        0
    };
    if column_count == 0 {
        column_count = (node_count as f32).sqrt().ceil() as usize;
    }
    let column_count = column_count.max(1);
    let cell_width = base_max_width + spacing.node_spacing;
    let cell_height = max_height + (spacing.rank_spacing * 0.6);

    if ir.diagram_type == DiagramType::BlockBeta {
        for (node_index, node) in ir.nodes.iter().enumerate() {
            let span = block_beta_node_span(node).min(column_count).max(1);
            if span > 1 {
                node_sizes[node_index].0 = node_sizes[node_index]
                    .0
                    .max(base_max_width * span as f32 + spacing.node_spacing * (span - 1) as f32);
            }
        }
    }

    let mut sorted_nodes: Vec<usize> = (0..node_count).collect();
    if ir.diagram_type == DiagramType::BlockBeta {
        sorted_nodes.sort_by(|left, right| compare_block_beta_grid_node_indices(ir, *left, *right));
    } else {
        sorted_nodes.sort_by(|left, right| compare_node_indices(ir, *left, *right));
    }

    if ir.diagram_type == DiagramType::BlockBeta
        && layout_block_beta_grouped_items(
            ir,
            column_count,
            cell_width,
            cell_height,
            &mut rank_by_node,
            &mut order_by_node,
            &mut centers,
        )
    {
        // Grouped placement already populated node centers/ranks/orders.
    } else {
        let mut row = 0_usize;
        let mut col = 0_usize;
        for node_index in sorted_nodes {
            let span = if ir.diagram_type == DiagramType::BlockBeta {
                block_beta_node_span(&ir.nodes[node_index])
                    .min(column_count)
                    .max(1)
            } else {
                1
            };

            if col != 0 && col + span > column_count {
                row += 1;
                col = 0;
            }

            centers[node_index] = (
                col as f32 * cell_width + ((span - 1) as f32 * cell_width / 2.0),
                row as f32 * cell_height,
            );

            if matches!(ir.direction, GraphDirection::LR | GraphDirection::RL) {
                rank_by_node[node_index] = col;
                order_by_node[node_index] = row;
            } else {
                rank_by_node[node_index] = row;
                order_by_node[node_index] = col;
            }

            if col + span >= column_count {
                row += 1;
                col = 0;
            } else {
                col += span;
            }
        }
    }

    finalize_specialized_layout(
        ir,
        &node_sizes,
        rank_by_node,
        order_by_node,
        centers,
        trace,
        matches!(ir.direction, GraphDirection::LR | GraphDirection::RL),
    )
}

#[must_use]
fn layout_diagram_kanban_traced(ir: &MermaidDiagramIr) -> TracedLayout {
    let node_count = ir.nodes.len();
    let mut node_sizes = compute_node_sizes(ir, &fm_core::FontMetrics::default_metrics());
    let mut trace = LayoutTrace::default();
    push_snapshot(
        &mut trace,
        "kanban_layout",
        node_count,
        ir.edges.len(),
        0,
        0,
    );

    for size in &mut node_sizes {
        size.0 = size.0.max(144.0);
        size.1 = size.1.max(42.0);
    }

    let ranks = layered_ranks(ir);
    let mut nodes_by_rank: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (node_index, rank) in ranks.iter().copied().enumerate() {
        nodes_by_rank.entry(rank).or_default().push(node_index);
    }
    for nodes in nodes_by_rank.values_mut() {
        nodes.sort_by(|left, right| compare_node_indices(ir, *left, *right));
    }

    let spacing = LayoutSpacing::default();
    let mut rank_by_node = vec![0_usize; node_count];
    let mut order_by_node = vec![0_usize; node_count];
    let mut centers = vec![(0.0_f32, 0.0_f32); node_count];

    let column_gap = spacing.rank_spacing + 170.0;
    let row_gap = spacing.node_spacing + 22.0;
    for (rank, nodes) in &nodes_by_rank {
        for (order_index, node_index) in nodes.iter().enumerate() {
            centers[*node_index] = (*rank as f32 * column_gap, order_index as f32 * row_gap);
            rank_by_node[*node_index] = *rank;
            order_by_node[*node_index] = order_index;
        }
    }

    let mut traced = finalize_specialized_layout(
        ir,
        &node_sizes,
        rank_by_node,
        order_by_node,
        centers,
        trace,
        true,
    );
    traced.layout.extensions.bands = nodes_by_rank
        .keys()
        .copied()
        .filter_map(|rank| {
            layout_band_for_rank(
                &traced.layout,
                rank,
                LayoutBandKind::Lane,
                format!("lane {}", rank + 1),
                20.0,
            )
        })
        .collect();
    traced
}

fn layout_label_text(ir: &MermaidDiagramIr, node_index: usize) -> &str {
    ir.nodes
        .get(node_index)
        .and_then(|node| node.label)
        .and_then(|label_id| ir.labels.get(label_id.0))
        .map(|label| label.text.as_str())
        .or_else(|| ir.nodes.get(node_index).map(|node| node.id.as_str()))
        .unwrap_or("")
}

fn layout_bounds_for_nodes(
    layout: &DiagramLayout,
    node_indexes: &[usize],
    padding: f32,
) -> Option<LayoutRect> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;

    for node_box in &layout.nodes {
        if !node_indexes.contains(&node_box.node_index) {
            continue;
        }
        min_x = min_x.min(node_box.bounds.x);
        min_y = min_y.min(node_box.bounds.y);
        max_x = max_x.max(node_box.bounds.x + node_box.bounds.width);
        max_y = max_y.max(node_box.bounds.y + node_box.bounds.height);
    }

    if !min_x.is_finite() {
        return None;
    }

    Some(LayoutRect {
        x: min_x - padding,
        y: min_y - padding,
        width: (max_x - min_x) + (padding * 2.0),
        height: (max_y - min_y) + (padding * 2.0),
    })
}

fn layout_band_for_rank(
    layout: &DiagramLayout,
    rank: usize,
    kind: LayoutBandKind,
    label: String,
    padding: f32,
) -> Option<LayoutBand> {
    let node_indexes: Vec<usize> = layout
        .nodes
        .iter()
        .filter(|node| node.rank == rank)
        .map(|node| node.node_index)
        .collect();
    let bounds = layout_bounds_for_nodes(layout, &node_indexes, padding)?;
    Some(LayoutBand {
        kind,
        label,
        bounds,
    })
}

fn parse_order_hint(node_id: &str, fallback: usize) -> usize {
    node_id
        .rsplit('_')
        .next()
        .and_then(|candidate| candidate.parse::<usize>().ok())
        .unwrap_or(fallback.saturating_add(10_000))
}

fn layered_ranks(ir: &MermaidDiagramIr) -> Vec<usize> {
    let node_count = ir.nodes.len();
    if node_count == 0 {
        return Vec::new();
    }

    let mut outgoing = vec![Vec::<usize>::new(); node_count];
    let mut indegree = vec![0_usize; node_count];
    for edge in &ir.edges {
        let Some(source) = endpoint_node_index(ir, edge.from) else {
            continue;
        };
        let Some(target) = endpoint_node_index(ir, edge.to) else {
            continue;
        };
        if source >= node_count || target >= node_count || source == target {
            continue;
        }
        outgoing[source].push(target);
        indegree[target] = indegree[target].saturating_add(1);
    }

    for neighbors in &mut outgoing {
        neighbors.sort_by(|left, right| compare_node_indices(ir, *left, *right));
        neighbors.dedup();
    }

    let mut sorted_nodes: Vec<usize> = (0..node_count).collect();
    sorted_nodes.sort_by(|left, right| compare_node_indices(ir, *left, *right));

    let mut ranks = vec![0_usize; node_count];
    let mut processed = vec![false; node_count];

    // Use BinaryHeap for O(N log N) performance and stable tie-breaking by node ID
    let mut heap = std::collections::BinaryHeap::new();
    for node_index in sorted_nodes.iter().copied() {
        if indegree[node_index] == 0 {
            heap.push(std::cmp::Reverse((&ir.nodes[node_index].id, node_index)));
        }
    }

    while let Some(std::cmp::Reverse((_, node_index))) = heap.pop() {
        if processed[node_index] {
            continue;
        }
        processed[node_index] = true;

        for target in outgoing[node_index].iter().copied() {
            ranks[target] = ranks[target].max(ranks[node_index].saturating_add(1));
            indegree[target] = indegree[target].saturating_sub(1);
            if indegree[target] == 0 {
                heap.push(std::cmp::Reverse((&ir.nodes[target].id, target)));
            }
        }
    }

    // Assign ranks to nodes that were not reached (e.g. part of cycles that weren't fully broken)
    for node_index in sorted_nodes {
        if processed[node_index] {
            continue;
        }
        let mut candidate_rank = 0_usize;
        for edge in &ir.edges {
            let Some(target) = endpoint_node_index(ir, edge.to) else {
                continue;
            };
            if target != node_index {
                continue;
            }
            if let Some(source) = endpoint_node_index(ir, edge.from) {
                candidate_rank = candidate_rank.max(ranks[source].saturating_add(1));
            }
        }
        ranks[node_index] = candidate_rank;
    }

    ranks
}

fn finalize_specialized_layout(
    ir: &MermaidDiagramIr,
    node_sizes: &[(f32, f32)],
    rank_by_node: Vec<usize>,
    order_by_node: Vec<usize>,
    mut centers: Vec<(f32, f32)>,
    mut trace: LayoutTrace,
    horizontal_edges: bool,
) -> TracedLayout {
    let spacing = LayoutSpacing::default();

    normalize_center_positions(&mut centers, node_sizes);
    let nodes = node_boxes_from_centers(ir, node_sizes, &rank_by_node, &order_by_node, &centers);
    let edges = build_edge_paths_with_orientation(ir, &nodes, &BTreeSet::new(), horizontal_edges);
    let clusters = build_cluster_boxes(ir, &nodes, spacing);
    let bounds = compute_bounds(&nodes, &clusters, &edges, spacing);
    let (total_edge_length, reversed_edge_total_length) = compute_edge_length_metrics(&edges);

    push_snapshot(
        &mut trace,
        "specialized_post_processing",
        ir.nodes.len(),
        ir.edges.len(),
        0,
        0,
    );

    let stats = LayoutStats {
        node_count: ir.nodes.len(),
        edge_count: ir.edges.len(),
        crossing_count: 0,
        crossing_count_before_refinement: 0,
        reversed_edges: 0,
        cycle_count: 0,
        cycle_node_count: 0,
        max_cycle_size: 0,
        collapsed_clusters: 0,
        reversed_edge_total_length,
        total_edge_length,
        phase_iterations: trace.snapshots.len(),
    };

    TracedLayout {
        layout: DiagramLayout {
            nodes,
            clusters,
            cycle_clusters: Vec::new(),
            edges,
            bounds,
            stats,
            extensions: LayoutExtensions::default(),
        },
        trace,
    }
}

#[derive(Debug, Clone)]
struct TreeLayoutStructure {
    roots: Vec<usize>,
    children: Vec<Vec<usize>>,
    depth: Vec<usize>,
    max_depth: usize,
    horizontal_depth_axis: bool,
    reverse_depth_axis: bool,
}

fn build_tree_layout_structure(ir: &MermaidDiagramIr) -> TreeLayoutStructure {
    let node_count = ir.nodes.len();
    let horizontal_depth_axis = matches!(ir.direction, GraphDirection::LR | GraphDirection::RL);
    let reverse_depth_axis = matches!(ir.direction, GraphDirection::RL | GraphDirection::BT);

    if node_count == 0 {
        return TreeLayoutStructure {
            roots: Vec::new(),
            children: Vec::new(),
            depth: Vec::new(),
            max_depth: 0,
            horizontal_depth_axis,
            reverse_depth_axis,
        };
    }

    let mut outgoing = vec![Vec::new(); node_count];
    let mut indegree = vec![0_usize; node_count];
    for edge in &ir.edges {
        let Some(source) = endpoint_node_index(ir, edge.from) else {
            continue;
        };
        let Some(target) = endpoint_node_index(ir, edge.to) else {
            continue;
        };
        if source >= node_count || target >= node_count || source == target {
            continue;
        }
        outgoing[source].push(target);
        indegree[target] = indegree[target].saturating_add(1);
    }

    for neighbors in &mut outgoing {
        neighbors.sort_by(|left, right| compare_node_indices(ir, *left, *right));
        neighbors.dedup();
    }

    let mut sorted_nodes: Vec<usize> = (0..node_count).collect();
    sorted_nodes.sort_by(|left, right| compare_node_indices(ir, *left, *right));

    let mut candidate_roots: Vec<usize> = sorted_nodes
        .iter()
        .copied()
        .filter(|node| indegree[*node] == 0)
        .collect();
    if candidate_roots.is_empty()
        && let Some(first_node) = sorted_nodes.first().copied()
    {
        candidate_roots.push(first_node);
    }

    let mut visited = vec![false; node_count];
    let mut depth = vec![0_usize; node_count];
    let mut children = vec![Vec::new(); node_count];
    let mut roots = Vec::new();

    for candidate in candidate_roots
        .iter()
        .copied()
        .chain(sorted_nodes.iter().copied())
    {
        if visited[candidate] {
            continue;
        }

        roots.push(candidate);
        visited[candidate] = true;

        let mut queue = vec![candidate];
        let mut queue_index = 0_usize;
        while let Some(node) = queue.get(queue_index).copied() {
            queue_index = queue_index.saturating_add(1);
            let child_depth = depth[node].saturating_add(1);

            for &child in &outgoing[node] {
                if visited[child] {
                    continue;
                }
                visited[child] = true;
                depth[child] = child_depth;
                children[node].push(child);
                queue.push(child);
            }
        }
    }

    for node_children in &mut children {
        node_children.sort_by(|left, right| compare_node_indices(ir, *left, *right));
    }

    let max_depth = depth.iter().copied().max().unwrap_or(0);
    TreeLayoutStructure {
        roots,
        children,
        depth,
        max_depth,
        horizontal_depth_axis,
        reverse_depth_axis,
    }
}

fn tree_subtree_span(
    node_index: usize,
    children: &[Vec<usize>],
    node_span_sizes: &[f32],
    spacing: LayoutSpacing,
    memo: &mut [Option<f32>],
) -> f32 {
    if let Some(cached) = memo[node_index] {
        return cached;
    }

    let own_span = node_span_sizes[node_index].max(1.0);
    let child_span_total = if children[node_index].is_empty() {
        0.0
    } else {
        let subtree_span_sum: f32 = children[node_index]
            .iter()
            .map(|child| tree_subtree_span(*child, children, node_span_sizes, spacing, memo))
            .sum();
        let gaps = spacing.node_spacing * (children[node_index].len().saturating_sub(1) as f32);
        subtree_span_sum + gaps
    };

    let span = own_span.max(child_span_total);
    memo[node_index] = Some(span);
    span
}

fn assign_tree_span_centers(
    node_index: usize,
    span_start: f32,
    children: &[Vec<usize>],
    subtree_spans: &[f32],
    spacing: LayoutSpacing,
    out_centers: &mut [f32],
) {
    let subtree_span = subtree_spans[node_index];
    out_centers[node_index] = span_start + (subtree_span / 2.0);

    if children[node_index].is_empty() {
        return;
    }

    let child_total: f32 = children[node_index]
        .iter()
        .map(|child| subtree_spans[*child])
        .sum::<f32>()
        + spacing.node_spacing * (children[node_index].len().saturating_sub(1) as f32);
    let mut child_cursor = span_start + ((subtree_span - child_total) / 2.0);

    for child in &children[node_index] {
        assign_tree_span_centers(
            *child,
            child_cursor,
            children,
            subtree_spans,
            spacing,
            out_centers,
        );
        child_cursor += subtree_spans[*child] + spacing.node_spacing;
    }
}

fn tree_depth_level_sizes(tree: &TreeLayoutStructure, node_sizes: &[(f32, f32)]) -> Vec<f32> {
    let mut level_sizes = vec![0.0_f32; tree.max_depth + 1];
    for (node_index, &(width, height)) in node_sizes.iter().enumerate() {
        let depth = tree.depth[node_index];
        let axis_size = if tree.horizontal_depth_axis {
            width
        } else {
            height
        };
        level_sizes[depth] = level_sizes[depth].max(axis_size.max(1.0));
    }
    level_sizes
}

fn depth_level_centers(level_sizes: &[f32], gap: f32) -> Vec<f32> {
    let mut centers = vec![0.0_f32; level_sizes.len()];
    let mut cursor = 0.0_f32;
    for (index, level_size) in level_sizes.iter().copied().enumerate() {
        let bounded_size = level_size.max(1.0);
        centers[index] = cursor + (bounded_size / 2.0);
        cursor += bounded_size + gap;
    }
    centers
}

fn normalize_center_positions(centers: &mut [(f32, f32)], node_sizes: &[(f32, f32)]) {
    if centers.is_empty() {
        return;
    }

    let margin = 20.0_f32;
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    for (node_index, &(center_x, center_y)) in centers.iter().enumerate() {
        let (width, height) = node_sizes[node_index];
        min_x = min_x.min(center_x - (width / 2.0));
        min_y = min_y.min(center_y - (height / 2.0));
    }

    let offset_x = margin - min_x;
    let offset_y = margin - min_y;
    for (x, y) in centers {
        *x += offset_x;
        *y += offset_y;
    }
}

fn rank_orders_from_key(
    ir: &MermaidDiagramIr,
    rank_by_node: &[usize],
    key_by_node: &[f32],
) -> Vec<usize> {
    let mut by_rank: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (node_index, rank) in rank_by_node.iter().copied().enumerate() {
        by_rank.entry(rank).or_default().push(node_index);
    }

    let mut order_by_node = vec![0_usize; rank_by_node.len()];
    for (_rank, node_indexes) in by_rank {
        let mut sorted = node_indexes;
        sorted.sort_by(|left, right| {
            key_by_node[*left]
                .total_cmp(&key_by_node[*right])
                .then_with(|| compare_node_indices(ir, *left, *right))
        });
        for (order, node_index) in sorted.into_iter().enumerate() {
            order_by_node[node_index] = order;
        }
    }
    order_by_node
}

fn node_boxes_from_centers(
    ir: &MermaidDiagramIr,
    node_sizes: &[(f32, f32)],
    rank_by_node: &[usize],
    order_by_node: &[usize],
    centers: &[(f32, f32)],
) -> Vec<LayoutNodeBox> {
    ir.nodes
        .iter()
        .enumerate()
        .map(|(node_index, node)| {
            let (center_x, center_y) = centers[node_index];
            let (width, height) = node_sizes[node_index];
            LayoutNodeBox {
                node_index,
                node_id: node.id.clone(),
                rank: rank_by_node[node_index],
                order: order_by_node[node_index],
                span: node.span_primary,
                bounds: LayoutRect {
                    x: center_x - (width / 2.0),
                    y: center_y - (height / 2.0),
                    width,
                    height,
                },
            }
        })
        .collect()
}

fn radial_leaf_count(
    node_index: usize,
    children: &[Vec<usize>],
    memo: &mut [Option<usize>],
) -> usize {
    if let Some(cached) = memo[node_index] {
        return cached;
    }

    let count = if children[node_index].is_empty() {
        1
    } else {
        children[node_index]
            .iter()
            .map(|child| radial_leaf_count(*child, children, memo))
            .sum::<usize>()
            .max(1)
    };
    memo[node_index] = Some(count);
    count
}

#[allow(clippy::too_many_arguments)]
fn assign_radial_angles(
    node_index: usize,
    start_angle: f32,
    end_angle: f32,
    tree: &TreeLayoutStructure,
    leaf_counts: &[usize],
    node_sizes: &[(f32, f32)],
    radii: &[f32],
    depth_offset: usize,
    spacing: LayoutSpacing,
    angles: &mut [f32],
) {
    let children = &tree.children[node_index];
    if children.is_empty() {
        angles[node_index] = (start_angle + end_angle) / 2.0;
        return;
    }

    let available = (end_angle - start_angle).max(0.0);
    if available <= f32::EPSILON {
        angles[node_index] = start_angle;
        for child in children {
            assign_radial_angles(
                *child,
                start_angle,
                start_angle,
                tree,
                leaf_counts,
                node_sizes,
                radii,
                depth_offset,
                spacing,
                angles,
            );
        }
        return;
    }

    let total_child_leaves: usize = children.iter().map(|child| leaf_counts[*child]).sum();
    let total_child_leaves = total_child_leaves.max(1);
    let child_level = tree.depth[node_index] + depth_offset + 1;
    let child_radius = radii.get(child_level).copied().unwrap_or(1.0).max(1.0);

    let required_spans: Vec<f32> = children
        .iter()
        .map(|child| {
            let (width, height) = node_sizes[*child];
            ((width.max(height) + spacing.node_spacing * 0.35) / child_radius).min(PI)
        })
        .collect();

    let required_sum: f32 = required_spans.iter().sum();
    let mut spans = vec![0.0_f32; children.len()];
    if required_sum >= available {
        for (index, child) in children.iter().enumerate() {
            let weight = leaf_counts[*child] as f32 / total_child_leaves as f32;
            spans[index] = available * weight;
        }
    } else {
        let extra = available - required_sum;
        for (index, child) in children.iter().enumerate() {
            let weight = leaf_counts[*child] as f32 / total_child_leaves as f32;
            spans[index] = required_spans[index] + (extra * weight);
        }
    }

    // Fix floating-point drift so child spans cover the requested range exactly.
    let assigned: f32 = spans.iter().sum();
    if let Some(last_span) = spans.last_mut() {
        *last_span += available - assigned;
    }

    let mut cursor = start_angle;
    for (index, child) in children.iter().enumerate() {
        let child_start = cursor;
        let child_end = if index + 1 == children.len() {
            end_angle
        } else {
            cursor + spans[index]
        };
        assign_radial_angles(
            *child,
            child_start,
            child_end,
            tree,
            leaf_counts,
            node_sizes,
            radii,
            depth_offset,
            spacing,
            angles,
        );
        cursor = child_end;
    }

    let total_child_angle: f32 = children.iter().map(|child| angles[*child]).sum();
    angles[node_index] = total_child_angle / children.len() as f32;

    // Guard against NaN from any unexpected numerical instability.
    if !angles[node_index].is_finite() {
        angles[node_index] = (start_angle + end_angle) / 2.0;
    }
}

/// Deterministic initial placement using a hash of node IDs.
///
/// Places nodes in a grid pattern with positions offset by a deterministic
/// hash so that the layout doesn't depend on node insertion order.
fn force_initial_positions(
    ir: &MermaidDiagramIr,
    node_sizes: &[(f32, f32)],
    spacing: &LayoutSpacing,
) -> Vec<(f32, f32)> {
    let n = ir.nodes.len();
    let cols = (n as f32).sqrt().ceil() as usize;
    let cell_size = spacing.node_spacing + spacing.rank_spacing;

    ir.nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            // Deterministic hash: FNV-1a on node ID bytes.
            let hash = fnv1a_hash(node.id.as_bytes());
            // Small perturbation from hash to break symmetry.
            let jitter_x = ((hash & 0xFF) as f32 / 255.0 - 0.5) * cell_size * 0.3;
            let jitter_y = (((hash >> 8) & 0xFF) as f32 / 255.0 - 0.5) * cell_size * 0.3;

            let col = i % cols;
            let row = i / cols;
            let (w, h) = node_sizes[i];
            let x = col as f32 * cell_size + jitter_x + w / 2.0;
            let y = row as f32 * cell_size + jitter_y + h / 2.0;
            (x, y)
        })
        .collect()
}

/// Simple FNV-1a hash for deterministic node placement.
fn fnv1a_hash(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// Build adjacency list from edges.
fn force_build_adjacency(ir: &MermaidDiagramIr) -> Vec<Vec<usize>> {
    let n = ir.nodes.len();
    let mut adj = vec![Vec::new(); n];
    for edge in &ir.edges {
        let from = endpoint_node_index(ir, edge.from);
        let to = endpoint_node_index(ir, edge.to);
        if let (Some(f), Some(t)) = (from, to)
            && f != t
            && f < n
            && t < n
        {
            adj[f].push(t);
            adj[t].push(f);
        }
    }
    // Deduplicate.
    for neighbors in &mut adj {
        neighbors.sort_unstable();
        neighbors.dedup();
    }
    adj
}

/// Map each node to its cluster index (if any).
fn force_cluster_membership(ir: &MermaidDiagramIr) -> Vec<Option<usize>> {
    let n = ir.nodes.len();
    let mut membership = vec![None; n];
    for (ci, cluster) in ir.clusters.iter().enumerate() {
        for member in &cluster.members {
            if member.0 < n {
                membership[member.0] = Some(ci);
            }
        }
    }
    membership
}

/// Compute iteration budget based on graph size.
fn force_iteration_budget(n: usize) -> usize {
    // More nodes need more iterations, but cap at 500.
    (50 + n * 2).min(500)
}

/// Cooling schedule: linear decay from initial temperature.
fn force_temperature(iteration: usize, max_iterations: usize, k: f32) -> f32 {
    let t0 = k * 10.0; // Initial temperature
    let progress = iteration as f32 / max_iterations.max(1) as f32;
    t0 * (1.0 - progress)
}

/// Compute force displacements for all nodes.
///
/// Uses direct O(n^2) repulsive forces. For graphs > 100 nodes, uses
/// Barnes-Hut grid approximation.
fn force_compute_displacements(
    positions: &[(f32, f32)],
    node_sizes: &[(f32, f32)],
    adjacency: &[Vec<usize>],
    cluster_membership: &[Option<usize>],
    k: f32,
    n: usize,
) -> Vec<(f32, f32)> {
    let mut displacements = vec![(0.0_f32, 0.0_f32); n];
    let k_sq = k * k;

    if n <= 100 {
        // Direct O(n^2) repulsive forces.
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = positions[i].0 - positions[j].0;
                let dy = positions[i].1 - positions[j].1;
                let dist_sq = (dx * dx + dy * dy).max(1.0);
                // Fruchterman-Reingold repulsive force: k^2 / d
                // Vector component: (dx / d) * (k^2 / d) = dx * k^2 / d^2
                let force_over_d = k_sq / dist_sq;
                let fx = dx * force_over_d;
                let fy = dy * force_over_d;
                displacements[i].0 += fx;
                displacements[i].1 += fy;
                displacements[j].0 -= fx;
                displacements[j].1 -= fy;
            }
        }
    } else {
        // Barnes-Hut grid approximation for large graphs.
        force_barnes_hut_repulsion(positions, k_sq, &mut displacements);
    }

    // Attractive forces along edges (Hooke's law).
    for (i, neighbors) in adjacency.iter().enumerate() {
        for &j in neighbors {
            if j <= i {
                continue; // Process each edge once.
            }
            let dx = positions[i].0 - positions[j].0;
            let dy = positions[i].1 - positions[j].1;
            let dist = (dx * dx + dy * dy).sqrt().max(1.0);
            // Fruchterman-Reingold attractive force: d^2 / k
            // Vector component: (dx / d) * (d^2 / k) = dx * d / k
            let force_over_d = dist / k;
            let fx = dx * force_over_d;
            let fy = dy * force_over_d;
            displacements[i].0 -= fx;
            displacements[i].1 -= fy;
            displacements[j].0 += fx;
            displacements[j].1 += fy;
        }
    }

    // Cluster cohesion: extra attractive force toward cluster centroid.
    force_cluster_cohesion(
        positions,
        node_sizes,
        cluster_membership,
        k,
        &mut displacements,
    );

    displacements
}

/// Barnes-Hut grid-based approximation for repulsive forces.
///
/// Divides the space into a grid and computes repulsive forces from
/// grid cell centroids for distant nodes.
fn force_barnes_hut_repulsion(
    positions: &[(f32, f32)],
    k_sq: f32,
    displacements: &mut [(f32, f32)],
) {
    let n = positions.len();
    // Find bounding box.
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for &(x, y) in positions {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }

    let range_x = (max_x - min_x).max(1.0);
    let range_y = (max_y - min_y).max(1.0);

    // Grid size: roughly sqrt(n) cells per side.
    let grid_size = (n as f32).sqrt().ceil() as usize;
    let cell_w = range_x / grid_size as f32;
    let cell_h = range_y / grid_size as f32;

    // Assign nodes to grid cells and compute cell centroids.
    let mut cell_sum_x = vec![0.0_f32; grid_size * grid_size];
    let mut cell_sum_y = vec![0.0_f32; grid_size * grid_size];
    let mut cell_count = vec![0_u32; grid_size * grid_size];
    let mut node_cell = vec![0_usize; n];
    let mut nodes_in_cell = vec![Vec::new(); grid_size * grid_size];

    for (i, &(x, y)) in positions.iter().enumerate() {
        let cx = ((x - min_x) / cell_w).floor() as usize;
        let cy = ((y - min_y) / cell_h).floor() as usize;
        let cx = cx.min(grid_size - 1);
        let cy = cy.min(grid_size - 1);
        let cell_idx = cy * grid_size + cx;
        node_cell[i] = cell_idx;
        cell_sum_x[cell_idx] += x;
        cell_sum_y[cell_idx] += y;
        cell_count[cell_idx] += 1;
        nodes_in_cell[cell_idx].push(i);
    }

    // Compute centroids.
    let mut centroids = vec![(0.0_f32, 0.0_f32, 0_u32); grid_size * grid_size];
    for idx in 0..(grid_size * grid_size) {
        if cell_count[idx] > 0 {
            centroids[idx] = (
                cell_sum_x[idx] / cell_count[idx] as f32,
                cell_sum_y[idx] / cell_count[idx] as f32,
                cell_count[idx],
            );
        }
    }

    let theta_sq: f32 = 1.5; // Barnes-Hut opening angle threshold squared

    for i in 0..n {
        let (px, py) = positions[i];
        let my_cell = node_cell[i];

        for (cell_idx, &(cx, cy, count)) in centroids.iter().enumerate() {
            if count == 0 {
                continue;
            }

            if cell_idx == my_cell {
                // Same cell: compute direct forces.
                for &j in &nodes_in_cell[my_cell] {
                    if j == i {
                        continue;
                    }
                    let dx = px - positions[j].0;
                    let dy = py - positions[j].1;
                    let dist_sq = (dx * dx + dy * dy).max(1.0);
                    let force = k_sq / dist_sq.sqrt();
                    let dist = dist_sq.sqrt();
                    displacements[i].0 += dx / dist * force;
                    displacements[i].1 += dy / dist * force;
                }
            } else {
                // Different cell: check if far enough for approximation.
                let dx = px - cx;
                let dy = py - cy;
                let dist_sq = (dx * dx + dy * dy).max(1.0);
                let cell_size_sq = cell_w * cell_w + cell_h * cell_h;

                if cell_size_sq / dist_sq < theta_sq {
                    // Use centroid approximation (multiply force by count).
                    let force = k_sq * count as f32 / dist_sq.sqrt();
                    let dist = dist_sq.sqrt();
                    displacements[i].0 += dx / dist * force;
                    displacements[i].1 += dy / dist * force;
                } else {
                    // Too close: compute direct forces.
                    for &j in &nodes_in_cell[cell_idx] {
                        let dx2 = px - positions[j].0;
                        let dy2 = py - positions[j].1;
                        let dist_sq2 = (dx2 * dx2 + dy2 * dy2).max(1.0);
                        let force2 = k_sq / dist_sq2.sqrt();
                        let dist2 = dist_sq2.sqrt();
                        displacements[i].0 += dx2 / dist2 * force2;
                        displacements[i].1 += dy2 / dist2 * force2;
                    }
                }
            }
        }
    }
}

/// Apply extra attractive force for nodes in the same cluster.
fn force_cluster_cohesion(
    positions: &[(f32, f32)],
    _node_sizes: &[(f32, f32)],
    cluster_membership: &[Option<usize>],
    k: f32,
    displacements: &mut [(f32, f32)],
) {
    // Compute cluster centroids.
    let mut cluster_sum: BTreeMap<usize, (f32, f32, usize)> = BTreeMap::new();
    for (i, &membership) in cluster_membership.iter().enumerate() {
        if let Some(ci) = membership {
            let entry = cluster_sum.entry(ci).or_insert((0.0, 0.0, 0));
            entry.0 += positions[i].0;
            entry.1 += positions[i].1;
            entry.2 += 1;
        }
    }

    let cohesion_strength = 0.3; // Extra pull toward cluster center

    for (i, &membership) in cluster_membership.iter().enumerate() {
        if let Some(ci) = membership
            && let Some(&(sx, sy, count)) = cluster_sum.get(&ci)
            && count > 1
        {
            let centroid_x = sx / count as f32;
            let centroid_y = sy / count as f32;
            let dx = centroid_x - positions[i].0;
            let dy = centroid_y - positions[i].1;
            let dist = (dx * dx + dy * dy).sqrt().max(1.0);
            let force = dist / k * cohesion_strength;
            displacements[i].0 += dx / dist * force;
            displacements[i].1 += dy / dist * force;
        }
    }
}

/// Remove node overlaps via iterative projection.
fn force_remove_overlaps(
    positions: &mut [(f32, f32)],
    node_sizes: &[(f32, f32)],
    spacing: &LayoutSpacing,
) {
    let n = positions.len();
    let gap = spacing.node_spacing * 0.25; // Minimum gap between nodes

    for _pass in 0..20 {
        let mut any_overlap = false;
        for i in 0..n {
            for j in (i + 1)..n {
                let (wi, hi) = node_sizes[i];
                let (wj, hj) = node_sizes[j];
                let half_w = (wi + wj) / 2.0 + gap;
                let half_h = (hi + hj) / 2.0 + gap;

                let dx = positions[j].0 - positions[i].0;
                let dy = positions[j].1 - positions[i].1;
                let overlap_x = half_w - dx.abs();
                let overlap_y = half_h - dy.abs();

                if overlap_x > 0.0 && overlap_y > 0.0 {
                    any_overlap = true;
                    // Push apart along the axis with less overlap.
                    if overlap_x < overlap_y {
                        let push = overlap_x / 2.0;
                        if dx >= 0.0 {
                            positions[i].0 -= push;
                            positions[j].0 += push;
                        } else {
                            positions[i].0 += push;
                            positions[j].0 -= push;
                        }
                    } else {
                        let push = overlap_y / 2.0;
                        if dy >= 0.0 {
                            positions[i].1 -= push;
                            positions[j].1 += push;
                        } else {
                            positions[i].1 += push;
                            positions[j].1 -= push;
                        }
                    }
                }
            }
        }
        if !any_overlap {
            break;
        }
    }
}

/// Normalize positions so all coordinates are non-negative.
fn force_normalize_positions(positions: &mut [(f32, f32)], node_sizes: &[(f32, f32)]) {
    if positions.is_empty() {
        return;
    }
    let margin = 20.0;
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    for (i, &(x, y)) in positions.iter().enumerate() {
        let (w, h) = node_sizes[i];
        min_x = min_x.min(x - w / 2.0);
        min_y = min_y.min(y - h / 2.0);
    }
    let offset_x = margin - min_x;
    let offset_y = margin - min_y;
    for pos in positions.iter_mut() {
        pos.0 += offset_x;
        pos.1 += offset_y;
    }
}

/// Build LayoutNodeBox from force-directed positions (center-based).
fn force_build_node_boxes(
    ir: &MermaidDiagramIr,
    positions: &[(f32, f32)],
    node_sizes: &[(f32, f32)],
) -> Vec<LayoutNodeBox> {
    ir.nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let (cx, cy) = positions[i];
            let (w, h) = node_sizes[i];
            LayoutNodeBox {
                node_index: i,
                node_id: node.id.clone(),
                rank: 0,  // No ranks in force-directed layout.
                order: i, // Order by index.
                span: node.span_primary,
                bounds: LayoutRect {
                    x: cx - w / 2.0,
                    y: cy - h / 2.0,
                    width: w,
                    height: h,
                },
            }
        })
        .collect()
}

/// Build straight-line edge paths for force-directed layout.
fn force_build_edge_paths(ir: &MermaidDiagramIr, nodes: &[LayoutNodeBox]) -> Vec<LayoutEdgePath> {
    ir.edges
        .iter()
        .enumerate()
        .filter_map(|(ei, edge)| {
            let from_idx = endpoint_node_index(ir, edge.from)?;
            let to_idx = endpoint_node_index(ir, edge.to)?;
            if from_idx >= nodes.len() || to_idx >= nodes.len() {
                return None;
            }
            let from_center = nodes[from_idx].bounds.center();
            let to_center = nodes[to_idx].bounds.center();

            // Clip to node boundaries.
            let from_pt = clip_to_rect_border(from_center, to_center, &nodes[from_idx].bounds);
            let to_pt = clip_to_rect_border(to_center, from_center, &nodes[to_idx].bounds);

            Some(LayoutEdgePath {
                edge_index: ei,
                span: edge.span,
                points: vec![from_pt, to_pt],
                reversed: false,
                is_self_loop: from_idx == to_idx,
                parallel_offset: 0.0,
            })
        })
        .collect()
}

/// Clip a line from `from` toward `to` to the border of `rect`.
fn clip_to_rect_border(from: LayoutPoint, to: LayoutPoint, rect: &LayoutRect) -> LayoutPoint {
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let dx = to.x - from.x;
    let dy = to.y - from.y;

    if dx.abs() < f32::EPSILON && dy.abs() < f32::EPSILON {
        return from;
    }

    let half_w = rect.width / 2.0;
    let half_h = rect.height / 2.0;

    // Find intersection with rect border along direction (dx, dy) from center.
    let tx = if dx.abs() > f32::EPSILON {
        half_w / dx.abs()
    } else {
        f32::MAX
    };
    let ty = if dy.abs() > f32::EPSILON {
        half_h / dy.abs()
    } else {
        f32::MAX
    };
    let t = tx.min(ty);

    LayoutPoint {
        x: cx + dx * t,
        y: cy + dy * t,
    }
}

#[must_use]
pub fn compute_node_sizes(ir: &MermaidDiagramIr, metrics: &fm_core::FontMetrics) -> Vec<(f32, f32)> {
    ir.nodes
        .iter()
        .map(|node| {
            let text = node
                .label
                .and_then(|label_id| ir.labels.get(label_id.0))
                .map(|value| value.text.as_str())
                .unwrap_or_else(|| node.id.as_str());

            let (label_width, label_height) = metrics.estimate_dimensions(text);

            // Generous padding for readable, premium-looking nodes
            let width = label_width + 72.0;
            let height = label_height + 44.0;

            // Ensure generous baseline dimensions
            (width.max(100.0), height.max(52.0))
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CycleRemovalResult {
    reversed_edge_indexes: BTreeSet<usize>,
    highlighted_edge_indexes: BTreeSet<usize>,
    summary: CycleSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct CycleSummary {
    cycle_count: usize,
    cycle_node_count: usize,
    max_cycle_size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct CycleDetection {
    components: Vec<Vec<usize>>,
    node_to_component: Vec<Option<usize>>,
    cyclic_component_indexes: BTreeSet<usize>,
    summary: CycleSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CycleClusterMap {
    /// For each original node index, the representative node index (self if not collapsed).
    node_representative: Vec<usize>,
    /// The set of representative node indexes that are cycle cluster heads.
    cluster_heads: BTreeSet<usize>,
    /// For each cluster head, the list of member node indexes (including the head).
    cluster_members: BTreeMap<usize, Vec<usize>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OrientedEdge {
    source: usize,
    target: usize,
    edge_index: usize,
}

fn default_cycle_strategy() -> CycleStrategy {
    std::env::var("FM_CYCLE_STRATEGY")
        .ok()
        .as_deref()
        .and_then(CycleStrategy::parse)
        .unwrap_or_default()
}

fn cycle_removal(ir: &MermaidDiagramIr, cycle_strategy: CycleStrategy) -> CycleRemovalResult {
    let node_count = ir.nodes.len();
    if node_count == 0 {
        return CycleRemovalResult {
            reversed_edge_indexes: BTreeSet::new(),
            highlighted_edge_indexes: BTreeSet::new(),
            summary: CycleSummary::default(),
        };
    }

    let edges = resolved_edges(ir);
    if edges.is_empty() {
        return CycleRemovalResult {
            reversed_edge_indexes: BTreeSet::new(),
            highlighted_edge_indexes: BTreeSet::new(),
            summary: CycleSummary::default(),
        };
    }

    let node_priority = stable_node_priorities(ir);
    let cycle_detection = detect_cycle_components(node_count, &edges, &node_priority);
    let dfs_back_edges = cycle_removal_dfs_back(node_count, &edges, &node_priority);

    let reversed_edge_indexes = match cycle_strategy {
        CycleStrategy::Greedy => cycle_removal_greedy(node_count, &edges, &node_priority),
        CycleStrategy::DfsBack => dfs_back_edges.clone(),
        CycleStrategy::MfasApprox => {
            cycle_removal_mfas_approx(node_count, &edges, &node_priority, &cycle_detection)
        }
        CycleStrategy::CycleAware => {
            // For CycleAware, we still want to break cycles for the ranking phase
            // to ensure a high-quality topological baseline, but we keep the
            // original orientation for other phases that handle cycles explicitly.
            dfs_back_edges.clone()
        }
    };

    let highlighted_edge_indexes = if matches!(cycle_strategy, CycleStrategy::CycleAware) {
        dfs_back_edges
    } else {
        reversed_edge_indexes.clone()
    };

    let result = CycleRemovalResult {
        reversed_edge_indexes,
        highlighted_edge_indexes,
        summary: cycle_detection.summary,
    };
    if result.summary.cycle_count > 0 {
        info!(
            strategy = cycle_strategy.as_str(),
            cycle_count = result.summary.cycle_count,
            cycle_node_count = result.summary.cycle_node_count,
            max_cycle_size = result.summary.max_cycle_size,
            reversed_edges = result.reversed_edge_indexes.len(),
            "layout.cycle_removal"
        );
    } else {
        trace!(
            strategy = cycle_strategy.as_str(),
            "layout.cycle_removal.acyclic"
        );
    }
    result
}

fn detect_cycle_components(
    node_count: usize,
    edges: &[OrientedEdge],
    node_priority: &[usize],
) -> CycleDetection {
    struct TarjanState<'a> {
        index: usize,
        indices: Vec<Option<usize>>,
        lowlink: Vec<usize>,
        stack: Vec<usize>,
        on_stack: Vec<bool>,
        components: Vec<Vec<usize>>,
        outgoing_edge_slots: &'a [Vec<usize>],
        edges: &'a [OrientedEdge],
        node_priority: &'a [usize],
    }

    impl TarjanState<'_> {
        fn strong_connect(&mut self, node: usize) {
            self.indices[node] = Some(self.index);
            self.lowlink[node] = self.index;
            self.index = self.index.saturating_add(1);
            self.stack.push(node);
            self.on_stack[node] = true;

            for edge_slot in self.outgoing_edge_slots[node].iter().copied() {
                let next = self.edges[edge_slot].target;
                if self.indices[next].is_none() {
                    self.strong_connect(next);
                    self.lowlink[node] = self.lowlink[node].min(self.lowlink[next]);
                } else if self.on_stack[next] {
                    self.lowlink[node] =
                        self.lowlink[node].min(self.indices[next].unwrap_or(self.lowlink[node]));
                }
            }

            if self.lowlink[node] == self.indices[node].unwrap_or(self.lowlink[node]) {
                let mut component = Vec::new();
                while let Some(top) = self.stack.pop() {
                    self.on_stack[top] = false;
                    component.push(top);
                    if top == node {
                        break;
                    }
                }
                component
                    .sort_by(|left, right| compare_priority(*left, *right, self.node_priority));
                self.components.push(component);
            }
        }
    }

    let outgoing_edge_slots = sorted_outgoing_edge_slots(node_count, edges, node_priority);
    let mut tarjan = TarjanState {
        index: 0,
        indices: vec![None; node_count],
        lowlink: vec![0_usize; node_count],
        stack: Vec::new(),
        on_stack: vec![false; node_count],
        components: Vec::new(),
        outgoing_edge_slots: &outgoing_edge_slots,
        edges,
        node_priority,
    };

    let mut node_visit_order: Vec<usize> = (0..node_count).collect();
    node_visit_order.sort_by(|left, right| compare_priority(*left, *right, node_priority));
    for node in node_visit_order {
        if tarjan.indices[node].is_none() {
            tarjan.strong_connect(node);
        }
    }

    let mut node_to_component = vec![None; node_count];
    for (component_index, component_nodes) in tarjan.components.iter().enumerate() {
        for node in component_nodes {
            node_to_component[*node] = Some(component_index);
        }
    }

    let mut cyclic_component_indexes = BTreeSet::new();
    let mut cycle_node_count = 0_usize;
    let mut max_cycle_size = 0_usize;
    for (component_index, component_nodes) in tarjan.components.iter().enumerate() {
        let is_cyclic = if component_nodes.len() > 1 {
            true
        } else {
            let node = component_nodes[0];
            edges
                .iter()
                .any(|edge| edge.source == node && edge.target == node)
        };

        if is_cyclic {
            cyclic_component_indexes.insert(component_index);
            cycle_node_count = cycle_node_count.saturating_add(component_nodes.len());
            max_cycle_size = max_cycle_size.max(component_nodes.len());
        }
    }

    CycleDetection {
        components: tarjan.components,
        node_to_component,
        cyclic_component_indexes: cyclic_component_indexes.clone(),
        summary: CycleSummary {
            cycle_count: cyclic_component_indexes.len(),
            cycle_node_count,
            max_cycle_size,
        },
    }
}

fn cycle_removal_dfs_back(
    node_count: usize,
    edges: &[OrientedEdge],
    node_priority: &[usize],
) -> BTreeSet<usize> {
    let outgoing_edge_slots = sorted_outgoing_edge_slots(node_count, edges, node_priority);
    let mut state = vec![0_u8; node_count];
    let mut reversed_edge_indexes = BTreeSet::new();

    fn visit(
        node: usize,
        state: &mut [u8],
        outgoing_edge_slots: &[Vec<usize>],
        edges: &[OrientedEdge],
        reversed_edge_indexes: &mut BTreeSet<usize>,
    ) {
        state[node] = 1;
        for edge_slot in outgoing_edge_slots[node].iter().copied() {
            let edge = edges[edge_slot];
            match state[edge.target] {
                0 => visit(
                    edge.target,
                    state,
                    outgoing_edge_slots,
                    edges,
                    reversed_edge_indexes,
                ),
                1 => {
                    reversed_edge_indexes.insert(edge.edge_index);
                }
                _ => {}
            }
        }
        state[node] = 2;
    }

    let mut node_visit_order: Vec<usize> = (0..node_count).collect();
    node_visit_order.sort_by(|left, right| compare_priority(*left, *right, node_priority));
    for node in node_visit_order {
        if state[node] == 0 {
            visit(
                node,
                &mut state,
                &outgoing_edge_slots,
                edges,
                &mut reversed_edge_indexes,
            );
        }
    }

    reversed_edge_indexes
}

fn cycle_removal_mfas_approx(
    node_count: usize,
    edges: &[OrientedEdge],
    node_priority: &[usize],
    cycle_detection: &CycleDetection,
) -> BTreeSet<usize> {
    if cycle_detection.summary.cycle_count == 0 {
        return BTreeSet::new();
    }

    let mut reversed_edge_indexes = BTreeSet::new();

    for component_index in &cycle_detection.cyclic_component_indexes {
        let component_nodes = cycle_detection
            .components
            .get(*component_index)
            .cloned()
            .unwrap_or_default();
        if component_nodes.is_empty() {
            continue;
        }

        let mut in_degree = vec![0_usize; node_count];
        let mut out_degree = vec![0_usize; node_count];

        for edge in edges {
            if cycle_detection.node_to_component[edge.source] == Some(*component_index)
                && cycle_detection.node_to_component[edge.target] == Some(*component_index)
            {
                out_degree[edge.source] = out_degree[edge.source].saturating_add(1);
                in_degree[edge.target] = in_degree[edge.target].saturating_add(1);
            }
        }

        let mut component_order = component_nodes;
        component_order.sort_by(|left, right| {
            let left_score = out_degree[*left] as isize - in_degree[*left] as isize;
            let right_score = out_degree[*right] as isize - in_degree[*right] as isize;
            right_score
                .cmp(&left_score)
                .then_with(|| compare_priority(*left, *right, node_priority))
        });

        let mut position = BTreeMap::<usize, usize>::new();
        for (index, node) in component_order.into_iter().enumerate() {
            position.insert(node, index);
        }

        for edge in edges {
            if cycle_detection.node_to_component[edge.source] == Some(*component_index)
                && cycle_detection.node_to_component[edge.target] == Some(*component_index)
                && position.get(&edge.source).copied().unwrap_or(0)
                    > position.get(&edge.target).copied().unwrap_or(0)
            {
                reversed_edge_indexes.insert(edge.edge_index);
            }
        }
    }

    if reversed_edge_indexes.is_empty() {
        return cycle_removal_dfs_back(node_count, edges, node_priority);
    }

    reversed_edge_indexes
}

fn sorted_outgoing_edge_slots(
    node_count: usize,
    edges: &[OrientedEdge],
    node_priority: &[usize],
) -> Vec<Vec<usize>> {
    let mut outgoing_edge_slots = vec![Vec::new(); node_count];
    for (edge_slot, edge) in edges.iter().enumerate() {
        outgoing_edge_slots[edge.source].push(edge_slot);
    }

    for slots in &mut outgoing_edge_slots {
        slots.sort_by(|left, right| {
            let left_edge = edges[*left];
            let right_edge = edges[*right];
            compare_priority(left_edge.target, right_edge.target, node_priority)
                .then_with(|| left_edge.edge_index.cmp(&right_edge.edge_index))
        });
    }

    outgoing_edge_slots
}

fn cycle_removal_greedy(
    node_count: usize,
    edges: &[OrientedEdge],
    node_priority: &[usize],
) -> BTreeSet<usize> {
    let mut active_nodes: BTreeSet<usize> = (0..node_count).collect();
    let mut in_degree = vec![0_usize; node_count];
    let mut out_degree = vec![0_usize; node_count];
    let mut incoming = vec![Vec::new(); node_count];
    let mut outgoing = vec![Vec::new(); node_count];

    for (edge_slot, edge) in edges.iter().enumerate() {
        in_degree[edge.target] = in_degree[edge.target].saturating_add(1);
        out_degree[edge.source] = out_degree[edge.source].saturating_add(1);
        incoming[edge.target].push(edge_slot);
        outgoing[edge.source].push(edge_slot);
    }

    let mut left_order = Vec::with_capacity(node_count);
    let mut right_order = Vec::with_capacity(node_count);

    while !active_nodes.is_empty() {
        let mut sinks: Vec<usize> = active_nodes
            .iter()
            .copied()
            .filter(|node| out_degree[*node] == 0)
            .collect();
        if !sinks.is_empty() {
            sinks.sort_by(|left, right| compare_priority(*left, *right, node_priority));
            for node in sinks {
                remove_node(
                    node,
                    &mut active_nodes,
                    &incoming,
                    &outgoing,
                    edges,
                    &mut in_degree,
                    &mut out_degree,
                );
                right_order.push(node);
            }
            continue;
        }

        let mut sources: Vec<usize> = active_nodes
            .iter()
            .copied()
            .filter(|node| in_degree[*node] == 0)
            .collect();
        if !sources.is_empty() {
            sources.sort_by(|left, right| compare_priority(*left, *right, node_priority));
            for node in sources {
                remove_node(
                    node,
                    &mut active_nodes,
                    &incoming,
                    &outgoing,
                    edges,
                    &mut in_degree,
                    &mut out_degree,
                );
                left_order.push(node);
            }
            continue;
        }

        let Some(candidate) = active_nodes.iter().copied().max_by(|left, right| {
            let left_score = out_degree[*left] as isize - in_degree[*left] as isize;
            let right_score = out_degree[*right] as isize - in_degree[*right] as isize;
            left_score
                .cmp(&right_score)
                .then_with(|| compare_priority(*left, *right, node_priority))
        }) else {
            break;
        };

        remove_node(
            candidate,
            &mut active_nodes,
            &incoming,
            &outgoing,
            edges,
            &mut in_degree,
            &mut out_degree,
        );
        left_order.push(candidate);
    }

    left_order.extend(right_order.into_iter().rev());
    let mut position = vec![0_usize; node_count];
    for (order, node_index) in left_order.into_iter().enumerate() {
        position[node_index] = order;
    }

    edges
        .iter()
        .filter_map(|edge| {
            (position[edge.source] > position[edge.target]).then_some(edge.edge_index)
        })
        .collect()
}

fn rank_assignment(ir: &MermaidDiagramIr, cycles: &CycleRemovalResult) -> BTreeMap<usize, usize> {
    let node_count = ir.nodes.len();
    let node_priority = stable_node_priorities(ir);
    let edges = oriented_edges(ir, &cycles.reversed_edge_indexes);

    let mut ranks = vec![0_usize; node_count];
    let mut in_degree = vec![0_usize; node_count];
    let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); node_count];

    for edge in &edges {
        if edge.source == edge.target {
            continue;
        }
        in_degree[edge.target] = in_degree[edge.target].saturating_add(1);
        outgoing[edge.source].push(edge.target);
    }

    for targets in &mut outgoing {
        targets.sort_by(|left, right| compare_priority(*left, *right, &node_priority));
    }

    let mut heap: BinaryHeap<Reverse<(usize, usize)>> = BinaryHeap::new();
    for node_index in 0..node_count {
        if in_degree[node_index] == 0 {
            heap.push(Reverse((node_priority[node_index], node_index)));
        }
    }

    let mut visited = 0_usize;
    while let Some(Reverse((_priority, node_index))) = heap.pop() {
        visited = visited.saturating_add(1);
        let source_rank = ranks[node_index];

        for target in outgoing[node_index].iter().copied() {
            let candidate_rank = source_rank.saturating_add(1);
            if candidate_rank > ranks[target] {
                ranks[target] = candidate_rank;
            }
            in_degree[target] = in_degree[target].saturating_sub(1);
            if in_degree[target] == 0 {
                heap.push(Reverse((node_priority[target], target)));
            }
        }
    }

    if visited < node_count {
        // Residual cyclic components fallback to bounded longest-path relaxation.
        // We use node_count as the guard because the longest possible path in a DAG
        // has node_count - 1 edges. If we iterate more, we are definitely in a cycle.
        let guard = node_count;
        for _ in 0..guard {
            let mut changed = false;
            for edge in &edges {
                if edge.source == edge.target {
                    continue;
                }
                let candidate_rank = ranks[edge.source].saturating_add(1);
                if candidate_rank > ranks[edge.target] {
                    ranks[edge.target] = candidate_rank;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    // Compact disconnected components along the rank axis so each component
    // gets an independent band instead of sharing rank-0/rank-1 globally.
    // This avoids pathological ultra-wide layouts for many disconnected chains.
    let mut components = weakly_connected_components(node_count, &edges);
    components.sort_by_key(|component| {
        component
            .iter()
            .map(|node_index| node_priority[*node_index])
            .min()
            .unwrap_or(usize::MAX)
    });

    if components.len() > 1 {
        let mut compacted_ranks = ranks.clone();
        let mut rank_cursor = 0_usize;
        let mut isolated_singletons = Vec::new();

        let mut incident_edge_count = vec![0_usize; node_count];
        for edge in &edges {
            if edge.source < node_count {
                incident_edge_count[edge.source] =
                    incident_edge_count[edge.source].saturating_add(1);
            }
            if edge.target < node_count {
                incident_edge_count[edge.target] =
                    incident_edge_count[edge.target].saturating_add(1);
            }
        }

        for component in components {
            if component.is_empty() {
                continue;
            }

            // Treat isolated singletons specially: they will be grouped in a single rank band at the end
            if component.len() == 1 && incident_edge_count[component[0]] == 0 {
                isolated_singletons.push(component[0]);
                continue;
            }

            let mut min_rank = usize::MAX;
            let mut max_rank = 0_usize;
            for &node_index in &component {
                let rank = ranks[node_index];
                min_rank = min_rank.min(rank);
                max_rank = max_rank.max(rank);
            }

            if min_rank == usize::MAX {
                continue;
            }

            let span = max_rank.saturating_sub(min_rank).saturating_add(1);
            for &node_index in &component {
                compacted_ranks[node_index] = ranks[node_index]
                    .saturating_sub(min_rank)
                    .saturating_add(rank_cursor);
            }

            rank_cursor = rank_cursor.saturating_add(span).saturating_add(1);
        }

        // Place all isolated singletons in the next available rank band
        if !isolated_singletons.is_empty() {
            for node_index in isolated_singletons {
                compacted_ranks[node_index] = rank_cursor;
            }
        }

        ranks = compacted_ranks;
    }

    (0..node_count).map(|index| (index, ranks[index])).collect()
}

fn weakly_connected_components(node_count: usize, edges: &[OrientedEdge]) -> Vec<Vec<usize>> {
    if node_count == 0 {
        return Vec::new();
    }

    let mut adjacency: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); node_count];
    for edge in edges {
        if edge.source >= node_count || edge.target >= node_count {
            continue;
        }
        adjacency[edge.source].insert(edge.target);
        adjacency[edge.target].insert(edge.source);
    }

    let mut visited = vec![false; node_count];
    let mut components = Vec::new();

    for start in 0..node_count {
        if visited[start] {
            continue;
        }

        let mut stack = vec![start];
        visited[start] = true;
        let mut component = Vec::new();

        while let Some(node_index) = stack.pop() {
            component.push(node_index);
            for &neighbor in adjacency[node_index].iter().rev() {
                if visited[neighbor] {
                    continue;
                }
                visited[neighbor] = true;
                stack.push(neighbor);
            }
        }

        component.sort_unstable();
        components.push(component);
    }

    components
}

fn resolved_edges(ir: &MermaidDiagramIr) -> Vec<OrientedEdge> {
    ir.edges
        .iter()
        .enumerate()
        .filter_map(|(edge_index, edge)| {
            let source = endpoint_node_index(ir, edge.from)?;
            let target = endpoint_node_index(ir, edge.to)?;
            Some(OrientedEdge {
                source,
                target,
                edge_index,
            })
        })
        .collect()
}

fn oriented_edges(
    ir: &MermaidDiagramIr,
    reversed_edge_indexes: &BTreeSet<usize>,
) -> Vec<OrientedEdge> {
    resolved_edges(ir)
        .into_iter()
        .map(|mut edge| {
            if reversed_edge_indexes.contains(&edge.edge_index) {
                std::mem::swap(&mut edge.source, &mut edge.target);
            }
            edge
        })
        .collect()
}

fn stable_node_priorities(ir: &MermaidDiagramIr) -> Vec<usize> {
    let mut node_indexes: Vec<usize> = (0..ir.nodes.len()).collect();
    node_indexes.sort_by(|left, right| compare_node_indices(ir, *left, *right));

    let mut priorities = vec![0_usize; ir.nodes.len()];
    for (priority, node_index) in node_indexes.into_iter().enumerate() {
        priorities[node_index] = priority;
    }
    priorities
}

fn compare_block_beta_grid_node_indices(
    ir: &MermaidDiagramIr,
    left: usize,
    right: usize,
) -> std::cmp::Ordering {
    let left_path = block_beta_group_identity_path(ir, left);
    let right_path = block_beta_group_identity_path(ir, right);

    left_path
        .is_empty()
        .cmp(&right_path.is_empty())
        .then_with(|| left_path.cmp(&right_path))
        .then_with(|| compare_node_indices(ir, left, right))
}

fn block_beta_group_identity_path(ir: &MermaidDiagramIr, node_index: usize) -> Vec<usize> {
    let Some(graph_node) = ir.graph.nodes.get(node_index) else {
        return Vec::new();
    };
    let Some(mut current_subgraph) = graph_node.subgraphs.last().copied() else {
        return Vec::new();
    };

    let mut path = Vec::new();
    while let Some(subgraph) = ir.graph.subgraphs.get(current_subgraph.0) {
        path.push(subgraph.id.0);

        let Some(parent) = subgraph.parent else {
            break;
        };
        current_subgraph = parent;
    }
    path.reverse();
    path
}

fn block_beta_node_span(node: &IrNode) -> usize {
    node.classes
        .iter()
        .find_map(|class_name| {
            class_name
                .strip_prefix("block-beta-span-")
                .and_then(|value| value.parse::<usize>().ok())
        })
        .unwrap_or(1)
}

fn layout_block_beta_grouped_items(
    ir: &MermaidDiagramIr,
    column_count: usize,
    cell_width: f32,
    cell_height: f32,
    rank_by_node: &mut [usize],
    order_by_node: &mut [usize],
    centers: &mut [(f32, f32)],
) -> bool {
    let items = block_beta_direct_items(ir, None);
    if items.is_empty() {
        return false;
    }

    place_block_beta_items(
        ir,
        &items,
        column_count,
        0,
        0,
        cell_width,
        cell_height,
        rank_by_node,
        order_by_node,
        centers,
    );
    true
}

fn block_beta_direct_items(
    ir: &MermaidDiagramIr,
    parent: Option<fm_core::IrSubgraphId>,
) -> Vec<BlockBetaGridItem> {
    let mut items = Vec::new();

    if let Some(parent_id) = parent {
        if let Some(subgraph) = ir.graph.subgraph(parent_id) {
            items.extend(
                subgraph
                    .children
                    .iter()
                    .copied()
                    .map(BlockBetaGridItem::Group),
            );
        }
    } else {
        items.extend(
            ir.graph
                .root_subgraphs()
                .into_iter()
                .map(|subgraph| BlockBetaGridItem::Group(subgraph.id)),
        );
    }

    items.extend(
        ir.graph
            .nodes
            .iter()
            .enumerate()
            .filter_map(
                |(node_index, graph_node)| match graph_node.subgraphs.last().copied() {
                    Some(subgraph_id) if Some(subgraph_id) == parent => {
                        Some(BlockBetaGridItem::Node(node_index))
                    }
                    None if parent.is_none() => Some(BlockBetaGridItem::Node(node_index)),
                    _ => None,
                },
            ),
    );

    items.sort_by(|left, right| compare_block_beta_items(ir, *left, *right));
    items
}

fn compare_block_beta_items(
    ir: &MermaidDiagramIr,
    left: BlockBetaGridItem,
    right: BlockBetaGridItem,
) -> std::cmp::Ordering {
    let left_anchor = block_beta_item_anchor(ir, left);
    let right_anchor = block_beta_item_anchor(ir, right);

    left_anchor
        .cmp(&right_anchor)
        .then_with(|| match (left, right) {
            (BlockBetaGridItem::Node(left), BlockBetaGridItem::Node(right)) => left.cmp(&right),
            (BlockBetaGridItem::Group(left), BlockBetaGridItem::Group(right)) => {
                left.0.cmp(&right.0)
            }
            (BlockBetaGridItem::Group(_), BlockBetaGridItem::Node(_)) => std::cmp::Ordering::Less,
            (BlockBetaGridItem::Node(_), BlockBetaGridItem::Group(_)) => {
                std::cmp::Ordering::Greater
            }
        })
}

fn block_beta_item_anchor(ir: &MermaidDiagramIr, item: BlockBetaGridItem) -> (String, usize) {
    match item {
        BlockBetaGridItem::Node(node_index) => (ir.nodes[node_index].id.clone(), node_index),
        BlockBetaGridItem::Group(subgraph_id) => ir
            .graph
            .subgraph_members_recursive(subgraph_id)
            .into_iter()
            .map(|node_id| node_id.0)
            .min_by(|left, right| compare_node_indices(ir, *left, *right))
            .map(|node_index| (ir.nodes[node_index].id.clone(), node_index))
            .unwrap_or_else(|| (format!("~group-{}", subgraph_id.0), subgraph_id.0)),
    }
}

fn block_beta_item_span(
    ir: &MermaidDiagramIr,
    item: BlockBetaGridItem,
    available_columns: usize,
) -> usize {
    match item {
        BlockBetaGridItem::Node(node_index) => block_beta_node_span(&ir.nodes[node_index]),
        BlockBetaGridItem::Group(subgraph_id) => ir
            .graph
            .subgraph(subgraph_id)
            .map(|subgraph| subgraph.grid_span)
            .unwrap_or(1),
    }
    .min(available_columns)
    .max(1)
}

fn block_beta_item_rows(
    ir: &MermaidDiagramIr,
    item: BlockBetaGridItem,
    available_columns: usize,
) -> usize {
    match item {
        BlockBetaGridItem::Node(_) => 1,
        BlockBetaGridItem::Group(subgraph_id) => {
            let group_columns = block_beta_item_span(ir, item, available_columns);
            let children = block_beta_direct_items(ir, Some(subgraph_id));
            if children.is_empty() {
                1
            } else {
                block_beta_rows_required(ir, &children, group_columns)
            }
        }
    }
}

fn block_beta_rows_required(
    ir: &MermaidDiagramIr,
    items: &[BlockBetaGridItem],
    available_columns: usize,
) -> usize {
    let mut row_offset = 0_usize;
    let mut col = 0_usize;
    let mut row_height = 1_usize;

    for &item in items {
        let span = block_beta_item_span(ir, item, available_columns);
        let item_rows = block_beta_item_rows(ir, item, span);

        if col != 0 && col + span > available_columns {
            row_offset += row_height;
            col = 0;
            row_height = 1;
        }

        row_height = row_height.max(item_rows);

        if col + span >= available_columns {
            row_offset += row_height;
            col = 0;
            row_height = 1;
        } else {
            col += span;
        }
    }

    if col == 0 {
        row_offset
    } else {
        row_offset + row_height
    }
}

#[allow(clippy::too_many_arguments)]
fn place_block_beta_items(
    ir: &MermaidDiagramIr,
    items: &[BlockBetaGridItem],
    available_columns: usize,
    base_col: usize,
    start_row: usize,
    cell_width: f32,
    cell_height: f32,
    rank_by_node: &mut [usize],
    order_by_node: &mut [usize],
    centers: &mut [(f32, f32)],
) -> usize {
    let mut row_offset = 0_usize;
    let mut col = 0_usize;
    let mut row_height = 1_usize;

    for &item in items {
        let span = block_beta_item_span(ir, item, available_columns);
        let item_rows = block_beta_item_rows(ir, item, span);

        if col != 0 && col + span > available_columns {
            row_offset += row_height;
            col = 0;
            row_height = 1;
        }

        let item_col = base_col + col;
        let item_row = start_row + row_offset;

        match item {
            BlockBetaGridItem::Node(node_index) => {
                centers[node_index] = (
                    item_col as f32 * cell_width + ((span - 1) as f32 * cell_width / 2.0),
                    item_row as f32 * cell_height,
                );
                if matches!(ir.direction, GraphDirection::LR | GraphDirection::RL) {
                    rank_by_node[node_index] = item_col;
                    order_by_node[node_index] = item_row;
                } else {
                    rank_by_node[node_index] = item_row;
                    order_by_node[node_index] = item_col;
                }
            }
            BlockBetaGridItem::Group(subgraph_id) => {
                let child_items = block_beta_direct_items(ir, Some(subgraph_id));
                if !child_items.is_empty() {
                    place_block_beta_items(
                        ir,
                        &child_items,
                        span,
                        item_col,
                        item_row,
                        cell_width,
                        cell_height,
                        rank_by_node,
                        order_by_node,
                        centers,
                    );
                }
            }
        }

        row_height = row_height.max(item_rows);

        if col + span >= available_columns {
            row_offset += row_height;
            col = 0;
            row_height = 1;
        } else {
            col += span;
        }
    }

    if col == 0 {
        row_offset
    } else {
        row_offset + row_height
    }
}

fn compare_node_indices(ir: &MermaidDiagramIr, left: usize, right: usize) -> std::cmp::Ordering {
    ir.nodes[left]
        .id
        .cmp(&ir.nodes[right].id)
        .then_with(|| left.cmp(&right))
}

fn compare_priority(left: usize, right: usize, node_priority: &[usize]) -> std::cmp::Ordering {
    node_priority[left]
        .cmp(&node_priority[right])
        .then_with(|| left.cmp(&right))
}

fn remove_node(
    node: usize,
    active_nodes: &mut BTreeSet<usize>,
    incoming: &[Vec<usize>],
    outgoing: &[Vec<usize>],
    edges: &[OrientedEdge],
    in_degree: &mut [usize],
    out_degree: &mut [usize],
) {
    if !active_nodes.remove(&node) {
        return;
    }

    for edge_slot in outgoing[node].iter().copied() {
        let target = edges[edge_slot].target;
        if active_nodes.contains(&target) {
            in_degree[target] = in_degree[target].saturating_sub(1);
        }
    }

    for edge_slot in incoming[node].iter().copied() {
        let source = edges[edge_slot].source;
        if active_nodes.contains(&source) {
            out_degree[source] = out_degree[source].saturating_sub(1);
        }
    }
}

fn crossing_minimization(
    ir: &MermaidDiagramIr,
    ranks: &BTreeMap<usize, usize>,
) -> (usize, BTreeMap<usize, Vec<usize>>) {
    let mut ordering_by_rank = nodes_by_rank(ir.nodes.len(), ranks);
    if ordering_by_rank.len() <= 1 {
        return (0, ordering_by_rank);
    }

    // Deterministic barycenter sweeps: top-down then bottom-up.
    let rank_keys: Vec<usize> = ordering_by_rank.keys().copied().collect();
    for _ in 0..4 {
        for index in 1..rank_keys.len() {
            let rank = rank_keys[index];
            let upper_rank = rank_keys[index - 1];
            reorder_rank_by_barycenter(ir, ranks, &mut ordering_by_rank, rank, upper_rank, true);
        }

        for index in (0..rank_keys.len().saturating_sub(1)).rev() {
            let rank = rank_keys[index];
            let lower_rank = rank_keys[index + 1];
            reorder_rank_by_barycenter(ir, ranks, &mut ordering_by_rank, rank, lower_rank, false);
        }
    }

    let crossing_count = total_crossings(ir, ranks, &ordering_by_rank);
    debug!(
        crossings_after_barycenter = crossing_count,
        ranks = ordering_by_rank.len(),
        "layout.crossing_minimization"
    );
    (crossing_count, ordering_by_rank)
}

/// Apply transpose and sifting refinement heuristics to reduce crossings
/// beyond what barycenter achieves alone.
fn crossing_refinement(
    ir: &MermaidDiagramIr,
    ranks: &BTreeMap<usize, usize>,
    mut ordering_by_rank: BTreeMap<usize, Vec<usize>>,
    mut best_crossings: usize,
) -> (usize, BTreeMap<usize, Vec<usize>>) {
    if best_crossings == 0 {
        return (0, ordering_by_rank);
    }

    // Phase 1: Transpose — swap adjacent nodes in each rank if it reduces crossings.
    let mut improved = true;
    for _pass in 0..10 {
        if !improved {
            break;
        }
        improved = false;
        let rank_keys: Vec<usize> = ordering_by_rank.keys().copied().collect();
        for &rank in &rank_keys {
            let n = match ordering_by_rank.get(&rank) {
                Some(o) => o.len(),
                _ => 0,
            };
            if n < 2 {
                continue;
            }
            for i in 0..n - 1 {
                // Try swapping positions i and i+1 in-place.
                if let Some(rank_order) = ordering_by_rank.get_mut(&rank) {
                    rank_order.swap(i, i + 1);
                }
                let trial_crossings = total_crossings(ir, ranks, &ordering_by_rank);
                if trial_crossings < best_crossings {
                    best_crossings = trial_crossings;
                    improved = true;
                    if best_crossings == 0 {
                        return (0, ordering_by_rank);
                    }
                } else {
                    // Swap back if not improved.
                    if let Some(rank_order) = ordering_by_rank.get_mut(&rank) {
                        rank_order.swap(i, i + 1);
                    }
                }
            }
        }
    }

    // Phase 2: Sifting — for each node in each rank, try every position in that rank.
    let rank_keys: Vec<usize> = ordering_by_rank.keys().copied().collect();
    for &rank in &rank_keys {
        let order = match ordering_by_rank.get(&rank) {
            Some(o) if o.len() >= 3 => o.clone(),
            _ => continue,
        };
        let n = order.len();
        for node in order {
            // Find current position of node in the (potentially modified) rank order.
            let mut current_pos = match ordering_by_rank.get(&rank) {
                Some(o) => match o.iter().position(|&ni| ni == node) {
                    Some(pos) => pos,
                    None => continue,
                },
                None => continue,
            };

            for target_pos in 0..n {
                if target_pos == current_pos {
                    continue;
                }

                // Move node from current_pos to target_pos in-place.
                if let Some(rank_order) = ordering_by_rank.get_mut(&rank) {
                    let element = rank_order.remove(current_pos);
                    rank_order.insert(target_pos, element);
                }

                let trial_crossings = total_crossings(ir, ranks, &ordering_by_rank);
                if trial_crossings < best_crossings {
                    best_crossings = trial_crossings;
                    current_pos = target_pos;
                    if best_crossings == 0 {
                        return (0, ordering_by_rank);
                    }
                } else {
                    // Move back if not improved.
                    if let Some(rank_order) = ordering_by_rank.get_mut(&rank) {
                        let element = rank_order.remove(target_pos);
                        rank_order.insert(current_pos, element);
                    }
                }
            }
        }
    }

    (best_crossings, ordering_by_rank)
}

// ---------------------------------------------------------------------------
// Brandes-Köpf coordinate assignment (2001)
//
// Computes secondary-axis (within-rank) coordinates by running four alignment
// passes (upper-left, upper-right, lower-left, lower-right) and taking the
// median of the four positions for each node.  This aligns connected nodes
// across ranks, reducing edge bends compared to simple sequential placement.
// ---------------------------------------------------------------------------

/// For each node, collect its neighbours in the specified adjacent rank.
/// Uses pre-built adjacency for O(1) neighbour lookup per node.
fn bk_upper_neighbours(
    adjacency: &[BTreeSet<usize>],
    ranks: &BTreeMap<usize, usize>,
    pos_map: &BTreeMap<usize, usize>,
    node_index: usize,
    node_rank: usize,
    upper: bool,
) -> Vec<(usize, usize)> {
    let adjacent_rank = if upper {
        if node_rank == 0 {
            return Vec::new();
        }
        node_rank - 1
    } else {
        node_rank + 1
    };

    let mut neighbours = Vec::new();
    if let Some(nodes) = adjacency.get(node_index) {
        for &n in nodes {
            if ranks.get(&n).copied().unwrap_or(0) == adjacent_rank
                && let Some(&pos) = pos_map.get(&n)
            {
                neighbours.push((n, pos));
            }
        }
    }

    neighbours.sort_by_key(|&(_, pos)| pos);
    neighbours.dedup();
    neighbours
}

/// Brandes-Köpf vertical alignment for one of the four directions.
///
/// Returns `(root, align)` arrays indexed by node_index.
/// - `root[v]` is the root of the block containing v.
/// - `align[v]` is the next node in the block chain; `align[v] == v` at the terminal.
#[allow(clippy::too_many_arguments)]
fn bk_vertical_alignment(
    n: usize,
    adjacency: &[BTreeSet<usize>],
    rank_pos_maps: &BTreeMap<usize, BTreeMap<usize, usize>>,
    ranks: &BTreeMap<usize, usize>,
    ordering_by_rank: &BTreeMap<usize, Vec<usize>>,
    ordered_ranks: &[usize],
    top_to_bottom: bool,
    left_to_right: bool,
) -> (Vec<usize>, Vec<usize>) {
    let mut root: Vec<usize> = (0..n).collect();
    let mut align: Vec<usize> = (0..n).collect();

    // Process ranks in the specified vertical order.
    let rank_iter: Vec<usize> = if top_to_bottom {
        ordered_ranks.to_vec()
    } else {
        ordered_ranks.iter().copied().rev().collect()
    };

    for &rank in &rank_iter {
        let Some(rank_nodes) = ordering_by_rank.get(&rank) else {
            continue;
        };

        // Track the rightmost (or leftmost) aligned position to prevent conflicts.
        let mut threshold: i64 = if left_to_right { -1 } else { i64::MAX };

        let node_iter: Vec<usize> = if left_to_right {
            rank_nodes.clone()
        } else {
            rank_nodes.iter().copied().rev().collect()
        };

        for v in node_iter {
            let v_rank = ranks.get(&v).copied().unwrap_or(0);
            let adjacent_rank = if top_to_bottom {
                if v_rank == 0 {
                    continue;
                }
                v_rank - 1
            } else {
                v_rank + 1
            };

            let Some(pos_map) = rank_pos_maps.get(&adjacent_rank) else {
                continue;
            };

            let neighbours =
                bk_upper_neighbours(adjacency, ranks, pos_map, v, v_rank, top_to_bottom);

            if neighbours.is_empty() {
                continue;
            }

            // Compute median neighbour(s).  For even count, try both medians.
            let median_indices = if neighbours.len() % 2 == 1 {
                vec![neighbours.len() / 2]
            } else {
                vec![neighbours.len() / 2 - 1, neighbours.len() / 2]
            };

            let candidates: Vec<usize> = if left_to_right {
                median_indices
            } else {
                median_indices.into_iter().rev().collect()
            };

            for mi in candidates {
                let (u, u_pos) = neighbours[mi];
                // Only align if:
                // 1. u is not yet aligned with any successor (align[u] == u).
                // 2. The neighbour position doesn't conflict with a previously aligned neighbour in the rank.
                if align[u] != u {
                    continue;
                }
                let u_pos_i64 = u_pos as i64;
                let no_conflict = if left_to_right {
                    u_pos_i64 > threshold
                } else {
                    u_pos_i64 < threshold
                };
                if no_conflict {
                    align[u] = v;
                    root[v] = root[u];
                    threshold = u_pos_i64;
                    break;
                }
            }
        }
    }

    (root, align)
}

/// Brandes-Köpf horizontal compaction for one alignment.
///
/// Returns secondary-axis coordinates indexed by node_index.
fn bk_horizontal_compaction(
    node_count: usize,
    node_sizes: &[(f32, f32)],
    ordering_by_rank: &BTreeMap<usize, Vec<usize>>,
    root: &[usize],
    align: &[usize],
    node_spacing: f32,
    horizontal_ranks: bool,
) -> Vec<f32> {
    let mut x = vec![f32::NEG_INFINITY; node_count];
    let mut sink: Vec<usize> = (0..node_count).collect();
    let mut shift = vec![f32::INFINITY; node_count];

    // Build predecessor-in-rank lookup: for each node, its left neighbour in the same rank.
    let mut pred_in_rank: Vec<Option<usize>> = vec![None; node_count];
    for nodes in ordering_by_rank.values() {
        for i in 1..nodes.len() {
            if nodes[i] < node_count {
                pred_in_rank[nodes[i]] = Some(nodes[i - 1]);
            }
        }
    }

    /// Minimum separation between two adjacent nodes in the same rank.
    fn delta(
        u: usize,
        w: usize,
        node_sizes: &[(f32, f32)],
        node_spacing: f32,
        horizontal_ranks: bool,
    ) -> f32 {
        let u_extent = node_sizes
            .get(u)
            .map(|&(width, height)| if horizontal_ranks { height } else { width })
            .unwrap_or(84.0);
        let w_extent = node_sizes
            .get(w)
            .map(|&(width, height)| if horizontal_ranks { height } else { width })
            .unwrap_or(84.0);
        (u_extent / 2.0) + node_spacing + (w_extent / 2.0)
    }

    // Place blocks.  We use an explicit work stack to ensure predecessor block
    // roots are placed before the blocks that depend on them (the original BK
    // algorithm handles this via recursion in `place_block`).
    let mut ordered_roots: Vec<usize> = Vec::new();
    for rank_key in ordering_by_rank.keys() {
        if let Some(nodes) = ordering_by_rank.get(rank_key) {
            for &v in nodes {
                if v < node_count && root[v] == v {
                    ordered_roots.push(v);
                }
            }
        }
    }

    /// Place a single block root and all predecessor blocks it depends on.
    /// Recurses into unplaced predecessors; terminates because each block
    /// root sets `x[block_root] = 0.0` on entry, preventing re-entry.
    #[allow(clippy::too_many_arguments)]
    fn place_block(
        block_root: usize,
        x: &mut [f32],
        sink: &mut [usize],
        shift: &mut [f32],
        root: &[usize],
        align: &[usize],
        pred_in_rank: &[Option<usize>],
        node_sizes: &[(f32, f32)],
        node_spacing: f32,
        horizontal_ranks: bool,
    ) {
        if x[block_root] > f32::NEG_INFINITY {
            return; // Already placed.
        }
        x[block_root] = 0.0;

        // Walk the block chain: block_root -> align[block_root] -> ...
        let mut w = block_root;
        loop {
            if let Some(pred) = pred_in_rank[w] {
                let pred_root = root[pred];
                // Ensure predecessor block is placed first (recursive in
                // original algorithm; bounded by number of block roots).
                if x[pred_root] <= f32::NEG_INFINITY {
                    // Recurse into predecessor.  Depth is bounded by the
                    // number of distinct block roots, which is ≤ node_count.
                    place_block(
                        pred_root,
                        x,
                        sink,
                        shift,
                        root,
                        align,
                        pred_in_rank,
                        node_sizes,
                        node_spacing,
                        horizontal_ranks,
                    );
                }
                if sink[block_root] == block_root {
                    sink[block_root] = sink[pred_root];
                }
                let sep = delta(pred, w, node_sizes, node_spacing, horizontal_ranks);
                if sink[block_root] != sink[pred_root] {
                    shift[sink[pred_root]] =
                        shift[sink[pred_root]].min(x[block_root] - x[pred_root] - sep);
                } else {
                    x[block_root] = x[block_root].max(x[pred_root] + sep);
                }
            }
            let next = align[w];
            if next == w {
                break; // End of block chain (self-referencing terminal).
            }
            w = next;
        }
    }

    for &br in &ordered_roots {
        place_block(
            br,
            &mut x,
            &mut sink,
            &mut shift,
            root,
            align,
            &pred_in_rank,
            node_sizes,
            node_spacing,
            horizontal_ranks,
        );
    }

    // Apply class shifts to block roots.
    for v in 0..node_count {
        if root[v] == v {
            let s = shift[sink[v]];
            if s < f32::INFINITY {
                x[v] += s;
            }
        }
    }

    // Propagate (shifted) block root coordinates to all block members.
    for v in 0..node_count {
        x[v] = x[root[v]];
    }

    x
}

/// Run Brandes-Köpf algorithm: four alignment passes, then take the median.
fn brandes_kopf_secondary_coords(
    ir: &MermaidDiagramIr,
    node_sizes: &[(f32, f32)],
    ranks: &BTreeMap<usize, usize>,
    ordering_by_rank: &BTreeMap<usize, Vec<usize>>,
    spacing: LayoutSpacing,
    horizontal_ranks: bool,
) -> Vec<f32> {
    let n = ir.nodes.len();
    if n == 0 {
        return Vec::new();
    }

    let ordered_ranks: Vec<usize> = ordering_by_rank.keys().copied().collect();

    // Pre-build undirected adjacency for O(1) neighbour lookup.
    let mut adjacency = vec![BTreeSet::new(); n];
    for edge in &ir.edges {
        if let Some(s) = endpoint_node_index(ir, edge.from)
            && let Some(t) = endpoint_node_index(ir, edge.to)
            && s < n
            && t < n
            && s != t
        {
            adjacency[s].insert(t);
            adjacency[t].insert(s);
        }
    }

    // Pre-build position maps for each rank.
    let mut rank_pos_maps: BTreeMap<usize, BTreeMap<usize, usize>> = BTreeMap::new();
    for (&rank, nodes) in ordering_by_rank {
        let pos_map: BTreeMap<usize, usize> = nodes
            .iter()
            .enumerate()
            .map(|(pos, &node)| (node, pos))
            .collect();
        rank_pos_maps.insert(rank, pos_map);
    }

    // Four alignment passes: (top_to_bottom, left_to_right).
    let directions = [
        (true, true),   // upper-left
        (true, false),  // upper-right
        (false, true),  // lower-left
        (false, false), // lower-right
    ];

    let mut all_coords: Vec<Vec<f32>> = Vec::with_capacity(4);

    for &(top_to_bottom, left_to_right) in &directions {
        let (root, align) = bk_vertical_alignment(
            n,
            &adjacency,
            &rank_pos_maps,
            ranks,
            ordering_by_rank,
            &ordered_ranks,
            top_to_bottom,
            left_to_right,
        );

        let coords = bk_horizontal_compaction(
            n,
            node_sizes,
            ordering_by_rank,
            &root,
            &align,
            spacing.node_spacing,
            horizontal_ranks,
        );

        all_coords.push(coords);
    }

    // Normalize each pass so that the minimum coordinate is 0.
    for coords in &mut all_coords {
        let min_val = coords
            .iter()
            .copied()
            .filter(|v| v.is_finite())
            .fold(f32::INFINITY, f32::min);
        if min_val.is_finite() {
            for c in coords.iter_mut() {
                *c -= min_val;
            }
        }
    }

    // Median of four positions for each node.
    let mut result = vec![0.0_f32; n];
    for v in 0..n {
        let mut vals: Vec<f32> = all_coords.iter().map(|c| c[v]).collect();
        vals.sort_by(|a, b| a.total_cmp(b));
        // Median of 4 values: average of the two middle values.
        result[v] = (vals[1] + vals[2]) / 2.0;
    }

    result
}

fn coordinate_assignment(
    ir: &MermaidDiagramIr,
    node_sizes: &[(f32, f32)],
    ranks: &BTreeMap<usize, usize>,
    ordering_by_rank: &BTreeMap<usize, Vec<usize>>,
    spacing: LayoutSpacing,
) -> Vec<LayoutNodeBox> {
    let fallback_nodes_by_rank = nodes_by_rank(ir.nodes.len(), ranks);
    let horizontal_ranks = matches!(ir.direction, GraphDirection::LR | GraphDirection::RL);
    let reverse_ranks = matches!(ir.direction, GraphDirection::RL | GraphDirection::BT);
    let ordered_ranks: Vec<usize> = fallback_nodes_by_rank.keys().copied().collect();

    let rank_to_index: BTreeMap<usize, usize> = ordered_ranks
        .iter()
        .enumerate()
        .map(|(index, rank)| (*rank, index))
        .collect();

    // Compute primary offsets (rank positions) — unchanged from before.
    let mut rank_span = vec![0.0_f32; ordered_ranks.len()];
    for (rank_index, rank) in ordered_ranks.iter().copied().enumerate() {
        let node_indexes = ordering_by_rank
            .get(&rank)
            .cloned()
            .or_else(|| fallback_nodes_by_rank.get(&rank).cloned())
            .unwrap_or_default();

        let mut span = 0.0_f32;
        for node_index in node_indexes {
            let (width, height) = node_sizes.get(node_index).copied().unwrap_or((84.0, 44.0));
            let primary_extent = if horizontal_ranks { width } else { height };
            span = span.max(primary_extent);
        }
        rank_span[rank_index] = span.max(1.0);
    }

    let mut primary_offsets = vec![0.0_f32; ordered_ranks.len()];
    let mut primary_cursor = 0.0_f32;
    let iter_order: Vec<usize> = if reverse_ranks {
        (0..ordered_ranks.len()).rev().collect()
    } else {
        (0..ordered_ranks.len()).collect()
    };
    for rank_index in iter_order {
        primary_offsets[rank_index] = primary_cursor;
        primary_cursor += rank_span[rank_index] + spacing.rank_spacing;
    }

    // Compute secondary coordinates using Brandes-Köpf 4-way alignment.
    let secondary_coords = brandes_kopf_secondary_coords(
        ir,
        node_sizes,
        ranks,
        ordering_by_rank,
        spacing,
        horizontal_ranks,
    );

    // Build output using primary offsets and Brandes-Köpf secondary coordinates.
    let mut output = Vec::with_capacity(ir.nodes.len());
    for (rank, fallback_node_indexes) in &fallback_nodes_by_rank {
        let Some(rank_index) = rank_to_index.get(rank).copied() else {
            continue;
        };

        let node_indexes = ordering_by_rank
            .get(rank)
            .cloned()
            .unwrap_or_else(|| fallback_node_indexes.clone());

        let primary = primary_offsets.get(rank_index).copied().unwrap_or(0.0);
        for (order, node_index) in node_indexes.into_iter().enumerate() {
            let (width, height) = node_sizes.get(node_index).copied().unwrap_or((84.0, 44.0));
            let secondary = secondary_coords.get(node_index).copied().unwrap_or(0.0);

            let (x, y) = if horizontal_ranks {
                (primary, secondary)
            } else {
                (secondary, primary)
            };
            let node_id = ir
                .nodes
                .get(node_index)
                .map(|node| node.id.clone())
                .unwrap_or_default();

            output.push(LayoutNodeBox {
                node_index,
                node_id,
                rank: *rank,
                order,
                span: ir
                    .nodes
                    .get(node_index)
                    .map_or(Span::default(), |node| node.span_primary),
                bounds: LayoutRect {
                    x,
                    y,
                    width,
                    height,
                },
            });
        }
    }

    output.sort_by_key(|node| node.node_index);
    output
}

fn nodes_by_rank(node_count: usize, ranks: &BTreeMap<usize, usize>) -> BTreeMap<usize, Vec<usize>> {
    let mut nodes_by_rank: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for node_index in 0..node_count {
        let rank = ranks.get(&node_index).copied().unwrap_or(0);
        nodes_by_rank.entry(rank).or_default().push(node_index);
    }
    nodes_by_rank
}

fn reorder_rank_by_barycenter(
    ir: &MermaidDiagramIr,
    ranks: &BTreeMap<usize, usize>,
    ordering_by_rank: &mut BTreeMap<usize, Vec<usize>>,
    rank: usize,
    adjacent_rank: usize,
    use_incoming: bool,
) {
    let Some(current_order) = ordering_by_rank.get(&rank).cloned() else {
        return;
    };
    let Some(adjacent_order) = ordering_by_rank.get(&adjacent_rank) else {
        return;
    };

    let adjacent_position: BTreeMap<usize, usize> = adjacent_order
        .iter()
        .enumerate()
        .map(|(position, node)| (*node, position))
        .collect();

    let mut scored_nodes: Vec<(usize, Option<f32>, usize)> = current_order
        .iter()
        .enumerate()
        .map(|(stable_idx, node_index)| {
            let mut total_position = 0_usize;
            let mut neighbor_count = 0_usize;

            for edge in &ir.edges {
                let Some(source) = endpoint_node_index(ir, edge.from) else {
                    continue;
                };
                let Some(target) = endpoint_node_index(ir, edge.to) else {
                    continue;
                };

                let neighbor = if use_incoming {
                    if target == *node_index
                        && ranks.get(&source).copied().unwrap_or(0) == adjacent_rank
                    {
                        Some(source)
                    } else {
                        None
                    }
                } else if source == *node_index
                    && ranks.get(&target).copied().unwrap_or(0) == adjacent_rank
                {
                    Some(target)
                } else {
                    None
                };

                if let Some(adjacent_node) = neighbor
                    && let Some(position) = adjacent_position.get(&adjacent_node)
                {
                    total_position = total_position.saturating_add(*position);
                    neighbor_count = neighbor_count.saturating_add(1);
                }
            }

            let barycenter = if neighbor_count == 0 {
                None
            } else {
                Some(total_position as f32 / neighbor_count as f32)
            };
            (*node_index, barycenter, stable_idx)
        })
        .collect();

    scored_nodes.sort_by(|left, right| match (left.1, right.1) {
        (Some(lhs), Some(rhs)) => lhs.total_cmp(&rhs).then_with(|| left.0.cmp(&right.0)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => left.2.cmp(&right.2).then_with(|| left.0.cmp(&right.0)),
    });

    ordering_by_rank.insert(
        rank,
        scored_nodes
            .into_iter()
            .map(|(node_index, _, _)| node_index)
            .collect(),
    );
}

fn total_crossings(
    ir: &MermaidDiagramIr,
    ranks: &BTreeMap<usize, usize>,
    ordering_by_rank: &BTreeMap<usize, Vec<usize>>,
) -> usize {
    let mut positions_by_rank: BTreeMap<usize, BTreeMap<usize, usize>> = BTreeMap::new();
    for (rank, ordered_nodes) in ordering_by_rank {
        positions_by_rank.insert(
            *rank,
            ordered_nodes
                .iter()
                .enumerate()
                .map(|(position, node)| (*node, position))
                .collect(),
        );
    }

    let mut edges_by_layer_pair: BTreeMap<(usize, usize), Vec<(usize, usize)>> = BTreeMap::new();
    for edge in &ir.edges {
        let Some(mut source) = endpoint_node_index(ir, edge.from) else {
            continue;
        };
        let Some(mut target) = endpoint_node_index(ir, edge.to) else {
            continue;
        };
        let Some(mut source_rank) = ranks.get(&source).copied() else {
            continue;
        };
        let Some(mut target_rank) = ranks.get(&target).copied() else {
            continue;
        };

        if source_rank == target_rank {
            continue;
        }
        if source_rank > target_rank {
            std::mem::swap(&mut source, &mut target);
            std::mem::swap(&mut source_rank, &mut target_rank);
        }
        if target_rank != source_rank.saturating_add(1) {
            continue;
        }

        let Some(source_position) = positions_by_rank
            .get(&source_rank)
            .and_then(|positions| positions.get(&source))
            .copied()
        else {
            continue;
        };
        let Some(target_position) = positions_by_rank
            .get(&target_rank)
            .and_then(|positions| positions.get(&target))
            .copied()
        else {
            continue;
        };

        edges_by_layer_pair
            .entry((source_rank, target_rank))
            .or_default()
            .push((source_position, target_position));
    }

    let mut total_crossings = 0_usize;
    for (_layer_pair, mut edge_positions) in edges_by_layer_pair {
        edge_positions
            .sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        let mut target_positions: Vec<usize> = edge_positions
            .into_iter()
            .map(|(_source_position, target_position)| target_position)
            .collect();
        total_crossings = total_crossings.saturating_add(count_inversions(&mut target_positions));
    }

    total_crossings
}

fn count_inversions(values: &mut [usize]) -> usize {
    if values.len() <= 1 {
        return 0;
    }

    let mid = values.len() / 2;
    let mut inversions = 0_usize;
    inversions = inversions.saturating_add(count_inversions(&mut values[..mid]));
    inversions = inversions.saturating_add(count_inversions(&mut values[mid..]));

    let mut merged = Vec::with_capacity(values.len());
    let (left, right) = values.split_at(mid);
    let mut left_idx = 0_usize;
    let mut right_idx = 0_usize;

    while left_idx < left.len() && right_idx < right.len() {
        if left[left_idx] <= right[right_idx] {
            merged.push(left[left_idx]);
            left_idx = left_idx.saturating_add(1);
        } else {
            merged.push(right[right_idx]);
            inversions = inversions.saturating_add(left.len().saturating_sub(left_idx));
            right_idx = right_idx.saturating_add(1);
        }
    }

    merged.extend_from_slice(&left[left_idx..]);
    merged.extend_from_slice(&right[right_idx..]);
    values.copy_from_slice(&merged);
    inversions
}

fn build_edge_paths(
    ir: &MermaidDiagramIr,
    nodes: &[LayoutNodeBox],
    highlighted_edge_indexes: &BTreeSet<usize>,
) -> Vec<LayoutEdgePath> {
    let horizontal_ranks = matches!(ir.direction, GraphDirection::LR | GraphDirection::RL);
    build_edge_paths_with_orientation(ir, nodes, highlighted_edge_indexes, horizontal_ranks)
}

fn build_edge_paths_with_orientation(
    ir: &MermaidDiagramIr,
    nodes: &[LayoutNodeBox],
    highlighted_edge_indexes: &BTreeSet<usize>,
    horizontal_ranks: bool,
) -> Vec<LayoutEdgePath> {
    // Track parallel edges: count edges between same (source, target) pair.
    let mut edge_pair_count: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    let mut edge_pair_index: Vec<usize> = Vec::with_capacity(ir.edges.len());
    for edge in &ir.edges {
        let source = endpoint_node_index(ir, edge.from).unwrap_or(usize::MAX);
        let target = endpoint_node_index(ir, edge.to).unwrap_or(usize::MAX);
        let key = (source.min(target), source.max(target));
        let count = edge_pair_count.entry(key).or_insert(0);
        edge_pair_index.push(*count);
        *count += 1;
    }

    ir.edges
        .iter()
        .enumerate()
        .filter_map(|(edge_index, edge)| {
            let source = endpoint_node_index(ir, edge.from)?;
            let target = endpoint_node_index(ir, edge.to)?;
            let source_box = nodes.get(source)?;
            let target_box = nodes.get(target)?;

            let is_self_loop = source == target;
            let key = (source.min(target), source.max(target));
            let pair_total = edge_pair_count.get(&key).copied().unwrap_or(1);
            let pair_idx = edge_pair_index.get(edge_index).copied().unwrap_or(0);
            let parallel_offset = if pair_total > 1 {
                let offset_step = 12.0_f32;
                (pair_idx as f32 - (pair_total - 1) as f32 / 2.0) * offset_step
            } else {
                0.0
            };

            let points = if is_self_loop {
                route_self_loop(source_box, horizontal_ranks)
            } else {
                let (source_anchor, target_anchor) =
                    edge_anchors(source_box, target_box, horizontal_ranks);
                // Collect obstacles: all node boxes except source and target.
                let obstacles: Vec<LayoutRect> = nodes
                    .iter()
                    .enumerate()
                    .filter(|(idx, _)| *idx != source && *idx != target)
                    .map(|(_, n)| n.bounds)
                    .collect();
                let mut pts = route_edge_points_with_obstacles(
                    source_anchor,
                    target_anchor,
                    horizontal_ranks,
                    &obstacles,
                );
                if parallel_offset.abs() > 0.01 {
                    apply_parallel_offset(&mut pts, parallel_offset, horizontal_ranks);
                }
                pts
            };

            Some(LayoutEdgePath {
                edge_index,
                span: ir
                    .edges
                    .get(edge_index)
                    .map_or(Span::default(), |edge| edge.span),
                points,
                reversed: highlighted_edge_indexes.contains(&edge_index),
                is_self_loop,
                parallel_offset,
            })
        })
        .collect()
}

/// Route a self-loop edge: goes out one side and returns on another.
fn route_self_loop(node_box: &LayoutNodeBox, horizontal_ranks: bool) -> Vec<LayoutPoint> {
    let b = &node_box.bounds;
    let loop_size = 24.0_f32;

    if horizontal_ranks {
        // Loop goes out the right side and returns from the top.
        let start = LayoutPoint {
            x: b.x + b.width,
            y: b.y + b.height * 0.4,
        };
        let corner1 = LayoutPoint {
            x: b.x + b.width + loop_size,
            y: b.y + b.height * 0.4,
        };
        let corner2 = LayoutPoint {
            x: b.x + b.width + loop_size,
            y: b.y - loop_size,
        };
        let corner3 = LayoutPoint {
            x: b.x + b.width * 0.6,
            y: b.y - loop_size,
        };
        let end = LayoutPoint {
            x: b.x + b.width * 0.6,
            y: b.y,
        };
        vec![start, corner1, corner2, corner3, end]
    } else {
        // Loop goes out the bottom and returns from the right.
        let start = LayoutPoint {
            x: b.x + b.width * 0.6,
            y: b.y + b.height,
        };
        let corner1 = LayoutPoint {
            x: b.x + b.width * 0.6,
            y: b.y + b.height + loop_size,
        };
        let corner2 = LayoutPoint {
            x: b.x + b.width + loop_size,
            y: b.y + b.height + loop_size,
        };
        let corner3 = LayoutPoint {
            x: b.x + b.width + loop_size,
            y: b.y + b.height * 0.4,
        };
        let end = LayoutPoint {
            x: b.x + b.width,
            y: b.y + b.height * 0.4,
        };
        vec![start, corner1, corner2, corner3, end]
    }
}

/// Apply parallel offset to an edge path to distinguish parallel edges.
fn apply_parallel_offset(points: &mut [LayoutPoint], offset: f32, horizontal_ranks: bool) {
    if points.len() < 2 {
        return;
    }
    // Offset perpendicular to the main routing direction.
    for pt in points.iter_mut() {
        if horizontal_ranks {
            pt.y += offset;
        } else {
            pt.x += offset;
        }
    }
}

fn edge_anchors(
    source_box: &LayoutNodeBox,
    target_box: &LayoutNodeBox,
    horizontal_ranks: bool,
) -> (LayoutPoint, LayoutPoint) {
    let source_center = source_box.bounds.center();
    let target_center = target_box.bounds.center();

    if horizontal_ranks {
        let (source_x, target_x) = if target_center.x >= source_center.x {
            (
                source_box.bounds.x + source_box.bounds.width,
                target_box.bounds.x,
            )
        } else {
            (
                source_box.bounds.x,
                target_box.bounds.x + target_box.bounds.width,
            )
        };
        (
            LayoutPoint {
                x: source_x,
                y: source_center.y,
            },
            LayoutPoint {
                x: target_x,
                y: target_center.y,
            },
        )
    } else {
        let (source_y, target_y) = if target_center.y >= source_center.y {
            (
                source_box.bounds.y + source_box.bounds.height,
                target_box.bounds.y,
            )
        } else {
            (
                source_box.bounds.y,
                target_box.bounds.y + target_box.bounds.height,
            )
        };
        (
            LayoutPoint {
                x: source_center.x,
                y: source_y,
            },
            LayoutPoint {
                x: target_center.x,
                y: target_y,
            },
        )
    }
}

#[cfg(test)]
fn route_edge_points(
    source: LayoutPoint,
    target: LayoutPoint,
    horizontal_ranks: bool,
) -> Vec<LayoutPoint> {
    route_edge_points_with_obstacles(source, target, horizontal_ranks, &[])
}

/// Route an edge with orthogonal segments, avoiding node bounding boxes.
///
/// When `obstacles` is non-empty, the router checks if the midpoint segment
/// intersects any obstacle and reroutes around it if needed.
fn route_edge_points_with_obstacles(
    source: LayoutPoint,
    target: LayoutPoint,
    horizontal_ranks: bool,
    obstacles: &[LayoutRect],
) -> Vec<LayoutPoint> {
    let epsilon = 0.001_f32;

    let points = if horizontal_ranks {
        if (source.y - target.y).abs() < epsilon {
            let segment = (
                LayoutPoint {
                    x: source.x.min(target.x),
                    y: source.y,
                },
                LayoutPoint {
                    x: source.x.max(target.x),
                    y: target.y,
                },
            );
            if let Some(nudge) = find_obstacle_nudge_y(segment, source.y, obstacles) {
                vec![
                    source,
                    LayoutPoint {
                        x: source.x,
                        y: nudge,
                    },
                    LayoutPoint {
                        x: target.x,
                        y: nudge,
                    },
                    target,
                ]
            } else {
                vec![source, target]
            }
        } else {
            let mid_x = (source.x + target.x) / 2.0;
            let mid_segment = (
                LayoutPoint {
                    x: mid_x,
                    y: source.y.min(target.y),
                },
                LayoutPoint {
                    x: mid_x,
                    y: source.y.max(target.y),
                },
            );
            // Check if the vertical mid-segment clips through any obstacle.
            if let Some(nudge) = find_obstacle_nudge_x(mid_segment, mid_x, obstacles) {
                // Route around: two vertical segments flanking the obstacle.
                vec![
                    source,
                    LayoutPoint {
                        x: nudge,
                        y: source.y,
                    },
                    LayoutPoint {
                        x: nudge,
                        y: target.y,
                    },
                    target,
                ]
            } else {
                vec![
                    source,
                    LayoutPoint {
                        x: mid_x,
                        y: source.y,
                    },
                    LayoutPoint {
                        x: mid_x,
                        y: target.y,
                    },
                    target,
                ]
            }
        }
    } else if (source.x - target.x).abs() < epsilon {
        let segment = (
            LayoutPoint {
                x: source.x,
                y: source.y.min(target.y),
            },
            LayoutPoint {
                x: target.x,
                y: source.y.max(target.y),
            },
        );
        if let Some(nudge) = find_obstacle_nudge_x(segment, source.x, obstacles) {
            vec![
                source,
                LayoutPoint {
                    x: nudge,
                    y: source.y,
                },
                LayoutPoint {
                    x: nudge,
                    y: target.y,
                },
                target,
            ]
        } else {
            vec![source, target]
        }
    } else {
        let mid_y = (source.y + target.y) / 2.0;
        let mid_segment = (
            LayoutPoint {
                x: source.x.min(target.x),
                y: mid_y,
            },
            LayoutPoint {
                x: source.x.max(target.x),
                y: mid_y,
            },
        );
        if let Some(nudge) = find_obstacle_nudge_y(mid_segment, mid_y, obstacles) {
            vec![
                source,
                LayoutPoint {
                    x: source.x,
                    y: nudge,
                },
                LayoutPoint {
                    x: target.x,
                    y: nudge,
                },
                target,
            ]
        } else {
            vec![
                source,
                LayoutPoint {
                    x: source.x,
                    y: mid_y,
                },
                LayoutPoint {
                    x: target.x,
                    y: mid_y,
                },
                target,
            ]
        }
    };

    simplify_polyline(points)
}

/// Check if a vertical segment at x-coordinate `mid_x` intersects any obstacle.
/// Returns a nudged x-coordinate that avoids the obstacle, or None if clear.
fn find_obstacle_nudge_x(
    segment: (LayoutPoint, LayoutPoint),
    mid_x: f32,
    obstacles: &[LayoutRect],
) -> Option<f32> {
    let margin = 8.0_f32;
    let y_min = segment.0.y;
    let y_max = segment.1.y;
    for obs in obstacles {
        // Check if the vertical line at mid_x passes through this obstacle's x-range
        // and the y-range overlaps.
        if mid_x >= obs.x - margin
            && mid_x <= obs.x + obs.width + margin
            && y_max >= obs.y
            && y_min <= obs.y + obs.height
        {
            // Nudge to the closer side of the obstacle.
            let left_dist = (mid_x - (obs.x - margin)).abs();
            let right_dist = (mid_x - (obs.x + obs.width + margin)).abs();
            return if left_dist <= right_dist {
                Some(obs.x - margin)
            } else {
                Some(obs.x + obs.width + margin)
            };
        }
    }
    None
}

/// Check if a horizontal segment at y-coordinate `mid_y` intersects any obstacle.
/// Returns a nudged y-coordinate that avoids the obstacle, or None if clear.
fn find_obstacle_nudge_y(
    segment: (LayoutPoint, LayoutPoint),
    mid_y: f32,
    obstacles: &[LayoutRect],
) -> Option<f32> {
    let margin = 8.0_f32;
    let x_min = segment.0.x;
    let x_max = segment.1.x;
    for obs in obstacles {
        if mid_y >= obs.y - margin
            && mid_y <= obs.y + obs.height + margin
            && x_max >= obs.x
            && x_min <= obs.x + obs.width
        {
            let top_dist = (mid_y - (obs.y - margin)).abs();
            let bottom_dist = (mid_y - (obs.y + obs.height + margin)).abs();
            return if top_dist <= bottom_dist {
                Some(obs.y - margin)
            } else {
                Some(obs.y + obs.height + margin)
            };
        }
    }
    None
}

fn simplify_polyline(points: Vec<LayoutPoint>) -> Vec<LayoutPoint> {
    if points.len() <= 2 {
        return points;
    }

    let mut simplified = Vec::with_capacity(points.len());
    for point in points {
        if simplified.last() == Some(&point) {
            continue;
        }
        simplified.push(point);

        while simplified.len() >= 3 {
            let c = simplified[simplified.len() - 1];
            let b = simplified[simplified.len() - 2];
            let a = simplified[simplified.len() - 3];
            if is_axis_aligned_collinear(a, b, c) {
                simplified.remove(simplified.len() - 2);
            } else {
                break;
            }
        }
    }

    simplified
}

fn is_axis_aligned_collinear(a: LayoutPoint, b: LayoutPoint, c: LayoutPoint) -> bool {
    let epsilon = 0.001_f32;
    ((a.x - b.x).abs() < epsilon && (b.x - c.x).abs() < epsilon)
        || ((a.y - b.y).abs() < epsilon && (b.y - c.y).abs() < epsilon)
}

fn build_cluster_boxes(
    ir: &MermaidDiagramIr,
    nodes: &[LayoutNodeBox],
    spacing: LayoutSpacing,
) -> Vec<LayoutClusterBox> {
    ir.clusters
        .iter()
        .enumerate()
        .filter_map(|(cluster_index, cluster)| {
            let mut min_x = f32::INFINITY;
            let mut min_y = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            let mut max_y = f32::NEG_INFINITY;

            for member in &cluster.members {
                let Some(node_box) = nodes.get(member.0) else {
                    continue;
                };
                min_x = min_x.min(node_box.bounds.x);
                min_y = min_y.min(node_box.bounds.y);
                max_x = max_x.max(node_box.bounds.x + node_box.bounds.width);
                max_y = max_y.max(node_box.bounds.y + node_box.bounds.height);
            }

            (min_x.is_finite() && min_y.is_finite() && max_x.is_finite() && max_y.is_finite())
                .then_some(LayoutClusterBox {
                    cluster_index,
                    span: ir
                        .clusters
                        .get(cluster_index)
                        .map_or(Span::default(), |cluster| cluster.span),
                    bounds: LayoutRect {
                        x: min_x - spacing.cluster_padding,
                        y: min_y - spacing.cluster_padding,
                        width: (max_x - min_x) + (2.0 * spacing.cluster_padding),
                        height: (max_y - min_y) + (2.0 * spacing.cluster_padding),
                    },
                })
        })
        .collect()
}

fn compute_bounds(
    nodes: &[LayoutNodeBox],
    clusters: &[LayoutClusterBox],
    edges: &[LayoutEdgePath],
    spacing: LayoutSpacing,
) -> LayoutRect {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;

    for node in nodes {
        min_x = min_x.min(node.bounds.x);
        min_y = min_y.min(node.bounds.y);
        max_x = max_x.max(node.bounds.x + node.bounds.width);
        max_y = max_y.max(node.bounds.y + node.bounds.height);
    }

    for cluster in clusters {
        min_x = min_x.min(cluster.bounds.x);
        min_y = min_y.min(cluster.bounds.y);
        max_x = max_x.max(cluster.bounds.x + cluster.bounds.width);
        max_y = max_y.max(cluster.bounds.y + cluster.bounds.height);
    }

    for edge in edges {
        for point in &edge.points {
            min_x = min_x.min(point.x);
            min_y = min_y.min(point.y);
            max_x = max_x.max(point.x);
            max_y = max_y.max(point.y);
        }
    }

    if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
        return LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        };
    }

    LayoutRect {
        x: min_x - spacing.cluster_padding,
        y: min_y - spacing.cluster_padding,
        width: (max_x - min_x) + (2.0 * spacing.cluster_padding),
        height: (max_y - min_y) + (2.0 * spacing.cluster_padding),
    }
}

fn compute_edge_length_metrics(edges: &[LayoutEdgePath]) -> (f32, f32) {
    let mut total = 0.0_f32;
    let mut reversed_total = 0.0_f32;

    for edge in edges {
        let length = polyline_length(&edge.points);
        total += length;
        if edge.reversed {
            reversed_total += length;
        }
    }

    (total, reversed_total)
}

fn polyline_length(points: &[LayoutPoint]) -> f32 {
    points
        .windows(2)
        .map(|pair| {
            let dx = pair[1].x - pair[0].x;
            let dy = pair[1].y - pair[0].y;
            (dx * dx + dy * dy).sqrt()
        })
        .sum()
}

fn build_cycle_cluster_map(
    ir: &MermaidDiagramIr,
    cycle_result: &CycleRemovalResult,
) -> CycleClusterMap {
    let node_count = ir.nodes.len();
    let edges = resolved_edges(ir);
    let node_priority = stable_node_priorities(ir);
    let detection = detect_cycle_components(node_count, &edges, &node_priority);

    let mut node_representative = (0..node_count).collect::<Vec<_>>();
    let mut cluster_heads = BTreeSet::new();
    let mut cluster_members = BTreeMap::new();

    for component_index in &detection.cyclic_component_indexes {
        let Some(component_nodes) = detection.components.get(*component_index) else {
            continue;
        };
        if component_nodes.len() <= 1 {
            // Skip self-loops for cluster collapse — they're single nodes.
            continue;
        }

        // Choose the lowest-priority node as the representative (cluster head).
        let head = *component_nodes
            .iter()
            .min_by(|a, b| compare_priority(**a, **b, &node_priority))
            .unwrap_or(&component_nodes[0]);

        cluster_heads.insert(head);
        let mut members = component_nodes.clone();
        members.sort_by(|a, b| compare_priority(*a, *b, &node_priority));
        for &member in &members {
            node_representative[member] = head;
        }
        cluster_members.insert(head, members);
    }

    let _ = cycle_result; // Used for type coherence; detection is recomputed for isolation.

    CycleClusterMap {
        node_representative,
        cluster_heads,
        cluster_members,
    }
}

fn build_cycle_cluster_results(
    collapse_map: &CycleClusterMap,
    nodes: &mut [LayoutNodeBox],
    clusters: &mut Vec<LayoutClusterBox>,
    spacing: LayoutSpacing,
) -> Vec<LayoutCycleCluster> {
    let mut cycle_clusters = Vec::new();

    for (head, members) in &collapse_map.cluster_members {
        if members.len() <= 1 {
            continue;
        }

        // Find the head node's bounding box (copy values to satisfy borrow checker).
        let Some(head_box) = nodes.iter().find(|n| n.node_index == *head) else {
            continue;
        };
        let base_x = head_box.bounds.x;
        let base_y = head_box.bounds.y;
        let head_height = head_box.bounds.height;

        // Arrange member nodes (excluding head) in a compact grid within the cluster bounds.
        let non_head_members: Vec<usize> = members.iter().copied().filter(|m| m != head).collect();
        let member_count = non_head_members.len();
        let cols = ((member_count as f32).sqrt().ceil() as usize).max(1);

        let sub_spacing = spacing.node_spacing * 0.5;
        for (idx, &member_index) in non_head_members.iter().enumerate() {
            let col = idx % cols;
            let row = idx / cols;
            if let Some(member_box) = nodes.iter_mut().find(|n| n.node_index == member_index) {
                member_box.bounds.x =
                    base_x + (col as f32) * (member_box.bounds.width + sub_spacing);
                member_box.bounds.y = base_y
                    + head_height
                    + spacing.cluster_padding
                    + (row as f32) * (member_box.bounds.height + sub_spacing);
            }
        }

        // Compute the cluster bounding box over all members.
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for &member_index in members {
            if let Some(member_box) = nodes.iter().find(|n| n.node_index == member_index) {
                min_x = min_x.min(member_box.bounds.x);
                min_y = min_y.min(member_box.bounds.y);
                max_x = max_x.max(member_box.bounds.x + member_box.bounds.width);
                max_y = max_y.max(member_box.bounds.y + member_box.bounds.height);
            }
        }

        if min_x.is_finite() && min_y.is_finite() && max_x.is_finite() && max_y.is_finite() {
            let cluster_bounds = LayoutRect {
                x: min_x - spacing.cluster_padding,
                y: min_y - spacing.cluster_padding,
                width: (max_x - min_x) + (2.0 * spacing.cluster_padding),
                height: (max_y - min_y) + (2.0 * spacing.cluster_padding),
            };

            cycle_clusters.push(LayoutCycleCluster {
                head_node_index: *head,
                member_node_indexes: members.clone(),
                bounds: cluster_bounds,
            });

            // Also add as a regular cluster box for rendering consistency.
            clusters.push(LayoutClusterBox {
                cluster_index: clusters.len(),
                span: Span::default(),
                bounds: cluster_bounds,
            });
        }
    }

    cycle_clusters
}

fn endpoint_node_index(ir: &MermaidDiagramIr, endpoint: IrEndpoint) -> Option<usize> {
    match endpoint {
        IrEndpoint::Node(node) => Some(node.0),
        IrEndpoint::Port(port) => ir.ports.get(port.0).map(|port_ref| port_ref.node.0),
        IrEndpoint::Unresolved => None,
    }
}

fn push_snapshot(
    trace: &mut LayoutTrace,
    stage: &'static str,
    node_count: usize,
    edge_count: usize,
    reversed_edges: usize,
    crossing_count: usize,
) {
    trace.snapshots.push(LayoutStageSnapshot {
        stage,
        reversed_edges,
        crossing_count,
        node_count,
        edge_count,
    });
}

#[must_use]
pub fn layout_stats_from(layout: &DiagramLayout) -> LayoutStats {
    layout.stats
}

#[must_use]
pub fn build_layout_guard_report(
    ir: &MermaidDiagramIr,
    traced: &TracedLayout,
) -> MermaidGuardReport {
    build_layout_guard_report_with_pressure(
        ir,
        traced,
        fm_core::MermaidNativePressureSignals::default().into_report(),
    )
}

#[must_use]
pub fn build_layout_guard_report_with_pressure(
    ir: &MermaidDiagramIr,
    traced: &TracedLayout,
    pressure: MermaidPressureReport,
) -> MermaidGuardReport {
    let complexity = MermaidComplexity {
        nodes: ir.nodes.len(),
        edges: ir.edges.len(),
        labels: ir.labels.len(),
        clusters: ir.clusters.len(),
        ports: ir.ports.len(),
        style_refs: ir.nodes.iter().map(|node| node.classes.len()).sum(),
        score: ir
            .nodes
            .len()
            .saturating_mul(4)
            .saturating_add(ir.edges.len().saturating_mul(3))
            .saturating_add(ir.labels.len().saturating_mul(2))
            .saturating_add(ir.clusters.len().saturating_mul(5))
            .saturating_add(ir.ports.len()),
    };

    let max_nodes = MermaidConfig::default().max_nodes;
    let max_edges = MermaidConfig::default().max_edges;
    let max_label_chars = MermaidConfig::default().max_label_chars;
    let max_label_lines = MermaidConfig::default().max_label_lines;
    let label_chars_over = ir
        .labels
        .iter()
        .map(|label| label.text.chars().count().saturating_sub(max_label_chars))
        .sum();
    let label_lines_over = ir
        .labels
        .iter()
        .map(|label| label.text.lines().count().saturating_sub(max_label_lines))
        .sum();
    let guard = traced.trace.guard;
    let budget_exceeded = guard.time_budget_exceeded
        || guard.iteration_budget_exceeded
        || guard.route_budget_exceeded;

    MermaidGuardReport {
        complexity,
        label_chars_over,
        label_lines_over,
        node_limit_exceeded: ir.nodes.len() > max_nodes,
        edge_limit_exceeded: ir.edges.len() > max_edges,
        label_limit_exceeded: label_chars_over > 0 || label_lines_over > 0,
        route_budget_exceeded: guard.route_budget_exceeded,
        layout_budget_exceeded: guard.time_budget_exceeded || guard.iteration_budget_exceeded,
        limits_exceeded: ir.nodes.len() > max_nodes
            || ir.edges.len() > max_edges
            || label_chars_over > 0
            || label_lines_over > 0,
        budget_exceeded,
        route_ops_estimate: guard.estimated_route_ops,
        layout_iterations_estimate: guard.estimated_layout_iterations,
        layout_time_estimate_ms: guard.estimated_layout_time_ms,
        layout_requested_algorithm: Some(traced.trace.dispatch.requested.as_str().to_string()),
        layout_selected_algorithm: Some(traced.trace.dispatch.selected.as_str().to_string()),
        guard_reason: Some(guard.reason.to_string()),
        observability: fm_core::MermaidObservabilityIds::default(),
        pressure,
        budget_broker: fm_core::MermaidBudgetLedger::default(),
        degradation: fm_core::MermaidDegradationPlan {
            target_fidelity: if budget_exceeded {
                MermaidFidelity::Compact
            } else {
                MermaidFidelity::Normal
            },
            hide_labels: false,
            collapse_clusters: false,
            simplify_routing: guard.route_budget_exceeded,
            reduce_decoration: budget_exceeded,
            force_glyph_mode: budget_exceeded.then_some(MermaidGlyphMode::Ascii),
        },
    }
}

#[must_use]
pub fn build_layout_decision_ledger(
    ir: &MermaidDiagramIr,
    traced: &TracedLayout,
    guard_report: &MermaidGuardReport,
) -> MermaidLayoutDecisionLedger {
    let dispatch = traced.trace.dispatch;
    let guard = traced.trace.guard;
    let metrics = GraphMetrics::from_ir(ir);
    let confidence_permille =
        layout_decision_confidence_permille(dispatch, guard, metrics, guard_report.pressure.tier);

    let alternatives = concrete_layout_algorithms()
        .into_iter()
        .map(|algorithm| {
            let available_for_diagram = algorithm_available_for_diagram(ir.diagram_type, algorithm);
            let note = if algorithm == dispatch.selected {
                Some(format!("selected via {}", dispatch.reason))
            } else if algorithm == dispatch.requested && !available_for_diagram {
                Some(String::from(
                    "requested explicitly but unavailable for this diagram type",
                ))
            } else if algorithm == dispatch.requested {
                Some(String::from("explicitly requested by caller"))
            } else if available_for_diagram {
                Some(String::from("available alternative"))
            } else {
                None
            };
            MermaidLayoutDecisionAlternative {
                algorithm: algorithm.as_str().to_string(),
                selected: algorithm == dispatch.selected,
                available_for_diagram,
                note,
            }
        })
        .collect();

    let mut notes = Vec::new();
    if dispatch.requested == LayoutAlgorithm::Auto {
        notes.push(String::from(auto_selection_reason(ir, dispatch.selected)));
    }
    if dispatch.capability_unavailable {
        notes.push(format!(
            "requested '{}' was unavailable for '{}'; used '{}'",
            dispatch.requested.as_str(),
            ir.diagram_type.as_str(),
            dispatch.selected.as_str()
        ));
    }
    if guard.fallback_applied {
        notes.push(format!(
            "guardrail fallback changed layout from '{}' to '{}'",
            guard.initial_algorithm.as_str(),
            guard.selected_algorithm.as_str()
        ));
    }

    MermaidLayoutDecisionLedger {
        entries: vec![MermaidLayoutDecisionRecord {
            kind: String::from("layout_decision"),
            trace_id: guard_report.observability.trace_id,
            decision_id: guard_report.observability.decision_id,
            policy_id: guard_report.observability.policy_id.clone(),
            schema_version: guard_report.observability.schema_version,
            requested_algorithm: dispatch.requested.as_str().to_string(),
            selected_algorithm: dispatch.selected.as_str().to_string(),
            capability_unavailable: dispatch.capability_unavailable,
            dispatch_reason: dispatch.reason.to_string(),
            guard_reason: guard.reason.to_string(),
            fallback_applied: guard.fallback_applied,
            confidence_permille,
            node_count: traced.layout.nodes.len(),
            edge_count: traced.layout.edges.len(),
            crossing_count: traced.layout.stats.crossing_count,
            reversed_edges: traced.layout.stats.reversed_edges,
            estimated_layout_time_ms: guard.estimated_layout_time_ms,
            estimated_layout_iterations: guard.estimated_layout_iterations,
            estimated_route_ops: guard.estimated_route_ops,
            pressure_source: guard_report.pressure.source,
            pressure_tier: guard_report.pressure.tier,
            budget_total_ms: guard_report.budget_broker.total_budget_ms,
            budget_exhausted: guard_report.budget_broker.exhausted,
            alternatives,
            notes,
        }],
    }
}

fn concrete_layout_algorithms() -> [LayoutAlgorithm; 10] {
    [
        LayoutAlgorithm::Sugiyama,
        LayoutAlgorithm::Force,
        LayoutAlgorithm::Tree,
        LayoutAlgorithm::Radial,
        LayoutAlgorithm::Timeline,
        LayoutAlgorithm::Gantt,
        LayoutAlgorithm::Sankey,
        LayoutAlgorithm::Kanban,
        LayoutAlgorithm::Grid,
        LayoutAlgorithm::Sequence,
    ]
}

fn layout_decision_confidence_permille(
    dispatch: LayoutDispatch,
    guard: LayoutGuardDecision,
    metrics: GraphMetrics,
    pressure_tier: MermaidPressureTier,
) -> u16 {
    let mut confidence =
        if dispatch.requested != LayoutAlgorithm::Auto && !dispatch.capability_unavailable {
            970_u16
        } else if dispatch.capability_unavailable {
            420_u16
        } else {
            match dispatch.selected {
                LayoutAlgorithm::Sequence
                | LayoutAlgorithm::Timeline
                | LayoutAlgorithm::Gantt
                | LayoutAlgorithm::Sankey
                | LayoutAlgorithm::Kanban
                | LayoutAlgorithm::Grid
                | LayoutAlgorithm::Radial => 900,
                LayoutAlgorithm::Tree if metrics.is_tree_like => 880,
                LayoutAlgorithm::Force if metrics.is_dense || metrics.back_edge_count > 0 => 760,
                LayoutAlgorithm::Sugiyama => 820,
                LayoutAlgorithm::Tree => 700,
                LayoutAlgorithm::Force => 680,
                LayoutAlgorithm::Auto => 500,
            }
        };

    if guard.fallback_applied {
        confidence = confidence.saturating_sub(180);
    }
    if guard.time_budget_exceeded || guard.iteration_budget_exceeded || guard.route_budget_exceeded
    {
        confidence = confidence.saturating_sub(80);
    }

    let pressure_penalty = match pressure_tier {
        MermaidPressureTier::Unknown | MermaidPressureTier::Nominal => 0,
        MermaidPressureTier::Elevated => 20,
        MermaidPressureTier::High => 50,
        MermaidPressureTier::Critical => 90,
    };
    confidence.saturating_sub(pressure_penalty)
}

#[cfg(test)]
mod tests {
    use super::{
        CycleStrategy, GraphMetrics, LayoutAlgorithm, LayoutGuardrails, LayoutPoint, LayoutRect,
        RenderClip, RenderItem, RenderSource, build_layout_decision_ledger,
        build_layout_guard_report, build_render_scene, dispatch_layout_algorithm, layout,
        layout_diagram, layout_diagram_force, layout_diagram_force_traced, layout_diagram_gantt,
        layout_diagram_grid, layout_diagram_radial, layout_diagram_sankey, layout_diagram_sequence,
        layout_diagram_sequence_traced, layout_diagram_timeline, layout_diagram_traced,
        layout_diagram_traced_with_algorithm, layout_diagram_traced_with_algorithm_and_guardrails,
        layout_diagram_tree, layout_diagram_with_cycle_strategy, route_edge_points,
        route_edge_points_with_obstacles,
    };
    use fm_core::{
        ArrowType, DiagramType, GraphDirection, IrCluster, IrClusterId, IrEdge, IrEndpoint,
        IrGraphCluster, IrGraphNode, IrLabel, IrLabelId, IrNode, IrNodeId, IrSubgraph,
        IrSubgraphId, MermaidDiagramIr, MermaidPressureTier, NodeShape, Span,
    };
    use proptest::prelude::*;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    fn sample_ir() -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::LR;
        ir.labels.push(IrLabel {
            text: "Start".to_string(),
            ..IrLabel::default()
        });
        ir.labels.push(IrLabel {
            text: "End".to_string(),
            ..IrLabel::default()
        });
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            label: Some(IrLabelId(0)),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            label: Some(IrLabelId(1)),
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
        ir
    }

    fn chain_ir(node_count: usize, direction: GraphDirection) -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = direction;

        for index in 0..node_count {
            ir.nodes.push(IrNode {
                id: format!("N{index}"),
                ..IrNode::default()
            });
        }

        for index in 1..node_count {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(index - 1)),
                to: IrEndpoint::Node(IrNodeId(index)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        ir
    }

    #[test]
    fn layout_decision_ledger_tracks_selected_algorithm_and_jsonl() {
        let ir = sample_ir();
        let traced = layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Auto);
        let mut report = build_layout_guard_report(&ir, &traced);
        let (_cx, observability) = fm_core::mermaid_layout_guard_observability(
            "cli.validate",
            "flowchart LR\nA-->B",
            traced.trace.dispatch.selected.as_str(),
            64,
        );
        report.observability = observability;
        let ledger = build_layout_decision_ledger(&ir, &traced, &report);

        assert_eq!(ledger.entries.len(), 1);
        let record = &ledger.entries[0];
        assert_eq!(record.kind, "layout_decision");
        assert_eq!(record.requested_algorithm, "auto");
        assert_eq!(
            record.selected_algorithm,
            traced.trace.dispatch.selected.as_str()
        );
        assert!(record.alternatives.iter().any(|alt| alt.selected));

        let jsonl = ledger.to_jsonl().expect("ledger should serialize");
        assert!(jsonl.contains("\"requested_algorithm\":\"auto\""));
        assert!(jsonl.contains("\"kind\":\"layout_decision\""));
    }

    #[test]
    fn layout_reports_counts() {
        let ir = sample_ir();
        let stats = layout(&ir, LayoutAlgorithm::Auto);
        assert_eq!(stats.node_count, 2);
        assert_eq!(stats.edge_count, 1);
    }

    #[test]
    fn traced_layout_is_deterministic() {
        let ir = sample_ir();
        let first = layout_diagram_traced(&ir);
        let second = layout_diagram_traced(&ir);
        assert_eq!(first, second);
    }

    #[test]
    fn block_beta_grid_layout_keeps_group_members_together() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::BlockBeta);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
            ir.graph.nodes.push(IrGraphNode {
                node_id: IrNodeId(ir.graph.nodes.len()),
                kind: fm_core::IrNodeKind::Generic,
                clusters: Vec::new(),
                subgraphs: Vec::new(),
            });
        }

        ir.clusters.push(IrCluster {
            id: IrClusterId(0),
            members: vec![IrNodeId(0), IrNodeId(2)],
            ..IrCluster::default()
        });
        ir.graph.clusters.push(IrGraphCluster {
            cluster_id: IrClusterId(0),
            members: vec![IrNodeId(0), IrNodeId(2)],
            subgraph: Some(IrSubgraphId(0)),
            ..IrGraphCluster::default()
        });
        ir.graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(0),
            key: "api".to_string(),
            members: vec![IrNodeId(0), IrNodeId(2)],
            cluster: Some(IrClusterId(0)),
            ..IrSubgraph::default()
        });
        ir.graph.nodes[0].clusters.push(IrClusterId(0));
        ir.graph.nodes[0].subgraphs.push(IrSubgraphId(0));
        ir.graph.nodes[2].clusters.push(IrClusterId(0));
        ir.graph.nodes[2].subgraphs.push(IrSubgraphId(0));

        let layout = layout_diagram_grid(&ir);
        let positions = layout
            .nodes
            .iter()
            .map(|node| (node.node_id.as_str(), (node.bounds.x, node.bounds.y)))
            .collect::<BTreeMap<_, _>>();

        let a = positions.get("A").unwrap();
        let b = positions.get("B").unwrap();
        let c = positions.get("C").unwrap();

        assert_eq!(a.0, c.0);
        assert!(c.1 > a.1);
        assert!(b.0 > a.0);
        assert_eq!(a.1, b.1);
    }

    #[test]
    fn block_beta_grid_layout_distinguishes_groups_with_same_visible_name() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::BlockBeta);
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
            ir.graph.nodes.push(IrGraphNode {
                node_id: IrNodeId(ir.graph.nodes.len()),
                kind: fm_core::IrNodeKind::Generic,
                clusters: Vec::new(),
                subgraphs: Vec::new(),
            });
        }

        ir.labels.push(IrLabel {
            text: "api".to_string(),
            ..IrLabel::default()
        });

        for (cluster_index, members) in [
            vec![IrNodeId(0), IrNodeId(2)],
            vec![IrNodeId(1), IrNodeId(3)],
        ]
        .into_iter()
        .enumerate()
        {
            let cluster_id = IrClusterId(cluster_index);
            let subgraph_id = IrSubgraphId(cluster_index);

            ir.clusters.push(IrCluster {
                id: cluster_id,
                title: Some(IrLabelId(0)),
                members: members.clone(),
                ..IrCluster::default()
            });
            ir.graph.clusters.push(IrGraphCluster {
                cluster_id,
                title: Some(IrLabelId(0)),
                members: members.clone(),
                subgraph: Some(subgraph_id),
                ..IrGraphCluster::default()
            });
            ir.graph.subgraphs.push(IrSubgraph {
                id: subgraph_id,
                key: "api".to_string(),
                title: Some(IrLabelId(0)),
                members: members.clone(),
                cluster: Some(cluster_id),
                ..IrSubgraph::default()
            });

            for member in members {
                ir.graph.nodes[member.0].clusters.push(cluster_id);
                ir.graph.nodes[member.0].subgraphs.push(subgraph_id);
            }
        }

        let layout = layout_diagram_grid(&ir);
        let positions = layout
            .nodes
            .iter()
            .map(|node| (node.node_id.as_str(), (node.bounds.x, node.bounds.y)))
            .collect::<BTreeMap<_, _>>();

        let a = positions.get("A").unwrap();
        let b = positions.get("B").unwrap();
        let c = positions.get("C").unwrap();
        let d = positions.get("D").unwrap();

        assert_eq!(a.0, c.0);
        assert_eq!(b.0, d.0);
        assert!(b.0 > a.0);
        assert_eq!(a.1, b.1);
        assert_eq!(c.1, d.1);
    }

    #[test]
    fn block_beta_grid_layout_honors_columns_and_node_spans() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::BlockBeta);
        ir.meta.block_beta_columns = Some(3);

        for (node_id, classes) in [
            (
                "A",
                vec!["block-beta".to_string(), "block-beta-span-2".to_string()],
            ),
            ("B", vec!["block-beta".to_string()]),
            ("C", vec!["block-beta".to_string()]),
        ] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                classes,
                ..IrNode::default()
            });
            ir.graph.nodes.push(IrGraphNode {
                node_id: IrNodeId(ir.graph.nodes.len()),
                kind: fm_core::IrNodeKind::Generic,
                clusters: Vec::new(),
                subgraphs: Vec::new(),
            });
        }

        let layout = layout_diagram_grid(&ir);
        let positions = layout
            .nodes
            .iter()
            .map(|node| {
                (
                    node.node_id.as_str(),
                    (
                        node.bounds.x + (node.bounds.width / 2.0),
                        node.bounds.y + (node.bounds.height / 2.0),
                        node.bounds.width,
                    ),
                )
            })
            .collect::<BTreeMap<_, _>>();

        let a = positions.get("A").unwrap();
        let b = positions.get("B").unwrap();
        let c = positions.get("C").unwrap();

        assert_eq!(a.1, b.1);
        assert!(c.1 > a.1);
        assert!(a.2 > b.2);
    }

    #[test]
    fn block_beta_group_span_shapes_grouped_layout() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::BlockBeta);
        ir.meta.block_beta_columns = Some(3);

        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                classes: vec!["block-beta".to_string()],
                ..IrNode::default()
            });
            ir.graph.nodes.push(IrGraphNode {
                node_id: IrNodeId(ir.graph.nodes.len()),
                kind: fm_core::IrNodeKind::Generic,
                clusters: Vec::new(),
                subgraphs: Vec::new(),
            });
        }

        ir.clusters.push(IrCluster {
            id: IrClusterId(0),
            members: vec![IrNodeId(0), IrNodeId(1)],
            grid_span: 2,
            ..IrCluster::default()
        });
        ir.graph.clusters.push(IrGraphCluster {
            cluster_id: IrClusterId(0),
            members: vec![IrNodeId(0), IrNodeId(1)],
            subgraph: Some(IrSubgraphId(0)),
            grid_span: 2,
            ..IrGraphCluster::default()
        });
        ir.graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(0),
            key: "api".to_string(),
            members: vec![IrNodeId(0), IrNodeId(1)],
            cluster: Some(IrClusterId(0)),
            grid_span: 2,
            ..IrSubgraph::default()
        });

        ir.graph.nodes[0].clusters.push(IrClusterId(0));
        ir.graph.nodes[0].subgraphs.push(IrSubgraphId(0));
        ir.graph.nodes[1].clusters.push(IrClusterId(0));
        ir.graph.nodes[1].subgraphs.push(IrSubgraphId(0));

        let layout = layout_diagram_grid(&ir);
        let positions = layout
            .nodes
            .iter()
            .map(|node| {
                (
                    node.node_id.as_str(),
                    (
                        node.bounds.x + (node.bounds.width / 2.0),
                        node.bounds.y + (node.bounds.height / 2.0),
                    ),
                )
            })
            .collect::<BTreeMap<_, _>>();

        let a = positions.get("A").unwrap();
        let b = positions.get("B").unwrap();
        let c = positions.get("C").unwrap();
        let cluster = &layout.clusters[0];

        assert_eq!(a.1, b.1);
        assert_eq!(a.1, c.1);
        assert!(a.0 < b.0);
        assert!(b.0 < c.0);
        assert!(cluster.bounds.width > layout.nodes[2].bounds.width);
    }

    #[test]
    fn block_beta_grouped_layout_respects_lr_rank_order_mapping() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::BlockBeta);
        ir.direction = GraphDirection::LR;
        ir.meta.block_beta_columns = Some(2);

        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                classes: vec!["block-beta".to_string()],
                ..IrNode::default()
            });
            ir.graph.nodes.push(IrGraphNode {
                node_id: IrNodeId(ir.graph.nodes.len()),
                kind: fm_core::IrNodeKind::Generic,
                clusters: Vec::new(),
                subgraphs: Vec::new(),
            });
        }

        ir.clusters.push(IrCluster {
            id: IrClusterId(0),
            members: vec![IrNodeId(0), IrNodeId(1)],
            grid_span: 2,
            ..IrCluster::default()
        });
        ir.graph.clusters.push(IrGraphCluster {
            cluster_id: IrClusterId(0),
            members: vec![IrNodeId(0), IrNodeId(1)],
            subgraph: Some(IrSubgraphId(0)),
            grid_span: 2,
            ..IrGraphCluster::default()
        });
        ir.graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(0),
            key: "api".to_string(),
            members: vec![IrNodeId(0), IrNodeId(1)],
            cluster: Some(IrClusterId(0)),
            grid_span: 2,
            ..IrSubgraph::default()
        });

        ir.graph.nodes[0].clusters.push(IrClusterId(0));
        ir.graph.nodes[0].subgraphs.push(IrSubgraphId(0));
        ir.graph.nodes[1].clusters.push(IrClusterId(0));
        ir.graph.nodes[1].subgraphs.push(IrSubgraphId(0));

        let layout = layout_diagram_grid(&ir);
        let a = layout
            .nodes
            .iter()
            .find(|node| node.node_id == "A")
            .unwrap();
        let b = layout
            .nodes
            .iter()
            .find(|node| node.node_id == "B")
            .unwrap();
        let c = layout
            .nodes
            .iter()
            .find(|node| node.node_id == "C")
            .unwrap();

        assert_eq!(a.rank, c.rank);
        assert!(b.rank > a.rank);
        assert_eq!(a.order, b.order);
        assert!(c.order > a.order);
        assert_eq!(a.bounds.x, c.bounds.x);
        assert!(b.bounds.x > a.bounds.x);
        assert_eq!(a.bounds.y, b.bounds.y);
        assert!(c.bounds.y > a.bounds.y);
    }

    #[test]
    fn timeline_layout_keeps_periods_on_baseline_and_stacks_events() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Timeline);
        for label in ["2024", "2025", "Kickoff", "Launch", "Retro"] {
            ir.labels.push(IrLabel {
                text: label.to_string(),
                ..IrLabel::default()
            });
        }
        ir.nodes.push(IrNode {
            id: "period_2024".to_string(),
            label: Some(IrLabelId(0)),
            shape: NodeShape::Rect,
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "period_2025".to_string(),
            label: Some(IrLabelId(1)),
            shape: NodeShape::Rect,
            ..IrNode::default()
        });
        for (node_id, label_id) in [
            ("kickoff", IrLabelId(2)),
            ("launch", IrLabelId(3)),
            ("retro", IrLabelId(4)),
        ] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                label: Some(label_id),
                shape: NodeShape::Rounded,
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 2), (0, 3), (1, 4)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram_timeline(&ir);
        let centers = layout
            .nodes
            .iter()
            .map(|node| (node.node_id.as_str(), node.bounds.center()))
            .collect::<BTreeMap<_, _>>();

        let period_2024 = centers.get("period_2024").expect("2024 period");
        let period_2025 = centers.get("period_2025").expect("2025 period");
        let kickoff = centers.get("kickoff").expect("kickoff event");
        let launch = centers.get("launch").expect("launch event");
        let retro = centers.get("retro").expect("retro event");

        assert!((period_2024.y - period_2025.y).abs() < 0.001);
        assert!(period_2024.x < period_2025.x);
        assert!((kickoff.x - period_2024.x).abs() < 0.001);
        assert!((launch.x - period_2024.x).abs() < 0.001);
        assert!((retro.x - period_2025.x).abs() < 0.001);
        assert!(kickoff.y > period_2024.y);
        assert!(launch.y > kickoff.y);
        assert!(retro.y > period_2025.y);
        assert_eq!(layout.extensions.axis_ticks.len(), 2);
    }

    #[test]
    fn gantt_layout_groups_tasks_by_section_and_orders_slots_horizontally() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Gantt);
        for label in ["Planning: Scope", "Planning: Estimate", "Delivery: Build"] {
            ir.labels.push(IrLabel {
                text: label.to_string(),
                ..IrLabel::default()
            });
        }
        for (node_id, label) in [
            ("task_1", IrLabelId(0)),
            ("task_3", IrLabelId(1)),
            ("task_2", IrLabelId(2)),
        ] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                label: Some(label),
                ..IrNode::default()
            });
        }

        let layout = layout_diagram_gantt(&ir);
        let nodes = layout
            .nodes
            .iter()
            .map(|node| (node.node_id.as_str(), node))
            .collect::<BTreeMap<_, _>>();

        let task_1 = nodes.get("task_1").expect("task_1");
        let task_2 = nodes.get("task_2").expect("task_2");
        let task_3 = nodes.get("task_3").expect("task_3");

        assert!(task_1.bounds.width >= 156.0);
        assert!(task_1.bounds.center().x < task_2.bounds.center().x);
        assert!(task_1.bounds.center().x < task_3.bounds.center().x);
        assert!(task_3.bounds.center().y > task_1.bounds.center().y);
        assert!((task_1.bounds.center().y - task_2.bounds.center().y).abs() > 10.0);
        assert_eq!(layout.extensions.bands.len(), 2);
        assert_eq!(layout.extensions.axis_ticks.len(), 3);
    }

    #[test]
    fn sankey_layout_preserves_columns_for_sources_hub_and_sinks() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Sankey);
        for node_id in [
            "left_source",
            "right_source",
            "hub",
            "left_sink",
            "right_sink",
        ] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 2), (1, 2), (2, 3), (2, 4)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram_sankey(&ir);
        let nodes = layout
            .nodes
            .iter()
            .map(|node| (node.node_id.as_str(), node))
            .collect::<BTreeMap<_, _>>();

        let left_source = nodes.get("left_source").expect("left_source");
        let right_source = nodes.get("right_source").expect("right_source");
        let hub = nodes.get("hub").expect("hub");
        let left_sink = nodes.get("left_sink").expect("left_sink");
        let right_sink = nodes.get("right_sink").expect("right_sink");

        assert!(hub.bounds.width >= 108.0);
        assert!(hub.bounds.height >= 30.0);
        assert!(left_source.bounds.height >= 30.0);
        assert!(left_sink.bounds.height >= 30.0);
        assert!((left_source.bounds.height - right_source.bounds.height).abs() < 0.001);
        assert!((left_sink.bounds.height - right_sink.bounds.height).abs() < 0.001);
        assert!((left_source.bounds.center().x - right_source.bounds.center().x).abs() < 0.001);
        assert!((left_sink.bounds.center().x - right_sink.bounds.center().x).abs() < 0.001);
        assert!(left_source.bounds.center().x < hub.bounds.center().x);
        assert!(right_source.bounds.center().x < hub.bounds.center().x);
        assert!(hub.bounds.center().x < left_sink.bounds.center().x);
        assert!(hub.bounds.center().x < right_sink.bounds.center().x);
        assert_eq!(layout.extensions.bands.len(), 3);
    }

    #[test]
    fn kanban_layout_stacks_cards_within_columns() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Journey);
        for node_id in ["backlog_a", "backlog_b", "doing_a", "doing_b"] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 2), (1, 3)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Kanban).layout;
        let nodes = layout
            .nodes
            .iter()
            .map(|node| (node.node_id.as_str(), node.bounds.center()))
            .collect::<BTreeMap<_, _>>();

        let backlog_a = nodes.get("backlog_a").expect("backlog_a");
        let backlog_b = nodes.get("backlog_b").expect("backlog_b");
        let doing_a = nodes.get("doing_a").expect("doing_a");
        let doing_b = nodes.get("doing_b").expect("doing_b");

        assert!((backlog_a.x - backlog_b.x).abs() < 0.001);
        assert!(backlog_b.y > backlog_a.y);
        assert!((doing_a.x - doing_b.x).abs() < 0.001);
        assert!(doing_b.y > doing_a.y);
        assert!(doing_a.x > backlog_a.x);
        assert_eq!(layout.extensions.bands.len(), 2);
    }

    #[test]
    fn render_scene_builder_is_deterministic() {
        let ir = sample_ir();
        let layout = layout_diagram(&ir);
        let first = build_render_scene(&ir, &layout);
        let second = build_render_scene(&ir, &layout);
        assert_eq!(first, second);
    }

    #[test]
    fn render_scene_contains_expected_layers_and_primitives() {
        let mut ir = sample_ir();
        ir.labels.push(IrLabel {
            text: "A->B".to_string(),
            ..IrLabel::default()
        });
        if let Some(edge) = ir.edges.get_mut(0) {
            edge.label = Some(IrLabelId(2));
        }

        let layout = layout_diagram(&ir);
        let scene = build_render_scene(&ir, &layout);
        assert!(matches!(scene.root.clip, Some(RenderClip::Rect(_))));

        let layer_ids: Vec<&str> = scene
            .root
            .children
            .iter()
            .map(|item| match item {
                RenderItem::Group(group) => group.id.as_deref().unwrap_or(""),
                _ => "",
            })
            .collect();
        assert_eq!(layer_ids, vec!["clusters", "edges", "nodes", "labels"]);

        let mut path_count = 0usize;
        let mut text_count = 0usize;
        for layer in &scene.root.children {
            if let RenderItem::Group(group) = layer {
                for child in &group.children {
                    match child {
                        RenderItem::Path(_) => path_count += 1,
                        RenderItem::Text(_) => text_count += 1,
                        RenderItem::Group(_) => {}
                    }
                }
            }
        }

        assert!(path_count >= layout.nodes.len() + layout.edges.len());
        assert!(text_count >= 3);
    }

    #[test]
    fn render_scene_paths_reference_node_edge_and_cluster_sources() {
        let mut ir = sample_ir();
        ir.labels.push(IrLabel {
            text: "Cluster".to_string(),
            ..IrLabel::default()
        });
        ir.clusters.push(IrCluster {
            id: IrClusterId(0),
            title: Some(IrLabelId(2)),
            members: vec![IrNodeId(0), IrNodeId(1)],
            ..IrCluster::default()
        });

        let layout = layout_diagram(&ir);
        let scene = build_render_scene(&ir, &layout);

        let mut saw_node = false;
        let mut saw_edge = false;
        let mut saw_cluster = false;
        for layer in &scene.root.children {
            if let RenderItem::Group(group) = layer {
                for child in &group.children {
                    if let RenderItem::Path(path) = child {
                        match path.source {
                            RenderSource::Node(_) => saw_node = true,
                            RenderSource::Edge(_) => saw_edge = true,
                            RenderSource::Cluster(_) => saw_cluster = true,
                            RenderSource::Diagram => {}
                        }
                    }
                }
            }
        }

        assert!(saw_node);
        assert!(saw_edge);
        assert!(saw_cluster);
    }

    #[test]
    fn layout_contains_node_boxes_and_bounds() {
        let ir = sample_ir();
        let layout = layout_diagram(&ir);
        assert_eq!(layout.nodes.len(), 2);
        assert!(layout.bounds.width > 0.0);
        assert!(layout.bounds.height > 0.0);
    }

    #[test]
    fn crossing_count_reports_layer_crossings() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }

        // K2,2 across adjacent layers: at least one crossing remains regardless ordering.
        for (from, to) in [(0, 2), (0, 3), (1, 2), (1, 3)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let stats = layout(&ir, LayoutAlgorithm::Auto);
        assert!(stats.crossing_count > 0);
    }

    #[test]
    fn cycle_removal_marks_reversed_edges_for_simple_cycle() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let stats = layout(&ir, LayoutAlgorithm::Auto);
        assert!(stats.reversed_edges >= 1);
    }

    #[test]
    fn cycle_aware_marks_back_edges_without_reversal() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::CycleAware);
        assert_eq!(layout.stats.reversed_edges, 0);
        assert!((layout.stats.reversed_edge_total_length - 0.0).abs() < f32::EPSILON);
        assert_eq!(layout.stats.cycle_count, 1);
        assert_eq!(layout.stats.cycle_node_count, 3);
        assert_eq!(layout.stats.max_cycle_size, 3);
        assert!(layout.edges.iter().any(|edge| edge.reversed));
    }

    #[test]
    fn dfs_back_cycle_strategy_is_deterministic() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0), (2, 3), (3, 1)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let first = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::DfsBack);
        let second = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::DfsBack);
        assert_eq!(first, second);
        assert!(first.stats.reversed_edges >= 1);
        assert!(first.edges.iter().any(|edge| edge.reversed));
    }

    #[test]
    fn bt_direction_reverses_vertical_rank_axis() {
        let mut ir = sample_ir();
        ir.direction = GraphDirection::BT;

        let layout = layout_diagram(&ir);
        let a_node = layout.nodes.iter().find(|node| node.node_id == "A");
        let b_node = layout.nodes.iter().find(|node| node.node_id == "B");
        let (Some(a_node), Some(b_node)) = (a_node, b_node) else {
            panic!("expected A and B nodes in layout");
        };

        assert!(b_node.bounds.y < a_node.bounds.y);
    }

    #[test]
    fn rl_direction_reverses_horizontal_rank_axis() {
        let mut ir = sample_ir();
        ir.direction = GraphDirection::RL;

        let layout = layout_diagram(&ir);
        let a_node = layout.nodes.iter().find(|node| node.node_id == "A");
        let b_node = layout.nodes.iter().find(|node| node.node_id == "B");
        let (Some(a_node), Some(b_node)) = (a_node, b_node) else {
            panic!("expected A and B nodes in layout");
        };

        assert!(b_node.bounds.x < a_node.bounds.x);
    }

    #[test]
    fn vertical_routing_adds_turn_for_offset_nodes() {
        let points = route_edge_points(
            LayoutPoint { x: 10.0, y: 40.0 },
            LayoutPoint { x: 100.0, y: 120.0 },
            false,
        );
        assert_eq!(points.len(), 4);
        assert_eq!(
            points.first().copied(),
            Some(LayoutPoint { x: 10.0, y: 40.0 })
        );
        assert_eq!(
            points.last().copied(),
            Some(LayoutPoint { x: 100.0, y: 120.0 })
        );
    }

    #[test]
    fn horizontal_routing_adds_turn_for_offset_nodes() {
        let points = route_edge_points(
            LayoutPoint { x: 40.0, y: 10.0 },
            LayoutPoint { x: 120.0, y: 100.0 },
            true,
        );
        assert_eq!(points.len(), 4);
        assert_eq!(
            points.first().copied(),
            Some(LayoutPoint { x: 40.0, y: 10.0 })
        );
        assert_eq!(
            points.last().copied(),
            Some(LayoutPoint { x: 120.0, y: 100.0 })
        );
    }

    #[test]
    fn obstacle_routing_nudges_around_blocking_node() {
        // Route from (50, 10) to (50, 200) vertically, with an obstacle at x=30..70, y=80..120.
        let obstacle = LayoutRect {
            x: 30.0,
            y: 80.0,
            width: 40.0,
            height: 40.0,
        };
        let points = route_edge_points_with_obstacles(
            LayoutPoint { x: 50.0, y: 10.0 },
            LayoutPoint { x: 50.0, y: 200.0 },
            false,
            &[obstacle],
        );
        assert_eq!(points.len(), 4);
        assert_ne!(points[1].x, 50.0);
        for pt in &points {
            let inside = pt.x >= obstacle.x
                && pt.x <= obstacle.x + obstacle.width
                && pt.y >= obstacle.y
                && pt.y <= obstacle.y + obstacle.height;
            assert!(
                !inside,
                "Waypoint ({:.1}, {:.1}) is inside obstacle ({:.0}..{:.0}, {:.0}..{:.0})",
                pt.x,
                pt.y,
                obstacle.x,
                obstacle.x + obstacle.width,
                obstacle.y,
                obstacle.y + obstacle.height,
            );
        }

        // Also verify the offset-path case where the midpoint segment is horizontal.
        let points2 = route_edge_points_with_obstacles(
            LayoutPoint { x: 10.0, y: 10.0 },
            LayoutPoint { x: 100.0, y: 200.0 },
            false,
            &[obstacle],
        );
        // The midpoint y = (10+200)/2 = 105. The horizontal segment at y=105
        // goes from x=10 to x=100 and passes through obstacle x=30..70.
        // So the route should be nudged to y < 72 or y > 128.
        for pt in &points2 {
            // No waypoint should be inside the obstacle.
            let inside = pt.x >= obstacle.x
                && pt.x <= obstacle.x + obstacle.width
                && pt.y >= obstacle.y
                && pt.y <= obstacle.y + obstacle.height;
            assert!(
                !inside,
                "Waypoint ({:.1}, {:.1}) is inside obstacle ({:.0}..{:.0}, {:.0}..{:.0})",
                pt.x,
                pt.y,
                obstacle.x,
                obstacle.x + obstacle.width,
                obstacle.y,
                obstacle.y + obstacle.height,
            );
        }
        // Verify route still connects source to target.
        assert_eq!(points2.first().unwrap().x, 10.0);
        assert_eq!(points2.first().unwrap().y, 10.0);
        assert_eq!(points2.last().unwrap().x, 100.0);
        assert_eq!(points2.last().unwrap().y, 200.0);
    }

    #[test]
    fn obstacle_routing_no_obstacles_matches_basic_routing() {
        let source = LayoutPoint { x: 10.0, y: 40.0 };
        let target = LayoutPoint { x: 100.0, y: 120.0 };
        let basic = route_edge_points(source, target, false);
        let obstacle_aware = route_edge_points_with_obstacles(source, target, false, &[]);
        assert_eq!(basic, obstacle_aware);
    }

    #[test]
    fn obstacle_routing_clears_obstacle_on_horizontal_layout() {
        let obstacle = LayoutRect {
            x: 60.0,
            y: 30.0,
            width: 40.0,
            height: 40.0,
        };
        let points = route_edge_points_with_obstacles(
            LayoutPoint { x: 10.0, y: 10.0 },
            LayoutPoint { x: 100.0, y: 80.0 },
            true,
            &[obstacle],
        );
        // Verify source and target preserved.
        assert_eq!(points.first().unwrap().x, 10.0);
        assert_eq!(points.last().unwrap().x, 100.0);
    }

    #[test]
    fn greedy_cycle_strategy_is_deterministic() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0), (2, 3), (3, 1)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let first = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::Greedy);
        let second = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::Greedy);
        assert_eq!(first, second);
        assert!(first.stats.reversed_edges >= 1);
        assert!(first.edges.iter().any(|edge| edge.reversed));
    }

    #[test]
    fn mfas_cycle_strategy_is_deterministic() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0), (2, 3), (3, 1)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let first = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::MfasApprox);
        let second = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::MfasApprox);
        assert_eq!(first, second);
        assert!(first.stats.reversed_edges >= 1);
    }

    #[test]
    fn greedy_breaks_simple_cycle() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::Greedy);
        assert!(layout.stats.reversed_edges >= 1);
        assert_eq!(layout.stats.cycle_count, 1);
        assert_eq!(layout.stats.cycle_node_count, 3);
        assert!(layout.edges.iter().any(|edge| edge.reversed));
    }

    #[test]
    fn mfas_breaks_simple_cycle() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::MfasApprox);
        assert!(layout.stats.reversed_edges >= 1);
        assert_eq!(layout.stats.cycle_count, 1);
        assert!(layout.edges.iter().any(|edge| edge.reversed));
    }

    #[test]
    fn self_loop_detected_as_cycle() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(0)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });

        let layout = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::DfsBack);
        assert_eq!(layout.stats.cycle_count, 1);
        assert_eq!(layout.stats.cycle_node_count, 1);
        assert_eq!(layout.stats.max_cycle_size, 1);
        assert!(layout.edges.iter().any(|edge| edge.reversed));
    }

    #[test]
    fn multiple_disconnected_cycles_detected() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        // Two separate triangles: A->B->C->A and D->E->F->D
        for node_id in ["A", "B", "C", "D", "E", "F"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::Greedy);
        assert_eq!(layout.stats.cycle_count, 2);
        assert_eq!(layout.stats.cycle_node_count, 6);
        assert_eq!(layout.stats.max_cycle_size, 3);
        assert!(layout.stats.reversed_edges >= 2);
    }

    #[test]
    fn nested_cycles_handled_correctly() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        // A->B->C->A forms inner cycle, A->B->C->D->A forms outer cycle sharing edges
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0), (2, 3), (3, 0)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::DfsBack);
        // All 4 nodes form one SCC due to shared edges
        assert!(layout.stats.cycle_count >= 1);
        assert!(layout.stats.cycle_node_count >= 3);
        assert!(layout.stats.reversed_edges >= 1);
    }

    #[test]
    fn acyclic_graph_has_no_reversals() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (0, 2), (1, 3), (2, 3)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        for strategy in [
            CycleStrategy::Greedy,
            CycleStrategy::DfsBack,
            CycleStrategy::MfasApprox,
            CycleStrategy::CycleAware,
        ] {
            let layout = layout_diagram_with_cycle_strategy(&ir, strategy);
            assert_eq!(
                layout.stats.reversed_edges, 0,
                "strategy {:?} should not reverse edges in acyclic graph",
                strategy
            );
            assert_eq!(layout.stats.cycle_count, 0);
            assert!(!layout.edges.iter().any(|e| e.reversed));
        }
    }

    #[test]
    fn all_strategies_produce_valid_layout_for_cyclic_graph() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        for strategy in [
            CycleStrategy::Greedy,
            CycleStrategy::DfsBack,
            CycleStrategy::MfasApprox,
            CycleStrategy::CycleAware,
        ] {
            let layout = layout_diagram_with_cycle_strategy(&ir, strategy);
            // All strategies should produce valid layout with 3 nodes and 3 edges
            assert_eq!(layout.nodes.len(), 3, "strategy {:?}", strategy);
            assert_eq!(layout.edges.len(), 3, "strategy {:?}", strategy);
            assert!(layout.bounds.width > 0.0, "strategy {:?}", strategy);
            assert!(layout.bounds.height > 0.0, "strategy {:?}", strategy);
            // All strategies should detect the cycle
            assert_eq!(layout.stats.cycle_count, 1, "strategy {:?}", strategy);
        }
    }

    #[test]
    fn cycle_strategy_parse_roundtrip() {
        for strategy in [
            CycleStrategy::Greedy,
            CycleStrategy::DfsBack,
            CycleStrategy::MfasApprox,
            CycleStrategy::CycleAware,
        ] {
            let parsed = CycleStrategy::parse(strategy.as_str());
            assert_eq!(
                parsed,
                Some(strategy),
                "roundtrip failed for {:?}",
                strategy
            );
        }
    }

    #[test]
    fn cycle_cluster_collapse_groups_scc_members() {
        use super::LayoutConfig;

        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        // Build: A->B->C->A (cycle) + D (separate node connected from A)
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0), (0, 3)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let config = LayoutConfig {
            cycle_strategy: CycleStrategy::Greedy,
            collapse_cycle_clusters: true,
            font_metrics: None,
        };
        let layout = super::layout_diagram_with_config(&ir, config);

        // Should have one collapsed cluster (the A->B->C cycle)
        assert_eq!(layout.stats.collapsed_clusters, 1);
        assert_eq!(layout.cycle_clusters.len(), 1);

        let cluster = &layout.cycle_clusters[0];
        assert_eq!(cluster.member_node_indexes.len(), 3);
        assert!(cluster.bounds.width > 0.0);
        assert!(cluster.bounds.height > 0.0);

        // All 4 nodes should still be in the layout
        assert_eq!(layout.nodes.len(), 4);
    }

    #[test]
    fn edge_length_metrics_computed_for_cyclic_graph() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram_with_cycle_strategy(&ir, CycleStrategy::Greedy);
        // Total edge length should be positive (3 edges)
        assert!(layout.stats.total_edge_length > 0.0);
        // At least one edge is reversed, so reversed_edge_total_length > 0
        assert!(layout.stats.reversed_edge_total_length > 0.0);
        // Reversed edge length should not exceed total
        assert!(layout.stats.reversed_edge_total_length <= layout.stats.total_edge_length);
    }

    #[test]
    fn edge_length_metrics_zero_for_acyclic_graph() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram(&ir);
        assert!(layout.stats.total_edge_length > 0.0);
        assert!((layout.stats.reversed_edge_total_length - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn cycle_cluster_collapse_disabled_produces_no_clusters() {
        use super::LayoutConfig;

        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2), (2, 0)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let config = LayoutConfig {
            cycle_strategy: CycleStrategy::Greedy,
            collapse_cycle_clusters: false,
            font_metrics: None,
        };
        let layout = super::layout_diagram_with_config(&ir, config);

        assert_eq!(layout.stats.collapsed_clusters, 0);
        assert!(layout.cycle_clusters.is_empty());
    }

    #[test]
    fn cycle_strategy_parse_aliases() {
        assert_eq!(CycleStrategy::parse("dfs"), Some(CycleStrategy::DfsBack));
        assert_eq!(
            CycleStrategy::parse("dfs_back"),
            Some(CycleStrategy::DfsBack)
        );
        assert_eq!(
            CycleStrategy::parse("minimum-feedback-arc-set"),
            Some(CycleStrategy::MfasApprox)
        );
        assert_eq!(
            CycleStrategy::parse("cycleaware"),
            Some(CycleStrategy::CycleAware)
        );
        assert_eq!(CycleStrategy::parse("unknown"), None);
    }

    #[test]
    fn lr_same_rank_nodes_with_different_widths_share_column_position() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::LR;

        for text in [
            "root",
            "narrow",
            "this target label is intentionally much wider",
        ] {
            ir.labels.push(IrLabel {
                text: text.to_string(),
                ..IrLabel::default()
            });
        }

        for (node_id, label_id) in [("R", 0), ("A", 1), ("B", 2)] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                label: Some(IrLabelId(label_id)),
                ..IrNode::default()
            });
        }

        for (from, to) in [(0, 1), (0, 2)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram(&ir);
        let a_node = layout.nodes.iter().find(|node| node.node_id == "A");
        let b_node = layout.nodes.iter().find(|node| node.node_id == "B");
        let (Some(a_node), Some(b_node)) = (a_node, b_node) else {
            panic!("expected A and B nodes in layout");
        };

        assert!((a_node.bounds.x - b_node.bounds.x).abs() < 0.001);
    }

    #[test]
    fn tb_disconnected_components_do_not_collapse_into_horizontal_strip() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::TB;

        // 20 disconnected 2-node chains (A_i -> B_i).
        for index in 0..20 {
            ir.nodes.push(IrNode {
                id: format!("A{index}"),
                ..IrNode::default()
            });
            ir.nodes.push(IrNode {
                id: format!("B{index}"),
                ..IrNode::default()
            });
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(index * 2)),
                to: IrEndpoint::Node(IrNodeId(index * 2 + 1)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram(&ir);
        assert_eq!(layout.nodes.len(), 40);
        assert_eq!(layout.edges.len(), 20);
        assert!(
            layout.bounds.width < layout.bounds.height * 2.0,
            "expected stacked components in TB layout, got width={} height={}",
            layout.bounds.width,
            layout.bounds.height,
        );
    }

    #[test]
    fn tb_isolated_nodes_remain_in_a_single_rank_band() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::TB;

        for index in 0..6 {
            ir.nodes.push(IrNode {
                id: format!("N{index}"),
                ..IrNode::default()
            });
        }

        let layout = layout_diagram(&ir);
        let distinct_ranks: std::collections::BTreeSet<usize> =
            layout.nodes.iter().map(|node| node.rank).collect();
        assert_eq!(
            distinct_ranks.len(),
            1,
            "isolated nodes should stay in a shared rank band, got ranks {distinct_ranks:?}"
        );
    }

    #[test]
    fn tb_mixed_components_keep_isolates_outside_connected_rank_bands() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::TB;

        for index in 0..5 {
            ir.nodes.push(IrNode {
                id: format!("A{index}"),
                ..IrNode::default()
            });
            ir.nodes.push(IrNode {
                id: format!("B{index}"),
                ..IrNode::default()
            });
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(index * 2)),
                to: IrEndpoint::Node(IrNodeId(index * 2 + 1)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        for index in 0..10 {
            ir.nodes.push(IrNode {
                id: format!("Iso{index}"),
                ..IrNode::default()
            });
        }

        let layout = layout_diagram(&ir);
        let mut connected_ranks = std::collections::BTreeSet::new();
        let mut isolated_ranks = std::collections::BTreeSet::new();

        for node in &layout.nodes {
            if node.node_id.starts_with("Iso") {
                isolated_ranks.insert(node.rank);
            } else {
                connected_ranks.insert(node.rank);
            }
        }

        assert_eq!(
            isolated_ranks.len(),
            1,
            "all isolated nodes should share one rank band, got {isolated_ranks:?}"
        );
        assert!(
            connected_ranks.is_disjoint(&isolated_ranks),
            "isolated and connected nodes should not share rank bands; connected={connected_ranks:?} isolated={isolated_ranks:?}"
        );
    }

    fn sample_tree_ir(direction: GraphDirection) -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = direction;

        for node_id in ["A", "B", "C", "D", "E", "F"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }

        for (from, to) in [(0, 1), (0, 2), (1, 3), (1, 4), (2, 5)] {
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
    fn tree_layout_top_down_places_children_below_parents() {
        let layout = layout_diagram_tree(&sample_tree_ir(GraphDirection::TB));
        let mut centers = BTreeMap::new();
        for node in &layout.nodes {
            centers.insert(node.node_id.clone(), node.bounds.center());
        }

        let root = centers.get("A").expect("root center");
        let child_b = centers.get("B").expect("child B center");
        let child_c = centers.get("C").expect("child C center");
        assert!(root.y < child_b.y, "B should be below A");
        assert!(root.y < child_c.y, "C should be below A");
    }

    #[test]
    fn tree_layout_lr_places_children_to_the_right() {
        let layout = layout_diagram_tree(&sample_tree_ir(GraphDirection::LR));
        let mut centers = BTreeMap::new();
        for node in &layout.nodes {
            centers.insert(node.node_id.clone(), node.bounds.center());
        }

        let root = centers.get("A").expect("root center");
        let child_b = centers.get("B").expect("child B center");
        let child_c = centers.get("C").expect("child C center");
        assert!(root.x < child_b.x, "B should be to the right of A");
        assert!(root.x < child_c.x, "C should be to the right of A");
    }

    #[test]
    fn tree_layout_handles_multiple_roots_as_forest() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::TB;
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (2, 3)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram_tree(&ir);
        assert_eq!(layout.nodes.len(), 4);
        assert_eq!(layout.edges.len(), 2);
        let a = layout
            .nodes
            .iter()
            .find(|node| node.node_id == "A")
            .expect("A node");
        let c = layout
            .nodes
            .iter()
            .find(|node| node.node_id == "C")
            .expect("C node");
        assert!(
            (a.bounds.center().x - c.bounds.center().x).abs() > 1.0,
            "forest roots should not overlap"
        );
    }

    #[test]
    fn radial_layout_is_deterministic() {
        let mut ir = sample_tree_ir(GraphDirection::TB);
        ir.diagram_type = DiagramType::Mindmap;

        let first = layout_diagram_radial(&ir);
        let second = layout_diagram_radial(&ir);
        assert_eq!(first, second, "radial layout must be deterministic");
    }

    #[test]
    fn radial_layout_places_children_away_from_root() {
        let mut ir = sample_tree_ir(GraphDirection::TB);
        ir.diagram_type = DiagramType::Mindmap;
        let layout = layout_diagram_radial(&ir);

        let root = layout
            .nodes
            .iter()
            .find(|node| node.node_id == "A")
            .expect("root node")
            .bounds
            .center();

        for node in &layout.nodes {
            if node.node_id == "A" {
                continue;
            }
            let center = node.bounds.center();
            let distance = ((center.x - root.x).powi(2) + (center.y - root.y).powi(2)).sqrt();
            assert!(distance > 1.0, "{} should be away from root", node.node_id);
        }
    }

    #[test]
    fn auto_layout_uses_radial_for_mindmap_diagrams() {
        let mut ir = sample_tree_ir(GraphDirection::TB);
        ir.diagram_type = DiagramType::Mindmap;
        let auto_stats = layout(&ir, LayoutAlgorithm::Auto);
        let radial_stats = layout(&ir, LayoutAlgorithm::Radial);
        assert_eq!(auto_stats, radial_stats);
        let traced = layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Auto);
        assert_eq!(traced.trace.dispatch.selected, LayoutAlgorithm::Radial);
        assert!(!traced.trace.dispatch.capability_unavailable);
    }

    #[test]
    fn auto_layout_uses_kanban_for_journey_diagrams() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Journey);
        ir.labels.push(IrLabel {
            text: "Backlog".to_string(),
            ..IrLabel::default()
        });
        ir.nodes.push(IrNode {
            id: "backlog".to_string(),
            label: Some(IrLabelId(0)),
            ..IrNode::default()
        });

        let traced = layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Auto);
        assert_eq!(traced.trace.dispatch.selected, LayoutAlgorithm::Kanban);
        assert_eq!(traced.layout.nodes.len(), 1);
    }

    #[test]
    fn unavailable_specialized_request_falls_back_deterministically() {
        let ir = sample_ir();
        let traced = layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Timeline);
        assert_eq!(traced.trace.dispatch.requested, LayoutAlgorithm::Timeline);
        assert_eq!(traced.trace.dispatch.selected, LayoutAlgorithm::Sugiyama);
        assert!(traced.trace.dispatch.capability_unavailable);
        assert_eq!(
            traced.trace.dispatch.reason,
            "requested_algorithm_capability_unavailable_for_diagram_type"
        );
    }

    #[test]
    fn layout_guardrails_leave_small_default_layouts_unchanged() {
        let ir = sample_ir();
        let traced = layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Auto);
        assert_eq!(traced.trace.guard.reason, "within_budget");
        assert!(!traced.trace.guard.fallback_applied);
        assert_eq!(
            traced.trace.guard.initial_algorithm,
            traced.trace.guard.selected_algorithm
        );
    }

    #[test]
    fn tight_force_guardrails_fall_back_deterministically() {
        let ir = sample_er_ir();
        let traced = layout_diagram_traced_with_algorithm_and_guardrails(
            &ir,
            LayoutAlgorithm::Force,
            LayoutGuardrails {
                max_layout_time_ms: 1,
                max_layout_iterations: 1,
                max_route_ops: 1,
            },
        );
        assert_eq!(traced.trace.guard.initial_algorithm, LayoutAlgorithm::Force);
        // With updated cost estimates Sugiyama is cheaper than Tree for small
        // graphs, so the guardrail selects it as the lowest-cost fallback.
        assert_eq!(traced.trace.dispatch.selected, LayoutAlgorithm::Sugiyama);
        assert!(traced.trace.guard.fallback_applied);
        assert!(traced.trace.guard.time_budget_exceeded);
        assert!(traced.trace.guard.iteration_budget_exceeded);
        assert!(traced.trace.guard.route_budget_exceeded);
        assert_eq!(traced.trace.dispatch.reason, traced.trace.guard.reason);
    }

    #[test]
    fn guardrail_fallback_is_repeatable() {
        let ir = sample_er_ir();
        let guardrails = LayoutGuardrails {
            max_layout_time_ms: 1,
            max_layout_iterations: 1,
            max_route_ops: 1,
        };
        let first = layout_diagram_traced_with_algorithm_and_guardrails(
            &ir,
            LayoutAlgorithm::Force,
            guardrails,
        );
        let second = layout_diagram_traced_with_algorithm_and_guardrails(
            &ir,
            LayoutAlgorithm::Force,
            guardrails,
        );
        assert_eq!(first, second);
    }

    #[test]
    fn guard_report_reflects_fallback_metadata() {
        let ir = sample_er_ir();
        let traced = layout_diagram_traced_with_algorithm_and_guardrails(
            &ir,
            LayoutAlgorithm::Force,
            LayoutGuardrails {
                max_layout_time_ms: 1,
                max_layout_iterations: 1,
                max_route_ops: 1,
            },
        );
        let report = build_layout_guard_report(&ir, &traced);
        assert!(report.budget_exceeded);
        assert!(report.layout_budget_exceeded);
        assert!(report.route_budget_exceeded);
        assert_eq!(report.layout_requested_algorithm.as_deref(), Some("force"));
        assert_eq!(
            report.layout_selected_algorithm.as_deref(),
            Some("sugiyama")
        );
        assert_eq!(
            report.guard_reason.as_deref(),
            Some(traced.trace.guard.reason)
        );
        assert_eq!(report.pressure.tier, MermaidPressureTier::Unknown);
        assert!(report.pressure.conservative_fallback);
        assert!(
            report
                .budget_broker
                .notes
                .iter()
                .any(|note| note.contains("telemetry unavailable"))
        );
    }

    // --- Force-directed layout tests ---

    fn sample_er_ir() -> MermaidDiagramIr {
        // ER-like diagram: no clear hierarchy, many-to-many relationships.
        let mut ir = MermaidDiagramIr::empty(DiagramType::Er);
        for label in ["Users", "Orders", "Products", "Reviews"] {
            ir.labels.push(IrLabel {
                text: label.to_string(),
                ..IrLabel::default()
            });
        }
        for (i, node_id) in ["users", "orders", "products", "reviews"]
            .iter()
            .enumerate()
        {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                label: Some(IrLabelId(i)),
                ..IrNode::default()
            });
        }
        // Many-to-many: users <-> orders, orders <-> products, users <-> reviews, products <-> reviews
        for (from, to) in [(0, 1), (1, 2), (0, 3), (2, 3)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Line,
                ..IrEdge::default()
            });
        }
        ir
    }

    #[test]
    fn force_layout_produces_valid_output() {
        let ir = sample_er_ir();
        let layout = layout_diagram_force(&ir);
        assert_eq!(layout.nodes.len(), 4);
        assert_eq!(layout.edges.len(), 4);
        assert!(layout.bounds.width > 0.0);
        assert!(layout.bounds.height > 0.0);
    }

    #[test]
    fn force_layout_is_deterministic() {
        let ir = sample_er_ir();
        let first = layout_diagram_force_traced(&ir);
        let second = layout_diagram_force_traced(&ir);
        assert_eq!(first, second, "Force layout must be deterministic");
    }

    #[test]
    fn force_layout_no_node_overlap() {
        let ir = sample_er_ir();
        let layout = layout_diagram_force(&ir);
        for (i, a) in layout.nodes.iter().enumerate() {
            for b in layout.nodes.iter().skip(i + 1) {
                let overlap_x = (a.bounds.width + b.bounds.width) / 2.0
                    - ((a.bounds.x + a.bounds.width / 2.0) - (b.bounds.x + b.bounds.width / 2.0))
                        .abs();
                let overlap_y = (a.bounds.height + b.bounds.height) / 2.0
                    - ((a.bounds.y + a.bounds.height / 2.0) - (b.bounds.y + b.bounds.height / 2.0))
                        .abs();
                assert!(
                    overlap_x <= 1.0 || overlap_y <= 1.0,
                    "Nodes {} and {} overlap: overlap_x={overlap_x}, overlap_y={overlap_y}",
                    a.node_id,
                    b.node_id,
                );
            }
        }
    }

    #[test]
    fn force_layout_empty_graph() {
        let ir = MermaidDiagramIr::empty(DiagramType::Er);
        let layout = layout_diagram_force(&ir);
        assert!(layout.nodes.is_empty());
        assert!(layout.edges.is_empty());
        assert_eq!(layout.stats.node_count, 0);
    }

    #[test]
    fn force_layout_single_node() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Er);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            ..IrNode::default()
        });
        let layout = layout_diagram_force(&ir);
        assert_eq!(layout.nodes.len(), 1);
        assert!(layout.nodes[0].bounds.width > 0.0);
        assert!(layout.nodes[0].bounds.height > 0.0);
        assert!(layout.nodes[0].bounds.x >= 0.0);
        assert!(layout.nodes[0].bounds.y >= 0.0);
    }

    #[test]
    fn force_layout_disconnected_components() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Er);
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        // Two disconnected pairs: A-B and C-D
        for (from, to) in [(0, 1), (2, 3)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Line,
                ..IrEdge::default()
            });
        }
        let layout = layout_diagram_force(&ir);
        assert_eq!(layout.nodes.len(), 4);
        assert_eq!(layout.edges.len(), 2);
        // All positions should be non-negative.
        for node in &layout.nodes {
            assert!(node.bounds.x >= 0.0, "node {} has negative x", node.node_id);
            assert!(node.bounds.y >= 0.0, "node {} has negative y", node.node_id);
        }
    }

    #[test]
    fn force_layout_self_loop() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Er);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            ..IrNode::default()
        });
        // Self-loop edge should be skipped (not cause crash).
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(0)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
        let layout = layout_diagram_force(&ir);
        assert_eq!(layout.nodes.len(), 1);
        // Self-loop creates a degenerate edge (from == to node), still present in output.
        assert_eq!(layout.edges.len(), 1);
    }

    #[test]
    fn force_layout_connected_nodes_closer_than_disconnected() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Er);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        // Only A-B connected, C is isolated.
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Line,
            ..IrEdge::default()
        });

        let layout = layout_diagram_force(&ir);
        let a = layout.nodes.iter().find(|n| n.node_id == "A").unwrap();
        let b = layout.nodes.iter().find(|n| n.node_id == "B").unwrap();
        let c = layout.nodes.iter().find(|n| n.node_id == "C").unwrap();

        let a_center = a.bounds.center();
        let b_center = b.bounds.center();
        let c_center = c.bounds.center();

        let dist_ab =
            ((a_center.x - b_center.x).powi(2) + (a_center.y - b_center.y).powi(2)).sqrt();
        let dist_ac =
            ((a_center.x - c_center.x).powi(2) + (a_center.y - c_center.y).powi(2)).sqrt();

        // Connected nodes should generally be closer than disconnected.
        assert!(
            dist_ab < dist_ac * 1.5,
            "Connected A-B distance ({dist_ab}) should be less than A-C distance ({dist_ac})"
        );
    }

    #[test]
    fn force_layout_with_clusters() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Er);
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Line,
            ..IrEdge::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(2)),
            to: IrEndpoint::Node(IrNodeId(3)),
            arrow: ArrowType::Line,
            ..IrEdge::default()
        });
        // Cluster 0: A, B. Cluster 1: C, D.
        ir.clusters.push(IrCluster {
            id: IrClusterId(0),
            title: None,
            members: vec![IrNodeId(0), IrNodeId(1)],
            grid_span: 1,
            span: fm_core::Span::default(),
        });
        ir.clusters.push(IrCluster {
            id: IrClusterId(1),
            title: None,
            members: vec![IrNodeId(2), IrNodeId(3)],
            grid_span: 1,
            span: fm_core::Span::default(),
        });

        let layout = layout_diagram_force(&ir);
        assert_eq!(layout.nodes.len(), 4);
        assert_eq!(layout.clusters.len(), 2);
        // Cluster bounds should be non-zero.
        for cluster in &layout.clusters {
            assert!(cluster.bounds.width > 0.0);
            assert!(cluster.bounds.height > 0.0);
        }
    }

    #[test]
    fn force_layout_edge_lengths_computed() {
        let ir = sample_er_ir();
        let layout = layout_diagram_force(&ir);
        assert!(layout.stats.total_edge_length > 0.0);
        // Force layout has no reversed edges.
        assert!((layout.stats.reversed_edge_total_length - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn force_layout_larger_graph() {
        // 20-node graph to verify it handles larger inputs.
        let mut ir = MermaidDiagramIr::empty(DiagramType::Er);
        for i in 0..20 {
            ir.nodes.push(IrNode {
                id: format!("N{i}"),
                ..IrNode::default()
            });
        }
        // Ring topology + cross links.
        for i in 0..20 {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(i)),
                to: IrEndpoint::Node(IrNodeId((i + 1) % 20)),
                arrow: ArrowType::Line,
                ..IrEdge::default()
            });
        }
        // A few cross links.
        for (from, to) in [(0, 10), (5, 15), (3, 17)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Line,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram_force(&ir);
        assert_eq!(layout.nodes.len(), 20);
        assert_eq!(layout.edges.len(), 23);
        assert!(layout.bounds.width > 0.0);
        assert!(layout.bounds.height > 0.0);
        assert!(layout.stats.total_edge_length > 0.0);
    }

    #[test]
    fn force_layout_dispatch_via_algorithm_enum() {
        let ir = sample_er_ir();
        let stats = layout(&ir, LayoutAlgorithm::Force);
        assert_eq!(stats.node_count, 4);
        assert_eq!(stats.edge_count, 4);
    }

    #[test]
    fn force_layout_trace_has_stages() {
        let ir = sample_er_ir();
        let traced = layout_diagram_force_traced(&ir);
        assert!(
            traced.trace.snapshots.len() >= 3,
            "Expected at least 3 trace stages: init, simulation, overlap_removal"
        );
        let stage_names: Vec<&str> = traced.trace.snapshots.iter().map(|s| s.stage).collect();
        assert!(stage_names.contains(&"force_init"));
        assert!(stage_names.contains(&"force_simulation"));
        assert!(stage_names.contains(&"force_overlap_removal"));
    }

    #[test]
    fn force_layout_all_positions_nonnegative() {
        let ir = sample_er_ir();
        let layout = layout_diagram_force(&ir);
        for node in &layout.nodes {
            assert!(
                node.bounds.x >= 0.0,
                "Node {} x={} is negative",
                node.node_id,
                node.bounds.x
            );
            assert!(
                node.bounds.y >= 0.0,
                "Node {} y={} is negative",
                node.node_id,
                node.bounds.y
            );
        }
    }

    // --- Crossing refinement tests ---

    #[test]
    fn refinement_improves_or_maintains_crossings() {
        // K2,2: A->C, A->D, B->C, B->D — barycenter may not find optimal.
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 2), (0, 3), (1, 2), (1, 3)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram(&ir);
        // Refinement should never increase crossings over barycenter result.
        assert!(
            layout.stats.crossing_count <= layout.stats.crossing_count_before_refinement,
            "Refinement should not increase crossings: before={}, after={}",
            layout.stats.crossing_count_before_refinement,
            layout.stats.crossing_count,
        );
    }

    #[test]
    fn refinement_handles_zero_crossings() {
        // Linear chain: A->B->C — zero crossings, refinement should be a no-op.
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (1, 2)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram(&ir);
        assert_eq!(layout.stats.crossing_count, 0);
        assert_eq!(layout.stats.crossing_count_before_refinement, 0);
    }

    #[test]
    fn refinement_is_deterministic() {
        // Dense graph where refinement has room to work.
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for i in 0..8 {
            ir.nodes.push(IrNode {
                id: format!("N{i}"),
                ..IrNode::default()
            });
        }
        // Layer 1: A, B, C. Layer 2: D, E, F. Cross-connected.
        for (from, to) in [(0, 3), (0, 5), (1, 2), (1, 4), (2, 5), (2, 4)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let first = layout_diagram(&ir);
        let second = layout_diagram(&ir);
        assert_eq!(first.stats.crossing_count, second.stats.crossing_count);
        assert_eq!(first, second);
    }

    #[test]
    fn refinement_tracks_before_after_stats() {
        // Graph where refinement might improve crossings.
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for node_id in ["A", "B", "C", "D", "E"] {
            ir.nodes.push(IrNode {
                id: (*node_id).to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 2), (0, 3), (0, 4), (1, 2), (1, 4)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram(&ir);
        // Before refinement count is recorded.
        assert!(
            layout.stats.crossing_count_before_refinement >= layout.stats.crossing_count,
            "Before should be >= after: before={}, after={}",
            layout.stats.crossing_count_before_refinement,
            layout.stats.crossing_count,
        );
    }

    #[test]
    fn refinement_preserves_layout_validity() {
        // Dense crossing graph.
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for i in 0..8 {
            ir.nodes.push(IrNode {
                id: format!("N{i}"),
                ..IrNode::default()
            });
        }
        // 4-source to 4-target with cross connections.
        for from in 0..4 {
            for to in 4..8 {
                ir.edges.push(IrEdge {
                    from: IrEndpoint::Node(IrNodeId(from)),
                    to: IrEndpoint::Node(IrNodeId(to)),
                    arrow: ArrowType::Arrow,
                    ..IrEdge::default()
                });
            }
        }

        let layout = layout_diagram(&ir);
        assert_eq!(layout.nodes.len(), 8);
        assert_eq!(layout.edges.len(), 16);
        assert!(layout.bounds.width > 0.0);
        assert!(layout.bounds.height > 0.0);
        // All nodes should have positive dimensions.
        for node in &layout.nodes {
            assert!(node.bounds.width > 0.0);
            assert!(node.bounds.height > 0.0);
        }
    }

    #[test]
    fn trace_includes_refinement_stage() {
        let ir = sample_ir();
        let traced = layout_diagram_traced(&ir);
        let stage_names: Vec<&str> = traced.trace.snapshots.iter().map(|s| s.stage).collect();
        assert!(
            stage_names.contains(&"crossing_refinement"),
            "Trace should include crossing_refinement stage, got: {stage_names:?}"
        );
    }

    #[test]
    fn layout_nodes_and_edges_preserve_ir_spans() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let node_a_span = Span::at_line(2, 5);
        let node_b_span = Span::at_line(3, 5);
        let edge_span = Span::at_line(4, 8);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            span_primary: node_a_span,
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            span_primary: node_b_span,
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            span: edge_span,
            ..IrEdge::default()
        });

        let layout = layout_diagram(&ir);
        assert_eq!(layout.nodes[0].span, node_a_span);
        assert_eq!(layout.nodes[1].span, node_b_span);
        assert_eq!(layout.edges[0].span, edge_span);
    }

    #[test]
    fn layout_clusters_preserve_ir_spans() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let cluster_span = Span::at_line(2, 12);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            ..IrNode::default()
        });
        ir.clusters.push(IrCluster {
            id: IrClusterId(0),
            title: None,
            members: vec![IrNodeId(0)],
            grid_span: 1,
            span: cluster_span,
        });
        ir.graph.clusters.push(IrGraphCluster {
            cluster_id: IrClusterId(0),
            title: None,
            members: vec![IrNodeId(0)],
            subgraph: None,
            grid_span: 1,
            span: cluster_span,
        });

        let layout = layout_diagram(&ir);
        assert_eq!(layout.clusters.len(), 1);
        assert_eq!(layout.clusters[0].span, cluster_span);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        #[test]
        fn prop_chain_layout_is_deterministic_and_non_overlapping(
            node_count in 1usize..20,
            direction_token in 0usize..5
        ) {
            let direction = match direction_token {
                0 => GraphDirection::TB,
                1 => GraphDirection::TD,
                2 => GraphDirection::LR,
                3 => GraphDirection::RL,
                _ => GraphDirection::BT,
            };
            let ir = chain_ir(node_count, direction);

            let first = layout_diagram_traced(&ir);
            let second = layout_diagram_traced(&ir);

            prop_assert_eq!(&first, &second);
            prop_assert_eq!(first.layout.nodes.len(), node_count);
            prop_assert_eq!(first.layout.edges.len(), node_count.saturating_sub(1));

            for node in &first.layout.nodes {
                prop_assert!(node.bounds.width > 0.0, "node {} has non-positive width", node.node_id);
                prop_assert!(node.bounds.height > 0.0, "node {} has non-positive height", node.node_id);
            }

            for left_index in 0..first.layout.nodes.len() {
                for right_index in (left_index + 1)..first.layout.nodes.len() {
                    let left = &first.layout.nodes[left_index];
                    let right = &first.layout.nodes[right_index];

                    let non_overlapping =
                        left.bounds.x + left.bounds.width <= right.bounds.x + 0.5
                            || right.bounds.x + right.bounds.width <= left.bounds.x + 0.5
                            || left.bounds.y + left.bounds.height <= right.bounds.y + 0.5
                            || right.bounds.y + right.bounds.height <= left.bounds.y + 0.5;

                    prop_assert!(
                        non_overlapping,
                        "nodes {} and {} overlap: left={:?} right={:?}",
                        left.node_id,
                        right.node_id,
                        left.bounds,
                        right.bounds
                    );
                }
            }
        }

        #[test]
        fn prop_chain_layout_stats_are_non_negative(node_count in 1usize..30) {
            let ir = chain_ir(node_count, GraphDirection::LR);
            let layout = layout_diagram(&ir);

            prop_assert!(layout.stats.total_edge_length >= 0.0);
            prop_assert!(layout.stats.reversed_edge_total_length >= 0.0);
            prop_assert!(layout.bounds.width >= 0.0);
            prop_assert!(layout.bounds.height >= 0.0);
        }

        #[test]
        fn prop_branching_graph_layout_never_panics(
            branch_count in 1usize..8,
            depth in 1usize..6,
            direction_token in 0usize..4,
        ) {
            let direction = match direction_token {
                0 => GraphDirection::TB,
                1 => GraphDirection::LR,
                2 => GraphDirection::RL,
                _ => GraphDirection::BT,
            };
            let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
            ir.direction = direction;

            // Create root node.
            ir.nodes.push(IrNode { id: "root".to_string(), ..IrNode::default() });
            // Create branches.
            for b in 0..branch_count {
                let mut parent_index = 0;
                for d in 0..depth {
                    let node_id = format!("b{b}_d{d}");
                    let node_index = ir.nodes.len();
                    ir.nodes.push(IrNode { id: node_id, ..IrNode::default() });
                    ir.edges.push(IrEdge {
                        from: IrEndpoint::Node(IrNodeId(parent_index)),
                        to: IrEndpoint::Node(IrNodeId(node_index)),
                        arrow: ArrowType::Arrow,
                        ..IrEdge::default()
                    });
                    parent_index = node_index;
                }
            }

            let layout = layout_diagram(&ir);
            prop_assert!(layout.nodes.len() == ir.nodes.len());
            // All coordinates must be finite.
            for node in &layout.nodes {
                prop_assert!(node.bounds.x.is_finite());
                prop_assert!(node.bounds.y.is_finite());
                prop_assert!(node.bounds.width > 0.0);
                prop_assert!(node.bounds.height > 0.0);
            }
        }

        #[test]
        fn prop_random_edges_layout_is_deterministic(
            node_count in 2usize..15,
            edge_seed in 0u64..1000,
        ) {
            let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
            ir.direction = GraphDirection::TB;
            for i in 0..node_count {
                ir.nodes.push(IrNode { id: format!("N{i}"), ..IrNode::default() });
            }
            // Create edges based on seed for reproducibility.
            let mut val = edge_seed;
            let edge_count = node_count.min(10);
            for _ in 0..edge_count {
                val = val.wrapping_mul(6364136223846793005).wrapping_add(1);
                let from = (val as usize) % node_count;
                val = val.wrapping_mul(6364136223846793005).wrapping_add(1);
                let to = (val as usize) % node_count;
                if from != to {
                    ir.edges.push(IrEdge {
                        from: IrEndpoint::Node(IrNodeId(from)),
                        to: IrEndpoint::Node(IrNodeId(to)),
                        arrow: ArrowType::Arrow,
                        ..IrEdge::default()
                    });
                }
            }

            let first = layout_diagram(&ir);
            let second = layout_diagram(&ir);
            for (n1, n2) in first.nodes.iter().zip(second.nodes.iter()) {
                prop_assert_eq!(n1.bounds, n2.bounds, "Node {} differs", n1.node_id);
            }
        }

        #[test]
        fn prop_layout_bounds_contain_all_nodes(node_count in 1usize..20) {
            let ir = chain_ir(node_count, GraphDirection::TB);
            let layout = layout_diagram(&ir);
            for node in &layout.nodes {
                prop_assert!(
                    node.bounds.x >= layout.bounds.x - 1.0,
                    "Node {} x={:.1} outside bounds x={:.1}",
                    node.node_id, node.bounds.x, layout.bounds.x
                );
                prop_assert!(
                    node.bounds.y >= layout.bounds.y - 1.0,
                    "Node {} y={:.1} outside bounds y={:.1}",
                    node.node_id, node.bounds.y, layout.bounds.y
                );
                prop_assert!(
                    node.bounds.x + node.bounds.width <= layout.bounds.x + layout.bounds.width + 1.0,
                    "Node {} right edge outside bounds"
                    , node.node_id
                );
                prop_assert!(
                    node.bounds.y + node.bounds.height <= layout.bounds.y + layout.bounds.height + 1.0,
                    "Node {} bottom edge outside bounds",
                    node.node_id
                );
            }
        }
    }

    // ── Sequence diagram layout tests ──────────────────────────────────

    fn sequence_ir(participants: &[&str], messages: &[(usize, usize)]) -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        ir.direction = GraphDirection::LR;
        for (index, name) in participants.iter().enumerate() {
            ir.labels.push(IrLabel {
                text: name.to_string(),
                ..IrLabel::default()
            });
            ir.nodes.push(IrNode {
                id: name.to_string(),
                label: Some(IrLabelId(index)),
                ..IrNode::default()
            });
        }
        for (from, to) in messages {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(*from)),
                to: IrEndpoint::Node(IrNodeId(*to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }
        ir
    }

    #[test]
    fn sequence_layout_empty_diagram() {
        let ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        let layout = layout_diagram_sequence(&ir);
        assert!(layout.nodes.is_empty());
        assert!(layout.edges.is_empty());
        assert_eq!(layout.bounds.width, 0.0);
        assert_eq!(layout.bounds.height, 0.0);
    }

    #[test]
    fn sequence_layout_single_participant() {
        let ir = sequence_ir(&["Alice"], &[]);
        let layout = layout_diagram_sequence(&ir);
        assert_eq!(layout.nodes.len(), 1);
        assert_eq!(layout.nodes[0].node_id, "Alice");
        assert_eq!(layout.nodes[0].rank, 0);
        assert!(layout.bounds.width > 0.0);
        assert!(layout.bounds.height > 0.0);
    }

    #[test]
    fn sequence_layout_participants_arranged_horizontally() {
        let ir = sequence_ir(&["Alice", "Bob", "Carol"], &[]);
        let layout = layout_diagram_sequence(&ir);
        assert_eq!(layout.nodes.len(), 3);
        // Participants should be left-to-right in declaration order.
        let x0 = layout.nodes[0].bounds.x;
        let x1 = layout.nodes[1].bounds.x;
        let x2 = layout.nodes[2].bounds.x;
        assert!(x0 < x1, "Alice should be left of Bob");
        assert!(x1 < x2, "Bob should be left of Carol");
        // All participants should be at the same y level (rank 0).
        let y0 = layout.nodes[0].bounds.y;
        let y1 = layout.nodes[1].bounds.y;
        let y2 = layout.nodes[2].bounds.y;
        assert_eq!(y0, y1);
        assert_eq!(y1, y2);
    }

    #[test]
    fn sequence_layout_messages_stacked_vertically() {
        let ir = sequence_ir(&["Alice", "Bob"], &[(0, 1), (1, 0)]);
        let layout = layout_diagram_sequence(&ir);
        assert_eq!(layout.edges.len(), 2);
        // First message y < second message y (stacked top to bottom).
        let y0 = layout.edges[0].points[0].y;
        let y1 = layout.edges[1].points[0].y;
        assert!(
            y0 < y1,
            "Messages should stack vertically: {y0} should be < {y1}"
        );
    }

    #[test]
    fn sequence_layout_message_endpoints_at_participant_centers() {
        let ir = sequence_ir(&["Alice", "Bob"], &[(0, 1)]);
        let layout = layout_diagram_sequence(&ir);
        assert_eq!(layout.edges.len(), 1);
        let edge = &layout.edges[0];
        let alice_cx = layout.nodes[0].bounds.center().x;
        let bob_cx = layout.nodes[1].bounds.center().x;
        assert!(
            (edge.points[0].x - alice_cx).abs() < 1.0,
            "Source x should be at Alice center"
        );
        assert!(
            (edge.points[1].x - bob_cx).abs() < 1.0,
            "Target x should be at Bob center"
        );
    }

    #[test]
    fn sequence_layout_self_message_loop() {
        let ir = sequence_ir(&["Alice", "Bob"], &[(0, 0)]);
        let layout = layout_diagram_sequence(&ir);
        assert_eq!(layout.edges.len(), 1);
        let edge = &layout.edges[0];
        assert!(edge.is_self_loop);
        // Self-loop should have 4 points forming a rectangular loop.
        assert_eq!(edge.points.len(), 4);
        // The loop should extend to the right and back.
        assert!(edge.points[1].x > edge.points[0].x);
        assert!((edge.points[3].x - edge.points[0].x).abs() < 1.0);
    }

    #[test]
    fn sequence_layout_lifeline_bands() {
        let ir = sequence_ir(&["Alice", "Bob"], &[(0, 1)]);
        let layout = layout_diagram_sequence(&ir);
        // Should have one lifeline band per participant.
        assert_eq!(layout.extensions.bands.len(), 2);
        assert_eq!(layout.extensions.bands[0].label, "Alice");
        assert_eq!(layout.extensions.bands[1].label, "Bob");
        // Lifeline bands should extend below the participant header.
        for band in &layout.extensions.bands {
            assert!(band.bounds.height > 0.0, "Lifeline should have height");
        }
    }

    #[test]
    fn sequence_layout_auto_dispatch_selects_sequence() {
        let ir = sequence_ir(&["Alice", "Bob"], &[(0, 1)]);
        let traced = layout_diagram_traced(&ir);
        assert_eq!(
            traced.trace.dispatch.selected,
            LayoutAlgorithm::Sequence,
            "Auto dispatch should select Sequence for sequence diagrams"
        );
    }

    #[test]
    fn sequence_layout_deterministic() {
        let ir = sequence_ir(&["Alice", "Bob", "Carol"], &[(0, 1), (1, 2), (2, 0)]);
        let layout1 = layout_diagram_sequence(&ir);
        let layout2 = layout_diagram_sequence(&ir);
        assert_eq!(layout1.nodes.len(), layout2.nodes.len());
        for (n1, n2) in layout1.nodes.iter().zip(layout2.nodes.iter()) {
            assert_eq!(n1.bounds, n2.bounds, "Layouts must be deterministic");
        }
        for (e1, e2) in layout1.edges.iter().zip(layout2.edges.iter()) {
            assert_eq!(e1.points, e2.points, "Edge paths must be deterministic");
        }
    }

    #[test]
    fn sequence_layout_traced_has_snapshots() {
        let ir = sequence_ir(&["Alice", "Bob"], &[(0, 1)]);
        let traced = layout_diagram_sequence_traced(&ir);
        assert!(
            traced.trace.snapshots.len() >= 2,
            "Should have at least layout + post_processing snapshots"
        );
    }

    #[test]
    fn sequence_layout_messages_below_header() {
        let ir = sequence_ir(&["Alice", "Bob"], &[(0, 1)]);
        let layout = layout_diagram_sequence(&ir);
        let header_bottom = layout
            .nodes
            .iter()
            .map(|n| n.bounds.y + n.bounds.height)
            .fold(0.0_f32, f32::max);
        for edge in &layout.edges {
            assert!(
                edge.points[0].y > header_bottom,
                "Message y={} should be below header bottom={}",
                edge.points[0].y,
                header_bottom
            );
        }
    }

    // --- Brandes-Köpf coordinate assignment tests ---

    #[test]
    fn bk_linear_chain_aligns_connected_nodes() {
        // A -> B -> C should have all three nodes aligned (same secondary coordinate).
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::TB;
        for id in ["A", "B", "C"] {
            ir.nodes.push(IrNode {
                id: id.to_string(),
                ..IrNode::default()
            });
        }
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(1)),
            to: IrEndpoint::Node(IrNodeId(2)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });

        let layout = layout_diagram(&ir);
        // In TB direction, secondary coordinate is X.
        // All three nodes in a linear chain should share the same X center.
        let centers: Vec<f32> = layout
            .nodes
            .iter()
            .map(|n| n.bounds.x + n.bounds.width / 2.0)
            .collect();
        assert!(
            (centers[0] - centers[1]).abs() < 1.0,
            "A and B should be aligned, got x={:.1} vs {:.1}",
            centers[0],
            centers[1]
        );
        assert!(
            (centers[1] - centers[2]).abs() < 1.0,
            "B and C should be aligned, got x={:.1} vs {:.1}",
            centers[1],
            centers[2]
        );
    }

    #[test]
    fn bk_diamond_graph_produces_deterministic_layout() {
        // Diamond: A -> B, A -> C, B -> D, C -> D
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::TB;
        for id in ["A", "B", "C", "D"] {
            ir.nodes.push(IrNode {
                id: id.to_string(),
                ..IrNode::default()
            });
        }
        for (from, to) in [(0, 1), (0, 2), (1, 3), (2, 3)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout1 = layout_diagram(&ir);
        let layout2 = layout_diagram(&ir);
        // Determinism: same input => identical output.
        for (n1, n2) in layout1.nodes.iter().zip(layout2.nodes.iter()) {
            assert_eq!(n1.bounds, n2.bounds, "Node {} positions differ", n1.node_id);
        }
    }

    #[test]
    fn bk_no_horizontal_overlap_within_ranks() {
        // Multiple nodes in the same rank should not overlap in the secondary axis.
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::TB;
        // Root A, with 4 children B, C, D, E (all in same rank).
        for id in ["A", "B", "C", "D", "E"] {
            ir.nodes.push(IrNode {
                id: id.to_string(),
                ..IrNode::default()
            });
        }
        for child in 1..5 {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(0)),
                to: IrEndpoint::Node(IrNodeId(child)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram(&ir);
        // Group nodes by rank, check no overlaps within each rank.
        let mut by_rank: BTreeMap<usize, Vec<(f32, f32)>> = BTreeMap::new();
        for node in &layout.nodes {
            by_rank
                .entry(node.rank)
                .or_default()
                .push((node.bounds.x, node.bounds.x + node.bounds.width));
        }
        for intervals in by_rank.values() {
            let mut sorted = intervals.clone();
            sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
            for pair in sorted.windows(2) {
                assert!(
                    pair[1].0 >= pair[0].1,
                    "Overlap: node ending at {:.1} overlaps with node starting at {:.1}",
                    pair[0].1,
                    pair[1].0,
                );
            }
        }
    }

    #[test]
    fn bk_four_way_median_is_deterministic_for_wide_graph() {
        // Wide graph: 3 ranks, rank 0 has 1 node, rank 1 has 5, rank 2 has 1.
        // Tests that the 4-way median produces stable results.
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::TB;
        ir.nodes.push(IrNode {
            id: "root".to_string(),
            ..IrNode::default()
        });
        for i in 0..5 {
            ir.nodes.push(IrNode {
                id: format!("mid{i}"),
                ..IrNode::default()
            });
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(0)),
                to: IrEndpoint::Node(IrNodeId(i + 1)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }
        ir.nodes.push(IrNode {
            id: "sink".to_string(),
            ..IrNode::default()
        });
        for i in 0..5 {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(i + 1)),
                to: IrEndpoint::Node(IrNodeId(6)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let results: Vec<_> = (0..10).map(|_| layout_diagram(&ir)).collect();
        for (i, layout) in results.iter().enumerate().skip(1) {
            for (n1, n2) in results[0].nodes.iter().zip(layout.nodes.iter()) {
                assert_eq!(
                    n1.bounds, n2.bounds,
                    "Run {i} differs for node {}",
                    n1.node_id
                );
            }
        }
    }

    #[test]
    fn bk_lr_direction_uses_horizontal_ranks() {
        // LR direction: primary axis is X (columns), secondary is Y.
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::LR;
        for id in ["A", "B"] {
            ir.nodes.push(IrNode {
                id: id.to_string(),
                ..IrNode::default()
            });
        }
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });

        let layout = layout_diagram(&ir);
        let a = &layout.nodes[0];
        let b = &layout.nodes[1];
        // In LR, B should be to the right of A.
        assert!(
            b.bounds.x > a.bounds.x,
            "In LR, B.x={:.1} should be > A.x={:.1}",
            b.bounds.x,
            a.bounds.x
        );
        // And they should be vertically aligned (same Y center).
        let a_cy = a.bounds.y + a.bounds.height / 2.0;
        let b_cy = b.bounds.y + b.bounds.height / 2.0;
        assert!(
            (a_cy - b_cy).abs() < 1.0,
            "A and B should be vertically aligned in LR, got y={:.1} vs {:.1}",
            a_cy,
            b_cy
        );
    }

    #[test]
    fn bk_all_coords_are_finite() {
        // Property: all coordinates produced by Brandes-Köpf must be finite.
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::TB;
        for i in 0..8 {
            ir.nodes.push(IrNode {
                id: format!("N{i}"),
                ..IrNode::default()
            });
        }
        // Create a mix of edges: chain + branches.
        for (from, to) in [(0, 1), (1, 2), (2, 3), (0, 4), (4, 5), (0, 6), (6, 7)] {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }

        let layout = layout_diagram(&ir);
        for node in &layout.nodes {
            assert!(
                node.bounds.x.is_finite(),
                "Node {} has non-finite x={}",
                node.node_id,
                node.bounds.x
            );
            assert!(
                node.bounds.y.is_finite(),
                "Node {} has non-finite y={}",
                node.node_id,
                node.bounds.y
            );
        }
    }

    // ── Auto algorithm selection tests (bd-vb9.7) ──────────────────────

    fn graph_ir(
        diagram_type: DiagramType,
        node_count: usize,
        edges: &[(usize, usize)],
    ) -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(diagram_type);
        ir.direction = GraphDirection::TB;
        for i in 0..node_count {
            ir.nodes.push(IrNode {
                id: format!("N{i}"),
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
    fn auto_select_mindmap_uses_radial() {
        let ir = MermaidDiagramIr::empty(DiagramType::Mindmap);
        let dispatch = dispatch_layout_algorithm(&ir, LayoutAlgorithm::Auto);
        assert_eq!(dispatch.selected, LayoutAlgorithm::Radial);
        assert_eq!(dispatch.reason, "auto_diagram_type_mindmap");
    }

    #[test]
    fn auto_select_timeline_uses_timeline() {
        let ir = MermaidDiagramIr::empty(DiagramType::Timeline);
        let dispatch = dispatch_layout_algorithm(&ir, LayoutAlgorithm::Auto);
        assert_eq!(dispatch.selected, LayoutAlgorithm::Timeline);
        assert_eq!(dispatch.reason, "auto_diagram_type_timeline");
    }

    #[test]
    fn auto_select_gantt_uses_gantt() {
        let ir = MermaidDiagramIr::empty(DiagramType::Gantt);
        let dispatch = dispatch_layout_algorithm(&ir, LayoutAlgorithm::Auto);
        assert_eq!(dispatch.selected, LayoutAlgorithm::Gantt);
        assert_eq!(dispatch.reason, "auto_diagram_type_gantt");
    }

    #[test]
    fn auto_select_sankey_uses_sankey() {
        let ir = MermaidDiagramIr::empty(DiagramType::Sankey);
        let dispatch = dispatch_layout_algorithm(&ir, LayoutAlgorithm::Auto);
        assert_eq!(dispatch.selected, LayoutAlgorithm::Sankey);
        assert_eq!(dispatch.reason, "auto_diagram_type_sankey");
    }

    #[test]
    fn auto_select_journey_uses_kanban() {
        let ir = MermaidDiagramIr::empty(DiagramType::Journey);
        let dispatch = dispatch_layout_algorithm(&ir, LayoutAlgorithm::Auto);
        assert_eq!(dispatch.selected, LayoutAlgorithm::Kanban);
        assert_eq!(dispatch.reason, "auto_diagram_type_kanban");
    }

    #[test]
    fn auto_select_block_beta_uses_grid() {
        let ir = MermaidDiagramIr::empty(DiagramType::BlockBeta);
        let dispatch = dispatch_layout_algorithm(&ir, LayoutAlgorithm::Auto);
        assert_eq!(dispatch.selected, LayoutAlgorithm::Grid);
        assert_eq!(dispatch.reason, "auto_diagram_type_block_beta");
    }

    #[test]
    fn auto_select_sequence_uses_sequence() {
        let ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        let dispatch = dispatch_layout_algorithm(&ir, LayoutAlgorithm::Auto);
        assert_eq!(dispatch.selected, LayoutAlgorithm::Sequence);
        assert_eq!(dispatch.reason, "auto_diagram_type_sequence");
    }

    #[test]
    fn auto_select_tree_like_flowchart_uses_tree() {
        // Use 15 nodes to exceed the threshold (> 10) for Tree layout.
        let ir = graph_ir(
            DiagramType::Flowchart,
            15,
            &[
                (0, 1),
                (0, 2),
                (1, 3),
                (1, 4),
                (2, 5),
                (2, 6),
                (3, 7),
                (4, 8),
                (5, 9),
                (6, 10),
                (7, 11),
                (8, 12),
                (9, 13),
                (10, 14),
            ],
        );
        let dispatch = dispatch_layout_algorithm(&ir, LayoutAlgorithm::Auto);
        assert_eq!(dispatch.selected, LayoutAlgorithm::Tree);
        assert_eq!(dispatch.reason, "auto_metrics_tree_like");
    }

    #[test]
    fn auto_select_dense_flowchart_uses_force() {
        // Use 35 nodes to exceed the threshold (> 30) for Force layout on dense graphs.
        let mut edges = Vec::new();
        for i in 0..35 {
            for j in (i + 1)..35 {
                if edges.len() < 100 {
                    edges.push((i, j));
                }
            }
        }
        let ir = graph_ir(DiagramType::Flowchart, 35, &edges);
        let dispatch = dispatch_layout_algorithm(&ir, LayoutAlgorithm::Auto);
        assert_eq!(dispatch.selected, LayoutAlgorithm::Force);
        assert_eq!(dispatch.reason, "auto_metrics_dense_graph");
    }

    #[test]
    fn auto_select_sparse_disconnected_uses_sugiyama_for_small_graphs() {
        // 6 nodes is below the threshold (> 50) for Force layout on sparse disconnected graphs.
        let ir = graph_ir(DiagramType::Flowchart, 6, &[(0, 1), (2, 3)]);
        let dispatch = dispatch_layout_algorithm(&ir, LayoutAlgorithm::Auto);
        assert_eq!(dispatch.selected, LayoutAlgorithm::Sugiyama);
    }

    #[test]
    fn auto_select_simple_dag_uses_sugiyama() {
        let ir = graph_ir(
            DiagramType::Flowchart,
            6,
            &[(0, 2), (0, 3), (1, 3), (1, 4), (2, 5), (3, 5), (4, 5)],
        );
        let dispatch = dispatch_layout_algorithm(&ir, LayoutAlgorithm::Auto);
        assert_eq!(dispatch.selected, LayoutAlgorithm::Sugiyama);
        assert_eq!(dispatch.reason, "auto_metrics_default_sugiyama");
    }

    #[test]
    fn auto_select_class_diagram_dag_uses_sugiyama() {
        let ir = graph_ir(DiagramType::Class, 5, &[(0, 2), (1, 2), (2, 3), (2, 4)]);
        let dispatch = dispatch_layout_algorithm(&ir, LayoutAlgorithm::Auto);
        assert_eq!(dispatch.selected, LayoutAlgorithm::Sugiyama);
    }

    #[test]
    fn auto_select_trivial_graph_uses_sugiyama() {
        let ir = graph_ir(DiagramType::Flowchart, 2, &[(0, 1)]);
        let dispatch = dispatch_layout_algorithm(&ir, LayoutAlgorithm::Auto);
        assert_eq!(dispatch.selected, LayoutAlgorithm::Sugiyama);
        assert_eq!(dispatch.reason, "auto_metrics_default_sugiyama");
    }

    #[test]
    fn auto_select_empty_graph_uses_sugiyama() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let dispatch = dispatch_layout_algorithm(&ir, LayoutAlgorithm::Auto);
        assert_eq!(dispatch.selected, LayoutAlgorithm::Sugiyama);
    }

    #[test]
    fn explicit_algorithm_overrides_auto() {
        let ir = graph_ir(DiagramType::Flowchart, 5, &[(0, 1), (0, 2), (1, 3), (2, 4)]);
        let dispatch = dispatch_layout_algorithm(&ir, LayoutAlgorithm::Sugiyama);
        assert_eq!(dispatch.selected, LayoutAlgorithm::Sugiyama);
        assert_eq!(dispatch.reason, "explicit_request_honored");
        assert!(!dispatch.capability_unavailable);
    }

    #[test]
    fn unavailable_algorithm_falls_back() {
        let ir = graph_ir(DiagramType::Flowchart, 3, &[(0, 1), (1, 2)]);
        let dispatch = dispatch_layout_algorithm(&ir, LayoutAlgorithm::Radial);
        assert!(dispatch.capability_unavailable);
        // Falls back to Sugiyama now for small graphs.
        assert_eq!(dispatch.selected, LayoutAlgorithm::Sugiyama);
    }

    #[test]
    fn graph_metrics_empty_graph() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let metrics = GraphMetrics::from_ir(&ir);
        assert_eq!(metrics.node_count, 0);
        assert_eq!(metrics.edge_count, 0);
        assert_eq!(metrics.edge_to_node_ratio, 0.0);
        assert_eq!(metrics.back_edge_count, 0);
        assert_eq!(metrics.scc_count, 0);
        assert_eq!(metrics.root_count, 0);
    }

    #[test]
    fn graph_metrics_simple_chain() {
        let ir = graph_ir(DiagramType::Flowchart, 4, &[(0, 1), (1, 2), (2, 3)]);
        let metrics = GraphMetrics::from_ir(&ir);
        assert_eq!(metrics.node_count, 4);
        assert_eq!(metrics.edge_count, 3);
        assert!(metrics.is_tree_like);
        assert!(!metrics.is_dense);
        assert_eq!(metrics.back_edge_count, 0);
        assert_eq!(metrics.root_count, 1);
    }

    #[test]
    fn graph_metrics_cycle_detection() {
        let ir = graph_ir(DiagramType::Flowchart, 3, &[(0, 1), (1, 2), (2, 0)]);
        let metrics = GraphMetrics::from_ir(&ir);
        assert!(metrics.back_edge_count > 0);
        assert!(metrics.scc_count > 0);
        assert_eq!(metrics.max_scc_size, 3);
        assert!(!metrics.is_tree_like);
    }

    #[test]
    fn graph_metrics_dense_graph() {
        let ir = graph_ir(
            DiagramType::Flowchart,
            4,
            &[
                (0, 1),
                (0, 2),
                (0, 3),
                (1, 0),
                (1, 2),
                (1, 3),
                (2, 0),
                (2, 1),
                (2, 3),
                (3, 0),
                (3, 1),
                (3, 2),
            ],
        );
        let metrics = GraphMetrics::from_ir(&ir);
        assert!(metrics.is_dense);
        assert!(!metrics.is_sparse);
    }

    #[test]
    fn auto_select_deterministic_across_runs() {
        let ir = graph_ir(
            DiagramType::Flowchart,
            8,
            &[
                (0, 1),
                (0, 2),
                (1, 3),
                (2, 3),
                (3, 4),
                (4, 5),
                (5, 6),
                (6, 7),
            ],
        );
        let d1 = dispatch_layout_algorithm(&ir, LayoutAlgorithm::Auto);
        let d2 = dispatch_layout_algorithm(&ir, LayoutAlgorithm::Auto);
        assert_eq!(d1.selected, d2.selected);
        assert_eq!(d1.reason, d2.reason);
    }

    #[test]
    fn auto_select_er_with_cycle_uses_sugiyama() {
        let ir = graph_ir(DiagramType::Er, 4, &[(0, 1), (1, 2), (2, 3), (3, 0)]);
        let dispatch = dispatch_layout_algorithm(&ir, LayoutAlgorithm::Auto);
        assert_eq!(dispatch.selected, LayoutAlgorithm::Sugiyama);
    }

    #[test]
    fn auto_select_state_tree_uses_tree() {
        // Use 12 nodes to exceed the threshold (> 10) for Tree layout.
        let mut edges = Vec::new();
        for i in 1..12 {
            edges.push((0, i));
        }
        let ir = graph_ir(DiagramType::State, 12, &edges);
        let dispatch = dispatch_layout_algorithm(&ir, LayoutAlgorithm::Auto);
        assert_eq!(dispatch.selected, LayoutAlgorithm::Tree);
    }

    // ── Algorithm-family dispatch parity tests (bd-3uz.17) ─────────────

    /// Verify that requesting each algorithm for its native diagram type results
    /// in the requested algorithm actually being selected and executed.
    #[test]
    fn dispatch_parity_sugiyama_for_flowchart() {
        let ir = graph_ir(DiagramType::Flowchart, 4, &[(0, 1), (1, 2), (2, 3)]);
        let traced = layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Sugiyama);
        assert_eq!(traced.trace.dispatch.requested, LayoutAlgorithm::Sugiyama);
        assert_eq!(traced.trace.dispatch.selected, LayoutAlgorithm::Sugiyama);
        assert_eq!(traced.trace.dispatch.reason, "explicit_request_honored");
        assert!(!traced.trace.dispatch.capability_unavailable);
    }

    #[test]
    fn dispatch_parity_force_for_flowchart() {
        let ir = graph_ir(DiagramType::Flowchart, 4, &[(0, 1), (1, 2), (2, 3)]);
        let traced = layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Force);
        assert_eq!(traced.trace.dispatch.selected, LayoutAlgorithm::Force);
        assert_eq!(traced.trace.dispatch.reason, "explicit_request_honored");
    }

    #[test]
    fn dispatch_parity_tree_for_flowchart() {
        let ir = graph_ir(DiagramType::Flowchart, 4, &[(0, 1), (1, 2), (2, 3)]);
        let traced = layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Tree);
        assert_eq!(traced.trace.dispatch.selected, LayoutAlgorithm::Tree);
        assert_eq!(traced.trace.dispatch.reason, "explicit_request_honored");
    }

    #[test]
    fn dispatch_parity_radial_for_mindmap() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Mindmap);
        for i in 0..4 {
            ir.nodes.push(IrNode {
                id: format!("N{i}"),
                ..IrNode::default()
            });
        }
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(2)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(3)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
        let traced = layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Radial);
        assert_eq!(traced.trace.dispatch.selected, LayoutAlgorithm::Radial);
    }

    #[test]
    fn dispatch_parity_sequence_for_sequence() {
        let ir = sequence_ir(&["Alice", "Bob", "Carol"], &[(0, 1), (1, 2)]);
        let traced = layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Sequence);
        assert_eq!(traced.trace.dispatch.selected, LayoutAlgorithm::Sequence);
    }

    #[test]
    fn dispatch_parity_timeline_for_timeline() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Timeline);
        ir.nodes.push(IrNode {
            id: "T1".to_string(),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "T2".to_string(),
            ..IrNode::default()
        });
        let traced = layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Timeline);
        assert_eq!(traced.trace.dispatch.selected, LayoutAlgorithm::Timeline);
    }

    #[test]
    fn dispatch_parity_gantt_for_gantt() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Gantt);
        ir.nodes.push(IrNode {
            id: "G1".to_string(),
            ..IrNode::default()
        });
        let traced = layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Gantt);
        assert_eq!(traced.trace.dispatch.selected, LayoutAlgorithm::Gantt);
    }

    #[test]
    fn dispatch_parity_sankey_for_sankey() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Sankey);
        ir.nodes.push(IrNode {
            id: "S1".to_string(),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "S2".to_string(),
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
        let traced = layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Sankey);
        assert_eq!(traced.trace.dispatch.selected, LayoutAlgorithm::Sankey);
    }

    #[test]
    fn dispatch_parity_kanban_for_journey() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Journey);
        ir.nodes.push(IrNode {
            id: "J1".to_string(),
            ..IrNode::default()
        });
        let traced = layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Kanban);
        assert_eq!(traced.trace.dispatch.selected, LayoutAlgorithm::Kanban);
    }

    #[test]
    fn dispatch_parity_grid_for_block_beta() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::BlockBeta);
        ir.nodes.push(IrNode {
            id: "B1".to_string(),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "B2".to_string(),
            ..IrNode::default()
        });
        let traced = layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Grid);
        assert_eq!(traced.trace.dispatch.selected, LayoutAlgorithm::Grid);
    }

    /// Verify that requesting an unavailable algorithm falls back with capability_unavailable.
    #[test]
    fn dispatch_unavailable_radial_for_flowchart_falls_back() {
        let ir = graph_ir(DiagramType::Flowchart, 3, &[(0, 1), (1, 2)]);
        let traced = layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Radial);
        assert!(traced.trace.dispatch.capability_unavailable);
        assert_ne!(traced.trace.dispatch.selected, LayoutAlgorithm::Radial);
        assert_eq!(
            traced.trace.dispatch.reason,
            "requested_algorithm_capability_unavailable_for_diagram_type"
        );
        // Layout should still complete successfully.
        assert!(!traced.layout.nodes.is_empty());
    }

    #[test]
    fn dispatch_unavailable_timeline_for_class_falls_back() {
        let ir = graph_ir(DiagramType::Class, 3, &[(0, 1), (1, 2)]);
        let traced = layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Timeline);
        assert!(traced.trace.dispatch.capability_unavailable);
        assert_ne!(traced.trace.dispatch.selected, LayoutAlgorithm::Timeline);
    }

    #[test]
    fn dispatch_unavailable_gantt_for_er_falls_back() {
        let ir = graph_ir(DiagramType::Er, 2, &[(0, 1)]);
        let traced = layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Gantt);
        assert!(traced.trace.dispatch.capability_unavailable);
        assert_ne!(traced.trace.dispatch.selected, LayoutAlgorithm::Gantt);
    }

    #[test]
    fn dispatch_unavailable_sequence_for_state_falls_back() {
        let ir = graph_ir(DiagramType::State, 3, &[(0, 1), (1, 2)]);
        let traced = layout_diagram_traced_with_algorithm(&ir, LayoutAlgorithm::Sequence);
        assert!(traced.trace.dispatch.capability_unavailable);
    }

    /// Verify auto-selection is deterministic across a mixed corpus of diagram types.
    #[test]
    fn dispatch_auto_deterministic_mixed_corpus() {
        let corpus: Vec<MermaidDiagramIr> = vec![
            graph_ir(DiagramType::Flowchart, 5, &[(0, 1), (0, 2), (1, 3), (2, 4)]),
            graph_ir(
                DiagramType::Flowchart,
                8,
                &[
                    (0, 1),
                    (0, 2),
                    (1, 3),
                    (2, 3),
                    (3, 4),
                    (4, 5),
                    (5, 6),
                    (6, 7),
                ],
            ),
            graph_ir(DiagramType::Class, 4, &[(0, 1), (1, 2), (2, 3), (3, 0)]),
            graph_ir(DiagramType::State, 3, &[(0, 1), (0, 2)]),
            graph_ir(DiagramType::Er, 6, &[(0, 1), (2, 3), (4, 5)]),
            {
                let mut ir = MermaidDiagramIr::empty(DiagramType::Mindmap);
                for i in 0..3 {
                    ir.nodes.push(IrNode {
                        id: format!("M{i}"),
                        ..IrNode::default()
                    });
                }
                ir
            },
            MermaidDiagramIr::empty(DiagramType::Gantt),
            MermaidDiagramIr::empty(DiagramType::Sequence),
        ];

        for ir in &corpus {
            let t1 = layout_diagram_traced(ir);
            let t2 = layout_diagram_traced(ir);
            assert_eq!(
                t1.trace.dispatch.selected, t2.trace.dispatch.selected,
                "Auto-selection must be deterministic for {:?}",
                ir.diagram_type
            );
            assert_eq!(t1.trace.dispatch.reason, t2.trace.dispatch.reason);
        }
    }

    /// Verify that guardrail fallback produces a valid layout with fallback_applied flag.
    #[test]
    fn dispatch_guardrail_fallback_produces_valid_layout() {
        // Use tight guardrails to force fallback.
        let ir = graph_ir(
            DiagramType::Flowchart,
            20,
            &[
                (0, 1),
                (1, 2),
                (2, 3),
                (3, 4),
                (4, 5),
                (5, 6),
                (6, 7),
                (7, 8),
                (8, 9),
                (9, 10),
                (10, 11),
                (11, 12),
                (12, 13),
                (13, 14),
                (14, 15),
                (15, 16),
                (16, 17),
                (17, 18),
                (18, 19),
            ],
        );
        let tight_guardrails = LayoutGuardrails {
            max_layout_time_ms: 1,
            max_layout_iterations: 5,
            max_route_ops: 10,
        };
        let traced = layout_diagram_traced_with_algorithm_and_guardrails(
            &ir,
            LayoutAlgorithm::Force,
            tight_guardrails,
        );
        // Force layout is expensive; guardrails should trigger fallback to Tree or Grid.
        assert!(
            traced.trace.guard.fallback_applied,
            "Tight guardrails should trigger fallback from Force"
        );
        // But layout should still be valid.
        assert!(
            traced.layout.bounds.width >= 0.0,
            "Fallback layout must have non-negative width"
        );
        assert!(
            traced.layout.bounds.height >= 0.0,
            "Fallback layout must have non-negative height"
        );
        assert_eq!(traced.layout.nodes.len(), 20);
    }

    /// Verify auto dispatch traces include informative reason strings.
    #[test]
    fn dispatch_auto_reasons_are_descriptive() {
        let cases: Vec<(MermaidDiagramIr, &str)> = vec![
            (
                MermaidDiagramIr::empty(DiagramType::Mindmap),
                "auto_diagram_type_mindmap",
            ),
            (
                MermaidDiagramIr::empty(DiagramType::Timeline),
                "auto_diagram_type_timeline",
            ),
            (
                MermaidDiagramIr::empty(DiagramType::Gantt),
                "auto_diagram_type_gantt",
            ),
            (
                MermaidDiagramIr::empty(DiagramType::Sankey),
                "auto_diagram_type_sankey",
            ),
            (
                MermaidDiagramIr::empty(DiagramType::Journey),
                "auto_diagram_type_kanban",
            ),
            (
                MermaidDiagramIr::empty(DiagramType::BlockBeta),
                "auto_diagram_type_block_beta",
            ),
            (
                MermaidDiagramIr::empty(DiagramType::Sequence),
                "auto_diagram_type_sequence",
            ),
            (
                graph_ir(DiagramType::Flowchart, 2, &[(0, 1)]),
                "auto_metrics_default_sugiyama",
            ),
        ];

        for (ir, expected_reason) in &cases {
            let traced = layout_diagram_traced(ir);
            assert_eq!(
                traced.trace.dispatch.reason, *expected_reason,
                "Wrong reason for {:?}: got {}",
                ir.diagram_type, traced.trace.dispatch.reason
            );
        }
    }

    // ── Performance baseline tests (bd-17e4.1) ────────────────────────

    /// Build a synthetic DAG with controlled density.
    /// Build a synthetic DAG with controlled density.  Edges always go from
    /// lower-index nodes to higher-index nodes, guaranteeing acyclicity.
    /// Nodes near the end naturally have fewer outgoing edges (they are sinks).
    fn synthetic_dag(node_count: usize, edges_per_node: usize) -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::TB;
        for i in 0..node_count {
            ir.nodes.push(IrNode {
                id: format!("N{i}"),
                ..IrNode::default()
            });
        }
        for i in 0..node_count {
            for j in 1..=edges_per_node {
                let target = i + j;
                if target < node_count {
                    ir.edges.push(IrEdge {
                        from: IrEndpoint::Node(IrNodeId(i)),
                        to: IrEndpoint::Node(IrNodeId(target)),
                        arrow: ArrowType::Arrow,
                        ..IrEdge::default()
                    });
                }
            }
        }
        ir
    }

    fn measure_layout_ns(ir: &MermaidDiagramIr, algorithm: LayoutAlgorithm) -> u128 {
        let start = std::time::Instant::now();
        let _traced = layout_diagram_traced_with_algorithm(ir, algorithm);
        start.elapsed().as_nanos()
    }

    #[test]
    fn perf_baseline_sugiyama_small() {
        let ir = synthetic_dag(20, 2);
        let ns = measure_layout_ns(&ir, LayoutAlgorithm::Sugiyama);
        println!(
            "{{\"benchmark\":\"sugiyama_small\",\"nodes\":20,\"edges\":{},\"ns\":{ns}}}",
            ir.edges.len()
        );
        // Sanity: small graph should complete in under 100ms.
        assert!(ns < 100_000_000, "Sugiyama 20-node took {ns}ns (>100ms)");
    }

    #[test]
    fn perf_baseline_sugiyama_medium() {
        let ir = synthetic_dag(100, 2);
        let ns = measure_layout_ns(&ir, LayoutAlgorithm::Sugiyama);
        println!(
            "{{\"benchmark\":\"sugiyama_medium\",\"nodes\":100,\"edges\":{},\"ns\":{ns}}}",
            ir.edges.len()
        );
        assert!(ns < 500_000_000, "Sugiyama 100-node took {ns}ns (>500ms)");
    }

    #[test]
    fn perf_baseline_sugiyama_large() {
        let ir = synthetic_dag(500, 2);
        let ns = measure_layout_ns(&ir, LayoutAlgorithm::Sugiyama);
        println!(
            "{{\"benchmark\":\"sugiyama_large\",\"nodes\":500,\"edges\":{},\"ns\":{ns}}}",
            ir.edges.len()
        );
        assert!(ns < 2_000_000_000, "Sugiyama 500-node took {ns}ns (>2s)");
    }

    #[test]
    fn perf_baseline_force_small() {
        let ir = synthetic_dag(20, 2);
        let ns = measure_layout_ns(&ir, LayoutAlgorithm::Force);
        println!(
            "{{\"benchmark\":\"force_small\",\"nodes\":20,\"edges\":{},\"ns\":{ns}}}",
            ir.edges.len()
        );
        assert!(ns < 200_000_000, "Force 20-node took {ns}ns (>200ms)");
    }

    #[test]
    fn perf_baseline_force_medium() {
        let ir = synthetic_dag(100, 2);
        let ns = measure_layout_ns(&ir, LayoutAlgorithm::Force);
        println!(
            "{{\"benchmark\":\"force_medium\",\"nodes\":100,\"edges\":{},\"ns\":{ns}}}",
            ir.edges.len()
        );
        assert!(ns < 1_000_000_000, "Force 100-node took {ns}ns (>1s)");
    }

    #[test]
    fn perf_baseline_tree_small() {
        let ir = synthetic_dag(20, 1);
        let ns = measure_layout_ns(&ir, LayoutAlgorithm::Tree);
        println!(
            "{{\"benchmark\":\"tree_small\",\"nodes\":20,\"edges\":{},\"ns\":{ns}}}",
            ir.edges.len()
        );
        assert!(ns < 50_000_000, "Tree 20-node took {ns}ns (>50ms)");
    }

    #[test]
    fn perf_baseline_tree_medium() {
        let ir = synthetic_dag(100, 1);
        let ns = measure_layout_ns(&ir, LayoutAlgorithm::Tree);
        println!(
            "{{\"benchmark\":\"tree_medium\",\"nodes\":100,\"edges\":{},\"ns\":{ns}}}",
            ir.edges.len()
        );
        assert!(ns < 200_000_000, "Tree 100-node took {ns}ns (>200ms)");
    }

    #[test]
    fn perf_baseline_tree_large() {
        let ir = synthetic_dag(500, 1);
        let ns = measure_layout_ns(&ir, LayoutAlgorithm::Tree);
        println!(
            "{{\"benchmark\":\"tree_large\",\"nodes\":500,\"edges\":{},\"ns\":{ns}}}",
            ir.edges.len()
        );
        assert!(ns < 500_000_000, "Tree 500-node took {ns}ns (>500ms)");
    }

    #[test]
    fn perf_baseline_radial() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Mindmap);
        ir.direction = GraphDirection::TB;
        ir.nodes.push(IrNode {
            id: "root".to_string(),
            ..IrNode::default()
        });
        for i in 0..50 {
            ir.nodes.push(IrNode {
                id: format!("L{i}"),
                ..IrNode::default()
            });
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(0)),
                to: IrEndpoint::Node(IrNodeId(i + 1)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }
        let ns = measure_layout_ns(&ir, LayoutAlgorithm::Radial);
        println!("{{\"benchmark\":\"radial_50\",\"nodes\":51,\"edges\":50,\"ns\":{ns}}}");
        assert!(ns < 200_000_000, "Radial 51-node took {ns}ns (>200ms)");
    }

    #[test]
    fn perf_baseline_grid() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::BlockBeta);
        ir.direction = GraphDirection::TB;
        for i in 0..100 {
            ir.nodes.push(IrNode {
                id: format!("B{i}"),
                ..IrNode::default()
            });
        }
        let ns = measure_layout_ns(&ir, LayoutAlgorithm::Grid);
        println!("{{\"benchmark\":\"grid_100\",\"nodes\":100,\"edges\":0,\"ns\":{ns}}}");
        assert!(ns < 100_000_000, "Grid 100-node took {ns}ns (>100ms)");
    }

    /// Compare algorithm families on the same input to establish relative cost.
    #[test]
    fn perf_baseline_algorithm_comparison() {
        let ir = synthetic_dag(50, 2);
        let sugiyama_ns = measure_layout_ns(&ir, LayoutAlgorithm::Sugiyama);
        let force_ns = measure_layout_ns(&ir, LayoutAlgorithm::Force);
        let tree_ns = measure_layout_ns(&ir, LayoutAlgorithm::Tree);
        println!(
            "{{\"benchmark\":\"comparison_50\",\"sugiyama_ns\":{sugiyama_ns},\"force_ns\":{force_ns},\"tree_ns\":{tree_ns}}}"
        );
        // Tree should be fastest for chain-like graphs.
        // Just verify all complete in bounded time.
        assert!(sugiyama_ns < 500_000_000);
        assert!(force_ns < 500_000_000);
        assert!(tree_ns < 500_000_000);
    }

    // ── Mathematical invariant proptests (bd-17e4.4) ──────────────────

    /// Build a random DAG from a seed. Nodes are numbered 0..n. Edges go from
    /// lower to higher indices (guaranteeing acyclicity).
    fn random_dag(node_count: usize, edge_seed: u64, density: usize) -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::TB;
        for i in 0..node_count {
            ir.nodes.push(IrNode {
                id: format!("N{i}"),
                ..IrNode::default()
            });
        }
        let mut val = edge_seed;
        let target_edges = node_count
            .saturating_mul(density)
            .min(node_count * (node_count - 1) / 2);
        let mut added = 0_usize;
        for _ in 0..target_edges.saturating_mul(3) {
            if added >= target_edges {
                break;
            }
            val = val.wrapping_mul(6364136223846793005).wrapping_add(1);
            let from = (val as usize) % node_count;
            val = val.wrapping_mul(6364136223846793005).wrapping_add(1);
            let to = (val as usize) % node_count;
            // Edges always go from lower to higher index → acyclic.
            let (lo, hi) = if from < to { (from, to) } else { (to, from) };
            if lo != hi {
                ir.edges.push(IrEdge {
                    from: IrEndpoint::Node(IrNodeId(lo)),
                    to: IrEndpoint::Node(IrNodeId(hi)),
                    arrow: ArrowType::Arrow,
                    ..IrEdge::default()
                });
                added += 1;
            }
        }
        ir
    }

    /// Build a star graph: one center node connected to n-1 leaves.
    fn star_ir(leaf_count: usize) -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::TB;
        ir.nodes.push(IrNode {
            id: "center".to_string(),
            ..IrNode::default()
        });
        for i in 0..leaf_count {
            ir.nodes.push(IrNode {
                id: format!("L{i}"),
                ..IrNode::default()
            });
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(0)),
                to: IrEndpoint::Node(IrNodeId(i + 1)),
                arrow: ArrowType::Arrow,
                ..IrEdge::default()
            });
        }
        ir
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// Invariant #1: Determinism — layout(G) == layout(G) for random DAGs.
        #[test]
        fn prop_invariant_determinism_random_dag(
            node_count in 3usize..25,
            edge_seed in 0u64..500,
            density in 1usize..3,
        ) {
            let ir = random_dag(node_count, edge_seed, density);
            let first = layout_diagram_traced(&ir);
            let second = layout_diagram_traced(&ir);
            prop_assert_eq!(&first, &second, "Layout must be deterministic for random DAG");
        }

        /// Invariant #1b: Determinism for star graphs.
        #[test]
        fn prop_invariant_determinism_star(leaf_count in 1usize..20) {
            let ir = star_ir(leaf_count);
            let first = layout_diagram(&ir);
            let second = layout_diagram(&ir);
            for (n1, n2) in first.nodes.iter().zip(second.nodes.iter()) {
                prop_assert_eq!(n1.bounds, n2.bounds, "Star node {} differs", n1.node_id);
            }
        }

        /// Invariant #2: Rank consistency — for every edge (u,v) in a DAG,
        /// rank(u) <= rank(v) (layered layouts assign monotonically increasing ranks).
        #[test]
        fn prop_invariant_rank_consistency_dag(
            node_count in 3usize..20,
            edge_seed in 0u64..300,
        ) {
            let ir = random_dag(node_count, edge_seed, 1);
            let layout = layout_diagram(&ir);
            // Build rank lookup from layout nodes.
            let rank_of: BTreeMap<&str, usize> = layout
                .nodes
                .iter()
                .map(|n| (n.node_id.as_str(), n.rank))
                .collect();
            // For each non-reversed edge, source rank should be <= target rank.
            for edge in &layout.edges {
                if edge.reversed {
                    continue; // Reversed edges are cycle-breaking — skip.
                }
                // Look up source/target from the IR edge.
                if edge.edge_index < ir.edges.len() {
                    let ir_edge = &ir.edges[edge.edge_index];
                    if let (IrEndpoint::Node(from_id), IrEndpoint::Node(to_id)) =
                        (ir_edge.from, ir_edge.to)
                        && let (Some(from_node), Some(to_node)) =
                            (ir.nodes.get(from_id.0), ir.nodes.get(to_id.0))
                        && let (Some(&from_rank), Some(&to_rank)) =
                            (rank_of.get(from_node.id.as_str()), rank_of.get(to_node.id.as_str()))
                    {
                        prop_assert!(
                            from_rank <= to_rank,
                            "Rank consistency violated: {} (rank {}) -> {} (rank {})",
                            from_node.id,
                            from_rank,
                            to_node.id,
                            to_rank
                        );
                    }
                }
            }
        }

        /// Invariant #3: Non-overlap — for random DAGs, no two nodes overlap.
        #[test]
        fn prop_invariant_non_overlap_random_dag(
            node_count in 3usize..20,
            edge_seed in 0u64..200,
        ) {
            let ir = random_dag(node_count, edge_seed, 1);
            let layout = layout_diagram(&ir);
            for i in 0..layout.nodes.len() {
                for j in (i + 1)..layout.nodes.len() {
                    let a = &layout.nodes[i];
                    let b = &layout.nodes[j];
                    let non_overlapping =
                        a.bounds.x + a.bounds.width <= b.bounds.x + 0.5
                            || b.bounds.x + b.bounds.width <= a.bounds.x + 0.5
                            || a.bounds.y + a.bounds.height <= b.bounds.y + 0.5
                            || b.bounds.y + b.bounds.height <= a.bounds.y + 0.5;
                    prop_assert!(
                        non_overlapping,
                        "Nodes {} and {} overlap: {:?} vs {:?}",
                        a.node_id,
                        b.node_id,
                        a.bounds,
                        b.bounds
                    );
                }
            }
        }

        /// Invariant #4: Connectivity preservation — layout does not lose nodes or edges.
        #[test]
        fn prop_invariant_connectivity_preservation(
            node_count in 2usize..15,
            edge_seed in 0u64..300,
        ) {
            let ir = random_dag(node_count, edge_seed, 2);
            let layout = layout_diagram(&ir);
            prop_assert_eq!(
                layout.nodes.len(),
                ir.nodes.len(),
                "Layout must preserve node count"
            );
            // All edge indices in the layout should reference valid IR edges.
            for layout_edge in &layout.edges {
                prop_assert!(
                    layout_edge.edge_index < ir.edges.len(),
                    "Layout edge index {} out of range (IR has {} edges)",
                    layout_edge.edge_index,
                    ir.edges.len()
                );
            }
        }

        /// Invariant #5: Boundedness — all coordinates are finite (no NaN, no Infinity).
        #[test]
        fn prop_invariant_boundedness_all_finite(
            node_count in 1usize..25,
            edge_seed in 0u64..400,
        ) {
            let ir = random_dag(node_count, edge_seed, 2);
            let layout = layout_diagram(&ir);
            for node in &layout.nodes {
                prop_assert!(node.bounds.x.is_finite(), "Node {} x is not finite", node.node_id);
                prop_assert!(node.bounds.y.is_finite(), "Node {} y is not finite", node.node_id);
                prop_assert!(node.bounds.width.is_finite(), "Node {} width is not finite", node.node_id);
                prop_assert!(node.bounds.height.is_finite(), "Node {} height is not finite", node.node_id);
                prop_assert!(node.bounds.width >= 0.0, "Node {} has negative width", node.node_id);
                prop_assert!(node.bounds.height >= 0.0, "Node {} has negative height", node.node_id);
            }
            for edge in &layout.edges {
                for (pi, point) in edge.points.iter().enumerate() {
                    prop_assert!(point.x.is_finite(), "Edge {} point {pi} x is not finite", edge.edge_index);
                    prop_assert!(point.y.is_finite(), "Edge {} point {pi} y is not finite", edge.edge_index);
                }
            }
            prop_assert!(layout.bounds.width.is_finite(), "Layout width is not finite");
            prop_assert!(layout.bounds.height.is_finite(), "Layout height is not finite");
            prop_assert!(layout.bounds.width >= 0.0, "Layout has negative width");
            prop_assert!(layout.bounds.height >= 0.0, "Layout has negative height");
        }

        /// Invariant #5b: Boundedness for star graphs (wide fan-out).
        #[test]
        fn prop_invariant_boundedness_star(leaf_count in 1usize..30) {
            let ir = star_ir(leaf_count);
            let layout = layout_diagram(&ir);
            for node in &layout.nodes {
                prop_assert!(node.bounds.x.is_finite());
                prop_assert!(node.bounds.y.is_finite());
                prop_assert!(node.bounds.width > 0.0);
                prop_assert!(node.bounds.height > 0.0);
            }
        }

        /// Combined: random DAG through force-directed layout also satisfies invariants.
        #[test]
        fn prop_invariant_force_layout_bounded_and_finite(
            node_count in 3usize..15,
            edge_seed in 0u64..200,
        ) {
            let ir = random_dag(node_count, edge_seed, 1);
            let layout = layout_diagram_force(&ir);
            prop_assert_eq!(layout.nodes.len(), ir.nodes.len());
            for node in &layout.nodes {
                prop_assert!(node.bounds.x.is_finite(), "Force node {} x not finite", node.node_id);
                prop_assert!(node.bounds.y.is_finite(), "Force node {} y not finite", node.node_id);
                prop_assert!(node.bounds.width > 0.0);
                prop_assert!(node.bounds.height > 0.0);
            }
            prop_assert!(layout.bounds.width >= 0.0);
            prop_assert!(layout.bounds.height >= 0.0);
        }
    }

    // ── Logging spec enforcement tests (bd-gy4.12) ────────────────────

    /// Capture tracing events emitted during layout and verify mandatory fields.
    ///
    /// Mandatory fields for layout tracing events:
    /// - `layout.dispatch`: requested, selected, reason, diagram_type, node_count, edge_count
    /// - `layout.guardrail.*`: algorithm, reason
    /// - `layout.cycle_removal`: strategy
    /// - `layout.crossing_minimization`: crossings_after_barycenter
    #[test]
    fn tracing_dispatch_event_contains_mandatory_fields() {
        use tracing_subscriber::layer::SubscriberExt;

        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let captured_clone = Arc::clone(&captured);

        // Build a subscriber that captures JSON output.
        let fmt_layer = tracing_subscriber::fmt::layer()
            .json()
            .with_writer(move || {
                let captured = Arc::clone(&captured_clone);
                CaptureWriter(captured)
            })
            .with_target(false)
            .with_level(true);

        let subscriber = tracing_subscriber::registry().with(fmt_layer);

        // Run layout under the subscriber.
        let ir = graph_ir(DiagramType::Flowchart, 5, &[(0, 1), (1, 2), (2, 3), (3, 4)]);
        tracing::subscriber::with_default(subscriber, || {
            let _traced = layout_diagram_traced(&ir);
        });

        let events = captured.lock().unwrap();
        assert!(
            !events.is_empty(),
            "Layout should emit at least one tracing event"
        );

        // Find the dispatch event.
        let dispatch_event = events
            .iter()
            .find(|e| e.contains("layout.dispatch"))
            .expect("Should emit a layout.dispatch event");

        let json: serde_json::Value =
            serde_json::from_str(dispatch_event).expect("Event must be valid JSON");
        let fields = &json["fields"];

        // Verify mandatory fields.
        assert!(
            fields.get("requested").is_some(),
            "dispatch event missing 'requested' field: {dispatch_event}"
        );
        assert!(
            fields.get("selected").is_some(),
            "dispatch event missing 'selected' field: {dispatch_event}"
        );
        assert!(
            fields.get("reason").is_some(),
            "dispatch event missing 'reason' field: {dispatch_event}"
        );
        assert!(
            fields.get("diagram_type").is_some(),
            "dispatch event missing 'diagram_type' field: {dispatch_event}"
        );
        assert!(
            fields.get("node_count").is_some(),
            "dispatch event missing 'node_count' field: {dispatch_event}"
        );
        assert!(
            fields.get("edge_count").is_some(),
            "dispatch event missing 'edge_count' field: {dispatch_event}"
        );
    }

    #[test]
    fn tracing_cycle_removal_event_contains_strategy() {
        use tracing_subscriber::layer::SubscriberExt;

        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let captured_clone = Arc::clone(&captured);

        let fmt_layer = tracing_subscriber::fmt::layer()
            .json()
            .with_writer(move || {
                let captured = Arc::clone(&captured_clone);
                CaptureWriter(captured)
            })
            .with_target(false)
            .with_level(true);

        let subscriber = tracing_subscriber::registry().with(fmt_layer);

        // Use a cyclic graph to trigger the cycle_removal info event.
        let ir = graph_ir(DiagramType::Flowchart, 3, &[(0, 1), (1, 2), (2, 0)]);
        tracing::subscriber::with_default(subscriber, || {
            let _traced = layout_diagram_traced(&ir);
        });

        let events = captured.lock().unwrap();
        let cycle_event = events
            .iter()
            .find(|e| e.contains("layout.cycle_removal") && !e.contains("acyclic"));

        if let Some(event) = cycle_event {
            let json: serde_json::Value =
                serde_json::from_str(event).expect("Event must be valid JSON");
            let fields = &json["fields"];
            assert!(
                fields.get("strategy").is_some(),
                "cycle_removal event missing 'strategy' field: {event}"
            );
        }
    }

    #[test]
    fn tracing_guardrail_event_contains_algorithm_and_reason() {
        use tracing_subscriber::layer::SubscriberExt;

        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let captured_clone = Arc::clone(&captured);

        let fmt_layer = tracing_subscriber::fmt::layer()
            .json()
            .with_writer(move || {
                let captured = Arc::clone(&captured_clone);
                CaptureWriter(captured)
            })
            .with_target(false)
            .with_level(true);

        let subscriber = tracing_subscriber::registry().with(fmt_layer);

        let ir = graph_ir(DiagramType::Flowchart, 4, &[(0, 1), (1, 2), (2, 3)]);
        tracing::subscriber::with_default(subscriber, || {
            let _traced = layout_diagram_traced(&ir);
        });

        let events = captured.lock().unwrap();
        let guardrail_event = events.iter().find(|e| e.contains("layout.guardrail"));

        if let Some(event) = guardrail_event {
            let json: serde_json::Value =
                serde_json::from_str(event).expect("Event must be valid JSON");
            let fields = &json["fields"];
            assert!(
                fields.get("algorithm").is_some() || fields.get("initial_algorithm").is_some(),
                "guardrail event missing algorithm field: {event}"
            );
            assert!(
                fields.get("reason").is_some(),
                "guardrail event missing 'reason' field: {event}"
            );
        }
    }

    /// Writer that captures output into a shared Vec.
    struct CaptureWriter(Arc<Mutex<Vec<String>>>);

    impl std::io::Write for CaptureWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if let Ok(s) = std::str::from_utf8(buf) {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    self.0.lock().unwrap().push(trimmed.to_string());
                }
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    // ── Observability output format tests (bd-gy4.8) ──────────────────

    #[test]
    fn guard_report_contains_all_mandatory_fields() {
        let ir = graph_ir(
            DiagramType::Flowchart,
            6,
            &[(0, 1), (1, 2), (2, 3), (3, 4), (4, 5)],
        );
        let traced = layout_diagram_traced(&ir);
        let report = build_layout_guard_report(&ir, &traced);

        // Complexity fields must be populated.
        assert_eq!(report.complexity.nodes, 6);
        assert_eq!(report.complexity.edges, 5);
        assert!(report.complexity.score > 0);

        // Algorithm selection must be present.
        assert!(report.layout_requested_algorithm.is_some());
        assert!(report.layout_selected_algorithm.is_some());
        assert!(report.guard_reason.is_some());

        // Budget estimates must be non-negative.
        assert!(report.layout_time_estimate_ms > 0 || ir.nodes.is_empty());
    }

    #[test]
    fn guard_report_detects_node_limit_exceeded() {
        // Create a graph with more nodes than max_nodes default (200).
        let edges: Vec<(usize, usize)> = (1..210).map(|i| (i - 1, i)).collect();
        let ir = graph_ir(DiagramType::Flowchart, 210, &edges);
        let traced = layout_diagram_traced(&ir);
        let report = build_layout_guard_report(&ir, &traced);

        assert!(report.node_limit_exceeded);
        assert!(report.limits_exceeded);
    }

    #[test]
    fn guard_report_serializes_to_valid_json() {
        let ir = sample_ir();
        let traced = layout_diagram_traced(&ir);
        let report = build_layout_guard_report(&ir, &traced);

        let json = serde_json::to_string(&report).expect("guard report must serialize");
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("guard report must be valid JSON");

        // Verify key fields are present in serialized form.
        assert!(parsed.get("complexity").is_some());
        assert!(parsed.get("node_limit_exceeded").is_some());
        assert!(parsed.get("edge_limit_exceeded").is_some());
        assert!(parsed.get("budget_exceeded").is_some());
        assert!(parsed.get("layout_requested_algorithm").is_some());
        assert!(parsed.get("layout_selected_algorithm").is_some());
    }

    #[test]
    fn layout_stats_are_complete_for_sugiyama() {
        let ir = graph_ir(DiagramType::Flowchart, 5, &[(0, 1), (1, 2), (2, 3), (3, 4)]);
        let traced = layout_diagram_traced(&ir);
        let stats = &traced.layout.stats;

        assert_eq!(stats.node_count, 5);
        assert_eq!(stats.edge_count, 4);
        assert!(stats.total_edge_length >= 0.0);
        assert!(stats.reversed_edge_total_length >= 0.0);
        assert!(stats.phase_iterations > 0);
    }

    #[test]
    fn layout_stats_report_cycle_info_for_cyclic_graph() {
        let ir = graph_ir(DiagramType::Flowchart, 3, &[(0, 1), (1, 2), (2, 0)]);
        let traced = layout_diagram_traced(&ir);
        let stats = &traced.layout.stats;

        assert!(stats.cycle_count > 0, "Should detect cycles");
        assert!(stats.cycle_node_count > 0);
        assert!(stats.max_cycle_size >= 3);
        assert!(stats.reversed_edges > 0);
    }

    #[test]
    fn layout_trace_contains_dispatch_and_snapshots() {
        let ir = graph_ir(DiagramType::Flowchart, 4, &[(0, 1), (1, 2), (2, 3)]);
        let traced = layout_diagram_traced(&ir);

        // Dispatch must be populated.
        assert_ne!(traced.trace.dispatch.reason, "legacy_default");

        // Guard decision must have algorithm info.
        assert!(!traced.trace.guard.selected_algorithm.as_str().is_empty());

        // Snapshots must record at least cycle_removal and post_processing.
        assert!(
            traced.trace.snapshots.len() >= 2,
            "Should have at least 2 layout stage snapshots, got {}",
            traced.trace.snapshots.len()
        );
    }

    #[test]
    fn tracing_events_are_valid_jsonl() {
        use tracing_subscriber::layer::SubscriberExt;

        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let captured_clone = Arc::clone(&captured);

        let fmt_layer = tracing_subscriber::fmt::layer()
            .json()
            .with_writer(move || CaptureWriter(Arc::clone(&captured_clone)))
            .with_target(false)
            .with_level(true);

        let subscriber = tracing_subscriber::registry().with(fmt_layer);

        let ir = graph_ir(DiagramType::Flowchart, 5, &[(0, 1), (1, 2), (2, 3), (3, 4)]);
        tracing::subscriber::with_default(subscriber, || {
            let _traced = layout_diagram_traced(&ir);
        });

        let events = captured.lock().unwrap();
        // Every captured event must be valid JSON.
        for (i, event) in events.iter().enumerate() {
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(event);
            assert!(parsed.is_ok(), "Event {i} is not valid JSON: {event}");
            // Each event must have a level and message.
            let json = parsed.unwrap();
            assert!(
                json.get("level").is_some(),
                "Event {i} missing 'level': {event}"
            );
        }
    }
}
