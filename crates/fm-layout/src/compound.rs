//! Compound (cluster-aware) placement for the layered layout.
//!
//! The flat layered pipeline ranks and orders every node of a diagram together and then draws each
//! cluster as the box around wherever its members happened to land. Nothing keeps a cluster's
//! members together or keeps other nodes out from between them, so subgraph boxes overlapped each
//! other and the nodes around them, and a composite state was ranked as a plain node in one place
//! while its contents were laid out somewhere else entirely.
//!
//! This does what dagre's cluster handling does instead. Every top-level cluster is laid out on its
//! own, recursively, so nested clusters get the same treatment; the parent graph is then laid out
//! with each cluster standing in as ONE node of exactly the size its contents need; and the
//! contents are moved into the box the parent gave it. Edges are routed afterwards on the final
//! positions by the caller, as before.

use fm_core::{
    DiagramType, IrCluster, IrClusterId, IrEdge, IrEndpoint, IrGraphNode, IrNode, IrNodeId,
    IrSubgraph, IrSubgraphId, MermaidDiagramIr,
};

use crate::{
    LayoutConfig, LayoutNodeBox, LayoutRect, LayoutSpacing, LayoutTrace, build_cluster_boxes,
    cycle_preparation, endpoint_node_index, flat_placement,
};

/// Node boxes for `ir` placed cluster by cluster, or `None` when the flat pipeline should run.
///
/// Flat placement is kept where compound placement has nothing to add (no cluster with members)
/// and where it would change the meaning of an input: explicit layout constraints name ranks of the
/// WHOLE diagram, and collapsed cycle clusters are their own grouping mechanism.
pub(crate) fn compound_node_boxes(
    ir: &MermaidDiagramIr,
    node_sizes: &[(f32, f32)],
    config: &LayoutConfig,
    spacing: LayoutSpacing,
    metrics: &fm_core::FontMetrics,
) -> Option<Vec<LayoutNodeBox>> {
    if !matches!(
        ir.diagram_type,
        DiagramType::Flowchart
            | DiagramType::State
            | DiagramType::Class
            | DiagramType::C4Context
            | DiagramType::C4Container
            | DiagramType::C4Component
            | DiagramType::C4Dynamic
            | DiagramType::C4Deployment
    ) || !ir.constraints.is_empty()
        || config.collapse_cycle_clusters
        || top_level_clusters(ir).is_empty()
        || subgraph_nesting_exceeds(ir, COMPOUND_MAX_NESTING)
    {
        return None;
    }
    Some(place(ir, node_sizes, config, spacing, metrics))
}

/// The deepest subgraph nesting compound placement recurses into.
///
/// [`place`] recurses once per nesting level and every frame keeps a cloned sub-IR (labels
/// included) alive, so recursion depth, memory and time all followed the input's nesting.
/// Only flowchart and block-beta nesting is capped by the parser; a composite state, C4
/// boundary or class namespace nested a few thousand levels deep (about 15 bytes per level)
/// overflowed the WASM stack. Deeper diagrams take the flat pipeline, which is what every
/// diagram used before compound placement existed.
const COMPOUND_MAX_NESTING: usize = 32;

/// Whether any subgraph sits deeper than `limit` levels. Each parent chain is walked at most
/// `limit` steps, so a malformed (cyclic) chain cannot loop.
fn subgraph_nesting_exceeds(ir: &MermaidDiagramIr, limit: usize) -> bool {
    ir.graph.subgraphs.iter().any(|subgraph| {
        let mut depth = 1;
        let mut parent = subgraph.parent;
        while let Some(id) = parent {
            depth += 1;
            if depth > limit {
                return true;
            }
            parent = ir.graph.subgraph(id).and_then(|next| next.parent);
        }
        false
    })
}

/// The top-level subgraphs that hold at least one node, in declaration order.
fn top_level_clusters(ir: &MermaidDiagramIr) -> Vec<&IrSubgraph> {
    ir.graph
        .subgraphs
        .iter()
        .filter(|subgraph| {
            subgraph.parent.is_none()
                && !ir.graph.subgraph_members_recursive(subgraph.id).is_empty()
        })
        .collect()
}

/// One cluster's own layout, ready to be dropped into the box the parent graph gives it.
struct PlacedCluster {
    /// `ir` node index of each node in `boxes`, in the same order.
    members: Vec<usize>,
    boxes: Vec<LayoutNodeBox>,
    /// The complete cluster rectangle, including nested frames, padding and its title.
    bounds: LayoutRect,
    /// The size the cluster occupies in its parent: the content plus the cluster's padding.
    size: (f32, f32),
}

fn place(
    ir: &MermaidDiagramIr,
    node_sizes: &[(f32, f32)],
    config: &LayoutConfig,
    spacing: LayoutSpacing,
    metrics: &fm_core::FontMetrics,
) -> Vec<LayoutNodeBox> {
    let tops = top_level_clusters(ir);
    if tops.is_empty() {
        return flat_boxes(ir, node_sizes, config, spacing);
    }

    // Which top-level cluster owns each node. A node listed in two clusters stays with the first,
    // which is the one it was declared in.
    let mut owner: Vec<Option<usize>> = vec![None; ir.nodes.len()];
    for (top, subgraph) in tops.iter().enumerate() {
        for member in ir.graph.subgraph_members_recursive(subgraph.id) {
            if let Some(slot) = owner.get_mut(member.0)
                && slot.is_none()
            {
                *slot = Some(top);
            }
        }
    }

    let placed: Vec<PlacedCluster> = tops
        .iter()
        .enumerate()
        .map(|(top, subgraph)| {
            let (sub, members) = cluster_sub_ir(ir, subgraph, &owner, top);
            let sub_sizes: Vec<(f32, f32)> =
                members.iter().map(|&member| node_sizes[member]).collect();
            let boxes = place(&sub, &sub_sizes, config, spacing, metrics);
            let content = bounding_box(boxes.iter().map(|node| node.bounds));
            let mut bounds = (
                content.x,
                content.y,
                content.x + content.width,
                content.y + content.height,
            );
            for nested in build_cluster_boxes(&sub, &boxes, spacing, metrics) {
                crate::extend_cluster_bounds_for_child(
                    &mut bounds,
                    nested.bounds,
                    spacing.cluster_padding,
                );
            }
            let title = subgraph
                .title
                .and_then(|label| ir.labels.get(label.0))
                .map(|label| label.text.as_str());
            let bounds = crate::cluster_bounds_with_title(bounds, title, spacing, metrics);
            let size = (bounds.width, bounds.height);
            PlacedCluster {
                members,
                boxes,
                bounds,
                size,
            }
        })
        .collect();

    // A composite state's own node IS its cluster (`anchor_composite_state_nodes`), so it stands
    // in for the cluster in the parent; every other cluster gets a stand-in of its own.
    let composite: Vec<Option<usize>> = tops
        .iter()
        .map(|subgraph| composite_state_node(ir, subgraph, &owner))
        .collect();

    let parent = parent_ir(ir, &owner, &tops, &composite);
    let mut parent_sizes: Vec<(f32, f32)> = parent
        .source
        .iter()
        .map(|source| match *source {
            Source::Node(node) => node_sizes[node],
            Source::Cluster(top) => placed[top].size,
        })
        .collect();
    for (top, stand_in) in parent.stand_in.iter().enumerate() {
        parent_sizes[*stand_in] = placed[top].size;
    }
    let parent_boxes = flat_boxes(&parent.ir, &parent_sizes, config, spacing);

    let mut out: Vec<Option<LayoutNodeBox>> = vec![None; ir.nodes.len()];
    for (index, source) in parent.source.iter().enumerate() {
        if let Source::Node(node) = *source {
            let mut node_box = parent_boxes[index].clone();
            node_box.node_index = node;
            out[node] = Some(node_box);
        }
    }
    for (top, cluster) in placed.iter().enumerate() {
        let slot = &parent_boxes[parent.stand_in[top]];
        // Allocate and reconstruct the same rectangle, including title-dominated widths and
        // asymmetric header space. Centring raw contents would spend half the header below them.
        let dx = slot.bounds.x - cluster.bounds.x;
        let dy = slot.bounds.y - cluster.bounds.y;
        for (member, member_box) in cluster.members.iter().zip(&cluster.boxes) {
            let mut node_box = member_box.clone();
            node_box.node_index = *member;
            node_box.bounds.x += dx;
            node_box.bounds.y += dy;
            node_box.rank += slot.rank;
            out[*member] = Some(node_box);
        }
    }

    out.into_iter()
        .enumerate()
        .map(|(index, node_box)| {
            node_box.unwrap_or_else(|| LayoutNodeBox {
                node_index: index,
                node_id: ir.nodes[index].id.clone(),
                rank: 0,
                order: 0,
                span: ir.nodes[index].span_primary,
                bounds: LayoutRect {
                    x: 0.0,
                    y: 0.0,
                    width: node_sizes[index].0,
                    height: node_sizes[index].1,
                },
            })
        })
        .collect()
}

/// The flat layered placement, through the same functions the full pipeline uses.
fn flat_boxes(
    ir: &MermaidDiagramIr,
    node_sizes: &[(f32, f32)],
    config: &LayoutConfig,
    spacing: LayoutSpacing,
) -> Vec<LayoutNodeBox> {
    let (node_priority, cycle_result) = cycle_preparation(ir, config);
    let (_, _, nodes) = flat_placement(
        ir,
        node_sizes,
        config,
        spacing,
        &cycle_result,
        &node_priority,
        &mut LayoutTrace::default(),
    );
    nodes
}

/// The node a composite state's cluster belongs to: the state declared with the cluster's title,
/// when it is not itself inside the cluster.
fn composite_state_node(
    ir: &MermaidDiagramIr,
    subgraph: &IrSubgraph,
    owner: &[Option<usize>],
) -> Option<usize> {
    if ir.diagram_type != DiagramType::State {
        return None;
    }
    let title = subgraph
        .cluster
        .and_then(|cluster| ir.clusters.get(cluster.0))
        .and_then(|cluster| cluster.title)
        .and_then(|label| ir.labels.get(label.0))?;
    ir.nodes
        .iter()
        .position(|node| node.id == title.text)
        .filter(|&node| owner[node].is_none())
}

/// A copy of `ir` restricted to one top-level cluster's members, with that cluster's nested
/// clusters, and the `ir` node index of each of its nodes.
fn cluster_sub_ir(
    ir: &MermaidDiagramIr,
    subgraph: &IrSubgraph,
    owner: &[Option<usize>],
    top: usize,
) -> (MermaidDiagramIr, Vec<usize>) {
    let members: Vec<usize> = (0..ir.nodes.len())
        .filter(|&node| owner[node] == Some(top))
        .collect();
    let mut new_index = vec![usize::MAX; ir.nodes.len()];
    for (new, &old) in members.iter().enumerate() {
        new_index[old] = new;
    }

    // Nested subgraphs: every descendant of `subgraph`, renumbered in declaration order, which is
    // the order `ir.graph.subgraphs` holds them in. Index maps are plain vectors, `usize::MAX`
    // marking "not in this cluster": sorted containers here cost kilobytes of wasm for a few
    // dozen entries.
    let is_descendant = |candidate: &IrSubgraph| {
        let mut parent = candidate.parent;
        while let Some(id) = parent {
            if id == subgraph.id {
                return true;
            }
            parent = ir.graph.subgraph(id).and_then(|found| found.parent);
        }
        false
    };
    let descendants: Vec<&IrSubgraph> = ir
        .graph
        .subgraphs
        .iter()
        .filter(|candidate| is_descendant(candidate))
        .collect();
    let mut subgraph_index = vec![usize::MAX; ir.graph.subgraphs.len()];
    let mut cluster_index = vec![usize::MAX; ir.clusters.len()];
    let mut cluster_order: Vec<usize> = Vec::new();
    for (new, found) in descendants.iter().enumerate() {
        subgraph_index[found.id.0] = new;
        if let Some(cluster) = found.cluster
            && cluster_index.get(cluster.0) == Some(&usize::MAX)
        {
            cluster_index[cluster.0] = cluster_order.len();
            cluster_order.push(cluster.0);
        }
    }
    let lookup = |table: &[usize], old: usize| table.get(old).copied().filter(|&n| n != usize::MAX);
    let remap_members = |list: &[IrNodeId]| -> Vec<IrNodeId> {
        list.iter()
            .filter_map(|node| lookup(&new_index, node.0).map(IrNodeId))
            .collect()
    };

    let mut sub = MermaidDiagramIr::empty(ir.diagram_type);
    sub.direction = subgraph.direction.unwrap_or(ir.direction);
    sub.labels.clone_from(&ir.labels);
    copy_layout_meta(&mut sub, ir);
    sub.meta.direction = sub.direction;
    sub.nodes = members.iter().map(|&old| ir.nodes[old].clone()).collect();
    sub.graph.nodes = members
        .iter()
        .enumerate()
        .map(|(new, &old)| {
            let original = ir.graph.nodes.get(old);
            IrGraphNode {
                node_id: IrNodeId(new),
                kind: original.map(|node| node.kind).unwrap_or_default(),
                clusters: original
                    .map(|node| {
                        node.clusters
                            .iter()
                            .filter_map(|id| lookup(&cluster_index, id.0).map(IrClusterId))
                            .collect()
                    })
                    .unwrap_or_default(),
                subgraphs: original
                    .map(|node| {
                        node.subgraphs
                            .iter()
                            .filter_map(|id| lookup(&subgraph_index, id.0).map(IrSubgraphId))
                            .collect()
                    })
                    .unwrap_or_default(),
            }
        })
        .collect();
    sub.edges = ir
        .edges
        .iter()
        .filter_map(|edge| {
            let from = new_index[endpoint_node_index(ir, edge.from)?];
            let to = new_index[endpoint_node_index(ir, edge.to)?];
            (from != usize::MAX && to != usize::MAX).then(|| IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                ..edge.clone()
            })
        })
        .collect();
    sub.graph.subgraphs = descendants
        .iter()
        .map(|&original| IrSubgraph {
            id: IrSubgraphId(subgraph_index[original.id.0]),
            parent: original
                .parent
                .and_then(|parent| lookup(&subgraph_index, parent.0).map(IrSubgraphId)),
            children: original
                .children
                .iter()
                .filter_map(|child| lookup(&subgraph_index, child.0).map(IrSubgraphId))
                .collect(),
            members: remap_members(&original.members),
            cluster: original
                .cluster
                .and_then(|cluster| lookup(&cluster_index, cluster.0).map(IrClusterId)),
            ..original.clone()
        })
        .collect();
    sub.clusters = cluster_order
        .iter()
        .enumerate()
        .map(|(new, &old)| IrCluster {
            id: IrClusterId(new),
            members: remap_members(&ir.clusters[old].members),
            ..ir.clusters[old].clone()
        })
        .collect();
    (sub, members)
}

/// The meta fields the layout reads: spacing and routing hints and the direction. Copying these
/// rather than the whole meta keeps the theme and init blocks out of every sub-diagram.
fn copy_layout_meta(into: &mut MermaidDiagramIr, from: &MermaidDiagramIr) {
    into.meta.direction = from.meta.direction;
    into.meta.node_spacing = from.meta.node_spacing;
    into.meta.rank_spacing = from.meta.rank_spacing;
    into.meta.edge_routing = from.meta.edge_routing;
}

/// Where a node of the parent graph came from.
#[derive(Clone, Copy)]
enum Source {
    Node(usize),
    Cluster(usize),
}

struct ParentGraph {
    ir: MermaidDiagramIr,
    /// For each parent node, the `ir` node or top-level cluster it stands for.
    source: Vec<Source>,
    /// For each top-level cluster, the parent node standing in for it.
    stand_in: Vec<usize>,
}

/// The diagram with every top-level cluster collapsed to one node. Edges keep their endpoints,
/// mapped onto the stand-in where they reach into a cluster; edges inside one cluster are dropped,
/// because that cluster's own layout already placed them.
fn parent_ir(
    ir: &MermaidDiagramIr,
    owner: &[Option<usize>],
    tops: &[&IrSubgraph],
    composite: &[Option<usize>],
) -> ParentGraph {
    let mut parent = MermaidDiagramIr::empty(ir.diagram_type);
    parent.direction = ir.direction;
    parent.labels.clone_from(&ir.labels);
    copy_layout_meta(&mut parent, ir);

    let mut source = Vec::new();
    let mut index_of_node = vec![usize::MAX; ir.nodes.len()];
    for (node, node_owner) in owner.iter().enumerate() {
        if node_owner.is_none() {
            index_of_node[node] = source.len();
            source.push(Source::Node(node));
            parent.nodes.push(ir.nodes[node].clone());
        }
    }
    let stand_in: Vec<usize> = tops
        .iter()
        .enumerate()
        .map(|(top, subgraph)| {
            if let Some(node) = composite[top] {
                return index_of_node[node];
            }
            let index = source.len();
            source.push(Source::Cluster(top));
            parent.nodes.push(IrNode {
                id: format!("\u{0}cluster:{}", subgraph.key),
                ..IrNode::default()
            });
            index
        })
        .collect();
    parent.graph.nodes = (0..parent.nodes.len())
        .map(|index| IrGraphNode {
            node_id: IrNodeId(index),
            ..IrGraphNode::default()
        })
        .collect();

    let place_of = |node: usize| match owner[node] {
        Some(top) => stand_in[top],
        None => index_of_node[node],
    };
    parent.edges = ir
        .edges
        .iter()
        .filter_map(|edge| {
            let from = place_of(endpoint_node_index(ir, edge.from)?);
            let to = place_of(endpoint_node_index(ir, edge.to)?);
            let inside_one_cluster = from == to
                && matches!(source[from], Source::Cluster(_))
                && edge_stays_inside(ir, edge, owner);
            (!inside_one_cluster).then(|| IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                ..edge.clone()
            })
        })
        .collect();
    ParentGraph {
        ir: parent,
        source,
        stand_in,
    }
}

/// Both ends of `edge` are members of the same top-level cluster.
fn edge_stays_inside(ir: &MermaidDiagramIr, edge: &IrEdge, owner: &[Option<usize>]) -> bool {
    match (
        endpoint_node_index(ir, edge.from),
        endpoint_node_index(ir, edge.to),
    ) {
        (Some(from), Some(to)) => owner[from].is_some() && owner[from] == owner[to],
        _ => false,
    }
}

fn bounding_box(mut rects: impl Iterator<Item = LayoutRect>) -> LayoutRect {
    let Some(first) = rects.next() else {
        return LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        };
    };
    rects.fold(first, union)
}

fn union(a: LayoutRect, b: LayoutRect) -> LayoutRect {
    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
    LayoutRect {
        x,
        y,
        width: (a.x + a.width).max(b.x + b.width) - x,
        height: (a.y + a.height).max(b.y + b.height) - y,
    }
}

#[cfg(test)]
mod tests {
    use super::{Source, compound_node_boxes, flat_boxes, parent_ir, top_level_clusters};
    use crate::{LayoutAlgorithm, LayoutConfig, LayoutRect, LayoutSpacing, build_cluster_boxes};
    use fm_core::{FontMetrics, FontMetricsConfig};

    #[test]
    fn cluster_titles_reserve_scaled_header_and_semibold_width() {
        for font_size in [15.0_f32, 30.0, 60.0] {
            for lines in [1, 2] {
                let mut ir = fm_parser::parse(
                    "flowchart TD\nsubgraph X[\"Very long title\"]\nA[Alpha]\nend",
                )
                .ir;
                let title_id = ir.clusters[0].title.expect("cluster title");
                if lines == 2 {
                    ir.labels[title_id.0].text.push_str("\nVery long title");
                }
                let metrics = FontMetrics::new(FontMetricsConfig {
                    font_size,
                    ..Default::default()
                });
                let traced = crate::layout_diagram_traced_with_config(
                    &ir,
                    LayoutAlgorithm::Sugiyama,
                    LayoutConfig {
                        font_metrics: Some(metrics),
                        fnx_enabled: false,
                        ..Default::default()
                    },
                );
                let cluster = &traced.layout.clusters[0];
                let node = &traced.layout.nodes[0];
                assert_eq!(
                    cluster.title.as_deref(),
                    Some(ir.labels[title_id.0].text.as_str())
                );
                // Independent browser measurement: at a 54px semibold title font, this exact
                // string draws 432.875px wide. The old layout reserved only 326.7px plus padding.
                let drawn_width = 432.875 * (font_size / 60.0);
                assert!(
                    cluster.bounds.width + 0.01 >= drawn_width + 16.0,
                    "title needs {drawn_width}px at font {font_size}: {:?}",
                    cluster.bounds
                );
                let title_font = font_size * 0.9;
                let last_baseline =
                    (title_font + 2.0).max(16.0) + (lines - 1) as f32 * title_font * 1.4;
                let header = node.bounds.y - cluster.bounds.y;
                assert!(
                    header + 0.01 >= last_baseline + title_font * 0.3 + 8.0,
                    "the title descenders must clear the node fill: header={header}, font={font_size}"
                );
                assert!(
                    (node.bounds.center().x - cluster.bounds.center().x).abs() < 0.01,
                    "title-dominated width must stay centred on its contents"
                );
                let bottom_padding =
                    cluster.bounds.y + cluster.bounds.height - node.bounds.y - node.bounds.height;
                assert!((bottom_padding - LayoutSpacing::default().cluster_padding).abs() < 0.01);
                if font_size == 15.0 && lines == 1 {
                    let padding = LayoutSpacing::default().cluster_padding;
                    assert_eq!(header, padding);
                    assert_eq!(cluster.bounds.width, node.bounds.width + 2.0 * padding);
                    assert_eq!(cluster.bounds.height, node.bounds.height + 2.0 * padding);
                }
            }
        }
    }

    fn assert_same_rect(actual: LayoutRect, expected: LayoutRect) {
        assert!(
            (actual.x - expected.x).abs() < 0.01
                && (actual.y - expected.y).abs() < 0.01
                && (actual.width - expected.width).abs() < 0.01
                && (actual.height - expected.height).abs() < 0.01,
            "reconstructed cluster {actual:?} must match its allocated slot {expected:?}"
        );
    }

    #[test]
    fn title_dominated_and_nested_clusters_match_parent_slots() {
        for font_size in [15.0, 60.0] {
            for direction in ["LR", "RL", "TD", "BT"] {
                for nested in [false, true] {
                    let inside = if nested {
                        "subgraph Inner[\"Nested title\"]\nA[Alpha]\nend"
                    } else {
                        "A[Alpha]"
                    };
                    let title = "Wm".repeat(20);
                    let ir = fm_parser::parse(&format!(
                        "flowchart {direction}\nsubgraph X[\"{title}\"]\n{inside}\nend\n\
                         subgraph Y[\"Another very long title\"]\nB[Beta]\nend\nA-->B"
                    ))
                    .ir;
                    let metrics = FontMetrics::new(FontMetricsConfig {
                        font_size,
                        ..Default::default()
                    });
                    let config = LayoutConfig {
                        font_metrics: Some(metrics.clone()),
                        fnx_enabled: false,
                        ..Default::default()
                    };
                    let spacing = LayoutSpacing::default();
                    let node_sizes = crate::compute_node_sizes(&ir, &metrics);
                    let nodes = compound_node_boxes(&ir, &node_sizes, &config, spacing, &metrics)
                        .expect("compound placement");
                    let clusters = build_cluster_boxes(&ir, &nodes, spacing, &metrics);
                    assert_eq!(clusters.len(), if nested { 3 } else { 2 });
                    let tops = top_level_clusters(&ir);
                    let mut owner = vec![None; ir.nodes.len()];
                    for (index, top) in tops.iter().enumerate() {
                        for member in ir.graph.subgraph_members_recursive(top.id) {
                            owner[member.0] = Some(index);
                        }
                    }
                    // Re-layout the top-level graph using the actual reconstructed frame sizes.
                    // If allocation and reconstruction disagree, the original boxes escape these
                    // slots even though their contents may still be inside their own rectangles.
                    let parent = parent_ir(&ir, &owner, &tops, &vec![None; tops.len()]);
                    let cluster_bounds = |top: usize| {
                        let index = tops[top].cluster.expect("cluster id").0;
                        clusters
                            .iter()
                            .find(|cluster| cluster.cluster_index == index)
                            .unwrap()
                            .bounds
                    };
                    let sizes: Vec<_> = parent
                        .source
                        .iter()
                        .map(|source| match *source {
                            Source::Node(index) => node_sizes[index],
                            Source::Cluster(top) => {
                                let bounds = cluster_bounds(top);
                                (bounds.width, bounds.height)
                            }
                        })
                        .collect();
                    let slots = flat_boxes(&parent.ir, &sizes, &config, spacing);
                    for (top, &stand_in) in parent.stand_in.iter().enumerate() {
                        assert_same_rect(cluster_bounds(top), slots[stand_in].bounds);
                    }
                    if nested {
                        let outer = clusters
                            .iter()
                            .find(|cluster| cluster.title.as_deref() == Some(title.as_str()))
                            .unwrap();
                        let inner = clusters
                            .iter()
                            .find(|cluster| cluster.title.as_deref() == Some("Nested title"))
                            .unwrap();
                        assert!(inner.bounds.x >= outer.bounds.x + 15.99);
                        assert!(
                            inner.bounds.x + inner.bounds.width
                                <= outer.bounds.x + outer.bounds.width - 15.99
                        );
                        let title_font = font_size * 0.9;
                        assert!(
                            inner.bounds.y
                                >= outer.bounds.y
                                    + (title_font + 2.0).max(16.0)
                                    + title_font * 0.3
                                    + 8.0
                                    - 0.01
                        );
                        assert!(
                            inner.bounds.y + inner.bounds.height
                                <= outer.bounds.y + outer.bounds.height - 15.99
                        );
                    }
                }
            }
        }
    }
}
