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
    ///
    /// ⚠️ NATIVE ONLY, and the `cfg` is load-bearing rather than tidy. This blocks on the adapter and
    /// device futures, and a browser resolves both from its own event loop — blocking the only thread
    /// there means the promise can never be driven, so this would hang a tab rather than fail. The
    /// browser path must use [`Self::request`].
    #[cfg(not(target_arch = "wasm32"))]
    pub fn headless() -> Result<Self, GpuDeviceError> {
        pollster::block_on(Self::request())
    }

    /// Acquire a device asynchronously — the form that works in a browser AND natively.
    ///
    /// `wgpu`'s adapter and device requests are futures on every backend; only native code can
    /// afford to block on them. Sharing one async implementation is what stops the WebGPU path
    /// diverging into a browser version and a native version that are tested differently and fail
    /// differently.
    ///
    /// No surface is requested, so this is equally valid for an offscreen render and for a canvas:
    /// `compatible_surface: None` asks for an adapter that can do either.
    ///
    /// # Errors
    /// Returns [`GpuDeviceError::NoAdapter`] or [`GpuDeviceError::NoDevice`].
    pub async fn request() -> Result<Self, GpuDeviceError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .map_err(|error| GpuDeviceError::NoAdapter(error.to_string()))?;

        let info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("fm-device"),
                ..Default::default()
            })
            .await
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

    /// Compile a WGSL source and report whether the driver accepted it.
    ///
    /// Exists because a shader constant is otherwise checked only by string parsing, which cannot
    /// tell a valid shader from a plausible-looking one. `EDGE_WGSL` shipped referencing an
    /// identifier that does not exist in its own vertex entry point, and nothing noticed until a
    /// device tried to compile it — the layout tests parse `@location` lines and are blind to the
    /// body.
    ///
    /// # Errors
    /// Returns the driver's diagnostic when the module does not compile.
    ///
    /// Native only: it blocks on `pop_error_scope`, which a browser resolves from its event loop.
    /// This is a diagnostic for the native test suite, not part of the browser render path.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn validate_shader(&self, label: &str, wgsl: &str) -> Result<(), String> {
        // An error SCOPE rather than the uncaptured-error handler: the handler's default panics the
        // process, so one broken shader would abort every other check in the same run instead of
        // being reported alongside them.
        self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(wgsl.into()),
            });
        drop(module);
        match pollster::block_on(self.device.pop_error_scope()) {
            Some(error) => Err(error.to_string()),
            None => Ok(()),
        }
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
    let width = if bounds.width > 0.0 {
        bounds.width
    } else {
        1.0
    };
    let height = if bounds.height > 0.0 {
        bounds.height
    } else {
        1.0
    };
    let scale_x = 2.0 / width;
    let scale_y = -2.0 / height;
    [
        scale_x,
        scale_y,
        (-1.0) - bounds.x * scale_x,
        1.0 - bounds.y * scale_y,
    ]
}

/// The glyph atlas as a GPU texture, plus the sampler the text shader reads it with.
///
/// ⚠️ NOTHING IN THIS CRATE RASTERISES GLYPHS YET. `GlyphAtlasPlan` describes the atlas — cell size,
/// grid, and the UV rectangle of every glyph — but carries no pixel data, so there is no real
/// coverage bitmap to upload. [`solid_coverage`] therefore fills each planned cell with full
/// coverage, which is enough to prove that the UV rectangles address the cells they claim and that
/// the quads land where the plan puts them, and is NOT a claim that this renders readable text.
/// Real glyph rasterisation is separate work.
pub struct GlyphAtlasTexture {
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
}

impl GlyphAtlasTexture {
    /// Upload an R8 coverage bitmap sized to the plan's `texture_px`.
    ///
    /// # Panics
    /// If `coverage` is not exactly `texture_px[0] * texture_px[1]` bytes — a mis-sized upload would
    /// shear every glyph rather than fail, which is the kind of wrongness that looks like a font bug.
    #[must_use]
    pub fn new(gpu: &GpuDevice, plan: &crate::gpu_plan::GlyphAtlasPlan, coverage: &[u8]) -> Self {
        let width = plan.texture_px[0].max(1);
        let height = plan.texture_px[1].max(1);
        assert_eq!(
            coverage.len(),
            (width as usize) * (height as usize),
            "coverage bitmap is {} bytes for a {width}x{height} atlas",
            coverage.len()
        );

        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fm-glyph-atlas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // R8: the shader reads `.r` as coverage. A single channel is all a glyph mask needs.
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            coverage,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        Self {
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            // NEAREST, not linear: a linear filter would bleed neighbouring cells into each other at
            // cell borders, so a glyph would sample its neighbour's coverage and the UV-addressing
            // test would pass or fail for reasons unrelated to the UVs.
            sampler: gpu.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("fm-glyph-atlas"),
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            }),
        }
    }
}

/// A fully-covered atlas bitmap for the plan's dimensions: every planned cell opaque, the rest zero.
///
/// Synthetic by necessity — see [`GlyphAtlasTexture`]. Filling only the PLANNED cells rather than the
/// whole texture is what makes it useful: a quad whose UVs address an unplanned region samples zero
/// and paints nothing, so a wrong UV rectangle is visible as a missing glyph rather than hidden by a
/// uniformly white texture.
#[must_use]
pub fn solid_coverage(plan: &crate::gpu_plan::GlyphAtlasPlan) -> Vec<u8> {
    let width = plan.texture_px[0].max(1) as usize;
    let height = plan.texture_px[1].max(1) as usize;
    let mut coverage = vec![0_u8; width * height];
    for cell in &plan.cells {
        let x0 = (cell.uv_min[0] * width as f32).round().max(0.0) as usize;
        let y0 = (cell.uv_min[1] * height as f32).round().max(0.0) as usize;
        let x1 = ((cell.uv_max[0] * width as f32).round() as usize).min(width);
        let y1 = ((cell.uv_max[1] * height as f32).round() as usize).min(height);
        for y in y0..y1 {
            for x in x0..x1 {
                coverage[y * width + x] = 255;
            }
        }
    }
    coverage
}

/// A built pipeline for one primitive family, plus the camera binding it draws with.
///
/// Family-agnostic on purpose: every field that differs between node, edge, arrowhead and text
/// comes out of the [`PipelineDescriptor`], including the vertex count per instance — arrowheads are
/// a THREE-vertex triangle while the other families expand a six-vertex quad, and a pass that
/// assumed six would draw two phantom vertices per head.
pub struct InstancePass {
    pipeline: wgpu::RenderPipeline,
    camera_layout: wgpu::BindGroupLayout,
    vertices_per_instance: u32,
    samples_atlas: bool,
}

impl InstancePass {
    /// Build a pipeline from its [`PipelineDescriptor`].
    ///
    /// Nothing here re-states the layout: the attributes, stride, shader and vertex count all come
    /// from the descriptor, so this cannot drift from what the GPU-free layout tests verified.
    #[must_use]
    pub fn new(gpu: &GpuDevice, descriptor: &PipelineDescriptor) -> Self {
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(descriptor.label),
                source: wgpu::ShaderSource::Wgsl(descriptor.wgsl.into()),
            });

        let mut entries = vec![wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }];
        if descriptor.samples_atlas {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            });
        }
        let camera_layout = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(descriptor.label),
                entries: &entries,
            });

        let pipeline_layout = gpu
            .device
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
            vertices_per_instance: descriptor.vertices_per_instance,
            samples_atlas: descriptor.samples_atlas,
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
        self.rgba
            .get(start..start + 4)
            .map(|p| [p[0], p[1], p[2], p[3]])
    }

    /// Convert a point in layout coordinates to the pixel it lands on.
    ///
    /// The caller needs this to ask "is there a node where the layout says one is?" without
    /// re-deriving the camera, which is exactly how a test comes to agree with a bug.
    #[must_use]
    pub fn pixel_for(&self, bounds: &fm_layout::LayoutRect, x: f32, y: f32) -> (u32, u32) {
        let width = if bounds.width > 0.0 {
            bounds.width
        } else {
            1.0
        };
        let height = if bounds.height > 0.0 {
            bounds.height
        } else {
            1.0
        };
        let u = ((x - bounds.x) / width * self.width as f32).round();
        let v = ((y - bounds.y) / height * self.height as f32).round();
        (
            (u.max(0.0) as u32).min(self.width.saturating_sub(1)),
            (v.max(0.0) as u32).min(self.height.saturating_sub(1)),
        )
    }
}

/// Every diagram pipeline, built once.
///
/// A pipeline is expensive to create and immutable once built, so a caller rendering more than one
/// diagram — or one diagram more than once, which is the whole point of a GPU path — builds this
/// once and keeps it.
pub struct DiagramPipelines {
    passes: Vec<(crate::gpu_pipeline::PrimitiveFamily, InstancePass)>,
}

impl DiagramPipelines {
    /// Build every pipeline in [`crate::gpu_pipeline::pipelines`] order.
    ///
    /// Order is preserved rather than sorted or keyed, because it IS the draw order: edges, then
    /// their arrowheads, then the boxes that paint over both, then the labels on top.
    #[must_use]
    pub fn new(gpu: &GpuDevice) -> Self {
        Self {
            passes: crate::gpu_pipeline::pipelines()
                .iter()
                .map(|descriptor| (descriptor.family, InstancePass::new(gpu, descriptor)))
                .collect(),
        }
    }
}

/// Render a WHOLE diagram — every primitive family — in ONE submission (bd-2u0.2).
///
/// **Why this exists and `render_instances` is not enough.** Rendering per family meant four
/// textures, four command encoders, four queue submissions and four readbacks, and the four images
/// could not be composed: each pass cleared its own target, so nothing ever produced a picture of
/// the diagram. This shares one texture, one encoder, one render pass and one readback, and draws
/// the families in order into the same attachment. That is both the correct picture and four times
/// fewer round trips to the device.
///
/// Per-family instance buffers are still separate — the families have different strides and
/// different vertex counts, and a single interleaved buffer would need a stride they do not share.
/// What is shared is everything that costs a round trip.
///
/// `atlas` is required exactly when the plan has text to draw; a plan with no glyphs renders without
/// one.
///
/// # Errors
/// Returns [`GpuDeviceError::Readback`] if the mapped buffer cannot be read.
/// Blocking wrapper for native callers and tests.
///
/// # Errors
/// Returns [`GpuDeviceError::Readback`] if the mapped buffer cannot be read.
#[cfg(not(target_arch = "wasm32"))]
pub fn render_diagram(
    gpu: &GpuDevice,
    pipelines: &DiagramPipelines,
    plan: &crate::gpu_plan::GpuRenderPlan,
    atlas: Option<&GlyphAtlasTexture>,
    width: u32,
    height: u32,
) -> Result<RenderedImage, GpuDeviceError> {
    pollster::block_on(render_diagram_async(
        gpu, pipelines, plan, atlas, width, height,
    ))
}

#[allow(clippy::missing_panics_doc)]
pub async fn render_diagram_async(
    gpu: &GpuDevice,
    pipelines: &DiagramPipelines,
    plan: &crate::gpu_plan::GpuRenderPlan,
    atlas: Option<&GlyphAtlasTexture>,
    width: u32,
    height: u32,
) -> Result<RenderedImage, GpuDeviceError> {
    use crate::gpu_pipeline::PrimitiveFamily;

    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fm-diagram"),
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

    // ONE camera for every family, which is what makes the passes compose: a per-family camera could
    // drift and the picture would come apart at the seams rather than fail.
    let camera = camera_transform(&plan.bounds);
    let camera_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fm-camera"),
        size: size_of::<[f32; 4]>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    gpu.queue
        .write_buffer(&camera_buffer, 0, &bytemuck_cast(&camera));

    // Serialise every family up front. Held for the whole render pass because the buffers they fill
    // must outlive it.
    let mut uploads: Vec<(PrimitiveFamily, wgpu::Buffer, u32)> = Vec::new();
    for (family, _) in &pipelines.passes {
        let (bytes, count) = match family {
            PrimitiveFamily::Edge => (
                edge_instance_bytes(&plan.edge_segments),
                plan.edge_segments.len(),
            ),
            PrimitiveFamily::Arrowhead => (
                arrowhead_instance_bytes(&plan.arrowheads),
                plan.arrowheads.len(),
            ),
            PrimitiveFamily::Node => (
                node_instance_bytes(&plan.node_instances),
                plan.node_instances.len(),
            ),
            // Dashed node borders ride the EDGE pipeline (bd-l3nsf) but draw AFTER the nodes, since
            // a border sits on top of the fill it outlines. Same pipeline, different slot in the
            // order — which is why the family exists separately from `Edge`.
            PrimitiveFamily::NodeBorder => (
                edge_instance_bytes(&plan.node_border_segments),
                plan.node_border_segments.len(),
            ),
            PrimitiveFamily::Text => (text_instance_bytes(&plan.text_quads), plan.text_quads.len()),
        };
        if count == 0 {
            continue;
        }
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fm-instances"),
            size: bytes.len().max(4) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu.queue.write_buffer(&buffer, 0, &bytes);
        uploads.push((*family, buffer, u32::try_from(count).unwrap_or(u32::MAX)));
    }

    // Bind groups, one per pass, because text binds three resources where the others bind one.
    let mut binds: Vec<(PrimitiveFamily, wgpu::BindGroup)> = Vec::new();
    for (family, pass) in &pipelines.passes {
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: camera_buffer.as_entire_binding(),
        }];
        if pass.samples_atlas {
            let Some(atlas) = atlas else {
                // A plan with no glyphs never reaches a text draw, so a missing atlas is only fatal
                // when there is text to draw; skipping keeps "render a diagram with no labels"
                // working without inventing a blank texture.
                continue;
            };
            entries.push(wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&atlas.view),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&atlas.sampler),
            });
        }
        binds.push((
            *family,
            gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("fm-bindings"),
                layout: &pass.camera_layout,
                entries: &entries,
            }),
        ));
    }

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
            label: Some("fm-diagram"),
        });
    {
        let mut render = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fm-diagram"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // Cleared ONCE, before any family draws. Clearing per family is exactly what
                    // made the separate passes uncomposable: each one erased the last.
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        for (family, pass) in &pipelines.passes {
            let Some((_, buffer, count)) = uploads.iter().find(|(f, _, _)| f == family) else {
                continue;
            };
            let Some((_, bind)) = binds.iter().find(|(f, _)| f == family) else {
                continue;
            };
            render.set_pipeline(&pass.pipeline);
            render.set_bind_group(0, bind, &[]);
            render.set_vertex_buffer(0, buffer.slice(..));
            render.draw(0..pass.vertices_per_instance, 0..*count);
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
    // ONE submission for the whole diagram.
    gpu.queue.submit(Some(encoder.finish()));

    read_back_async(gpu, &readback, width, height, padded_row, unpadded_row).await
}

/// What to draw in one pass: the geometry, and the atlas if the pipeline samples one.
///
/// Grouped into a struct rather than passed as loose arguments because these four travel together
/// and are individually easy to transpose — `instance_count` and a dimension are both bare `u32`,
/// so a swapped pair would compile and draw the wrong thing.
#[derive(Clone, Copy)]
pub struct InstanceDraw<'a> {
    /// The plan's bounds, so every family in a diagram shares one camera and the passes compose.
    pub bounds: &'a fm_layout::LayoutRect,
    /// Serialised instances, from the family's `*_instance_bytes` function.
    pub instance_bytes: &'a [u8],
    pub instance_count: u32,
    /// Required exactly when the pipeline's `samples_atlas` is set.
    pub atlas: Option<&'a GlyphAtlasTexture>,
}

/// Render one family's instances to an offscreen texture and read the pixels back.
///
/// Takes already-serialised bytes rather than a typed slice: each family has its own instance
/// struct, and the serialisers are the only place that knows how to lay one out. Passing bytes keeps
/// exactly one code path for the device work, so a rendering bug cannot be family-specific.
///
/// `bounds` comes from the plan rather than from the instances, so every family in a diagram shares
/// one camera and the passes compose into the same image.
///
/// # Errors
/// Returns [`GpuDeviceError::Readback`] if the mapped buffer cannot be read.
///
/// Native only: it blocks on the readback. A browser draws whole diagrams through
/// [`render_diagram_async`], and this per-family path exists so a native test can measure one family
/// in isolation — which is how the batched path proves it composes.
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::missing_panics_doc)]
pub fn render_instances(
    gpu: &GpuDevice,
    pass: &InstancePass,
    draw: &InstanceDraw<'_>,
    width: u32,
    height: u32,
) -> Result<RenderedImage, GpuDeviceError> {
    let InstanceDraw {
        bounds,
        instance_bytes,
        instance_count,
        atlas,
    } = *draw;
    assert_eq!(
        pass.samples_atlas,
        atlas.is_some(),
        "the atlas argument must match the pipeline: a text pass without one fails bind-group \
         validation, and a non-text pass with one binds a resource its layout does not declare"
    );
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

    let camera = camera_transform(bounds);
    let camera_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fm-camera"),
        size: size_of::<[f32; 4]>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    gpu.queue
        .write_buffer(&camera_buffer, 0, &bytemuck_cast(&camera));

    let mut bind_entries = vec![wgpu::BindGroupEntry {
        binding: 0,
        resource: camera_buffer.as_entire_binding(),
    }];
    if let Some(atlas) = atlas {
        bind_entries.push(wgpu::BindGroupEntry {
            binding: 1,
            resource: wgpu::BindingResource::TextureView(&atlas.view),
        });
        bind_entries.push(wgpu::BindGroupEntry {
            binding: 2,
            resource: wgpu::BindingResource::Sampler(&atlas.sampler),
        });
    }
    let camera_bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("fm-bindings"),
        layout: &pass.camera_layout,
        entries: &bind_entries,
    });

    let instance_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fm-instances"),
        // A zero-sized buffer is invalid; a diagram with no nodes still needs a bindable buffer,
        // and the draw below is skipped anyway.
        size: instance_bytes.len().max(4) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    if !instance_bytes.is_empty() {
        gpu.queue.write_buffer(&instance_buffer, 0, instance_bytes);
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
        if instance_count > 0 {
            render.set_pipeline(&pass.pipeline);
            render.set_bind_group(0, &camera_bind, &[]);
            render.set_vertex_buffer(0, instance_buffer.slice(..));
            // Vertex count FROM THE PASS, not a literal 6: an arrowhead is a three-vertex triangle
            // and a hardcoded quad would draw two phantom vertices per head.
            render.draw(0..pass.vertices_per_instance, 0..instance_count);
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

    read_back(gpu, &readback, width, height, padded_row, unpadded_row)
}

/// Map the readback buffer and strip the row padding.
///
/// Shared by both entry points so there is exactly one place that understands the 256-byte row
/// alignment. Two copies of this drifting apart would show up as a diagonal shear in one path only.
#[cfg(not(target_arch = "wasm32"))]
fn read_back(
    gpu: &GpuDevice,
    readback: &wgpu::Buffer,
    width: u32,
    height: u32,
    padded_row: usize,
    unpadded_row: usize,
) -> Result<RenderedImage, GpuDeviceError> {
    pollster::block_on(read_back_async(
        gpu,
        readback,
        width,
        height,
        padded_row,
        unpadded_row,
    ))
}

/// Await the mapped readback buffer and strip the row padding.
///
/// ASYNC because a browser has no other option: `map_async` resolves from the event loop, and
/// `device.poll(Wait)` — which is what makes the native path work — is a no-op on WebGPU. A blocking
/// readback in a browser waits forever on a callback the blocked thread is preventing.
async fn read_back_async(
    gpu: &GpuDevice,
    readback: &wgpu::Buffer,
    width: u32,
    height: u32,
    padded_row: usize,
    unpadded_row: usize,
) -> Result<RenderedImage, GpuDeviceError> {
    let slice = readback.slice(..);
    let (sender, receiver) = futures_channel::oneshot::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    // Native backends need an explicit poll to make progress; on WebGPU this is a no-op and the
    // browser drives the callback itself. A timeout here would turn a slow software adapter into an
    // intermittent failure, and this repo has already paid for wall-clock-dependent assertions once.
    gpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .map_err(|error| GpuDeviceError::Readback(error.to_string()))?;
    receiver
        .await
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

/// Text quads as instance bytes, each field written AT ITS `offset_of!` POSITION.
///
/// One quad per GLYPH, not per label: `uv_min`/`uv_max` address that glyph's cell in the atlas, so a
/// whole diagram's text is one instanced draw.
#[must_use]
pub fn text_instance_bytes(quads: &[crate::gpu_plan::GpuTextQuad]) -> Vec<u8> {
    use crate::gpu_plan::GpuTextQuad;

    let stride = size_of::<GpuTextQuad>();
    let mut out = vec![0_u8; std::mem::size_of_val(quads)];
    for (index, quad) in quads.iter().enumerate() {
        let base = index * stride;
        let mut put = |offset: usize, bytes: &[u8]| {
            out[base + offset..base + offset + bytes.len()].copy_from_slice(bytes);
        };
        put(
            core::mem::offset_of!(GpuTextQuad, center),
            &[quad.center[0].to_ne_bytes(), quad.center[1].to_ne_bytes()].concat(),
        );
        put(
            core::mem::offset_of!(GpuTextQuad, half_extent),
            &[
                quad.half_extent[0].to_ne_bytes(),
                quad.half_extent[1].to_ne_bytes(),
            ]
            .concat(),
        );
        put(
            core::mem::offset_of!(GpuTextQuad, uv_min),
            &[quad.uv_min[0].to_ne_bytes(), quad.uv_min[1].to_ne_bytes()].concat(),
        );
        put(
            core::mem::offset_of!(GpuTextQuad, uv_max),
            &[quad.uv_max[0].to_ne_bytes(), quad.uv_max[1].to_ne_bytes()].concat(),
        );
        put(
            core::mem::offset_of!(GpuTextQuad, color),
            &quad.color.map(f32::to_ne_bytes).concat(),
        );
        put(
            core::mem::offset_of!(GpuTextQuad, run_index),
            &quad.run_index.to_ne_bytes(),
        );
    }
    out
}

/// Arrowheads as instance bytes, each field written AT ITS `offset_of!` POSITION.
#[must_use]
pub fn arrowhead_instance_bytes(heads: &[crate::gpu_plan::GpuArrowheadInstance]) -> Vec<u8> {
    use crate::gpu_plan::GpuArrowheadInstance;

    let stride = size_of::<GpuArrowheadInstance>();
    let mut out = vec![0_u8; std::mem::size_of_val(heads)];
    for (index, head) in heads.iter().enumerate() {
        let base = index * stride;
        let mut put = |offset: usize, bytes: &[u8]| {
            out[base + offset..base + offset + bytes.len()].copy_from_slice(bytes);
        };
        put(
            core::mem::offset_of!(GpuArrowheadInstance, position),
            &[
                head.position[0].to_ne_bytes(),
                head.position[1].to_ne_bytes(),
            ]
            .concat(),
        );
        put(
            core::mem::offset_of!(GpuArrowheadInstance, angle),
            &head.angle.to_ne_bytes(),
        );
        put(
            core::mem::offset_of!(GpuArrowheadInstance, size),
            &head.size.to_ne_bytes(),
        );
        put(
            core::mem::offset_of!(GpuArrowheadInstance, edge_index),
            &head.edge_index.to_ne_bytes(),
        );
        put(
            core::mem::offset_of!(GpuArrowheadInstance, color),
            &head.color.map(f32::to_ne_bytes).concat(),
        );
    }
    out
}

/// Edge segments as instance bytes, each field written AT ITS `offset_of!` POSITION.
///
/// ⚠️ THIS IS THE FAMILY WHERE WRITING IN FIELD ORDER WOULD BE HARMLESS AND BINDING IN FIELD ORDER
/// WOULD NOT. The serialisation order below is irrelevant precisely because every field is placed at
/// its own offset; what matters is that `EDGE_ATTRIBUTES` binds `dash` to `@location(4)`, `width` to
/// `@location(5)` and `dash_phase` to `@location(6)` while the struct declares
/// `dash_phase, dash, width`. Those two facts only agree because both sides go through `offset_of!`.
#[must_use]
pub fn edge_instance_bytes(segments: &[crate::gpu_plan::GpuEdgeSegment]) -> Vec<u8> {
    use crate::gpu_plan::GpuEdgeSegment;

    let stride = size_of::<GpuEdgeSegment>();
    let mut out = vec![0_u8; std::mem::size_of_val(segments)];
    for (index, segment) in segments.iter().enumerate() {
        let base = index * stride;
        let mut put = |offset: usize, bytes: &[u8]| {
            out[base + offset..base + offset + bytes.len()].copy_from_slice(bytes);
        };
        put(
            core::mem::offset_of!(GpuEdgeSegment, from),
            &[segment.from[0].to_ne_bytes(), segment.from[1].to_ne_bytes()].concat(),
        );
        put(
            core::mem::offset_of!(GpuEdgeSegment, to),
            &[segment.to[0].to_ne_bytes(), segment.to[1].to_ne_bytes()].concat(),
        );
        put(
            core::mem::offset_of!(GpuEdgeSegment, edge_index),
            &segment.edge_index.to_ne_bytes(),
        );
        put(
            core::mem::offset_of!(GpuEdgeSegment, color),
            &segment.color.map(f32::to_ne_bytes).concat(),
        );
        put(
            core::mem::offset_of!(GpuEdgeSegment, dash),
            &[segment.dash[0].to_ne_bytes(), segment.dash[1].to_ne_bytes()].concat(),
        );
        put(
            core::mem::offset_of!(GpuEdgeSegment, width),
            &segment.width.to_ne_bytes(),
        );
        put(
            core::mem::offset_of!(GpuEdgeSegment, dash_phase),
            &segment.dash_phase.to_ne_bytes(),
        );
    }
    out
}
