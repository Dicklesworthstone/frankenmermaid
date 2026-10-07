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
    {
        return None;
    }
    Some(place(ir, node_sizes, config, spacing, metrics))
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
    /// The extent of the contents, nested cluster boxes included.
    content: LayoutRect,
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
            let mut content = bounding_box(boxes.iter().map(|node| node.bounds));
            for nested in build_cluster_boxes(&sub, &boxes, spacing, metrics) {
                content = union(content, nested.bounds);
            }
            let title_width = subgraph
                .title
                .and_then(|label| ir.labels.get(label.0))
                .map_or(0.0, |label| {
                    crate::cluster_title_width(&label.text, metrics)
                });
            let size = (
                (2.0f32.mul_add(spacing.cluster_padding, content.width)).max(title_width),
                2.0f32.mul_add(spacing.cluster_padding, content.height),
            );
            PlacedCluster {
                members,
                boxes,
                content,
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
        // Centre the contents in the slot: the slot may be wider than the contents for a title.
        let dx = (slot.bounds.width - cluster.content.width)
            .mul_add(0.5, slot.bounds.x - cluster.content.x);
        let dy = (slot.bounds.height - cluster.content.height)
            .mul_add(0.5, slot.bounds.y - cluster.content.y);
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
