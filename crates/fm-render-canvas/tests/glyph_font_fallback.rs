//! Font-stack coverage without a GPU, system fonts, font downloads or skipped tests.
#![cfg(feature = "webgpu")]

use fm_render_canvas::glyph_raster::{
    GlyphFont, baseline_fits_font, fitted_pixel_size, rasterize_atlas,
    rasterize_atlas_with_fallbacks,
};
use fm_render_canvas::{CanvasRenderConfig, GlyphAtlasPlan, GpuRenderPlan};

fn plan_for(label: &str) -> GpuRenderPlan {
    let ir = fm_parser::parse(&format!("flowchart LR\n  A[{label}]\n")).ir;
    let layout = fm_layout::layout_diagram(&ir);
    GpuRenderPlan::from_layout(
        &ir,
        &layout,
        CanvasRenderConfig::default().edge_stroke_width as f32,
    )
}

fn set_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

fn set_i16(bytes: &mut [u8], offset: usize, value: i16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

fn set_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn word(value: usize) -> u32 {
    u32::try_from(value).expect("tiny synthetic font fits u32")
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0_u32, |sum, chunk| {
        let mut padded = [0; 4];
        padded[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(padded))
    })
}

/// A single rectangular TrueType contour, not a copied typeface or an external font fixture.
fn rectangle(width: i16, bottom: i16, top: i16) -> Vec<u8> {
    let mut data = vec![0; 14];
    set_i16(&mut data, 0, 1); // numberOfContours
    set_i16(&mut data, 4, bottom);
    set_i16(&mut data, 6, width);
    set_i16(&mut data, 8, top);
    set_u16(&mut data, 10, 3); // last point of the contour
    data.extend_from_slice(&[1; 4]); // four on-curve points, signed 16-bit deltas
    for delta in [0, width, 0, -width, bottom, 0, top - bottom, 0] {
        data.extend_from_slice(&delta.to_be_bytes());
    }
    data
}

/// Build a minimal SFNT with a visible `.notdef`, one named rectangle and one empty glyph.
/// A visible `.notdef` is essential: missing coverage used to rasterise it and count as success.
/// Space and zero-width space map to the empty glyph; format-12 cmap also supports non-BMP tests.
fn font_bytes(character: char, width: i16, ascent: i16, descent: i16) -> Vec<u8> {
    let notdef = rectangle(100, 0, 600);
    let visible = rectangle(width, descent.min(0), ascent);
    let mut glyf = notdef.clone();
    glyf.extend_from_slice(&visible);
    let mut loca = Vec::new();
    for offset in [0, notdef.len(), glyf.len(), glyf.len()] {
        loca.extend_from_slice(&word(offset).to_be_bytes());
    }

    let mut groups = vec![(u32::from(' '), 2_u32), (u32::from(character), 1)];
    if character != '\u{200b}' {
        groups.push((0x200b, 2));
    }
    groups.sort_unstable_by_key(|&(codepoint, _)| codepoint);
    let mut cmap = vec![0; 28];
    set_u16(&mut cmap, 2, 1); // one Unicode encoding record
    set_u16(&mut cmap, 4, 3); // Windows Unicode full repertoire
    set_u16(&mut cmap, 6, 10);
    set_u32(&mut cmap, 8, 12);
    set_u16(&mut cmap, 12, 12); // cmap format 12
    set_u32(&mut cmap, 16, word(16 + groups.len() * 12));
    set_u32(&mut cmap, 24, word(groups.len()));
    for (codepoint, glyph) in groups {
        for value in [codepoint, codepoint, glyph] {
            cmap.extend_from_slice(&value.to_be_bytes());
        }
    }

    let mut head = vec![0; 54];
    set_u32(&mut head, 0, 0x0001_0000);
    set_u32(&mut head, 12, 0x5f0f_3cf5);
    set_u16(&mut head, 18, 1000); // unitsPerEm
    set_i16(&mut head, 38, descent.min(0));
    set_i16(&mut head, 40, width.max(100));
    set_i16(&mut head, 42, ascent.max(600));
    set_u16(&mut head, 46, 8);
    set_i16(&mut head, 48, 2);
    set_i16(&mut head, 50, 1); // long loca offsets
    let mut hhea = vec![0; 36];
    set_u32(&mut hhea, 0, 0x0001_0000);
    set_i16(&mut hhea, 4, ascent);
    set_i16(&mut hhea, 6, descent);
    set_u16(&mut hhea, 10, 700);
    set_i16(&mut hhea, 16, width.max(100));
    set_i16(&mut hhea, 18, 1);
    set_u16(&mut hhea, 34, 3);
    let mut hmtx = vec![0; 12];
    for offset in [0, 4, 8] {
        set_u16(&mut hmtx, offset, 700);
    }
    let mut maxp = vec![0; 32];
    set_u32(&mut maxp, 0, 0x0001_0000);
    set_u16(&mut maxp, 4, 3);
    set_u16(&mut maxp, 6, 4);
    set_u16(&mut maxp, 8, 1);
    set_u16(&mut maxp, 14, 2);

    let tables = [
        (*b"cmap", cmap),
        (*b"glyf", glyf),
        (*b"head", head),
        (*b"hhea", hhea),
        (*b"hmtx", hmtx),
        (*b"loca", loca),
        (*b"maxp", maxp),
    ];
    let mut bytes = vec![0; 12 + tables.len() * 16];
    set_u32(&mut bytes, 0, 0x0001_0000);
    set_u16(&mut bytes, 4, 7);
    set_u16(&mut bytes, 6, 64);
    set_u16(&mut bytes, 8, 2);
    set_u16(&mut bytes, 10, 48);
    let mut head_offset = 0;
    for (index, (tag, data)) in tables.into_iter().enumerate() {
        let record = 12 + index * 16;
        let offset = bytes.len();
        bytes[record..record + 4].copy_from_slice(&tag);
        set_u32(&mut bytes, record + 4, checksum(&data));
        set_u32(&mut bytes, record + 8, word(offset));
        set_u32(&mut bytes, record + 12, word(data.len()));
        if &tag == b"head" {
            head_offset = offset;
        }
        bytes.extend_from_slice(&data);
        bytes.resize(bytes.len().next_multiple_of(4), 0);
    }
    let adjustment = 0xb1b0_afba_u32.wrapping_sub(checksum(&bytes));
    set_u32(&mut bytes, head_offset + 8, adjustment);
    assert_eq!(checksum(&bytes), 0xb1b0_afba);
    bytes
}

fn font(character: char, width: i16) -> GlyphFont {
    GlyphFont::from_bytes(&font_bytes(character, width, 800, -200))
        .expect("synthetic outline font parses")
}

#[test]
fn unsupported_characters_do_not_rasterise_notdef() {
    let primary = font('A', 400);
    assert!(primary.has_glyph('A'));
    assert!(primary.rasterize('A', 32.0).is_some());
    assert!(!primary.has_glyph('中'));
    assert!(primary.rasterize('中', 32.0).is_none());
    let plan = plan_for("A中");
    let atlas = rasterize_atlas(&plan.glyph_atlas, &primary);
    assert_eq!(atlas.rendered, 1);
    assert_eq!(atlas.missing, ['中']);
}

#[test]
fn multilingual_stack_combines_actual_glyphs_without_changing_the_plan() {
    let primary = font('A', 400);
    let greek = font('λ', 300);
    let cjk = font('中', 600);
    let plan = plan_for("Aλ中");
    assert_eq!(plan.glyph_atlas.cells.len(), 3);
    let combined = rasterize_atlas_with_fallbacks(&plan.glyph_atlas, &[&primary, &greek, &cjk]);
    let singles: Vec<_> = [&primary, &greek, &cjk]
        .iter()
        .map(|font| rasterize_atlas(&plan.glyph_atlas, font))
        .collect();
    let expected: Vec<u8> = (0..combined.bitmap.len())
        .map(|pixel| {
            singles
                .iter()
                .map(|atlas| atlas.bitmap[pixel])
                .max()
                .expect("three single-font atlases")
        })
        .collect();
    assert_eq!(combined.rendered, 3);
    assert!(combined.missing.is_empty());
    assert_eq!(combined.bitmap, expected);
    assert_eq!(
        combined,
        rasterize_atlas_with_fallbacks(&plan.glyph_atlas, &[&primary, &greek, &cjk])
    );
}

#[test]
fn the_first_covering_face_wins_and_single_font_api_is_compatible() {
    let narrow = font('A', 200);
    let wide = font('A', 600);
    let plan = plan_for("A");
    let narrow_atlas = rasterize_atlas(&plan.glyph_atlas, &narrow);
    let wide_atlas = rasterize_atlas(&plan.glyph_atlas, &wide);
    assert_ne!(narrow_atlas.bitmap, wide_atlas.bitmap);
    assert_eq!(
        narrow_atlas,
        rasterize_atlas_with_fallbacks(&plan.glyph_atlas, &[&narrow, &wide])
    );
    assert_eq!(
        wide_atlas,
        rasterize_atlas_with_fallbacks(&plan.glyph_atlas, &[&wide, &narrow])
    );
    assert_eq!(
        narrow_atlas,
        rasterize_atlas_with_fallbacks(&plan.glyph_atlas, &[&narrow])
    );
}

#[test]
fn an_empty_stack_reports_missing_in_plan_order_and_paints_nothing() {
    let plan = plan_for("Aλ中");
    let atlas = rasterize_atlas_with_fallbacks(&plan.glyph_atlas, &[]);
    let expected: Vec<_> = plan
        .glyph_atlas
        .cells
        .iter()
        .map(|cell| cell.glyph)
        .collect();
    assert_eq!(atlas.missing, expected);
    assert_eq!(atlas.rendered, 0);
    assert!(atlas.bitmap.iter().all(|&pixel| pixel == 0));
    assert!(atlas.clipped.is_empty());
}

#[test]
fn supported_empty_glyphs_do_not_fall_through_to_a_visible_fallback() {
    let primary = font('A', 400);
    let fallback = font('\u{200b}', 400);
    assert!(primary.has_glyph('\u{200b}'));
    assert!(primary.rasterize('\u{200b}', 32.0).is_none());
    assert!(fallback.rasterize('\u{200b}', 32.0).is_some());
    let mut plan = plan_for("A");
    plan.glyph_atlas.cells[0].glyph = '\u{200b}';
    let atlas = rasterize_atlas_with_fallbacks(&plan.glyph_atlas, &[&primary, &fallback]);
    assert_eq!(atlas.rendered, 0);
    assert!(atlas.missing.is_empty());
    assert!(atlas.bitmap.iter().all(|&pixel| pixel == 0));
}

#[test]
fn uncovered_whitespace_does_not_paint_a_replacement_or_report_missing() {
    let primary = font('A', 400);
    assert!(!primary.has_glyph('\u{2003}'));
    let mut plan = plan_for("A");
    plan.glyph_atlas.cells[0].glyph = '\u{2003}';
    for atlas in [
        rasterize_atlas(&plan.glyph_atlas, &primary),
        rasterize_atlas_with_fallbacks(&plan.glyph_atlas, &[]),
    ] {
        assert_eq!(atlas.rendered, 0);
        assert!(atlas.missing.is_empty());
        assert!(atlas.bitmap.iter().all(|&pixel| pixel == 0));
    }
}

#[test]
fn a_fallback_can_cover_a_non_bmp_character() {
    let primary = font('A', 400);
    let fallback = font('\u{1f680}', 600);
    let mut plan = plan_for("A");
    plan.glyph_atlas.cells[0].glyph = '\u{1f680}';
    assert!(!primary.has_glyph('\u{1f680}'));
    assert!(fallback.has_glyph('\u{1f680}'));
    let atlas = rasterize_atlas_with_fallbacks(&plan.glyph_atlas, &[&primary, &fallback]);
    assert_eq!(atlas.rendered, 1);
    assert!(atlas.missing.is_empty());
    assert_eq!(atlas, rasterize_atlas(&plan.glyph_atlas, &fallback));
}

#[test]
fn deep_descenders_fit_instead_of_being_cropped_at_the_bottom_of_the_cell() {
    let deep = GlyphFont::from_bytes(&font_bytes('g', 200, 800, -400)).expect("deep face parses");
    let plan = plan_for("g");
    let cell = plan.glyph_atlas.cell_px as f32;
    let baseline = plan.glyph_atlas.baseline_px;
    let old_px = cell * baseline / deep.ascent(cell).expect("face has line metrics");
    let old_raster = deep.rasterize('g', old_px).expect("old size has ink");
    assert!(
        -(old_raster.ymin as f32) > cell - baseline.round(),
        "control: ascent-only sizing must overflow this face's descenders"
    );
    let fitted = fitted_pixel_size(&plan.glyph_atlas, &deep).expect("face fits");
    assert!(fitted > 0.0 && fitted < old_px);
    assert!(baseline_fits_font(&plan.glyph_atlas, &deep));
    let atlas = rasterize_atlas(&plan.glyph_atlas, &deep);
    assert_eq!(atlas.rendered, 1);
    assert!(atlas.missing.is_empty());
    assert!(
        atlas.clipped.is_empty(),
        "descender was cropped: {:?}",
        atlas.clipped
    );
}

#[test]
fn fallback_faces_with_different_line_metrics_are_fitted_independently() {
    let primary = font('A', 200);
    let fallback =
        GlyphFont::from_bytes(&font_bytes('中', 200, 600, -600)).expect("fallback face parses");
    let plan = plan_for("A中");
    let a = fitted_pixel_size(&plan.glyph_atlas, &primary).expect("primary fits");
    let b = fitted_pixel_size(&plan.glyph_atlas, &fallback).expect("fallback fits");
    assert!(
        b < a,
        "deep fallback must not inherit the primary's pixel size"
    );
    assert!(baseline_fits_font(&plan.glyph_atlas, &primary));
    assert!(baseline_fits_font(&plan.glyph_atlas, &fallback));
    let atlas = rasterize_atlas_with_fallbacks(&plan.glyph_atlas, &[&primary, &fallback]);
    assert_eq!(atlas.rendered, 2);
    assert!(atlas.missing.is_empty());
    assert!(atlas.clipped.is_empty());
}

#[test]
fn fractional_baselines_budget_the_rows_used_by_the_blitter() {
    let deep = GlyphFont::from_bytes(&font_bytes('g', 200, 800, -400)).expect("deep face parses");
    let mut plan = plan_for("g");
    for fraction in [0.19, 0.21, 0.79, 0.8, 0.81] {
        plan.glyph_atlas.baseline_px = plan.glyph_atlas.cell_px as f32 * fraction;
        assert!(baseline_fits_font(&plan.glyph_atlas, &deep));
        let atlas = rasterize_atlas(&plan.glyph_atlas, &deep);
        assert_eq!(atlas.rendered, 1);
        assert!(
            atlas.clipped.is_empty(),
            "baseline fraction {fraction} clipped"
        );
    }
}

#[test]
fn outline_scaling_does_not_round_a_fitted_face_outside_its_cell() {
    // The unconservative f32 fit can round the descent to 7.0000005 or the ascent to
    // 7.0000005 pixels here. Outward rounding then turns that into an eighth row.
    for (cell, ascent, descent, baseline) in
        [(32, 1925, -749, 24.677_412), (16, 869, -256, 7.012_917)]
    {
        let face = GlyphFont::from_bytes(&font_bytes('g', 200, ascent, descent))
            .expect("boundary face parses");
        let mut plan = GlyphAtlasPlan::for_texts(["g"], cell);
        plan.baseline_px = baseline;
        assert!(baseline_fits_font(&plan, &face));
        let atlas = rasterize_atlas(&plan, &face);
        assert_eq!(atlas.rendered, 1);
        assert!(
            atlas.clipped.is_empty(),
            "cell {cell} baseline {baseline} clipped"
        );
    }
}

#[test]
fn non_finite_baselines_do_not_produce_a_claimed_fit() {
    let face = font('A', 200);
    let mut plan = plan_for("A");
    for baseline in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        plan.glyph_atlas.baseline_px = baseline;
        assert!(fitted_pixel_size(&plan.glyph_atlas, &face).is_none());
        assert!(!baseline_fits_font(&plan.glyph_atlas, &face));
    }
    for baseline in [-100.0, plan.glyph_atlas.cell_px as f32 + 100.0] {
        plan.glyph_atlas.baseline_px = baseline;
        let px = fitted_pixel_size(&plan.glyph_atlas, &face).expect("finite edge baseline");
        assert!(px.abs() < f32::EPSILON);
        assert!(!baseline_fits_font(&plan.glyph_atlas, &face));
    }
}

#[test]
fn invalid_pixel_sizes_do_not_reach_the_font_rasteriser() {
    let face = font('A', 200);
    for px in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -32.0, 0.0] {
        assert!(face.rasterize('A', px).is_none());
    }
    assert!(face.rasterize('A', 32.0).is_some());
}
