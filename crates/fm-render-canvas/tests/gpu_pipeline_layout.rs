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
    DrawBatch, PrimitiveFamily, VertexFormat, draw_batches, node_pipeline,
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

/// The load-bearing test: layout and shader must agree on location, name and type.
#[test]
fn the_node_instance_layout_matches_the_shader_that_consumes_it() {
    let pipeline = node_pipeline();
    let declared = shader_locations(pipeline.wgsl, "NodeInstance");

    // NON-VACUITY. If the struct name ever changes, `shader_locations` would return an empty list
    // and every assertion below would pass over nothing.
    assert!(
        !declared.is_empty(),
        "CONTROL FAILED: parsed no @location from NodeInstance, so this test proves nothing"
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
                panic!("shader input {name:?} is bound by no attribute in the node layout")
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

/// Every attribute must lie inside one instance. An attribute reaching past the stride reads into
/// the NEXT instance, which produces a plausible-looking diagram shifted by one element.
#[test]
fn no_node_attribute_reads_past_the_instance_stride() {
    let pipeline = node_pipeline();
    let stride = pipeline.instance.array_stride;
    assert!(stride > 0, "CONTROL FAILED: zero stride");

    for attribute in pipeline.instance.attributes {
        let end = attribute.offset + attribute.format.size();
        assert!(
            end <= stride,
            "attribute {:?} spans bytes {}..{} of a {stride}-byte instance, so it reads into the \
             next one",
            attribute.name,
            attribute.offset,
            end
        );
    }
}

/// No two attributes may overlap. Overlap means one field's bytes are delivered as two inputs, and
/// whichever the shader reads second is wrong.
#[test]
fn node_attributes_do_not_overlap() {
    let pipeline = node_pipeline();
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
            "attributes {previous_name:?} and {name:?} overlap in the instance buffer"
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
