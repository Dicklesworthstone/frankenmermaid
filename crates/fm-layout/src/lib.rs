#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use fm_core::{IrEndpoint, IrLabelId, MermaidDiagramIr};

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
pub struct LayoutStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub crossing_count: usize,
    pub reversed_edges: usize,
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
pub fn layout_diagram_traced(ir: &MermaidDiagramIr) -> TracedLayout {
    let mut trace = LayoutTrace::default();
    let spacing = LayoutSpacing::default();
    let node_sizes = compute_node_sizes(ir);
    let cycle_result = cycle_removal(ir);
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

    let crossing_count = crossing_minimization(ir, &ranks);
    push_snapshot(
        &mut trace,
        "crossing_minimization",
        ir.nodes.len(),
        ir.edges.len(),
        cycle_result.reversed_edge_indexes.len(),
        crossing_count,
    );

    let nodes = coordinate_assignment(ir, &node_sizes, &ranks, spacing);
    let edges = build_edge_paths(ir, &nodes, &cycle_result.reversed_edge_indexes);
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
            let label_len = label_length(ir, node.label);
            let label_width = (label_len.max(4) as f32) * 8.0;
            let width = label_width.max(72.0);
            let height = 40.0;
            (width, height)
        })
        .collect()
}

fn label_length(ir: &MermaidDiagramIr, label: Option<IrLabelId>) -> usize {
    label
        .and_then(|label_id| ir.labels.get(label_id.0))
        .map(|value| value.text.chars().count())
        .unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CycleRemovalResult {
    reversed_edge_indexes: BTreeSet<usize>,
}

fn cycle_removal(ir: &MermaidDiagramIr) -> CycleRemovalResult {
    let reversed_edge_indexes = ir
        .edges
        .iter()
        .enumerate()
        .filter_map(|(index, edge)| {
            let source = endpoint_node_index(ir, edge.from)?;
            let target = endpoint_node_index(ir, edge.to)?;
            (source > target).then_some(index)
        })
        .collect();

    CycleRemovalResult {
        reversed_edge_indexes,
    }
}

fn rank_assignment(ir: &MermaidDiagramIr, cycles: &CycleRemovalResult) -> BTreeMap<usize, usize> {
    let mut ranks = BTreeMap::new();
    for index in 0..ir.nodes.len() {
        ranks.insert(index, 0_usize);
    }

    let mut changed = true;
    let mut guard = 0_usize;
    while changed && guard < ir.edges.len().saturating_mul(2).saturating_add(1) {
        changed = false;
        guard = guard.saturating_add(1);

        for (edge_index, edge) in ir.edges.iter().enumerate() {
            let Some(mut source) = endpoint_node_index(ir, edge.from) else {
                continue;
            };
            let Some(mut target) = endpoint_node_index(ir, edge.to) else {
                continue;
            };
            if cycles.reversed_edge_indexes.contains(&edge_index) {
                std::mem::swap(&mut source, &mut target);
            }

            let source_rank = ranks.get(&source).copied().unwrap_or(0);
            let candidate_rank = source_rank.saturating_add(1);
            let target_rank = ranks.get(&target).copied().unwrap_or(0);
            if candidate_rank > target_rank {
                ranks.insert(target, candidate_rank);
                changed = true;
            }
        }
    }

    ranks
}

fn crossing_minimization(_ir: &MermaidDiagramIr, _ranks: &BTreeMap<usize, usize>) -> usize {
    0
}

fn coordinate_assignment(
    ir: &MermaidDiagramIr,
    node_sizes: &[(f32, f32)],
    ranks: &BTreeMap<usize, usize>,
    spacing: LayoutSpacing,
) -> Vec<LayoutNodeBox> {
    let mut nodes_by_rank: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for node_index in 0..ir.nodes.len() {
        let rank = ranks.get(&node_index).copied().unwrap_or(0);
        nodes_by_rank.entry(rank).or_default().push(node_index);
    }

    let mut output = Vec::with_capacity(ir.nodes.len());
    for (rank, node_indexes) in nodes_by_rank {
        for (order, node_index) in node_indexes.into_iter().enumerate() {
            let (width, height) = node_sizes.get(node_index).copied().unwrap_or((72.0, 40.0));
            let x = (order as f32) * (spacing.node_spacing + width);
            let y = (rank as f32) * (spacing.rank_spacing + height);
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
        }
    }

    output.sort_by_key(|node| node.node_index);
    output
}

fn build_edge_paths(
    ir: &MermaidDiagramIr,
    nodes: &[LayoutNodeBox],
    reversed_edge_indexes: &BTreeSet<usize>,
) -> Vec<LayoutEdgePath> {
    ir.edges
        .iter()
        .enumerate()
        .filter_map(|(edge_index, edge)| {
            let source = endpoint_node_index(ir, edge.from)?;
            let target = endpoint_node_index(ir, edge.to)?;
            let source_box = nodes.get(source)?;
            let target_box = nodes.get(target)?;

            Some(LayoutEdgePath {
                edge_index,
                points: vec![source_box.bounds.center(), target_box.bounds.center()],
                reversed: reversed_edge_indexes.contains(&edge_index),
            })
        })
        .collect()
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
    use super::{LayoutAlgorithm, layout, layout_diagram, layout_diagram_traced};
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
}
