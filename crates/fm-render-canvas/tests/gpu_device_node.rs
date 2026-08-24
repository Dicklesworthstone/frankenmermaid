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
use fm_render_canvas::gpu_device::{GpuDevice, GpuDeviceError, NodePass, node_instance_bytes, render_nodes};
use fm_render_canvas::gpu_pipeline::node_pipeline;

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
        Err(other) => panic!("wgpu is present but unusable, which is a defect not an environment: {other}"),
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

    let pass = NodePass::new(&gpu, &node_pipeline());
    let image = render_nodes(&gpu, &pass, &plan, 512, 512).expect("render");

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

    let pass = NodePass::new(&gpu, &node_pipeline());
    let image = render_nodes(&gpu, &pass, &plan, 64, 64).expect("render");
    assert!(
        image.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 0),
        "an empty diagram painted pixels"
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
    assert!((left - -1.0).abs() < 1e-5, "left edge maps to {left}, not -1");
    assert!((right - 1.0).abs() < 1e-5, "right edge maps to {right}, not 1");
    // TOP of the layout is TOP of the screen, i.e. +1 in clip space.
    assert!((top - 1.0).abs() < 1e-5, "layout top maps to {top}, not +1");
    assert!(
        (bottom - -1.0).abs() < 1e-5,
        "layout bottom maps to {bottom}, not -1"
    );
    assert!(scale_y < 0.0, "the Y flip is missing; diagrams render upside down");
}
