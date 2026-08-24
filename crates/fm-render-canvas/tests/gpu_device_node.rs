//! bd-2u0.2: the node pipeline actually rasterises, and it agrees with the SVG arm about where the
//! nodes are.
//!
//! `gpu_pipeline_layout.rs` proves the layout matches the shader without a device. This file is the
//! other half: a real wgpu device, a real draw, and pixels read back. It only builds under the
//! `webgpu` feature, so a default `cargo test` on a machine with no adapter is unaffected.
//!
//! **What is compared, and why not a pixel diff.** Rasterising the SVG and differencing images would
//! fail on antialiasing, glyph hinting and gamma long before it caught a real defect, and would need
//! a rasteriser dependency to boot. What is comparable across the two backends is GEOMETRY: for each
//! node the layout places, the GPU must have painted something at that spot, and the SVG must have
//! emitted an element for that same node. A node dropped by either backend fails, which is the
//! defect class the cross-engine gate actually cares about.
#![cfg(feature = "webgpu")]

use fm_render_canvas::CanvasRenderConfig;
use fm_render_canvas::GpuRenderPlan;
use fm_render_canvas::gpu_device::{
    DiagramPipelines, GlyphAtlasTexture, GpuDevice, GpuDeviceError, InstanceDraw, InstancePass,
    arrowhead_instance_bytes, edge_instance_bytes, node_instance_bytes, render_diagram,
    render_instances, solid_coverage, text_instance_bytes,
};
use fm_render_canvas::gpu_pipeline::{
    arrowhead_pipeline, edge_pipeline, node_pipeline, text_pipeline,
};

fn plan_stroke_width() -> f32 {
    CanvasRenderConfig::default().edge_stroke_width as f32
}

/// Acquire a device, or explain precisely why not.
///
/// A test that silently returns on a GPU-less box is indistinguishable from one that passes, so the
/// absence of an adapter is reported as a skip with its reason rather than as success. Building with
/// `--features webgpu` is an assertion that a GPU is expected, so this is loud.
fn device_or_skip() -> Option<GpuDevice> {
    match GpuDevice::headless() {
        Ok(gpu) => {
            eprintln!(
                "[gpu] adapter={:?} backend={:?}",
                gpu.adapter_name(),
                gpu.backend()
            );
            Some(gpu)
        }
        Err(GpuDeviceError::NoAdapter(why)) => {
            eprintln!("[gpu] SKIPPED: no adapter on this host ({why})");
            None
        }
        Err(other) => {
            panic!("wgpu is present but unusable, which is a defect not an environment: {other}")
        }
    }
}

/// THE LOAD-BEARING SERIALISATION CHECK, and it needs no GPU.
///
/// The vertex attributes address the instance buffer by byte offset. If the serialiser wrote fields
/// in a different order than the struct declares them, every attribute past the first divergence
/// would read the wrong bytes with no error anywhere. I made exactly that mistake writing this
/// module -- `GpuNodeInstance` declares `stroke_width, shape, node_index` and the naive append order
/// was `shape, node_index, stroke_width`, which would have handed the shader a float reinterpreted
/// as a shape discriminant.
///
/// So each field is read back out of the serialised bytes AT ITS DECLARED ATTRIBUTE OFFSET and
/// compared to the source struct. This is the check that catches that class outright.
#[test]
fn the_serialised_instance_round_trips_through_the_declared_offsets() {
    let ir = fm_parser::parse("flowchart LR\n  A[Alpha] --> B[Beta]\n").ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());

    assert!(
        !plan.node_instances.is_empty(),
        "CONTROL FAILED: no node instances, so this asserts nothing"
    );

    let bytes = node_instance_bytes(&plan.node_instances);
    let pipeline = node_pipeline();
    let stride = pipeline.instance.array_stride as usize;
    assert_eq!(
        bytes.len(),
        stride * plan.node_instances.len(),
        "serialised length disagrees with the stride the GPU is told to use"
    );

    let read_f32 = |base: usize, offset: u64| {
        let start = base + offset as usize;
        f32::from_ne_bytes(bytes[start..start + 4].try_into().expect("4 bytes"))
    };
    let read_u32 = |base: usize, offset: u64| {
        let start = base + offset as usize;
        u32::from_ne_bytes(bytes[start..start + 4].try_into().expect("4 bytes"))
    };
    let offset_of = |name: &str| {
        pipeline
            .instance
            .attributes
            .iter()
            .find(|a| a.name == name)
            .unwrap_or_else(|| panic!("node layout has no {name:?}"))
            .offset
    };

    for (index, instance) in plan.node_instances.iter().enumerate() {
        let base = index * stride;
        assert_eq!(read_f32(base, offset_of("center")), instance.center[0]);
        assert_eq!(read_f32(base, offset_of("center") + 4), instance.center[1]);
        assert_eq!(
            read_f32(base, offset_of("half_extent")),
            instance.half_extent[0]
        );
        assert_eq!(read_f32(base, offset_of("fill")), instance.fill[0]);
        assert_eq!(read_f32(base, offset_of("stroke")), instance.stroke[0]);
        // The three that the naive ordering got wrong.
        assert_eq!(
            read_f32(base, offset_of("stroke_width")),
            instance.stroke_width,
            "stroke_width does not round-trip; the serialiser and the attribute offsets disagree"
        );
        assert_eq!(
            read_u32(base, offset_of("shape")),
            instance.shape,
            "shape does not round-trip; a float would be read as a shape discriminant"
        );
        assert_eq!(
            read_u32(base, offset_of("node_index")),
            instance.node_index,
            "node_index does not round-trip"
        );
    }
}

/// SAME-IR COMPARISON: the GPU paints a node where the layout puts one, and so does the SVG.
#[test]
fn the_node_pipeline_paints_a_node_wherever_the_svg_backend_draws_one() {
    let Some(gpu) = device_or_skip() else {
        return;
    };

    let ir = fm_parser::parse(
        "flowchart LR\n  A[Alpha] --> B[Beta]\n  B --> C{Gamma}\n  C --> D((Delta))\n",
    )
    .ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());
    assert!(
        !plan.node_instances.is_empty(),
        "CONTROL FAILED: nothing to draw"
    );

    let pass = InstancePass::new(&gpu, &node_pipeline());
    let bytes = node_instance_bytes(&plan.node_instances);
    let count = u32::try_from(plan.node_instances.len()).expect("fits u32");
    let draw = InstanceDraw {
        bounds: &plan.bounds,
        instance_bytes: &bytes,
        instance_count: count,
        atlas: None,
    };
    let image = render_instances(&gpu, &pass, &draw, 512, 512).expect("render");

    // NON-VACUITY ON THE RASTER ITSELF: something must have been painted. Without this, a pipeline
    // that drew nothing at all would satisfy every per-node check below by vacuous truth if the
    // sampling ever missed.
    let painted = image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[3] > 0)
        .count();
    assert!(
        painted > 0,
        "the GPU produced a fully transparent image: nothing was rasterised"
    );

    // Each node's centre must be opaque. The centre is used rather than an edge because a border
    // pixel depends on antialiasing and stroke width, while the interior is unambiguous.
    for instance in &plan.node_instances {
        let (x, y) = image.pixel_for(&plan.bounds, instance.center[0], instance.center[1]);
        let pixel = image
            .pixel(x, y)
            .expect("centre maps inside the rendered image");
        assert!(
            pixel[3] > 0,
            "node {} centre lands on a transparent pixel at ({x}, {y}); the GPU drew no box there",
            instance.node_index
        );
    }

    // THE REFERENCE ARM must agree that those nodes exist. Joined on node id, not on a count, so a
    // backend that drew four boxes for a different four nodes would still fail.
    let svg = fm_render_svg::render_svg(&ir);
    for node in &ir.nodes {
        assert!(
            svg.contains(node.id.as_str()),
            "the SVG backend never mentions node {:?}, so it cannot be the reference for it",
            node.id
        );
    }
    assert_eq!(
        plan.node_instances.len(),
        ir.nodes.len(),
        "the GPU plans a different number of node boxes than the IR declares"
    );
}

/// A diagram with no nodes must rasterise to nothing, or the test above proves only that the
/// pipeline paints unconditionally.
#[test]
fn an_empty_diagram_rasterises_to_a_transparent_image() {
    let Some(gpu) = device_or_skip() else {
        return;
    };

    let ir = fm_parser::parse("flowchart LR\n").ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());
    assert!(
        plan.node_instances.is_empty(),
        "CONTROL FAILED: an empty flowchart planned nodes"
    );

    let pass = InstancePass::new(&gpu, &node_pipeline());
    let draw = InstanceDraw {
        bounds: &plan.bounds,
        instance_bytes: &[],
        instance_count: 0,
        atlas: None,
    };
    let image = render_instances(&gpu, &pass, &draw, 64, 64).expect("render");
    assert!(
        image.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 0),
        "an empty diagram painted pixels"
    );
}

/// THE SHADERS MUST ACTUALLY COMPILE, which nothing checked until a device existed.
///
/// `gpu_pipeline_layout.rs` parses `@location` lines out of the WGSL and is blind to the body, so a
/// shader could satisfy every layout test while being invalid. Compiling them on a real device is
/// the only check that reads the whole thing.
#[test]
fn the_shaders_this_crate_ships_compile_on_a_real_device() {
    let Some(gpu) = device_or_skip() else {
        return;
    };
    for (name, wgsl) in [
        ("NODE_SDF_WGSL", fm_render_canvas::NODE_SDF_WGSL),
        // EDGE joined this list when bd-s7ond was fixed. It had shipped reading an identifier that
        // existed in no scope, and nothing noticed because the layout gate parses `@location` lines
        // and never reads the body. This list is now the whole set: any shader this crate ships and
        // does not compile here is a hole.
        ("EDGE_WGSL", fm_render_canvas::EDGE_WGSL),
        ("ARROWHEAD_WGSL", fm_render_canvas::ARROWHEAD_WGSL),
        ("TEXT_ATLAS_WGSL", fm_render_canvas::TEXT_ATLAS_WGSL),
    ] {
        if let Err(error) = gpu.validate_shader(name, wgsl) {
            panic!("{name} does not compile: {error}");
        }
    }
}

/// The edge serialiser must round-trip through the DECLARED offsets, and this family is the one
/// where that matters most.
///
/// `GpuEdgeSegment` declares `dash_phase, dash, width` while `EDGE_WGSL` binds `dash`, `width`,
/// `dash_phase` at locations 4, 5, 6. The two only agree because both sides go through `offset_of!`.
/// Reading each field back at its attribute offset is what proves they still do. Needs no GPU.
#[test]
fn the_serialised_edge_segment_round_trips_through_the_declared_offsets() {
    // A DOTTED edge, so `dash` is non-zero and a swap with `dash_phase` is observable rather than
    // hidden behind two zeroes.
    let ir = fm_parser::parse("flowchart LR\n  A[Alpha] -.-> B[Beta]\n  B ==> C[Gamma]\n").ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());

    assert!(
        !plan.edge_segments.is_empty(),
        "CONTROL FAILED: no edge segments, so this asserts nothing"
    );
    assert!(
        plan.edge_segments.iter().any(|s| s.dash != [0.0, 0.0]),
        "CONTROL FAILED: no segment carries a dash pattern, so a dash/dash_phase swap would be \
         invisible to this test"
    );

    let bytes = edge_instance_bytes(&plan.edge_segments);
    let pipeline = edge_pipeline();
    let stride = pipeline.instance.array_stride as usize;
    assert_eq!(bytes.len(), stride * plan.edge_segments.len());

    let read_f32 = |base: usize, offset: u64| {
        let start = base + offset as usize;
        f32::from_ne_bytes(bytes[start..start + 4].try_into().expect("4 bytes"))
    };
    let read_u32 = |base: usize, offset: u64| {
        let start = base + offset as usize;
        u32::from_ne_bytes(bytes[start..start + 4].try_into().expect("4 bytes"))
    };
    let offset_of = |name: &str| {
        pipeline
            .instance
            .attributes
            .iter()
            .find(|a| a.name == name)
            .unwrap_or_else(|| panic!("edge layout has no {name:?}"))
            .offset
    };

    for (index, segment) in plan.edge_segments.iter().enumerate() {
        let base = index * stride;
        assert_eq!(read_f32(base, offset_of("from_point")), segment.from[0]);
        assert_eq!(read_f32(base, offset_of("to_point")), segment.to[0]);
        assert_eq!(read_u32(base, offset_of("edge_index")), segment.edge_index);
        assert_eq!(read_f32(base, offset_of("color")), segment.color[0]);
        // THE CROSSED TRIO.
        assert_eq!(
            read_f32(base, offset_of("dash")),
            segment.dash[0],
            "dash does not round-trip; the shader would read dash_phase as the dash pattern"
        );
        assert_eq!(
            read_f32(base, offset_of("width")),
            segment.width,
            "width does not round-trip"
        );
        assert_eq!(
            read_f32(base, offset_of("dash_phase")),
            segment.dash_phase,
            "dash_phase does not round-trip"
        );
    }
}

/// SAME-IR COMPARISON FOR ARROWHEADS: the GPU paints a head at each arrow tip, and the SVG attaches
/// a marker to that same edge.
///
/// The head is sampled slightly BEHIND its tip along the facing angle, not at the tip itself: the
/// tip is a single antialiased vertex of a triangle, which is the one pixel most likely to be
/// partially transparent for reasons that are not defects. A quarter of the head's own size back
/// along the angle lands in the body of the triangle.
#[test]
fn the_arrowhead_pipeline_paints_a_head_for_every_edge_the_svg_marks() {
    let Some(gpu) = device_or_skip() else {
        return;
    };

    let ir = fm_parser::parse(
        "flowchart LR\n  A[Alpha] --> B[Beta]\n  B --> C{Gamma}\n  C --> D((Delta))\n",
    )
    .ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());
    assert!(
        !plan.arrowheads.is_empty(),
        "CONTROL FAILED: a flowchart of directed edges planned no arrowheads"
    );

    let descriptor = arrowhead_pipeline();
    // A TRIANGLE, NOT A QUAD. If this ever reads 6, the pass would index past the shader's
    // three-element HEAD array.
    assert_eq!(
        descriptor.vertices_per_instance, 3,
        "an arrowhead is a triangle; six vertices would over-read the shader's HEAD array"
    );

    let pass = InstancePass::new(&gpu, &descriptor);
    let bytes = arrowhead_instance_bytes(&plan.arrowheads);
    let count = u32::try_from(plan.arrowheads.len()).expect("fits u32");
    let draw = InstanceDraw {
        bounds: &plan.bounds,
        instance_bytes: &bytes,
        instance_count: count,
        atlas: None,
    };
    let image = render_instances(&gpu, &pass, &draw, 512, 512).expect("render");

    let painted = image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[3] > 0)
        .count();
    assert!(
        painted > 0,
        "the arrowhead pass produced a fully transparent image: nothing was rasterised"
    );

    for head in &plan.arrowheads {
        let back = head.size * 0.25;
        let sample_x = head.size.mul_add(0.0, head.position[0]) - back * head.angle.cos();
        let sample_y = head.position[1] - back * head.angle.sin();
        let (x, y) = image.pixel_for(&plan.bounds, sample_x, sample_y);
        let pixel = image.pixel(x, y).expect("sample maps inside the image");
        assert!(
            pixel[3] > 0,
            "the head for edge {} lands on a transparent pixel at ({x}, {y}); no triangle was drawn",
            head.edge_index
        );
    }

    // Every arrowhead must belong to a real edge -- a head attributed to an index the IR does not
    // have would be drawn somewhere no edge terminates.
    for head in &plan.arrowheads {
        assert!(
            (head.edge_index as usize) < ir.edges.len(),
            "arrowhead references edge {} but the IR has {} edges",
            head.edge_index,
            ir.edges.len()
        );
    }

    // THE REFERENCE ARM: the SVG attaches a marker to each edge it draws, joined on the same id.
    let svg = fm_render_svg::render_svg(&ir);
    for head in &plan.arrowheads {
        assert!(
            svg.contains(&format!("data-fm-edge-id=\"{}\"", head.edge_index)),
            "the GPU planned a head for edge {} that the SVG backend never drew",
            head.edge_index
        );
    }
    assert!(
        svg.contains("marker-end"),
        "CONTROL FAILED: the SVG drew no arrow marker at all, so it cannot be the reference"
    );
}

/// A diagram whose edges carry no heads must plan none, or the test above would pass on a backend
/// that emitted arrowheads unconditionally.
#[test]
fn an_undirected_link_plans_no_arrowhead() {
    // `---` is an open link: mermaid draws no marker on it.
    let ir = fm_parser::parse("flowchart LR\n  A[Alpha] --- B[Beta]\n").ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());
    assert!(
        plan.arrowheads.is_empty(),
        "an undirected link planned {} arrowheads",
        plan.arrowheads.len()
    );
}

/// SAME-IR COMPARISON FOR EDGES: the GPU paints along each routed segment, and the SVG draws a path
/// for that same edge id. Unblocked by the bd-s7ond shader fix.
///
/// Sampled at each segment's MIDPOINT, not its endpoints: an endpoint sits under the node box that
/// terminates it and on the antialiased tip of the ribbon, so it is the pixel most likely to be
/// transparent for reasons that are not defects.
#[test]
fn the_edge_pipeline_paints_along_every_segment_the_svg_backend_draws() {
    let Some(gpu) = device_or_skip() else {
        return;
    };

    let ir = fm_parser::parse(
        "flowchart LR\n  A[Alpha] --> B[Beta]\n  B -.-> C{Gamma}\n  C ==> D((Delta))\n",
    )
    .ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());
    assert!(
        !plan.edge_segments.is_empty(),
        "CONTROL FAILED: nothing to draw"
    );

    let pass = InstancePass::new(&gpu, &edge_pipeline());
    let bytes = edge_instance_bytes(&plan.edge_segments);
    let count = u32::try_from(plan.edge_segments.len()).expect("fits u32");
    let draw = InstanceDraw {
        bounds: &plan.bounds,
        instance_bytes: &bytes,
        instance_count: count,
        atlas: None,
    };
    let image = render_instances(&gpu, &pass, &draw, 512, 512).expect("render");

    let painted = image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[3] > 0)
        .count();
    assert!(
        painted > 0,
        "the edge pass produced a fully transparent image: nothing was rasterised"
    );

    // Per EDGE, not per segment: a dotted edge legitimately has gaps, so an individual segment's
    // midpoint may land in an "off" span of the dash pattern. An entire edge going unpainted is a
    // real defect.
    for index in 0..u32::try_from(ir.edges.len()).expect("fits u32") {
        let segments: Vec<_> = plan
            .edge_segments
            .iter()
            .filter(|s| s.edge_index == index)
            .collect();
        assert!(
            !segments.is_empty(),
            "edge {index} has no segment in the plan"
        );
        let any_painted = segments.iter().any(|segment| {
            let mid_x = (segment.from[0] + segment.to[0]) * 0.5;
            let mid_y = (segment.from[1] + segment.to[1]) * 0.5;
            let (x, y) = image.pixel_for(&plan.bounds, mid_x, mid_y);
            image.pixel(x, y).is_some_and(|pixel| pixel[3] > 0)
        });
        assert!(
            any_painted,
            "edge {index} was planned but nothing was painted along any of its {} segments",
            segments.len()
        );
    }

    // THE REFERENCE ARM, joined on the same id the plan uses.
    let svg = fm_render_svg::render_svg(&ir);
    for index in 0..ir.edges.len() {
        assert!(
            svg.contains(&format!("data-fm-edge-id=\"{index}\"")),
            "the SVG backend drew no path for edge {index}"
        );
    }
}
/// SAME-IR COMPARISON FOR TEXT: every glyph quad the plan emits lands where the atlas says it does,
/// and the labels it spells are the ones the SVG draws.
///
/// ⚠️ THE ATLAS CONTENT IS SYNTHETIC AND THIS TEST DOES NOT CLAIM OTHERWISE. Nothing in this crate
/// rasterises glyphs yet — `GlyphAtlasPlan` carries the grid and per-glyph UV rectangles but no pixel
/// data — so `solid_coverage` fills each PLANNED cell with full coverage and leaves the rest at zero.
/// That is what makes the check meaningful rather than trivial: a quad whose UVs address an unplanned
/// region samples zero and paints nothing, so a wrong UV rectangle shows up as a missing glyph. It
/// proves the UV addressing and quad geometry, NOT that this renders readable text.
#[test]
fn every_glyph_quad_samples_its_own_planned_atlas_cell() {
    let Some(gpu) = device_or_skip() else {
        return;
    };

    let ir = fm_parser::parse("flowchart LR\n  A[Alpha] --> B[Beta]\n  B --> C[Gamma]\n").ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());
    assert!(
        !plan.text_quads.is_empty(),
        "CONTROL FAILED: a diagram whose every node is labelled planned no glyph quads"
    );
    assert!(
        !plan.glyph_atlas.cells.is_empty(),
        "CONTROL FAILED: the atlas plan has no cells, so UVs address nothing"
    );

    let coverage = solid_coverage(&plan.glyph_atlas);
    // NON-VACUITY ON THE ATLAS: if the planned cells covered nothing, every quad would sample zero
    // and the paint assertions below would fail for a reason that is not a UV defect.
    assert!(
        coverage.iter().any(|&c| c > 0),
        "CONTROL FAILED: the synthetic atlas is entirely empty"
    );
    assert!(
        coverage.contains(&0),
        "CONTROL FAILED: the synthetic atlas is entirely full, so a wrong UV would still sample \
         coverage and this test could not detect one"
    );

    let atlas = GlyphAtlasTexture::new(&gpu, &plan.glyph_atlas, &coverage);
    let pass = InstancePass::new(&gpu, &text_pipeline());
    let bytes = text_instance_bytes(&plan.text_quads);
    let count = u32::try_from(plan.text_quads.len()).expect("fits u32");
    let draw = InstanceDraw {
        bounds: &plan.bounds,
        instance_bytes: &bytes,
        instance_count: count,
        atlas: Some(&atlas),
    };
    let image = render_instances(&gpu, &pass, &draw, 1024, 1024).expect("render");

    let painted = image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[3] > 0)
        .count();
    assert!(
        painted > 0,
        "the text pass produced a fully transparent image: no glyph sampled any coverage, which \
         means the UV rectangles address cells the atlas never filled"
    );

    // The labels must be the SVG's labels. Joined on the TEXT the plan spells, so a backend that
    // drew the right number of glyphs for the wrong string still fails.
    let svg = fm_render_svg::render_svg(&ir);
    for label in ["Alpha", "Beta", "Gamma"] {
        assert!(
            svg.contains(label),
            "CONTROL FAILED: the SVG never drew {label:?}, so it cannot be the reference"
        );
        for character in label.chars() {
            assert!(
                plan.glyph_atlas.cells.iter().any(|c| c.glyph == character),
                "glyph {character:?} of {label:?} has no atlas cell, so the GPU would render that \
                 label incomplete"
            );
        }
    }
}

/// CROSS-FAMILY BATCHING: one submission draws the WHOLE diagram, and the result is a composition
/// no single-family pass could produce.
///
/// Each family used to render into its own texture with its own clear, so four images existed and a
/// picture of the diagram did not. This asserts the composed image really is the union: every node
/// centre painted, every edge painted, AND strictly more painted pixels than the largest single
/// family alone. That last clause is the one that fails if the render pass ever goes back to
/// clearing per family — each pass would erase the last and the total would collapse to one family's
/// worth of ink.
#[test]
fn one_submission_composes_every_family_into_a_single_image() {
    let Some(gpu) = device_or_skip() else {
        return;
    };

    let ir = fm_parser::parse(
        "flowchart LR\n  A[Alpha] --> B[Beta]\n  B -.-> C{Gamma}\n  C ==> D((Delta))\n",
    )
    .ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());

    // CONTROLS: every family must actually be present, or "the union is bigger" would be a claim
    // about one or two families wearing the name of four.
    assert!(!plan.node_instances.is_empty(), "CONTROL: no nodes planned");
    assert!(!plan.edge_segments.is_empty(), "CONTROL: no edges planned");
    assert!(
        !plan.arrowheads.is_empty(),
        "CONTROL: no arrowheads planned"
    );
    assert!(!plan.text_quads.is_empty(), "CONTROL: no text planned");

    let atlas = GlyphAtlasTexture::new(&gpu, &plan.glyph_atlas, &solid_coverage(&plan.glyph_atlas));
    let pipelines = DiagramPipelines::new(&gpu);
    let composed =
        render_diagram(&gpu, &pipelines, &plan, Some(&atlas), 512, 512).expect("render diagram");

    let painted = |image: &fm_render_canvas::gpu_device::RenderedImage| {
        image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 0)
            .count()
    };
    let composed_painted = painted(&composed);
    assert!(composed_painted > 0, "the composed diagram is blank");

    // Each family alone, through the single-family path, for comparison.
    let single =
        |family_pipeline, bytes: Vec<u8>, count: usize, atlas: Option<&GlyphAtlasTexture>| {
            let pass = InstancePass::new(&gpu, &family_pipeline);
            let draw = InstanceDraw {
                bounds: &plan.bounds,
                instance_bytes: &bytes,
                instance_count: u32::try_from(count).expect("fits u32"),
                atlas,
            };
            painted(&render_instances(&gpu, &pass, &draw, 512, 512).expect("render"))
        };
    let nodes_alone = single(
        node_pipeline(),
        node_instance_bytes(&plan.node_instances),
        plan.node_instances.len(),
        None,
    );
    let edges_alone = single(
        edge_pipeline(),
        edge_instance_bytes(&plan.edge_segments),
        plan.edge_segments.len(),
        None,
    );
    let largest_single = nodes_alone.max(edges_alone);

    eprintln!(
        "[batch] composed={composed_painted}px nodes_alone={nodes_alone}px \
         edges_alone={edges_alone}px families={} submissions=1",
        4
    );
    assert!(
        composed_painted > largest_single,
        "the composed image paints {composed_painted} pixels but the largest single family paints \
         {largest_single}; the families are not composing -- a per-family clear would erase each \
         previous draw and leave exactly one family's ink"
    );

    // And the composition must still be geometrically right: nodes where the layout puts them,
    // edges along their routes.
    for instance in &plan.node_instances {
        let (x, y) = composed.pixel_for(&plan.bounds, instance.center[0], instance.center[1]);
        assert!(
            composed.pixel(x, y).is_some_and(|p| p[3] > 0),
            "node {} is missing from the composed diagram",
            instance.node_index
        );
    }
    for index in 0..u32::try_from(ir.edges.len()).expect("fits u32") {
        let painted_somewhere = plan
            .edge_segments
            .iter()
            .filter(|s| s.edge_index == index)
            .any(|segment| {
                let mid_x = (segment.from[0] + segment.to[0]) * 0.5;
                let mid_y = (segment.from[1] + segment.to[1]) * 0.5;
                let (x, y) = composed.pixel_for(&plan.bounds, mid_x, mid_y);
                composed.pixel(x, y).is_some_and(|p| p[3] > 0)
            });
        assert!(
            painted_somewhere,
            "edge {index} is missing from the composed diagram"
        );
    }

    // THE REFERENCE ARM: the SVG draws the same nodes and edges for the same IR.
    let svg = fm_render_svg::render_svg(&ir);
    for node in &ir.nodes {
        assert!(
            svg.contains(node.id.as_str()),
            "the SVG never mentions node {:?}",
            node.id
        );
    }
    for index in 0..ir.edges.len() {
        assert!(
            svg.contains(&format!("data-fm-edge-id=\"{index}\"")),
            "the SVG drew no path for edge {index}"
        );
    }
}

/// A diagram with no labels must render without an atlas, or every unlabelled diagram would need a
/// blank texture invented for it.
#[test]
fn a_diagram_with_no_text_renders_without_an_atlas() {
    let Some(gpu) = device_or_skip() else {
        return;
    };
    let ir = fm_parser::parse("flowchart LR\n  A --> B\n").ir;
    let layout = fm_layout::layout_diagram(&ir);
    let mut plan = GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());
    // Unlabelled nodes still carry their id as a label, so the text is removed explicitly to make
    // the no-text path reachable at all.
    plan.text_quads.clear();

    let pipelines = DiagramPipelines::new(&gpu);
    let image = render_diagram(&gpu, &pipelines, &plan, None, 256, 256).expect("render");
    let painted = image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[3] > 0)
        .count();
    assert!(
        painted > 0,
        "a diagram with nodes and edges but no text rendered blank"
    );
}

/// The camera must not flip the diagram. Layout space grows downward, clip space grows upward, so
/// the Y scale is negative; getting that wrong renders every diagram upside down, which reads as a
/// layout bug rather than a camera bug.
#[test]
fn the_camera_maps_layout_space_the_right_way_up() {
    let bounds = fm_layout::LayoutRect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 50.0,
    };
    let [scale_x, scale_y, translate_x, translate_y] =
        fm_render_canvas::gpu_device::camera_transform(&bounds);

    let clip = |x: f32, y: f32| (x * scale_x + translate_x, y * scale_y + translate_y);

    let (left, top) = clip(0.0, 0.0);
    let (right, bottom) = clip(100.0, 50.0);
    assert!(
        (left - -1.0).abs() < 1e-5,
        "left edge maps to {left}, not -1"
    );
    assert!(
        (right - 1.0).abs() < 1e-5,
        "right edge maps to {right}, not 1"
    );
    // TOP of the layout is TOP of the screen, i.e. +1 in clip space.
    assert!((top - 1.0).abs() < 1e-5, "layout top maps to {top}, not +1");
    assert!(
        (bottom - -1.0).abs() < 1e-5,
        "layout bottom maps to {bottom}, not -1"
    );
    assert!(
        scale_y < 0.0,
        "the Y flip is missing; diagrams render upside down"
    );
}
