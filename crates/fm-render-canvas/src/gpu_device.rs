//! The wgpu device layer: turns the descriptions in [`crate::gpu_pipeline`] into real GPU objects
//! and actually rasterises a diagram (bd-2u0.2).
//!
//! `gpu_pipeline` is deliberately device-free so its layouts can be proven against the shader source
//! without an adapter. This module is the other half: it builds a `wgpu::RenderPipeline` from a
//! [`PipelineDescriptor`], uploads a plan's instance buffer, draws to an offscreen texture, and reads
//! the pixels back. Everything here is behind the crate's `webgpu` feature, off by default, because
//! this crate also ships inside the size-optimised WASM bundle where wgpu has no business being
//! unless the caller asked for it.
//!
//! **Headless on purpose.** There is no surface, no window and no swapchain: rendering targets a
//! plain texture and the result is read back as RGBA bytes. That is what makes the pipeline testable
//! in CI on a machine with a render node and no display, and it is the same code path a browser
//! caller uses right up to the point where the target texture comes from a canvas instead.

use crate::gpu_pipeline::{InstanceLayout, PipelineDescriptor, VertexFormat};
use crate::gpu_plan::GpuRenderPlan;

/// Why a GPU could not be obtained or used.
///
/// Distinguished rather than collapsed into one string: "this machine exposes no adapter" is an
/// environment fact a caller may reasonably fall back from, while "the device rejected our pipeline"
/// is a bug in this crate. A single opaque error would make a test that silently passes on a
/// GPU-less box indistinguishable from one that passes on a broken pipeline.
#[derive(Debug)]
pub enum GpuDeviceError {
    /// No adapter at all — no render node, no software fallback, nothing.
    NoAdapter(String),
    /// An adapter exists but would not give us a device.
    NoDevice(String),
    /// The GPU accepted the work but reading the result back failed.
    Readback(String),
}

impl core::fmt::Display for GpuDeviceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoAdapter(why) => write!(f, "no wgpu adapter available: {why}"),
            Self::NoDevice(why) => write!(f, "adapter would not provide a device: {why}"),
            Self::Readback(why) => write!(f, "could not read the rendered texture back: {why}"),
        }
    }
}

impl std::error::Error for GpuDeviceError {}

/// The texture format everything here renders to.
///
/// `Rgba8Unorm`, NOT `Rgba8UnormSrgb`: the plan's colours are already the linear values the Canvas2D
/// and SVG passes use, so an sRGB target would apply a transfer function those two backends do not,
/// and the same diagram would come out visibly lighter on the GPU than in the reference arm.
pub const RENDER_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// An adapter, device and queue, obtained headlessly.
pub struct GpuDevice {
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter_name: String,
    backend: wgpu::Backend,
}

impl GpuDevice {
    /// Acquire a device with no surface, preferring whatever adapter the platform offers.
    ///
    /// `power_preference` is left at default rather than forced to high-performance: on a headless
    /// box the only adapter is often a software or integrated one, and demanding discrete hardware
    /// turns "slower than ideal" into "no adapter at all".
    ///
    /// # Errors
    /// Returns [`GpuDeviceError::NoAdapter`] or [`GpuDeviceError::NoDevice`].
    pub fn headless() -> Result<Self, GpuDeviceError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .map_err(|error| GpuDeviceError::NoAdapter(error.to_string()))?;

        let info = adapter.get_info();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("fm-headless"),
            ..Default::default()
        }))
        .map_err(|error| GpuDeviceError::NoDevice(error.to_string()))?;

        Ok(Self {
            device,
            queue,
            adapter_name: info.name,
            backend: info.backend,
        })
    }

    /// Human-readable adapter identity, for test output and diagnostics. A GPU result whose adapter
    /// is unnamed is not reproducible by anyone else.
    #[must_use]
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// Which wgpu backend served the device (Vulkan, GL, Metal, …).
    #[must_use]
    pub const fn backend(&self) -> wgpu::Backend {
        self.backend
    }
}

/// Translate one of our formats into wgpu's.
///
/// A `match` rather than a numeric cast, so adding a format to [`VertexFormat`] is a compile error
/// here instead of a silently wrong buffer read.
const fn wgpu_format(format: VertexFormat) -> wgpu::VertexFormat {
    match format {
        VertexFormat::Float32 => wgpu::VertexFormat::Float32,
        VertexFormat::Float32x2 => wgpu::VertexFormat::Float32x2,
        VertexFormat::Float32x4 => wgpu::VertexFormat::Float32x4,
        VertexFormat::Uint32 => wgpu::VertexFormat::Uint32,
    }
}

/// Our attribute list as wgpu's, preserving offset and `@location` exactly.
///
/// This is the ONE place the two representations meet, which is why `gpu_pipeline` bothers to carry
/// explicit offsets and locations: everything this function does is mechanical, so the interesting
/// decisions stay in the layout where a test without a GPU can check them.
#[must_use]
pub fn vertex_attributes(layout: &InstanceLayout) -> Vec<wgpu::VertexAttribute> {
    layout
        .attributes
        .iter()
        .map(|attribute| wgpu::VertexAttribute {
            format: wgpu_format(attribute.format),
            offset: attribute.offset,
            shader_location: attribute.shader_location,
        })
        .collect()
}

/// Camera uniform matching `struct Camera { transform: vec4<f32> }` in every diagram shader:
/// `xy` scale and `zw` translate, mapping layout coordinates to clip space.
///
/// The Y scale is NEGATIVE. Layout space grows downward like every 2D canvas; clip space grows
/// upward. Without the flip a diagram renders upside down, which is a mistake that looks like a
/// layout bug rather than a camera bug and is correspondingly slow to find.
#[must_use]
pub fn camera_transform(bounds: &fm_layout::LayoutRect) -> [f32; 4] {
    let width = if bounds.width > 0.0 { bounds.width } else { 1.0 };
    let height = if bounds.height > 0.0 { bounds.height } else { 1.0 };
    let scale_x = 2.0 / width;
    let scale_y = -2.0 / height;
    [
        scale_x,
        scale_y,
        (-1.0) - bounds.x * scale_x,
        1.0 - bounds.y * scale_y,
    ]
}

/// A built node pipeline plus the camera binding it draws with.
pub struct NodePass {
    pipeline: wgpu::RenderPipeline,
    camera_layout: wgpu::BindGroupLayout,
}

impl NodePass {
    /// Build the node pipeline from its [`PipelineDescriptor`].
    ///
    /// Nothing here re-states the layout: the attributes, stride and shader all come from the
    /// descriptor, so this cannot drift from what the layout tests verified.
    #[must_use]
    pub fn new(gpu: &GpuDevice, descriptor: &PipelineDescriptor) -> Self {
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(descriptor.label),
                source: wgpu::ShaderSource::Wgsl(descriptor.wgsl.into()),
            });

        let camera_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("fm-camera"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });

        let pipeline_layout =
            gpu.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some(descriptor.label),
                    bind_group_layouts: &[&camera_layout],
                    push_constant_ranges: &[],
                });

        let attributes = vertex_attributes(&descriptor.instance);
        let buffers = [wgpu::VertexBufferLayout {
            array_stride: descriptor.instance.array_stride,
            // PER INSTANCE, not per vertex: the shaders expand a unit quad from
            // `@builtin(vertex_index)` and there is no vertex buffer at all.
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &attributes,
        }];

        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(descriptor.label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &buffers,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: RENDER_FORMAT,
                        // Straight alpha blending, matching what the Canvas2D pass does when a node
                        // or cluster carries opacity. Without it every translucent fill would land
                        // opaque and the GPU would disagree with both other backends.
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        Self {
            pipeline,
            camera_layout,
        }
    }
}

/// A rendered image: RGBA8 rows, tightly packed at `width * 4` bytes.
pub struct RenderedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl RenderedImage {
    /// The RGBA at a pixel, or `None` when it is outside the image.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let start = ((y as usize) * (self.width as usize) + (x as usize)) * 4;
        self.rgba.get(start..start + 4).map(|p| [p[0], p[1], p[2], p[3]])
    }

    /// Convert a point in layout coordinates to the pixel it lands on.
    ///
    /// The caller needs this to ask "is there a node where the layout says one is?" without
    /// re-deriving the camera, which is exactly how a test comes to agree with a bug.
    #[must_use]
    pub fn pixel_for(&self, bounds: &fm_layout::LayoutRect, x: f32, y: f32) -> (u32, u32) {
        let width = if bounds.width > 0.0 { bounds.width } else { 1.0 };
        let height = if bounds.height > 0.0 { bounds.height } else { 1.0 };
        let u = ((x - bounds.x) / width * self.width as f32).round();
        let v = ((y - bounds.y) / height * self.height as f32).round();
        (
            (u.max(0.0) as u32).min(self.width.saturating_sub(1)),
            (v.max(0.0) as u32).min(self.height.saturating_sub(1)),
        )
    }
}

/// Render a plan's NODE instances to an offscreen texture and read the pixels back.
///
/// # Errors
/// Returns [`GpuDeviceError::Readback`] if the mapped buffer cannot be read.
#[allow(clippy::missing_panics_doc)]
pub fn render_nodes(
    gpu: &GpuDevice,
    pass: &NodePass,
    plan: &GpuRenderPlan,
    width: u32,
    height: u32,
) -> Result<RenderedImage, GpuDeviceError> {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fm-target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: RENDER_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let camera = camera_transform(&plan.bounds);
    let camera_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fm-camera"),
        size: size_of::<[f32; 4]>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    gpu.queue
        .write_buffer(&camera_buffer, 0, &bytemuck_cast(&camera));

    let camera_bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("fm-camera"),
        layout: &pass.camera_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: camera_buffer.as_entire_binding(),
        }],
    });

    let instances = plan.node_instances.as_slice();
    let instance_bytes = instance_bytes(instances);
    let instance_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fm-node-instances"),
        // A zero-sized buffer is invalid; a diagram with no nodes still needs a bindable buffer,
        // and the draw below is skipped anyway.
        size: instance_bytes.len().max(4) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    if !instance_bytes.is_empty() {
        gpu.queue.write_buffer(&instance_buffer, 0, &instance_bytes);
    }

    // Readback rows must be 256-byte aligned; the padding is stripped after mapping.
    let unpadded_row = width as usize * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
    let padded_row = unpadded_row.div_ceil(align) * align;
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fm-readback"),
        size: (padded_row * height as usize) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("fm-nodes"),
        });
    {
        let mut render = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fm-nodes"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // Transparent, not a theme colour: the test asks "did anything get painted
                    // here?", and a cleared background that happened to match a node fill would
                    // make that question unanswerable.
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        if !instances.is_empty() {
            render.set_pipeline(&pass.pipeline);
            render.set_bind_group(0, &camera_bind, &[]);
            render.set_vertex_buffer(0, instance_buffer.slice(..));
            let count = u32::try_from(instances.len()).unwrap_or(u32::MAX);
            render.draw(0..6, 0..count);
        }
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row as u32),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit(Some(encoder.finish()));

    let slice = readback.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    // Wait for the most recent submission, indefinitely. A timeout here would turn a slow software
    // adapter into an intermittent failure, and this repo has already paid for wall-clock-dependent
    // assertions once.
    gpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .map_err(|error| GpuDeviceError::Readback(error.to_string()))?;
    receiver
        .recv()
        .map_err(|error| GpuDeviceError::Readback(error.to_string()))?
        .map_err(|error| GpuDeviceError::Readback(error.to_string()))?;

    let mapped = slice.get_mapped_range();
    let mut rgba = Vec::with_capacity(unpadded_row * height as usize);
    for row in 0..height as usize {
        let start = row * padded_row;
        rgba.extend_from_slice(&mapped[start..start + unpadded_row]);
    }
    drop(mapped);
    readback.unmap();

    Ok(RenderedImage {
        width,
        height,
        rgba,
    })
}

/// `[f32; 4]` as bytes, without pulling in `bytemuck` for one cast.
fn bytemuck_cast(camera: &[f32; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(16);
    for value in camera {
        out.extend_from_slice(&value.to_ne_bytes());
    }
    out
}

/// The instance slice as raw bytes, each field written AT ITS `offset_of!` POSITION.
///
/// ⚠️ NOT IN FIELD-WRITING ORDER, and the difference is not pedantry. The vertex attributes in
/// `gpu_pipeline` address this buffer by byte offset taken from `offset_of!`; if this function
/// appended fields in a different order than the struct declares them, every attribute after the
/// first divergence would read the wrong bytes — with no error from any layer, which is the exact
/// failure mode the whole pipeline module exists to prevent.
///
/// I wrote this the naive way first and got it wrong: `GpuNodeInstance` declares
/// `stroke_width, shape, node_index`, and appending `shape, node_index, stroke_width` would have
/// fed the shader a float reinterpreted as a shape discriminant. Placing each field at its own
/// offset makes the ordering question disappear rather than requiring it to be answered correctly,
/// and `the_serialised_instance_round_trips_through_the_declared_offsets` pins it.
///
/// `GpuNodeInstance` is `#[repr(C)]` with only `f32`/`u32` members at 4-byte alignment, so the
/// stride has no interior padding and a zero-filled buffer leaves no uninitialised bytes.
fn instance_bytes(instances: &[crate::gpu_plan::GpuNodeInstance]) -> Vec<u8> {
    use crate::gpu_plan::GpuNodeInstance;

    let stride = size_of::<GpuNodeInstance>();
    let mut out = vec![0_u8; std::mem::size_of_val(instances)];
    for (index, instance) in instances.iter().enumerate() {
        let base = index * stride;
        let mut put = |offset: usize, bytes: &[u8]| {
            out[base + offset..base + offset + bytes.len()].copy_from_slice(bytes);
        };
        put(
            core::mem::offset_of!(GpuNodeInstance, center),
            &[
                instance.center[0].to_ne_bytes(),
                instance.center[1].to_ne_bytes(),
            ]
            .concat(),
        );
        put(
            core::mem::offset_of!(GpuNodeInstance, half_extent),
            &[
                instance.half_extent[0].to_ne_bytes(),
                instance.half_extent[1].to_ne_bytes(),
            ]
            .concat(),
        );
        put(
            core::mem::offset_of!(GpuNodeInstance, fill),
            &instance.fill.map(f32::to_ne_bytes).concat(),
        );
        put(
            core::mem::offset_of!(GpuNodeInstance, stroke),
            &instance.stroke.map(f32::to_ne_bytes).concat(),
        );
        put(
            core::mem::offset_of!(GpuNodeInstance, stroke_width),
            &instance.stroke_width.to_ne_bytes(),
        );
        put(
            core::mem::offset_of!(GpuNodeInstance, shape),
            &instance.shape.to_ne_bytes(),
        );
        put(
            core::mem::offset_of!(GpuNodeInstance, node_index),
            &instance.node_index.to_ne_bytes(),
        );
    }
    out
}

/// Serialised bytes for one instance, exposed so a test can verify the buffer the GPU is handed
/// matches the layout the shader reads it with. Without this the serialisation would be checkable
/// only by rendering, which cannot distinguish "wrong bytes" from "wrong shader".
#[must_use]
pub fn node_instance_bytes(instances: &[crate::gpu_plan::GpuNodeInstance]) -> Vec<u8> {
    instance_bytes(instances)
}
