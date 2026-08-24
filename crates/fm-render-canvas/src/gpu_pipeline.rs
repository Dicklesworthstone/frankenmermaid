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

use crate::gpu_plan::{GpuNodeInstance, GpuRenderPlan, NODE_SDF_WGSL};

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
    if !plan.node_instances.is_empty() {
        batches.push(DrawBatch {
            family: PrimitiveFamily::Node,
            instance_count: u32::try_from(plan.node_instances.len()).unwrap_or(u32::MAX),
            vertices_per_instance: QUAD_VERTICES_PER_INSTANCE,
        });
    }
    batches
}
