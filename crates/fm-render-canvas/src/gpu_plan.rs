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
}

/// Deterministic primitive buffers for a future WebGPU command encoder.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuRenderPlan {
    pub bounds: LayoutRect,
    pub node_instances: Vec<GpuNodeInstance>,
    pub edge_segments: Vec<GpuEdgeSegment>,
}

impl GpuRenderPlan {
    /// Build GPU-uploadable primitives without changing layout or render ordering.
    #[must_use]
    pub fn from_layout(ir: &MermaidDiagramIr, layout: &DiagramLayout) -> Self {
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
        for edge in layout.edges.iter().filter(|edge| !edge.bundled) {
            for points in edge.points.windows(2) {
                let [from, to] = points else {
                    continue;
                };
                edge_segments.push(GpuEdgeSegment {
                    from: [from.x, from.y],
                    to: [to.x, to.y],
                    edge_index: edge.edge_index.try_into().unwrap_or(u32::MAX),
                });
            }
        }

        Self {
            bounds: layout.bounds,
            node_instances,
            edge_segments,
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

        let plan = GpuRenderPlan::from_layout(&ir, &test_layout());

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
        let plan = GpuRenderPlan::from_layout(&ir, &test_layout());

        assert_eq!(plan.edge_segments.len(), 2);
        assert_eq!(plan.edge_segments[0].from, [50.0, 35.0]);
        assert_eq!(plan.edge_segments[1].to, [90.0, 30.0]);
        assert!(
            plan.edge_segments
                .iter()
                .all(|segment| segment.edge_index == 7)
        );
    }
}
