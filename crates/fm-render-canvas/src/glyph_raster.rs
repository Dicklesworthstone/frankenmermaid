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

    /// Whether the font maps this character to a glyph other than `.notdef`.
    ///
    /// Coverage is independent of ink: spaces and deliberately empty glyphs can be supported.
    #[must_use]
    pub fn has_glyph(&self, glyph: char) -> bool {
        self.font.has_glyph(glyph)
    }

    /// Coverage bitmap and its dimensions for one glyph at `px`.
    ///
    /// Returns `None` for a non-finite/non-positive size, an unsupported character or an empty
    /// raster. In particular,
    /// an unsupported character must not rasterise the font's `.notdef` replacement box: that
    /// would hide missing coverage and prevent a caller from selecting a fallback font.
    #[must_use]
    pub fn rasterize(&self, glyph: char, px: f32) -> Option<RasterizedGlyph> {
        if !px.is_finite() || px <= 0.0 || !self.has_glyph(glyph) {
            return None;
        }
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

/// Space above and below both the declared baseline and the pixel row used by the blitter.
/// Taking the smaller spaces matters for fractional baselines: rounding 25.6 to row 26 leaves
/// only six rows below it in a 32px cell, not the 6.4 suggested by the unrounded coordinate.
fn baseline_space(plan: &GlyphAtlasPlan) -> Option<(f32, f32)> {
    if !plan.baseline_px.is_finite() {
        return None;
    }
    let cell = plan.cell_px.max(1) as f32;
    let baseline = plan.baseline_px.clamp(0.0, cell);
    let row = baseline.round();
    Some((baseline.min(row), (cell - baseline).min(cell - row)))
}

/// A pixel size whose ascent AND descent fit around the shared atlas baseline.
///
/// Fitting only the ascent assumes every face has the same ascent/descent ratio. A fallback with
/// deeper descenders then loses the bottoms of its letters even though its ascenders fit perfectly.
/// Limit the size by both spaces, including baseline rounding, and apply that one size to the whole
/// face. This never shrinks individual glyphs independently or changes the plan's UV rectangles.
/// Glyph outlines extending beyond their face's line metrics can still be reported as clipped.
///
/// Returns `None` for a non-finite baseline or missing/invalid horizontal line metrics. A baseline
/// at an edge can leave zero room for the face, in which case the fitted size is zero.
#[must_use]
pub fn fitted_pixel_size(plan: &GlyphAtlasPlan, font: &GlyphFont) -> Option<f32> {
    let (above, below) = baseline_space(plan)?;
    let cell = plan.cell_px.max(1) as f32;
    let metrics = font.font.horizontal_line_metrics(cell)?;
    if !metrics.ascent.is_finite() || !metrics.descent.is_finite() || metrics.ascent <= 0.0 {
        return None;
    }
    let mut px = cell * (above / metrics.ascent);
    if metrics.descent < 0.0 {
        px = px.min(cell * (below / -metrics.descent));
    }
    // The rasteriser scales outlines again, then rounds their extents outward to whole pixels.
    // A few ulps of headroom prevent a rounded product just beyond an exact boundary from costing
    // an entire row. This is one negligible, uniform adjustment per face, not per-glyph resizing.
    px *= 1.0 - 4.0 * f32::EPSILON;
    px.is_finite().then_some(px)
}

/// Whether the fitted face has positive size and fits both sides of the pixel-aligned baseline.
/// Invalid baselines or unavailable metrics cannot establish a fit and return `false`.
#[must_use]
pub fn baseline_fits_font(plan: &GlyphAtlasPlan, font: &GlyphFont) -> bool {
    let Some(px) = fitted_pixel_size(plan, font) else {
        return false;
    };
    if px <= 0.0 {
        return false;
    }
    let Some(metrics) = font.font.horizontal_line_metrics(px) else {
        return false;
    };
    let Some((above, below)) = baseline_space(plan) else {
        return false;
    };
    // A second metrics scaling can round a few ulps beyond the computed constraint.
    let tolerance = f32::EPSILON * plan.cell_px.max(1) as f32 * 4.0;
    metrics.ascent <= above + tolerance && -metrics.descent <= below + tolerance
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
    /// Characters not mapped by any supplied font, in plan order (whitespace excluded).
    /// Supported empty glyphs are not missing: a zero-width character can legitimately have no ink.
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
///
/// For multilingual labels, use [`rasterize_atlas_with_fallbacks`] with an ordered font stack.
#[must_use]
pub fn rasterize_atlas(plan: &GlyphAtlasPlan, font: &GlyphFont) -> AtlasCoverage {
    rasterize_atlas_with_fallbacks(plan, &[font])
}

/// Rasterise an atlas using the first font in `fonts` that covers each character.
///
/// Fonts are in caller-defined priority order; no system-font lookup or platform-dependent fallback
/// is performed. Each face is fitted once to the same atlas baseline. The atlas plan, cell order,
/// UVs and text quads are unchanged, so the result can replace a single-font atlas directly.
/// Selection uses character-map coverage, not whether a raster has ink: a supported empty glyph
/// must stay empty rather than acquire a visible glyph from another face.
///
/// An empty stack produces a transparent atlas and reports every non-whitespace planned character
/// as missing. Unsupported characters never paint `.notdef` boxes. This is character fallback,
/// not script shaping, bidi reordering, ligature substitution or color-emoji rendering.
#[must_use]
pub fn rasterize_atlas_with_fallbacks(
    plan: &GlyphAtlasPlan,
    fonts: &[&GlyphFont],
) -> AtlasCoverage {
    let width = plan.texture_px[0].max(1) as usize;
    let height = plan.texture_px[1].max(1) as usize;
    let mut bitmap = vec![0_u8; width * height];
    let mut rendered = 0_usize;
    let mut missing = Vec::new();
    let mut clipped = Vec::new();

    // Fit each face once, not once per glyph. A fallback face can have very different line metrics
    // from the primary face; sharing its pixel size would destroy the common baseline convention.
    let fitted: Vec<(&GlyphFont, f32)> = fonts
        .iter()
        .map(|&font| {
            let px = fitted_pixel_size(plan, font).unwrap_or_else(|| plan.cell_px.max(1) as f32);
            (font, px)
        })
        .collect();

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

        let Some(&(font, px)) = fitted.iter().find(|(font, _)| font.has_glyph(cell.glyph)) else {
            if !cell.glyph.is_whitespace() {
                missing.push(cell.glyph);
            }
            continue;
        };
        let Some(raster) = font.rasterize(cell.glyph, px) else {
            // Character-map coverage is authoritative. Empty supported glyphs (including spaces
            // and zero-width controls) are not missing and must not fall through to another face.
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
