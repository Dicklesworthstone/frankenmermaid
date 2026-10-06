//! `treeView-beta` layout: a file-explorer style indented tree.
//!
//! The geometry follows the pinned mermaid 11.15.0 `treeView` renderer: one row per node in
//! pre-order, each level indented by `rowIndent + paddingX`, a short horizontal connector into every
//! row and a vertical trunk from each parent down to the middle of its last child's row. The
//! implicit `/` root that upstream always draws is a real node in the IR, so it is laid out here like
//! any other row.
//!
//! As with the other specialised layouts, the result is ordinary node boxes and edge polylines: each
//! parent → child edge is the elbow "down the trunk, then across into the row", and the trunks of
//! siblings coincide, so every backend draws the tree without a family-specific branch.

use std::sync::Arc;

use fm_core::{FontMetrics, IrEndpoint, MermaidDiagramIr};

use crate::{
    DiagramLayout, LayoutEdgePath, LayoutExtensions, LayoutNodeBox, LayoutPoint, LayoutRect,
    LayoutSpacing, LayoutStats, LayoutTrace, TracedLayout, compute_bounds,
    compute_edge_length_metrics, display_node_label_ref, push_snapshot,
};

/// Upstream `treeView.rowIndent`: the horizontal connector's length.
const ROW_INDENT: f32 = 10.0;
/// Upstream `treeView.paddingX`: gap between a row's left edge and its text.
const PADDING_X: f32 = 5.0;
/// Upstream `treeView.paddingY`: vertical padding above and below a row's text.
const PADDING_Y: f32 = 5.0;
/// Extra room between a connector's end and its row's text.
const TEXT_NUDGE: f32 = 3.0;
/// Every backend fits a node label into its box minus 20 on each axis; text boxes are padded to
/// match so no label is shrunk.
const LABEL_FIT_INSET: f32 = 20.0;

fn point(x: f32, y: f32) -> LayoutPoint {
    LayoutPoint { x, y }
}

/// Lay out a `treeView-beta` diagram.
#[must_use]
pub(crate) fn layout_diagram_tree_view_traced(ir: &MermaidDiagramIr) -> TracedLayout {
    let mut trace = LayoutTrace::default();
    let metrics = FontMetrics::default_metrics();
    let spacing = LayoutSpacing::default();
    let node_count = ir.nodes.len();

    // parent → children in edge (= document) order; each node keeps the first edge that reaches it.
    let mut children: Vec<Vec<(usize, usize)>> = vec![Vec::new(); node_count];
    let mut has_parent = vec![false; node_count];
    for (edge_index, edge) in ir.edges.iter().enumerate() {
        let (IrEndpoint::Node(parent), IrEndpoint::Node(child)) = (edge.from, edge.to) else {
            continue;
        };
        if parent.0 >= node_count || child.0 >= node_count || parent == child || has_parent[child.0]
        {
            continue;
        }
        has_parent[child.0] = true;
        children[parent.0].push((child.0, edge_index));
    }

    let mut boxes: Vec<Option<LayoutRect>> = vec![None; node_count];
    let mut depths = vec![0_usize; node_count];
    // Row geometry per node: (row top, row height, row left = indent).
    let mut rows: Vec<Option<(f32, f32, f32)>> = vec![None; node_count];
    let mut routes: Vec<Option<crate::EdgePoints>> = vec![None; ir.edges.len()];
    let mut cursor_y = 0.0_f32;

    // Pre-order walk over every root (a hand-built IR may hold several), iterative because a tree
    // is untrusted input and can be arbitrarily deep.
    let mut visited = vec![false; node_count];
    for root in (0..node_count).filter(|&node| !has_parent[node]) {
        let mut stack = vec![(root, 0_usize)];
        while let Some((node, depth)) = stack.pop() {
            if std::mem::replace(&mut visited[node], true) {
                continue;
            }
            let text = display_node_label_ref(ir, &ir.nodes[node]);
            let (text_width, text_height) = metrics.estimate_dimensions(text);
            #[allow(clippy::cast_precision_loss)]
            let indent = depth as f32 * (ROW_INDENT + PADDING_X);
            let row_height = PADDING_Y.mul_add(2.0, text_height);
            let center_y = cursor_y + row_height / 2.0;
            // Text starts `TEXT_NUDGE` past upstream's padding: the label is drawn centred on its
            // measured box, so an underestimated width would otherwise spill back over the
            // connector's end.
            let text_left = indent + PADDING_X + TEXT_NUDGE;
            boxes[node] = Some(LayoutRect {
                x: text_left - LABEL_FIT_INSET / 2.0,
                y: center_y - (text_height + LABEL_FIT_INSET) / 2.0,
                width: text_width + LABEL_FIT_INSET,
                height: text_height + LABEL_FIT_INSET,
            });
            rows[node] = Some((cursor_y, row_height, indent));
            depths[node] = depth;
            cursor_y += row_height;
            for &(child, _) in children[node].iter().rev() {
                stack.push((child, depth + 1));
            }
        }
    }
    push_snapshot(
        &mut trace,
        "tree_view_rows",
        node_count,
        ir.edges.len(),
        0,
        0,
    );

    // Connectors: down the parent's trunk, then across into the child's row.
    for (parent, kids) in children.iter().enumerate() {
        let Some((parent_top, parent_height, parent_indent)) = rows[parent] else {
            continue;
        };
        let trunk_x = parent_indent + PADDING_X;
        let trunk_top = parent_top + parent_height;
        for &(child, edge_index) in kids {
            let Some((child_top, child_height, child_indent)) = rows[child] else {
                continue;
            };
            let row_middle = child_top + child_height / 2.0;
            let corner = point(trunk_x, row_middle);
            // ⚠️ THE CORNER IS WRITTEN TWICE ON PURPOSE: the SVG backend smooths polylines of three
            // or more points, and a repeated vertex keeps this elbow square, as a tree view's must be.
            routes[edge_index] = Some(smallvec::smallvec![
                point(trunk_x, trunk_top),
                corner,
                corner,
                point(child_indent, row_middle),
            ]);
        }
    }

    let mut nodes = Vec::with_capacity(node_count);
    for (node_index, node) in ir.nodes.iter().enumerate() {
        let bounds = boxes[node_index].unwrap_or(LayoutRect {
            x: 0.0,
            y: cursor_y,
            width: LABEL_FIT_INSET,
            height: LABEL_FIT_INSET,
        });
        nodes.push(LayoutNodeBox {
            node_index,
            node_id: node.id.clone(),
            rank: depths[node_index],
            order: node_index,
            span: node.span_primary,
            bounds,
        });
    }
    let mut edges = Vec::with_capacity(ir.edges.len());
    for (edge_index, edge) in ir.edges.iter().enumerate() {
        let points = routes[edge_index].take().unwrap_or_else(|| {
            let center = |endpoint: IrEndpoint| match endpoint {
                IrEndpoint::Node(node) => nodes.get(node.0).map_or(point(0.0, 0.0), |node| {
                    point(
                        node.bounds.x + node.bounds.width / 2.0,
                        node.bounds.y + node.bounds.height / 2.0,
                    )
                }),
                _ => point(0.0, 0.0),
            };
            smallvec::smallvec![center(edge.from), center(edge.to)]
        });
        edges.push(LayoutEdgePath {
            edge_index,
            span: edge.span,
            points,
            reversed: false,
            is_self_loop: false,
            parallel_offset: 0.0,
            bundle_count: 1,
            bundled: false,
        });
    }

    // Anchor the tree's top-left at the origin plus the usual padding; the root's text box starts
    // left of x = 0 by its own fit inset.
    let raw = compute_bounds(&nodes, &[], &edges, spacing);
    for node in &mut nodes {
        node.bounds.x -= raw.x;
        node.bounds.y -= raw.y;
    }
    for edge in &mut edges {
        for p in &mut edge.points {
            p.x -= raw.x;
            p.y -= raw.y;
        }
    }
    let bounds = compute_bounds(&nodes, &[], &edges, spacing);
    let (total_edge_length, reversed_edge_total_length) = compute_edge_length_metrics(&edges);
    let stats = LayoutStats {
        node_count,
        edge_count: ir.edges.len(),
        total_edge_length,
        reversed_edge_total_length,
        phase_iterations: trace.snapshots.len(),
        ..LayoutStats::default()
    };
    TracedLayout {
        layout: Arc::new(DiagramLayout {
            nodes,
            clusters: Vec::new(),
            cycle_clusters: Vec::new(),
            edges,
            bounds,
            stats,
            extensions: LayoutExtensions::default(),
            dirty_regions: Vec::new(),
        }),
        trace,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(source: &str) -> (MermaidDiagramIr, DiagramLayout) {
        let ir = fm_parser::parse(source).ir;
        let traced = layout_diagram_tree_view_traced(&ir);
        (ir, Arc::unwrap_or_clone(traced.layout))
    }

    fn text_left(layout: &DiagramLayout, index: usize) -> f32 {
        layout.nodes[index].bounds.x + LABEL_FIT_INSET / 2.0
    }

    fn row_middle(layout: &DiagramLayout, index: usize) -> f32 {
        let b = layout.nodes[index].bounds;
        b.y + b.height / 2.0
    }

    #[test]
    fn rows_follow_preorder_and_indent_by_depth() {
        let (ir, layout) =
            layout("treeView-beta\n  \"src\"\n    \"main.rs\"\n    \"lib.rs\"\n  \"Cargo.toml\"\n");
        let names: Vec<&str> = ir.nodes.iter().map(|n| ir.node_display_text(n)).collect();
        assert_eq!(names, ["/", "src", "main.rs", "lib.rs", "Cargo.toml"]);
        // Pre-order rows: every node sits below the one declared before it.
        for index in 1..layout.nodes.len() {
            assert!(row_middle(&layout, index) > row_middle(&layout, index - 1));
        }
        let step = ROW_INDENT + PADDING_X;
        assert!((text_left(&layout, 1) - text_left(&layout, 0) - step).abs() < 0.01);
        assert!((text_left(&layout, 2) - text_left(&layout, 1) - step).abs() < 0.01);
        assert!((text_left(&layout, 4) - text_left(&layout, 1)).abs() < 0.01);
    }

    #[test]
    fn every_connector_is_a_square_elbow_from_the_parent_trunk() {
        let (ir, layout) = layout("treeView-beta\n  \"a\"\n    \"b\"\n  \"c\"\n");
        for edge in &layout.edges {
            let points = &edge.points;
            assert_eq!(points.len(), 4);
            // Down the trunk (vertical), then across into the row (horizontal).
            assert!((points[0].x - points[1].x).abs() < 0.01);
            assert!((points[2].y - points[3].y).abs() < 0.01);
            assert!(points[3].x > points[2].x);
            let IrEndpoint::Node(child) = ir.edges[edge.edge_index].to else {
                panic!("tree edges join nodes");
            };
            assert!((points[3].y - row_middle(&layout, child.0)).abs() < 0.01);
        }
    }

    #[test]
    fn deterministic_and_inside_bounds() {
        let source = "treeView-beta\n  \"x\"\n    \"y\"\n      \"z\"\n";
        let (_, first) = layout(source);
        let (_, second) = layout(source);
        assert_eq!(first, second);
        for node in &first.nodes {
            assert!(node.bounds.x >= first.bounds.x - 0.01);
            assert!(node.bounds.y >= first.bounds.y - 0.01);
            assert!(
                node.bounds.x + node.bounds.width <= first.bounds.x + first.bounds.width + 0.01
            );
        }
    }

    #[test]
    fn an_empty_tree_still_draws_its_root() {
        let (_, layout) = layout("treeView-beta\n");
        assert_eq!(layout.nodes.len(), 1);
        assert!(layout.edges.is_empty());
    }

    #[test]
    fn a_very_deep_tree_does_not_overflow_the_stack() {
        let mut source = String::from("treeView-beta\n");
        for depth in 1..=3_000 {
            source.push_str(&" ".repeat(depth));
            source.push_str("\"d\"\n");
        }
        let (ir, layout) = layout(&source);
        assert_eq!(layout.nodes.len(), ir.nodes.len());
    }
}
