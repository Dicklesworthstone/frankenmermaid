//! Real glyph rasterisation into the atlas the text pipeline samples (bd-2u0.2).
//!
//! [`crate::gpu_plan::GlyphAtlasPlan`] describes the atlas — the cell grid and every glyph's UV
//! rectangle — but carries no pixel data, because layout planning has no business owning a font.
//! This module fills that texture in, turning the synthetic all-cells-opaque bitmap the device tests
//! started with into actual letterforms.
//!
//! **Device-free on purpose.** Nothing here touches wgpu: it produces a plain R8 coverage bitmap the
//! caller uploads, so the mapping from a glyph to its cell can be tested without an adapter, exactly
//! as the vertex layouts are. It rides the `webgpu` feature only because that is the only consumer
//! today.
//!
//! **This module ships no font.** The caller supplies the bytes. Embedding a typeface would put
//! megabytes of binary in this repo and would decide, for every downstream user, which face their
//! diagrams are drawn in — a choice the SVG backend leaves to CSS and this one should leave to the
//! caller. The consequence is that a caller with no font gets a blank atlas, which is why
//! [`rasterize_atlas`] reports what it could not place instead of silently producing empty cells.

use crate::gpu_plan::GlyphAtlasPlan;

/// A parsed font, ready to rasterise glyphs at a given pixel size.
pub struct GlyphFont {
    font: fontdue::Font,
}

impl GlyphFont {
    /// Parse font bytes (TTF/OTF).
    ///
    /// # Errors
    /// Returns the parser's message when the bytes are not a font this can read.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())
            .map(|font| Self { font })
            .map_err(str::to_string)
    }

    /// Coverage bitmap and its dimensions for one glyph at `px`.
    ///
    /// Returns `None` for a glyph with no raster — a space has real metrics and an empty bitmap, and
    /// treating that as a failure would report every space in every label as a missing glyph.
    #[must_use]
    pub fn rasterize(&self, glyph: char, px: f32) -> Option<RasterizedGlyph> {
        let (metrics, coverage) = self.font.rasterize(glyph, px);
        if metrics.width == 0 || metrics.height == 0 || coverage.is_empty() {
            return None;
        }
        Some(RasterizedGlyph {
            width: metrics.width,
            height: metrics.height,
            coverage,
        })
    }
}

/// One glyph's 8-bit coverage bitmap.
pub struct RasterizedGlyph {
    pub width: usize,
    pub height: usize,
    /// Row-major coverage, `width * height` bytes, 0 = transparent, 255 = solid.
    pub coverage: Vec<u8>,
}

/// What [`rasterize_atlas`] managed to do, so a caller is never left guessing at a blank texture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtlasCoverage {
    /// The R8 bitmap, `texture_px[0] * texture_px[1]` bytes.
    pub bitmap: Vec<u8>,
    /// Cells that received a glyph raster.
    pub rendered: usize,
    /// Glyphs the font had no outline for, in plan order. A caller seeing entries here is rendering
    /// text the supplied font cannot draw, which is a font choice problem rather than a bug.
    pub missing: Vec<char>,
    /// Glyphs whose raster was larger than a cell and was clipped to fit.
    pub clipped: Vec<char>,
}

/// Rasterise every glyph the plan placed into its own cell.
///
/// Each glyph is drawn CENTRED in its cell rather than at the cell origin. The plan allocates a
/// square cell per glyph and the UV rectangle addresses that whole square, so a glyph pinned to the
/// corner would appear offset by however much narrower it is than the cell — an `i` would sit hard
/// left of where an `M` sits. Centring makes every glyph's ink land where its quad expects it.
///
/// Baseline alignment is deliberately NOT applied here: the plan's cells are uniform squares with no
/// per-glyph baseline offset to honour, so applying `ymin` would push descenders out of their own
/// cell and into a neighbour's. Proper baseline-relative placement needs per-glyph offsets in the
/// plan, which is a change to `gpu_plan` rather than to this module, and is noted on the bead.
///
/// A glyph larger than a cell is CLIPPED rather than scaled, and reported in
/// [`AtlasCoverage::clipped`]: silently scaling one glyph would make it a different size from every
/// other glyph on the same line, which looks like a font bug and is harder to trace than a clip.
#[must_use]
pub fn rasterize_atlas(plan: &GlyphAtlasPlan, font: &GlyphFont) -> AtlasCoverage {
    let width = plan.texture_px[0].max(1) as usize;
    let height = plan.texture_px[1].max(1) as usize;
    let mut bitmap = vec![0_u8; width * height];
    let mut rendered = 0_usize;
    let mut missing = Vec::new();
    let mut clipped = Vec::new();

    // Rasterise at the cell size so a glyph fills its cell as fully as the face allows. `cell_px` is
    // an EM box, and most glyphs are shorter than their em, so this is an upper bound rather than a
    // target.
    let px = plan.cell_px.max(1) as f32;

    for cell in &plan.cells {
        // The cell's pixel rectangle, derived from the SAME UVs the shader samples with. Deriving it
        // from `columns`/`rows` arithmetic instead would be a second source of truth, and the two
        // could disagree without anything noticing.
        let x0 = (cell.uv_min[0] * width as f32).round().max(0.0) as usize;
        let y0 = (cell.uv_min[1] * height as f32).round().max(0.0) as usize;
        let x1 = ((cell.uv_max[0] * width as f32).round() as usize).min(width);
        let y1 = ((cell.uv_max[1] * height as f32).round() as usize).min(height);
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        let cell_w = x1 - x0;
        let cell_h = y1 - y0;

        let Some(raster) = font.rasterize(cell.glyph, px) else {
            // A space rasterises to nothing and is not "missing" in any useful sense; anything else
            // with no outline is worth reporting.
            if !cell.glyph.is_whitespace() {
                missing.push(cell.glyph);
            }
            continue;
        };

        if raster.width > cell_w || raster.height > cell_h {
            clipped.push(cell.glyph);
        }

        // Centre within the cell; see the doc comment.
        let offset_x = cell_w.saturating_sub(raster.width) / 2;
        let offset_y = cell_h.saturating_sub(raster.height) / 2;

        for row in 0..raster.height.min(cell_h.saturating_sub(offset_y)) {
            let dest_y = y0 + offset_y + row;
            let dest_row = dest_y * width;
            let src_row = row * raster.width;
            for column in 0..raster.width.min(cell_w.saturating_sub(offset_x)) {
                bitmap[dest_row + x0 + offset_x + column] = raster.coverage[src_row + column];
            }
        }
        rendered += 1;
    }

    AtlasCoverage {
        bitmap,
        rendered,
        missing,
        clipped,
    }
}
