//! bd-2u0.2: real glyph rasterisation into the text atlas.
//!
//! The text device pass previously ran against `solid_coverage`, a synthetic bitmap with every
//! planned cell filled. That proved the UV rectangles addressed the cells they claimed, and nothing
//! about letterforms. These tests use a real font and assert the properties that distinguish a
//! rasterised atlas from a filled rectangle.
//!
//! Most of this file needs no GPU: an atlas bitmap is a `Vec<u8>`, so glyph-to-cell placement is
//! checkable on any machine. Only the last test builds a device.
#![cfg(feature = "webgpu")]

use fm_render_canvas::CanvasRenderConfig;
use fm_render_canvas::GpuRenderPlan;
use fm_render_canvas::glyph_raster::{GlyphFont, rasterize_atlas};
use fm_render_canvas::gpu_device::{
    GlyphAtlasTexture, GpuDevice, GpuDeviceError, InstanceDraw, InstancePass, render_instances,
    solid_coverage, text_instance_bytes,
};
use fm_render_canvas::gpu_pipeline::text_pipeline;

/// A font this box actually has. Chosen by path rather than through fontconfig so the test is
/// deterministic: fontconfig's "sans-serif" resolves differently per machine, and an atlas rendered
/// with a different face would make any byte-level expectation meaningless.
const FONT_PATH: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";

fn font_or_skip() -> Option<GlyphFont> {
    let Ok(bytes) = std::fs::read(FONT_PATH) else {
        // Reported, never silent: a test that returns quietly on a fontless box is
        // indistinguishable from one that passed.
        eprintln!("[glyph] SKIPPED: no font at {FONT_PATH}");
        return None;
    };
    Some(GlyphFont::from_bytes(&bytes).expect("DejaVuSans should parse"))
}

fn plan_for(source: &str) -> GpuRenderPlan {
    let ir = fm_parser::parse(source).ir;
    let layout = fm_layout::layout_diagram(&ir);
    GpuRenderPlan::from_layout(
        &ir,
        &layout,
        CanvasRenderConfig::default().edge_stroke_width as f32,
    )
}

/// THE LOAD-BEARING DIFFERENCE: a rasterised atlas is not a filled rectangle.
///
/// `solid_coverage` fills every planned cell edge to edge. Real glyphs leave the space around their
/// ink transparent. If this module ever regressed to filling cells, every other assertion here would
/// still pass — so the distinguishing property is asserted directly: within a cell that received a
/// glyph, some pixels are covered and some are not.
#[test]
fn a_rasterised_cell_has_both_ink_and_gaps_unlike_the_synthetic_fill() {
    let Some(font) = font_or_skip() else {
        return;
    };
    let plan = plan_for("flowchart LR\n  A[Alpha] --> B[Beta]\n");
    assert!(
        !plan.glyph_atlas.cells.is_empty(),
        "CONTROL FAILED: no atlas cells"
    );

    let real = rasterize_atlas(&plan.glyph_atlas, &font);
    assert!(
        real.rendered > 0,
        "no glyph was rasterised at all; missing={:?}",
        real.missing
    );
    assert!(
        real.missing.is_empty(),
        "DejaVuSans has no outline for {:?}, which a diagram font must",
        real.missing
    );

    let width = plan.glyph_atlas.texture_px[0] as usize;
    let height = plan.glyph_atlas.texture_px[1] as usize;

    // Pick a cell whose glyph definitely has both ink and surrounding space.
    let cell = plan
        .glyph_atlas
        .cells
        .iter()
        .find(|c| c.glyph == 'A')
        .expect("the fixture labels contain 'A'");
    let x0 = (cell.uv_min[0] * width as f32).round() as usize;
    let y0 = (cell.uv_min[1] * height as f32).round() as usize;
    let x1 = ((cell.uv_max[0] * width as f32).round() as usize).min(width);
    let y1 = ((cell.uv_max[1] * height as f32).round() as usize).min(height);

    let mut ink = 0_usize;
    let mut gap = 0_usize;
    for y in y0..y1 {
        for x in x0..x1 {
            if real.bitmap[y * width + x] > 0 {
                ink += 1;
            } else {
                gap += 1;
            }
        }
    }
    assert!(
        ink > 0,
        "the cell for 'A' is entirely empty: no ink was drawn"
    );
    assert!(
        gap > 0,
        "the cell for 'A' is entirely covered, which is what the SYNTHETIC atlas looks like -- \
         real glyph rasterisation has regressed to filling cells"
    );

    // And the two atlases must genuinely differ, which is the whole point of this commit.
    let synthetic = solid_coverage(&plan.glyph_atlas);
    assert_ne!(
        real.bitmap, synthetic,
        "the rasterised atlas is byte-identical to the synthetic fill"
    );
}

/// Ink must stay inside the cell it belongs to. A glyph bleeding into a neighbour would be sampled
/// by the wrong quad, so one label would show a fragment of another's letter.
#[test]
fn no_glyph_paints_outside_its_own_cell() {
    let Some(font) = font_or_skip() else {
        return;
    };
    // Descenders and tall glyphs, which are the ones that would overflow a square cell.
    let plan = plan_for("flowchart LR\n  A[gjpqy] --> B[HIJKL]\n");
    let real = rasterize_atlas(&plan.glyph_atlas, &font);
    let width = plan.glyph_atlas.texture_px[0] as usize;
    let height = plan.glyph_atlas.texture_px[1] as usize;

    // Every covered pixel must fall inside SOME planned cell. Anything else is ink in the gutter,
    // which is ink a quad will never sample and a neighbouring quad might.
    let mut inside = vec![false; width * height];
    for cell in &plan.glyph_atlas.cells {
        let x0 = (cell.uv_min[0] * width as f32).round() as usize;
        let y0 = (cell.uv_min[1] * height as f32).round() as usize;
        let x1 = ((cell.uv_max[0] * width as f32).round() as usize).min(width);
        let y1 = ((cell.uv_max[1] * height as f32).round() as usize).min(height);
        for y in y0..y1 {
            for x in x0..x1 {
                inside[y * width + x] = true;
            }
        }
    }

    let stray = real
        .bitmap
        .iter()
        .zip(inside.iter())
        .filter(|(coverage, is_inside)| **coverage > 0 && !**is_inside)
        .count();
    assert_eq!(
        stray, 0,
        "{stray} covered pixels fall outside every planned cell; a glyph is bleeding into the \
         gutter or a neighbour"
    );
}

/// Different glyphs must produce different ink. A rasteriser that returned the same bitmap for every
/// character would satisfy every "is there ink?" check while rendering gibberish.
#[test]
fn distinct_glyphs_rasterise_to_distinct_bitmaps() {
    let Some(font) = font_or_skip() else {
        return;
    };
    let px = 32.0;
    let i = font.rasterize('i', px).expect("'i' has an outline");
    let m = font.rasterize('M', px).expect("'M' has an outline");

    assert!(
        m.width > i.width,
        "'M' rasterised no wider than 'i' ({} vs {}), so glyph identity is being ignored",
        m.width,
        i.width
    );
    assert_ne!(
        (i.width, i.height, &i.coverage),
        (m.width, m.height, &m.coverage),
        "'i' and 'M' produced identical rasters"
    );
}

/// A space has metrics but no ink, and must not be reported as a missing glyph -- otherwise every
/// multi-word label would look like a font failure.
#[test]
fn whitespace_is_not_reported_missing() {
    let Some(font) = font_or_skip() else {
        return;
    };
    assert!(
        font.rasterize(' ', 32.0).is_none(),
        "a space rasterised to ink"
    );

    let plan = plan_for("flowchart LR\n  A[Hello World] --> B[Beta]\n");
    assert!(
        plan.glyph_atlas.cells.iter().any(|c| c.glyph == ' '),
        "CONTROL FAILED: the fixture has a space but the atlas planned no cell for it"
    );
    let real = rasterize_atlas(&plan.glyph_atlas, &font);
    assert!(
        !real.missing.contains(&' '),
        "the space was reported missing; every multi-word label would read as a font failure"
    );
}

/// Determinism: the same plan and font must produce a byte-identical atlas. Reproducible output is a
/// stated requirement of this project, and a texture that varied per run would make any golden or
/// cross-backend comparison of text meaningless.
#[test]
fn rasterising_the_same_atlas_twice_is_byte_identical() {
    let Some(font) = font_or_skip() else {
        return;
    };
    let plan = plan_for("flowchart LR\n  A[Alpha] --> B[Beta]\n  B --> C[Gamma]\n");
    let first = rasterize_atlas(&plan.glyph_atlas, &font);
    let second = rasterize_atlas(&plan.glyph_atlas, &font);
    assert_eq!(
        first.bitmap, second.bitmap,
        "atlas rasterisation is not deterministic"
    );
    assert_eq!(first.rendered, second.rendered);
}

/// END TO END ON A DEVICE: the text pipeline sampling a REAL atlas still paints, and paints less
/// than it did against the synthetic fill.
///
/// The second half is the interesting one. Real glyphs cover a fraction of each cell, so a correct
/// pipeline must paint strictly fewer pixels than it does when every cell is solid. Equal counts
/// would mean the sampler is not reading the atlas content at all.
#[test]
fn the_text_pipeline_paints_real_glyphs_and_less_than_a_solid_fill() {
    let Some(font) = font_or_skip() else {
        return;
    };
    let gpu = match GpuDevice::headless() {
        Ok(gpu) => gpu,
        Err(GpuDeviceError::NoAdapter(why)) => {
            eprintln!("[gpu] SKIPPED: no adapter ({why})");
            return;
        }
        Err(other) => panic!("wgpu present but unusable: {other}"),
    };
    eprintln!(
        "[gpu] adapter={:?} backend={:?}",
        gpu.adapter_name(),
        gpu.backend()
    );

    let plan = plan_for("flowchart LR\n  A[Alpha] --> B[Beta]\n  B --> C[Gamma]\n");
    let pass = InstancePass::new(&gpu, &text_pipeline());
    let bytes = text_instance_bytes(&plan.text_quads);
    let count = u32::try_from(plan.text_quads.len()).expect("fits u32");

    let painted_with = |coverage: &[u8]| {
        let atlas = GlyphAtlasTexture::new(&gpu, &plan.glyph_atlas, coverage);
        let draw = InstanceDraw {
            bounds: &plan.bounds,
            instance_bytes: &bytes,
            instance_count: count,
            atlas: Some(&atlas),
        };
        let image = render_instances(&gpu, &pass, &draw, 1024, 1024).expect("render");
        image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 0)
            .count()
    };

    let real = rasterize_atlas(&plan.glyph_atlas, &font);
    let real_painted = painted_with(&real.bitmap);
    let solid_painted = painted_with(&solid_coverage(&plan.glyph_atlas));

    eprintln!(
        "[glyph] rendered={} cells, painted {real_painted} px with real glyphs vs {solid_painted} \
         px with a solid fill",
        real.rendered
    );
    assert!(
        real_painted > 0,
        "a real glyph atlas painted nothing: the quads sample cells the rasteriser left empty"
    );
    assert!(
        solid_painted > 0,
        "CONTROL FAILED: even the solid atlas painted nothing, so this comparison is meaningless"
    );
    assert!(
        real_painted < solid_painted,
        "real glyphs painted {real_painted} pixels and a solid fill painted {solid_painted}; equal \
         or greater means the sampler is not reading the atlas content"
    );
}
