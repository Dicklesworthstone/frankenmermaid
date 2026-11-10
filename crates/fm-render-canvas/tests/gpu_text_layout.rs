//! GPU text must preserve the typography and authored rows drawn by Canvas2D.

use fm_core::{FontMetrics, MermaidDiagramIr};
use fm_layout::{DiagramLayout, LayoutConfig};
use fm_render_canvas::{
    Canvas2dRenderer, CanvasRenderConfig, DrawOperation, GlyphFace, GpuRenderPlan, GpuTextQuad,
    GpuTextRun, GpuTextSource, MockCanvas2dContext,
};

struct CanvasRow {
    text: String,
    x: f32,
    y: f32,
    font: String,
}

fn configured_layout(ir: &MermaidDiagramIr, config: &CanvasRenderConfig) -> DiagramLayout {
    fm_layout::layout_diagram_with_config(
        ir,
        LayoutConfig {
            font_metrics: Some(config.font_metrics()),
            ..Default::default()
        },
    )
}

fn canvas_rows(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    config: &CanvasRenderConfig,
) -> Vec<CanvasRow> {
    assert!(!config.auto_fit);
    let mut context = MockCanvas2dContext::new(1600.0, 1200.0);
    Canvas2dRenderer::new(config.clone()).render(layout, ir, &mut context);
    let mut font = String::new();
    let mut rows = Vec::new();
    for operation in context.operations() {
        match operation {
            DrawOperation::SetFont(value) => font.clone_from(value),
            DrawOperation::FillText(text, x, y) => rows.push(CanvasRow {
                text: text.clone(),
                // These fixtures have no graph edge-label collision placement, so the Canvas
                // world offset is exactly its padding minus the shared layout origin.
                x: (*x - config.padding) as f32 + layout.bounds.x,
                y: (*y - config.padding) as f32 + layout.bounds.y,
                font: font.clone(),
            }),
            _ => {}
        }
    }
    rows
}

fn quads<'a>(plan: &'a GpuRenderPlan, run: &GpuTextRun) -> &'a [GpuTextQuad] {
    &plan.text_quads[run.first_quad as usize..(run.first_quad + run.quad_count) as usize]
}

fn run_text(plan: &GpuRenderPlan, run: &GpuTextRun) -> String {
    quads(plan, run)
        .iter()
        .map(|quad| {
            plan.glyph_atlas
                .cells
                .iter()
                .find(|cell| cell.uv_min == quad.uv_min && cell.uv_max == quad.uv_max)
                .map(|cell| {
                    assert_eq!(
                        cell.face_index, run.face_index,
                        "a glyph sampled a different face"
                    );
                    cell.glyph
                })
                .expect("every emitted quad must address its glyph")
        })
        .collect()
}

fn metrics_at_size(config: &CanvasRenderConfig, font_size: f32) -> FontMetrics {
    let mut metrics = config.font_metrics().config().clone();
    metrics.font_size = font_size;
    FontMetrics::new(metrics)
}

fn glyph_width(metrics: &FontMetrics, glyph: char) -> f32 {
    metrics.estimate_width(glyph.encode_utf8(&mut [0; 4]))
}

fn assert_near(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.02,
        "actual {actual}, expected {expected}"
    );
}

fn assert_centered_row(
    plan: &GpuRenderPlan,
    run: &GpuTextRun,
    canvas: &CanvasRow,
    config: &CanvasRenderConfig,
    font_size: f32,
) {
    let text = run_text(plan, run);
    assert_eq!(text, canvas.text);
    assert!(canvas.font.starts_with(&format!("{font_size}px ")));
    let metrics = metrics_at_size(config, font_size);
    let glyphs = quads(plan, run);
    assert_eq!(glyphs.len(), text.chars().count());
    let mut pen_x = canvas.x - metrics.estimate_width(&text) * 0.5;
    for (quad, glyph) in glyphs.iter().zip(text.chars()) {
        let advance = glyph_width(&metrics, glyph);
        assert_near(quad.center[0], pen_x + advance * 0.5);
        assert_near(quad.center[1], canvas.y);
        assert_eq!(quad.half_extent, [font_size * 0.5; 2]);
        pen_x += advance;
    }
}

#[test]
fn gpu_multiline_nodes_match_canvas_with_blank_rows_and_declared_sizes() {
    let ir = fm_parser::parse(
        "flowchart TB\n A[\"First 界<br/><br/>Last line\"] --> B[\"Other<br/>Lines\"]\n \
         classDef emphasis font-size:32px\n class A emphasis\n",
    )
    .ir;
    assert!(
        ir.labels
            .iter()
            .any(|label| label.text == "First 界\n\nLast line")
    );
    let config = CanvasRenderConfig {
        font_size: 18.0,
        line_height: 1.75,
        padding: 13.0,
        auto_fit: false,
        ..Default::default()
    };
    let layout = configured_layout(&ir, &config);
    let scene = fm_layout::build_render_scene(&ir, &layout);
    let plan = GpuRenderPlan::from_layout_and_scene_with_config(&ir, &layout, &scene, &config);
    let canvas = canvas_rows(&ir, &layout, &config);
    for (node_id, expected, size, row_steps) in [
        ("A", ["First 界", "Last line"], 32.0, 2.0),
        ("B", ["Other", "Lines"], 18.0, 1.0),
    ] {
        let index = ir.nodes.iter().position(|node| node.id == node_id).unwrap();
        let runs: Vec<_> = plan
            .text_runs
            .iter()
            .filter(|run| run.source == GpuTextSource::Node && run.node_index as usize == index)
            .collect();
        assert_eq!(runs.len(), expected.len(), "one run for each visible row");
        for (run, text) in runs.iter().zip(expected) {
            let reference = canvas.iter().find(|row| row.text == text).unwrap();
            assert_centered_row(&plan, run, reference, &config, size);
        }
        assert_near(
            quads(&plan, runs[1])[0].center[1] - quads(&plan, runs[0])[0].center[1],
            size * config.line_height as f32 * row_steps,
        );
    }
    for (index, run) in plan.text_runs.iter().enumerate() {
        assert!(
            run.quad_count > 0,
            "blank rows reserve space without empty draw calls"
        );
        assert!(
            quads(&plan, run)
                .iter()
                .all(|quad| quad.run_index as usize == index)
        );
    }
    assert!(
        plan.glyph_atlas
            .cells
            .iter()
            .all(|cell| !cell.glyph.is_control())
    );
    for quad in &plan.text_quads {
        assert!(quad.center[0] - quad.half_extent[0] >= plan.bounds.x - 0.001);
        assert!(quad.center[1] - quad.half_extent[1] >= plan.bounds.y - 0.001);
        assert!(quad.center[0] + quad.half_extent[0] <= plan.bounds.x + plan.bounds.width + 0.001);
        assert!(quad.center[1] + quad.half_extent[1] <= plan.bounds.y + plan.bounds.height + 0.001);
    }
}

#[test]
fn gpu_long_unicode_labels_use_the_configured_monospace_or_proportional_metrics() {
    let label = ["iiiiWWWW 東京 é narrow and WIDE"; 5].join(" ");
    let ir = fm_parser::parse(&format!("flowchart LR\n A[\"{label}\"]\n")).ir;
    for family in ["monospace", "serif", "sans-serif"] {
        for size in [11.0, 31.0] {
            let config = CanvasRenderConfig {
                font_family: family.to_string(),
                font_size: size,
                auto_fit: false,
                ..Default::default()
            };
            let layout = configured_layout(&ir, &config);
            let plan = GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
            let run = plan
                .text_runs
                .iter()
                .find(|run| run.source == GpuTextSource::Node)
                .unwrap();
            let rows = canvas_rows(&ir, &layout, &config);
            // 1e0dc058 composes long node labels into wrapped paragraphs, so the label can
            // span several canvas rows. Match the reference to the Node run under test (its
            // first row) instead of the whole label, keeping the centring + glyph-metric checks.
            let run_glyphs = run_text(&plan, run);
            let reference = rows.iter().find(|row| row.text == run_glyphs).unwrap();
            assert_centered_row(&plan, run, reference, &config, size as f32);
            assert!(reference.font.ends_with(family));
            let glyphs = quads(&plan, run);
            let narrow_step = glyphs[1].center[0] - glyphs[0].center[0];
            let wide_step = glyphs[6].center[0] - glyphs[5].center[0];
            if family == "monospace" {
                assert_near(narrow_step, wide_step);
            } else {
                assert!(wide_step > narrow_step * 2.0);
            }
            if size > 15.0 {
                assert!(
                    plan.glyph_atlas.cell_px > 32,
                    "large configured fonts need sharp atlas cells"
                );
            }
        }
    }
}

#[test]
fn gpu_sequence_notes_keep_measured_wrapping_and_configured_line_height() {
    let ir = fm_parser::parse(
        "sequenceDiagram\n participant A\n participant B\n \
         Note left of A: Customer checks the current account balance before approving a payment<br/><br/>Finish\n \
         A->>B: Continue\n",
    )
    .ir;
    let config = CanvasRenderConfig {
        font_family: "monospace".to_string(),
        font_size: 22.0,
        line_height: 1.75,
        auto_fit: false,
        ..Default::default()
    };
    let layout = configured_layout(&ir, &config);
    let note = &layout.extensions.sequence_notes[0];
    assert!(
        note.text.lines().count() > 3,
        "the fixture must exercise automatic wrapping"
    );
    let plan = GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
    let rows = canvas_rows(&ir, &layout, &config);
    let runs: Vec<_> = plan
        .text_runs
        .iter()
        .filter(|run| run.source == GpuTextSource::SequenceNote)
        .collect();
    let expected: Vec<_> = note.text.lines().filter(|line| !line.is_empty()).collect();
    assert_eq!(runs.len(), expected.len());
    let font_size = config.font_size as f32 * fm_layout::LayoutSequenceNote::FONT_SCALE;
    let metrics = metrics_at_size(&config, font_size);
    for (run, text) in runs.into_iter().zip(expected) {
        assert_eq!(run_text(&plan, run), text);
        let reference = rows.iter().find(|row| row.text == text).unwrap();
        let first = &quads(&plan, run)[0];
        assert_near(
            first.center[0] - glyph_width(&metrics, text.chars().next().unwrap()) * 0.5,
            reference.x,
        );
        assert_near(first.center[1] - font_size * 0.5, reference.y);
        assert_near(first.half_extent[1] * 2.0, font_size);
    }
}

#[test]
fn gpu_multiline_sequence_messages_match_canvas_secondary_text() {
    let ir = fm_parser::parse(
        "sequenceDiagram\n participant A\n participant B\n A->>B: First<br/><br/>Last\n",
    )
    .ir;
    let config = CanvasRenderConfig {
        font_family: "monospace".to_string(),
        font_size: 20.0,
        line_height: 1.8,
        auto_fit: false,
        ..Default::default()
    };
    let layout = configured_layout(&ir, &config);
    let plan = GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
    let rows = canvas_rows(&ir, &layout, &config);
    let runs: Vec<_> = plan
        .text_runs
        .iter()
        .filter(|run| run.source == GpuTextSource::Edge)
        .collect();
    assert_eq!(runs.len(), 2);
    for (run, text) in runs.into_iter().zip(["First", "Last"]) {
        let reference = rows.iter().find(|row| row.text == text).unwrap();
        assert_centered_row(&plan, run, reference, &config, 17.0);
    }
}

#[test]
fn gpu_node_faces_follow_the_style_cascade_and_share_cells_across_sizes() {
    let ir = fm_parser::parse(
        "flowchart LR\n A[Shared]:::emphasis\n B[Shared]:::secondary\n C[Shared]\n D[Shared]:::emphasis\n \
         classDef default font-family:serif,font-weight:400\n \
         classDef emphasis font-family:monospace,font-weight:700,font-style:italic\n \
         classDef secondary font-weight:600\n \
         style B font-family:sans-serif,font-style:oblique\n style D font-size:32px\n",
    ).ir;
    assert_eq!(ir.nodes.len(), 4);
    let config = CanvasRenderConfig {
        auto_fit: false,
        ..Default::default()
    };
    let layout = configured_layout(&ir, &config);
    let plan = GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
    let canvas = canvas_rows(&ir, &layout, &config);
    let expected = [
        ("A", "monospace", "700", "italic", 15.0),
        ("B", "sans-serif", "600", "oblique", 15.0),
        ("C", "serif", "400", "normal", 15.0),
        ("D", "monospace", "700", "italic", 32.0),
    ];
    let mut ids = Vec::new();
    for (id, family, weight, style, size) in expected {
        let index = ir.nodes.iter().position(|node| node.id == id).unwrap();
        let run = plan
            .text_runs
            .iter()
            .find(|run| run.source == GpuTextSource::Node && run.node_index as usize == index)
            .unwrap();
        assert_eq!(run_text(&plan, run), "Shared");
        assert_eq!(
            plan.glyph_atlas.faces[run.face_index as usize],
            GlyphFace {
                family: family.into(),
                weight: weight.into(),
                style: style.into()
            }
        );
        let prefix = if style == "normal" {
            String::new()
        } else {
            format!("{style} ")
        };
        let font = format!("{prefix}{weight} {size}px {family}");
        assert!(
            canvas
                .iter()
                .any(|row| row.text == "Shared" && row.font == font),
            "Canvas must draw the same declared face: {font}"
        );
        assert!(
            quads(&plan, run)
                .iter()
                .all(|quad| quad.half_extent == [size as f32 * 0.5; 2])
        );
        ids.push(run.face_index);
    }
    assert_eq!(ids[0], ids[3], "size does not create another raster face");
    assert_ne!(ids[0], ids[1]);
    assert_ne!(ids[1], ids[2]);
    assert_eq!(plan.glyph_atlas.faces.len(), 3);
    assert_eq!(
        plan.glyph_atlas.cells.len(),
        "Shared".chars().count() * 3,
        "duplicate labels and unrequested default faces add no cells"
    );
    assert_eq!(
        plan,
        GpuRenderPlan::from_layout_with_config(&ir, &layout, &config)
    );
}

fn assert_face_matches_canvas(plan: &GpuRenderPlan, run: &GpuTextRun, row: &CanvasRow) {
    let (prefix, family) = row
        .font
        .split_once("px ")
        .expect("Canvas font has a pixel size");
    let mut parts: Vec<_> = prefix.split_whitespace().collect();
    let size: f32 = parts.pop().unwrap().parse().unwrap();
    let style = parts
        .iter()
        .copied()
        .find(|part| matches!(*part, "italic" | "oblique"))
        .unwrap_or("normal");
    let weight = parts
        .iter()
        .copied()
        .find(|part| !matches!(*part, "normal" | "italic" | "oblique"))
        .unwrap_or("normal");
    let face = &plan.glyph_atlas.faces[run.face_index as usize];
    assert_eq!(
        (&*face.family, &*face.weight, &*face.style),
        (family, weight, style),
        "font mismatch for {}",
        row.text
    );
    assert_eq!(run_text(plan, run), row.text);
    for quad in quads(plan, run) {
        assert_near(quad.half_extent[1] * 2.0, size);
    }
}

#[test]
fn gpu_card_faces_keep_semantic_emphasis_and_declared_component_overrides() {
    let fixtures = [
        "classDiagram\n class Repository~T~ {\n <<interface>>\n +String identifier\n +fetch() List~T~\n +save()*\n }\n",
        "erDiagram\n CUSTOMER {\n string identifier PK\n int age\n }\n",
        "C4Context\n System(api, \"Catalog\", \"Stores product data\")\n",
        "requirementDiagram\n functionalRequirement Secure {\n id: SEC-007\n text: Preserve data\n risk: high\n verifymethod: test\n }\n",
    ];
    let config = CanvasRenderConfig {
        font_family: "serif".into(),
        auto_fit: false,
        ..Default::default()
    };
    for source in fixtures {
        // Both direct parsed sources and typed IR styling are public renderer inputs. The latter
        // must override row defaults independently, without changing their measured font sizes.
        for declared in [
            None,
            Some("font-weight:400;font-style:normal;font-family:monospace"),
            Some("font-weight:800;font-style:oblique;font-size:27px"),
        ] {
            let mut ir = fm_parser::parse(source).ir;
            assert_eq!(ir.nodes.len(), 1, "fixture must isolate one card");
            if let Some(style) = declared {
                ir.nodes[0].inline_style = Some(Box::new(fm_core::parse_style_string(style)));
            }
            let layout = configured_layout(&ir, &config);
            let plan = GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
            let rows = canvas_rows(&ir, &layout, &config);
            let runs: Vec<_> = plan
                .text_runs
                .iter()
                .filter(|run| run.source == GpuTextSource::Node)
                .collect();
            assert!(
                runs.len() >= 2,
                "fixture must exercise multiple card roles: {source}"
            );
            for run in runs {
                let text = run_text(&plan, run);
                let row = rows.iter().find(|row| row.text == text).unwrap();
                assert_face_matches_canvas(&plan, run, row);
            }
            if declared.is_none() {
                assert!(
                    plan.glyph_atlas.faces.len() >= 2,
                    "semantic emphasis needs distinct faces"
                );
            } else {
                assert_eq!(
                    plan.glyph_atlas.faces.len(),
                    1,
                    "an explicit component overrides every row default"
                );
            }
        }
    }
}

#[test]
fn gpu_and_canvas_sequence_mirrors_keep_the_participants_declared_face_and_rows() {
    let mut ir = fm_parser::parse(
        "%%{init: {\"sequence\": {\"mirrorActors\": true}}}%%\n\
         sequenceDiagram\n\
         participant A as First<br/><br/>Last\n\
         participant B as Other\n\
         A->>B: Send\n",
    )
    .ir;
    assert_eq!(ir.meta.init.config.sequence_mirror_actors, Some(true));
    let participant = ir.nodes.iter().position(|node| node.id == "A").unwrap();
    ir.nodes[participant].inline_style = Some(Box::new(fm_core::parse_style_string(
        "font-size:26px;font-family:monospace;font-weight:700;font-style:italic",
    )));
    let config = CanvasRenderConfig {
        font_size: 18.0,
        line_height: 1.75,
        auto_fit: false,
        ..Default::default()
    };
    let layout = configured_layout(&ir, &config);
    assert_eq!(layout.extensions.sequence_mirror_headers.len(), 2);
    let plan = GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
    let canvas = canvas_rows(&ir, &layout, &config);
    let head: Vec<_> = plan
        .text_runs
        .iter()
        .filter(|run| run.source == GpuTextSource::Node && run.node_index as usize == participant)
        .collect();
    let mirror: Vec<_> = plan
        .text_runs
        .iter()
        .filter(|run| {
            run.source == GpuTextSource::MirrorHeader && run.node_index as usize == participant
        })
        .collect();
    assert_eq!((head.len(), mirror.len()), (2, 2));
    for (head, mirror) in head.iter().zip(&mirror) {
        assert_eq!(head.face_index, mirror.face_index);
        let text = run_text(&plan, head);
        assert_eq!(text, run_text(&plan, mirror));
        let painted: Vec<_> = canvas.iter().filter(|row| row.text == text).collect();
        assert_eq!(painted.len(), 2, "both participant rows render {text}");
        for run in [*head, *mirror] {
            let row = painted
                .iter()
                .min_by(|a, b| {
                    (a.y - quads(&plan, run)[0].center[1])
                        .abs()
                        .total_cmp(&(b.y - quads(&plan, run)[0].center[1]).abs())
                })
                .unwrap();
            assert_face_matches_canvas(&plan, run, row);
            assert_near(quads(&plan, run)[0].center[1], row.y);
        }
    }
    assert_near(
        quads(&plan, mirror[1])[0].center[1] - quads(&plan, mirror[0])[0].center[1],
        26.0 * 1.75 * 2.0,
    );
}
