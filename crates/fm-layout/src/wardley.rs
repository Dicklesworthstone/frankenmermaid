//! `wardley-beta` layout: every piece of the map at the position the pinned mermaid 11.15.0
//! `wardleyRenderer` gives it.
//!
//! A Wardley map has no layout freedom — the author places every component by evolution (x) and
//! visibility (y) — so this is a projection, not a search:
//!
//! - the canvas is `size [w, h]` or 900 × 600 with a 48px padding, the chart is the rectangle
//!   inside it, evolution runs left to right and visibility bottom to top;
//! - stages are captioned under the chart and divided by dashed lines (the frame cluster's
//!   dividers), at their declared END boundaries when every stage has one, else evenly;
//! - a pipeline's box spans its members ±15px at their visibility, 24px tall, and its parent's
//!   square is re-centred above the box, as upstream moves it;
//! - labels sit 8px right of and 8px above their dot (18px with a sourcing ring) unless the
//!   author's `label [dx, dy]` says otherwise; anchors are centred captions;
//! - links run between dots, shortened by each end's radius, and an evolution trend stops 8px
//!   short of its target.
//!
//! Everything is an ordinary node box, edge route, cluster or cluster divider, so every backend
//! draws the map without a family-specific branch.

use std::sync::Arc;

use fm_core::{
    FontMetrics, IrEndpoint, IrWardleyMarkKind as Kind, IrWardleyMeta, MermaidDiagramIr,
};

use crate::{
    DiagramLayout, LayoutClusterBox, LayoutClusterDivider, LayoutEdgePath, LayoutExtensions,
    LayoutNodeBox, LayoutPoint, LayoutRect, LayoutStats, LayoutTrace, TracedLayout,
    compute_edge_length_metrics, display_node_label_ref, push_snapshot,
};

/// Upstream `wardley-beta.padding`.
const PADDING: f32 = 48.0;
/// Upstream `nodeRadius`; a pipeline parent's square is `1.6 ×` it.
const NODE_RADIUS: f32 = 6.0;
const SQUARE: f32 = NODE_RADIUS * 1.6;
/// Every backend fits a node label into its box minus 20 on each axis.
const LABEL_FIT_INSET: f32 = 20.0;

fn rect(x: f32, y: f32, width: f32, height: f32) -> LayoutRect {
    LayoutRect {
        x,
        y,
        width,
        height,
    }
}

/// Lay out a `wardley-beta` map.
#[must_use]
#[allow(clippy::too_many_lines)]
pub(crate) fn layout_diagram_wardley_traced(ir: &MermaidDiagramIr) -> TracedLayout {
    let mut trace = LayoutTrace::default();
    let metrics = FontMetrics::default_metrics();
    let empty = IrWardleyMeta {
        width: 900.0,
        height: 600.0,
        ..IrWardleyMeta::default()
    };
    let meta = ir.wardley_meta.as_deref().unwrap_or(&empty);
    let (width, height) = (
        meta.width.max(2.0 * PADDING + 1.0),
        meta.height.max(2.0 * PADDING + 1.0),
    );
    let chart_width = 2.0f32.mul_add(-PADDING, width);
    let chart_height = 2.0f32.mul_add(-PADDING, height);

    let mut positions: Vec<(f32, f32)> = meta
        .points
        .iter()
        .map(|p| {
            (
                (p.x / 100.0).mul_add(chart_width, PADDING),
                (p.y / 100.0).mul_add(-chart_height, height - PADDING),
            )
        })
        .collect();

    // Pipelines: the box around the members, and the parent's square re-centred above it.
    let mut clusters = Vec::with_capacity(meta.pipelines.len() + 1);
    let frame_span = ir
        .clusters
        .get(meta.frame_cluster)
        .map_or_else(Default::default, |c| c.span);
    clusters.push(LayoutClusterBox {
        cluster_index: meta.frame_cluster,
        span: frame_span,
        title: None,
        color: None,
        bounds: rect(PADDING, PADDING, chart_width, chart_height),
    });
    for pipeline in &meta.pipelines {
        let mut span: Option<(f32, f32, f32)> = None;
        for &member in &pipeline.members {
            if let Some(&(x, y)) = positions.get(member) {
                span = Some(span.map_or((x, x, y), |(lo, hi, _)| (lo.min(x), hi.max(x), y)));
            }
        }
        let Some((lo, hi, y)) = span else {
            continue;
        };
        let box_height = NODE_RADIUS * 4.0;
        let top = y - box_height / 2.0;
        if let Some(parent) = positions.get_mut(pipeline.point) {
            *parent = (f32::midpoint(lo, hi), top - SQUARE / 6.0);
        }
        clusters.push(LayoutClusterBox {
            cluster_index: pipeline.cluster,
            span: ir
                .clusters
                .get(pipeline.cluster)
                .map_or_else(Default::default, |c| c.span),
            title: None,
            color: None,
            bounds: rect(lo - 15.0, top, hi - lo + 30.0, box_height),
        });
    }

    let text_size = |node: usize| {
        let text = ir
            .nodes
            .get(node)
            .map_or("", |node| display_node_label_ref(ir, node));
        metrics.estimate_dimensions(text)
    };
    // A text box whose text is CENTRED on (cx, cy).
    let centred = |node: usize, cx: f32, cy: f32| {
        let (w, h) = text_size(node);
        let (w, h) = (w + LABEL_FIT_INSET, h + LABEL_FIT_INSET);
        rect(cx - w / 2.0, cy - h / 2.0, w, h)
    };
    // Renderers put a single line's baseline a third of the font size below the box centre.
    let baseline_lift = metrics.font_size() / 3.0;

    let mut boxes: Vec<Option<LayoutRect>> = vec![None; ir.nodes.len()];
    // Where an edge meets a node, and how far short of that point it stops.
    let mut ends: Vec<Option<(f32, f32, f32)>> = vec![None; ir.nodes.len()];
    for mark in &meta.marks {
        let Some(&(px, py)) = positions.get(mark.point) else {
            continue;
        };
        let (x, y) = (px + mark.dx, py + mark.dy);
        let around = |r: f32| rect(x - r, y - r, 2.0 * r, 2.0 * r);
        let (bounds, end) = match mark.kind {
            Kind::Dot | Kind::Market => (around(NODE_RADIUS), Some(NODE_RADIUS)),
            Kind::Square => (
                around(SQUARE / 2.0),
                Some(SQUARE / std::f32::consts::SQRT_2),
            ),
            Kind::Ring => (around(NODE_RADIUS * 2.0), None),
            Kind::Annotation => (around(10.0), None),
            Kind::Inertia => (
                rect(x - 3.0, py - NODE_RADIUS, 6.0, NODE_RADIUS * 2.0),
                None,
            ),
            Kind::Ghost => (around(0.0), Some(0.0)),
            Kind::Target => (around(0.0), Some(NODE_RADIUS + 2.0)),
            // An anchor's caption is its link endpoint, at the anchor's own point.
            Kind::Caption => (centred(mark.node, x, y), Some(NODE_RADIUS)),
            Kind::Label => {
                let (w, h) = text_size(mark.node);
                let (w, h) = (w + LABEL_FIT_INSET, h + LABEL_FIT_INSET);
                (
                    rect(x - LABEL_FIT_INSET / 2.0, y - baseline_lift - h / 2.0, w, h),
                    None,
                )
            }
            Kind::AnnotationsBox => {
                let (w, h) = text_size(mark.node);
                let (w, h) = (w + LABEL_FIT_INSET + 20.0, h + LABEL_FIT_INSET);
                let bx = x.clamp(PADDING, (width - PADDING - w).max(PADDING));
                let by = y.clamp(PADDING, (height - PADDING - h).max(PADDING));
                (rect(bx, by, w, h), None)
            }
        };
        if let Some(slot) = boxes.get_mut(mark.node) {
            *slot = Some(bounds);
        }
        if let (Some(radius), Some(slot)) = (end, ends.get_mut(mark.node)) {
            *slot = Some(if mark.kind == Kind::Caption {
                (px, py, radius)
            } else {
                (x, y, radius)
            });
        }
    }

    // Stage captions under the chart, axis captions beside it, and the stage dividers.
    let mut dividers = Vec::new();
    for (index, stage) in meta.stages.iter().enumerate() {
        let start_x = stage.start.mul_add(chart_width, PADDING);
        let end_x = stage.end.mul_add(chart_width, PADDING);
        if let Some(slot) = boxes.get_mut(stage.node) {
            *slot = Some(centred(
                stage.node,
                f32::midpoint(start_x, end_x),
                height - PADDING / 1.5,
            ));
        }
        if index > 0 {
            dividers.push(LayoutClusterDivider {
                cluster_index: meta.frame_cluster,
                start: LayoutPoint {
                    x: start_x,
                    y: PADDING,
                },
                end: LayoutPoint {
                    x: start_x,
                    y: height - PADDING,
                },
            });
        }
    }
    let [evolution, visibility] = meta.axis_nodes;
    if ir.nodes.len() > evolution.max(visibility) {
        boxes[evolution] = Some(centred(
            evolution,
            PADDING + chart_width / 2.0,
            height - PADDING / 4.0,
        ));
        // Upstream rotates this caption up the y axis; here it heads the axis, readable in every
        // backend, including the terminal.
        boxes[visibility] = Some(centred(visibility, PADDING, PADDING / 2.0));
    }
    push_snapshot(
        &mut trace,
        "wardley_marks",
        ir.nodes.len(),
        ir.edges.len(),
        0,
        0,
    );

    let nodes: Vec<LayoutNodeBox> = ir
        .nodes
        .iter()
        .enumerate()
        .map(|(node_index, node)| LayoutNodeBox {
            node_index,
            node_id: node.id.clone(),
            rank: 0,
            order: node_index,
            span: node.span_primary,
            bounds: boxes[node_index].unwrap_or_else(|| centred(node_index, PADDING, height)),
        })
        .collect();

    let centre_of = |endpoint: IrEndpoint| match endpoint {
        IrEndpoint::Node(node) => ends.get(node.0).copied().flatten().or_else(|| {
            nodes.get(node.0).map(|n| {
                let b = n.bounds;
                (b.x + b.width / 2.0, b.y + b.height / 2.0, 0.0)
            })
        }),
        _ => None,
    };
    let edges: Vec<LayoutEdgePath> = ir
        .edges
        .iter()
        .enumerate()
        .map(|(edge_index, edge)| {
            let points = match (centre_of(edge.from), centre_of(edge.to)) {
                (Some((x1, y1, r1)), Some((x2, y2, r2))) => {
                    let (dx, dy) = (x2 - x1, y2 - y1);
                    let length = dx.hypot(dy);
                    let (ux, uy) = if length > r1 + r2 {
                        (dx / length, dy / length)
                    } else {
                        (0.0, 0.0)
                    };
                    smallvec::smallvec![
                        LayoutPoint {
                            x: ux.mul_add(r1, x1),
                            y: uy.mul_add(r1, y1),
                        },
                        LayoutPoint {
                            x: ux.mul_add(-r2, x2),
                            y: uy.mul_add(-r2, y2),
                        },
                    ]
                }
                _ => smallvec::SmallVec::new(),
            };
            LayoutEdgePath {
                edge_index,
                span: edge.span,
                points,
                reversed: false,
                is_self_loop: false,
                parallel_offset: 0.0,
                bundle_count: 1,
                bundled: false,
            }
        })
        .collect();

    // The canvas, grown only if an author's label offset pushes text off it.
    let mut bounds = rect(0.0, 0.0, width, height);
    for node in &nodes {
        let b = node.bounds;
        let (x0, y0) = (bounds.x.min(b.x), bounds.y.min(b.y));
        let x1 = (bounds.x + bounds.width).max(b.x + b.width);
        let y1 = (bounds.y + bounds.height).max(b.y + b.height);
        bounds = rect(x0, y0, x1 - x0, y1 - y0);
    }
    let (total_edge_length, reversed_edge_total_length) = compute_edge_length_metrics(&edges);
    let stats = LayoutStats {
        node_count: ir.nodes.len(),
        edge_count: ir.edges.len(),
        total_edge_length,
        reversed_edge_total_length,
        phase_iterations: trace.snapshots.len(),
        ..LayoutStats::default()
    };
    TracedLayout {
        layout: Arc::new(DiagramLayout {
            nodes,
            clusters,
            cycle_clusters: Vec::new(),
            edges,
            bounds,
            stats,
            extensions: LayoutExtensions {
                cluster_dividers: dividers,
                ..LayoutExtensions::default()
            },
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
        let traced = layout_diagram_wardley_traced(&ir);
        (ir, Arc::unwrap_or_clone(traced.layout))
    }

    fn centre(layout: &DiagramLayout, node: usize) -> (f32, f32) {
        let b = layout.nodes[node].bounds;
        (b.x + b.width / 2.0, b.y + b.height / 2.0)
    }

    fn node_labelled(ir: &MermaidDiagramIr, label: &str) -> usize {
        ir.nodes
            .iter()
            .position(|node| display_node_label_ref(ir, node) == label)
            .unwrap_or_else(|| panic!("no node labelled {label}"))
    }

    /// Visibility 0.95 / evolution 0.63 projects into the 900 × 600 chart exactly as upstream's
    /// `projectX` / `projectY` do, and the label sits 8px right of and 8px above the dot.
    #[test]
    fn components_project_onto_the_chart() {
        let (ir, layout) = layout(
            "wardley-beta\ncomponent Tea [0.95, 0.63]\ncomponent Cup [0.2, 0.1] label [-30, 12]\n",
        );
        let meta = ir.wardley_meta.as_deref().expect("meta");
        let dot = meta
            .marks
            .iter()
            .find(|m| m.kind == Kind::Dot)
            .expect("a dot")
            .node;
        let (x, y) = centre(&layout, dot);
        assert!((x - 0.63f32.mul_add(804.0, 48.0)).abs() < 0.01, "{x}");
        assert!((y - (552.0 - 0.95 * 504.0)).abs() < 0.01, "{y}");
        let label = layout.nodes[node_labelled(&ir, "Tea")].bounds;
        assert!((label.x + LABEL_FIT_INSET / 2.0 - (x + 8.0)).abs() < 0.01);
        assert!(
            label.y + label.height / 2.0 < y,
            "the label sits above its dot"
        );
        // An explicit offset moves the text start.
        let cup = layout.nodes[node_labelled(&ir, "Cup")].bounds;
        let cup_dot = 0.1f32.mul_add(804.0, 48.0);
        assert!((cup.x + LABEL_FIT_INSET / 2.0 - (cup_dot - 30.0)).abs() < 0.01);
        // The canvas is upstream's 900 × 600; only the bottom caption's invisible fit inset may
        // reach past it.
        assert_eq!((layout.bounds.x, layout.bounds.y), (0.0, 0.0));
        assert_eq!(layout.bounds.width, 900.0);
        assert!(
            (600.0..=600.0 + LABEL_FIT_INSET).contains(&layout.bounds.height),
            "{}",
            layout.bounds.height
        );
    }

    /// Four default stages divide the chart evenly; declared END boundaries move the dividers.
    #[test]
    fn stages_divide_the_chart() {
        let (_, even) = layout("wardley-beta\ncomponent A [0.5, 0.5]\n");
        let xs: Vec<f32> = even
            .extensions
            .cluster_dividers
            .iter()
            .map(|d| d.start.x)
            .collect();
        assert_eq!(xs, [249.0, 450.0, 651.0]);

        let (_, bounded) = layout(
            "wardley-beta\nevolution Genesis@0.1 -> Custom@0.4 -> Product@0.7 -> Commodity@1.0\n",
        );
        let xs: Vec<f32> = bounded
            .extensions
            .cluster_dividers
            .iter()
            .map(|d| (d.start.x * 10.0).round() / 10.0)
            .collect();
        assert_eq!(xs, [128.4, 369.6, 610.8]);
    }

    /// A link runs between the two dots, stopping at each one's edge; a trend stops 8px short of
    /// its target.
    #[test]
    fn links_and_trends_stop_at_their_ends() {
        let (ir, layout) = layout(
            "wardley-beta\ncomponent A [0.5, 0.2]\ncomponent B [0.5, 0.8]\nA -> B\nevolve A 0.9\n",
        );
        assert_eq!(layout.edges.len(), 2);
        let link = &layout.edges[0].points;
        let dot_x = 0.2f32.mul_add(804.0, 48.0);
        assert!((link[0].x - (dot_x + NODE_RADIUS)).abs() < 0.01, "{link:?}");
        let trend = &layout.edges[1].points;
        let target_x = 0.9f32.mul_add(804.0, 48.0);
        assert!((trend[1].x - (target_x - 8.0)).abs() < 0.01, "{trend:?}");
        assert_eq!(ir.edges[1].arrow, fm_core::ArrowType::DottedArrow);
    }

    /// A pipeline's box spans its members, and its parent's square is re-centred above the box.
    #[test]
    fn pipeline_parent_sits_above_its_box() {
        let (ir, layout) = layout(
            "wardley-beta\ncomponent Kettle [0.45, 0.57]\npipeline Kettle {\n  component Campfire [0.35]\n  component Electric [0.75]\n}\n",
        );
        let meta = ir.wardley_meta.as_deref().expect("meta");
        let square = meta
            .marks
            .iter()
            .find(|m| m.kind == Kind::Square)
            .expect("the parent is a square")
            .node;
        let pipeline_box = layout
            .clusters
            .iter()
            .find(|c| c.cluster_index == meta.pipelines[0].cluster)
            .expect("pipeline box")
            .bounds;
        let (lo, hi) = (0.35f32.mul_add(804.0, 48.0), 0.75f32.mul_add(804.0, 48.0));
        assert!((pipeline_box.x - (lo - 15.0)).abs() < 0.01);
        assert!((pipeline_box.width - (hi - lo + 30.0)).abs() < 0.01);
        let (sx, sy) = centre(&layout, square);
        assert!((sx - f32::midpoint(lo, hi)).abs() < 0.01);
        assert!(sy < pipeline_box.y, "the square sits above the box");
        // The members' evolution link is drawn, dashed.
        assert!(
            ir.edges
                .iter()
                .any(|e| e.arrow == fm_core::ArrowType::DottedLine)
        );
    }
}
