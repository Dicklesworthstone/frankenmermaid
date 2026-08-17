//! GPU-ready primitive extraction from the shared diagram layout.
//!
//! A browser-side WebGPU encoder can upload these tightly packed instances directly:
//! node instances are suitable for an SDF shape shader and edge instances are suitable
//! for instanced line-segment rendering. Text remains a separate glyph-atlas pass.

use fm_core::{MermaidDiagramIr, NodeShape};
use fm_layout::{DiagramLayout, LayoutRect};

/// Shape discriminator consumed by an SDF node shader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum GpuNodeShape {
    Rect = 0,
    RoundedRect = 1,
    Circle = 2,
    Diamond = 3,
    Cylinder = 4,
    Polygon = 5,
}

impl From<NodeShape> for GpuNodeShape {
    fn from(shape: NodeShape) -> Self {
        match shape {
            NodeShape::Rect | NodeShape::Note | NodeShape::HorizontalBar => Self::Rect,
            NodeShape::Rounded | NodeShape::Stadium | NodeShape::Subroutine => Self::RoundedRect,
            NodeShape::Circle | NodeShape::FilledCircle | NodeShape::DoubleCircle => Self::Circle,
            NodeShape::Diamond => Self::Diamond,
            NodeShape::Cylinder => Self::Cylinder,
            NodeShape::Hexagon
            | NodeShape::Asymmetric
            | NodeShape::Trapezoid
            | NodeShape::InvTrapezoid
            | NodeShape::Parallelogram
            | NodeShape::InvParallelogram
            | NodeShape::Triangle
            | NodeShape::Pentagon
            | NodeShape::Star
            | NodeShape::Cloud
            | NodeShape::Tag
            | NodeShape::CrossedCircle => Self::Polygon,
        }
    }
}

/// One node instance for a WebGPU SDF shape pass.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct GpuNodeInstance {
    /// Center position in layout coordinates.
    pub center: [f32; 2],
    /// Half extents in layout coordinates.
    pub half_extent: [f32; 2],
    /// [`GpuNodeShape`] encoded as a shader-friendly integer.
    pub shape: u32,
    /// Index back into `MermaidDiagramIr::nodes` for styling and labels.
    pub node_index: u32,
}

/// One edge segment for an instanced WebGPU line pass.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct GpuEdgeSegment {
    pub from: [f32; 2],
    pub to: [f32; 2],
    /// Index back into `MermaidDiagramIr::edges`.
    pub edge_index: u32,
    /// Stroke width in layout units, from the SAME `legacy_edge_stroke` rule the Canvas2D pass
    /// uses — bd-2u0.2 calls for "instanced line strips with VARIABLE WIDTH", and a plan that
    /// assumed one width could not draw a `==>` thick edge or a dotted one correctly.
    pub width: f32,
}

/// One arrowhead for an instanced triangle pass.
///
/// bd-2u0.2: "Arrowheads as small triangle instances". Geometry mirrors the Canvas2D pass exactly —
/// the END head sits on the last point with the angle of the final segment, and a BIDIRECTIONAL
/// edge additionally gets a START head on the first point facing the other way.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct GpuArrowheadInstance {
    /// Tip position in layout coordinates.
    pub position: [f32; 2],
    /// Facing, in radians, matching `atan2` of the segment it terminates.
    pub angle: f32,
    /// Edge length of the head, in layout units.
    pub size: f32,
    /// Index back into `MermaidDiagramIr::edges`.
    pub edge_index: u32,
}

/// Deterministic primitive buffers for a future WebGPU command encoder.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuRenderPlan {
    pub bounds: LayoutRect,
    pub node_instances: Vec<GpuNodeInstance>,
    pub edge_segments: Vec<GpuEdgeSegment>,
    /// Triangle instances for edge arrowheads.
    pub arrowheads: Vec<GpuArrowheadInstance>,
}


/// Whether this arrow type terminates in a head at the TARGET end.
///
/// `Line`, `DottedLine` and `ThickLine` are undirected — giving them a head would assert a direction
/// the author did not write, which is the ER-notation defect bd-m0a9 fixed in the SVG renderer.
const fn arrow_has_end_head(arrow: fm_core::ArrowType) -> bool {
    !matches!(
        arrow,
        fm_core::ArrowType::Line | fm_core::ArrowType::DottedLine | fm_core::ArrowType::ThickLine
    )
}

impl GpuRenderPlan {
    /// Build GPU-uploadable primitives without changing layout or render ordering.
    ///
    /// `edge_stroke_width` is the Canvas2D config's default, so a plan and a raster render of the
    /// same diagram agree on stroke width rather than each inventing one.
    #[must_use]
    pub fn from_layout(
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        edge_stroke_width: f32,
    ) -> Self {
        let mut node_instances = Vec::with_capacity(layout.nodes.len());
        for node in &layout.nodes {
            let shape = ir
                .nodes
                .get(node.node_index)
                .map_or(GpuNodeShape::Rect, |ir_node| ir_node.shape.into());
            node_instances.push(GpuNodeInstance {
                center: [
                    node.bounds.x + (node.bounds.width * 0.5),
                    node.bounds.y + (node.bounds.height * 0.5),
                ],
                half_extent: [node.bounds.width * 0.5, node.bounds.height * 0.5],
                shape: shape as u32,
                node_index: node.node_index.try_into().unwrap_or(u32::MAX),
            });
        }

        let segment_count = layout
            .edges
            .iter()
            .filter(|edge| !edge.bundled)
            .map(|edge| edge.points.len().saturating_sub(1))
            .sum();
        let mut edge_segments = Vec::with_capacity(segment_count);
        let mut arrowheads = Vec::new();
        for edge in layout.edges.iter().filter(|edge| !edge.bundled) {
            let arrow = ir
                .edges
                .get(edge.edge_index)
                .map_or(fm_core::ArrowType::Arrow, |ir_edge| ir_edge.arrow);
            // ONE source of truth with the raster pass: `legacy_edge_stroke` is the same function
            // `draw_edges` strokes with, so a `==>` thick edge and a dotted edge carry the widths
            // they are actually drawn at instead of a plan-local guess.
            let (width, _dash) =
                crate::renderer::legacy_edge_stroke(arrow, f64::from(edge_stroke_width));
            let width = width as f32;
            let edge_index = edge.edge_index.try_into().unwrap_or(u32::MAX);

            for points in edge.points.windows(2) {
                let [from, to] = points else {
                    continue;
                };
                edge_segments.push(GpuEdgeSegment {
                    from: [from.x, from.y],
                    to: [to.x, to.y],
                    edge_index,
                    width,
                });
            }

            // Arrowheads mirror the Canvas2D geometry exactly: the END head on the last point,
            // angled along the final segment; a BIDIRECTIONAL edge also gets a START head on the
            // first point, facing back the way it came.
            const HEAD_SIZE: f32 = 10.0;
            if edge.points.len() >= 2 && arrow_has_end_head(arrow) {
                let last = edge.points[edge.points.len() - 1];
                let prev = edge.points[edge.points.len() - 2];
                arrowheads.push(GpuArrowheadInstance {
                    position: [last.x, last.y],
                    angle: (last.y - prev.y).atan2(last.x - prev.x),
                    size: HEAD_SIZE,
                    edge_index,
                });
            }
            if edge.points.len() >= 2 && matches!(arrow, fm_core::ArrowType::DoubleArrow) {
                let start = edge.points[0];
                let next = edge.points[1];
                arrowheads.push(GpuArrowheadInstance {
                    position: [start.x, start.y],
                    angle: (start.y - next.y).atan2(start.x - next.x),
                    size: HEAD_SIZE,
                    edge_index,
                });
            }
        }

        Self {
            bounds: layout.bounds,
            node_instances,
            edge_segments,
            arrowheads,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GpuNodeShape, GpuRenderPlan};
    use fm_core::{DiagramType, IrNode, MermaidDiagramIr, NodeShape, Span};
    use fm_layout::{
        DiagramLayout, EdgePoints, LayoutEdgePath, LayoutExtensions, LayoutNodeBox, LayoutPoint,
        LayoutRect, LayoutStats,
    };

    /// An IR whose edge sits at index 7, matching `test_layout()`'s `edge_index`.
    ///
    /// The shared fixture uses 7 deliberately — it proves the index is carried through rather than
    /// assumed to be 0. My first version of these tests put the edge at index 0, so the arrow lookup
    /// missed and every edge silently fell back to `ArrowType::Arrow` at the default width. The
    /// tests failed loudly, which is the only reason the mismatch did not become the "expected"
    /// values baked into an assertion.
    fn ir_with_edge_at_fixture_index(arrow: fm_core::ArrowType) -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(fm_core::DiagramType::Flowchart);
        for _ in 0..7 {
            ir.edges.push(fm_core::IrEdge::default());
        }
        ir.edges.push(fm_core::IrEdge {
            from: fm_core::IrEndpoint::Node(fm_core::IrNodeId(0)),
            to: fm_core::IrEndpoint::Node(fm_core::IrNodeId(1)),
            arrow,
            ..fm_core::IrEdge::default()
        });
        ir
    }

    fn test_layout() -> DiagramLayout {
        DiagramLayout {
            nodes: vec![
                LayoutNodeBox {
                    node_index: 0,
                    node_id: String::from("a"),
                    rank: 0,
                    order: 0,
                    span: Span::default(),
                    bounds: LayoutRect {
                        x: 10.0,
                        y: 20.0,
                        width: 40.0,
                        height: 30.0,
                    },
                },
                LayoutNodeBox {
                    node_index: 1,
                    node_id: String::from("b"),
                    rank: 1,
                    order: 0,
                    span: Span::default(),
                    bounds: LayoutRect {
                        x: 90.0,
                        y: 20.0,
                        width: 20.0,
                        height: 20.0,
                    },
                },
            ],
            clusters: Vec::new(),
            cycle_clusters: Vec::new(),
            edges: vec![
                LayoutEdgePath {
                    edge_index: 7,
                    span: Span::default(),
                    points: EdgePoints::from_vec(vec![
                        LayoutPoint { x: 50.0, y: 35.0 },
                        LayoutPoint { x: 70.0, y: 35.0 },
                        LayoutPoint { x: 90.0, y: 30.0 },
                    ]),
                    reversed: false,
                    is_self_loop: false,
                    parallel_offset: 0.0,
                    bundle_count: 1,
                    bundled: false,
                },
                LayoutEdgePath {
                    edge_index: 8,
                    span: Span::default(),
                    points: EdgePoints::from_vec(vec![
                        LayoutPoint { x: 0.0, y: 0.0 },
                        LayoutPoint { x: 1.0, y: 1.0 },
                    ]),
                    reversed: false,
                    is_self_loop: false,
                    parallel_offset: 0.0,
                    bundle_count: 2,
                    bundled: true,
                },
            ],
            bounds: LayoutRect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 40.0,
            },
            stats: LayoutStats::default(),
            extensions: LayoutExtensions::default(),
            dirty_regions: Vec::new(),
        }
    }

    #[test]
    fn plan_preserves_node_geometry_and_shape_discriminators() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes = vec![
            IrNode {
                id: String::from("a"),
                shape: NodeShape::Rounded,
                ..IrNode::default()
            },
            IrNode {
                id: String::from("b"),
                shape: NodeShape::Cylinder,
                ..IrNode::default()
            },
        ];

        let plan = GpuRenderPlan::from_layout(&ir, &test_layout(), 1.25);

        assert_eq!(plan.node_instances.len(), 2);
        assert_eq!(plan.node_instances[0].center, [30.0, 35.0]);
        assert_eq!(plan.node_instances[0].half_extent, [20.0, 15.0]);
        assert_eq!(
            plan.node_instances[0].shape,
            GpuNodeShape::RoundedRect as u32
        );
        assert_eq!(plan.node_instances[1].shape, GpuNodeShape::Cylinder as u32);
    }

    #[test]
    fn plan_expands_only_visible_edge_polylines_into_segments() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let plan = GpuRenderPlan::from_layout(&ir, &test_layout(), 1.25);

        assert_eq!(plan.edge_segments.len(), 2);
        assert_eq!(plan.edge_segments[0].from, [50.0, 35.0]);
        assert_eq!(plan.edge_segments[1].to, [90.0, 30.0]);
        assert!(
            plan.edge_segments
                .iter()
                .all(|segment| segment.edge_index == 7)
        );
    }

    /// Edge width comes from the SAME rule the raster pass strokes with (bd-2u0.2).
    ///
    /// bd-2u0.2 asks for "instanced line strips with VARIABLE WIDTH". A plan that assumed one width
    /// could not draw a `==>` thick edge or a dotted one correctly. The widths are not hard-coded
    /// here — they are compared against `legacy_edge_stroke`, which is what `draw_edges` calls, so
    /// this fails if the two ever diverge instead of quietly drifting.
    #[test]
    fn edge_segment_width_matches_the_raster_stroke_rule() {
        const DEFAULT_WIDTH: f32 = 1.25;
        for arrow in [
            fm_core::ArrowType::Arrow,
            fm_core::ArrowType::ThickArrow,
            fm_core::ArrowType::DottedArrow,
        ] {
            let ir = ir_with_edge_at_fixture_index(arrow);
            let plan = GpuRenderPlan::from_layout(&ir, &test_layout(), DEFAULT_WIDTH);
            let (expected, _dash) =
                crate::renderer::legacy_edge_stroke(arrow, f64::from(DEFAULT_WIDTH));

            assert!(
                !plan.edge_segments.is_empty(),
                "{arrow:?}: no segments, so the width assertion would be vacuous"
            );
            for segment in &plan.edge_segments {
                assert!(
                    (f64::from(segment.width) - expected).abs() < 1e-6,
                    "{arrow:?}: plan width {} does not match the raster rule {expected}",
                    segment.width
                );
            }
        }
    }

    /// A directed edge gets ONE head; a bidirectional edge gets TWO; an undirected line gets NONE.
    ///
    /// The last is the point of the `arrow_has_end_head` rule: giving `---` a head would assert a
    /// direction the author never wrote, which is exactly the ER-notation defect bd-m0a9 fixed in
    /// the SVG renderer.
    #[test]
    fn arrowhead_count_follows_the_arrow_type() {
        for (arrow, expected) in [
            (fm_core::ArrowType::Arrow, 1_usize),
            (fm_core::ArrowType::DoubleArrow, 2),
            (fm_core::ArrowType::Line, 0),
            (fm_core::ArrowType::DottedLine, 0),
        ] {
            let ir = ir_with_edge_at_fixture_index(arrow);
            let plan = GpuRenderPlan::from_layout(&ir, &test_layout(), 1.25);
            assert_eq!(
                plan.arrowheads.len(),
                expected,
                "{arrow:?} should produce {expected} arrowhead(s), got {:?}",
                plan.arrowheads
            );
        }
    }

    /// The head sits on the edge's LAST point, angled along its final segment.
    ///
    /// Position and angle are compared against the layout's own points rather than to constants, so
    /// the assertion still means something if the fixture's geometry changes.
    #[test]
    fn arrowhead_geometry_matches_the_final_segment() {
        let ir = ir_with_edge_at_fixture_index(fm_core::ArrowType::Arrow);
        let layout = test_layout();
        let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.25);
        let head = plan.arrowheads.first().expect("a directed edge must have a head");

        let points = &layout.edges[0].points;
        let last = points[points.len() - 1];
        let prev = points[points.len() - 2];

        assert!(
            (head.position[0] - last.x).abs() < 1e-6 && (head.position[1] - last.y).abs() < 1e-6,
            "the head is not on the edge's last point: {head:?} vs ({}, {})",
            last.x,
            last.y
        );
        let expected_angle = (last.y - prev.y).atan2(last.x - prev.x);
        assert!(
            (head.angle - expected_angle).abs() < 1e-6,
            "the head angle {} does not follow the final segment {expected_angle}",
            head.angle
        );
    }
}
