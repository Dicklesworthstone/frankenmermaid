//! Bidirectional layout editing for flowchart node ordering.
//!
//! Mermaid source has no coordinate syntax.  A host may still let a reader drag a node within a
//! rank, but that edit must become a deterministic IR ordering change rather than a private pixel
//! override that disappears on the next layout.  [`LayoutLens`] carries the original rank/order
//! complement and makes that one safe putback explicit.  Moving across ranks or changing topology
//! is rejected: guessing a new edge direction from a pixel drag would silently rewrite meaning.

use std::collections::{BTreeMap, BTreeSet};

use fm_core::{DiagramType, GraphDirection, IrEndpoint, IrNodeId, IrStyleTarget, MermaidDiagramIr};

use crate::{
    DiagramLayout, LayoutConfig, LayoutPoint, LayoutRect, layout_diagram_traced_with_config,
};

const RANK_AXIS_EPSILON: f32 = 0.001;

/// The layout coordinate axis that represents a rank change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutLensAxis {
    Horizontal,
    Vertical,
}

/// One node as a host can position it for a layout edit.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutLensNode {
    pub node_id: String,
    pub rank: usize,
    pub order: usize,
    pub center: LayoutPoint,
}

/// The positioned graph a host edits before giving it back to [`LayoutLens::put`].
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutLensSnapshot {
    pub nodes: Vec<LayoutLensNode>,
    pub bounds: LayoutRect,
}

/// The ordering decisions the layout made but Mermaid source does not spell out.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutComplement {
    pub rank_axis: LayoutLensAxis,
    /// Node IDs in their rendered order for every rank, indexed by rank number.
    pub rank_orders: Vec<Vec<String>>,
    pub bounds: LayoutRect,
}

/// The failure modes deliberately kept distinct so a UI can tell an unsupported edit from a
/// malformed one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutLensError {
    NotFlowchart,
    NodeSetChanged,
    DuplicateNodeId(String),
    InvalidRank {
        node_id: String,
        rank: usize,
        node_count: usize,
    },
    DuplicateOrder {
        rank: usize,
        order: usize,
    },
    RankChanged {
        node_id: String,
        expected: usize,
        actual: usize,
    },
    RankAxisMoved(String),
    NonFinitePosition(String),
    InvalidNodeReference(usize),
    InvalidPortReference(usize),
}

impl std::fmt::Display for LayoutLensError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFlowchart => formatter.write_str(
                "LayoutLens currently supports flowcharts; other diagram families have no safe ordering putback yet",
            ),
            Self::NodeSetChanged => formatter.write_str(
                "LayoutLens only reorders existing nodes within a rank; node additions and removals are unsupported",
            ),
            Self::DuplicateNodeId(node_id) => {
                write!(formatter, "LayoutLens requires unique node IDs; '{node_id}' appears more than once")
            }
            Self::InvalidRank {
                node_id,
                rank,
                node_count,
            } => write!(
                formatter,
                "node '{node_id}' has rank {rank}, outside a {node_count}-node layout"
            ),
            Self::DuplicateOrder { rank, order } => write!(
                formatter,
                "LayoutLens requires unique order values within each rank; rank {rank} repeats order {order}"
            ),
            Self::RankChanged {
                node_id,
                expected,
                actual,
            } => write!(
                formatter,
                "node '{node_id}' moved from rank {expected} to rank {actual}; cross-rank edits require an explicit topology edit"
            ),
            Self::RankAxisMoved(node_id) => write!(
                formatter,
                "node '{node_id}' moved along the rank axis; cross-rank edits require an explicit topology edit"
            ),
            Self::NonFinitePosition(node_id) => {
                write!(formatter, "node '{node_id}' has a non-finite layout position")
            }
            Self::InvalidNodeReference(index) => write!(
                formatter,
                "LayoutLens cannot reorder an IR with an invalid node reference {index}"
            ),
            Self::InvalidPortReference(index) => write!(
                formatter,
                "LayoutLens cannot reorder an IR with an invalid port reference {index}"
            ),
        }
    }
}

impl std::error::Error for LayoutLensError {}

/// A flowchart IR paired with its layout-order complement.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutLens {
    original: MermaidDiagramIr,
    snapshot: LayoutLensSnapshot,
    complement: LayoutComplement,
}

impl LayoutLens {
    /// Lay out `ir` and retain the ordering choices needed for a safe in-rank drag putback.
    pub fn new(ir: &MermaidDiagramIr, config: LayoutConfig) -> Result<Self, LayoutLensError> {
        if ir.diagram_type != DiagramType::Flowchart {
            return Err(LayoutLensError::NotFlowchart);
        }
        let layout =
            layout_diagram_traced_with_config(ir, crate::LayoutAlgorithm::Auto, config).layout;
        Self::from_layout(ir, &layout)
    }

    /// Build the lens from a layout a caller already computed.
    pub fn from_layout(
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
    ) -> Result<Self, LayoutLensError> {
        if ir.diagram_type != DiagramType::Flowchart {
            return Err(LayoutLensError::NotFlowchart);
        }

        if layout.nodes.len() != ir.nodes.len() {
            return Err(LayoutLensError::NodeSetChanged);
        }

        let mut ir_node_ids = BTreeSet::new();
        for node in &ir.nodes {
            if !ir_node_ids.insert(node.id.as_str()) {
                return Err(LayoutLensError::DuplicateNodeId(node.id.clone()));
            }
        }

        let mut seen = BTreeSet::new();
        for node in &layout.nodes {
            if !seen.insert(node.node_id.as_str()) {
                return Err(LayoutLensError::DuplicateNodeId(node.node_id.clone()));
            }
            if ir
                .nodes
                .get(node.node_index)
                .is_none_or(|ir_node| ir_node.id != node.node_id)
            {
                return Err(LayoutLensError::NodeSetChanged);
            }
            if node.rank >= layout.nodes.len() {
                return Err(LayoutLensError::InvalidRank {
                    node_id: node.node_id.clone(),
                    rank: node.rank,
                    node_count: layout.nodes.len(),
                });
            }
            let center = node.bounds.center();
            if !center.x.is_finite() || !center.y.is_finite() {
                return Err(LayoutLensError::NonFinitePosition(node.node_id.clone()));
            }
        }
        if seen != ir_node_ids {
            return Err(LayoutLensError::NodeSetChanged);
        }

        // Rank validity is established before sizing this vector. A caller-supplied layout with a
        // rank near `usize::MAX` used to overflow `rank + 1` (or request an impossible allocation)
        // before `from_layout` had a chance to reject it.
        let rank_axis = rank_axis(ir.direction);
        let mut rank_orders = vec![
            Vec::new();
            layout
                .nodes
                .iter()
                .map(|node| node.rank)
                .max()
                .map_or(0, |rank| rank + 1)
        ];
        let mut nodes = Vec::with_capacity(layout.nodes.len());
        for node in &layout.nodes {
            rank_orders[node.rank].push((node.order, node.node_id.clone()));
            nodes.push(LayoutLensNode {
                node_id: node.node_id.clone(),
                rank: node.rank,
                order: node.order,
                center: node.bounds.center(),
            });
        }
        for (rank, order) in rank_orders.iter_mut().enumerate() {
            order.sort_unstable_by_key(|(node_order, _)| *node_order);
            if let Some(duplicate) = order.windows(2).find(|pair| pair[0].0 == pair[1].0) {
                return Err(LayoutLensError::DuplicateOrder {
                    rank,
                    order: duplicate[0].0,
                });
            }
        }
        let rank_orders = rank_orders
            .into_iter()
            .map(|order| order.into_iter().map(|(_, node_id)| node_id).collect())
            .collect();

        Ok(Self {
            original: ir.clone(),
            snapshot: LayoutLensSnapshot {
                nodes,
                bounds: layout.bounds,
            },
            complement: LayoutComplement {
                rank_axis,
                rank_orders,
                bounds: layout.bounds,
            },
        })
    }

    #[must_use]
    pub fn get(&self) -> &LayoutLensSnapshot {
        &self.snapshot
    }

    #[must_use]
    pub fn complement(&self) -> &LayoutComplement {
        &self.complement
    }

    /// Convert a safe within-rank drag to an IR declaration-order update.
    ///
    /// Only ranks whose visual order changed are written back. Other ranks retain their exact
    /// declaration order, which need not match the layout algorithm's chosen visual order.
    ///
    /// `IrNodeId` is a vector index, not an authored ID. Every flowchart node reference, including
    /// the graph mirror, ports, memberships and style targets, is remapped with the permutation so
    /// a drag cannot reconnect an edge or move another node's metadata onto the dragged node.
    pub fn put(&self, edited: &LayoutLensSnapshot) -> Result<MermaidDiagramIr, LayoutLensError> {
        if edited.nodes.len() != self.snapshot.nodes.len() {
            return Err(LayoutLensError::NodeSetChanged);
        }

        let original_by_id: BTreeMap<&str, &LayoutLensNode> = self
            .snapshot
            .nodes
            .iter()
            .map(|node| (node.node_id.as_str(), node))
            .collect();
        let mut edited_by_rank: BTreeMap<usize, Vec<&LayoutLensNode>> = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for node in &edited.nodes {
            if !seen.insert(node.node_id.as_str()) {
                return Err(LayoutLensError::DuplicateNodeId(node.node_id.clone()));
            }
            let Some(original) = original_by_id.get(node.node_id.as_str()) else {
                return Err(LayoutLensError::NodeSetChanged);
            };
            if node.rank != original.rank {
                return Err(LayoutLensError::RankChanged {
                    node_id: node.node_id.clone(),
                    expected: original.rank,
                    actual: node.rank,
                });
            }
            if !node.center.x.is_finite() || !node.center.y.is_finite() {
                return Err(LayoutLensError::NonFinitePosition(node.node_id.clone()));
            }
            if (rank_coordinate(self.complement.rank_axis, node.center)
                - rank_coordinate(self.complement.rank_axis, original.center))
            .abs()
                > RANK_AXIS_EPSILON
            {
                return Err(LayoutLensError::RankAxisMoved(node.node_id.clone()));
            }
            edited_by_rank.entry(node.rank).or_default().push(node);
        }
        if seen.len() != original_by_id.len() {
            return Err(LayoutLensError::NodeSetChanged);
        }

        let source_indices: BTreeMap<&str, usize> = self
            .original
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id.as_str(), index))
            .collect();
        let mut new_to_old: Vec<usize> = (0..self.original.nodes.len()).collect();
        let compare = |left: &&LayoutLensNode, right: &&LayoutLensNode| {
            rank_secondary(self.complement.rank_axis, left.center)
                .total_cmp(&rank_secondary(self.complement.rank_axis, right.center))
                // Ties retain the captured order, not lexicographic ID order or caller Vec order.
                .then_with(|| {
                    original_by_id[left.node_id.as_str()]
                        .order
                        .cmp(&original_by_id[right.node_id.as_str()].order)
                })
        };
        for (rank, mut nodes) in edited_by_rank {
            nodes.sort_unstable_by(compare);
            let mut original_nodes: Vec<_> = self
                .snapshot
                .nodes
                .iter()
                .filter(|node| node.rank == rank)
                .collect();
            original_nodes.sort_unstable_by(compare);
            if nodes
                .iter()
                .map(|node| &node.node_id)
                .eq(original_nodes.iter().map(|node| &node.node_id))
            {
                continue;
            }

            let mut slots: Vec<_> = nodes
                .iter()
                .map(|node| source_indices[node.node_id.as_str()])
                .collect();
            slots.sort_unstable();
            for (slot, node) in slots.into_iter().zip(nodes) {
                new_to_old[slot] = source_indices[node.node_id.as_str()];
            }
        }

        // A permutation is the single source of truth for both node storage and all its users.
        // Copying only `ir.nodes` leaves numerically valid but semantically wrong references.
        let reordered = permute_flowchart_nodes(&self.original, &new_to_old)?;

        tracing::info!(
            node_count = reordered.nodes.len(),
            "layout_lens.in_rank_order_putback"
        );
        Ok(reordered)
    }
}

/// Apply a declaration-order permutation without changing any flowchart relationship.
fn permute_flowchart_nodes(
    original: &MermaidDiagramIr,
    new_to_old: &[usize],
) -> Result<MermaidDiagramIr, LayoutLensError> {
    let count = original.nodes.len();
    if new_to_old.len() != count {
        return Err(LayoutLensError::NodeSetChanged);
    }
    let mut old_to_new = vec![usize::MAX; count];
    for (new, &old) in new_to_old.iter().enumerate() {
        let slot = old_to_new
            .get_mut(old)
            .ok_or(LayoutLensError::InvalidNodeReference(old))?;
        if *slot != usize::MAX {
            return Err(LayoutLensError::NodeSetChanged);
        }
        *slot = new;
    }
    if new_to_old.iter().copied().eq(0..count) {
        return Ok(original.clone());
    }

    let remap_node = |node: &mut IrNodeId| -> Result<(), LayoutLensError> {
        node.0 = *old_to_new
            .get(node.0)
            .ok_or(LayoutLensError::InvalidNodeReference(node.0))?;
        Ok(())
    };
    let remap_endpoint = |endpoint: &mut IrEndpoint| -> Result<(), LayoutLensError> {
        match endpoint {
            IrEndpoint::Node(node) => remap_node(node),
            IrEndpoint::Port(port) if port.0 >= original.ports.len() => {
                Err(LayoutLensError::InvalidPortReference(port.0))
            }
            // Port storage is not permuted; its owner is remapped below.
            IrEndpoint::Port(_) | IrEndpoint::Unresolved => Ok(()),
        }
    };
    let mut reordered = original.clone();
    reordered.nodes = new_to_old
        .iter()
        .map(|&old| original.nodes[old].clone())
        .collect();
    for edge in &mut reordered.edges {
        remap_endpoint(&mut edge.from)?;
        remap_endpoint(&mut edge.to)?;
    }
    for port in &mut reordered.ports {
        remap_node(&mut port.node)?;
    }
    for cluster in &mut reordered.clusters {
        for member in &mut cluster.members {
            remap_node(member)?;
        }
    }
    for style in &mut reordered.style_refs {
        if let IrStyleTarget::Node(node) = &mut style.target {
            remap_node(node)?;
        }
    }

    // MermaidGraphIr::node(id) indexes its Vec too. Remapping node_id fields without moving the
    // corresponding records would leave graph.node(id) returning some other node's memberships.
    if !original.graph.nodes.is_empty() {
        if original.graph.nodes.len() != count
            || original
                .graph
                .nodes
                .iter()
                .enumerate()
                .any(|(index, node)| node.node_id.0 != index)
        {
            return Err(LayoutLensError::NodeSetChanged);
        }
        reordered.graph.nodes = new_to_old
            .iter()
            .enumerate()
            .map(|(new, &old)| {
                let mut node = original.graph.nodes[old].clone();
                node.node_id = IrNodeId(new);
                node
            })
            .collect();
    }
    for edge in &mut reordered.graph.edges {
        remap_endpoint(&mut edge.from)?;
        remap_endpoint(&mut edge.to)?;
    }
    for cluster in &mut reordered.graph.clusters {
        for member in &mut cluster.members {
            remap_node(member)?;
        }
    }
    for subgraph in &mut reordered.graph.subgraphs {
        for member in &mut subgraph.members {
            remap_node(member)?;
        }
    }
    Ok(reordered)
}

fn rank_axis(direction: GraphDirection) -> LayoutLensAxis {
    match direction {
        GraphDirection::LR | GraphDirection::RL => LayoutLensAxis::Horizontal,
        GraphDirection::TB | GraphDirection::TD | GraphDirection::BT => LayoutLensAxis::Vertical,
    }
}

fn rank_coordinate(axis: LayoutLensAxis, point: LayoutPoint) -> f32 {
    match axis {
        LayoutLensAxis::Horizontal => point.x,
        LayoutLensAxis::Vertical => point.y,
    }
}

fn rank_secondary(axis: LayoutLensAxis, point: LayoutPoint) -> f32 {
    match axis {
        LayoutLensAxis::Horizontal => point.y,
        LayoutLensAxis::Vertical => point.x,
    }
}

#[cfg(test)]
mod tests {
    use super::{LayoutLens, LayoutLensError};
    use crate::LayoutConfig;

    fn flowchart() -> fm_core::MermaidDiagramIr {
        fm_parser::parse("flowchart TB\nA[Alpha] --> B[Bravo]\nA --> C[Charlie]\n").ir
    }

    #[test]
    fn in_rank_drag_reorders_only_that_ranks_ir_slots() {
        let source = flowchart();
        let lens = LayoutLens::new(&source, LayoutConfig::default()).expect("flowchart lens");
        let mut edited = lens.get().clone();
        let rank = edited
            .nodes
            .iter()
            .map(|node| node.rank)
            .find(|rank| {
                edited
                    .nodes
                    .iter()
                    .filter(|node| node.rank == *rank)
                    .count()
                    >= 2
            })
            .expect("fixture has a rank with siblings");
        let original_order: Vec<String> = lens
            .get()
            .nodes
            .iter()
            .filter(|node| node.rank == rank)
            .map(|node| node.node_id.clone())
            .collect();
        let mut desired_order = original_order.clone();
        desired_order.reverse();
        for (index, node_id) in desired_order.iter().enumerate() {
            let node = edited
                .nodes
                .iter_mut()
                .find(|node| &node.node_id == node_id)
                .expect("edited sibling");
            node.center.x = (index as f32) * 10.0;
        }

        let updated = lens.put(&edited).expect("in-rank drag is safe");
        let source_slots: Vec<usize> = source
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| original_order.contains(&node.id).then_some(index))
            .collect();
        let actual_order: Vec<String> = source_slots
            .iter()
            .map(|slot| updated.nodes[*slot].id.clone())
            .collect();
        assert_eq!(actual_order, desired_order);
        assert_edge_meaning_unchanged(&source, &updated);
    }

    #[test]
    fn cross_rank_drag_is_rejected_instead_of_guessing_a_topology_change() {
        let source = flowchart();
        let lens = LayoutLens::new(&source, LayoutConfig::default()).expect("flowchart lens");
        let mut edited = lens.get().clone();
        edited.nodes[0].center.y += 100.0;

        assert!(matches!(
            lens.put(&edited),
            Err(LayoutLensError::RankAxisMoved(node_id)) if node_id == edited.nodes[0].node_id
        ));
    }

    #[test]
    fn malformed_layout_order_is_rejected_before_it_can_duplicate_ir_nodes() {
        let source = flowchart();
        let mut layout = crate::layout_diagram(&source);
        let sibling_indexes: Vec<usize> = layout
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.rank == 1)
            .map(|(index, _)| index)
            .collect();
        assert!(sibling_indexes.len() >= 2, "fixture needs rank siblings");
        layout.nodes[sibling_indexes[1]].order = layout.nodes[sibling_indexes[0]].order;

        assert!(matches!(
            LayoutLens::from_layout(&source, &layout),
            Err(LayoutLensError::DuplicateOrder { rank: 1, .. })
        ));
    }

    #[test]
    fn impossible_rank_is_rejected_without_sizing_from_untrusted_input() {
        let source = flowchart();
        let mut layout = crate::layout_diagram(&source);
        let node_id = layout.nodes[0].node_id.clone();
        layout.nodes[0].rank = usize::MAX;

        assert!(matches!(
            LayoutLens::from_layout(&source, &layout),
            Err(LayoutLensError::InvalidRank {
                node_id: rejected,
                rank: usize::MAX,
                ..
            }) if rejected == node_id
        ));
    }

    fn endpoint_id(ir: &fm_core::MermaidDiagramIr, endpoint: fm_core::IrEndpoint) -> String {
        let index = ir.resolve_endpoint_node(endpoint).expect("resolved endpoint");
        ir.nodes[index.0].id.clone()
    }

    fn assert_edge_meaning_unchanged(
        before: &fm_core::MermaidDiagramIr,
        after: &fm_core::MermaidDiagramIr,
    ) {
        assert_eq!(before.edges.len(), after.edges.len());
        for (old, new) in before.edges.iter().zip(&after.edges) {
            assert_eq!(endpoint_id(before, old.from), endpoint_id(after, new.from));
            assert_eq!(endpoint_id(before, old.to), endpoint_id(after, new.to));
            let mut payload = new.clone();
            payload.from = old.from;
            payload.to = old.to;
            assert_eq!(&payload, old, "edge labels, arrows and styles must not change");
        }
        assert_eq!(before.graph.edges.len(), after.graph.edges.len());
        for (old, new) in before.graph.edges.iter().zip(&after.graph.edges) {
            assert_eq!(endpoint_id(before, old.from), endpoint_id(after, new.from));
            assert_eq!(endpoint_id(before, old.to), endpoint_id(after, new.to));
        }
    }

    #[test]
    fn drag_preserves_labeled_edges_ports_memberships_and_style_targets() {
        use fm_core::{IrEndpoint, IrNodeId, IrPort, IrPortId, IrStyleTarget};

        let mut source = fm_parser::parse(
            "flowchart TB\nA[Root]\nsubgraph left\nB[Accept]\nend\n\
             subgraph right\nC[Reject]\nend\nA -->|yes| B\nA -->|no| C\n\
             style B fill:#ff0000\nlinkStyle 0 stroke:#0000ff\n",
        )
        .ir;
        assert_eq!(source.nodes.len(), 3);
        let b = source.nodes.iter().position(|node| node.id == "B").unwrap();
        let c = source.nodes.iter().position(|node| node.id == "C").unwrap();
        assert!(source.style_refs.iter().any(|style| {
            matches!(style.target, IrStyleTarget::Node(node) if node.0 == b)
        }));
        assert!(!source.clusters.is_empty());
        assert!(!source.graph.subgraphs.is_empty());
        source.ports.push(IrPort {
            node: IrNodeId(b),
            name: "input".to_string(),
            ..Default::default()
        });
        source.edges[0].to = IrEndpoint::Port(IrPortId(0));
        source.graph.edges[0].to = IrEndpoint::Port(IrPortId(0));

        let mut permutation: Vec<_> = (0..source.nodes.len()).collect();
        permutation.swap(b, c);
        let updated = super::permute_flowchart_nodes(&source, &permutation).unwrap();
        assert_eq!(updated.nodes[b].id, "C");
        assert_eq!(updated.nodes[c].id, "B");
        assert_edge_meaning_unchanged(&source, &updated);
        assert_eq!(updated.ports[0].node, IrNodeId(c));
        assert_eq!(updated.edges[0].to, IrEndpoint::Port(IrPortId(0)));

        let member_ids = |ir: &fm_core::MermaidDiagramIr, members: &[IrNodeId]| {
            members
                .iter()
                .map(|id| ir.nodes[id.0].id.clone())
                .collect::<Vec<_>>()
        };
        for (old, new) in source.clusters.iter().zip(&updated.clusters) {
            assert_eq!(
                member_ids(&source, &old.members),
                member_ids(&updated, &new.members)
            );
        }
        for (old, new) in source.graph.clusters.iter().zip(&updated.graph.clusters) {
            assert_eq!(
                member_ids(&source, &old.members),
                member_ids(&updated, &new.members)
            );
        }
        for (old, new) in source.graph.subgraphs.iter().zip(&updated.graph.subgraphs) {
            assert_eq!(
                member_ids(&source, &old.members),
                member_ids(&updated, &new.members)
            );
        }
        for (new, &old) in permutation.iter().enumerate() {
            let mirror = updated.graph.node(IrNodeId(new)).unwrap();
            assert_eq!(mirror.node_id, IrNodeId(new));
            assert_eq!(mirror.clusters, source.graph.nodes[old].clusters);
            assert_eq!(mirror.subgraphs, source.graph.nodes[old].subgraphs);
        }
        for (old, new) in source.style_refs.iter().zip(&updated.style_refs) {
            match (&old.target, &new.target) {
                (IrStyleTarget::Node(a), IrStyleTarget::Node(b)) => {
                    assert_eq!(source.nodes[a.0].id, updated.nodes[b.0].id);
                }
                (a, b) => assert_eq!(a, b),
            }
            assert_eq!(old.style, new.style);
        }
        assert_eq!(updated.labels, source.labels);
        assert_eq!(updated.constraints, source.constraints);
        assert_eq!(updated.meta, source.meta);
    }

    #[test]
    fn unchanged_visual_order_does_not_rewrite_declaration_order() {
        let source = flowchart();
        let mut layout = crate::layout_diagram(&source);
        // Deliberately make the visual order disagree with declaration order. Layout algorithms
        // are allowed to do this, and a GetPut must not commit their choice as a user edit.
        for node in &mut layout.nodes {
            node.rank = 0;
            node.order = source.nodes.len() - 1 - node.node_index;
            node.bounds.x = node.order as f32 * 100.0;
            node.bounds.y = 0.0;
        }
        let lens = LayoutLens::from_layout(&source, &layout).unwrap();
        assert_eq!(lens.put(lens.get()).unwrap(), source);

        let mut edited = lens.get().clone();
        edited.nodes.reverse();
        for node in &mut edited.nodes {
            node.center.x += 1.0; // A small drag, but no node crossed another.
        }
        assert_eq!(lens.put(&edited).unwrap(), source);
    }

    #[test]
    fn editing_one_rank_does_not_normalize_untouched_ranks() {
        let source = fm_parser::parse("flowchart TB\nA --> C\nB --> D\n").ir;
        let mut layout = crate::layout_diagram(&source);
        for node in &mut layout.nodes {
            let (rank, order) = match node.node_id.as_str() {
                "A" => (0, 1),
                "B" => (0, 0),
                "C" => (1, 0),
                "D" => (1, 1),
                _ => unreachable!(),
            };
            node.rank = rank;
            node.order = order;
            node.bounds = crate::LayoutRect {
                x: order as f32 * 100.0,
                y: rank as f32 * 100.0,
                width: 10.0,
                height: 10.0,
            };
        }
        let lens = LayoutLens::from_layout(&source, &layout).unwrap();
        let mut edited = lens.get().clone();
        let d_x = edited
            .nodes
            .iter()
            .find(|node| node.node_id == "D")
            .unwrap()
            .center
            .x;
        edited
            .nodes
            .iter_mut()
            .find(|node| node.node_id == "C")
            .unwrap()
            .center
            .x = d_x + 10.0;
        let updated = lens.put(&edited).unwrap();
        for (slot, node) in source.nodes.iter().enumerate() {
            if matches!(node.id.as_str(), "A" | "B") {
                assert_eq!(updated.nodes[slot], *node, "untouched rank changed");
            }
        }
        let actual: Vec<_> = updated
            .nodes
            .iter()
            .filter(|node| matches!(node.id.as_str(), "C" | "D"))
            .map(|node| node.id.as_str())
            .collect();
        assert_eq!(actual, ["D", "C"]);
        assert_edge_meaning_unchanged(&source, &updated);
    }

    #[test]
    fn tied_positions_keep_captured_order_instead_of_sorting_by_id() {
        let source = fm_parser::parse("flowchart LR\nZ --> A\n").ir;
        let mut layout = crate::layout_diagram(&source);
        for node in &mut layout.nodes {
            node.rank = 0;
            node.order = node.node_index;
            node.bounds = crate::LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            };
        }
        let lens = LayoutLens::from_layout(&source, &layout).unwrap();
        let mut edited = lens.get().clone();
        edited.nodes.reverse();
        assert_eq!(lens.put(&edited).unwrap(), source);
    }

    #[test]
    fn corrupt_node_references_fail_without_mutating_the_original() {
        let mut source = flowchart();
        source.edges[0].to = fm_core::IrEndpoint::Node(fm_core::IrNodeId(usize::MAX));
        let original = source.clone();
        let mut permutation: Vec<_> = (0..source.nodes.len()).collect();
        permutation.swap(1, 2);
        assert!(matches!(
            super::permute_flowchart_nodes(&source, &permutation),
            Err(LayoutLensError::InvalidNodeReference(usize::MAX))
        ));
        assert_eq!(source, original);
        source.edges[0].to = fm_core::IrEndpoint::Port(fm_core::IrPortId(usize::MAX));
        assert!(matches!(
            super::permute_flowchart_nodes(&source, &permutation),
            Err(LayoutLensError::InvalidPortReference(usize::MAX))
        ));
    }

    #[test]
    fn permutation_followed_by_its_inverse_preserves_the_entire_ir() {
        let source = flowchart();
        for permutation in [[0, 2, 1], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
            let mut inverse = [0; 3];
            for (new, old) in permutation.into_iter().enumerate() {
                inverse[old] = new;
            }
            let updated = super::permute_flowchart_nodes(&source, &permutation).unwrap();
            assert_edge_meaning_unchanged(&source, &updated);
            let restored = super::permute_flowchart_nodes(&updated, &inverse).unwrap();
            assert_eq!(restored, source);
        }
    }
}
