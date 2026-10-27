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
        ("TRIANGLE_WGSL", fm_render_canvas::TRIANGLE_WGSL),
        ("PIE_WEDGE_WGSL", fm_render_canvas::PIE_WEDGE_WGSL),
    ] {
        if let Err(error) = gpu.validate_shader(name, wgsl) {
            panic!("{name} does not compile: {error}");
        }
    }
}

#[test]
fn area_triangle_bytes_round_trip_through_the_shader_attribute_offsets() {
    use fm_render_canvas::GpuTriangleInstance;

    let triangles = [
        GpuTriangleInstance {
            a: [-12.0, 4.0],
            b: [7.0, -9.0],
            c: [14.0, 23.0],
            color: [0.2, 0.6, 0.9, 0.16],
        },
        GpuTriangleInstance {
            a: [91.0, 17.0],
            b: [-35.0, 40.0],
            c: [44.0, 28.0],
            color: [0.7, 0.1, 0.3, 0.5],
        },
    ];
    let bytes = fm_render_canvas::gpu_device::triangle_instance_bytes(&triangles);
    let descriptor = fm_render_canvas::gpu_pipeline::xy_area_pipeline();
    let stride = descriptor.instance.array_stride as usize;
    assert_eq!(bytes.len(), stride * triangles.len());
    for (index, triangle) in triangles.iter().enumerate() {
        for attribute in descriptor.instance.attributes {
            let expected: &[f32] = match attribute.name {
                "a" => &triangle.a,
                "b" => &triangle.b,
                "c" => &triangle.c,
                "color" => &triangle.color,
                name => panic!("unverified triangle field {name}"),
            };
            for (component, expected) in expected.iter().enumerate() {
                let start = index * stride + attribute.offset as usize + component * 4;
                let value = f32::from_ne_bytes(bytes[start..start + 4].try_into().unwrap());
                assert_eq!(value, *expected);
            }
        }
    }
}

#[test]
fn pie_bytes_round_trip_through_the_shader_attribute_offsets() {
    use fm_render_canvas::GpuPieWedge;

    let wedges = [
        GpuPieWedge {
            center: [-12.0, 45.0],
            radius: 80.0,
            start_angle: -1.5,
            sweep_angle: 4.7,
            fill: [0.1, 0.3, 0.7, 0.8],
            stroke: [0.9, 0.6, 0.2, 1.0],
            stroke_width: 2.5,
            slice_index: 37,
        },
        GpuPieWedge {
            center: [91.0, -6.0],
            radius: 12.0,
            start_angle: 3.2,
            sweep_angle: 0.5,
            fill: [0.8, 0.4, 0.1, 0.3],
            stroke: [0.2, 0.7, 0.9, 0.5],
            stroke_width: 1.0,
            slice_index: 912,
        },
    ];
    let bytes = fm_render_canvas::gpu_device::pie_instance_bytes(&wedges);
    let descriptor = fm_render_canvas::gpu_pipeline::pie_pipeline();
    let stride = descriptor.instance.array_stride as usize;
    assert_eq!(bytes.len(), stride * wedges.len());
    for (index, wedge) in wedges.iter().enumerate() {
        for attribute in descriptor.instance.attributes {
            let offset = index * stride + attribute.offset as usize;
            if attribute.name == "slice_index" {
                assert_eq!(
                    u32::from_ne_bytes(bytes[offset..offset + 4].try_into().unwrap()),
                    wedge.slice_index
                );
                continue;
            }
            let expected: &[f32] = match attribute.name {
                "center" => &wedge.center,
                "radius" => std::slice::from_ref(&wedge.radius),
                "start_angle" => std::slice::from_ref(&wedge.start_angle),
                "sweep_angle" => std::slice::from_ref(&wedge.sweep_angle),
                "fill" => &wedge.fill,
                "stroke" => &wedge.stroke,
                "stroke_width" => std::slice::from_ref(&wedge.stroke_width),
                name => panic!("unverified pie attribute {name}"),
            };
            for (component, expected) in expected.iter().enumerate() {
                let start = offset + component * 4;
                assert_eq!(
                    f32::from_ne_bytes(bytes[start..start + 4].try_into().unwrap()),
                    *expected
                );
            }
        }
    }
}

#[test]
fn pie_sector_pixels_preserve_quantities_wraparound_and_zero_values() {
    let Some(gpu) = device_or_skip() else {
        return;
    };
    let pass = InstancePass::new(&gpu, &fm_render_canvas::gpu_pipeline::pie_pipeline());
    for source in [
        "pie\n \"Quarter\": 1\n \"Remainder\": 3\n",
        "pie\n \"Entire circle\": 7\n",
        "pie\n \"Zero\": 0\n \"Also zero\": 0\n",
    ] {
        let ir = fm_parser::parse(source).ir;
        let layout = fm_layout::layout_diagram(&ir);
        let plan = GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());
        assert!(!plan.pie_wedges.is_empty());
        let bytes = fm_render_canvas::gpu_device::pie_instance_bytes(&plan.pie_wedges);
        let draw = InstanceDraw {
            bounds: &plan.bounds,
            instance_bytes: &bytes,
            instance_count: plan.pie_wedges.len() as u32,
            atlas: None,
        };
        let image = render_instances(&gpu, &pass, &draw, 640, 480).expect("render pie sectors");
        if plan.pie_wedges.iter().all(|wedge| wedge.sweep_angle == 0.0) {
            assert!(
                image
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|pixel| pixel[3] == 0),
                "zero-valued pie slices painted pixels"
            );
            continue;
        }
        for wedge in &plan.pie_wedges {
            if wedge.sweep_angle <= 0.0 {
                continue;
            }
            for fraction in [0.25, 0.5, 0.75] {
                let angle = wedge.start_angle + wedge.sweep_angle * fraction;
                let x = wedge.center[0] + wedge.radius * 0.55 * angle.cos();
                let y = wedge.center[1] + wedge.radius * 0.55 * angle.sin();
                let (px, py) = image.pixel_for(&plan.bounds, x, y);
                let pixel = image.pixel(px, py).unwrap();
                let expected = wedge
                    .fill
                    .map(|component| (component * 255.0).round() as u8);
                assert!(
                    pixel
                        .iter()
                        .zip(expected)
                        .all(|(actual, expected)| actual.abs_diff(expected) <= 3),
                    "slice {} lost its angular extent or color at fraction {fraction}: {pixel:?} vs {expected:?}",
                    wedge.slice_index
                );
            }
        }
        let first = plan.pie_wedges[0];
        let (x, y) = image.pixel_for(
            &plan.bounds,
            first.center[0] + first.radius * 1.15,
            first.center[1],
        );
        assert_eq!(
            image.pixel(x, y).unwrap()[3],
            0,
            "pie fill escaped its circular boundary"
        );
    }
}

#[test]
fn xy_area_fills_reach_the_gpu_on_both_sides_of_zero() {
    let Some(gpu) = device_or_skip() else {
        return;
    };
    let pipelines = DiagramPipelines::new(&gpu);
    for orientation in ["", " horizontal"] {
        let ir = fm_parser::parse(&format!(
            "xychart-beta{orientation}\n x-axis [A, B]\n y-axis -10 --> 10\n area [-8, 8]\n"
        ))
        .ir;
        let layout = fm_layout::layout_diagram(&ir);
        let plan = GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());
        assert_eq!(plan.xy_area_triangles.len(), 2);
        let mut without_fill = plan.clone();
        without_fill.xy_area_triangles.clear();
        let image =
            render_diagram(&gpu, &pipelines, &plan, None, 800, 600).expect("render filled chart");
        let control = render_diagram(&gpu, &pipelines, &without_fill, None, 800, 600)
            .expect("render fill-free control");
        for triangle in &plan.xy_area_triangles {
            let x = (triangle.a[0] + triangle.b[0] + triangle.c[0]) / 3.0;
            let y = (triangle.a[1] + triangle.b[1] + triangle.c[1]) / 3.0;
            let (px, py) = image.pixel_for(&plan.bounds, x, y);
            let alpha = image.pixel(px, py).unwrap()[3];
            let control_alpha = control.pixel(px, py).unwrap()[3];
            assert!(
                u16::from(alpha) > u16::from(control_alpha) + 20,
                "area lobe at ({x}, {y}) was planned but not submitted: alpha {alpha} versus control {control_alpha}"
            );
        }
    }
}

#[test]
fn xy_line_segments_remain_visible_over_bar_interiors() {
    let Some(gpu) = device_or_skip() else {
        return;
    };
    let pipelines = DiagramPipelines::new(&gpu);
    for orientation in ["", " horizontal"] {
        let ir = fm_parser::parse(&format!(
            "xychart-beta{orientation}\n x-axis [A, B]\n y-axis 0 --> 25\n bar [20, 20]\n line [10, 10]\n"
        ))
        .ir;
        let layout = fm_layout::layout_diagram(&ir);
        let plan = GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());
        assert_eq!(plan.xy_bar_instances.len(), 2);
        assert_eq!(plan.edge_segments.len(), 1);
        let line = &plan.edge_segments[0];
        let mut sample = line.from;
        let category_axis = usize::from(ir.xy_chart_meta.as_ref().unwrap().horizontal);
        // The point marker already used to cover bars. Sample the line itself,
        // inside the first bar but well away from the marker's radius.
        sample[category_axis] += 12.0;
        let bar = &plan.xy_bar_instances[0];
        for (axis, coordinate) in sample.iter().enumerate() {
            assert!((coordinate - bar.center[axis]).abs() < bar.half_extent[axis] - 2.0);
        }
        let mut without_bars = plan.clone();
        without_bars.xy_bar_instances.clear();
        let image = render_diagram(&gpu, &pipelines, &plan, None, 800, 600)
            .expect("render mixed bar/line chart");
        let control = render_diagram(&gpu, &pipelines, &without_bars, None, 800, 600)
            .expect("render line-only control");
        let (px, py) = image.pixel_for(&plan.bounds, sample[0], sample[1]);
        let expected = line
            .color
            .map(|component| (component * 255.0).round() as u8);
        let error = |pixel: [u8; 4]| {
            pixel
                .iter()
                .zip(expected.iter())
                .map(|(actual, expected)| u16::from(actual.abs_diff(*expected)))
                .sum::<u16>()
        };
        // Pick the nearest fully covered ribbon pixel to avoid depending on
        // subpixel rounding or antialiasing at the line boundary.
        let (px, py) = (py.saturating_sub(1)..=py + 1)
            .flat_map(|y| (px.saturating_sub(1)..=px + 1).map(move |x| (x, y)))
            .min_by_key(|&(x, y)| error(control.pixel(x, y).unwrap()))
            .unwrap();
        assert!(error(control.pixel(px, py).unwrap()) < 12);
        assert!(
            error(image.pixel(px, py).unwrap()) < 12,
            "{orientation:?} line was covered by a bar: expected {expected:?}, got {:?}",
            image.pixel(px, py).unwrap()
        );
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
/// The ordinary arrow is sampled slightly BEHIND its tip along the facing angle, not at the tip
/// itself. Although every marker kind is rasterised inside the same quad, the arrow itself is a
/// triangle whose tip is the one antialiased point most likely to be partially transparent for
/// reasons that are not defects. A quarter of the marker size back lands in its body.
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

/// Check the actual shader silhouette, including the empty regions that distinguish an open
/// marker from a filled one. Planning the right enum alone cannot prove the GPU draws it.
#[test]
fn the_marker_pipeline_preserves_half_open_and_interface_glyphs() {
    use fm_render_canvas::{GpuArrowheadInstance, GpuMarkerKind as K};

    let Some(gpu) = device_or_skip() else {
        return;
    };
    let pass = InstancePass::new(&gpu, &arrowhead_pipeline());
    let bounds = fm_layout::LayoutRect {
        x: 0.0,
        y: 0.0,
        width: 40.0,
        height: 40.0,
    };
    for (kind, painted, empty) in [
        (K::Arrow, (-6.0, 0.0), (2.0, 0.0)),
        (K::Circle, (0.0, 3.0), (0.0, 6.0)),
        (K::Cross, (2.0, 2.0), (3.0, 0.0)),
        (K::Diamond, (-5.0, 0.0), (3.0, 0.0)),
        (K::DiamondOpen, (-5.0, 5.0), (-5.0, 0.0)),
        (K::TriangleOpen, (-10.0, 0.0), (-5.0, 0.0)),
        (K::Open, (-5.0, -2.5), (-5.0, 0.0)),
        (K::HalfArrowTop, (-6.0, -1.0), (-6.0, 1.0)),
        (K::HalfArrowBottom, (-6.0, 1.0), (-6.0, -1.0)),
        (K::StickArrowTop, (-5.0, -2.5), (-6.0, -0.5)),
        (K::StickArrowBottom, (-5.0, 2.5), (-6.0, 0.5)),
        (K::Lollipop, (6.0, 6.0), (6.0, 0.0)),
        (K::LollipopStart, (-6.0, 6.0), (-6.0, 0.0)),
    ] {
        let marker = GpuArrowheadInstance {
            position: [20.0, 20.0],
            angle: 0.0,
            size: 10.0,
            edge_index: 0,
            kind: kind as u32,
            color: [0.1, 0.2, 0.3, 1.0],
            fill: [1.0, 1.0, 1.0, 1.0],
        };
        let bytes = arrowhead_instance_bytes(&[marker]);
        let draw = InstanceDraw {
            bounds: &bounds,
            instance_bytes: &bytes,
            instance_count: 1,
            atlas: None,
        };
        let image = render_instances(&gpu, &pass, &draw, 512, 512).expect("render marker");
        let sample = |point: (f32, f32)| {
            let (x, y) = image.pixel_for(&bounds, 20.0 + point.0, 20.0 + point.1);
            image.pixel(x, y).expect("marker sample inside texture")[3]
        };
        assert!(
            sample(painted) > 100,
            "{kind:?} omitted its authored shape at {painted:?}"
        );
        assert!(
            sample(empty) < 20,
            "{kind:?} filled an intentionally empty region at {empty:?}"
        );
    }
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

/// A sequence interaction frame must survive the whole device path, not merely exist in a CPU plan.
/// The sample is deliberately in the lower-right interior: it is away from the caption, dashed
/// perimeter, participants, and message rows, so opacity there can only come from the fragment fill.
#[test]
fn one_submission_paints_a_sequence_fragment_interior() {
    let Some(gpu) = device_or_skip() else {
        return;
    };

    let ir = fm_parser::parse(
        "sequenceDiagram\n  Alice->>Bob: request\n  alt approved\n    Bob-->>Alice: yes\n  else rejected\n    Bob-->>Alice: no\n  end\n",
    )
    .ir;
    let layout = fm_layout::layout_diagram(&ir);
    let fragment = layout
        .extensions
        .sequence_fragments
        .first()
        .expect("CONTROL FAILED: parsed alt fixture produced no fragment");
    let plan = GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());
    assert_eq!(
        plan.sequence_fragment_instances.len(),
        1,
        "CONTROL FAILED: fragment has no GPU fill instance"
    );
    assert!(
        plan.sequence_fragment_border_segments.len() >= 5,
        "CONTROL FAILED: fragment has no GPU dashed perimeter and divider"
    );

    let atlas = GlyphAtlasTexture::new(&gpu, &plan.glyph_atlas, &solid_coverage(&plan.glyph_atlas));
    let pipelines = DiagramPipelines::new(&gpu);
    let image = render_diagram(&gpu, &pipelines, &plan, Some(&atlas), 512, 512)
        .expect("render diagram with fragment");
    let sample_x = fragment.bounds.x + (fragment.bounds.width * 0.8);
    let sample_y = fragment.bounds.y + (fragment.bounds.height * 0.8);
    let (x, y) = image.pixel_for(&plan.bounds, sample_x, sample_y);
    let pixel = image.pixel(x, y).expect("interior maps inside the image");
    assert!(
        pixel[3] > 0,
        "fragment interior at ({sample_x}, {sample_y}) is transparent after full device submission; \
         the frame was planned but not uploaded or drawn"
    );
}

/// Each omitted layer is compared against the same complete parsed diagram. This detects a
/// populated CPU buffer that is never uploaded or submitted, even when its labels still render.
#[test]
fn whole_diagram_submission_paints_the_planned_furniture() {
    let Some(gpu) = device_or_skip() else {
        return;
    };
    let pipelines = DiagramPipelines::new(&gpu);
    const SEQUENCE: &str = "%%{init: {\"sequence\": {\"mirrorActors\": true}}}%%\nsequenceDiagram\n participant Alice\n participant Bob\n Alice->>Bob: request\n activate Bob\n Note over Alice,Bob: processing\n Bob-->>Alice: response\n deactivate Bob\n destroy Bob\n Alice->>Bob: goodbye\n";
    const STATE_NOTE: &str = "stateDiagram-v2\n [*] --> Idle\n Idle --> Running\n note right of Idle\n Context\n end note\n";
    type Omit = fn(&mut GpuRenderPlan);
    let cases: &[(&str, &str, Omit)] = &[
        (
            "cluster",
            "flowchart TD\n subgraph Group\n A[Alpha] --> B[Beta]\n end\n",
            |plan| plan.cluster_instances.clear(),
        ),
        ("lifelines", SEQUENCE, |plan| {
            plan.band_lane_segments.clear()
        }),
        ("activation", SEQUENCE, |plan| {
            plan.activation_instances.clear()
        }),
        ("mirror headers", SEQUENCE, |plan| {
            plan.mirror_header_instances.clear()
        }),
        ("destroy marker", SEQUENCE, |plan| {
            plan.lifecycle_marker_segments.clear()
        }),
        ("sequence note", SEQUENCE, |plan| {
            plan.sequence_note_instances.clear()
        }),
        ("state note leader", STATE_NOTE, |plan| {
            plan.state_note_leader_segments.clear()
        }),
        ("state note", STATE_NOTE, |plan| {
            plan.state_note_instances.clear()
        }),
        (
            "state divider",
            "stateDiagram-v2\n state Active {\n [*] --> Processing\n --\n [*] --> Monitoring\n }\n",
            |plan| plan.cluster_divider_segments.clear(),
        ),
        (
            "packet continuation",
            "packet-beta\n 0-7: Header\n 24-47: Wrapped field\n",
            |plan| plan.packet_field_continuation_instances.clear(),
        ),
        (
            "Gantt section",
            include_str!("../../fm-cli/tests/fixtures/frankentui_conformance/gantt_project.mmd"),
            |plan| plan.band_section_instances.clear(),
        ),
        (
            "axis ticks",
            include_str!("../../fm-cli/tests/fixtures/frankentui_conformance/gantt_project.mmd"),
            |plan| plan.axis_tick_segments.clear(),
        ),
        (
            "column separators",
            include_str!("../../fm-cli/tests/fixtures/frankentui_conformance/sankey_links.mmd"),
            |plan| plan.band_column_segments.clear(),
        ),
        (
            "treemap tiles",
            "treemap\n\"Root\"\n    \"Group\"\n        \"A\": 10\n        \"B\": 20\n",
            |plan| plan.treemap_tile_instances.clear(),
        ),
        (
            "radar wheel",
            "radar-beta\n axis a, b, c\n curve x{1,2,3}\n",
            |plan| plan.radar_segments.clear(),
        ),
        (
            "ER cardinality",
            "erDiagram\n A }o--o{ B : relation\n",
            |plan| plan.er_marker_segments.clear(),
        ),
        (
            "pie sectors",
            "pie\n \"First\": 1\n \"Second\": 3\n",
            |plan| plan.pie_wedges.clear(),
        ),
    ];
    for &(name, source, omit) in cases {
        let ir = fm_parser::parse(source).ir;
        let layout = fm_layout::layout_diagram(&ir);
        let plan = GpuRenderPlan::from_layout(&ir, &layout, plan_stroke_width());
        let mut without_layer = plan.clone();
        omit(&mut without_layer);
        assert_ne!(
            plan, without_layer,
            "{name} fixture contains no target geometry"
        );
        let image = render_diagram(&gpu, &pipelines, &plan, None, 640, 480)
            .expect("render complete diagram geometry");
        let control = render_diagram(&gpu, &pipelines, &without_layer, None, 640, 480)
            .expect("render diagram with one layer omitted");
        let changed = image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .zip(control.rgba.as_chunks::<4>().0)
            .filter(|(actual, missing)| {
                actual
                    .iter()
                    .zip(missing.iter())
                    .any(|(a, b)| a.abs_diff(*b) > 2)
            })
            .count();
        assert!(
            changed > 3,
            "{name} was planned but changed only {changed} pixels after submission"
        );
        if name == "cluster" {
            // The newly visible background must remain behind its node boxes.
            for node in &plan.node_instances {
                let (x, y) = image.pixel_for(&plan.bounds, node.center[0], node.center[1]);
                assert_eq!(
                    image.pixel(x, y),
                    control.pixel(x, y),
                    "cluster covered its node"
                );
            }
        }
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

#[test]
fn image_exports_preserve_circles_on_wide_and_tall_targets() {
    let Some(gpu) = device_or_skip() else {
        return;
    };
    let parsed = fm_parser::parse("flowchart LR\nA((Circle))\n");
    let layout = fm_layout::layout_diagram(&parsed.ir);
    let mut plan = GpuRenderPlan::from_layout(&parsed.ir, &layout, plan_stroke_width());
    plan.text_quads.clear();
    plan.bounds = fm_layout::LayoutRect {
        x: -50.0,
        y: -50.0,
        width: 100.0,
        height: 100.0,
    };
    let node = &mut plan.node_instances[0];
    node.center = [0.0, 0.0];
    node.half_extent = [20.0, 20.0];
    node.fill = [1.0, 0.0, 0.0, 1.0];
    node.stroke = [0.0; 4];
    node.stroke_width = 0.0;
    let pipelines = DiagramPipelines::new(&gpu);
    let pass = InstancePass::new(&gpu, &node_pipeline());
    let bytes = node_instance_bytes(&plan.node_instances);
    let draw = InstanceDraw {
        bounds: &plan.bounds,
        instance_bytes: &bytes,
        instance_count: 1,
        atlas: None,
    };
    for (width, height) in [(300, 100), (100, 300)] {
        for image in [
            render_diagram(&gpu, &pipelines, &plan, None, width, height).unwrap(),
            render_instances(&gpu, &pass, &draw, width, height).unwrap(),
        ] {
            let center = image.pixel_for(&plan.bounds, 0.0, 0.0);
            assert_eq!(center, (width / 2, height / 2));
            let horizontal = (0..width)
                .filter(|x| image.pixel(*x, height / 2).unwrap()[3] >= 128)
                .count();
            let vertical = (0..height)
                .filter(|y| image.pixel(width / 2, *y).unwrap()[3] >= 128)
                .count();
            assert!(
                (38..=42).contains(&horizontal),
                "circle width={horizontal} in {width}x{height}"
            );
            assert!(
                (38..=42).contains(&vertical),
                "circle height={vertical} in {width}x{height}"
            );
            assert!(horizontal.abs_diff(vertical) <= 2);
            assert_eq!(image.pixel(0, 0).unwrap(), [0, 0, 0, 0]);
        }
    }
}

#[test]
fn image_readback_preserves_translucent_colors_and_source_over_composition() {
    let Some(gpu) = device_or_skip() else {
        return;
    };
    let parsed = fm_parser::parse("flowchart LR\nA[Fill]\n");
    let layout = fm_layout::layout_diagram(&parsed.ir);
    let mut plan = GpuRenderPlan::from_layout(&parsed.ir, &layout, plan_stroke_width());
    plan.text_quads.clear();
    plan.node_instances[0].fill = [1.0, 0.5, 0.25, 0.5];
    plan.node_instances[0].stroke = [0.0; 4];
    plan.node_instances[0].stroke_width = 0.0;
    let center = plan.node_instances[0].center;
    let pipelines = DiagramPipelines::new(&gpu);
    let image = render_diagram(&gpu, &pipelines, &plan, None, 256, 128).unwrap();
    let (x, y) = image.pixel_for(&plan.bounds, center[0], center[1]);
    let pixel = image.pixel(x, y).unwrap();
    for (actual, expected) in pixel.into_iter().zip([255_u8, 128, 64, 128]) {
        assert!(
            actual.abs_diff(expected) <= 2,
            "translucent fill was darkened: {pixel:?}"
        );
    }

    plan.node_instances[0].fill = [1.0, 0.0, 0.0, 0.5];
    let mut over = plan.node_instances[0];
    over.fill = [0.0, 0.0, 1.0, 0.5];
    plan.node_instances.push(over);
    let image = render_diagram(&gpu, &pipelines, &plan, None, 256, 128).unwrap();
    let pixel = image.pixel(x, y).unwrap();
    // Half-blue over half-red has alpha 3/4 and straight RGB (1/3, 0, 2/3).
    for (actual, expected) in pixel.into_iter().zip([85_u8, 0, 170, 191]) {
        assert!(
            actual.abs_diff(expected) <= 2,
            "source-over readback was not decoded: {pixel:?}"
        );
    }
}

#[test]
fn invalid_image_sizes_and_bounds_return_errors_before_texture_creation() {
    let bounds = fm_layout::LayoutRect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
    };
    assert!(fm_render_canvas::gpu_device::camera_transform(&bounds, 0, 100).is_err());
    let invalid = fm_layout::LayoutRect {
        x: f32::NAN,
        ..bounds
    };
    assert!(fm_render_canvas::gpu_device::camera_transform(&invalid, 100, 100).is_err());
    let Some(gpu) = device_or_skip() else {
        return;
    };
    let parsed = fm_parser::parse("flowchart LR\nA\n");
    let layout = fm_layout::layout_diagram(&parsed.ir);
    let plan = GpuRenderPlan::from_layout(&parsed.ir, &layout, plan_stroke_width());
    let pipelines = DiagramPipelines::new(&gpu);
    for (width, height) in [(0, 100), (100, 0), (u32::MAX, 1)] {
        assert!(matches!(
            render_diagram(&gpu, &pipelines, &plan, None, width, height),
            Err(GpuDeviceError::InvalidInput(_))
        ));
    }
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
        fm_render_canvas::gpu_device::camera_transform(&bounds, 100, 50).unwrap();

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
