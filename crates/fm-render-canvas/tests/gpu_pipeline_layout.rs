//! bd-2u0.2: every instance layout must agree with the shader that consumes it.
//!
//! A vertex layout that disagrees with its WGSL does not fail to compile, does not panic, and does
//! not error at draw time — it draws the wrong bytes reinterpreted as the right types. There is no
//! signal except output that looks subtly wrong, which is why this is asserted against the shader
//! SOURCE rather than against a hand-maintained second table.
//!
//! The shader is parsed for its vertex-input struct and each `@location(N) name: type` is matched to
//! the declared attribute BY NAME. Matching by name rather than by position is the whole point: the
//! two sides genuinely disagree on declaration order elsewhere in this crate (`GpuEdgeSegment`
//! declares `dash_phase` before `dash`, `EDGE_WGSL` binds `dash` before `dash_phase`), so a
//! positional check would certify exactly the bug this file exists to catch.

use fm_render_canvas::CanvasRenderConfig;
use fm_render_canvas::gpu_pipeline::{
    DrawBatch, PipelineDescriptor, PrimitiveFamily, VertexFormat, draw_batches, edge_pipeline,
    node_pipeline, pipelines, text_pipeline,
};

/// The stroke width the plan is built with, taken from the Canvas2D config rather than written as
/// a literal. `GpuRenderPlan::from_layout` documents that this is how a plan and a raster render of
/// the same diagram come to agree on stroke width instead of each inventing one; a hardcoded 1.5
/// here would pin the current default and stop tracking it.
fn plan_stroke_width() -> f32 {
    CanvasRenderConfig::default().edge_stroke_width as f32
}

/// `@location(N) name: type,` as declared in a WGSL struct, in source order.
fn shader_locations(wgsl: &str, struct_name: &str) -> Vec<(u32, String, String)> {
    let start = wgsl
        .find(&format!("struct {struct_name} {{"))
        .unwrap_or_else(|| panic!("shader has no `struct {struct_name}`"));
    let body = &wgsl[start..];
    let end = body.find('}').expect("unterminated struct");
    let mut out = Vec::new();
    for line in body[..end].lines() {
        let line = line.trim().trim_end_matches(',');
        let Some(rest) = line.strip_prefix("@location(") else {
            continue;
        };
        let (loc, rest) = rest.split_once(')').expect("malformed @location");
        let location: u32 = loc.trim().parse().expect("non-numeric @location");
        // `@interpolate(flat)` may sit between the location and the name; it is a fragment-stage
        // concern and never appears on a vertex INPUT, but stripping it keeps this helper usable
        // for the output struct too rather than silently mis-parsing one.
        let rest = rest.trim();
        let rest = rest.strip_prefix("@interpolate(flat)").unwrap_or(rest);
        let (name, ty) = rest.trim().split_once(':').expect("attribute has no type");
        out.push((
            location,
            name.trim().to_string(),
            ty.trim().replace(' ', ""),
        ));
    }
    out
}

/// Number of vertices the private array indexed by `vertex_index` actually contains.
///
/// This is derived from the shader source rather than from a family table. Marker rendering grew
/// from one three-vertex arrow triangle into a six-vertex quad whose fragment shader can express
/// circles, crosses and UML diamonds too. A device-only test kept asserting the obsolete triangle
/// count because no headless contract connected the descriptor to the shader array.
fn shader_vertices_per_instance(wgsl: &str) -> u32 {
    let vertex_stage = wgsl
        .split_once("@vertex")
        .map(|(_, stage)| stage)
        .expect("shader has no vertex stage");
    let indexed_line = vertex_stage
        .lines()
        .find(|line| line.contains("[vertex_index]"))
        .expect("vertex stage never indexes a private geometry array");
    let before_index = indexed_line
        .split_once("[vertex_index]")
        .map(|(before, _)| before)
        .expect("checked that the line contains vertex_index");
    let array_name = before_index
        .rsplit(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .find(|part| !part.is_empty())
        .expect("vertex array expression has no identifier");

    let declaration_prefix = format!("var<private> {array_name}: array<vec2<f32>,");
    let declaration = wgsl
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with(&declaration_prefix))
        .unwrap_or_else(|| {
            panic!("shader indexes {array_name} but never declares its vertex array")
        });
    declaration
        .strip_prefix(&declaration_prefix)
        .expect("selected the line by this prefix")
        .split_once('>')
        .map(|(count, _)| count.trim())
        .expect("vertex array declaration has no closing angle bracket")
        .parse()
        .expect("vertex array length is not an integer")
}

/// Assert one pipeline's layout against the shader that consumes it.
fn assert_layout_matches_shader(pipeline: &PipelineDescriptor, vertex_input_struct: &str) {
    let declared = shader_locations(pipeline.wgsl, vertex_input_struct);

    // NON-VACUITY. If the struct name ever changes, `shader_locations` would return an empty list
    // and every assertion below would pass over nothing.
    assert!(
        !declared.is_empty(),
        "CONTROL FAILED: parsed no @location from {vertex_input_struct}, so this proves nothing"
    );
    assert_eq!(
        declared.len(),
        pipeline.instance.attributes.len(),
        "shader declares {} inputs but the layout binds {}; an unbound input reads garbage and an \
         unused attribute is dead stride",
        declared.len(),
        pipeline.instance.attributes.len()
    );

    for (location, name, wgsl_type) in &declared {
        // BY NAME, not by index -- see the module comment. A positional pairing would accept a
        // reordering that silently rebinds every field after the first moved one.
        let attribute = pipeline
            .instance
            .attributes
            .iter()
            .find(|a| a.name == name)
            .unwrap_or_else(|| {
                panic!(
                    "shader input {name:?} is bound by no attribute in the {} layout",
                    pipeline.label
                )
            });
        assert_eq!(
            attribute.shader_location, *location,
            "attribute {name:?} binds @location({}) but the shader declares it at @location({})",
            attribute.shader_location, location
        );
        assert_eq!(
            attribute.format.wgsl_type(),
            wgsl_type,
            "attribute {name:?} is declared {} in the layout and {wgsl_type} in the shader; the \
             GPU would reinterpret the bytes rather than reject them",
            attribute.format.wgsl_type()
        );
    }
}

#[test]
fn the_node_instance_layout_matches_the_shader_that_consumes_it() {
    assert_layout_matches_shader(&node_pipeline(), "NodeInstance");
}

#[test]
fn the_edge_instance_layout_matches_the_shader_that_consumes_it() {
    assert_layout_matches_shader(&edge_pipeline(), "EdgeSegment");
}

/// Every draw must submit exactly the private geometry array its vertex shader indexes.
///
/// Too few vertices silently leave part of a primitive undrawn. Too many make `vertex_index`
/// address beyond the array. Parsing the shader is load-bearing here: asserting every descriptor
/// equals the shared Rust quad constant would stay green if the WGSL changed independently.
#[test]
fn every_pipeline_submits_exactly_its_shader_vertex_array() {
    for pipeline in pipelines() {
        let shader_count = shader_vertices_per_instance(pipeline.wgsl);
        assert_eq!(
            pipeline.vertices_per_instance, shader_count,
            "{} submits {} vertices per instance but its shader indexes a {shader_count}-element array",
            pipeline.label, pipeline.vertices_per_instance
        );
    }
}

/// THE HAZARD THIS MODULE EXISTS FOR, pinned as its own case so it cannot be lost in a loop.
///
/// `GpuEdgeSegment` declares `dash_phase` BEFORE `dash`, while `EDGE_WGSL` binds `dash` at
/// `@location(4)` and `dash_phase` at `@location(6)`. If anyone ever "simplifies" the layout by
/// walking struct fields and numbering locations in order, `dash` would receive the bytes of
/// `dash_phase` — every dotted edge would draw a pattern computed from its start distance along the
/// route, with no error from any layer. This asserts the two are genuinely crossed, so the
/// simplification cannot be made silently.
#[test]
fn the_edge_layout_binds_dash_and_dash_phase_across_the_struct_order() {
    let pipeline = edge_pipeline();
    let by_name = |name: &str| {
        *pipeline
            .instance
            .attributes
            .iter()
            .find(|a| a.name == name)
            .unwrap_or_else(|| panic!("edge layout has no {name:?} attribute"))
    };
    let dash = by_name("dash");
    let dash_phase = by_name("dash_phase");

    assert_eq!(
        dash.shader_location, 4,
        "the shader binds dash at location 4"
    );
    assert_eq!(
        dash_phase.shader_location, 6,
        "the shader binds dash_phase at location 6"
    );

    // CONTROL: if the struct is ever reordered so the two agree, this test has stopped guarding
    // anything and should be revisited rather than left passing for the wrong reason.
    assert!(
        dash_phase.offset < dash.offset,
        "GpuEdgeSegment no longer declares dash_phase before dash (offsets {} and {}); the crossed \
         binding this test guards no longer exists, so re-derive the guard rather than deleting it",
        dash_phase.offset,
        dash.offset
    );
    assert!(
        dash_phase.shader_location > dash.shader_location,
        "the shader no longer binds dash before dash_phase"
    );
}

/// Every attribute must lie inside one instance. An attribute reaching past the stride reads into
/// the NEXT instance, which produces a plausible-looking diagram shifted by one element.
#[test]
fn no_attribute_reads_past_the_instance_stride() {
    // Over EVERY pipeline, so a family added later is covered without anyone remembering to add a
    // test for it.
    for pipeline in pipelines() {
        let stride = pipeline.instance.array_stride;
        assert!(
            stride > 0,
            "CONTROL FAILED: {} has zero stride",
            pipeline.label
        );

        for attribute in pipeline.instance.attributes {
            let end = attribute.offset + attribute.format.size();
            assert!(
                end <= stride,
                "{}: attribute {:?} spans bytes {}..{end} of a {stride}-byte instance, so it reads \
                 into the next one",
                pipeline.label,
                attribute.name,
                attribute.offset
            );
        }
    }
}

/// No two attributes may overlap. Overlap means one field's bytes are delivered as two inputs, and
/// whichever the shader reads second is wrong.
#[test]
fn attributes_do_not_overlap() {
    for pipeline in pipelines() {
        let mut spans: Vec<(u64, u64, &str)> = pipeline
            .instance
            .attributes
            .iter()
            .map(|a| (a.offset, a.offset + a.format.size(), a.name))
            .collect();
        spans.sort_unstable();
        for pair in spans.windows(2) {
            let (_, previous_end, previous_name) = pair[0];
            let (start, _, name) = pair[1];
            assert!(
                previous_end <= start,
                "{}: attributes {previous_name:?} and {name:?} overlap in the instance buffer",
                pipeline.label
            );
        }
    }
}

/// Every pipeline must bind a distinct `@location` set, and locations must be unique within one.
/// A duplicate location is a validation error on a real device, but this catches it without one.
#[test]
fn no_pipeline_binds_a_location_twice() {
    for pipeline in pipelines() {
        let mut locations: Vec<u32> = pipeline
            .instance
            .attributes
            .iter()
            .map(|a| a.shader_location)
            .collect();
        locations.sort_unstable();
        let before = locations.len();
        locations.dedup();
        assert_eq!(
            locations.len(),
            before,
            "{} binds the same @location twice",
            pipeline.label
        );
    }
}

/// SAME-IR COMPARISON AGAINST THE REFERENCE ARM.
///
/// fm-render-svg is the arm the cross-engine equivalence gate compares against, so it is the right
/// reference for "did the GPU plan keep every node?". Asserting an absolute count would pin the
/// fixture; asserting AGREEMENT with SVG pins the property, and it fails if either backend starts
/// dropping nodes.
#[test]
fn the_node_batch_draws_one_instance_per_node_the_svg_backend_draws() {
    let ir = fm_parser::parse(
        "flowchart LR\n  A[Alpha] --> B[Beta]\n  B --> C{Gamma}\n  C --> D((Delta))\n",
    )
    .ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = fm_render_canvas::GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());

    let batches = draw_batches(&plan);
    let node_batch = batches
        .iter()
        .find(|b| b.family == PrimitiveFamily::Node)
        .expect("no node batch was planned for a four-node flowchart");

    assert_eq!(
        node_batch.instance_count as usize,
        ir.nodes.len(),
        "the node batch draws {} instances for {} IR nodes",
        node_batch.instance_count,
        ir.nodes.len()
    );

    // And the SVG arm must agree, which is what makes this a cross-backend check rather than a
    // restatement of the plan's own length.
    let svg = fm_render_svg::render_svg(&ir);
    let svg_nodes = svg.matches("class=\"fm-node").count();
    assert_eq!(
        node_batch.instance_count as usize, svg_nodes,
        "GPU plans {} node instances but the SVG backend emits {svg_nodes} node groups for the \
         same IR",
        node_batch.instance_count
    );

    assert_eq!(
        node_batch.vertex_count(),
        node_batch.instance_count * 6,
        "a quad is six vertices"
    );
}

/// SAME-IR COMPARISON FOR THE EDGE FAMILY, against the SVG arm.
///
/// An edge is not one segment: a routed edge becomes one instance per `points.windows(2)` pair,
/// so instance count cannot be compared to edge count. What CAN be compared is that every IR edge
/// is represented — at least one segment each, and every segment attributed to a real edge index.
/// That catches a dropped edge, which is the failure the cross-engine gate cares about, without
/// pinning the router's segment count.
#[test]
fn every_ir_edge_reaches_the_edge_batch_and_the_svg_backend() {
    let ir = fm_parser::parse(
        "flowchart LR\n  A[Alpha] --> B[Beta]\n  B -.-> C{Gamma}\n  C ==> D((Delta))\n",
    )
    .ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = fm_render_canvas::GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());

    let batches = draw_batches(&plan);
    let edge_batch = batches
        .iter()
        .find(|b| b.family == PrimitiveFamily::Edge)
        .expect("no edge batch was planned for a three-edge flowchart");

    assert!(
        edge_batch.instance_count as usize >= ir.edges.len(),
        "{} segments for {} edges: an edge reached the GPU with no segment at all",
        edge_batch.instance_count,
        ir.edges.len()
    );

    // Every IR edge is represented at least once, by index. A count-only check passes if one edge
    // contributes many segments while another contributes none.
    for index in 0..ir.edges.len() {
        let index = u32::try_from(index).expect("edge index fits u32");
        assert!(
            plan.edge_segments.iter().any(|s| s.edge_index == index),
            "edge {index} has no segment in the plan, so it would not be drawn"
        );
    }

    // THE SVG ARM, JOINED ON THE SAME KEY. `data-fm-edge-id` is the SVG's per-edge identifier and
    // `edge_index` is the plan's, so both backends are asked the same question: is every IR edge
    // attributed geometry?
    //
    // Counting `class="fm-edge"` substrings instead would be wrong twice over: each edge emits TWO
    // matching elements (a base one and a kind-modified `fm-edge-solid`/`-dashed`/`-thick`), and a
    // class is a style, not an identity. The id is the identity.
    let svg = fm_render_svg::render_svg(&ir);
    for index in 0..ir.edges.len() {
        assert!(
            svg.contains(&format!("data-fm-edge-id=\"{index}\"")),
            "the SVG backend drew no path for edge {index}, so the two backends disagree about \
             which edges exist"
        );
    }
}

/// EDGES ARE SUBMITTED BEFORE NODES. A node box must paint over the segment that terminates at it;
/// reversed, every edge would be drawn on top of the boxes it connects.
#[test]
fn the_edge_batch_is_submitted_before_the_node_batch() {
    let ir = fm_parser::parse("flowchart LR\n  A[Alpha] --> B[Beta]\n").ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = fm_render_canvas::GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());

    let families: Vec<PrimitiveFamily> = draw_batches(&plan).iter().map(|b| b.family).collect();
    let edge_at = families
        .iter()
        .position(|f| *f == PrimitiveFamily::Edge)
        .expect("no edge batch");
    let node_at = families
        .iter()
        .position(|f| *f == PrimitiveFamily::Node)
        .expect("no node batch");
    assert!(
        edge_at < node_at,
        "edges are submitted after nodes, so every edge would paint over the boxes it connects"
    );

    // The pipeline listing must agree with the batch order, or a caller building pipelines from
    // `pipelines()` and submitting batches from `draw_batches` would get two different orders.
    let pipeline_families: Vec<PrimitiveFamily> = pipelines().iter().map(|p| p.family).collect();
    assert_eq!(
        pipeline_families,
        vec![
            PrimitiveFamily::Edge,
            PrimitiveFamily::Arrowhead,
            PrimitiveFamily::Node,
            PrimitiveFamily::NodeBorder,
            PrimitiveFamily::Text
        ],
        "pipeline order must stay edges -> arrowheads -> nodes -> node borders -> text, matching \
         GpuRenderPlan's own field order: heads ride with their edges, boxes paint over both, a \
         dashed border sits on top of the box it outlines, labels over everything"
    );
}

/// The public batch plan and the device pipeline descriptions must submit the same vertex counts.
///
/// They are separate APIs and used to repeat the count independently. A descriptor/shader check
/// alone cannot protect a caller that budgets or submits from `draw_batches`, so exercise all five
/// families in one plan and join them by family rather than by array position.
#[test]
fn every_draw_batch_uses_its_pipeline_vertex_count() {
    let ir =
        fm_parser::parse("flowchart LR\n  A[Alpha] --> B[Beta]\n  style A stroke-dasharray:5 3\n")
            .ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = fm_render_canvas::GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());
    let batches = draw_batches(&plan);

    assert_eq!(
        batches.len(),
        pipelines().len(),
        "CONTROL FAILED: the fixture does not exercise every primitive family"
    );
    for batch in batches {
        let pipeline = pipelines()
            .into_iter()
            .find(|pipeline| pipeline.family == batch.family)
            .unwrap_or_else(|| panic!("draw batch {:?} has no pipeline", batch.family));
        assert_eq!(
            batch.vertices_per_instance, pipeline.vertices_per_instance,
            "draw batch {:?} submits {} vertices per instance but its pipeline submits {}",
            batch.family, batch.vertices_per_instance, pipeline.vertices_per_instance
        );
    }
}

#[test]
fn the_text_instance_layout_matches_the_shader_that_consumes_it() {
    assert_layout_matches_shader(&text_pipeline(), "TextQuad");
}

/// SAME-IR COMPARISON FOR THE TEXT FAMILY: every node label the SVG draws must reach the atlas.
///
/// Glyph quads cannot be counted against label count — one label is many quads, and the atlas packs
/// per glyph. What is comparable is COVERAGE: each label's characters must all be present in the
/// atlas plan, so a label cannot be silently dropped or truncated on the way to the GPU. That is the
/// text analogue of "no edge disappeared", and it is the property the cross-engine gate cares about.
#[test]
fn every_node_label_the_svg_draws_has_its_glyphs_in_the_atlas() {
    let ir = fm_parser::parse("flowchart LR\n  A[Alpha] --> B[Beta]\n  B --> C[Gamma]\n").ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = fm_render_canvas::GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());

    let batches = draw_batches(&plan);
    let text_batch = batches
        .iter()
        .find(|b| b.family == PrimitiveFamily::Text)
        .expect("no text batch was planned for a diagram whose every node is labelled");
    assert!(
        text_batch.instance_count > 0,
        "the text batch draws no glyph quads"
    );

    let svg = fm_render_svg::render_svg(&ir);
    for label in ["Alpha", "Beta", "Gamma"] {
        // CONTROL on the reference arm first: if the SVG does not draw the label, the GPU is not
        // required to either, and asserting against the atlas would be asserting the wrong thing.
        assert!(
            svg.contains(label),
            "CONTROL FAILED: the SVG backend never drew {label:?}, so it cannot be the reference"
        );
        for character in label.chars() {
            assert!(
                plan.glyph_atlas.cells.iter().any(|c| c.glyph == character),
                "glyph {character:?} of label {label:?} is drawn by the SVG backend but has no \
                 atlas cell, so that label would render incomplete on the GPU"
            );
        }
    }
}

/// TEXT IS SUBMITTED LAST, or a label is painted over by the box it labels.
#[test]
fn the_text_batch_is_submitted_after_the_node_batch() {
    let ir = fm_parser::parse("flowchart LR\n  A[Alpha] --> B[Beta]\n").ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = fm_render_canvas::GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());

    let families: Vec<PrimitiveFamily> = draw_batches(&plan).iter().map(|b| b.family).collect();
    let node_at = families
        .iter()
        .position(|f| *f == PrimitiveFamily::Node)
        .expect("no node batch");
    let text_at = families
        .iter()
        .position(|f| *f == PrimitiveFamily::Text)
        .expect("no text batch");
    assert!(
        node_at < text_at,
        "text is submitted before nodes, so every label would be painted over by its own box"
    );
}

/// SAME-IR COMPARISON FOR TEXT ADVANCE: the GPU lays a label out to exactly the width `fm-layout`
/// measured when it sized that label's box.
///
/// This is the check that the flat `CHAR_ADVANCE_RATIO` could never pass. `fm-core::FontMetrics` —
/// the model `fm-layout` uses for every label, and therefore what decides how wide a node box is —
/// is proportional: `font_size * avg_char_ratio * classify(c).multiplier()`, with multipliers from
/// 0.4 for `i` to 2.0. The GPU advanced a flat 0.57 per character, so a narrow label overflowed its
/// box and a wide one sat short inside it.
///
/// Equality is asserted, not proximity: `run_advance` sums the SAME terms in the SAME order as
/// `estimate_width`, so any difference means the two models have diverged rather than that floating
/// point drifted.
#[test]
fn the_gpu_run_width_equals_the_metric_layout_sized_the_box_with() {
    // Deliberately mixed widths. A label of uniform-width letters would agree under the old flat
    // advance too, and prove nothing.
    for label in ["Alpha", "iiiii", "WWWWW", "Illustration", "mix Wi lI"] {
        let metrics = fm_core::FontMetrics::new(fm_core::FontMetricsConfig {
            preset: fm_core::FontPreset::SansSerif,
            font_size: 14.0,
            ..Default::default()
        });
        let expected = metrics.estimate_width(label);
        let actual = fm_render_canvas::gpu_pipeline::run_advance_for(label, 14.0);
        assert!(
            (actual - expected).abs() < 1e-4,
            "label {label:?}: the GPU lays it out {actual} wide but layout sized its box for \
             {expected}; the two text models have diverged"
        );
    }

    // NON-VACUITY: the fixture must actually contain labels whose widths differ, or the assertion
    // above would hold for any model at all, flat included.
    let narrow = fm_render_canvas::gpu_pipeline::run_advance_for("iiiii", 14.0);
    let wide = fm_render_canvas::gpu_pipeline::run_advance_for("WWWWW", 14.0);
    assert!(
        wide > narrow * 1.5,
        "CONTROL FAILED: 'WWWWW' ({wide}) is not meaningfully wider than 'iiiii' ({narrow}), so \
         this test cannot tell a proportional model from a flat one"
    );
}

/// SAME-IR COMPARISON FOR DASHED NODE BORDERS: what the SVG expresses as `stroke-dasharray`, the GPU
/// plans as dashed border segments (bd-l3nsf).
///
/// The SDF pass cannot dash a border at all — `shape_distance` is a distance field and a dash needs
/// arc length — so a dashed node is planned with NO SDF border and its outline emitted as edge-style
/// segments, which already carry `dash` and the accumulated `dash_phase`.
///
/// Four things are asserted, and the first two are the ones that catch the plausible wrong answers:
/// the segments carry the AUTHOR'S pattern rather than some default; the node's SDF stroke is
/// suppressed so the dashes are not drawn over a solid border; the phase accumulates across corners
/// so the pattern does not restart on each side; and the SVG says `stroke-dasharray` for the same
/// node, so the two backends agree that this border is dashed at all.
#[test]
fn a_dashed_node_border_reaches_the_gpu_as_dashed_segments_and_the_svg_as_stroke_dasharray() {
    // `style <id>` rather than `:::class`. The two are NOT interchangeable here: `:::` reaches the
    // SVG as a CSS RULE (`.fm-node-user-dashy .fm-node-shape { stroke-dasharray: … }`) and never
    // enters the per-node style map the resolvers read, so a fixture using it would test nothing on
    // any raster backend. `node_dash.rs` uses `style` and `class` for the same reason.
    let source = "flowchart LR\n  A[Alpha] --> B[Beta]\n  style A stroke-dasharray:5 3\n";
    let ir = fm_parser::parse(source).ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = fm_render_canvas::GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());

    // CONTROL ON THE REFERENCE ARM FIRST: if the SVG does not dash it, the style never applied and
    // every GPU assertion below would be about a diagram with no dashed border in it.
    //
    // The PATTERN is matched, not the property name: the emitted stylesheet contains
    // `stroke-dasharray: none` as a theme default, so `contains("stroke-dasharray")` is satisfied by
    // every diagram ever rendered and would make this control worthless.
    let svg = fm_render_svg::render_svg(&ir);
    assert!(
        svg.contains("stroke-dasharray:5 3") || svg.contains("stroke-dasharray: 5 3"),
        "CONTROL FAILED: the SVG backend never emitted the `5 3` pattern, so the style did not \
         apply and this fixture cannot test dashing"
    );

    assert!(
        !plan.node_border_segments.is_empty(),
        "a dashed node planned no border segments, so its border would not be drawn at all"
    );

    // THE AUTHOR'S PATTERN, not a default. A backend that dashed everything with a house pattern
    // would satisfy "is it dashed?" and still ignore what the diagram asked for.
    for segment in &plan.node_border_segments {
        assert_eq!(
            segment.dash,
            [5.0, 3.0],
            "border segment carries {:?}, not the classDef's `5 3`",
            segment.dash
        );
        assert!(
            segment.width > 0.0,
            "a border segment with no width draws nothing"
        );
    }

    // THE PHASE ACCUMULATES. Without it every side restarts the pattern at its own corner and the
    // dashes visibly jump at all four — the exact defect `dash_phase` exists to prevent on edges.
    let phases: Vec<f32> = plan
        .node_border_segments
        .iter()
        .map(|s| s.dash_phase)
        .collect();
    assert!(
        phases.windows(2).any(|pair| pair[1] > pair[0]),
        "every border segment starts at the same dash phase {phases:?}; the pattern restarts at \
         each corner instead of marching around the border"
    );

    // THE SDF BORDER IS SUPPRESSED for the dashed node, or the dashes sit on top of a solid line.
    let dashed_node = plan
        .node_border_segments
        .first()
        .expect("checked non-empty")
        .edge_index;
    let instance = plan
        .node_instances
        .iter()
        .find(|n| n.node_index == dashed_node)
        .expect("the dashed border belongs to a planned node");
    assert_eq!(
        instance.stroke_width, 0.0,
        "the dashed node still draws an SDF border, so its dashes would lie on a solid one"
    );

    // AND THE UNDASHED NODE IS UNTOUCHED — a backend that zeroed every border would pass the
    // assertion above while erasing every solid outline in the diagram.
    let plain = plan
        .node_instances
        .iter()
        .find(|n| n.node_index != dashed_node)
        .expect("the fixture has a second, undashed node");
    assert!(
        plain.stroke_width > 0.0,
        "the undashed node lost its border too; borders are being zeroed unconditionally"
    );
}

/// A diagram with no `stroke-dasharray` must plan NO border segments, or the test above would pass
/// on a backend that emitted them for everything.
#[test]
fn an_undashed_diagram_plans_no_node_border_segments() {
    let ir = fm_parser::parse("flowchart LR\n  A[Alpha] --> B[Beta]\n").ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = fm_render_canvas::GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());
    assert!(
        plan.node_border_segments.is_empty(),
        "an undashed diagram planned {} border segments",
        plan.node_border_segments.len()
    );
    assert!(
        plan.node_instances.iter().all(|n| n.stroke_width > 0.0),
        "an undashed node lost its SDF border"
    );
}

/// A diagram with no nodes must plan no batch, or the test above would pass on a backend that
/// always emitted one.
#[test]
fn an_empty_plan_draws_nothing() {
    let ir = fm_parser::parse("flowchart LR\n").ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = fm_render_canvas::GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());
    assert!(
        plan.node_instances.is_empty(),
        "CONTROL FAILED: an empty flowchart planned {} nodes, so this cannot show the pass is \
         inert",
        plan.node_instances.len()
    );
    assert_eq!(draw_batches(&plan), Vec::<DrawBatch>::new());
}

/// Sizes must match what WGSL requires, independent of the shaders — a wrong size here would move
/// every offset after it.
#[test]
fn vertex_format_sizes_are_the_wgsl_sizes() {
    assert_eq!(VertexFormat::Float32.size(), 4);
    assert_eq!(VertexFormat::Uint32.size(), 4);
    assert_eq!(VertexFormat::Float32x2.size(), 8);
    assert_eq!(VertexFormat::Float32x4.size(), 16);
}
