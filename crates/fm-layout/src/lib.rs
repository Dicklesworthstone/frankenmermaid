#![forbid(unsafe_code)]

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use fm_core::{GraphDirection, IrEndpoint, IrLabelId, MermaidDiagramIr};

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
    Grid,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LayoutStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub crossing_count: usize,
    pub reversed_edges: usize,
    pub cycle_count: usize,
    pub cycle_node_count: usize,
    pub max_cycle_size: usize,
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
    pub bounds: LayoutRect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutClusterBox {
    pub cluster_index: usize,
    pub bounds: LayoutRect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutEdgePath {
    pub edge_index: usize,
    pub points: Vec<LayoutPoint>,
    pub reversed: bool,
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
            node_spacing: 48.0,
            rank_spacing: 72.0,
            cluster_padding: 24.0,
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
    pub snapshots: Vec<LayoutStageSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiagramLayout {
    pub nodes: Vec<LayoutNodeBox>,
    pub clusters: Vec<LayoutClusterBox>,
    pub edges: Vec<LayoutEdgePath>,
    pub bounds: LayoutRect,
    pub stats: LayoutStats,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TracedLayout {
    pub layout: DiagramLayout,
    pub trace: LayoutTrace,
}

#[must_use]
pub fn layout(ir: &MermaidDiagramIr, _algorithm: LayoutAlgorithm) -> LayoutStats {
    layout_diagram(ir).stats
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
pub fn layout_diagram_traced(ir: &MermaidDiagramIr) -> TracedLayout {
    layout_diagram_traced_with_cycle_strategy(ir, default_cycle_strategy())
}

#[must_use]
pub fn layout_diagram_traced_with_cycle_strategy(
    ir: &MermaidDiagramIr,
    cycle_strategy: CycleStrategy,
) -> TracedLayout {
    let mut trace = LayoutTrace::default();
    let spacing = LayoutSpacing::default();
    let node_sizes = compute_node_sizes(ir);
    let cycle_result = cycle_removal(ir, cycle_strategy);
    push_snapshot(
        &mut trace,
        "cycle_removal",
        ir.nodes.len(),
        ir.edges.len(),
        cycle_result.reversed_edge_indexes.len(),
        0,
    );

    let ranks = rank_assignment(ir, &cycle_result);
    push_snapshot(
        &mut trace,
        "rank_assignment",
        ir.nodes.len(),
        ir.edges.len(),
        cycle_result.reversed_edge_indexes.len(),
        0,
    );

    let (crossing_count, ordering_by_rank) = crossing_minimization(ir, &ranks);
    push_snapshot(
        &mut trace,
        "crossing_minimization",
        ir.nodes.len(),
        ir.edges.len(),
        cycle_result.reversed_edge_indexes.len(),
        crossing_count,
    );

    let nodes = coordinate_assignment(ir, &node_sizes, &ranks, &ordering_by_rank, spacing);
    let edges = build_edge_paths(ir, &nodes, &cycle_result.highlighted_edge_indexes);
    let clusters = build_cluster_boxes(ir, &nodes, spacing);
    let bounds = compute_bounds(&nodes, &clusters, spacing);

    push_snapshot(
        &mut trace,
        "post_processing",
        ir.nodes.len(),
        ir.edges.len(),
        cycle_result.reversed_edge_indexes.len(),
        crossing_count,
    );

    let stats = LayoutStats {
        node_count: ir.nodes.len(),
        edge_count: ir.edges.len(),
        crossing_count,
        reversed_edges: cycle_result.reversed_edge_indexes.len(),
        cycle_count: cycle_result.summary.cycle_count,
        cycle_node_count: cycle_result.summary.cycle_node_count,
        max_cycle_size: cycle_result.summary.max_cycle_size,
        phase_iterations: trace.snapshots.len(),
    };

    TracedLayout {
        layout: DiagramLayout {
            nodes,
            clusters,
            edges,
            bounds,
            stats,
        },
        trace,
    }
}

#[must_use]
pub fn compute_node_sizes(ir: &MermaidDiagramIr) -> Vec<(f32, f32)> {
    ir.nodes
        .iter()
        .map(|node| {
            let (max_len, lines) = label_length_and_lines(ir, node);
            let label_width = (max_len.max(4) as f32) * 8.0;
            let width = label_width.max(72.0);
            let height = 40.0 + ((lines.saturating_sub(1) as f32) * 16.0);
            (width, height)
        })
        .collect()
}

fn label_length_and_lines(ir: &MermaidDiagramIr, node: &fm_core::IrNode) -> (usize, usize) {
    let text = node.label
        .and_then(|label_id| ir.labels.get(label_id.0))
        .map(|value| value.text.as_str())
        .unwrap_or_else(|| node.id.as_str());
        
    let lines = text.lines().count().max(1);
    let max_len = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
    
    (max_len, lines)
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
        CycleStrategy::CycleAware => BTreeSet::new(),
    };

    let highlighted_edge_indexes = if matches!(cycle_strategy, CycleStrategy::CycleAware) {
        dfs_back_edges
    } else {
        reversed_edge_indexes.clone()
    };

    CycleRemovalResult {
        reversed_edge_indexes,
        highlighted_edge_indexes,
        summary: cycle_detection.summary,
    }
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
                .then_with(|| compare_priority(*right, *left, node_priority))
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
        let guard = edges.len().saturating_mul(2).saturating_add(1);
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

    (0..node_count).map(|index| (index, ranks[index])).collect()
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
    (crossing_count, ordering_by_rank)
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

    let mut rank_span = vec![0.0_f32; ordered_ranks.len()];
    for (rank_index, rank) in ordered_ranks.iter().copied().enumerate() {
        let node_indexes = ordering_by_rank
            .get(&rank)
            .cloned()
            .or_else(|| fallback_nodes_by_rank.get(&rank).cloned())
            .unwrap_or_default();

        let mut span = 0.0_f32;
        for node_index in node_indexes {
            let (width, height) = node_sizes.get(node_index).copied().unwrap_or((72.0, 40.0));
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

    let mut output = Vec::with_capacity(ir.nodes.len());
    for (rank, fallback_node_indexes) in fallback_nodes_by_rank {
        let Some(rank_index) = rank_to_index.get(&rank).copied() else {
            continue;
        };

        let node_indexes = ordering_by_rank
            .get(&rank)
            .cloned()
            .unwrap_or(fallback_node_indexes);

        let primary = primary_offsets.get(rank_index).copied().unwrap_or(0.0);
        let mut secondary_cursor = 0.0_f32;
        for (order, node_index) in node_indexes.into_iter().enumerate() {
            let (width, height) = node_sizes.get(node_index).copied().unwrap_or((72.0, 40.0));
            let (x, y) = if horizontal_ranks {
                (primary, secondary_cursor)
            } else {
                (secondary_cursor, primary)
            };
            let node_id = ir
                .nodes
                .get(node_index)
                .map(|node| node.id.clone())
                .unwrap_or_default();

            output.push(LayoutNodeBox {
                node_index,
                node_id,
                rank,
                order,
                bounds: LayoutRect {
                    x,
                    y,
                    width,
                    height,
                },
            });

            let secondary_extent = if horizontal_ranks { height } else { width };
            secondary_cursor += secondary_extent + spacing.node_spacing;
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

    ir.edges
        .iter()
        .enumerate()
        .filter_map(|(edge_index, edge)| {
            let source = endpoint_node_index(ir, edge.from)?;
            let target = endpoint_node_index(ir, edge.to)?;
            let source_box = nodes.get(source)?;
            let target_box = nodes.get(target)?;
            let (source_anchor, target_anchor) =
                edge_anchors(source_box, target_box, horizontal_ranks);
            let points = route_edge_points(source_anchor, target_anchor, horizontal_ranks);

            Some(LayoutEdgePath {
                edge_index,
                points,
                reversed: highlighted_edge_indexes.contains(&edge_index),
            })
        })
        .collect()
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

fn route_edge_points(
    source: LayoutPoint,
    target: LayoutPoint,
    horizontal_ranks: bool,
) -> Vec<LayoutPoint> {
    let epsilon = 0.001_f32;

    let points = if horizontal_ranks {
        if (source.y - target.y).abs() < epsilon {
            vec![source, target]
        } else {
            let mid_x = (source.x + target.x) / 2.0;
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
    } else if (source.x - target.x).abs() < epsilon {
        vec![source, target]
    } else {
        let mid_y = (source.y + target.y) / 2.0;
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
    };

    simplify_polyline(points)
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
            let mut min_x = f32::MAX;
            let mut min_y = f32::MAX;
            let mut max_x = f32::MIN;
            let mut max_y = f32::MIN;

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
    spacing: LayoutSpacing,
) -> LayoutRect {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;

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

#[cfg(test)]
mod tests {
    use super::{
        CycleStrategy, LayoutAlgorithm, LayoutPoint, layout, layout_diagram, layout_diagram_traced,
        layout_diagram_with_cycle_strategy, route_edge_points,
    };
    use fm_core::{
        ArrowType, DiagramType, GraphDirection, IrEdge, IrEndpoint, IrLabel, IrLabelId, IrNode,
        IrNodeId, MermaidDiagramIr,
    };

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
                id: node_id.to_string(),
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
                id: node_id.to_string(),
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
                id: node_id.to_string(),
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
                id: node_id.to_string(),
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
    fn lr_same_rank_nodes_with_different_widths_share_column_position() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::LR;

        for text in [
            "root-one",
            "root-two",
            "narrow",
            "this target label is intentionally much wider",
        ] {
            ir.labels.push(IrLabel {
                text: text.to_string(),
                ..IrLabel::default()
            });
        }

        for (node_id, label_id) in [("R1", 0), ("R2", 1), ("A", 2), ("B", 3)] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                label: Some(IrLabelId(label_id)),
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

        let layout = layout_diagram(&ir);
        let a_node = layout.nodes.iter().find(|node| node.node_id == "A");
        let b_node = layout.nodes.iter().find(|node| node.node_id == "B");
        let (Some(a_node), Some(b_node)) = (a_node, b_node) else {
            panic!("expected A and B nodes in layout");
        };

        assert!((a_node.bounds.x - b_node.bounds.x).abs() < 0.001);
    }
}
