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

/// Theme fill used when the author declared none.
///
/// Mirrors `CanvasRenderConfig::default().node_fill` (`#ffffff`). Kept as a constant rather than
/// read from the config so this module does not depend on a renderer instance — and pinned by
/// `gpu_theme_defaults_match_the_canvas_config`, because a GPU pass that quietly disagreed with the
/// raster pass about the DEFAULT colour would repaint every unstyled diagram.
pub const DEFAULT_NODE_FILL_RGBA: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Theme stroke used when the author declared none: `#94a3b8`.
pub const DEFAULT_NODE_STROKE_RGBA: [f32; 4] = [0.580_392_2, 0.639_215_7, 0.721_568_6, 1.0];
/// Border width used when the author declared none. Matches `CanvasRenderConfig::node_stroke_width`
/// and the value the shader previously hard-coded, so an undeclared node renders identically to
/// before this became per-instance.
pub const DEFAULT_NODE_STROKE_WIDTH: f32 = 1.5;

/// One node instance for a WebGPU SDF shape pass.
///
/// Field ORDER is the vertex-attribute order a shader declares, so the two `u32` discriminators sit
/// last and the float vectors stay contiguous: `center`, `half_extent`, `fill`, `stroke` occupy a
/// clean 8+8+16+16 bytes before them.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct GpuNodeInstance {
    /// Center position in layout coordinates.
    pub center: [f32; 2],
    /// Half extents in layout coordinates.
    pub half_extent: [f32; 2],
    /// Premultiplied-alpha-free linear RGBA fill, resolved from the author's own styling.
    ///
    /// bd-2u0.2 specifies each node instance carries "fill color, stroke color". Resolved through
    /// the SAME `resolve_node_colors` the Canvas2D pass uses, so `classDef`, `style` and
    /// `inline_style` reach the GPU exactly as they reach the raster path instead of the GPU
    /// inventing a second styling model.
    pub fill: [f32; 4],
    /// Linear RGBA stroke, same resolution rule as [`Self::fill`].
    pub stroke: [f32; 4],
    /// Border width in LAYOUT units, resolved from the author's own styling (bd-lvj3, bd-2u0.2).
    ///
    /// The same units the Canvas2D pass uses: the shader's former `STROKE_WIDTH` constant was
    /// `1.5`, which is exactly `CanvasRenderConfig::node_stroke_width`, so the SDF band is already
    /// measured in the coordinates `half_extent` is in. A declared `stroke-width:4px` therefore
    /// travels here unscaled and the two renderers draw the same border.
    ///
    /// Placed with the floats rather than after `shape` so the vector members stay contiguous, per
    /// the note above; `offset_of!` derives the attribute offset either way, so this is about
    /// padding rather than correctness.
    pub stroke_width: f32,
    /// [`GpuNodeShape`] encoded as a shader-friendly integer.
    pub shape: u32,
    /// Index back into `MermaidDiagramIr::nodes` for labels.
    ///
    /// Retained even though colour no longer needs a lookup: the glyph-atlas text pass still has to
    /// find this node's label, and dropping the back-reference would strand it.
    pub node_index: u32,
}

/// Parse a CSS colour into RGBA floats for a shader, or `None` if it is not one we can honour.
///
/// Accepts `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()`/`rgba()` with integer or percentage
/// channels, `transparent`, and the handful of bare keywords a diagram realistically uses.
///
/// Returning `None` rather than a guess matters more on the GPU than on a canvas: a canvas ignores
/// an unparsable `fillStyle` and keeps the previous colour, but a shader reads whatever bytes are in
/// the buffer, so an invented value becomes a confidently wrong pixel. The caller substitutes the
/// theme default instead.
#[must_use]
pub fn parse_paint_rgba(value: &str) -> Option<[f32; 4]> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let lower = value.to_ascii_lowercase();

    if lower == "transparent" {
        return Some([0.0, 0.0, 0.0, 0.0]);
    }
    if let Some(rgba) = named_paint_rgba(&lower) {
        return Some(rgba);
    }

    if let Some(hex) = lower.strip_prefix('#') {
        return parse_hex_rgba(hex);
    }

    for prefix in ["rgba(", "rgb("] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let args = rest.strip_suffix(')')?;
            let mut channels = [0.0_f32; 4];
            channels[3] = 1.0;
            let mut seen = 0_usize;
            for (index, raw) in args.split(',').enumerate() {
                if index >= 4 {
                    return None;
                }
                let raw = raw.trim();
                let (number, is_pct) = match raw.strip_suffix('%') {
                    Some(head) => (head.trim(), true),
                    None => (raw, false),
                };
                let parsed: f32 = number.parse().ok()?;
                if !parsed.is_finite() {
                    return None;
                }
                channels[index] = if index == 3 {
                    if is_pct { parsed / 100.0 } else { parsed }
                } else if is_pct {
                    parsed / 100.0
                } else {
                    parsed / 255.0
                };
                seen = index + 1;
            }
            if seen < 3 {
                return None;
            }
            return Some(channels.map(|c| c.clamp(0.0, 1.0)));
        }
    }

    None
}

/// `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`.
fn parse_hex_rgba(hex: &str) -> Option<[f32; 4]> {
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    // No `as` casts: the workspace clippy gate runs with -D warnings, and a shorthand digit is
    // expanded by duplicating the nibble (`#abc` -> `#aabbcc`), which is exact integer work.
    let expand = |i: usize| -> Option<u8> {
        let d = u8::from_str_radix(hex.get(i..i + 1)?, 16).ok()?;
        Some((d << 4) | d)
    };
    let bytes = hex.as_bytes();
    let channels: [u8; 4] = match bytes.len() {
        3 => [expand(0)?, expand(1)?, expand(2)?, 255],
        4 => [expand(0)?, expand(1)?, expand(2)?, expand(3)?],
        6 | 8 => {
            let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
            [
                pair(0)?,
                pair(2)?,
                pair(4)?,
                if bytes.len() == 8 { pair(6)? } else { 255 },
            ]
        }
        _ => return None,
    };
    Some(channels.map(|c| f32::from(c) / 255.0))
}

/// The bare keywords a diagram realistically writes. Deliberately short: an unknown keyword returns
/// `None` and takes the theme default, which is visible, rather than black, which looks deliberate.
fn named_paint_rgba(lower: &str) -> Option<[f32; 4]> {
    let rgb = match lower {
        "black" => [0.0, 0.0, 0.0],
        "white" => [1.0, 1.0, 1.0],
        "red" => [1.0, 0.0, 0.0],
        "green" => [0.0, 0.501_960_8, 0.0],
        "blue" => [0.0, 0.0, 1.0],
        "yellow" => [1.0, 1.0, 0.0],
        "cyan" | "aqua" => [0.0, 1.0, 1.0],
        "magenta" | "fuchsia" => [1.0, 0.0, 1.0],
        "gray" | "grey" => [0.501_960_8, 0.501_960_8, 0.501_960_8],
        "orange" => [1.0, 0.647_058_8, 0.0],
        _ => return None,
    };
    Some([rgb[0], rgb[1], rgb[2], 1.0])
}

/// One edge segment for an instanced WebGPU line pass.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct GpuEdgeSegment {
    pub from: [f32; 2],
    pub to: [f32; 2],
    /// Index back into `MermaidDiagramIr::edges`.
    pub edge_index: u32,
    /// Linear RGBA stroke, resolved from `linkStyle` through the same helper the raster pass uses.
    pub color: [f32; 4],
    /// Dash pattern as `[on, off]` in layout units; `[0.0, 0.0]` means solid.
    ///
    /// Carried because a dotted edge is a SEMANTIC distinction in mermaid, not decoration: `-.->`
    /// and `-->` mean different things to a reader. The plan previously discarded the dash that
    /// `legacy_edge_stroke` returns, so every dotted edge would have reached the GPU solid.
    pub dash: [f32; 2],
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
    /// Linear RGBA, matching the segment it terminates.
    ///
    /// A head that kept the theme colour while its line took the author's would be a worse bug than
    /// no colour support at all, because it looks deliberate.
    pub color: [f32; 4],
}

/// Theme edge stroke used when no `linkStyle` applies: `#475569`, mirroring
/// `CanvasRenderConfig::default().edge_stroke`. Pinned by
/// `gpu_theme_defaults_match_the_canvas_config`.
pub const DEFAULT_EDGE_STROKE_RGBA: [f32; 4] = [0.278_431_4, 0.333_333_34, 0.411_764_7, 1.0];

/// Theme label colour used for text quads: `#0f172a`, mirroring
/// `CanvasRenderConfig::default().label_color`. Pinned by
/// `gpu_theme_defaults_match_the_canvas_config`.
pub const DEFAULT_LABEL_RGBA: [f32; 4] = [0.058_823_53, 0.090_196_08, 0.164_705_88, 1.0];

/// Label font size in layout units, mirroring `CanvasRenderConfig::default().font_size`.
pub const DEFAULT_FONT_SIZE_PX: f32 = 14.0;

/// Advance width as a fraction of font size.
///
/// The SAME 0.57 the Canvas2D `measure_text` uses, so a glyph quad lands where the raster pass puts
/// the character. A GPU pass that invented its own advance would drift text out of its box on long
/// labels while looking correct on short ones — the worst kind of disagreement, because it is
/// invisible on every small test fixture.
pub const CHAR_ADVANCE_RATIO: f32 = 0.57;

/// One glyph's cell in the atlas texture.
///
/// UVs are normalised, so the shader samples without knowing the texture size, and the browser side
/// is free to rasterise at whatever device pixel ratio it likes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphCell {
    pub glyph: char,
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
}

/// A deterministic glyph atlas layout for one diagram (bd-2u0.2 component 3).
///
/// This plans the atlas; it does not rasterise. Rasterisation belongs on the browser side, where a
/// 2D context can draw each glyph into the texture — the same split FrankenTUI uses. What must be
/// decided HERE is which glyphs the diagram needs and where each one lives, because the quads in
/// [`GpuRenderPlan::text_quads`] carry UVs into this layout and both have to agree.
///
/// Deterministic by construction: glyphs are collected into a `BTreeSet`, so the atlas for a given
/// diagram is byte-identical across runs. A hash-ordered atlas would reshuffle UVs between two
/// renders of the same document and make any golden comparison useless.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphAtlasPlan {
    /// Side of one square cell, in texture pixels.
    pub cell_px: u32,
    /// Cells per row.
    pub columns: u32,
    /// Total rows used.
    pub rows: u32,
    /// Texture dimensions in pixels.
    pub texture_px: [u32; 2],
    /// Cells, sorted by `glyph` — binary-searchable and stable.
    pub cells: Vec<GlyphCell>,
}

impl GlyphAtlasPlan {
    /// Plan an atlas covering every glyph in `texts`.
    ///
    /// A square-ish grid rather than a tight shelf pack: every cell is the same size because every
    /// glyph is rasterised at the same font size, so the packing problem a shelf packer solves does
    /// not exist here, and a grid is exactly reproducible.
    #[must_use]
    pub fn for_texts<'a, I>(texts: I, cell_px: u32) -> Self
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut glyphs: std::collections::BTreeSet<char> = std::collections::BTreeSet::new();
        for text in texts {
            for glyph in text.chars() {
                // Control characters have no raster; a newline is a layout instruction, not ink.
                if !glyph.is_control() {
                    glyphs.insert(glyph);
                }
            }
        }

        let count = u32::try_from(glyphs.len()).unwrap_or(u32::MAX);
        if count == 0 || cell_px == 0 {
            return Self {
                cell_px,
                columns: 0,
                rows: 0,
                texture_px: [0, 0],
                cells: Vec::new(),
            };
        }

        // Square-ish grid: ceil(sqrt(count)) columns.
        let mut columns = 1_u32;
        while columns * columns < count {
            columns += 1;
        }
        let rows = count.div_ceil(columns);
        let texture_px = [columns * cell_px, rows * cell_px];

        let width = f32::from(u16::try_from(texture_px[0]).unwrap_or(u16::MAX));
        let height = f32::from(u16::try_from(texture_px[1]).unwrap_or(u16::MAX));
        let cell = f32::from(u16::try_from(cell_px).unwrap_or(u16::MAX));

        let mut cells = Vec::with_capacity(glyphs.len());
        for (index, glyph) in glyphs.into_iter().enumerate() {
            let i = u32::try_from(index).unwrap_or(u32::MAX);
            let col = f32::from(u16::try_from(i % columns).unwrap_or(u16::MAX));
            let row = f32::from(u16::try_from(i / columns).unwrap_or(u16::MAX));
            let x0 = (col * cell) / width;
            let y0 = (row * cell) / height;
            let x1 = ((col + 1.0) * cell) / width;
            let y1 = ((row + 1.0) * cell) / height;
            cells.push(GlyphCell {
                glyph,
                uv_min: [x0, y0],
                uv_max: [x1, y1],
            });
        }

        Self {
            cell_px,
            columns,
            rows,
            texture_px,
            cells,
        }
    }

    /// The cell for `glyph`, or `None` if the atlas does not carry it.
    #[must_use]
    pub fn cell(&self, glyph: char) -> Option<&GlyphCell> {
        self.cells
            .binary_search_by(|candidate| candidate.glyph.cmp(&glyph))
            .ok()
            .and_then(|index| self.cells.get(index))
    }
}

/// One textured quad: a single glyph placed in layout space.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct GpuTextQuad {
    /// Centre of the glyph cell in layout coordinates.
    pub center: [f32; 2],
    /// Half extents in layout coordinates.
    pub half_extent: [f32; 2],
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub color: [f32; 4],
    /// Which [`GpuTextRun`] this glyph belongs to.
    pub run_index: u32,
}

/// One drawn text run — the GPU counterpart of a single Canvas2D `fill_text` call.
///
/// Kept alongside the per-glyph quads because a run is the unit the raster pass draws in, so it is
/// the unit an equivalence check can compare. Counting quads instead would compare glyphs against
/// runs and never agree.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuTextRun {
    /// Index back into `MermaidDiagramIr::nodes`.
    pub node_index: u32,
    /// Index of this run's first quad in [`GpuRenderPlan::text_quads`].
    pub first_quad: u32,
    /// How many quads belong to this run.
    pub quad_count: u32,
}

/// Default glyph cell side in texture pixels.
///
/// 32 comfortably holds a 14px glyph with room for the antialiased fringe, and a power of two keeps
/// the atlas texture friendly to every backend.
pub const DEFAULT_GLYPH_CELL_PX: u32 = 32;

/// Deterministic primitive buffers for a future WebGPU command encoder.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuRenderPlan {
    pub bounds: LayoutRect,
    pub node_instances: Vec<GpuNodeInstance>,
    pub edge_segments: Vec<GpuEdgeSegment>,
    /// Triangle instances for edge arrowheads.
    pub arrowheads: Vec<GpuArrowheadInstance>,
    /// Per-glyph textured quads for the text pass.
    pub text_quads: Vec<GpuTextQuad>,
    /// One entry per drawn label — the unit the Canvas2D pass draws in.
    pub text_runs: Vec<GpuTextRun>,
    /// Where each glyph lives in the atlas the quads sample.
    pub glyph_atlas: GlyphAtlasPlan,
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
            // Colour resolved through the raster pass's own helper, so a GPU render and a Canvas2D
            // render of the same document cannot disagree about what the author asked for. An
            // undeclared or unparsable colour falls back to the theme constant rather than to
            // whatever happens to be in the instance buffer.
            let (declared_fill, declared_stroke) =
                crate::renderer::resolve_node_colors(ir, node.node_index);
            let fill = declared_fill
                .as_deref()
                .and_then(parse_paint_rgba)
                .unwrap_or(DEFAULT_NODE_FILL_RGBA);
            let stroke = declared_stroke
                .as_deref()
                .and_then(parse_paint_rgba)
                .unwrap_or(DEFAULT_NODE_STROKE_RGBA);
            // Same resolver the Canvas2D pass uses, so a `classDef stroke-width` reaches the GPU
            // exactly as it reaches the raster path instead of the GPU inventing a second rule.
            let stroke_width = crate::renderer::resolve_node_stroke_width(ir, node.node_index)
                .map(|width| width as f32)
                .filter(|width| width.is_finite() && *width > 0.0)
                .unwrap_or(DEFAULT_NODE_STROKE_WIDTH);
            node_instances.push(GpuNodeInstance {
                center: [
                    node.bounds.x + (node.bounds.width * 0.5),
                    node.bounds.y + (node.bounds.height * 0.5),
                ],
                half_extent: [node.bounds.width * 0.5, node.bounds.height * 0.5],
                fill,
                stroke,
                stroke_width,
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
            let (width, dash_pattern) =
                crate::renderer::legacy_edge_stroke(arrow, f64::from(edge_stroke_width));
            let width = width as f32;
            let edge_index = edge.edge_index.try_into().unwrap_or(u32::MAX);

            // `linkStyle` resolved through the raster pass's own helper, so a GPU render and a
            // Canvas2D render cannot disagree about what the author declared. A declared width
            // overrides the arrow-derived one exactly as it does in `draw_edges`.
            let (declared_stroke, declared_width) =
                crate::renderer::resolve_edge_style(ir, edge.edge_index);
            let color = declared_stroke
                .as_deref()
                .and_then(parse_paint_rgba)
                .unwrap_or(DEFAULT_EDGE_STROKE_RGBA);
            let width = declared_width.map_or(width, |declared| declared as f32);

            // The dash the arrow type carries. Only `[on, off]` patterns exist today; a longer
            // pattern would need a different encoding, so it is truncated deliberately rather than
            // silently taking its first two entries as if they were the whole thing.
            let dash = match dash_pattern {
                [on, off] => [*on as f32, *off as f32],
                _ => [0.0, 0.0],
            };

            for points in edge.points.windows(2) {
                let [from, to] = points else {
                    continue;
                };
                edge_segments.push(GpuEdgeSegment {
                    from: [from.x, from.y],
                    to: [to.x, to.y],
                    edge_index,
                    color,
                    dash,
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
                    color,
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
                    color,
                });
            }
        }

        // TEXT (bd-2u0.2 component 3). One run per node label, matching the raster pass, which
        // issues exactly one fill_text per label — so the two are countable against each other.
        //
        // Two passes on purpose: the atlas has to know every glyph the diagram uses before any quad
        // can be given a UV, and a quad built against a half-finished atlas would point at a cell
        // that later moves.
        let mut labelled: Vec<(u32, [f32; 2], &str)> = Vec::new();
        for node in &layout.nodes {
            if let Some(ir_node) = ir.nodes.get(node.node_index)
                && let Some(label_id) = ir_node.label
                && let Some(label) = ir.labels.get(label_id.0)
                && !label.text.is_empty()
            {
                labelled.push((
                    node.node_index.try_into().unwrap_or(u32::MAX),
                    [
                        node.bounds.x + (node.bounds.width * 0.5),
                        node.bounds.y + (node.bounds.height * 0.5),
                    ],
                    label.text.as_str(),
                ));
            }
        }

        let glyph_atlas = GlyphAtlasPlan::for_texts(
            labelled.iter().map(|(_, _, text)| *text),
            DEFAULT_GLYPH_CELL_PX,
        );

        let advance = DEFAULT_FONT_SIZE_PX * CHAR_ADVANCE_RATIO;
        let half_height = DEFAULT_FONT_SIZE_PX * 0.5;
        let mut text_quads: Vec<GpuTextQuad> = Vec::new();
        let mut text_runs: Vec<GpuTextRun> = Vec::with_capacity(labelled.len());

        for (run_index, (node_index, center, text)) in labelled.iter().enumerate() {
            let run_index_u32 = u32::try_from(run_index).unwrap_or(u32::MAX);
            let first_quad = u32::try_from(text_quads.len()).unwrap_or(u32::MAX);

            // Control characters carry no ink and are excluded from the atlas, so they must not
            // consume an advance either — otherwise a label with a newline would render with a gap
            // where nothing is drawn.
            let inked: Vec<char> = text.chars().filter(|c| !c.is_control()).collect();
            let width = advance * u16::try_from(inked.len()).map_or(f32::from(u16::MAX), f32::from);
            let start_x = center[0] - (width * 0.5) + (advance * 0.5);

            for (offset, glyph) in inked.iter().enumerate() {
                let Some(cell) = glyph_atlas.cell(*glyph) else {
                    continue;
                };
                let step = u16::try_from(offset).map_or(f32::from(u16::MAX), f32::from);
                text_quads.push(GpuTextQuad {
                    center: [start_x + (step * advance), center[1]],
                    half_extent: [advance * 0.5, half_height],
                    uv_min: cell.uv_min,
                    uv_max: cell.uv_max,
                    color: DEFAULT_LABEL_RGBA,
                    run_index: run_index_u32,
                });
            }

            let quad_count = u32::try_from(text_quads.len()).unwrap_or(u32::MAX) - first_quad;
            text_runs.push(GpuTextRun {
                node_index: *node_index,
                first_quad,
                quad_count,
            });
        }

        Self {
            bounds: layout.bounds,
            node_instances,
            edge_segments,
            arrowheads,
            text_quads,
            text_runs,
            glyph_atlas,
        }
    }
}

/// WGSL for the instanced SDF node pass (bd-2u0.2 component 4).
///
/// One draw call covers every node: the vertex stage expands a unit quad to each instance's
/// half-extent, and the fragment stage evaluates a signed distance field for that instance's shape.
/// SDF rather than tessellated geometry is what keeps edges crisp at any zoom — the distance is
/// recomputed per pixel, so a diagram zoomed 50x has the same edge quality as one at 1x, which is
/// the stated benefit over Canvas2D.
///
/// The `shape` discriminators are [`GpuNodeShape`] values and MUST stay in step with that enum;
/// `wgsl_shape_constants_match_the_rust_enum` pins them.
///
/// Antialiasing uses screen-space derivatives of the distance itself, so the transition band is one
/// pixel wide at every zoom level instead of a fixed world-space fudge that blurs when magnified.
pub const NODE_SDF_WGSL: &str = r#"
struct Camera {
    // Maps layout coordinates to clip space: xy scale, zw translate.
    transform: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;

struct NodeInstance {
    @location(0) center: vec2<f32>,
    @location(1) half_extent: vec2<f32>,
    @location(2) fill: vec4<f32>,
    @location(3) stroke: vec4<f32>,
    @location(4) shape: u32,
    @location(5) node_index: u32,
    @location(6) stroke_width: f32,
};

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) half_extent: vec2<f32>,
    @location(2) fill: vec4<f32>,
    @location(3) stroke: vec4<f32>,
    @location(4) @interpolate(flat) shape: u32,
    @location(5) @interpolate(flat) stroke_width: f32,
};

// A unit quad, expanded per instance. Two triangles, corners in [-1, 1].
var<private> QUAD: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0), vec2<f32>( 1.0, -1.0), vec2<f32>( 1.0,  1.0),
    vec2<f32>(-1.0, -1.0), vec2<f32>( 1.0,  1.0), vec2<f32>(-1.0,  1.0),
);

// One pixel of slack so the stroke and the antialiased edge are not clipped by the quad itself.
const EDGE_PAD: f32 = 2.0;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, instance: NodeInstance) -> VertexOut {
    let corner = QUAD[vertex_index];
    let padded = instance.half_extent + vec2<f32>(EDGE_PAD, EDGE_PAD);
    let world = instance.center + corner * padded;

    var out: VertexOut;
    out.clip_position = vec4<f32>(world * camera.transform.xy + camera.transform.zw, 0.0, 1.0);
    out.local = corner * padded;
    out.half_extent = instance.half_extent;
    out.fill = instance.fill;
    out.stroke = instance.stroke;
    out.stroke_width = instance.stroke_width;
    out.shape = instance.shape;
    return out;
}

fn sd_rect(p: vec2<f32>, b: vec2<f32>) -> f32 {
    let d = abs(p) - b;
    return length(max(d, vec2<f32>(0.0, 0.0))) + min(max(d.x, d.y), 0.0);
}

fn sd_rounded_rect(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    return sd_rect(p, b - vec2<f32>(r, r)) - r;
}

// Ellipse inscribed in the box. Exact ellipse SDFs are iterative; this gradient-normalised
// approximation is stable and is what the shape needs for an antialiased edge.
fn sd_ellipse(p: vec2<f32>, b: vec2<f32>) -> f32 {
    let k1 = length(p / b);
    let k2 = length(p / (b * b));
    if (k2 == 0.0) {
        return -min(b.x, b.y);
    }
    return k1 * (k1 - 1.0) / k2;
}

fn sd_diamond(p: vec2<f32>, b: vec2<f32>) -> f32 {
    let q = abs(p);
    // Distance to the line x/bx + y/by = 1, normalised by the gradient magnitude.
    return (q.x / b.x + q.y / b.y - 1.0) / length(vec2<f32>(1.0 / b.x, 1.0 / b.y));
}

// A cylinder reads as a rectangle capped by ellipses; the union is the min of the two fields.
fn sd_cylinder(p: vec2<f32>, b: vec2<f32>) -> f32 {
    let cap = min(b.y * 0.35, b.x);
    let body = sd_rect(p, vec2<f32>(b.x, b.y - cap));
    let top = sd_ellipse(p - vec2<f32>(0.0, b.y - cap), vec2<f32>(b.x, cap));
    let bottom = sd_ellipse(p + vec2<f32>(0.0, b.y - cap), vec2<f32>(b.x, cap));
    return min(body, min(top, bottom));
}

// Regular hexagon in the box: the generic stand-in for every many-sided shape.
fn sd_polygon(p: vec2<f32>, b: vec2<f32>) -> f32 {
    let q = abs(p);
    let inset = b.x * 0.25;
    let horizontal = q.x - (b.x - inset);
    let slanted = (q.x - b.x + inset) * 0.5 + q.y * (b.x / max(b.y, 0.0001)) * 0.5 - inset * 0.5;
    return max(max(horizontal, q.y - b.y), slanted);
}

fn shape_distance(shape: u32, p: vec2<f32>, b: vec2<f32>) -> f32 {
    switch shape {
        case 0u: { return sd_rect(p, b); }
        case 1u: { return sd_rounded_rect(p, b, min(6.0, min(b.x, b.y) * 0.5)); }
        case 2u: { return sd_ellipse(p, b); }
        case 3u: { return sd_diamond(p, b); }
        case 4u: { return sd_cylinder(p, b); }
        case 5u: { return sd_polygon(p, b); }
        default: { return sd_rect(p, b); }
    }
}


@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let dist = shape_distance(in.shape, in.local, in.half_extent);

    // Screen-space width of one pixel in DISTANCE units. Using the derivative of the field keeps
    // the antialias band exactly one pixel at any zoom, which fixed world-space smoothing cannot.
    let aa = max(fwidth(dist), 0.0001);

    let fill_alpha = 1.0 - smoothstep(-aa, aa, dist);
    // Per-instance rather than the former global constant: a declared `stroke-width` now reaches
    // the shader (bd-lvj3). The default the plan substitutes is the same 1.5 this used to hard-code,
    // so an undeclared node is byte-identical to before.
    let half_stroke = in.stroke_width * 0.5;
    let stroke_alpha = 1.0 - smoothstep(half_stroke - aa, half_stroke + aa, abs(dist));

    // Stroke composited over fill, both premultiplied by their own coverage.
    var color = in.fill;
    color.a = color.a * fill_alpha;
    let s = in.stroke.a * stroke_alpha;
    let out_a = s + color.a * (1.0 - s);
    if (out_a <= 0.0) {
        discard;
    }
    let out_rgb = (in.stroke.rgb * s + color.rgb * color.a * (1.0 - s)) / out_a;
    return vec4<f32>(out_rgb, out_a);
}
"#;

/// WGSL for the glyph-atlas text pass (bd-2u0.2 component 3).
///
/// Textured quads sampling the alpha of a rasterised atlas. The atlas is drawn browser-side into a
/// single texture; this pass only places and tints it, which is what keeps thousands of labels to
/// one draw call.
///
/// The sampled value is used as COVERAGE, not colour: glyphs are tinted by the per-quad colour so
/// one greyscale atlas serves every label colour in the diagram. Rasterising a coloured atlas
/// instead would need one atlas per distinct label colour.
pub const TEXT_ATLAS_WGSL: &str = r#"
struct Camera {
    transform: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var atlas_texture: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;

struct TextQuad {
    @location(0) center: vec2<f32>,
    @location(1) half_extent: vec2<f32>,
    @location(2) uv_min: vec2<f32>,
    @location(3) uv_max: vec2<f32>,
    @location(4) color: vec4<f32>,
    @location(5) run_index: u32,
};

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

var<private> QUAD: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0), vec2<f32>( 1.0, -1.0), vec2<f32>( 1.0,  1.0),
    vec2<f32>(-1.0, -1.0), vec2<f32>( 1.0,  1.0), vec2<f32>(-1.0,  1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, quad: TextQuad) -> VertexOut {
    let corner = QUAD[vertex_index];
    let world = quad.center + corner * quad.half_extent;

    // Corner in [-1,1] maps to the cell's UV rect. Y is flipped because texture space runs downward
    // while the quad's local Y runs upward.
    let t = (corner + vec2<f32>(1.0, 1.0)) * 0.5;
    let uv = vec2<f32>(
        mix(quad.uv_min.x, quad.uv_max.x, t.x),
        mix(quad.uv_max.y, quad.uv_min.y, t.y),
    );

    var out: VertexOut;
    out.clip_position = vec4<f32>(world * camera.transform.xy + camera.transform.zw, 0.0, 1.0);
    out.uv = uv;
    out.color = quad.color;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Coverage, not colour: the atlas is greyscale so one texture serves every label tint.
    let coverage = textureSample(atlas_texture, atlas_sampler, in.uv).r;
    if (coverage <= 0.0) {
        discard;
    }
    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}
"#;

/// WGSL for the instanced edge pass (bd-2u0.2 component 2).
///
/// Each segment is expanded to a screen-aligned quad in the vertex stage: a line of finite width is
/// a rectangle, so no geometry shader and no CPU-side triangulation is needed. One instanced draw
/// covers every segment of every edge.
///
/// Dashes are evaluated from ARC LENGTH along the segment rather than from a texture, which keeps a
/// dotted edge dotted at any zoom and needs no per-pattern resources. `dash = [0, 0]` means solid,
/// and that is the common case, so the branch is cheap and predictable.
pub const EDGE_WGSL: &str = r#"
struct Camera {
    transform: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;

struct EdgeSegment {
    @location(0) from_point: vec2<f32>,
    @location(1) to_point: vec2<f32>,
    @location(2) edge_index: u32,
    @location(3) color: vec4<f32>,
    @location(4) dash: vec2<f32>,
    @location(5) width: f32,
};

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) @interpolate(flat) dash: vec2<f32>,
    // Distance travelled along the segment, in layout units, for dash evaluation.
    @location(2) arc_length: f32,
    // Signed distance across the ribbon, in half-width units, for the antialiased edge.
    @location(3) across: f32,
};

// Two triangles over the segment's ribbon: (start left, start right, end right), (…, end left).
var<private> RIBBON: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, -1.0), vec2<f32>(0.0,  1.0), vec2<f32>(1.0,  1.0),
    vec2<f32>(0.0, -1.0), vec2<f32>(1.0,  1.0), vec2<f32>(1.0, -1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, seg: EdgeSegment) -> VertexOut {
    let corner = RIBBON[vertex_index];
    let delta = seg.to_point - seg.from_point;
    let length_units = max(length(delta), 0.0001);
    let direction = delta / length_units;
    let normal = vec2<f32>(-direction.y, direction.x);

    let half_width = max(seg.width, 0.0001) * 0.5;
    let world = seg.from_point
        + direction * (corner.x * length_units)
        + normal * (corner.y * half_width);

    var out: VertexOut;
    out.clip_position = vec4<f32>(world * camera.transform.xy + camera.transform.zw, 0.0, 1.0);
    out.color = seg.color;
    out.dash = seg.dash;
    out.arc_length = corner.x * length_units;
    out.across = corner.y;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let period = in.dash.x + in.dash.y;
    if (period > 0.0) {
        // Position within one on/off cycle. Discarding in the gap is what makes a dotted edge read
        // as dotted rather than as a lighter solid line.
        let phase = in.arc_length - period * floor(in.arc_length / period);
        if (phase > in.dash.x) {
            discard;
        }
    }

    // One-pixel antialiased edge across the ribbon, from the derivative of the across coordinate.
    let aa = max(fwidth(in.across), 0.0001);
    let coverage = 1.0 - smoothstep(1.0 - aa, 1.0, abs(in.across));
    if (coverage <= 0.0) {
        discard;
    }
    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}
"#;

/// WGSL for the instanced arrowhead pass (bd-2u0.2 component 2, "arrowheads as triangle instances").
///
/// A separate pipeline from [`EDGE_WGSL`] because it draws triangles, not ribbons, and because a
/// head must be drawn AFTER its line so the line does not overdraw the tip.
pub const ARROWHEAD_WGSL: &str = r#"
struct Camera {
    transform: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;

struct Arrowhead {
    @location(0) position: vec2<f32>,
    @location(1) angle: f32,
    @location(2) size: f32,
    @location(3) edge_index: u32,
    @location(4) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

// Tip at the origin, tail swept back along -x. Matches the Canvas2D head: the tip sits ON the
// endpoint and the barbs trail behind it, so the head points where the segment was going.
var<private> HEAD: array<vec2<f32>, 3> = array<vec2<f32>, 3>(
    vec2<f32>( 0.0,  0.0),
    vec2<f32>(-1.0,  0.4),
    vec2<f32>(-1.0, -0.4),
);

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, head: Arrowhead) -> VertexOut {
    let local = HEAD[vertex_index] * head.size;
    let c = cos(head.angle);
    let s = sin(head.angle);
    let rotated = vec2<f32>(local.x * c - local.y * s, local.x * s + local.y * c);
    let world = head.position + rotated;

    var out: VertexOut;
    out.clip_position = vec4<f32>(world * camera.transform.xy + camera.transform.zw, 0.0, 1.0);
    out.color = head.color;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

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
        let head = plan
            .arrowheads
            .first()
            .expect("a directed edge must have a head");

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

    /// The GPU theme constants must equal the Canvas2D config defaults.
    ///
    /// Promised by the doc on `DEFAULT_NODE_FILL_RGBA`. Two renderers that disagree about the
    /// DEFAULT colour repaint every UNSTYLED diagram, which is the largest possible blast radius and
    /// the least likely to be noticed — nobody inspects the colour of a diagram they never styled.
    #[test]
    fn gpu_theme_defaults_match_the_canvas_config() {
        let config = crate::CanvasRenderConfig::default();
        let fill = super::parse_paint_rgba(&config.node_fill).expect("config fill must parse");
        let stroke =
            super::parse_paint_rgba(&config.node_stroke).expect("config stroke must parse");

        for (got, want) in fill.iter().zip(super::DEFAULT_NODE_FILL_RGBA.iter()) {
            assert!(
                (got - want).abs() < 1e-4,
                "fill {fill:?} drifted from the config default"
            );
        }
        for (got, want) in stroke.iter().zip(super::DEFAULT_NODE_STROKE_RGBA.iter()) {
            assert!(
                (got - want).abs() < 1e-4,
                "stroke {stroke:?} drifted from the config default"
            );
        }
    }

    /// The WGSL shape switch must cover exactly the Rust enum.
    ///
    /// Promised by the doc on `NODE_SDF_WGSL`. A shader and an enum drifting apart is silent: the
    /// `default:` arm keeps compiling and every unmatched shape renders as a rectangle, so a new
    /// diamond would simply come out square with nothing failing.
    #[test]
    fn wgsl_shape_constants_match_the_rust_enum() {
        let wgsl = super::NODE_SDF_WGSL;
        for shape in [
            GpuNodeShape::Rect,
            GpuNodeShape::RoundedRect,
            GpuNodeShape::Circle,
            GpuNodeShape::Diamond,
            GpuNodeShape::Cylinder,
            GpuNodeShape::Polygon,
        ] {
            let arm = format!("case {}u:", shape as u32);
            assert!(
                wgsl.contains(&arm),
                "the shader has no arm for {shape:?} ({arm})"
            );
        }
        // And no arm beyond the enum, which would mean the shader outlived a removed variant.
        assert!(
            !wgsl.contains("case 6u:"),
            "the shader switches on a shape the Rust enum does not define"
        );
        assert!(
            wgsl.contains("@location(2) fill") && wgsl.contains("@location(3) stroke"),
            "the vertex attributes do not match GpuNodeInstance's field order"
        );
    }

    #[test]
    fn paint_parsing_accepts_the_forms_a_diagram_writes() {
        let cases: [(&str, [f32; 4]); 7] = [
            ("#ff0000", [1.0, 0.0, 0.0, 1.0]),
            ("#F00", [1.0, 0.0, 0.0, 1.0]),
            ("#ff000080", [1.0, 0.0, 0.0, 0.501_960_8]),
            ("rgb(255,0,0)", [1.0, 0.0, 0.0, 1.0]),
            ("rgba(255, 0, 0, 0.5)", [1.0, 0.0, 0.0, 0.5]),
            ("red", [1.0, 0.0, 0.0, 1.0]),
            ("transparent", [0.0, 0.0, 0.0, 0.0]),
        ];
        for (input, want) in cases {
            let got =
                super::parse_paint_rgba(input).unwrap_or_else(|| panic!("{input} should parse"));
            for (g, w) in got.iter().zip(want.iter()) {
                assert!(
                    (g - w).abs() < 1e-3,
                    "{input} parsed as {got:?}, wanted {want:?}"
                );
            }
        }
    }

    /// CONTROL: junk is REFUSED, never guessed.
    ///
    /// A shader reads whatever is in the buffer, so a parser that fell back to zeroes would paint
    /// transparent-black rather than the theme colour — invisible nodes from a typo.
    #[test]
    fn paint_parsing_refuses_what_it_cannot_honour() {
        for input in [
            "",
            "not a colour",
            "#12345",
            "#gg0000",
            "rgb(255,0)",
            "rgb(255,0,0",
            "url(#x)",
        ] {
            assert!(
                super::parse_paint_rgba(input).is_none(),
                "{input} should have been refused"
            );
        }
    }

    /// A declared fill reaches the GPU instance, through the SAME resolver the raster pass uses.
    #[test]
    fn a_declared_fill_reaches_the_node_instance() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            inline_style: Some(Box::new(fm_core::parse_style_string("fill:#ff0000"))),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode::default());

        let plan = GpuRenderPlan::from_layout(&ir, &test_layout(), 1.5);
        let styled = plan.node_instances.first().expect("a node instance");

        assert!(
            (styled.fill[0] - 1.0).abs() < 1e-4
                && styled.fill[1].abs() < 1e-4
                && styled.fill[2].abs() < 1e-4,
            "the declared fill never reached the instance: {:?}",
            styled.fill
        );
    }

    /// CONTROL: an unstyled node keeps the theme default, and the styling does not leak.
    #[test]
    fn an_unstyled_node_keeps_the_theme_default() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            inline_style: Some(Box::new(fm_core::parse_style_string("fill:#ff0000"))),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode::default());

        let plan = GpuRenderPlan::from_layout(&ir, &test_layout(), 1.5);
        let plain = plan.node_instances.get(1).expect("a second node instance");

        assert_eq!(
            plain.fill,
            super::DEFAULT_NODE_FILL_RGBA,
            "an unstyled node did not keep the theme fill"
        );
        assert_eq!(
            plain.stroke,
            super::DEFAULT_NODE_STROKE_RGBA,
            "an unstyled node did not keep the theme stroke"
        );
    }

    /// The atlas must be deterministic for a given diagram.
    ///
    /// UVs that reshuffle between two renders of the same document would make every golden or
    /// cross-render comparison meaningless, and the reshuffle would be invisible in a screenshot.
    #[test]
    fn the_glyph_atlas_is_deterministic_and_deduplicated() {
        let first = super::GlyphAtlasPlan::for_texts(["Alpha", "Beta"], 32);
        let second = super::GlyphAtlasPlan::for_texts(["Alpha", "Beta"], 32);
        assert_eq!(
            first, second,
            "the same input produced two different atlases"
        );

        // 'a' appears in both words and twice in "Alpha"; the atlas carries one cell for it.
        let distinct: std::collections::BTreeSet<char> =
            "AlphaBeta".chars().filter(|c| !c.is_control()).collect();
        assert_eq!(
            first.cells.len(),
            distinct.len(),
            "the atlas did not deduplicate glyphs: {:?}",
            first.cells
        );

        // Sorted, so `cell()` may binary search.
        let mut sorted = first.cells.clone();
        sorted.sort_by_key(|cell| cell.glyph);
        assert_eq!(first.cells, sorted, "cells are not sorted by glyph");
    }

    /// Every cell must be inside the texture and non-degenerate.
    #[test]
    fn glyph_cells_stay_inside_the_texture() {
        let atlas = super::GlyphAtlasPlan::for_texts(["Gamma", "Delta", "Epsilon"], 32);
        assert!(!atlas.cells.is_empty(), "no cells, so this proves nothing");
        for cell in &atlas.cells {
            assert!(
                cell.uv_min[0] >= 0.0
                    && cell.uv_min[1] >= 0.0
                    && cell.uv_max[0] <= 1.0 + 1e-6
                    && cell.uv_max[1] <= 1.0 + 1e-6,
                "cell for {:?} leaves the texture: {cell:?}",
                cell.glyph
            );
            assert!(
                cell.uv_max[0] > cell.uv_min[0] && cell.uv_max[1] > cell.uv_min[1],
                "cell for {:?} is degenerate: {cell:?}",
                cell.glyph
            );
        }
    }

    /// CONTROL: an empty diagram produces an empty atlas rather than a 1x1 texture of nothing.
    #[test]
    fn an_atlas_with_no_glyphs_is_empty() {
        let atlas = super::GlyphAtlasPlan::for_texts(std::iter::empty(), 32);
        assert!(atlas.cells.is_empty());
        assert_eq!(atlas.texture_px, [0, 0]);
        assert!(atlas.cell('a').is_none());
    }

    /// A label becomes one run whose quads carry that label's glyphs.
    #[test]
    fn a_node_label_becomes_one_run_of_glyph_quads() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.labels.push(fm_core::IrLabel {
            text: String::from("Alpha"),
            span: Span::default(),
        });
        ir.nodes.push(IrNode {
            label: Some(fm_core::IrLabelId(0)),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode::default());

        let plan = GpuRenderPlan::from_layout(&ir, &test_layout(), 1.5);

        assert_eq!(plan.text_runs.len(), 1, "one label should give one run");
        let run = plan.text_runs[0];
        assert_eq!(
            run.quad_count, 5,
            "Alpha has five glyphs; got {} quads",
            run.quad_count
        );
        assert_eq!(
            plan.text_quads.len(),
            usize::try_from(run.quad_count).unwrap_or(0),
            "quad buffer and run range disagree"
        );

        // Every quad's UV must be a real atlas cell, not a zeroed default.
        for quad in &plan.text_quads {
            assert!(
                quad.uv_max[0] > quad.uv_min[0],
                "a quad carries a degenerate UV: {quad:?}"
            );
        }

        // Centred on the node: the run's midpoint should sit at the node centre.
        let first = plan.text_quads.first().expect("a quad");
        let last = plan.text_quads.last().expect("a quad");
        let midpoint = (first.center[0] + last.center[0]) * 0.5;
        let node_center = 10.0 + (40.0 * 0.5);
        assert!(
            (midpoint - node_center).abs() < 1e-3,
            "the run is not centred on its node: midpoint {midpoint} vs {node_center}"
        );
    }

    /// CONTROL: an unlabelled node contributes no run.
    #[test]
    fn an_unlabelled_node_contributes_no_text() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode::default());
        ir.nodes.push(IrNode::default());

        let plan = GpuRenderPlan::from_layout(&ir, &test_layout(), 1.5);

        assert!(
            plan.text_runs.is_empty() && plan.text_quads.is_empty(),
            "an unlabelled diagram produced text"
        );
    }

    /// A dotted arrow must carry its dash to the GPU.
    ///
    /// `-.->` and `-->` mean different things to a reader, so a dash dropped in the plan is a
    /// semantic loss, not a cosmetic one — the GPU would draw a solid line asserting a relationship
    /// the author did not write.
    #[test]
    fn a_dotted_arrow_carries_its_dash_pattern() {
        let ir = ir_with_edge_at_fixture_index(fm_core::ArrowType::DottedArrow);
        let plan = GpuRenderPlan::from_layout(&ir, &test_layout(), 1.25);

        let segment = plan.edge_segments.first().expect("a segment");
        assert!(
            segment.dash[0] > 0.0 && segment.dash[1] > 0.0,
            "the dotted arrow reached the plan solid: {:?}",
            segment.dash
        );
    }

    /// CONTROL: a solid arrow carries NO dash.
    ///
    /// Without this, a bug that stamped the dotted pattern on everything would satisfy the test
    /// above and dot the entire diagram.
    #[test]
    fn a_solid_arrow_carries_no_dash() {
        let ir = ir_with_edge_at_fixture_index(fm_core::ArrowType::Arrow);
        let plan = GpuRenderPlan::from_layout(&ir, &test_layout(), 1.25);

        let segment = plan.edge_segments.first().expect("a segment");
        assert_eq!(
            segment.dash,
            [0.0, 0.0],
            "a solid arrow acquired a dash pattern"
        );
    }

    /// A declared `linkStyle` colour reaches both the segment AND its arrowhead.
    #[test]
    fn a_declared_link_colour_reaches_the_segment_and_its_head() {
        let mut ir = ir_with_edge_at_fixture_index(fm_core::ArrowType::Arrow);
        ir.style_refs.push(fm_core::IrStyleRef {
            target: fm_core::IrStyleTarget::Link(7),
            style: String::from("stroke:#ff0000"),
            span: Span::default(),
        });

        let plan = GpuRenderPlan::from_layout(&ir, &test_layout(), 1.25);
        let segment = plan.edge_segments.first().expect("a segment");

        assert!(
            (segment.color[0] - 1.0).abs() < 1e-4
                && segment.color[1].abs() < 1e-4
                && segment.color[2].abs() < 1e-4,
            "the declared link colour never reached the segment: {:?}",
            segment.color
        );

        if let Some(head) = plan.arrowheads.first() {
            assert_eq!(
                head.color, segment.color,
                "the arrowhead kept a different colour from the line it terminates"
            );
        }
    }

    /// CONTROL: an unstyled edge keeps the theme stroke.
    #[test]
    fn an_unstyled_edge_keeps_the_theme_stroke() {
        let ir = ir_with_edge_at_fixture_index(fm_core::ArrowType::Arrow);
        let plan = GpuRenderPlan::from_layout(&ir, &test_layout(), 1.25);

        let segment = plan.edge_segments.first().expect("a segment");
        assert_eq!(
            segment.color,
            super::DEFAULT_EDGE_STROKE_RGBA,
            "an unstyled edge did not keep the theme stroke"
        );
    }

    /// Every shader the plan ships must declare attributes matching its instance struct.
    ///
    /// Same silent-drift risk as the shape switch: a WGSL location that does not match the Rust
    /// field order still compiles and simply reads the wrong bytes, so the picture is wrong rather
    /// than the build being broken.
    #[test]
    fn the_edge_shaders_declare_the_fields_the_buffers_carry() {
        assert!(
            super::EDGE_WGSL.contains("@location(3) color")
                && super::EDGE_WGSL.contains("@location(4) dash")
                && super::EDGE_WGSL.contains("@location(5) width"),
            "the edge shader attributes do not match GpuEdgeSegment's field order"
        );
        assert!(
            super::ARROWHEAD_WGSL.contains("@location(4) color"),
            "the arrowhead shader does not read the colour the instance carries"
        );
        assert!(
            super::EDGE_WGSL.contains("discard"),
            "the edge shader never discards, so the dash gap would render as a solid line"
        );
    }

    /// A declared `stroke-width` reaches the GPU instance, and an undeclared one gets the default.
    ///
    /// The ABI guard in `gpu_layout` proves the struct, the attribute table and the WGSL agree about
    /// where this field IS. It says nothing about whether anything ever puts a value there, which is
    /// the difference between a wired feature and a field of zeroes the shader faithfully reads.
    ///
    /// Parses REAL mermaid source rather than hand-building IR, for the reason this crate's
    /// dev-dependency note records: a hand-built fixture is how a kanban fix came to pass its own
    /// tests while being unreachable from parser output.
    #[test]
    fn a_declared_stroke_width_reaches_the_gpu_instance() {
        let declared = fm_parser::parse(
            "flowchart TD\n  a[A]\n  classDef thick stroke-width:6\n  class a thick\n",
        )
        .ir;
        let layout = fm_layout::layout_diagram(&declared);
        let plan = GpuRenderPlan::from_layout(&declared, &layout, 1.0);
        assert!(!plan.node_instances.is_empty(), "no instances, so this proves nothing");
        assert!(
            plan.node_instances.iter().any(|i| (i.stroke_width - 6.0).abs() < f32::EPSILON),
            "the declared width never reached an instance: {:?}",
            plan.node_instances.iter().map(|i| i.stroke_width).collect::<Vec<_>>()
        );

        // CONTROL: without a declaration the instance carries the theme default, NOT zero. A zero
        // would make the shader's smoothstep band collapse and the border vanish, which is the
        // silent-wrong-output failure this whole bead family is about.
        let plain = fm_parser::parse("flowchart TD\n  a[A]\n").ir;
        let plain_layout = fm_layout::layout_diagram(&plain);
        let plain_plan = GpuRenderPlan::from_layout(&plain, &plain_layout, 1.0);
        assert!(
            plain_plan
                .node_instances
                .iter()
                .all(|i| (i.stroke_width - super::DEFAULT_NODE_STROKE_WIDTH).abs() < f32::EPSILON),
            "an undeclared node did not get the default width: {:?}",
            plain_plan.node_instances.iter().map(|i| i.stroke_width).collect::<Vec<_>>()
        );
    }
}
