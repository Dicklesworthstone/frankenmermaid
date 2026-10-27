//! Browser font rasterization for the WebGPU text atlas.
//!
//! Each quad samples a square cell, independently of the glyph's advance. Ink is centered
//! horizontally in that cell and placed on one shared alphabetic baseline. Canvas supplies
//! the configured font stack, including browser font fallback; no font file is bundled here.

#[cfg(target_arch = "wasm32")]
use fm_render_canvas::GlyphAtlasPlan;

pub(crate) fn coverage_len([width, height]: [u32; 2]) -> Result<usize, String> {
    if width == 0 || height == 0 {
        return Err("WebGPU glyph atlas dimensions must be nonzero".to_string());
    }
    usize::try_from(width)
        .ok()
        .and_then(|width| usize::try_from(height).ok()?.checked_mul(width))
        .ok_or_else(|| "WebGPU glyph atlas dimensions overflow addressable memory".to_string())
}

#[derive(Clone, Copy, Debug)]
struct InkMetrics {
    left: f64,
    right: f64,
    ascent: f64,
    descent: f64,
}

impl InkMetrics {
    fn is_finite(self) -> bool {
        [self.left, self.right, self.ascent, self.descent]
            .into_iter()
            .all(f64::is_finite)
    }

    fn centered_origin(self, cell_left: f64, cell_width: f64) -> f64 {
        // Canvas's left/right metrics are distances FROM the text origin. Center the ink,
        // rather than its advance, including negative side bearings and italic overhangs.
        cell_left + (cell_width + self.left - self.right) * 0.5
    }
}

/// Choose ONE size for the entire atlas, including fallback glyphs and deep descenders.
/// Resizing individual glyphs would enlarge punctuation and destroy relative letter heights.
fn fitted_font_size(cell: f64, baseline: f64, metrics: &[InkMetrics]) -> Result<f64, String> {
    if !cell.is_finite()
        || cell <= 0.0
        || !baseline.is_finite()
        || !(0.0..=cell).contains(&baseline)
        || metrics.iter().any(|metrics| !metrics.is_finite())
    {
        return Err("WebGPU glyph atlas has invalid font metrics or baseline".to_string());
    }
    let gutter = 1.0_f64.min(cell / 8.0);
    let above = (baseline - gutter).max(0.0);
    let below = (cell - baseline - gutter).max(0.0);
    let mut scale = 1.0_f64;
    for glyph in metrics {
        for (available, required) in [
            (above, glyph.ascent),
            (below, glyph.descent),
            (cell - 2.0 * gutter, glyph.left + glyph.right),
        ] {
            if required > 0.0 {
                scale = scale.min(available / required);
            }
        }
    }
    let size = cell * scale;
    if size <= 0.0 || !size.is_finite() {
        return Err("WebGPU glyphs do not fit around the atlas baseline".to_string());
    }
    Ok(size)
}

fn alpha_coverage(rgba: &[u8], dimensions: [u32; 2]) -> Result<Vec<u8>, String> {
    let expected = coverage_len(dimensions)?
        .checked_mul(4)
        .ok_or("WebGPU glyph atlas RGBA size overflows addressable memory")?;
    if rgba.len() != expected {
        return Err(format!(
            "Canvas returned {} glyph atlas bytes, expected {expected}",
            rgba.len()
        ));
    }
    Ok(rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| pixel[3])
        .collect())
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::{GlyphAtlasPlan, InkMetrics, alpha_coverage, coverage_len, fitted_font_size};
    use js_sys::{Function, Promise, Reflect};
    use wasm_bindgen::{JsCast, JsValue};
    use web_sys::CanvasRenderingContext2d;

    fn js_failure(operation: &str, error: JsValue) -> String {
        format!("{operation}: {error:?}")
    }

    /// Use an independent surface so atlas generation cannot clear or resize the diagram.
    /// OffscreenCanvas works in workers; older main-thread browsers can use a detached canvas.
    fn create_context(width: u32, height: u32) -> Result<CanvasRenderingContext2d, String> {
        let global = js_sys::global();
        let offscreen = Reflect::get(&global, &JsValue::from_str("OffscreenCanvas"));
        if offscreen.is_ok_and(|value| value.is_function())
            && let Ok(canvas) = web_sys::OffscreenCanvas::new(width, height)
            && let Ok(Some(context)) = canvas.get_context("2d")
        {
            // OffscreenCanvasRenderingContext2D has the same methods used below. As with
            // Diagram::from_offscreen_canvas, a structural cast avoids a DOM dependency.
            return Ok(context.unchecked_into());
        }

        let document = Reflect::get(&global, &JsValue::from_str("document"))
            .map_err(|error| js_failure("failed to access the glyph atlas document", error))?;
        if document.is_null() || document.is_undefined() {
            return Err(
                "WebGPU text requires an OffscreenCanvas 2D context in a worker".to_string(),
            );
        }
        let create_element = Reflect::get(&document, &JsValue::from_str("createElement"))
            .and_then(|value| value.dyn_into::<Function>())
            .map_err(|error| js_failure("failed to create the glyph atlas canvas", error))?;
        let canvas = create_element
            .call1(&document, &JsValue::from_str("canvas"))
            .and_then(|value| value.dyn_into::<web_sys::HtmlCanvasElement>())
            .map_err(|error| js_failure("failed to create the glyph atlas canvas", error))?;
        canvas.set_width(width);
        canvas.set_height(height);
        canvas
            .get_context("2d")
            .map_err(|error| js_failure("failed to acquire the glyph atlas 2D context", error))?
            .map(JsCast::unchecked_into)
            .ok_or_else(|| "Canvas2D is unavailable for WebGPU glyph rasterization".to_string())
    }

    /// Wait for fonts used by these actual characters, including unicode-range web fonts.
    /// Workers expose fonts on the global scope; windows expose them on document.
    async fn load_fonts(font: &str, sample: &str) -> Result<(), String> {
        let global = js_sys::global();
        let mut fonts = Reflect::get(&global, &JsValue::from_str("fonts"))
            .map_err(|error| js_failure("failed to access browser fonts", error))?;
        if fonts.is_null() || fonts.is_undefined() {
            let document = Reflect::get(&global, &JsValue::from_str("document"))
                .map_err(|error| js_failure("failed to access browser fonts", error))?;
            if document.is_null() || document.is_undefined() {
                return Ok(());
            }
            fonts = Reflect::get(&document, &JsValue::from_str("fonts"))
                .map_err(|error| js_failure("failed to access document fonts", error))?;
        }
        // Browsers without the Font Loading API can still rasterize their installed fonts.
        if fonts.is_null() || fonts.is_undefined() {
            return Ok(());
        }
        let load = Reflect::get(&fonts, &JsValue::from_str("load"))
            .and_then(|value| value.dyn_into::<Function>())
            .map_err(|error| js_failure("failed to access browser font loading", error))?;
        let promise = load
            .call2(&fonts, &JsValue::from_str(font), &JsValue::from_str(sample))
            .map_err(|error| js_failure("failed to load WebGPU label fonts", error))?;
        wasm_bindgen_futures::JsFuture::from(Promise::resolve(&promise))
            .await
            .map_err(|error| js_failure("failed to load WebGPU label fonts", error))?;
        Ok(())
    }

    fn measure(context: &CanvasRenderingContext2d, glyph: char) -> Result<InkMetrics, String> {
        let metrics = context
            .measure_text(&glyph.to_string())
            .map_err(|error| js_failure("failed to measure a WebGPU label glyph", error))?;
        Ok(InkMetrics {
            left: metrics.actual_bounding_box_left(),
            right: metrics.actual_bounding_box_right(),
            ascent: metrics.actual_bounding_box_ascent(),
            descent: metrics.actual_bounding_box_descent(),
        })
    }

    pub(crate) async fn rasterize(
        plan: &GlyphAtlasPlan,
        font_family: &str,
    ) -> Result<Vec<u8>, String> {
        coverage_len(plan.texture_px)?;
        let font_family = if font_family.trim().is_empty() {
            "sans-serif"
        } else {
            font_family
        };
        let initial_font = format!("{}px {font_family}", plan.cell_px);
        let sample: String = plan.cells.iter().map(|cell| cell.glyph).collect();
        load_fonts(&initial_font, &sample).await?;

        let [width, height] = plan.texture_px;
        let context = create_context(width, height)?;
        context.set_text_align("left");
        context.set_text_baseline("alphabetic");
        context.set_font(&initial_font);
        let metrics: Vec<_> = plan
            .cells
            .iter()
            .map(|cell| measure(&context, cell.glyph))
            .collect::<Result<_, _>>()?;
        let font_px = fitted_font_size(
            f64::from(plan.cell_px),
            f64::from(plan.baseline_px),
            &metrics,
        )?;
        context.set_font(&format!("{font_px}px {font_family}"));
        context.set_fill_style_str("#ffffff");

        for cell in &plan.cells {
            if cell.glyph.is_whitespace() {
                continue;
            }
            let x0 = (f64::from(cell.uv_min[0]) * f64::from(width)).round();
            let y0 = (f64::from(cell.uv_min[1]) * f64::from(height)).round();
            let x1 = (f64::from(cell.uv_max[0]) * f64::from(width)).round();
            let y1 = (f64::from(cell.uv_max[1]) * f64::from(height)).round();
            let metrics = measure(&context, cell.glyph)?;
            let x = metrics.centered_origin(x0, x1 - x0);
            let y = y0 + f64::from(plan.baseline_px);
            context.save();
            context.begin_path();
            context.rect(x0, y0, x1 - x0, y1 - y0);
            context.clip();
            let painted = context.fill_text(&cell.glyph.to_string(), x, y);
            context.restore();
            painted
                .map_err(|error| js_failure("failed to rasterize a WebGPU label glyph", error))?;
        }
        let rgba = context
            // web-sys uses f64 here in its stable API and i32 with unstable APIs enabled.
            .get_image_data(0 as _, 0 as _, width as _, height as _)
            .map_err(|error| js_failure("failed to read the WebGPU glyph atlas", error))?
            .data();
        alpha_coverage(&rgba.0, plan.texture_px)
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) use browser::rasterize;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shared_size_fits_accents_descenders_and_wide_fallback_glyphs() {
        let metrics = [
            InkMetrics {
                left: 0.0,
                right: 22.0,
                ascent: 24.0,
                descent: 0.0,
            },
            InkMetrics {
                left: 2.0,
                right: 18.0,
                ascent: 15.0,
                descent: 10.0,
            },
            InkMetrics {
                left: 4.0,
                right: 48.0,
                ascent: 30.0,
                descent: 2.0,
            },
        ];
        let size = fitted_font_size(32.0, 25.0, &metrics).expect("font fits");
        let scale = size / 32.0;
        assert!(
            scale > 0.5 && scale < 0.6,
            "wide fallback must limit the whole face"
        );
        for glyph in metrics {
            assert!(25.0 - glyph.ascent * scale >= 1.0);
            assert!(25.0 + glyph.descent * scale <= 31.0);
            assert!((glyph.left + glyph.right) * scale <= 30.0);
        }
        // The lower-case glyph retains its smaller ascent and a real descender; it was not
        // independently enlarged to the same bounding box as the capital/accented glyph.
        assert!(metrics[1].ascent * scale < metrics[0].ascent * scale);
        assert!(metrics[1].descent * scale > 0.0);
    }

    #[test]
    fn ink_centering_accounts_for_both_positive_and_negative_side_bearings() {
        for (left, right) in [(4.0, 22.0), (-2.0, 10.0), (0.0, 8.0)] {
            let glyph = InkMetrics {
                left,
                right,
                ascent: 20.0,
                descent: 4.0,
            };
            let origin = glyph.centered_origin(32.0, 32.0);
            let ink_left = origin - left;
            let ink_right = origin + right;
            assert_eq!((ink_left + ink_right) / 2.0, 48.0);
            assert!(ink_left >= 32.0 && ink_right <= 64.0);
        }
    }

    #[test]
    fn coverage_preserves_transparency_and_antialiasing_instead_of_filling_cells() {
        let rgba = [255, 255, 255, 0, 255, 255, 255, 73, 0, 0, 0, 255];
        assert_eq!(alpha_coverage(&rgba, [3, 1]).unwrap(), [0, 73, 255]);
        assert!(alpha_coverage(&rgba[..11], [3, 1]).is_err());
        assert!(coverage_len([0, 1]).is_err());
    }

    #[test]
    fn invalid_metrics_and_unusable_baselines_fail_explicitly() {
        let glyph = InkMetrics {
            left: 0.0,
            right: 20.0,
            ascent: 20.0,
            descent: 5.0,
        };
        assert!(fitted_font_size(32.0, f64::NAN, &[glyph]).is_err());
        assert!(fitted_font_size(32.0, 33.0, &[glyph]).is_err());
        assert!(fitted_font_size(32.0, 0.0, &[glyph]).is_err());
        assert!(fitted_font_size(32.0, 32.0, &[glyph]).is_err());
        assert!(
            fitted_font_size(
                32.0,
                25.0,
                &[InkMetrics {
                    ascent: f64::NAN,
                    ..glyph
                }]
            )
            .is_err()
        );
        assert_eq!(fitted_font_size(32.0, 25.0, &[]).unwrap(), 32.0);
    }
}
