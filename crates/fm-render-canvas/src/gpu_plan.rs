//! GPU-ready primitive extraction from the shared diagram layout.
//!
//! A browser-side WebGPU encoder can upload these tightly packed instances directly:
//! node instances are suitable for an SDF shape shader and edge instances are suitable
//! for instanced line-segment rendering. Text remains a separate glyph-atlas pass.

use fm_core::{MermaidDiagramIr, NodeShape};
use fm_layout::{DiagramLayout, LayoutPoint, LayoutRect};

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
/// Subgraph container fill when the author declared none (bd-dh6cy).
///
/// The linear-RGBA form of `CanvasRenderConfig::cluster_fill` (`rgba(226,232,240,0.44)`), so an
/// undeclared subgraph is the same translucent slate on both surfaces. Taken from the raster
/// default rather than invented, because two renderers disagreeing about an UNDECLARED colour is
/// the same defect class as disagreeing about a declared one -- it just has nobody to report it.
pub const DEFAULT_CLUSTER_FILL_RGBA: [f32; 4] = [0.886_274_5, 0.909_803_9, 0.941_176_5, 0.44];

/// Theme stroke used when the author declared none: `#94a3b8`.
pub const DEFAULT_NODE_STROKE_RGBA: [f32; 4] = [0.580_392_2, 0.639_215_7, 0.721_568_6, 1.0];
/// Border width used when the author declared none. Matches `CanvasRenderConfig::node_stroke_width`
/// and the value the shader previously hard-coded, so an undeclared node renders identically to
/// before this became per-instance.
pub const DEFAULT_NODE_STROKE_WIDTH: f32 = 1.5;

/// Border width of a sequence note, which is NOT the node stroke width (bd-adabx).
///
/// `draw_sequence_notes` sets `line_width(1.0)` literally rather than reading
/// `config.node_stroke_width`, so a note's border is thinner than a node's. Planning it with the
/// node width would be a plausible-looking guess that renders a heavier box than the canvas draws.
pub const SEQUENCE_NOTE_STROKE_WIDTH: f32 = 1.0;

/// Stroke width of a destroy marker's cross.
///
/// `draw_sequence_lifecycle_markers` sets `line_width(1.5)` literally. That happens to EQUAL
/// `DEFAULT_NODE_STROKE_WIDTH` today, which is a coincidence and not a shared source -- so this is
/// its own constant, and no test asserts the two differ, because they do not.
pub const LIFECYCLE_MARKER_STROKE_WIDTH: f32 = 1.5;

/// `edge_index` on a segment that is NOT an edge.
///
/// A cross, a divider or a leader line is a line without an edge behind it, and `GpuEdgeSegment`
/// exists to describe lines. Reusing it means one shader and one vertex layout instead of a
/// parallel type -- but the `edge_index` field would then be a lookup key pointing at an unrelated
/// edge, which is the exact defect the text runs' `GpuTextSource` discriminator was added to
/// prevent. The sentinel makes "no edge" explicit and unresolvable rather than plausible.
pub const NO_EDGE_INDEX: u32 = u32::MAX;

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
    /// Distance along the whole edge at which this segment STARTS, in layout units (bd-f7ctn).
    ///
    /// A routed edge becomes one segment per `points.windows(2)` pair, and each fragment computes
    /// its dash phase from its distance along ITS OWN segment. Without this offset every segment
    /// restarts the pattern at zero, so a dotted edge with bends shows the dashes jumping at each
    /// vertex while the SVG and Canvas2D paths draw one continuous `stroke-dasharray` around the
    /// corners. Carrying the accumulated length makes the pattern march unbroken across the joins.
    pub dash_phase: f32,
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
/// The name in a sequence foot-row header, resolved from the node it mirrors (bd-adabx).
///
/// The head row and the foot row name the same participant, so both read the IR node: its label if
/// it has one, else its id, and only then the layout box's own id as a last resort. Resolving them
/// from one source is what stops the foot row drifting from the head.
///
/// ONE helper for the atlas fill and the quad pass, for the reason recorded on bd-qj46q: a glyph
/// that never reached the atlas emits no quad and the run vanishes silently.
fn mirror_header_label<'a>(
    ir: &'a MermaidDiagramIr,
    node_box: &'a fm_layout::LayoutNodeBox,
) -> &'a str {
    ir.nodes
        .get(node_box.node_index)
        .map_or(node_box.node_id.as_str(), |node| {
            node.label
                .and_then(|label_id| ir.labels.get(label_id.0))
                .map_or(node.id.as_str(), |label| label.text.as_str())
        })
}

/// A cardinality's anchor: inset from its OWN endpoint, along the edge (bd-2ogh5, the GPU twin of the raster bead bd-rk14).
///
/// Each number goes by the end it belongs to, since which end carries `1` and which carries `many`
/// is the entire content. A zero-length first segment has no direction to inset along, so the text
/// sits on the point rather than being pushed to NaN by a divide by zero.
fn cardinality_anchor(from: LayoutPoint, toward: LayoutPoint, inset: f32) -> (f32, f32) {
    let (dx, dy) = (toward.x - from.x, toward.y - from.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len > 0.0 {
        (from.x + (dx / len) * inset, from.y + (dy / len) * inset)
    } else {
        (from.x, from.y)
    }
}

/// Emit one centred text run, or nothing if it would be empty (bd-qj46q, bd-2ogh5).
///
/// ONE place where a glyph missing from the atlas is handled, because that is the failure that does
/// not announce itself: the loop skips uncelled glyphs, so a run whose text never reached
/// `GlyphAtlasPlan::for_texts` emits zero quads and the plan looks structurally correct while
/// rendering nothing. Three callers now share this, and every one of them must also have fed its
/// text into the atlas chain.
///
/// Centred on the anchor because that is what the raster pass does for both edge labels (a centred
/// label plate) and cardinalities (`TextAlign::Center` with a `Middle` baseline).
struct TextSink<'a> {
    atlas: &'a GlyphAtlasPlan,
    quads: &'a mut Vec<GpuTextQuad>,
    runs: &'a mut Vec<GpuTextRun>,
}

impl TextSink<'_> {
    fn push_centred(
        &mut self,
        text: &str,
        center: (f32, f32),
        color: [f32; 4],
        source: GpuTextSource,
        index: usize,
    ) {
        let inked: Vec<char> = text.chars().filter(|c| !c.is_control()).collect();
        if inked.is_empty() {
            return;
        }
        let first_quad = u32::try_from(self.quads.len()).unwrap_or(u32::MAX);
        let advance = DEFAULT_FONT_SIZE_PX * CHAR_ADVANCE_RATIO;
        let half_height = DEFAULT_FONT_SIZE_PX * 0.5;
        let run_width = u16::try_from(inked.len()).map_or(f32::from(u16::MAX), f32::from) * advance;
        let start_x = center.0 - (run_width * 0.5) + (advance * 0.5);
        let run_index = u32::try_from(self.runs.len()).unwrap_or(u32::MAX);
        for (offset, glyph) in inked.iter().enumerate() {
            let Some(cell) = self.atlas.cell(*glyph) else {
                continue;
            };
            let step = u16::try_from(offset).map_or(f32::from(u16::MAX), f32::from);
            self.quads.push(GpuTextQuad {
                center: [start_x + (step * advance), center.1],
                half_extent: [advance * 0.5, half_height],
                uv_min: cell.uv_min,
                uv_max: cell.uv_max,
                color,
                run_index,
            });
        }
        let quad_count = u32::try_from(self.quads.len()).unwrap_or(u32::MAX) - first_quad;
        if quad_count > 0 {
            self.runs.push(GpuTextRun {
                source,
                node_index: u32::try_from(index).unwrap_or(u32::MAX),
                first_quad,
                quad_count,
            });
        }
    }
}

/// The cardinality texts at an edge's two ends, source first (bd-2ogh5, the GPU twin of the raster bead bd-rk14).
///
/// Class values take precedence over ER exactly as the raster pass has it: in practice an edge
/// carries one or the other, since `er_notation` is set by the ER path and `*_cardinality` by the
/// class one, but the precedence is duplicated rather than assumed so the two surfaces cannot
/// disagree if both ever appear.
fn edge_cardinality_texts<'a>(
    ir: &'a MermaidDiagramIr,
    edge: &fm_layout::LayoutEdgePath,
) -> (Option<&'a str>, Option<&'a str>) {
    let Some(ir_edge) = ir.edges.get(edge.edge_index) else {
        return (None, None);
    };
    let er = ir_edge.er_cardinality_labels();
    let source = ir_edge
        .source_cardinality()
        .or_else(|| er.map(|(source, _)| source))
        .filter(|text| !text.is_empty());
    let target = ir_edge
        .target_cardinality()
        .or_else(|| er.map(|(_, target)| target))
        .filter(|text| !text.is_empty());
    (source, target)
}

/// An edge's label text, or `None` when it has none (bd-qj46q).
///
/// ONE lookup used by BOTH the atlas fill and the quad pass. Two copies of "which edges have text"
/// is precisely how a glyph ends up missing from the atlas and its label silently emits zero quads,
/// which is the trap already flagged where the atlas is built.
fn edge_label_text<'a>(
    ir: &'a MermaidDiagramIr,
    edge: &fm_layout::LayoutEdgePath,
) -> Option<&'a str> {
    ir.edges
        .get(edge.edge_index)
        .and_then(|ir_edge| ir_edge.label)
        .and_then(|id| ir.labels.get(id.0))
        .map(|label| label.text.as_str())
}

/// Where the raster pass anchors an edge label, duplicated rather than approximated (bd-qj46q).
///
/// `draw_edges` picks the anchor by POINT COUNT, not by a general midpoint: a 4-point route uses the
/// middle of its two interior points, a straight 2-point edge the middle of the whole span, and any
/// other route the middle POINT itself. Approximating all three as "halfway along the polyline"
/// would put the label somewhere the raster pass never draws it on exactly the routes that bend,
/// which is most of them. Both surfaces then lift it by the same `font_size * 0.8`.
fn edge_label_anchor(points: &[LayoutPoint], label_offset: f32) -> Option<(f32, f32)> {
    let (x, y) = match points.len() {
        0 | 1 => return None,
        2 => (
            f32::midpoint(points[0].x, points[1].x),
            f32::midpoint(points[0].y, points[1].y),
        ),
        4 => (
            f32::midpoint(points[1].x, points[2].x),
            f32::midpoint(points[1].y, points[2].y),
        ),
        len => {
            let mid = &points[len / 2];
            (mid.x, mid.y)
        }
    };
    Some((x, y - label_offset))
}

/// A cluster's title, resolved the same way the Canvas2D pass resolves it (bd-dh6cy).
///
/// Two sources, in the raster path's order: the layout box carries one for diagram types that place
/// it there, and otherwise it comes from the IR cluster's label id. Duplicating that order rather
/// than picking one is what keeps the two surfaces titling the same subgraphs — reading only
/// `LayoutClusterBox::title` would silently drop every flowchart subgraph, whose title lives in the
/// IR.
fn cluster_title<'a>(
    ir: &'a MermaidDiagramIr,
    cluster: &'a fm_layout::LayoutClusterBox,
) -> Option<&'a str> {
    cluster.title.as_deref().or_else(|| {
        ir.clusters
            .get(cluster.cluster_index)
            .and_then(|ir_cluster| ir_cluster.title)
            .and_then(|title_id| ir.labels.get(title_id.0))
            .map(|label| label.text.as_str())
    })
}

/// Which collection a [`GpuTextRun`]'s index refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuTextSource {
    Node,
    Cluster,
    Edge,
    /// A class/ER cardinality at the edge's SOURCE end (`"1" --> "many"`, `}o--o|`).
    ///
    /// Split from `EdgeTargetCardinality` rather than sharing one variant with `node_index`: both
    /// ends of one edge carry the same edge index, so a single variant would emit two runs a
    /// consumer could not tell apart, and WHICH end carries `1` and which carries `many` is the
    /// entire content of a cardinality.
    EdgeSourceCardinality,
    /// A class/ER cardinality at the edge's TARGET end.
    EdgeTargetCardinality,
    /// The body of a sequence note.
    ///
    /// `node_index` here indexes `layout.extensions.sequence_notes`, NOT `ir.nodes`: a note has no
    /// node of its own, and pointing this at an unrelated node would be exactly the wrong-key
    /// defect the discriminator exists to prevent.
    SequenceNote,
    /// The participant name repeated in a sequence diagram's foot row.
    ///
    /// `node_index` is the participant's index in `ir.nodes`, the same node the head row names --
    /// the two rows resolve their text from ONE source so the foot cannot drift from the head.
    MirrorHeader,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuTextRun {
    /// What `node_index` indexes. Without this the field is a lookup key that is NOT the id it
    /// claims to be: a cluster title's run carries a CLUSTER index, and a consumer resolving it
    /// against `ir.nodes` would silently read an unrelated node or none at all.
    pub source: GpuTextSource,
    /// Index back into `MermaidDiagramIr::nodes`, or `::clusters` / `::edges` when `source` says so.
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
///
/// Subgraph containers ARE planned as of bd-dh6cy (`cluster_instances`), with their fill, stroke,
/// border width and opacity resolved through the same helpers the Canvas2D pass uses.
///
/// The text pass plans NODE labels, CLUSTER titles and EDGE labels, each tagged by `GpuTextSource`
/// so a consumer can tell which collection `node_index` indexes. This note is where a reader looks
/// to find out what a plan covers, so it is kept current: it claimed cluster titles were absent for
/// a while after they landed, which is worse than saying nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuRenderPlan {
    pub bounds: LayoutRect,
    /// Subgraph containers, as rect instances (bd-dh6cy).
    ///
    /// ⚠️ DECLARED BEFORE `node_instances` BECAUSE THAT IS THE DRAW ORDER. A container painted after
    /// its contents covers them, and the Canvas2D pass draws clusters first for exactly this
    /// reason -- the peer who added cluster opacity there had to restore `globalAlpha` afterwards
    /// so it did not fade the nodes drawn inside. A consumer that submits these buffers in field
    /// order gets the right picture; one that reorders them gets subgraphs painted over their own
    /// nodes.
    ///
    /// Reuses `GpuNodeInstance` rather than introducing a cluster type: a subgraph box IS a rect
    /// with a fill, a stroke, a border width and an opacity, which is exactly this struct, and the
    /// SDF shader already draws that shape. A parallel type would have needed its own attribute
    /// table and its own WGSL for no expressive gain, and would have had to be kept in step with
    /// this one by hand.
    ///
    /// `node_index` on these instances refers to the CLUSTER index, not a node. It is the label
    /// back-reference the text pass uses -- cluster titles ARE planned now, tagged
    /// `GpuTextSource::Cluster`.
    pub cluster_instances: Vec<GpuNodeInstance>,
    /// Sequence foot-row participant headers, as rect instances (bd-adabx).
    ///
    /// Before the activation bars because `render` draws them first (call order: draw_clusters,
    /// draw_sequence_mirror_headers, draw_activation_bars, draw_edges, draw_nodes). Their labels
    /// are planned too, tagged `GpuTextSource::MirrorHeader`.
    pub mirror_header_instances: Vec<GpuNodeInstance>,
    /// Sequence activation bars, as rect instances (bd-adabx).
    ///
    /// Between the clusters and the edges because that is where `render` draws them (call order:
    /// draw_clusters, draw_activation_bars, draw_edges, draw_nodes). Reuses `GpuNodeInstance` for
    /// the same reason the clusters do: an activation bar IS a filled, stroked rect, which is
    /// exactly this struct and a shape the SDF shader already draws.
    ///
    /// `node_index` here is the PARTICIPANT node index the bar sits on, which is a genuine index
    /// into `ir.nodes` -- unlike the cluster instances, where it indexes clusters.
    pub activation_instances: Vec<GpuNodeInstance>,
    /// Destroy-marker crosses, as line segments (bd-adabx).
    ///
    /// Two segments per marker. Reuses `GpuEdgeSegment` because a cross is two lines and that is
    /// what the type describes -- their `edge_index` is [`NO_EDGE_INDEX`], since no edge produced
    /// them.
    ///
    /// Its own field rather than a shared "decorations" buffer: the remaining line sources (cluster
    /// dividers, axis ticks, state-note leaders) are drawn at DIFFERENT points in the raster pass,
    /// and one buffer submitted at one point cannot match several positions in the draw order.
    pub lifecycle_marker_segments: Vec<GpuEdgeSegment>,
    /// Sequence notes, as rect instances (bd-adabx).
    ///
    /// After the activation bars and before the edges, matching the raster call order. Their text
    /// is planned too, tagged `GpuTextSource::SequenceNote`.
    pub sequence_note_instances: Vec<GpuNodeInstance>,
    pub edge_segments: Vec<GpuEdgeSegment>,
    /// Triangle instances for edge arrowheads.
    pub arrowheads: Vec<GpuArrowheadInstance>,
    /// ⚠️ AFTER THE EDGES, BECAUSE THE RASTER PASS DRAWS NODES LAST (bd-adabx).
    ///
    /// `render` calls `draw_edges` and THEN `draw_nodes`, so an opaque node fill covers any edge
    /// routed under it. This field sat before `edge_segments`, which -- under the submit-in-field-
    /// order contract the cluster field documents -- paints the edges ON TOP of the nodes instead.
    /// Every route that passes under a node, every self-loop, and every arrowhead meeting a node
    /// boundary renders differently between the two surfaces under that order.
    ///
    /// Pinned by `the_plan_field_order_matches_the_raster_draw_order`, which reads the call order
    /// out of renderer.rs rather than trusting this comment to stay true.
    pub node_instances: Vec<GpuNodeInstance>,
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
        // Clusters first, so the buffer order is the draw order.
        let mut cluster_instances = Vec::with_capacity(layout.clusters.len());
        for cluster in &layout.clusters {
            let (declared_fill, declared_stroke) =
                crate::renderer::resolve_cluster_colors(ir, cluster.cluster_index);
            let opacity = crate::renderer::resolve_cluster_opacity(ir, cluster.cluster_index)
                .map(|value| value.clamp(0.0, 1.0) as f32)
                .unwrap_or(1.0);
            let fill = declared_fill
                .as_deref()
                .and_then(parse_paint_rgba)
                .unwrap_or(DEFAULT_CLUSTER_FILL_RGBA);
            let stroke = declared_stroke
                .as_deref()
                .and_then(parse_paint_rgba)
                .unwrap_or(DEFAULT_NODE_STROKE_RGBA);
            let stroke_width =
                crate::renderer::resolve_cluster_stroke_width(ir, cluster.cluster_index)
                    .map(|width| width as f32)
                    .filter(|width| width.is_finite() && *width > 0.0)
                    .unwrap_or(DEFAULT_NODE_STROKE_WIDTH);
            cluster_instances.push(GpuNodeInstance {
                center: [
                    cluster.bounds.x + (cluster.bounds.width * 0.5),
                    cluster.bounds.y + (cluster.bounds.height * 0.5),
                ],
                half_extent: [cluster.bounds.width * 0.5, cluster.bounds.height * 0.5],
                fill: [fill[0], fill[1], fill[2], fill[3] * opacity],
                stroke: [stroke[0], stroke[1], stroke[2], stroke[3] * opacity],
                stroke_width,
                shape: GpuNodeShape::Rect as u32,
                node_index: u32::try_from(cluster.cluster_index).unwrap_or(u32::MAX),
            });
        }

        // SEQUENCE FOOT-ROW HEADERS (bd-adabx). Same rect the participant's head row is, drawn
        // with the node fill, stroke and width, and carrying the participant's name.
        let mut mirror_header_instances =
            Vec::with_capacity(layout.extensions.sequence_mirror_headers.len());
        for node_box in &layout.extensions.sequence_mirror_headers {
            if node_box.bounds.width <= 0.0 || node_box.bounds.height <= 0.0 {
                continue;
            }
            mirror_header_instances.push(GpuNodeInstance {
                center: [
                    node_box.bounds.x + (node_box.bounds.width * 0.5),
                    node_box.bounds.y + (node_box.bounds.height * 0.5),
                ],
                half_extent: [node_box.bounds.width * 0.5, node_box.bounds.height * 0.5],
                fill: DEFAULT_NODE_FILL_RGBA,
                stroke: DEFAULT_NODE_STROKE_RGBA,
                stroke_width: DEFAULT_NODE_STROKE_WIDTH,
                shape: GpuNodeShape::Rect as u32,
                node_index: u32::try_from(node_box.node_index).unwrap_or(u32::MAX),
            });
        }

        // SEQUENCE ACTIVATION BARS (bd-adabx). Plain rects on a participant's lifeline, drawn by
        // the raster pass with the node fill, node stroke and node stroke width -- so they are
        // planned with the same three, not with a bar-specific guess.
        //
        // The degenerate-size skip mirrors `draw_activation_bars`: a zero-width or zero-height bar
        // is not drawn there, and planning one would put a rect with no area in the buffer for a
        // consumer to rasterise into nothing.
        let mut activation_instances = Vec::with_capacity(layout.extensions.activation_bars.len());
        for bar in &layout.extensions.activation_bars {
            if bar.bounds.width <= 0.0 || bar.bounds.height <= 0.0 {
                continue;
            }
            activation_instances.push(GpuNodeInstance {
                center: [
                    bar.bounds.x + (bar.bounds.width * 0.5),
                    bar.bounds.y + (bar.bounds.height * 0.5),
                ],
                half_extent: [bar.bounds.width * 0.5, bar.bounds.height * 0.5],
                fill: DEFAULT_NODE_FILL_RGBA,
                stroke: DEFAULT_NODE_STROKE_RGBA,
                stroke_width: DEFAULT_NODE_STROKE_WIDTH,
                shape: GpuNodeShape::Rect as u32,
                node_index: u32::try_from(bar.participant_index).unwrap_or(u32::MAX),
            });
        }

        // DESTROY MARKERS (bd-adabx). The X that terminates a destroyed participant's lifeline:
        // two crossing segments through `center`, half a `size` in each direction.
        //
        // The zero-size skip mirrors the raster pass, which bails when `half <= 0.0` -- a cross
        // with no extent is drawn by nobody.
        let mut lifecycle_marker_segments =
            Vec::with_capacity(layout.extensions.sequence_lifecycle_markers.len() * 2);
        for marker in &layout.extensions.sequence_lifecycle_markers {
            match marker.kind {
                fm_layout::LayoutSequenceLifecycleMarkerKind::Destroy => {
                    let half = marker.size * 0.5;
                    if half <= 0.0 {
                        continue;
                    }
                    let (cx, cy) = (marker.center.x, marker.center.y);
                    for (from, to) in [
                        ([cx - half, cy - half], [cx + half, cy + half]),
                        ([cx + half, cy - half], [cx - half, cy + half]),
                    ] {
                        lifecycle_marker_segments.push(GpuEdgeSegment {
                            from,
                            to,
                            edge_index: NO_EDGE_INDEX,
                            color: DEFAULT_EDGE_STROKE_RGBA,
                            dash_phase: 0.0,
                            dash: [0.0, 0.0],
                            width: LIFECYCLE_MARKER_STROKE_WIDTH,
                        });
                    }
                }
            }
        }

        // SEQUENCE NOTES (bd-adabx). Rect plus centred body text.
        //
        // NO degenerate-size skip here, deliberately: unlike draw_activation_bars and
        // draw_sequence_mirror_headers, draw_sequence_notes has no such guard and draws whatever
        // bounds it is given. Adding one would make the plan disagree with the canvas on the very
        // case the guard is about, so the two surfaces stay identical instead of the plan being
        // independently "tidier".
        let mut sequence_note_instances =
            Vec::with_capacity(layout.extensions.sequence_notes.len());
        for note in &layout.extensions.sequence_notes {
            sequence_note_instances.push(GpuNodeInstance {
                center: [
                    note.bounds.x + (note.bounds.width * 0.5),
                    note.bounds.y + (note.bounds.height * 0.5),
                ],
                half_extent: [note.bounds.width * 0.5, note.bounds.height * 0.5],
                fill: DEFAULT_NODE_FILL_RGBA,
                stroke: DEFAULT_NODE_STROKE_RGBA,
                stroke_width: SEQUENCE_NOTE_STROKE_WIDTH,
                shape: GpuNodeShape::Rect as u32,
                node_index: u32::try_from(sequence_note_instances.len()).unwrap_or(u32::MAX),
            });
        }

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
            // A declared `opacity` multiplies BOTH alphas, which is what the property means: CSS
            // `opacity` applies to the whole element, and the Canvas2D pass implements it with
            // `globalAlpha` around the shape. Folding it into the instance alphas is the shader
            // equivalent -- fs_main already composites stroke over fill using those alphas, so a
            // half-transparent node comes out half-transparent as a unit rather than as two
            // independently faded layers.
            //
            // Fourth field in this struct resolved from the author's styling, and the fourth time
            // the field ALREADY EXISTED while nothing wrote the declared value into it: alpha rode
            // in on the colour and the separate `opacity` declaration was dropped.
            let opacity = crate::renderer::resolve_node_opacity(ir, node.node_index)
                .map(|value| value.clamp(0.0, 1.0) as f32)
                .unwrap_or(1.0);
            let fill = [fill[0], fill[1], fill[2], fill[3] * opacity];
            let stroke = [stroke[0], stroke[1], stroke[2], stroke[3] * opacity];

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

            // The dash pattern belongs to the EDGE, not to a segment of it, so each segment records
            // how far along the edge it begins and the shader offsets its phase by that. Accumulated
            // in the same order the segments are emitted, which is the order the points were routed.
            let mut dash_phase = 0.0f32;
            for points in edge.points.windows(2) {
                let [from, to] = points else {
                    continue;
                };
                edge_segments.push(GpuEdgeSegment {
                    from: [from.x, from.y],
                    to: [to.x, to.y],
                    edge_index,
                    color,
                    dash_phase,
                    dash,
                    width,
                });
                let dx = to.x - from.x;
                let dy = to.y - from.y;
                dash_phase += (dx * dx + dy * dy).sqrt();
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

        // PER-RUN FONT SIZE (bd-eudpo). Resolved from the node, exactly like fill, stroke, stroke
        // width and opacity -- NOT from the layout text primitive, whose `font_size` is a hardcoded
        // 14.0/12.0 in every branch of `build_render_scene` and would have delivered nothing.
        let run_font_px: Vec<f32> = labelled
            .iter()
            .map(|(node_index, _, _)| {
                crate::renderer::resolve_declared_node_font(
                    ir,
                    usize::try_from(*node_index).unwrap_or(usize::MAX),
                )
                .size
                .map(|size| size as f32)
                .filter(|size| size.is_finite() && *size > 0.0)
                .unwrap_or(DEFAULT_FONT_SIZE_PX)
            })
            .collect();

        // ⚠️ THE ATLAS MUST GROW WITH THE LARGEST LABEL, or this renders BLURRY rather than wrong --
        // a failure that survives review because the geometry is right and only the raster is soft.
        // The default cell is 32px for a 14px glyph, so the headroom ratio is what scales; smaller
        // labels then sample DOWN from a sharp cell, which is the harmless direction. Capped so a
        // pathological declaration cannot ask a backend for an enormous texture.
        let max_font_px = run_font_px.iter().copied().fold(DEFAULT_FONT_SIZE_PX, f32::max);
        let cell_px = ((DEFAULT_GLYPH_CELL_PX as f32) * (max_font_px / DEFAULT_FONT_SIZE_PX))
            .ceil()
            .clamp(DEFAULT_GLYPH_CELL_PX as f32, 256.0) as u32;

        let glyph_atlas =
            // ⚠️ CLUSTER TITLES AND EDGE LABELS MUST BE IN THE ATLAS OR THEY VANISH WITHOUT A TRACE. The quad loop
            // does `let Some(cell) = glyph_atlas.cell(glyph) else { continue }`, so a glyph the
            // atlas never saw is skipped silently — the title would emit zero quads and the plan
            // would look correct. Feeding both sets in is what makes the titles renderable at all.
            GlyphAtlasPlan::for_texts(
                labelled
                    .iter()
                    .map(|(_, _, text)| *text)
                    .chain(layout.clusters.iter().filter_map(|c| cluster_title(ir, c)))
                    .chain(
                        layout
                            .edges
                            .iter()
                            .filter(|edge| !edge.bundled)
                            .filter_map(|edge| edge_label_text(ir, edge)),
                    )
                    // CARDINALITIES GO IN THE ATLAS TOO. Same trap: a `1` that never reached the
                    // atlas emits no quad and the run vanishes without an error anywhere.
                    .chain(
                        layout
                            .edges
                            .iter()
                            .filter(|edge| !edge.bundled)
                            .flat_map(|edge| {
                                let (source, target) = edge_cardinality_texts(ir, edge);
                                [source, target]
                            })
                            .flatten(),
                    )
                    // FOOT-ROW NAMES GO IN THE ATLAS TOO -- same silent-vanish trap.
                    .chain(
                        layout
                            .extensions
                            .sequence_mirror_headers
                            .iter()
                            .map(|node_box| mirror_header_label(ir, node_box)),
                    )
                    // NOTE BODIES GO IN THE ATLAS TOO -- same silent-vanish trap.
                    .chain(
                        layout
                            .extensions
                            .sequence_notes
                            .iter()
                            .map(|note| note.text.as_str()),
                    ),
                cell_px,
            );
        let mut text_quads: Vec<GpuTextQuad> = Vec::new();
        let mut text_runs: Vec<GpuTextRun> = Vec::with_capacity(labelled.len());

        for (run_index, (node_index, center, text)) in labelled.iter().enumerate() {
            let run_index_u32 = u32::try_from(run_index).unwrap_or(u32::MAX);
            let font_px = run_font_px.get(run_index).copied().unwrap_or(DEFAULT_FONT_SIZE_PX);
            let advance = font_px * CHAR_ADVANCE_RATIO;
            let half_height = font_px * 0.5;
            let first_quad = u32::try_from(text_quads.len()).unwrap_or(u32::MAX);

            // Control characters carry no ink and are excluded from the atlas, so they must not
            // consume an advance either — otherwise a label with a newline would render with a gap
            // where nothing is drawn.
            // The author's declared label colour, resolved through the SAME helper the Canvas2D
            // pass uses (bd-lvj3). This quad carried DEFAULT_LABEL_RGBA unconditionally, so
            // `style a color:#f00` and a `classDef` carrying `color:` reached the raster path and
            // stopped here -- the field existed and was populated with a constant, which is the
            // failure an ABI check cannot see and a plan test can.
            //
            // Resolved once per RUN rather than per glyph: every glyph of one label belongs to one
            // node, so resolving inside the glyph loop would repeat the lookup for each character
            // and could not produce a different answer.
            let label_rgba = crate::renderer::resolve_node_text_color(ir, usize::try_from(*node_index).unwrap_or(usize::MAX))
                .as_deref()
                .and_then(parse_paint_rgba)
                .unwrap_or(DEFAULT_LABEL_RGBA);

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
                    color: label_rgba,
                    run_index: run_index_u32,
                });
            }

            let quad_count = u32::try_from(text_quads.len()).unwrap_or(u32::MAX) - first_quad;
            text_runs.push(GpuTextRun {
                source: GpuTextSource::Node,
                node_index: *node_index,
                first_quad,
                quad_count,
            });
        }

        // CLUSTER TITLES (bd-dh6cy). Containers were planned without their titles; this is the
        // other half.
        //
        // ⚠️ LEFT-ALIGNED AT THE TOP-LEFT INSET, NOT CENTRED. The Canvas2D pass draws a subgraph
        // title with `fill_text(title, x + 8.0, y + 4.0)` — a corner label, not a centred one — so
        // centring these the way node labels are centred would put every subgraph title in the
        // middle of its own box, on top of the nodes it contains. The first glyph's CENTRE is half
        // an advance right of the inset because a quad is centred on its cell.
        for cluster in &layout.clusters {
            let Some(title) = cluster_title(ir, cluster) else {
                continue;
            };
            let inked: Vec<char> = title.chars().filter(|c| !c.is_control()).collect();
            if inked.is_empty() {
                continue;
            }
            let first_quad = u32::try_from(text_quads.len()).unwrap_or(u32::MAX);
            let advance = DEFAULT_FONT_SIZE_PX * CHAR_ADVANCE_RATIO;
            let half_height = DEFAULT_FONT_SIZE_PX * 0.5;
            let colour = crate::renderer::resolve_cluster_text_color(ir, cluster.cluster_index)
                .as_deref()
                .and_then(parse_paint_rgba)
                .unwrap_or(DEFAULT_LABEL_RGBA);
            for (offset, glyph) in inked.iter().enumerate() {
                let Some(cell) = glyph_atlas.cell(*glyph) else {
                    continue;
                };
                let step = u16::try_from(offset).map_or(f32::from(u16::MAX), f32::from);
                text_quads.push(GpuTextQuad {
                    center: [
                        cluster.bounds.x + 8.0 + (advance * 0.5) + (step * advance),
                        cluster.bounds.y + 4.0 + half_height,
                    ],
                    half_extent: [advance * 0.5, half_height],
                    uv_min: cell.uv_min,
                    uv_max: cell.uv_max,
                    color: colour,
                    run_index: u32::try_from(text_runs.len()).unwrap_or(u32::MAX),
                });
            }
            let quad_count = u32::try_from(text_quads.len()).unwrap_or(u32::MAX) - first_quad;
            if quad_count > 0 {
                text_runs.push(GpuTextRun {
                    source: GpuTextSource::Cluster,
                    node_index: u32::try_from(cluster.cluster_index).unwrap_or(u32::MAX),
                    first_quad,
                    quad_count,
                });
            }
        }

        // EDGE LABELS AND CARDINALITIES (bd-qj46q, bd-2ogh5). `draw_edges` emits a fill_text for a
        // labelled edge and one PER END for a class/ER cardinality, so without this pass a
        // `A -->|yes| B` flowchart planned its edge and dropped the word on it, and a class diagram
        // planned a relationship with no `1` or `many` at either end.
        //
        // Cardinalities are placed OUTSIDE the label branch on purpose, mirroring the raster pass:
        // an edge may carry cardinality and no label at all.
        let mut sink = TextSink {
            atlas: &glyph_atlas,
            quads: &mut text_quads,
            runs: &mut text_runs,
        };
        for edge in layout.edges.iter().filter(|edge| !edge.bundled) {
            let colour = crate::renderer::resolve_edge_label_color(ir, edge.edge_index)
                .as_deref()
                .and_then(parse_paint_rgba)
                .unwrap_or(DEFAULT_LABEL_RGBA);

            if let Some(text) = edge_label_text(ir, edge)
                && let Some(anchor) = edge_label_anchor(&edge.points, DEFAULT_FONT_SIZE_PX * 0.8)
            {
                sink.push_centred(text, anchor, colour, GpuTextSource::Edge, edge.edge_index);
            }

            let (source_text, target_text) = edge_cardinality_texts(ir, edge);
            if (source_text.is_some() || target_text.is_some()) && edge.points.len() >= 2 {
                let last = edge.points.len() - 1;
                let ends = [
                    (source_text, 0, 1, GpuTextSource::EdgeSourceCardinality),
                    (target_text, last, last - 1, GpuTextSource::EdgeTargetCardinality),
                ];
                for (text, from_idx, toward_idx, source) in ends {
                    let Some(text) = text else {
                        continue;
                    };
                    let from = edge.points[from_idx];
                    let toward = edge.points[toward_idx];
                    sink.push_centred(
                        text,
                        cardinality_anchor(from, toward, DEFAULT_FONT_SIZE_PX * 1.2),
                        colour,
                        source,
                        edge.edge_index,
                    );
                }
            }
        }

        // FOOT-ROW LABELS (bd-adabx). Centred in the box, exactly as draw_sequence_mirror_headers
        // centres them, and skipped when the box is degenerate or the name is empty -- the raster
        // pass draws neither.
        for node_box in &layout.extensions.sequence_mirror_headers {
            if node_box.bounds.width <= 0.0 || node_box.bounds.height <= 0.0 {
                continue;
            }
            sink.push_centred(
                mirror_header_label(ir, node_box),
                (
                    node_box.bounds.x + (node_box.bounds.width * 0.5),
                    node_box.bounds.y + (node_box.bounds.height * 0.5),
                ),
                DEFAULT_LABEL_RGBA,
                GpuTextSource::MirrorHeader,
                node_box.node_index,
            );
        }

        // NOTE BODIES (bd-adabx). Centred in the box exactly as draw_sequence_notes centres them,
        // and skipped when empty, which is the one case that pass also skips.
        for (index, note) in layout.extensions.sequence_notes.iter().enumerate() {
            if note.text.is_empty() {
                continue;
            }
            sink.push_centred(
                note.text.as_str(),
                (
                    note.bounds.x + (note.bounds.width * 0.5),
                    note.bounds.y + (note.bounds.height * 0.5),
                ),
                DEFAULT_LABEL_RGBA,
                GpuTextSource::SequenceNote,
                index,
            );
        }

        Self {
            bounds: layout.bounds,
            cluster_instances,
            mirror_header_instances,
            activation_instances,
            lifecycle_marker_segments,
            sequence_note_instances,
            edge_segments,
            arrowheads,
            node_instances,
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
    @location(6) dash_phase: f32,
};

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) @interpolate(flat) dash: vec2<f32>,
    // Distance travelled along the segment, in layout units, for dash evaluation.
    @location(2) arc_length: f32,
    // Where this segment starts along the whole edge, so the pattern continues across joins.
    @location(4) @interpolate(flat) dash_phase: f32,
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
    out.dash_phase = instance.dash_phase;
    out.across = corner.y;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let period = in.dash.x + in.dash.y;
    if (period > 0.0) {
        // Position within one on/off cycle. Discarding in the gap is what makes a dotted edge read
        // as dotted rather than as a lighter solid line.
        // Offset by where this segment began along the edge (bd-f7ctn), so a bend does not
        // restart the pattern.
        let along = in.arc_length + in.dash_phase;
        let phase = along - period * floor(along / period);
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

    /// A labelled edge reaches the GPU plan WITH its label (bd-qj46q).
    ///
    /// `draw_edges` emits one `fill_text` per labelled edge. The plan's text pass covered nodes and
    /// cluster titles only, so `A -->|yes| B` planned the line and silently dropped the word on it.
    /// The anchor is checked, not just the run's existence: a run placed somewhere the raster pass
    /// never draws is still a wrong picture, and the point-count rule is the part most likely to be
    /// "simplified" into a plain polyline midpoint by a later reader.
    #[test]
    fn a_labelled_edge_reaches_the_gpu_plan_with_its_text() {
        let mut ir = ir_with_edge_at_fixture_index(fm_core::ArrowType::Arrow);
        ir.labels.push(fm_core::IrLabel {
            text: "yes".to_string(),
            span: Default::default(),
        });
        ir.edges[7].label = Some(fm_core::IrLabelId(0));

        let plan = super::GpuRenderPlan::from_layout(&ir, &test_layout(), 1.25);

        let runs: Vec<_> = plan
            .text_runs
            .iter()
            .filter(|run| run.source == super::GpuTextSource::Edge)
            .collect();
        assert_eq!(runs.len(), 1, "one labelled, unbundled edge means one run");
        assert_eq!(runs[0].node_index, 7, "the run must index ir.edges");
        assert_eq!(runs[0].quad_count, 3, "three inked glyphs in \"yes\"");

        // The fixture edge has THREE points, so the raster rule anchors on the middle POINT
        // (70.0, 35.0) -- not on the midpoint of the whole span, which would be 70.0 but at a
        // different y, and not on interior-pair midpoints, which is the 4-point branch.
        let quads = &plan.text_quads[runs[0].first_quad as usize..][..runs[0].quad_count as usize];
        let mean_x = quads.iter().map(|q| q.center[0]).sum::<f32>() / 3.0;
        assert!(
            (mean_x - 70.0).abs() < 0.01,
            "the run must be CENTRED on the anchor, not inset from it: {mean_x}"
        );
        for quad in quads {
            assert!(
                (quad.center[1] - (35.0 - super::DEFAULT_FONT_SIZE_PX * 0.8)).abs() < 0.01,
                "the label must be lifted off the line by the raster pass's own offset"
            );
        }
    }

    /// The anchor follows POINT COUNT, the way `draw_edges` does (bd-qj46q).
    #[test]
    fn the_edge_label_anchor_follows_the_raster_point_count_rule() {
        let p = |x: f32, y: f32| super::LayoutPoint { x, y };

        assert_eq!(super::edge_label_anchor(&[], 0.0), None);
        assert_eq!(super::edge_label_anchor(&[p(1.0, 1.0)], 0.0), None);

        // Two points: the middle of the whole span.
        assert_eq!(
            super::edge_label_anchor(&[p(0.0, 0.0), p(10.0, 20.0)], 0.0),
            Some((5.0, 10.0))
        );
        // Four points: the middle of the INTERIOR pair, which is not the middle of the span.
        assert_eq!(
            super::edge_label_anchor(&[p(0.0, 0.0), p(2.0, 4.0), p(6.0, 8.0), p(100.0, 0.0)], 0.0),
            Some((4.0, 6.0))
        );
        // Anything else: the middle POINT itself.
        assert_eq!(
            super::edge_label_anchor(&[p(0.0, 0.0), p(3.0, 7.0), p(50.0, 50.0)], 0.0),
            Some((3.0, 7.0))
        );
        // The lift applies to every branch.
        assert_eq!(
            super::edge_label_anchor(&[p(0.0, 0.0), p(10.0, 20.0)], 4.0),
            Some((5.0, 6.0))
        );
    }

    /// Class/ER cardinalities reach the GPU plan at THEIR OWN ends (bd-2ogh5, the GPU twin of the raster bead bd-rk14).
    ///
    /// `"1" --> "many"` lives in `IrEdgeExtras`, not `edge.label`, so the edge-label pass does not
    /// see it and a class diagram planned its relationship lines with no numbers at either end.
    /// The two ends are checked separately and against independently computed anchors: a run that
    /// exists but sits at the wrong end inverts the meaning of the diagram, which is a worse
    /// failure than drawing nothing.
    #[test]
    fn edge_cardinalities_reach_the_gpu_plan_at_both_ends() {
        let mut ir = ir_with_edge_at_fixture_index(fm_core::ArrowType::Arrow);
        ir.edges[7].extras = Some(Box::new(fm_core::IrEdgeExtras {
            source_cardinality: Some("1".into()),
            target_cardinality: Some("many".into()),
            ..Default::default()
        }));

        let plan = super::GpuRenderPlan::from_layout(&ir, &test_layout(), 1.25);
        let run_at = |source: super::GpuTextSource| {
            plan.text_runs
                .iter()
                .find(|run| run.source == source)
                .copied()
                .unwrap_or_else(|| panic!("no run for {source:?}"))
        };

        let source_run = run_at(super::GpuTextSource::EdgeSourceCardinality);
        let target_run = run_at(super::GpuTextSource::EdgeTargetCardinality);
        assert_eq!(source_run.quad_count, 1, "\"1\" is one glyph");
        assert_eq!(target_run.quad_count, 4, "\"many\" is four glyphs");
        assert_eq!(source_run.node_index, 7);
        assert_eq!(target_run.node_index, 7);

        // Independently computed, NOT read back from cardinality_anchor, or this would assert the
        // implementation against itself. The fixture edge runs (50,35) -> (70,35) -> (90,30), and
        // the inset is DEFAULT_FONT_SIZE_PX * 1.2 = 16.8 along each end's own first segment.
        let centre_of = |run: super::GpuTextRun| {
            let quads =
                &plan.text_quads[run.first_quad as usize..][..run.quad_count as usize];
            let x = quads.iter().map(|q| q.center[0]).sum::<f32>() / quads.len() as f32;
            (x, quads[0].center[1])
        };
        let (sx, sy) = centre_of(source_run);
        assert!((sx - 66.8).abs() < 0.01 && (sy - 35.0).abs() < 0.01, "source at ({sx}, {sy})");
        let (tx, ty) = centre_of(target_run);
        assert!((tx - 73.702).abs() < 0.01 && (ty - 34.075).abs() < 0.01, "target at ({tx}, {ty})");
    }

    /// A degenerate first segment must not push the text to NaN (bd-2ogh5, the GPU twin of the raster bead bd-rk14).
    #[test]
    fn a_zero_length_segment_anchors_a_cardinality_on_the_point() {
        let p = |x: f32, y: f32| super::LayoutPoint { x, y };
        let (x, y) = super::cardinality_anchor(p(12.0, 8.0), p(12.0, 8.0), 16.8);
        assert!(x.is_finite() && y.is_finite(), "a zero-length segment divided by zero");
        assert_eq!((x, y), (12.0, 8.0));

        // And a normal segment insets along it, so the guard did not flatten the real case.
        let (x, y) = super::cardinality_anchor(p(0.0, 0.0), p(10.0, 0.0), 4.0);
        assert_eq!((x, y), (4.0, 0.0));
    }

    /// A dash pattern runs along the WHOLE edge, not restarting at every bend (bd-f7ctn).
    ///
    /// A routed edge is emitted as one segment per point pair, and the fragment shader derives its
    /// dash phase from the distance along the segment it is in. With no carried offset, every
    /// segment restarted the pattern at zero, so a dotted edge with bends showed the dashes
    /// snapping back at each vertex while SVG and Canvas2D draw one continuous `stroke-dasharray`
    /// through the corners. The shader arithmetic cannot be executed here (no device, no naga), but
    /// the data it reads can: each segment must carry the summed length of the segments before it.
    ///
    /// The multi-segment assertion is not decoration. The accumulator is trivially "correct" on an
    /// edge with one segment — every phase is 0.0 — so a fixture that never bends would pass this
    /// test with the field deleted.
    #[test]
    fn a_bent_edge_carries_its_dash_phase_across_the_joins() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let plan = GpuRenderPlan::from_layout(&ir, &test_layout(), 1.25);

        assert!(
            plan.edge_segments.len() >= 2,
            "fixture must produce a BENT edge or this test proves nothing"
        );

        let mut expected = 0.0f32;
        for (index, segment) in plan.edge_segments.iter().enumerate() {
            assert!(
                (segment.dash_phase - expected).abs() < 1e-3,
                "segment {index} starts at {} along the edge, expected {expected}",
                segment.dash_phase
            );
            let dx = segment.to[0] - segment.from[0];
            let dy = segment.to[1] - segment.from[1];
            expected += (dx * dx + dy * dy).sqrt();
        }

        assert!(
            plan.edge_segments[1].dash_phase > 0.0,
            "the second segment must begin PAST the start of the edge"
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

    /// A declared label colour reaches the GPU text quads (bd-lvj3).
    ///
    /// `GpuTextQuad::color` existed and was filled with `DEFAULT_LABEL_RGBA` for every glyph, so the
    /// field looked wired from every angle an ABI or struct check can see. Only a test that asks
    /// what VALUE arrives can tell a resolved colour from a constant.
    #[test]
    fn a_declared_label_colour_reaches_the_gpu_text_quads() {
        let declared =
            fm_parser::parse("flowchart TD\n  a[Alpha]\n  style a color:#ff0000\n").ir;
        let layout = fm_layout::layout_diagram(&declared);
        let plan = GpuRenderPlan::from_layout(&declared, &layout, 1.0);

        assert!(!plan.text_quads.is_empty(), "no glyphs, so this proves nothing");
        assert!(
            plan.text_quads
                .iter()
                .any(|q| q.color[0] > 0.9 && q.color[1] < 0.1 && q.color[2] < 0.1),
            "no glyph carries the declared red: {:?}",
            plan.text_quads.iter().map(|q| q.color).take(4).collect::<Vec<_>>()
        );

        // CONTROL: an undeclared label keeps the theme default. Without this, returning red for
        // everything would satisfy the assertion above while repainting every label in every
        // diagram -- the same shape as the malformed-colour controls on the raster path.
        let plain = fm_parser::parse("flowchart TD\n  a[Alpha]\n").ir;
        let plain_layout = fm_layout::layout_diagram(&plain);
        let plain_plan = GpuRenderPlan::from_layout(&plain, &plain_layout, 1.0);
        assert!(
            plain_plan.text_quads.iter().all(|q| q.color == super::DEFAULT_LABEL_RGBA),
            "an undeclared label did not keep the theme colour: {:?}",
            plain_plan.text_quads.iter().map(|q| q.color).take(4).collect::<Vec<_>>()
        );
    }

    /// A declared `opacity` reaches the GPU instance alphas (bd-lvj3, bd-2u0.2).
    ///
    /// The alpha channel was never missing -- it arrived on the colour -- so the field looked
    /// populated. What was dropped is the separate `opacity` DECLARATION, which the Canvas2D pass
    /// applies with `globalAlpha` and the GPU pass ignored entirely.
    #[test]
    fn a_declared_opacity_reaches_the_gpu_instance() {
        let declared = fm_parser::parse("flowchart TD\n  a[A]\n  style a opacity:0.5\n").ir;
        let layout = fm_layout::layout_diagram(&declared);
        let plan = GpuRenderPlan::from_layout(&declared, &layout, 1.0);
        assert!(!plan.node_instances.is_empty(), "no instances, so this proves nothing");
        assert!(
            plan.node_instances
                .iter()
                .any(|i| (i.fill[3] - 0.5).abs() < 0.01 && (i.stroke[3] - 0.5).abs() < 0.01),
            "the declared opacity reached neither alpha: {:?}",
            plan.node_instances.iter().map(|i| (i.fill[3], i.stroke[3])).collect::<Vec<_>>()
        );

        // CONTROL: an undeclared node stays fully opaque. Without this, multiplying by a stray 0.5
        // everywhere would satisfy the assertion above while fading every diagram -- and a uniform
        // fade is exactly the kind of wrong that looks like a theme choice rather than a defect.
        let plain = fm_parser::parse("flowchart TD\n  a[A]\n").ir;
        let plain_layout = fm_layout::layout_diagram(&plain);
        let plain_plan = GpuRenderPlan::from_layout(&plain, &plain_layout, 1.0);
        assert!(
            plain_plan
                .node_instances
                .iter()
                .all(|i| (i.fill[3] - 1.0).abs() < f32::EPSILON),
            "an undeclared node was faded: {:?}",
            plain_plan.node_instances.iter().map(|i| i.fill[3]).collect::<Vec<_>>()
        );
    }

    /// A declared font size sizes the GPU quads AND the glyph atlas (bd-eudpo).
    ///
    /// Resolved from the NODE, not from the layout text primitive: every `font_size` in
    /// `build_render_scene` is a hardcoded 14.0 or 12.0, so routing through the scene -- which is
    /// what this bead originally recommended -- would have delivered nothing at all.
    #[test]
    fn a_declared_font_size_sizes_the_quads_and_the_atlas() {
        let big = fm_parser::parse("flowchart TD\n  a[Alpha]\n  style a font-size:32px\n").ir;
        let big_plan = GpuRenderPlan::from_layout(&big, &fm_layout::layout_diagram(&big), 1.0);
        let plain = fm_parser::parse("flowchart TD\n  a[Alpha]\n").ir;
        let plain_plan =
            GpuRenderPlan::from_layout(&plain, &fm_layout::layout_diagram(&plain), 1.0);

        let big_h = big_plan.text_quads.first().map(|q| q.half_extent[1]).unwrap_or(0.0);
        let plain_h = plain_plan.text_quads.first().map(|q| q.half_extent[1]).unwrap_or(0.0);
        assert!(big_h > 0.0 && plain_h > 0.0, "no glyphs, so this proves nothing");
        assert!(
            big_h > plain_h * 1.5,
            "a 32px declaration did not enlarge the quad: {big_h} vs default {plain_h}"
        );

        // ⚠️ THE GEOMETRY ALONE IS NOT THE FIX. Enlarged quads sampling a 32px cell would render
        // blurry -- right shape, soft raster, and it survives review because nothing is misplaced.
        // The atlas has to grow with the largest label.
        assert!(
            big_plan.glyph_atlas.cell_px > plain_plan.glyph_atlas.cell_px,
            "the atlas cell did not grow with the label: {} vs {}",
            big_plan.glyph_atlas.cell_px,
            plain_plan.glyph_atlas.cell_px
        );

        // CONTROL: an undeclared diagram keeps the default cell, so the atlas is not enlarged for
        // every diagram on the strength of one styled node.
        assert_eq!(plain_plan.glyph_atlas.cell_px, super::DEFAULT_GLYPH_CELL_PX);
    }

    /// Every per-node style resolver the raster path has must be CONSUMED by the GPU plan.
    ///
    /// This gate exists because the same defect happened five times, and each one was found by
    /// hand, months apart, by noticing a neighbouring field:
    ///
    ///   stroke width  a3157251   label colour  58564713   opacity  a2b268c5
    ///   font size     54d5f637   border dash   bd-l3nsf (open)
    ///
    /// Every one had the same shape: a resolver existed for the Canvas2D pass, `node_index` was
    /// already in scope where the GPU plan is built, and nothing called it. The field was often
    /// present and populated with a CONSTANT, so struct checks, the ABI guard and the compiler all
    /// passed. Only a test that asks what value ARRIVES catches it — and only per channel, which is
    /// why five separate tests exist and did not prevent the sixth.
    ///
    /// So this asks the structural question instead: does the GPU plan MENTION every resolver? It
    /// is a weaker claim than "uses it correctly" — mention is not use — but it is the claim that
    /// scales, and it fails the moment someone adds a sixth channel to the raster path alone.
    ///
    /// KNOWN GAPS ARE NAMED, NOT SILENT. `resolve_node_stroke_dasharray` is exempt with its bead
    /// id, because a dashed SDF border needs perimeter arc length the shader does not have. An
    /// Sequence activation bars reach the plan as rects (bd-adabx).
    ///
    /// First of the sixteen unplanned raster draw sources to be closed. A bar is a filled, stroked
    /// rect on a participant's lifeline, so it needs no new instance type and no new shader -- the
    /// same reuse the cluster containers made.
    ///
    /// The degenerate case is pinned alongside the real one because `draw_activation_bars` skips a
    /// zero-area bar, and a plan that emitted it would hand a consumer a rect that rasterises to
    /// nothing while still counting as an instance.
    #[test]
    fn activation_bars_reach_the_gpu_plan_as_rects() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let mut layout = test_layout();
        layout.extensions.activation_bars = vec![
            fm_layout::LayoutActivationBar {
                participant_index: 1,
                depth: 0,
                bounds: LayoutRect { x: 40.0, y: 10.0, width: 8.0, height: 60.0 },
            },
            // Zero height: drawn by nobody, so planned by nobody.
            fm_layout::LayoutActivationBar {
                participant_index: 1,
                depth: 1,
                bounds: LayoutRect { x: 40.0, y: 90.0, width: 8.0, height: 0.0 },
            },
        ];

        let plan = super::GpuRenderPlan::from_layout(&ir, &layout, 1.25);

        assert_eq!(
            plan.activation_instances.len(),
            1,
            "the zero-height bar must be skipped the way the raster pass skips it"
        );
        let bar = plan.activation_instances[0];
        assert_eq!(bar.center, [44.0, 40.0], "centre of x40..48, y10..70");
        assert_eq!(bar.half_extent, [4.0, 30.0]);
        assert_eq!(bar.shape, super::GpuNodeShape::Rect as u32);
        assert_eq!(
            bar.node_index, 1,
            "node_index on a bar is the PARTICIPANT node it sits on"
        );
        assert_eq!(bar.fill, super::DEFAULT_NODE_FILL_RGBA);
        assert_eq!(bar.stroke, super::DEFAULT_NODE_STROKE_RGBA);
        assert_eq!(bar.stroke_width, super::DEFAULT_NODE_STROKE_WIDTH);

        // A diagram with no bars must not gain any: the plan is shared by every diagram type.
        let empty = super::GpuRenderPlan::from_layout(&ir, &test_layout(), 1.25);
        assert!(empty.activation_instances.is_empty());
    }

    /// A sequence foot row reaches the plan as a box AND a name (bd-adabx).
    ///
    /// The label is the half that can vanish silently, so it is asserted by CONTENT: the run must
    /// carry as many quads as the mirrored participant's label has glyphs, which fails both if the
    /// text never reached the atlas and if the wrong source was resolved. A foot row naming the
    /// wrong participant, or naming nothing, is a different diagram.
    #[test]
    fn a_sequence_foot_row_reaches_the_plan_as_a_box_and_a_name() {
        // THE PARTICIPANT MUST NOT BE ONE OF THE LAID-OUT NODES, or this test cannot see the
        // atlas half at all. The foot row resolves its name from the SAME ir node the node-label
        // pass draws, so a mirrored participant that is also a drawn node has its glyphs in the
        // atlas via that pass -- and the first version of this test passed with the foot-row atlas
        // feed DELETED. Node index 2 is referenced only by the header, so "Zephyr" reaches the
        // atlas only if this source feeds it.
        let mut ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        for id in ["a", "b"] {
            ir.nodes.push(fm_core::IrNode {
                id: id.into(),
                ..fm_core::IrNode::default()
            });
        }
        ir.nodes.push(fm_core::IrNode {
            id: "zeph".into(),
            ..fm_core::IrNode::default()
        });
        ir.labels.push(fm_core::IrLabel {
            text: "Zephyr".to_string(),
            span: Default::default(),
        });
        ir.nodes[2].label = Some(fm_core::IrLabelId(0));

        let mut layout = test_layout();
        layout.extensions.sequence_mirror_headers = vec![LayoutNodeBox {
            node_index: 2,
            node_id: String::from("zeph"),
            rank: 0,
            order: 0,
            span: Span::default(),
            bounds: LayoutRect { x: 20.0, y: 200.0, width: 60.0, height: 30.0 },
        }];

        let plan = super::GpuRenderPlan::from_layout(&ir, &layout, 1.25);

        assert_eq!(plan.mirror_header_instances.len(), 1);
        let box_instance = plan.mirror_header_instances[0];
        assert_eq!(box_instance.center, [50.0, 215.0]);
        assert_eq!(box_instance.half_extent, [30.0, 15.0]);
        assert_eq!(box_instance.node_index, 2);

        let run = plan
            .text_runs
            .iter()
            .find(|run| run.source == super::GpuTextSource::MirrorHeader)
            .expect("the foot row must carry the participant's name");
        assert_eq!(
            run.quad_count, 6,
            "\"Zephyr\" is six glyphs -- a smaller count means the atlas never saw them"
        );
        assert_eq!(run.node_index, 2, "the run indexes the participant node");

        // Centred in the box, like the raster pass centres it.
        let quads = &plan.text_quads[run.first_quad as usize..][..run.quad_count as usize];
        let mean_x = quads.iter().map(|q| q.center[0]).sum::<f32>() / 6.0;
        assert!((mean_x - 50.0).abs() < 0.01, "name not centred: {mean_x}");
        assert!((quads[0].center[1] - 215.0).abs() < 0.01);
    }

    /// Sequence notes reach the plan as a box and a body (bd-adabx).
    ///
    /// Two details are pinned because both are easy to "improve" into a disagreement with the
    /// canvas: the border is 1.0 wide, NOT the node stroke width, and a zero-area note is still
    /// planned because `draw_sequence_notes` still draws one. The empty-TEXT note is the only case
    /// that pass skips, so it is the only case skipped here.
    ///
    /// The body text uses glyphs no other label in the fixture carries, so the atlas feed for this
    /// source is the only path to them -- the foot-row test taught that a shared path makes this
    /// assertion vacuous.
    #[test]
    fn sequence_notes_reach_the_plan_as_a_box_and_a_body() {
        let ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        let mut layout = test_layout();
        layout.extensions.sequence_notes = vec![
            fm_layout::LayoutSequenceNote {
                position: fm_core::NotePosition::Over,
                text: "Wqkj".to_string(),
                bounds: LayoutRect { x: 100.0, y: 50.0, width: 80.0, height: 40.0 },
            },
            fm_layout::LayoutSequenceNote {
                position: fm_core::NotePosition::Over,
                text: String::new(),
                bounds: LayoutRect { x: 100.0, y: 120.0, width: 0.0, height: 0.0 },
            },
        ];

        let plan = super::GpuRenderPlan::from_layout(&ir, &layout, 1.25);

        assert_eq!(
            plan.sequence_note_instances.len(),
            2,
            "the zero-area note is still drawn by the raster pass, so it is still planned"
        );
        let note = plan.sequence_note_instances[0];
        assert_eq!(note.center, [140.0, 70.0]);
        assert_eq!(note.half_extent, [40.0, 20.0]);
        assert_eq!(
            note.stroke_width,
            super::SEQUENCE_NOTE_STROKE_WIDTH,
            "a note border is 1.0, not the node stroke width"
        );
        assert!(
            (note.stroke_width - super::DEFAULT_NODE_STROKE_WIDTH).abs() > f32::EPSILON,
            "this assertion is pointless if the two widths are ever made equal"
        );

        let runs: Vec<_> = plan
            .text_runs
            .iter()
            .filter(|run| run.source == super::GpuTextSource::SequenceNote)
            .collect();
        assert_eq!(runs.len(), 1, "the empty note contributes no run");
        assert_eq!(runs[0].quad_count, 4, "\"Wqkj\" is four glyphs");
        assert_eq!(runs[0].node_index, 0, "node_index indexes the notes, not ir.nodes");

        let quads = &plan.text_quads[runs[0].first_quad as usize..][..4];
        let mean_x = quads.iter().map(|q| q.center[0]).sum::<f32>() / 4.0;
        assert!((mean_x - 140.0).abs() < 0.01, "body not centred: {mean_x}");
        assert!((quads[0].center[1] - 70.0).abs() < 0.01);
    }

    /// A destroyed participant's cross reaches the plan as two segments (bd-adabx).
    ///
    /// First line source that is NOT an edge. The `edge_index` sentinel is asserted explicitly:
    /// a consumer resolving it against `ir.edges` must fail loudly rather than read an unrelated
    /// edge's style, which is the wrong-key defect this file has hit before.
    ///
    /// The two diagonals are checked as a SET, not by position, because their order is an internal
    /// detail -- but both must be present, since one diagonal is a slash, not a cross.
    #[test]
    fn a_destroy_marker_reaches_the_plan_as_two_crossing_segments() {
        let ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        let mut layout = test_layout();
        layout.extensions.sequence_lifecycle_markers = vec![
            fm_layout::LayoutSequenceLifecycleMarker {
                participant_index: 0,
                kind: fm_layout::LayoutSequenceLifecycleMarkerKind::Destroy,
                center: super::LayoutPoint { x: 100.0, y: 200.0 },
                size: 10.0,
            },
            // Zero size: the raster pass bails on `half <= 0.0`, so nothing is planned either.
            fm_layout::LayoutSequenceLifecycleMarker {
                participant_index: 1,
                kind: fm_layout::LayoutSequenceLifecycleMarkerKind::Destroy,
                center: super::LayoutPoint { x: 300.0, y: 200.0 },
                size: 0.0,
            },
        ];

        let plan = super::GpuRenderPlan::from_layout(&ir, &layout, 1.25);

        assert_eq!(
            plan.lifecycle_marker_segments.len(),
            2,
            "one cross is two segments, and the zero-size marker adds none"
        );
        let ends: Vec<([f32; 2], [f32; 2])> = plan
            .lifecycle_marker_segments
            .iter()
            .map(|segment| (segment.from, segment.to))
            .collect();
        assert!(
            ends.contains(&([95.0, 195.0], [105.0, 205.0])),
            "missing the top-left to bottom-right diagonal: {ends:?}"
        );
        assert!(
            ends.contains(&([105.0, 195.0], [95.0, 205.0])),
            "missing the top-right to bottom-left diagonal: {ends:?}"
        );

        for segment in &plan.lifecycle_marker_segments {
            assert_eq!(
                segment.edge_index,
                super::NO_EDGE_INDEX,
                "a cross has no edge behind it, and must not claim one"
            );
            assert_eq!(segment.dash, [0.0, 0.0], "the cross is solid");
            assert_eq!(segment.width, super::LIFECYCLE_MARKER_STROKE_WIDTH);
            assert_eq!(segment.color, super::DEFAULT_EDGE_STROKE_RGBA);
        }

        // The real edges must NOT have been given the sentinel, or the assertion above is trivially
        // satisfiable by breaking every edge in the plan.
        assert!(
            plan.edge_segments
                .iter()
                .all(|segment| segment.edge_index != super::NO_EDGE_INDEX),
            "a real edge must still resolve to its own index"
        );
    }

    /// The plan's FIELD ORDER is the raster pass's DRAW ORDER (bd-adabx).
    ///
    /// The plan documents field order as submit order -- "a consumer that submits these buffers in
    /// field order gets the right picture". That makes the declaration order a contract, and it was
    /// wrong: `node_instances` sat before `edge_segments` while `render` calls `draw_edges` and THEN
    /// `draw_nodes`, so the canvas covers an under-routed edge with the node's opaque fill and a
    /// consumer following the plan would have drawn that edge on top.
    ///
    /// Both orders are READ FROM SOURCE. A comment asserting "these match" is exactly what already
    /// went stale here, so this compares the field positions in this file against the call
    /// positions in renderer.rs, and either side moving alone fails.
    #[test]
    fn the_plan_field_order_matches_the_raster_draw_order() {
        const RENDERER_SRC: &str = include_str!("renderer.rs");
        const GPU_FULL_SRC: &str = include_str!("gpu_plan.rs");
        // Production half only: the struct fields are declared there, and this test's own prose
        // names every field and every draw call, which would satisfy a naive search of the whole
        // file. That vacuity bit the resolver gate once already.
        let gpu_src: &str = GPU_FULL_SRC
            .split_once("#[cfg(test)]")
            .map_or(GPU_FULL_SRC, |(production, _tests)| production);
        let struct_src = gpu_src
            .split_once("pub struct GpuRenderPlan {")
            .expect("the plan struct must be declared in the production half")
            .1;

        // (plan field, the raster call that produces it), in the order both must agree on.
        const PAIRS: &[(&str, &str)] = &[
            ("pub cluster_instances:", "self.draw_clusters("),
            ("pub mirror_header_instances:", "self.draw_sequence_mirror_headers("),
            ("pub activation_instances:", "self.draw_activation_bars("),
            ("pub lifecycle_marker_segments:", "self.draw_sequence_lifecycle_markers("),
            ("pub sequence_note_instances:", "self.draw_sequence_notes("),
            ("pub edge_segments:", "self.draw_edges("),
            ("pub node_instances:", "self.draw_nodes("),
        ];

        let mut field_positions = Vec::new();
        let mut call_positions = Vec::new();
        for (field, call) in PAIRS {
            field_positions.push((
                *field,
                struct_src
                    .find(field)
                    .unwrap_or_else(|| panic!("plan field {field} not found -- renamed?")),
            ));
            call_positions.push((
                *call,
                RENDERER_SRC
                    .find(call)
                    .unwrap_or_else(|| panic!("raster call {call} not found -- renamed?")),
            ));
        }

        let ordered = |positions: &[(&str, usize)]| -> Vec<String> {
            let mut sorted = positions.to_vec();
            sorted.sort_by_key(|(_, at)| *at);
            sorted.iter().map(|(name, _)| (*name).to_string()).collect()
        };
        let fields = ordered(&field_positions);
        let calls = ordered(&call_positions);

        let field_rank: Vec<usize> = fields
            .iter()
            .map(|name| PAIRS.iter().position(|(f, _)| f == name).unwrap())
            .collect();
        let call_rank: Vec<usize> = calls
            .iter()
            .map(|name| PAIRS.iter().position(|(_, c)| c == name).unwrap())
            .collect();
        assert_eq!(
            field_rank, call_rank,
            "plan field order {fields:?} does not match the raster draw order {calls:?} -- a \
             consumer submitting buffers in field order would paint them in a different order \
             than the Canvas2D pass does"
        );
    }

    /// Every raster draw source is CLASSIFIED against the GPU plan (bd-adabx).
    ///
    /// The plan mirrors three of renderer.rs's nineteen `draw_*` entry points. The other sixteen
    /// have no GPU counterpart, so a sequence, gantt, pie, quadrant, packet or state diagram plans
    /// its nodes and edges and none of the furniture that makes it that diagram type. Nothing
    /// failed over this, because a plan that omits a draw source is indistinguishable from a plan
    /// for a diagram that never had one.
    ///
    /// FAILS BOTH WAYS, which is the whole point. A new `draw_*` that nobody classified fails here
    /// until someone decides whether the plan covers it; and a classification naming a `draw_*` that
    /// no longer exists ALSO fails, so the list cannot rot into the permanent hole an allowlist
    /// becomes when its entries stop matching reality.
    #[test]
    fn every_raster_draw_source_is_accounted_for_in_the_gpu_plan() {
        const RENDERER_SRC: &str = include_str!("renderer.rs");

        // Mirrored by the plan today.
        const PLANNED: &[&str] = &[
            "draw_activation_bars",
            "draw_sequence_mirror_headers",
            "draw_sequence_lifecycle_markers",
            "draw_sequence_notes",
            "draw_clusters",
            "draw_edges",
            "draw_nodes",
        ];

        // (draw source, why the plan does not carry it) -- every entry cites bd-adabx, which holds
        // the coverage map, so a reader lands on the decision rather than on this list.
        const NOT_PLANNED: &[(&str, &str)] = &[
            ("draw_generic_diagram_title", "bd-adabx: diagram title text"),
            ("draw_path_markers", "bd-adabx: RenderScene path pipeline, not the layout pipeline"),
            ("draw_marker", "bd-adabx: RenderScene path pipeline, not the layout pipeline"),
            ("draw_quadrant_axis_labels", "bd-adabx: quadrant furniture"),
            ("draw_bands", "bd-adabx: journey/kanban band furniture"),
            ("draw_packet_field_continuations", "bd-adabx: packet furniture"),
            ("draw_axis_ticks", "bd-adabx: axis furniture"),
            ("draw_gantt_today_marker", "bd-adabx: gantt furniture"),
            ("draw_cluster_dividers", "bd-adabx: cluster divider rules"),
            ("draw_state_notes", "bd-adabx: state furniture"),
            (
                "draw_sequence_fragments",
                "bd-l3nsf, NOT merely unplanned: a fragment box has a DASHED border \
                 (set_line_dash([4,4])) and the SDF carries no perimeter arc length, so planning \
                 it as a plain rect instance would draw a solid border where the canvas draws a \
                 dashed one -- a wrong picture, not a missing one",
            ),
            ("draw_pie_wedges", "bd-adabx: pie geometry"),
        ];

        let mut found = Vec::new();
        let mut rest = RENDERER_SRC;
        while let Some(at) = rest.find("    fn draw_") {
            rest = &rest[at + "    fn ".len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            found.push(name);
        }
        found.sort();
        found.dedup();

        // NON-VACUITY: a rename of the `fn draw_` idiom would leave every assertion below iterating
        // over nothing and passing forever.
        assert!(
            found.len() >= 15,
            "found only {} draw sources, so this scan is not reading renderer.rs: {found:?}",
            found.len()
        );

        let unclassified: Vec<&String> = found
            .iter()
            .filter(|name| !PLANNED.contains(&name.as_str()))
            .filter(|name| !NOT_PLANNED.iter().any(|(known, _)| known == &name.as_str()))
            .collect();
        assert!(
            unclassified.is_empty(),
            "new raster draw source(s) with no GPU-plan decision: {unclassified:?} -- add to \
             PLANNED if the plan carries it, or to NOT_PLANNED citing a bead"
        );

        let stale: Vec<&str> = PLANNED
            .iter()
            .copied()
            .chain(NOT_PLANNED.iter().map(|(name, _)| *name))
            .filter(|name| !found.iter().any(|seen| seen == name))
            .collect();
        assert!(
            stale.is_empty(),
            "classified draw source(s) that no longer exist in renderer.rs: {stale:?} -- a list \
             entry naming nothing is a permanent hole, not a harmless leftover"
        );
    }

    /// exemption without a bead would turn this gate into a place to hide gaps.
    #[test]
    fn every_per_node_resolver_is_consumed_by_the_gpu_plan() {
        const RENDERER_SRC: &str = include_str!("renderer.rs");
        // ⚠️ THE PRODUCTION HALF ONLY, AND THIS TEST FAILED TO FAIL WITHOUT IT. `include_str!`
        // pulls in THIS FILE, tests included, so `GPU_SRC.contains(name)` was satisfied by the
        // resolver names appearing in the EXEMPT list and in this very doc comment. The gate passed
        // while consuming nothing — it was reading its own text as evidence. Caught by breaking the
        // exemption on purpose and watching it stay green.
        const GPU_FULL_SRC: &str = include_str!("gpu_plan.rs");
        let gpu_src: &str = GPU_FULL_SRC
            .split_once("#[cfg(test)]")
            .map_or(GPU_FULL_SRC, |(production, _tests)| production);
        // (resolver, why it is exempt) — an entry here must cite a bead.
        const EXEMPT: &[(&str, &str)] = &[
            (
                "resolve_cluster_dash_array",
                "bd-l3nsf: a dashed border needs perimeter arc length the SDF does not carry, and a \
                 cluster border is the same rect the node case is",
            ),
            (
            "resolve_node_stroke_dasharray",
            "bd-l3nsf: a dashed SDF border needs perimeter arc length, which shape_distance does \
             not provide; the field is trivial and the shader is not",
        )];

        let mut resolvers = Vec::new();
        let mut rest = RENDERER_SRC;
        while let Some(at) = rest.find("pub(crate) fn resolve_") {
            rest = &rest[at + "pub(crate) fn ".len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.contains("node") || name.contains("cluster") {
                resolvers.push(name);
            }
        }
        resolvers.sort();
        resolvers.dedup();

        // NON-VACUITY: a rename or a visibility change that made this scan find nothing would
        // otherwise leave the loop below asserting over an empty set and passing forever.
        assert!(
            resolvers.len() >= 9,
            "found only {} per-node resolvers, so this scan is not reading the source it thinks \
             it is: {resolvers:?}",
            resolvers.len()
        );

        // AN EXEMPTION THAT IS NO LONGER TRUE IS A HOLE, NOT A LEFTOVER. resolve_cluster_text_color
        // sat here claiming "cluster labels are not planned yet" for a while after cluster titles
        // landed and started consuming it -- harmless on that day, but the same entry would have
        // silently excused the resolver being DROPPED again later. Every exemption must still name
        // a resolver the plan genuinely does not consume.
        let obsolete: Vec<&str> = EXEMPT
            .iter()
            .map(|(name, _)| *name)
            .filter(|name| gpu_src.contains(name))
            .collect();
        assert!(
            obsolete.is_empty(),
            "exemption(s) for resolver(s) the plan now DOES consume: {obsolete:?} -- delete the \
             entry so the resolver is covered by the check again"
        );

        let missing: Vec<&String> = resolvers
            .iter()
            .filter(|name| !gpu_src.contains(name.as_str()))
            .filter(|name| !EXEMPT.iter().any(|(exempt, _)| *exempt == name.as_str()))
            .collect();
        assert!(
            missing.is_empty(),
            "the raster path resolves these per node and the GPU plan never mentions them: \
             {missing:?}\nEither consume them, or add an EXEMPT entry citing a bead that says why \
             the GPU cannot."
        );
    }

    /// A subgraph now reaches the GPU plan as a container instance (bd-dh6cy).
    ///
    /// INVERTED from the stale-detector it started as, which is why it still exists. It was landed
    /// asserting the opposite — that `GpuRenderPlan` declared no cluster field — with instructions
    /// to flip rather than delete it, and it fired on exactly the commit that added the field. A
    /// detector deleted on the day of the fix proves nothing about the fix; this one proved the
    /// change reached the behaviour, and now guards the behaviour it proved.
    #[test]
    fn a_subgraph_reaches_the_gpu_plan_as_a_container() {
        let ir = fm_parser::parse(
            "flowchart TD\n  subgraph one[Group One]\n    a[A]\n    b[B]\n  end\n  a --- b\n",
        )
        .ir;
        let layout = fm_layout::layout_diagram(&ir);

        // NON-VACUITY, and the assertion the old hand-built `clusters: Vec::new()` fixture could
        // never make: if the parser or layout stopped producing clusters, everything below would
        // pass against two empty lists.
        assert!(
            !layout.clusters.is_empty(),
            "the fixture produced no clusters, so it cannot detect whether the plan drops them"
        );

        let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.0);
        assert_eq!(
            plan.cluster_instances.len(),
            layout.clusters.len(),
            "the plan does not carry one container instance per laid-out cluster"
        );

        // The container must actually cover its contents, not sit at the origin with zero size --
        // a plan that emitted the right COUNT of empty rects would satisfy a count check alone.
        let container = &plan.cluster_instances[0];
        let cluster = &layout.clusters[0];
        assert!(
            (container.half_extent[0] - cluster.bounds.width * 0.5).abs() < 0.001
                && (container.half_extent[1] - cluster.bounds.height * 0.5).abs() < 0.001,
            "container half-extent {:?} does not match the cluster bounds {}x{}",
            container.half_extent,
            cluster.bounds.width,
            cluster.bounds.height
        );
        assert!(
            container.half_extent[0] > 0.0 && container.half_extent[1] > 0.0,
            "the container has no area"
        );

        // ⚠️ DRAW ORDER IS THE DECLARATION ORDER, and it is load-bearing: a container submitted
        // after its contents paints over them. Asserted on the source because no runtime value
        // expresses it -- both fields are plain Vecs and a consumer reads them in field order.
        const GPU_FULL_SRC: &str = include_str!("gpu_plan.rs");
        let declaration = GPU_FULL_SRC
            .split_once("pub struct GpuRenderPlan {")
            .and_then(|(_, rest)| rest.split_once('}'))
            .map(|(fields, _)| fields)
            .expect("GpuRenderPlan declaration not found; this scan is not reading what it thinks");
        let cluster_at = declaration
            .find("pub cluster_instances")
            .expect("cluster_instances field not found in the declaration");
        let nodes_at = declaration
            .find("pub node_instances")
            .expect("node_instances field not found in the declaration");
        assert!(
            cluster_at < nodes_at,
            "cluster_instances must be declared BEFORE node_instances: the field order is the \
             submit order, and a subgraph drawn after its nodes covers them"
        );
    }

    /// A subgraph TITLE reaches the GPU plan, at the corner rather than the centre (bd-dh6cy).
    ///
    /// ⚠️ THE FAILURE MODE HERE IS SILENT, which is why this asserts a quad COUNT first. The glyph
    /// loop does `let Some(cell) = glyph_atlas.cell(glyph) else { continue }`, so if the atlas were
    /// still built from node labels alone, every title glyph would be skipped and the plan would
    /// carry a tidy zero-quad run that looks like a title nobody typed.
    #[test]
    fn a_subgraph_title_reaches_the_gpu_plan_at_its_corner() {
        let ir = fm_parser::parse(
            "flowchart TD\n  subgraph one[Group One]\n    a[A]\n    b[B]\n  end\n  a --- b\n",
        )
        .ir;
        let layout = fm_layout::layout_diagram(&ir);
        let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.0);

        let title_runs: Vec<&super::GpuTextRun> = plan
            .text_runs
            .iter()
            .filter(|run| run.source == super::GpuTextSource::Cluster)
            .collect();
        assert_eq!(title_runs.len(), 1, "expected exactly one cluster title run");
        assert!(
            title_runs[0].quad_count >= 5,
            "the title emitted {} quads for 'Group One'; a glyph missing from the atlas is skipped \
             silently, so a low count here means the title is not renderable",
            title_runs[0].quad_count
        );

        // CORNER, NOT CENTRE. Centring a subgraph title the way node labels are centred puts it in
        // the middle of its own box, on top of the nodes it contains — right text, wrong picture,
        // and a count-only assertion would never notice.
        let cluster = &layout.clusters[0];
        let first = &plan.text_quads[title_runs[0].first_quad as usize];
        let cluster_centre_x = cluster.bounds.x + (cluster.bounds.width * 0.5);
        assert!(
            first.center[0] < cluster_centre_x,
            "the title starts at {} which is not left of the cluster centre {cluster_centre_x}",
            first.center[0]
        );
        assert!(
            first.center[1] < cluster.bounds.y + (cluster.bounds.height * 0.5),
            "the title sits below the cluster's vertical midpoint; it belongs at the top"
        );

        // The run's index means a CLUSTER here. Resolving it against ir.nodes would be the
        // wrong-lookup-key defect, which is what `source` exists to prevent.
        assert_eq!(title_runs[0].node_index, cluster.cluster_index as u32);
    }

    /// The Canvas2D pass and the GPU plan must AGREE on a declared value, not merely each honour it.
    ///
    /// Five channels reached the GPU one at a time (stroke width a3157251, label colour 58564713,
    /// opacity a2b268c5, font size 54d5f637, cluster styling f5769ac8), and each has its own test
    /// asserting that surface carries the declaration. None of them compares the two surfaces, so
    /// both could honour the same `style` directive and still disagree — different default, different
    /// parse, different clamp — and every existing test would pass. That is exactly how bd-lvj3
    /// started: two renderers, each locally correct by its own tests, disagreeing about one document.
    ///
    /// The structural gate above asks whether the GPU plan MENTIONS each resolver. This asks whether
    /// the two surfaces produce the same ANSWER, which is the stronger claim and the one a user sees.
    #[test]
    fn the_canvas_and_the_gpu_plan_agree_on_a_declared_fill() {
        let ir =
            fm_parser::parse("flowchart TD\n  a[Alpha]\n  style a fill:#ff0000\n").ir;
        let layout = fm_layout::layout_diagram(&ir);

        // GPU side: the instance's fill, as linear RGBA.
        let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.0);
        let gpu_fill = plan
            .node_instances
            .first()
            .map(|instance| instance.fill)
            .expect("no node instance, so this comparison has nothing to compare");

        // Raster side: the fill styles the canvas actually set, parsed through the SAME helper the
        // plan uses. Comparing the STRING to the floats would compare spellings rather than colours
        // and would fail on `#ff0000` against `rgb(255,0,0)` while both are correct.
        let mut context = crate::MockCanvas2dContext::new(1200.0, 900.0);
        crate::render_to_canvas(&ir, &mut context, &crate::CanvasRenderConfig::default());
        let ops = format!("{:?}", context.operations());
        let mut canvas_fills = Vec::new();
        let mut rest = ops.as_str();
        while let Some(at) = rest.find("SetFillStyle(\"") {
            rest = &rest[at + "SetFillStyle(\"".len()..];
            if let Some(end) = rest.find('"') {
                if let Some(rgba) = super::parse_paint_rgba(&rest[..end]) {
                    canvas_fills.push(rgba);
                }
                rest = &rest[end..];
            }
        }

        assert!(
            !canvas_fills.is_empty(),
            "the canvas set no parseable fill, so this proves nothing about agreement"
        );
        assert!(
            canvas_fills.iter().any(|fill| {
                fill.iter()
                    .zip(gpu_fill.iter())
                    .all(|(a, b)| (a - b).abs() < 0.001)
            }),
            "no canvas fill matches the GPU instance fill {gpu_fill:?}; the two surfaces honour the \
             same declaration differently"
        );

        // CONTROL: the agreement must be on the DECLARED colour, not on a shared default. Without
        // this, two surfaces that both ignored the directive would agree perfectly on white.
        assert!(
            gpu_fill[0] > 0.9 && gpu_fill[1] < 0.1 && gpu_fill[2] < 0.1,
            "the agreed colour is not the declared red: {gpu_fill:?}"
        );
    }
}
