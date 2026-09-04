//! Bidirectional layout editing for flowchart node ordering.
//!
//! Mermaid source has no coordinate syntax.  A host may still let a reader drag a node within a
//! rank, but that edit must become a deterministic IR ordering change rather than a private pixel
//! override that disappears on the next layout.  [`LayoutLens`] carries the original rank/order
//! complement and makes that one safe putback explicit.  Moving across ranks or changing topology
//! is rejected: guessing a new edge direction from a pixel drag would silently rewrite meaning.

use std::collections::{BTreeMap, BTreeSet};

use fm_core::{DiagramType, GraphDirection, MermaidDiagramIr};

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
        let mut seen_orders = BTreeSet::new();
        for node in &layout.nodes {
            if !seen.insert(node.node_id.clone()) {
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
            if !seen_orders.insert((node.rank, node.order)) {
                return Err(LayoutLensError::DuplicateOrder {
                    rank: node.rank,
                    order: node.order,
                });
            }
            let center = node.bounds.center();
            if !center.x.is_finite() || !center.y.is_finite() {
                return Err(LayoutLensError::NonFinitePosition(node.node_id.clone()));
            }
        }
        if seen.iter().map(String::as_str).collect::<BTreeSet<_>>() != ir_node_ids {
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
        for order in &mut rank_orders {
            order.sort_unstable_by_key(|(node_order, _)| *node_order);
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
    /// The returned IR preserves nodes from other ranks at their existing declaration slots. Edge
    /// endpoints are IDs, so reordering these nodes cannot reconnect an edge or alter a label.
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

        let original_nodes_by_id: BTreeMap<&str, _> = self
            .original
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        if original_nodes_by_id.len() != self.original.nodes.len() {
            return Err(LayoutLensError::NodeSetChanged);
        }

        let mut reordered = self.original.clone();
        for (rank, mut nodes) in edited_by_rank {
            nodes.sort_unstable_by(|left, right| {
                rank_secondary(self.complement.rank_axis, left.center)
                    .total_cmp(&rank_secondary(self.complement.rank_axis, right.center))
                    .then_with(|| left.node_id.cmp(&right.node_id))
            });
            let original_slots: Vec<usize> = self
                .snapshot
                .nodes
                .iter()
                .filter(|node| node.rank == rank)
                .map(|node| node.order)
                .collect();
            if original_slots.len() != nodes.len() {
                return Err(LayoutLensError::NodeSetChanged);
            }
            let mut declaration_slots: Vec<usize> = original_slots
                .iter()
                .map(|order| {
                    self.snapshot
                        .nodes
                        .iter()
                        .find(|node| node.rank == rank && node.order == *order)
                        .and_then(|node| {
                            self.original
                                .nodes
                                .iter()
                                .position(|original| original.id == node.node_id)
                        })
                        .ok_or(LayoutLensError::NodeSetChanged)
                })
                .collect::<Result<_, _>>()?;
            declaration_slots.sort_unstable();
            for (slot, node) in declaration_slots.into_iter().zip(nodes) {
                let original = original_nodes_by_id
                    .get(node.node_id.as_str())
                    .ok_or(LayoutLensError::NodeSetChanged)?;
                reordered.nodes[slot] = (*original).clone();
            }
        }

        tracing::info!(
            node_count = reordered.nodes.len(),
            "layout_lens.in_rank_order_putback"
        );
        Ok(reordered)
    }
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
        assert_eq!(
            updated.edges, source.edges,
            "dragging cannot reconnect an edge"
        );
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
}
