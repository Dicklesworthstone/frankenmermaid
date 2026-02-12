//! Diagram diffing with visual highlighting.
//!
//! Compares two `MermaidDiagramIr` instances and produces a diff result
//! that identifies added, removed, changed, and unchanged elements.

use fm_core::{ArrowType, IrEndpoint, IrNode, IrNodeId, MermaidDiagramIr, NodeShape};
use std::collections::{BTreeMap, BTreeSet};

/// Status of a diff element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffStatus {
    /// Element exists only in the new diagram.
    Added,
    /// Element exists only in the old diagram.
    Removed,
    /// Element exists in both but has changed.
    Changed,
    /// Element is identical in both diagrams.
    Unchanged,
}

/// A diffed node with its status.
#[derive(Debug, Clone)]
pub struct DiffNode {
    /// Node ID.
    pub id: String,
    /// Diff status.
    pub status: DiffStatus,
    /// The node data (from new if exists, else from old).
    pub node: IrNode,
    /// Changes if status is Changed.
    pub changes: Vec<NodeChange>,
}

/// What changed about a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeChange {
    LabelChanged { old: String, new: String },
    ShapeChanged { old: NodeShape, new: NodeShape },
    ClassesChanged { old: Vec<String>, new: Vec<String> },
}

/// A diffed edge with its status.
#[derive(Debug, Clone)]
pub struct DiffEdge {
    /// Edge from-to identifier.
    pub from_id: String,
    pub to_id: String,
    /// Diff status.
    pub status: DiffStatus,
    /// Arrow type.
    pub arrow: ArrowType,
    /// Changes if status is Changed.
    pub changes: Vec<EdgeChange>,
}

/// What changed about an edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdgeChange {
    ArrowChanged { old: ArrowType, new: ArrowType },
    LabelChanged { old: String, new: String },
}

/// Complete diff result between two diagrams.
#[derive(Debug, Clone)]
pub struct DiagramDiff {
    /// Diffed nodes.
    pub nodes: Vec<DiffNode>,
    /// Diffed edges.
    pub edges: Vec<DiffEdge>,
    /// Summary counts.
    pub added_nodes: usize,
    pub removed_nodes: usize,
    pub changed_nodes: usize,
    pub unchanged_nodes: usize,
    pub added_edges: usize,
    pub removed_edges: usize,
    pub changed_edges: usize,
    pub unchanged_edges: usize,
}

impl DiagramDiff {
    /// Returns true if there are any differences.
    #[must_use]
    pub fn has_changes(&self) -> bool {
        self.added_nodes > 0
            || self.removed_nodes > 0
            || self.changed_nodes > 0
            || self.added_edges > 0
            || self.removed_edges > 0
            || self.changed_edges > 0
    }

    /// Total number of changed elements.
    #[must_use]
    pub fn total_changes(&self) -> usize {
        self.added_nodes
            + self.removed_nodes
            + self.changed_nodes
            + self.added_edges
            + self.removed_edges
            + self.changed_edges
    }
}

/// Compute the diff between two diagrams.
#[must_use]
pub fn diff_diagrams(old: &MermaidDiagramIr, new: &MermaidDiagramIr) -> DiagramDiff {
    let (nodes, node_counts) = diff_nodes(old, new);
    let (edges, edge_counts) = diff_edges(old, new);

    DiagramDiff {
        nodes,
        edges,
        added_nodes: node_counts.0,
        removed_nodes: node_counts.1,
        changed_nodes: node_counts.2,
        unchanged_nodes: node_counts.3,
        added_edges: edge_counts.0,
        removed_edges: edge_counts.1,
        changed_edges: edge_counts.2,
        unchanged_edges: edge_counts.3,
    }
}

fn diff_nodes(
    old: &MermaidDiagramIr,
    new: &MermaidDiagramIr,
) -> (Vec<DiffNode>, (usize, usize, usize, usize)) {
    let old_by_id: BTreeMap<&str, (usize, &IrNode)> = old
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), (i, n)))
        .collect();

    let new_by_id: BTreeMap<&str, (usize, &IrNode)> = new
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), (i, n)))
        .collect();

    let all_ids: BTreeSet<&str> = old_by_id.keys().chain(new_by_id.keys()).copied().collect();

    let mut results = Vec::new();
    let mut added = 0_usize;
    let mut removed = 0_usize;
    let mut changed = 0_usize;
    let mut unchanged = 0_usize;

    for id in all_ids {
        match (old_by_id.get(id), new_by_id.get(id)) {
            (None, Some((_idx, new_node))) => {
                results.push(DiffNode {
                    id: id.to_string(),
                    status: DiffStatus::Added,
                    node: (*new_node).clone(),
                    changes: Vec::new(),
                });
                added += 1;
            }
            (Some((_idx, old_node)), None) => {
                results.push(DiffNode {
                    id: id.to_string(),
                    status: DiffStatus::Removed,
                    node: (*old_node).clone(),
                    changes: Vec::new(),
                });
                removed += 1;
            }
            (Some((old_idx, old_node)), Some((_new_idx, new_node))) => {
                let changes = compare_nodes(old, old_node, *old_idx, new, new_node);
                if changes.is_empty() {
                    results.push(DiffNode {
                        id: id.to_string(),
                        status: DiffStatus::Unchanged,
                        node: (*new_node).clone(),
                        changes: Vec::new(),
                    });
                    unchanged += 1;
                } else {
                    results.push(DiffNode {
                        id: id.to_string(),
                        status: DiffStatus::Changed,
                        node: (*new_node).clone(),
                        changes,
                    });
                    changed += 1;
                }
            }
            (None, None) => unreachable!(),
        }
    }

    (results, (added, removed, changed, unchanged))
}

fn compare_nodes(
    old_ir: &MermaidDiagramIr,
    old_node: &IrNode,
    _old_idx: usize,
    new_ir: &MermaidDiagramIr,
    new_node: &IrNode,
) -> Vec<NodeChange> {
    let mut changes = Vec::new();

    // Compare shapes.
    if old_node.shape != new_node.shape {
        changes.push(NodeChange::ShapeChanged {
            old: old_node.shape,
            new: new_node.shape,
        });
    }

    // Compare labels.
    let old_label = old_node
        .label
        .and_then(|lid| old_ir.labels.get(lid.0))
        .map(|l| l.text.clone())
        .unwrap_or_default();

    let new_label = new_node
        .label
        .and_then(|lid| new_ir.labels.get(lid.0))
        .map(|l| l.text.clone())
        .unwrap_or_default();

    if old_label != new_label {
        changes.push(NodeChange::LabelChanged {
            old: old_label,
            new: new_label,
        });
    }

    // Compare classes.
    if old_node.classes != new_node.classes {
        changes.push(NodeChange::ClassesChanged {
            old: old_node.classes.clone(),
            new: new_node.classes.clone(),
        });
    }

    changes
}

fn diff_edges(
    old: &MermaidDiagramIr,
    new: &MermaidDiagramIr,
) -> (Vec<DiffEdge>, (usize, usize, usize, usize)) {
    // Build edge identity maps.
    let old_edges: BTreeMap<(String, String), &_> = old
        .edges
        .iter()
        .filter_map(|e| {
            let from_id = endpoint_id(old, e.from)?;
            let to_id = endpoint_id(old, e.to)?;
            Some(((from_id, to_id), e))
        })
        .collect();

    let new_edges: BTreeMap<(String, String), &_> = new
        .edges
        .iter()
        .filter_map(|e| {
            let from_id = endpoint_id(new, e.from)?;
            let to_id = endpoint_id(new, e.to)?;
            Some(((from_id, to_id), e))
        })
        .collect();

    let all_keys: BTreeSet<(String, String)> = old_edges
        .keys()
        .cloned()
        .chain(new_edges.keys().cloned())
        .collect();

    let mut results = Vec::new();
    let mut added = 0_usize;
    let mut removed = 0_usize;
    let mut changed = 0_usize;
    let mut unchanged = 0_usize;

    for key in all_keys {
        match (old_edges.get(&key), new_edges.get(&key)) {
            (None, Some(new_edge)) => {
                results.push(DiffEdge {
                    from_id: key.0,
                    to_id: key.1,
                    status: DiffStatus::Added,
                    arrow: new_edge.arrow,
                    changes: Vec::new(),
                });
                added += 1;
            }
            (Some(old_edge), None) => {
                results.push(DiffEdge {
                    from_id: key.0,
                    to_id: key.1,
                    status: DiffStatus::Removed,
                    arrow: old_edge.arrow,
                    changes: Vec::new(),
                });
                removed += 1;
            }
            (Some(old_edge), Some(new_edge)) => {
                let changes = compare_edges(old, old_edge, new, new_edge);
                if changes.is_empty() {
                    results.push(DiffEdge {
                        from_id: key.0,
                        to_id: key.1,
                        status: DiffStatus::Unchanged,
                        arrow: new_edge.arrow,
                        changes: Vec::new(),
                    });
                    unchanged += 1;
                } else {
                    results.push(DiffEdge {
                        from_id: key.0,
                        to_id: key.1,
                        status: DiffStatus::Changed,
                        arrow: new_edge.arrow,
                        changes,
                    });
                    changed += 1;
                }
            }
            (None, None) => unreachable!(),
        }
    }

    (results, (added, removed, changed, unchanged))
}

fn compare_edges(
    old_ir: &MermaidDiagramIr,
    old_edge: &fm_core::IrEdge,
    new_ir: &MermaidDiagramIr,
    new_edge: &fm_core::IrEdge,
) -> Vec<EdgeChange> {
    let mut changes = Vec::new();

    // Compare arrow types.
    if old_edge.arrow != new_edge.arrow {
        changes.push(EdgeChange::ArrowChanged {
            old: old_edge.arrow,
            new: new_edge.arrow,
        });
    }

    // Compare labels.
    let old_label = old_edge
        .label
        .and_then(|lid| old_ir.labels.get(lid.0))
        .map(|l| l.text.clone())
        .unwrap_or_default();

    let new_label = new_edge
        .label
        .and_then(|lid| new_ir.labels.get(lid.0))
        .map(|l| l.text.clone())
        .unwrap_or_default();

    if old_label != new_label {
        changes.push(EdgeChange::LabelChanged {
            old: old_label,
            new: new_label,
        });
    }

    changes
}

fn endpoint_id(ir: &MermaidDiagramIr, endpoint: IrEndpoint) -> Option<String> {
    match endpoint {
        IrEndpoint::Node(IrNodeId(idx)) => ir.nodes.get(idx).map(|n| n.id.clone()),
        IrEndpoint::Port(port_id) => ir
            .ports
            .get(port_id.0)
            .and_then(|p| ir.nodes.get(p.node.0))
            .map(|n| n.id.clone()),
        IrEndpoint::Unresolved => None,
    }
}

/// ANSI color codes for diff rendering.
pub mod colors {
    pub const ADDED: &str = "\x1b[32m";     // Green
    pub const REMOVED: &str = "\x1b[31m";   // Red
    pub const CHANGED: &str = "\x1b[33m";   // Yellow
    pub const UNCHANGED: &str = "\x1b[90m"; // Gray
    pub const RESET: &str = "\x1b[0m";

    pub const BG_ADDED: &str = "\x1b[42m";   // Green background
    pub const BG_REMOVED: &str = "\x1b[41m"; // Red background
    pub const BG_CHANGED: &str = "\x1b[43m"; // Yellow background
}

/// Render a diff summary to a string.
#[must_use]
pub fn render_diff_summary(diff: &DiagramDiff, use_colors: bool) -> String {
    let mut output = String::new();

    output.push_str("Diagram Diff Summary:\n");
    output.push_str("=====================\n\n");

    // Nodes section.
    output.push_str("Nodes:\n");
    if diff.added_nodes > 0 {
        if use_colors {
            output.push_str(colors::ADDED);
        }
        output.push_str(&format!("  + {} added\n", diff.added_nodes));
        if use_colors {
            output.push_str(colors::RESET);
        }
    }
    if diff.removed_nodes > 0 {
        if use_colors {
            output.push_str(colors::REMOVED);
        }
        output.push_str(&format!("  - {} removed\n", diff.removed_nodes));
        if use_colors {
            output.push_str(colors::RESET);
        }
    }
    if diff.changed_nodes > 0 {
        if use_colors {
            output.push_str(colors::CHANGED);
        }
        output.push_str(&format!("  ~ {} changed\n", diff.changed_nodes));
        if use_colors {
            output.push_str(colors::RESET);
        }
    }
    if diff.unchanged_nodes > 0 {
        if use_colors {
            output.push_str(colors::UNCHANGED);
        }
        output.push_str(&format!("  = {} unchanged\n", diff.unchanged_nodes));
        if use_colors {
            output.push_str(colors::RESET);
        }
    }

    output.push('\n');

    // Edges section.
    output.push_str("Edges:\n");
    if diff.added_edges > 0 {
        if use_colors {
            output.push_str(colors::ADDED);
        }
        output.push_str(&format!("  + {} added\n", diff.added_edges));
        if use_colors {
            output.push_str(colors::RESET);
        }
    }
    if diff.removed_edges > 0 {
        if use_colors {
            output.push_str(colors::REMOVED);
        }
        output.push_str(&format!("  - {} removed\n", diff.removed_edges));
        if use_colors {
            output.push_str(colors::RESET);
        }
    }
    if diff.changed_edges > 0 {
        if use_colors {
            output.push_str(colors::CHANGED);
        }
        output.push_str(&format!("  ~ {} changed\n", diff.changed_edges));
        if use_colors {
            output.push_str(colors::RESET);
        }
    }
    if diff.unchanged_edges > 0 {
        if use_colors {
            output.push_str(colors::UNCHANGED);
        }
        output.push_str(&format!("  = {} unchanged\n", diff.unchanged_edges));
        if use_colors {
            output.push_str(colors::RESET);
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{DiagramType, GraphDirection, IrEdge, IrLabel, IrLabelId};

    fn make_ir_with_nodes(node_ids: &[&str]) -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::LR;
        for (i, id) in node_ids.iter().enumerate() {
            ir.labels.push(IrLabel {
                text: id.to_string(),
                ..Default::default()
            });
            ir.nodes.push(IrNode {
                id: id.to_string(),
                label: Some(IrLabelId(i)),
                ..Default::default()
            });
        }
        ir
    }

    #[test]
    fn identical_diagrams_have_no_changes() {
        let ir = make_ir_with_nodes(&["A", "B", "C"]);
        let diff = diff_diagrams(&ir, &ir);
        assert!(!diff.has_changes());
        assert_eq!(diff.unchanged_nodes, 3);
    }

    #[test]
    fn detects_added_nodes() {
        let old = make_ir_with_nodes(&["A", "B"]);
        let new = make_ir_with_nodes(&["A", "B", "C"]);
        let diff = diff_diagrams(&old, &new);
        assert!(diff.has_changes());
        assert_eq!(diff.added_nodes, 1);
        assert_eq!(diff.unchanged_nodes, 2);
    }

    #[test]
    fn detects_removed_nodes() {
        let old = make_ir_with_nodes(&["A", "B", "C"]);
        let new = make_ir_with_nodes(&["A", "B"]);
        let diff = diff_diagrams(&old, &new);
        assert!(diff.has_changes());
        assert_eq!(diff.removed_nodes, 1);
    }

    #[test]
    fn detects_changed_node_labels() {
        let mut old = make_ir_with_nodes(&["A"]);
        let mut new = make_ir_with_nodes(&["A"]);
        new.labels[0].text = "Changed".to_string();

        let diff = diff_diagrams(&old, &new);
        assert!(diff.has_changes());
        assert_eq!(diff.changed_nodes, 1);
    }

    #[test]
    fn detects_added_edges() {
        let mut old = make_ir_with_nodes(&["A", "B"]);
        let mut new = make_ir_with_nodes(&["A", "B"]);

        new.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..Default::default()
        });

        let diff = diff_diagrams(&old, &new);
        assert!(diff.has_changes());
        assert_eq!(diff.added_edges, 1);
    }

    #[test]
    fn diff_summary_includes_counts() {
        let old = make_ir_with_nodes(&["A", "B"]);
        let new = make_ir_with_nodes(&["A", "B", "C"]);
        let diff = diff_diagrams(&old, &new);
        let summary = render_diff_summary(&diff, false);
        assert!(summary.contains("1 added"));
    }
}
