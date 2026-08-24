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

    assert_eq!(dash.shader_location, 4, "the shader binds dash at location 4");
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
        assert!(stride > 0, "CONTROL FAILED: {} has zero stride", pipeline.label);

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
            PrimitiveFamily::Node,
            PrimitiveFamily::Text
        ],
        "pipeline order must stay edges -> nodes -> text: boxes over their edges, labels over \
         their boxes"
    );
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
