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

/// The baseline row within a cell, in cell-local pixels.
///
/// Taken from the plan and clamped into the cell. A `baseline_px` outside the cell would place every
/// glyph out of bounds and clip the whole atlas to nothing, which reads as "the rasteriser is
/// broken" rather than "the plan's baseline is wrong".
fn baseline_row(plan: &GlyphAtlasPlan, cell_h: usize) -> i64 {
    let baseline = plan.baseline_px.round().max(0.0) as i64;
    baseline.min(cell_h as i64)
}

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
            // Whole-pixel offset of the bitmap's BOTTOM edge from the baseline, negative below it.
            // This is the number that makes baseline placement possible at all: without it the only
            // available reference is the bitmap's own box, and aligning boxes is what centring does.
            ymin: metrics.ymin,
            coverage,
        })
    }

    /// The font's own ascent at `px`, in pixels below the top of an em box.
    ///
    /// # Errors
    /// Returns `None` for a font with no horizontal line metrics.
    #[must_use]
    pub fn ascent(&self, px: f32) -> Option<f32> {
        self.font.horizontal_line_metrics(px).map(|m| m.ascent)
    }
}

/// The pixel size to rasterise at so this font's ascent lands exactly on the plan's baseline.
///
/// ⚠️ NOT `cell_px`, and that mistake is worth stating because it is the obvious thing to do. A cell
/// is one em square, but a text face's `hhea` ascent plus descent usually EXCEEDS its em — DejaVu
/// Sans measures 29.70px of ascent alone at 32px, against a 32px cell — so rasterising at the cell
/// size guarantees the tall letters overflow the cell no matter where the baseline sits. There is no
/// baseline ratio that rescues it; the size itself is wrong.
///
/// Scaling so that `ascent(px) == baseline_px` makes ascenders reach the baseline exactly and leaves
/// the rest of the cell for descenders. For DejaVu Sans and a 0.8 baseline this lands at ~27.6px in
/// a 32px cell, which is also why 0.8 is a good convention rather than a lucky one: it is close to
/// the ascent share of the face's own line box.
///
/// # Errors
/// Returns `None` when the font reports no horizontal line metrics.
#[must_use]
pub fn fitted_pixel_size(plan: &GlyphAtlasPlan, font: &GlyphFont) -> Option<f32> {
    let cell = plan.cell_px.max(1) as f32;
    let ascent_at_cell = font.ascent(cell)?;
    if ascent_at_cell <= 0.0 {
        return None;
    }
    Some(cell * plan.baseline_px / ascent_at_cell)
}

/// Does this font's DESCENT fit below the plan's baseline once scaled to it?
///
/// Ascenders fit by construction — [`fitted_pixel_size`] chooses the size that makes them fit. What
/// can still overflow is the descender space: a face with unusually deep descenders needs more than
/// the `1 - GLYPH_BASELINE_RATIO` of the cell left underneath. This lets a caller ask in advance
/// instead of reading it back out of [`AtlasCoverage::clipped`].
#[must_use]
pub fn baseline_fits_font(plan: &GlyphAtlasPlan, font: &GlyphFont) -> bool {
    let Some(px) = fitted_pixel_size(plan, font) else {
        return true;
    };
    let Some(metrics) = font.font.horizontal_line_metrics(px) else {
        return true;
    };
    // `descent` is negative below the baseline.
    let below = plan.cell_px as f32 - plan.baseline_px;
    -metrics.descent <= below
}

/// One glyph's 8-bit coverage bitmap.
pub struct RasterizedGlyph {
    pub width: usize,
    pub height: usize,
    /// Whole-pixel offset of the bitmap's bottom edge from the baseline; negative for a descender.
    pub ymin: i32,
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
/// VERTICALLY, every glyph sits on the plan's shared baseline (`GlyphAtlasPlan::baseline_px`), not
/// centred in its cell. This is the difference between text and a row of stamps. Centring aligns the
/// MIDDLE of each letter, so `x` and `g` occupy the same band and a word appears to bounce; baseline
/// placement puts `g`'s bowl below the line `x` rests on, which is what the SVG and Canvas2D
/// backends do and what a reader expects. The offset comes from the font's own `ymin` — the bitmap's
/// bottom edge relative to the baseline, negative for a descender — so the placement is the face's
/// judgement rather than this module's.
///
/// HORIZONTALLY, glyphs remain centred. The plan gives every glyph an identical square cell and no
/// per-glyph advance or left side bearing, so there is no horizontal reference to align to; centring
/// is the only choice that does not bias narrow glyphs to one side. Proportional horizontal metrics
/// would change how wide a LABEL is, which is a `gpu_plan` layout change that must stay in step with
/// the SVG text measurement, and is out of scope here.
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

    // NOT `cell_px` — see `fitted_pixel_size`. Rasterising at the cell size overflows the cell for
    // any face whose ascent plus descent exceeds its em, which is most of them. Falling back to the
    // cell size when the font reports no line metrics is the best available guess, and any resulting
    // overflow is reported through `clipped` rather than silently written into a neighbour.
    let px = fitted_pixel_size(plan, font).unwrap_or_else(|| plan.cell_px.max(1) as f32);

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

        // Centre horizontally; place vertically ON THE BASELINE. See the doc comment.
        let offset_x = cell_w.saturating_sub(raster.width) / 2;

        // The glyph's ink spans `ymin ..= ymin + height` in baseline-relative coordinates with y UP.
        // The bitmap runs y DOWN from the cell's top, so the ink's first row sits at
        // `baseline - ymin - height` below that top. A descender has a negative `ymin`, which pushes
        // its rows further down — exactly the intent.
        let baseline = baseline_row(plan, cell_h);
        let top = baseline - i64::from(raster.ymin) - raster.height as i64;

        // Rows falling outside the cell are dropped rather than wrapped, and the glyph is reported.
        // Wrapping would write a descender's tail into the cell BELOW it in the texture, so one
        // label would show a sliver of an unrelated letter — a corruption far harder to read back
        // than a clipped glyph.
        let mut clipped_this = raster.width > cell_w;
        for row in 0..raster.height {
            let dest_row_in_cell = top + row as i64;
            if dest_row_in_cell < 0 || dest_row_in_cell >= cell_h as i64 {
                clipped_this = true;
                continue;
            }
            let dest_y = y0 + dest_row_in_cell as usize;
            let dest_row = dest_y * width;
            let src_row = row * raster.width;
            for column in 0..raster.width.min(cell_w.saturating_sub(offset_x)) {
                bitmap[dest_row + x0 + offset_x + column] = raster.coverage[src_row + column];
            }
        }
        if clipped_this {
            clipped.push(cell.glyph);
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
