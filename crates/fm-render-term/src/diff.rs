//! Diagram diffing with visual highlighting.
//!
//! Compares two `MermaidDiagramIr` instances and produces a diff result
//! that identifies added, removed, changed, and unchanged elements.

use crate::{TermRenderConfig, render_diagram_with_config};
use fm_core::{ArrowType, IrEndpoint, IrNode, IrNodeId, MermaidDiagramIr, NodeShape};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// Status of a diff element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum NodeChange {
    LabelChanged { old: String, new: String },
    ShapeChanged { old: NodeShape, new: NodeShape },
    ClassesChanged { old: Vec<String>, new: Vec<String> },
    MembersChanged { old: Vec<String>, new: Vec<String> },
}

/// A diffed edge with its status.
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum EdgeChange {
    ArrowChanged { old: ArrowType, new: ArrowType },
    LabelChanged { old: String, new: String },
}

/// Complete diff result between two diagrams.
#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
struct AlignedDiffLine {
    status: DiffStatus,
    old_line: Option<String>,
    new_line: Option<String>,
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

    let old_members = node_member_strings(old_node);
    let new_members = node_member_strings(new_node);
    if old_members != new_members {
        changes.push(NodeChange::MembersChanged {
            old: old_members,
            new: new_members,
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

fn node_member_strings(node: &IrNode) -> Vec<String> {
    node.members
        .iter()
        .map(|member| {
            let key = match member.key {
                fm_core::IrAttributeKey::Pk => " PK",
                fm_core::IrAttributeKey::Fk => " FK",
                fm_core::IrAttributeKey::Uk => " UK",
                fm_core::IrAttributeKey::None => "",
            };
            match &member.comment {
                Some(comment) => {
                    format!("{}:{}{} // {}", member.name, member.data_type, key, comment)
                }
                None => format!("{}:{}{}", member.name, member.data_type, key),
            }
        })
        .collect()
}

/// ANSI color codes for diff rendering.
pub mod colors {
    pub const ADDED: &str = "\x1b[32m"; // Green
    pub const REMOVED: &str = "\x1b[31m"; // Red
    pub const CHANGED: &str = "\x1b[33m"; // Yellow
    pub const UNCHANGED: &str = "\x1b[90m"; // Gray
    pub const RESET: &str = "\x1b[0m";

    pub const BG_ADDED: &str = "\x1b[42m"; // Green background
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

/// Render a detailed diff report suitable for CI logs and plain text tooling.
#[must_use]
pub fn render_diff_plain(diff: &DiagramDiff) -> String {
    let mut output = render_diff_summary(diff, false);

    output.push('\n');
    output.push_str("Node Details:\n");
    for node in &diff.nodes {
        output.push_str(&format!(
            "  {} node {}\n",
            status_symbol(node.status),
            node.id
        ));
        for change in &node.changes {
            output.push_str(&format!("    - {}\n", format_node_change(change)));
        }
    }

    output.push('\n');
    output.push_str("Edge Details:\n");
    for edge in &diff.edges {
        output.push_str(&format!(
            "  {} edge {} -> {}\n",
            status_symbol(edge.status),
            edge.from_id,
            edge.to_id
        ));
        for change in &edge.changes {
            output.push_str(&format!("    - {}\n", format_edge_change(change)));
        }
    }

    output
}

/// Render a side-by-side terminal diff between two diagrams with default settings.
#[must_use]
pub fn render_diff_terminal(
    old: &MermaidDiagramIr,
    new: &MermaidDiagramIr,
    cols: usize,
    rows: usize,
    use_colors: bool,
) -> String {
    render_diff_terminal_with_config(old, new, &TermRenderConfig::rich(), cols, rows, use_colors)
}

/// Render a side-by-side terminal diff between two diagrams with an explicit config.
#[must_use]
pub fn render_diff_terminal_with_config(
    old: &MermaidDiagramIr,
    new: &MermaidDiagramIr,
    config: &TermRenderConfig,
    cols: usize,
    rows: usize,
    use_colors: bool,
) -> String {
    let total_cols = cols.max(60);
    let pane_width = (total_cols.saturating_sub(7) / 2).max(24);
    let pane_rows = rows.max(12);

    let old_render = render_diagram_with_config(old, config, pane_width, pane_rows);
    let new_render = render_diagram_with_config(new, config, pane_width, pane_rows);
    let diff = diff_diagrams(old, new);

    let aligned = align_rendered_lines(&old_render.output, &new_render.output);
    let old_line_width = aligned
        .iter()
        .filter_map(|line| line.old_line.as_deref())
        .map(display_width)
        .max()
        .unwrap_or(0)
        .min(pane_width);

    let mut output = String::new();
    output.push_str("Diagram Diff\n");
    output.push_str("============\n");
    output.push_str(&render_diff_summary(&diff, use_colors));
    output.push('\n');
    output.push_str(&format!(
        "{:<3} {:<width$} | New\n",
        "",
        "Old",
        width = old_line_width
    ));
    output.push_str(&format!(
        "{}\n",
        "-".repeat(old_line_width.saturating_add(3 + 3 + 5))
    ));

    for line in aligned {
        let marker = status_symbol(line.status);
        let marker = colorize_marker(marker, line.status, use_colors);
        let old_text = line.old_line.unwrap_or_default();
        let new_text = line.new_line.unwrap_or_default();
        let old_padded = pad_display(&truncate_display(&old_text, old_line_width), old_line_width);
        let new_trimmed = truncate_display(&new_text, pane_width);
        output.push_str(&format!("{marker}  {old_padded} | {new_trimmed}\n"));
    }

    output
}

fn format_node_change(change: &NodeChange) -> String {
    match change {
        NodeChange::LabelChanged { old, new } => format!("label: {old:?} -> {new:?}"),
        NodeChange::ShapeChanged { old, new } => format!("shape: {old:?} -> {new:?}"),
        NodeChange::ClassesChanged { old, new } => format!("classes: {old:?} -> {new:?}"),
        NodeChange::MembersChanged { old, new } => format!("members: {old:?} -> {new:?}"),
    }
}

fn format_edge_change(change: &EdgeChange) -> String {
    match change {
        EdgeChange::ArrowChanged { old, new } => format!("arrow: {old:?} -> {new:?}"),
        EdgeChange::LabelChanged { old, new } => format!("label: {old:?} -> {new:?}"),
    }
}

fn status_symbol(status: DiffStatus) -> char {
    match status {
        DiffStatus::Added => '+',
        DiffStatus::Removed => '-',
        DiffStatus::Changed => '~',
        DiffStatus::Unchanged => '=',
    }
}

fn colorize_marker(marker: char, status: DiffStatus, use_colors: bool) -> String {
    if !use_colors {
        return marker.to_string();
    }

    let color = match status {
        DiffStatus::Added => colors::ADDED,
        DiffStatus::Removed => colors::REMOVED,
        DiffStatus::Changed => colors::CHANGED,
        DiffStatus::Unchanged => colors::UNCHANGED,
    };
    format!("{color}{marker}{}", colors::RESET)
}

fn display_width(value: &str) -> usize {
    value.chars().count()
}

fn truncate_display(value: &str, max_width: usize) -> String {
    if display_width(value) <= max_width {
        return value.to_string();
    }

    let mut truncated = String::new();
    for ch in value.chars().take(max_width.saturating_sub(1)) {
        truncated.push(ch);
    }
    truncated.push('…');
    truncated
}

fn pad_display(value: &str, width: usize) -> String {
    let current = display_width(value);
    if current >= width {
        return value.to_string();
    }
    format!("{value}{}", " ".repeat(width - current))
}

fn align_rendered_lines(old_output: &str, new_output: &str) -> Vec<AlignedDiffLine> {
    let old_lines: Vec<&str> = old_output.lines().collect();
    let new_lines: Vec<&str> = new_output.lines().collect();
    let anchors = lcs_pairs(&old_lines, &new_lines);

    let mut aligned = Vec::new();
    let mut old_index = 0;
    let mut new_index = 0;

    for (anchor_old, anchor_new) in anchors
        .into_iter()
        .chain(std::iter::once((old_lines.len(), new_lines.len())))
    {
        aligned.extend(align_changed_block(
            &old_lines[old_index..anchor_old],
            &new_lines[new_index..anchor_new],
        ));

        if anchor_old < old_lines.len() && anchor_new < new_lines.len() {
            aligned.push(AlignedDiffLine {
                status: DiffStatus::Unchanged,
                old_line: Some(old_lines[anchor_old].to_string()),
                new_line: Some(new_lines[anchor_new].to_string()),
            });
        }

        old_index = anchor_old.saturating_add(1);
        new_index = anchor_new.saturating_add(1);
    }

    aligned
}

fn align_changed_block(old_lines: &[&str], new_lines: &[&str]) -> Vec<AlignedDiffLine> {
    let shared = old_lines.len().min(new_lines.len());
    let mut aligned = Vec::new();

    for index in 0..shared {
        aligned.push(AlignedDiffLine {
            status: DiffStatus::Changed,
            old_line: Some(old_lines[index].to_string()),
            new_line: Some(new_lines[index].to_string()),
        });
    }

    for line in &old_lines[shared..] {
        aligned.push(AlignedDiffLine {
            status: DiffStatus::Removed,
            old_line: Some((*line).to_string()),
            new_line: None,
        });
    }

    for line in &new_lines[shared..] {
        aligned.push(AlignedDiffLine {
            status: DiffStatus::Added,
            old_line: None,
            new_line: Some((*line).to_string()),
        });
    }

    aligned
}

fn lcs_pairs(old_lines: &[&str], new_lines: &[&str]) -> Vec<(usize, usize)> {
    let mut dp = vec![vec![0_usize; new_lines.len() + 1]; old_lines.len() + 1];

    for old_index in (0..old_lines.len()).rev() {
        for new_index in (0..new_lines.len()).rev() {
            dp[old_index][new_index] = if old_lines[old_index] == new_lines[new_index] {
                dp[old_index + 1][new_index + 1] + 1
            } else {
                dp[old_index + 1][new_index].max(dp[old_index][new_index + 1])
            };
        }
    }

    let mut old_index = 0;
    let mut new_index = 0;
    let mut pairs = Vec::new();
    while old_index < old_lines.len() && new_index < new_lines.len() {
        if old_lines[old_index] == new_lines[new_index] {
            pairs.push((old_index, new_index));
            old_index += 1;
            new_index += 1;
        } else if dp[old_index + 1][new_index] >= dp[old_index][new_index + 1] {
            old_index += 1;
        } else {
            new_index += 1;
        }
    }

    pairs
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{
        DiagramType, GraphDirection, IrAttributeKey, IrEdge, IrEntityAttribute, IrLabel, IrLabelId,
    };

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
        let old = make_ir_with_nodes(&["A"]);
        let mut new = make_ir_with_nodes(&["A"]);
        new.labels[0].text = "Changed".to_string();

        let diff = diff_diagrams(&old, &new);
        assert!(diff.has_changes());
        assert_eq!(diff.changed_nodes, 1);
    }

    #[test]
    fn detects_changed_node_members() {
        let mut old = make_ir_with_nodes(&["A"]);
        let mut new = make_ir_with_nodes(&["A"]);
        old.nodes[0].members.push(IrEntityAttribute {
            data_type: "int".to_string(),
            name: "id".to_string(),
            key: IrAttributeKey::Pk,
            comment: None,
        });
        new.nodes[0].members.push(IrEntityAttribute {
            data_type: "string".to_string(),
            name: "id".to_string(),
            key: IrAttributeKey::Pk,
            comment: None,
        });

        let diff = diff_diagrams(&old, &new);
        assert_eq!(diff.changed_nodes, 1);
        assert!(matches!(
            diff.nodes[0].changes[0],
            NodeChange::MembersChanged { .. }
        ));
    }

    #[test]
    fn detects_added_edges() {
        let old = make_ir_with_nodes(&["A", "B"]);
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

    #[test]
    fn plain_diff_includes_detailed_changes() {
        let old = make_ir_with_nodes(&["A"]);
        let mut new = make_ir_with_nodes(&["A"]);
        new.labels[0].text = "Changed".to_string();

        let diff = diff_diagrams(&old, &new);
        let plain = render_diff_plain(&diff);
        assert!(plain.contains("Node Details"));
        assert!(plain.contains("label"));
    }

    #[test]
    fn terminal_diff_renders_side_by_side() {
        let old = make_ir_with_nodes(&["A"]);
        let new = make_ir_with_nodes(&["A", "B"]);
        let rendered = render_diff_terminal(&old, &new, 100, 24, false);
        assert!(rendered.contains("Diagram Diff"));
        assert!(rendered.contains("Old"));
        assert!(rendered.contains("| New"));
    }
}
