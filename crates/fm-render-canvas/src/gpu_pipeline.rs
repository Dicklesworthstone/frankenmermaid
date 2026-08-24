//! Mapping from a [`GpuRenderPlan`](crate::gpu_plan::GpuRenderPlan) to WebGPU pipeline state.
//!
//! `gpu_plan` answers *what to draw*: it turns a `DiagramLayout` into instance buffers and carries
//! the WGSL each family is drawn with. It says nothing about *how those bytes reach the shader* —
//! the stride, the per-attribute byte offsets, and which `@location` each one binds to. That gap is
//! where GPU backends fail silently, so it is described here as data rather than being open-coded
//! at the call site that eventually builds a `wgpu::RenderPipeline`.
//!
//! **Why this is a separate module and not three lines inside a device wrapper.** A vertex layout
//! that disagrees with its shader does not fail to compile, does not panic, and does not error at
//! draw time. It renders *something* — the wrong bytes reinterpreted as the right types — which is
//! the most expensive class of bug to find by looking at output. Describing the layout as data
//! makes it testable without a GPU, an adapter, or a browser, which is what
//! `tests/gpu_pipeline_layout.rs` does against the shader source itself.
//!
//! **The trap this module exists to prevent, stated concretely.** `GpuEdgeSegment`'s Rust fields are
//! declared `… color, dash_phase, dash, width`, while `EDGE_WGSL` binds `@location(3) color`,
//! `@location(4) dash`, `@location(5) width`, `@location(6) dash_phase`. The declaration orders do
//! **not** agree. A layout built by walking the struct's fields and handing out locations `0..n` in
//! order would bind `dash_phase` to the shader's `dash`, and every dotted edge would draw with a
//! dash pattern read out of an accumulated arc length. Offsets are therefore taken from
//! [`core::mem::offset_of`] against the real struct and matched to locations **by name**, so neither
//! side can be reordered into disagreement without the test noticing.
//!
//! Deliberately free of any `wgpu` dependency: these are plain descriptions a caller translates into
//! `wgpu::VertexBufferLayout` (or `GPUVertexBufferLayout` in the browser) in a handful of lines. The
//! device layer is bd-2u0.2's next step; this is the part that can be proven correct headlessly.

use crate::gpu_plan::{
    ARROWHEAD_WGSL, EDGE_WGSL, GpuArrowheadInstance, GpuEdgeSegment, GpuNodeInstance,
    GpuRenderPlan, GpuTextQuad, NODE_SDF_WGSL, TEXT_ATLAS_WGSL,
};

/// Vertex attribute scalar/vector type, spelled the way WGSL spells it.
///
/// Only the formats the diagram shaders actually use. A format that no shader binds would be dead
/// surface that the parity test could not exercise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VertexFormat {
    /// WGSL `f32`.
    Float32,
    /// WGSL `vec2<f32>`.
    Float32x2,
    /// WGSL `vec4<f32>`.
    Float32x4,
    /// WGSL `u32`.
    Uint32,
}

impl VertexFormat {
    /// Size in bytes, used to bounds-check every attribute against the instance stride.
    #[must_use]
    pub const fn size(self) -> u64 {
        match self {
            Self::Float32 | Self::Uint32 => 4,
            Self::Float32x2 => 8,
            Self::Float32x4 => 16,
        }
    }

    /// The WGSL spelling, so the parity test compares against the shader's own words rather than
    /// against a second table that could drift from it.
    #[must_use]
    pub const fn wgsl_type(self) -> &'static str {
        match self {
            Self::Float32 => "f32",
            Self::Float32x2 => "vec2<f32>",
            Self::Float32x4 => "vec4<f32>",
            Self::Uint32 => "u32",
        }
    }
}

/// One instance-buffer attribute: where the bytes are, what they are, and which `@location` in the
/// shader consumes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VertexAttribute {
    /// The shader's `@location(N)`.
    pub shader_location: u32,
    /// Byte offset within one instance, from `offset_of!` on the real struct.
    pub offset: u64,
    pub format: VertexFormat,
    /// Field name, identical on both sides. This is what makes the binding order-independent: the
    /// test pairs Rust attribute to shader location by NAME, so a reordering on either side is a
    /// mismatch rather than a silent re-binding.
    pub name: &'static str,
}

/// The instance step-mode buffer layout for one primitive family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstanceLayout {
    /// `size_of` one instance. Every attribute must fit inside this.
    pub array_stride: u64,
    pub attributes: &'static [VertexAttribute],
}

/// Which primitive family a pipeline draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveFamily {
    /// Node boxes, drawn as SDF quads.
    Node,
    /// Edge segments, drawn as instanced quads expanded across the line's width.
    Edge,
    /// Path-end markers, drawn as instanced quads so the shader can represent UML endpoint forms.
    Arrowhead,
    /// Dashed node borders, drawn with the EDGE pipeline but after the nodes (bd-l3nsf).
    ///
    /// A separate family rather than more `Edge` instances because the two occupy different slots in
    /// the draw order: edges go under the boxes they connect, a border goes on top of the box it
    /// outlines. Same shader, same layout, different moment.
    NodeBorder,
    /// Glyph quads sampled from the text atlas.
    Text,
}

/// Everything a caller needs to build one render pipeline, minus the device.
#[derive(Debug, Clone, Copy)]
pub struct PipelineDescriptor {
    /// Stable debug label; shows up in GPU captures and validation messages.
    pub label: &'static str,
    pub family: PrimitiveFamily,
    /// The shader this layout is bound to. Carried by reference to the `gpu_plan` constant rather
    /// than copied, so there is exactly one shader source in the crate.
    pub wgsl: &'static str,
    pub instance: InstanceLayout,
    /// Vertices the vertex stage generates per instance. The shaders expand a unit quad from
    /// `@builtin(vertex_index)`, so there is no vertex buffer at all — only instance data.
    pub vertices_per_instance: u32,
    /// Whether the fragment stage samples the glyph atlas.
    ///
    /// Text binds three resources at `@group(0)` — the camera uniform plus `atlas_texture` and
    /// `atlas_sampler`; every other family binds only the camera. A device layer that built one
    /// bind-group layout for all four would fail pipeline validation on text, so the difference is
    /// declared here rather than special-cased by label at the call site.
    pub samples_atlas: bool,
}

/// Node instance layout, offsets taken from the struct rather than hand-counted.
///
/// Hand-counting is how a layout drifts: `[f32; 2]` at offset 8 followed by `u32` looks obvious
/// until a field is inserted above it. `offset_of!` cannot be wrong about the struct it is given.
const NODE_ATTRIBUTES: &[VertexAttribute] = &[
    VertexAttribute {
        shader_location: 0,
        offset: core::mem::offset_of!(GpuNodeInstance, center) as u64,
        format: VertexFormat::Float32x2,
        name: "center",
    },
    VertexAttribute {
        shader_location: 1,
        offset: core::mem::offset_of!(GpuNodeInstance, half_extent) as u64,
        format: VertexFormat::Float32x2,
        name: "half_extent",
    },
    VertexAttribute {
        shader_location: 2,
        offset: core::mem::offset_of!(GpuNodeInstance, fill) as u64,
        format: VertexFormat::Float32x4,
        name: "fill",
    },
    VertexAttribute {
        shader_location: 3,
        offset: core::mem::offset_of!(GpuNodeInstance, stroke) as u64,
        format: VertexFormat::Float32x4,
        name: "stroke",
    },
    VertexAttribute {
        shader_location: 4,
        offset: core::mem::offset_of!(GpuNodeInstance, shape) as u64,
        format: VertexFormat::Uint32,
        name: "shape",
    },
    VertexAttribute {
        shader_location: 5,
        offset: core::mem::offset_of!(GpuNodeInstance, node_index) as u64,
        format: VertexFormat::Uint32,
        name: "node_index",
    },
    VertexAttribute {
        shader_location: 6,
        offset: core::mem::offset_of!(GpuNodeInstance, stroke_width) as u64,
        format: VertexFormat::Float32,
        name: "stroke_width",
    },
];

/// Six vertices: two triangles of a unit quad, matching `QUAD` in every diagram shader.
pub const QUAD_VERTICES_PER_INSTANCE: u32 = 6;

/// The node pipeline description.
#[must_use]
pub const fn node_pipeline() -> PipelineDescriptor {
    PipelineDescriptor {
        samples_atlas: false,
        label: "fm-node-sdf",
        family: PrimitiveFamily::Node,
        wgsl: NODE_SDF_WGSL,
        instance: InstanceLayout {
            array_stride: size_of::<GpuNodeInstance>() as u64,
            attributes: NODE_ATTRIBUTES,
        },
        vertices_per_instance: QUAD_VERTICES_PER_INSTANCE,
    }
}

/// Edge instance layout.
///
/// ⚠️ THE STRUCT ORDER AND THE SHADER ORDER DISAGREE HERE, AND THAT IS THE WHOLE REASON THIS MODULE
/// EXISTS. `GpuEdgeSegment` declares `… color, dash_phase, dash, width`; `EDGE_WGSL` binds
/// `@location(3) color`, `@location(4) dash`, `@location(5) width`, `@location(6) dash_phase`. Any
/// scheme that derived locations from declaration order would hand the shader's `dash` the bytes of
/// `dash_phase`, and every dotted edge would draw a pattern computed from how far along the route it
/// started — a wrong picture with no error anywhere. Offsets come from `offset_of!`, locations are
/// written out explicitly, and the two are reconciled BY NAME in the test.
const EDGE_ATTRIBUTES: &[VertexAttribute] = &[
    VertexAttribute {
        shader_location: 0,
        offset: core::mem::offset_of!(GpuEdgeSegment, from) as u64,
        format: VertexFormat::Float32x2,
        // The shader spells these `from_point`/`to_point` while the struct spells them `from`/`to`.
        // The NAME IS THE JOIN KEY for the parity test, so the shader's spelling is authoritative
        // and the difference is recorded here rather than silently reconciled by position.
        name: "from_point",
    },
    VertexAttribute {
        shader_location: 1,
        offset: core::mem::offset_of!(GpuEdgeSegment, to) as u64,
        format: VertexFormat::Float32x2,
        name: "to_point",
    },
    VertexAttribute {
        shader_location: 2,
        offset: core::mem::offset_of!(GpuEdgeSegment, edge_index) as u64,
        format: VertexFormat::Uint32,
        name: "edge_index",
    },
    VertexAttribute {
        shader_location: 3,
        offset: core::mem::offset_of!(GpuEdgeSegment, color) as u64,
        format: VertexFormat::Float32x4,
        name: "color",
    },
    VertexAttribute {
        shader_location: 4,
        offset: core::mem::offset_of!(GpuEdgeSegment, dash) as u64,
        format: VertexFormat::Float32x2,
        name: "dash",
    },
    VertexAttribute {
        shader_location: 5,
        offset: core::mem::offset_of!(GpuEdgeSegment, width) as u64,
        format: VertexFormat::Float32,
        name: "width",
    },
    VertexAttribute {
        shader_location: 6,
        offset: core::mem::offset_of!(GpuEdgeSegment, dash_phase) as u64,
        format: VertexFormat::Float32,
        name: "dash_phase",
    },
];

/// The edge pipeline description.
#[must_use]
pub const fn edge_pipeline() -> PipelineDescriptor {
    PipelineDescriptor {
        samples_atlas: false,
        label: "fm-edge",
        family: PrimitiveFamily::Edge,
        wgsl: EDGE_WGSL,
        instance: InstanceLayout {
            array_stride: size_of::<GpuEdgeSegment>() as u64,
            attributes: EDGE_ATTRIBUTES,
        },
        vertices_per_instance: QUAD_VERTICES_PER_INSTANCE,
    }
}

/// Arrowhead instance layout.
const ARROWHEAD_ATTRIBUTES: &[VertexAttribute] = &[
    VertexAttribute {
        shader_location: 0,
        offset: core::mem::offset_of!(GpuArrowheadInstance, position) as u64,
        format: VertexFormat::Float32x2,
        name: "position",
    },
    VertexAttribute {
        shader_location: 1,
        offset: core::mem::offset_of!(GpuArrowheadInstance, angle) as u64,
        format: VertexFormat::Float32,
        name: "angle",
    },
    VertexAttribute {
        shader_location: 2,
        offset: core::mem::offset_of!(GpuArrowheadInstance, size) as u64,
        format: VertexFormat::Float32,
        name: "size",
    },
    VertexAttribute {
        shader_location: 3,
        offset: core::mem::offset_of!(GpuArrowheadInstance, edge_index) as u64,
        format: VertexFormat::Uint32,
        name: "edge_index",
    },
    VertexAttribute {
        shader_location: 4,
        offset: core::mem::offset_of!(GpuArrowheadInstance, kind) as u64,
        format: VertexFormat::Uint32,
        name: "kind",
    },
    VertexAttribute {
        shader_location: 5,
        offset: core::mem::offset_of!(GpuArrowheadInstance, color) as u64,
        format: VertexFormat::Float32x4,
        name: "color",
    },
    VertexAttribute {
        shader_location: 6,
        offset: core::mem::offset_of!(GpuArrowheadInstance, fill) as u64,
        format: VertexFormat::Float32x4,
        name: "fill",
    },
];

/// The arrowhead pipeline description.
#[must_use]
pub const fn arrowhead_pipeline() -> PipelineDescriptor {
    PipelineDescriptor {
        samples_atlas: false,
        label: "fm-arrowhead",
        family: PrimitiveFamily::Arrowhead,
        wgsl: ARROWHEAD_WGSL,
        instance: InstanceLayout {
            array_stride: size_of::<GpuArrowheadInstance>() as u64,
            attributes: ARROWHEAD_ATTRIBUTES,
        },
        vertices_per_instance: QUAD_VERTICES_PER_INSTANCE,
    }
}

/// Text instance layout.
///
/// The only family that samples a texture: `uv_min`/`uv_max` address the glyph's cell in the atlas
/// `GlyphAtlasPlan` describes, so this pipeline additionally needs the atlas texture and its sampler
/// bound. That binding is a device-layer concern and is deliberately not modelled here — what IS
/// modelled is that the UV rectangle travels per instance rather than per draw, which is what lets
/// one draw call cover every glyph of every label.
///
/// Struct and shader agree on order here, unlike the edge family. That is a fact about today's
/// declarations, not a property, which is why the test joins by name for this family too.
const TEXT_ATTRIBUTES: &[VertexAttribute] = &[
    VertexAttribute {
        shader_location: 0,
        offset: core::mem::offset_of!(GpuTextQuad, center) as u64,
        format: VertexFormat::Float32x2,
        name: "center",
    },
    VertexAttribute {
        shader_location: 1,
        offset: core::mem::offset_of!(GpuTextQuad, half_extent) as u64,
        format: VertexFormat::Float32x2,
        name: "half_extent",
    },
    VertexAttribute {
        shader_location: 2,
        offset: core::mem::offset_of!(GpuTextQuad, uv_min) as u64,
        format: VertexFormat::Float32x2,
        name: "uv_min",
    },
    VertexAttribute {
        shader_location: 3,
        offset: core::mem::offset_of!(GpuTextQuad, uv_max) as u64,
        format: VertexFormat::Float32x2,
        name: "uv_max",
    },
    VertexAttribute {
        shader_location: 4,
        offset: core::mem::offset_of!(GpuTextQuad, color) as u64,
        format: VertexFormat::Float32x4,
        name: "color",
    },
    VertexAttribute {
        shader_location: 5,
        offset: core::mem::offset_of!(GpuTextQuad, run_index) as u64,
        format: VertexFormat::Uint32,
        name: "run_index",
    },
];

/// The text pipeline description.
#[must_use]
pub const fn text_pipeline() -> PipelineDescriptor {
    PipelineDescriptor {
        // The only family that samples a texture.
        samples_atlas: true,
        label: "fm-text-atlas",
        family: PrimitiveFamily::Text,
        wgsl: TEXT_ATLAS_WGSL,
        instance: InstanceLayout {
            array_stride: size_of::<GpuTextQuad>() as u64,
            attributes: TEXT_ATTRIBUTES,
        },
        vertices_per_instance: QUAD_VERTICES_PER_INSTANCE,
    }
}

/// Every pipeline this module describes, in SUBMISSION order.
///
/// The order is the draw order, not an arbitrary listing, and it is taken from `GpuRenderPlan`'s own
/// field order — `edge_segments`, `arrowheads`, `node_instances`, `text_quads` — which that struct
/// documents as the order a consumer must submit in. Edges and their heads go down first so a node
/// box paints over the segment terminating at it; text goes last so a label is never painted over by
/// the box that carries it. A caller iterating this slice gets the right picture without having to
/// know why.
#[must_use]
pub const fn pipelines() -> [PipelineDescriptor; 5] {
    [
        edge_pipeline(),
        arrowhead_pipeline(),
        node_pipeline(),
        node_border_pipeline(),
        text_pipeline(),
    ]
}

/// The dashed-node-border pipeline: the EDGE pipeline, drawn in a later slot.
///
/// Shares `edge_pipeline`'s layout and shader exactly — a border segment IS an edge segment, and
/// giving it its own copy would be a second place for the dash binding to drift. Only the family and
/// the label differ, and the family is what puts it after the nodes.
#[must_use]
pub const fn node_border_pipeline() -> PipelineDescriptor {
    PipelineDescriptor {
        family: PrimitiveFamily::NodeBorder,
        label: "fm-node-border",
        ..edge_pipeline()
    }
}

/// Width of a text run at `font_px`, in layout units.
///
/// Re-exported here because this module is the crate's device-free surface and a caller sizing a
/// text pass needs the width without reaching into the plan builder. It is `gpu_plan`'s function,
/// not a second implementation — the whole point of bd-2u0.2's advance work is that there is exactly
/// one text-width model, shared with `fm-layout`.
#[must_use]
pub fn run_advance_for(text: &str, font_px: f32) -> f32 {
    crate::gpu_plan::run_advance(text, font_px)
}

/// One instanced draw call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrawBatch {
    pub family: PrimitiveFamily,
    /// Number of instances, i.e. the length of the family's slice in the plan.
    pub instance_count: u32,
    pub vertices_per_instance: u32,
}

impl DrawBatch {
    /// Total vertices this batch submits. Useful to a caller budgeting a frame, and the quantity a
    /// draw call is actually issued with.
    #[must_use]
    pub const fn vertex_count(self) -> u32 {
        self.instance_count * self.vertices_per_instance
    }
}

/// Draw batches for a plan, in submission order.
///
/// An **empty family produces no batch**. A zero-instance draw is legal and free on most backends,
/// but emitting one makes "did this diagram plan any nodes?" unanswerable from the batch list, and
/// the batch list is what a caller inspects when a diagram renders blank.
#[must_use]
pub fn draw_batches(plan: &GpuRenderPlan) -> Vec<DrawBatch> {
    let mut batches = Vec::new();
    // EDGES FIRST, matching `pipelines()` and the Canvas2D draw order: a node box paints over the
    // segment that terminates at it, not under it.
    if !plan.edge_segments.is_empty() {
        batches.push(DrawBatch {
            family: PrimitiveFamily::Edge,
            instance_count: u32::try_from(plan.edge_segments.len()).unwrap_or(u32::MAX),
            vertices_per_instance: QUAD_VERTICES_PER_INSTANCE,
        });
    }
    // Arrowheads ride with their edges, before the boxes those edges terminate at.
    if !plan.arrowheads.is_empty() {
        batches.push(DrawBatch {
            family: PrimitiveFamily::Arrowhead,
            instance_count: u32::try_from(plan.arrowheads.len()).unwrap_or(u32::MAX),
            vertices_per_instance: QUAD_VERTICES_PER_INSTANCE,
        });
    }
    if !plan.node_instances.is_empty() {
        batches.push(DrawBatch {
            family: PrimitiveFamily::Node,
            instance_count: u32::try_from(plan.node_instances.len()).unwrap_or(u32::MAX),
            vertices_per_instance: QUAD_VERTICES_PER_INSTANCE,
        });
    }
    // Dashed borders after their nodes: a border sits on top of the fill it outlines (bd-l3nsf).
    if !plan.node_border_segments.is_empty() {
        batches.push(DrawBatch {
            family: PrimitiveFamily::NodeBorder,
            instance_count: u32::try_from(plan.node_border_segments.len()).unwrap_or(u32::MAX),
            vertices_per_instance: QUAD_VERTICES_PER_INSTANCE,
        });
    }
    // TEXT LAST: a label must not be painted over by the box it labels.
    if !plan.text_quads.is_empty() {
        batches.push(DrawBatch {
            family: PrimitiveFamily::Text,
            instance_count: u32::try_from(plan.text_quads.len()).unwrap_or(u32::MAX),
            vertices_per_instance: QUAD_VERTICES_PER_INSTANCE,
        });
    }
    batches
}
