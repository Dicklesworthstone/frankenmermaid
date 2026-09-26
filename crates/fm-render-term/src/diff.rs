//! Structural and semantic diagram diffing.
//!
//! Nodes and edges are only part of a Mermaid document. Chart values, schedules,
//! sequence annotations and source-level configuration can change without adding
//! or removing a single graph element. The public diff includes both surfaces.
//! [`structural`] remains available for callers explicitly requesting graph-only
//! comparison; its implementation and existing regression tests are unchanged.

pub mod structural;
mod semantic;

pub use semantic::DiagramChange;
pub use structural::{DiffEdge, DiffNode, DiffStatus, EdgeChange, NodeChange, colors};

use crate::TermRenderConfig;
use fm_core::MermaidDiagramIr;
use serde::Serialize;
use std::fmt::Write;

/// Complete diff between two diagrams, including non-graph document semantics.
#[derive(Debug, Clone, Serialize)]
pub struct DiagramDiff {
    pub nodes: Vec<DiffNode>,
    pub edges: Vec<DiffEdge>,
    pub added_nodes: usize,
    pub removed_nodes: usize,
    pub changed_nodes: usize,
    pub unchanged_nodes: usize,
    pub added_edges: usize,
    pub removed_edges: usize,
    pub changed_edges: usize,
    pub unchanged_edges: usize,
    /// Changed document fields, in deterministic order. Empty for a graph-only edit.
    ///
    /// Omitted from serialization when empty to preserve existing graph-only reports.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub diagram_changes: Vec<DiagramChange>,
}

impl DiagramDiff {
    /// Whether either graph elements or document semantics changed.
    #[must_use]
    pub fn has_changes(&self) -> bool {
        self.total_changes() != 0
    }

    /// Changed graph elements plus changed document fields (not a source-line count).
    #[must_use]
    pub fn total_changes(&self) -> usize {
        self.added_nodes
            + self.removed_nodes
            + self.changed_nodes
            + self.added_edges
            + self.removed_edges
            + self.changed_edges
            + self.diagram_changes.len()
    }

    fn structural_snapshot(&self, details: bool) -> structural::DiagramDiff {
        structural::DiagramDiff {
            nodes: if details { self.nodes.clone() } else { Vec::new() },
            edges: if details { self.edges.clone() } else { Vec::new() },
            added_nodes: self.added_nodes,
            removed_nodes: self.removed_nodes,
            changed_nodes: self.changed_nodes,
            unchanged_nodes: self.unchanged_nodes,
            added_edges: self.added_edges,
            removed_edges: self.removed_edges,
            changed_edges: self.changed_edges,
            unchanged_edges: self.unchanged_edges,
        }
    }
}

/// Compare graph structure and the semantic data consumed by specialized renderers.
#[must_use]
pub fn diff_diagrams(old: &MermaidDiagramIr, new: &MermaidDiagramIr) -> DiagramDiff {
    let structural::DiagramDiff {
        nodes,
        edges,
        added_nodes,
        removed_nodes,
        changed_nodes,
        unchanged_nodes,
        added_edges,
        removed_edges,
        changed_edges,
        unchanged_edges,
    } = structural::diff_diagrams(old, new);
    DiagramDiff {
        nodes,
        edges,
        added_nodes,
        removed_nodes,
        changed_nodes,
        unchanged_nodes,
        added_edges,
        removed_edges,
        changed_edges,
        unchanged_edges,
        diagram_changes: semantic::diff_metadata(old, new),
    }
}

/// Render node, edge, and document-field counts.
#[must_use]
pub fn render_diff_summary(diff: &DiagramDiff, use_colors: bool) -> String {
    let mut output = structural::render_diff_summary(&diff.structural_snapshot(false), use_colors);
    if !diff.diagram_changes.is_empty() {
        output.push_str("\nDiagram fields:\n");
        if use_colors {
            output.push_str(colors::CHANGED);
        }
        let _ = writeln!(output, "  ~ {} changed", diff.diagram_changes.len());
        if use_colors {
            output.push_str(colors::RESET);
        }
    }
    output
}

fn append_diagram_details(output: &mut String, diff: &DiagramDiff, use_colors: bool) {
    if diff.diagram_changes.is_empty() {
        return;
    }
    output.push_str("\nDiagram Details:\n");
    for change in &diff.diagram_changes {
        if use_colors {
            output.push_str(colors::CHANGED);
        }
        let _ = writeln!(output, "  ~ {}", change.field.escape_debug());
        if use_colors {
            output.push_str(colors::RESET);
        }
        // Values are Debug-escaped when collected. User labels cannot inject terminal
        // control sequences or impersonate a new report section through this channel.
        let _ = writeln!(output, "    before: {}", change.before);
        let _ = writeln!(output, "    after:  {}", change.after);
    }
}

/// Render an actionable report, including before/after document-field values.
#[must_use]
pub fn render_diff_plain(diff: &DiagramDiff) -> String {
    let snapshot = diff.structural_snapshot(true);
    let old_summary = structural::render_diff_summary(&snapshot, false);
    let mut output = structural::render_diff_plain(&snapshot).replacen(
        &old_summary,
        &render_diff_summary(diff, false),
        1,
    );
    append_diagram_details(&mut output, diff, false);
    output
}

/// Render a side-by-side terminal diff with the default rich configuration.
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

/// Render a side-by-side comparison without losing changes invisible at terminal resolution.
#[must_use]
pub fn render_diff_terminal_with_config(
    old: &MermaidDiagramIr,
    new: &MermaidDiagramIr,
    config: &TermRenderConfig,
    cols: usize,
    rows: usize,
    use_colors: bool,
) -> String {
    let diff = diff_diagrams(old, new);
    let old_summary = structural::render_diff_summary(&diff.structural_snapshot(false), use_colors);
    let mut output = structural::render_diff_terminal_with_config(
        old, new, config, cols, rows, use_colors,
    )
    .replacen(&old_summary, &render_diff_summary(&diff, use_colors), 1);
    append_diagram_details(&mut output, &diff, use_colors);
    output
}
