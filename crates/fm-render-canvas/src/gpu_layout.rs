//! Vertex-buffer layouts for the WebGPU passes (bd-2u0.2).
//!
//! A backend has to tell the device three things about every instance buffer: its stride, and for
//! each attribute a byte offset, a format, and a shader location. Those three descriptions live in
//! three different places — the `#[repr(C)]` struct in `gpu_plan`, the `@location` list in the WGSL,
//! and the pipeline descriptor — and NOTHING makes them agree. A wrong offset does not fail to
//! compile and does not fail to run: the shader reads the bytes that happen to be there, so a
//! diagram renders with its colours in its coordinates. That is the single most expensive bug
//! available in this area, and it is invisible to every test that only checks the plan's contents.
//!
//! This module is that missing third description, expressed as data, with tests that derive the
//! offsets from the structs themselves via `offset_of!` rather than restating them. It deliberately
//! does NOT depend on `wgpu`: the crate has no such dependency today, and a plain description a
//! backend maps onto `wgpu::VertexBufferLayout` is both testable here and cheap to consume there.

use core::mem::{align_of, offset_of, size_of};

use crate::gpu_plan::{GpuArrowheadInstance, GpuEdgeSegment, GpuNodeInstance, GpuTextQuad};

/// The vertex formats these buffers use.
///
/// A deliberately tiny set: every field in every instance struct is one of these three, and a format
/// this layer cannot describe is a field a shader could not read anyway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVertexFormat {
    Float32,
    Float32x2,
    Float32x4,
    Uint32,
}

impl GpuVertexFormat {
    /// Size in bytes, which must equal the size of the Rust field it describes.
    #[must_use]
    pub const fn size_bytes(self) -> usize {
        match self {
            Self::Float32 => 4,
            Self::Float32x2 => 8,
            Self::Float32x4 => 16,
            Self::Uint32 => 4,
        }
    }

    /// The `wgpu::VertexFormat` variant name a backend maps this to.
    ///
    /// A string rather than the enum itself, because taking the dependency here would pull a
    /// graphics stack into a crate that otherwise renders to a 2D canvas.
    #[must_use]
    pub const fn wgpu_name(self) -> &'static str {
        match self {
            Self::Float32 => "Float32",
            Self::Float32x2 => "Float32x2",
            Self::Float32x4 => "Float32x4",
            Self::Uint32 => "Uint32",
        }
    }
}

/// One attribute: where it sits, what it is, and which `@location` reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuVertexAttribute {
    /// The `@location(N)` in the WGSL that consumes this attribute.
    pub shader_location: u32,
    /// Byte offset from the start of the instance.
    pub offset: usize,
    pub format: GpuVertexFormat,
    /// Field name, carried for diagnostics — a mismatch reported as "offset 24" is far harder to
    /// act on than one reported as "half_extent".
    pub field: &'static str,
}

/// A whole instance buffer: stride plus attributes, stepped per instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuBufferLayout {
    /// Bytes between consecutive instances. Always the struct's own size.
    pub stride: usize,
    /// Alignment the struct requires, which a backend needs when suballocating.
    pub align: usize,
    pub attributes: &'static [GpuVertexAttribute],
}

const NODE_ATTRIBUTES: &[GpuVertexAttribute] = &[
    GpuVertexAttribute {
        shader_location: 0,
        offset: offset_of!(GpuNodeInstance, center),
        format: GpuVertexFormat::Float32x2,
        field: "center",
    },
    GpuVertexAttribute {
        shader_location: 1,
        offset: offset_of!(GpuNodeInstance, half_extent),
        format: GpuVertexFormat::Float32x2,
        field: "half_extent",
    },
    GpuVertexAttribute {
        shader_location: 2,
        offset: offset_of!(GpuNodeInstance, fill),
        format: GpuVertexFormat::Float32x4,
        field: "fill",
    },
    GpuVertexAttribute {
        shader_location: 3,
        offset: offset_of!(GpuNodeInstance, stroke),
        format: GpuVertexFormat::Float32x4,
        field: "stroke",
    },
    GpuVertexAttribute {
        shader_location: 4,
        offset: offset_of!(GpuNodeInstance, shape),
        format: GpuVertexFormat::Uint32,
        field: "shape",
    },
    GpuVertexAttribute {
        shader_location: 5,
        offset: offset_of!(GpuNodeInstance, node_index),
        format: GpuVertexFormat::Uint32,
        field: "node_index",
    },
    GpuVertexAttribute {
        // Per-instance border width (bd-lvj3 / bd-2u0.2). Appended as location 6 rather than
        // inserted next to `stroke`, because renumbering the existing locations would require the
        // shader to move in lockstep for no gain -- and `offset_of!` means the ORDER of the struct
        // fields and the ORDER of these entries are independent.
        shader_location: 6,
        offset: offset_of!(GpuNodeInstance, stroke_width),
        format: GpuVertexFormat::Float32,
        field: "stroke_width",
    },
];

const EDGE_ATTRIBUTES: &[GpuVertexAttribute] = &[
    GpuVertexAttribute {
        shader_location: 0,
        offset: offset_of!(GpuEdgeSegment, from),
        format: GpuVertexFormat::Float32x2,
        field: "from",
    },
    GpuVertexAttribute {
        shader_location: 1,
        offset: offset_of!(GpuEdgeSegment, to),
        format: GpuVertexFormat::Float32x2,
        field: "to",
    },
    GpuVertexAttribute {
        shader_location: 2,
        offset: offset_of!(GpuEdgeSegment, edge_index),
        format: GpuVertexFormat::Uint32,
        field: "edge_index",
    },
    GpuVertexAttribute {
        shader_location: 3,
        offset: offset_of!(GpuEdgeSegment, color),
        format: GpuVertexFormat::Float32x4,
        field: "color",
    },
    GpuVertexAttribute {
        shader_location: 4,
        offset: offset_of!(GpuEdgeSegment, dash),
        format: GpuVertexFormat::Float32x2,
        field: "dash",
    },
    GpuVertexAttribute {
        shader_location: 5,
        offset: offset_of!(GpuEdgeSegment, width),
        format: GpuVertexFormat::Float32,
        field: "width",
    },
    GpuVertexAttribute {
        shader_location: 6,
        offset: offset_of!(GpuEdgeSegment, dash_phase),
        format: GpuVertexFormat::Float32,
        field: "dash_phase",
    },
];

const ARROWHEAD_ATTRIBUTES: &[GpuVertexAttribute] = &[
    GpuVertexAttribute {
        shader_location: 0,
        offset: offset_of!(GpuArrowheadInstance, position),
        format: GpuVertexFormat::Float32x2,
        field: "position",
    },
    GpuVertexAttribute {
        shader_location: 1,
        offset: offset_of!(GpuArrowheadInstance, angle),
        format: GpuVertexFormat::Float32,
        field: "angle",
    },
    GpuVertexAttribute {
        shader_location: 2,
        offset: offset_of!(GpuArrowheadInstance, size),
        format: GpuVertexFormat::Float32,
        field: "size",
    },
    GpuVertexAttribute {
        shader_location: 3,
        offset: offset_of!(GpuArrowheadInstance, edge_index),
        format: GpuVertexFormat::Uint32,
        field: "edge_index",
    },
    // `kind` MUST hold @location(4). It was absent while `color` sat here wearing location 4, so the
    // shader's `kind: u32` was fed the first four bytes of an RGBA vec4 reinterpreted as an integer
    // — a marker whose FORM is decided by a colour channel. That is why this is not merely a missing
    // field: the two that follow were undefined, and the one that was present was wrong.
    GpuVertexAttribute {
        shader_location: 4,
        offset: offset_of!(GpuArrowheadInstance, kind),
        format: GpuVertexFormat::Uint32,
        field: "kind",
    },
    GpuVertexAttribute {
        shader_location: 5,
        offset: offset_of!(GpuArrowheadInstance, color),
        format: GpuVertexFormat::Float32x4,
        field: "color",
    },
    GpuVertexAttribute {
        shader_location: 6,
        offset: offset_of!(GpuArrowheadInstance, fill),
        format: GpuVertexFormat::Float32x4,
        field: "fill",
    },
];

const TEXT_ATTRIBUTES: &[GpuVertexAttribute] = &[
    GpuVertexAttribute {
        shader_location: 0,
        offset: offset_of!(GpuTextQuad, center),
        format: GpuVertexFormat::Float32x2,
        field: "center",
    },
    GpuVertexAttribute {
        shader_location: 1,
        offset: offset_of!(GpuTextQuad, half_extent),
        format: GpuVertexFormat::Float32x2,
        field: "half_extent",
    },
    GpuVertexAttribute {
        shader_location: 2,
        offset: offset_of!(GpuTextQuad, uv_min),
        format: GpuVertexFormat::Float32x2,
        field: "uv_min",
    },
    GpuVertexAttribute {
        shader_location: 3,
        offset: offset_of!(GpuTextQuad, uv_max),
        format: GpuVertexFormat::Float32x2,
        field: "uv_max",
    },
    GpuVertexAttribute {
        shader_location: 4,
        offset: offset_of!(GpuTextQuad, color),
        format: GpuVertexFormat::Float32x4,
        field: "color",
    },
    GpuVertexAttribute {
        shader_location: 5,
        offset: offset_of!(GpuTextQuad, run_index),
        format: GpuVertexFormat::Uint32,
        field: "run_index",
    },
];

/// Layout of the node instance buffer consumed by `NODE_SDF_WGSL`.
#[must_use]
pub const fn node_buffer_layout() -> GpuBufferLayout {
    GpuBufferLayout {
        stride: size_of::<GpuNodeInstance>(),
        align: align_of::<GpuNodeInstance>(),
        attributes: NODE_ATTRIBUTES,
    }
}

/// Layout of the edge segment buffer consumed by `EDGE_WGSL`.
#[must_use]
pub const fn edge_buffer_layout() -> GpuBufferLayout {
    GpuBufferLayout {
        stride: size_of::<GpuEdgeSegment>(),
        align: align_of::<GpuEdgeSegment>(),
        attributes: EDGE_ATTRIBUTES,
    }
}

/// Layout of the arrowhead buffer consumed by `ARROWHEAD_WGSL`.
#[must_use]
pub const fn arrowhead_buffer_layout() -> GpuBufferLayout {
    GpuBufferLayout {
        stride: size_of::<GpuArrowheadInstance>(),
        align: align_of::<GpuArrowheadInstance>(),
        attributes: ARROWHEAD_ATTRIBUTES,
    }
}

/// Layout of the text quad buffer consumed by `TEXT_ATLAS_WGSL`.
#[must_use]
pub const fn text_buffer_layout() -> GpuBufferLayout {
    GpuBufferLayout {
        stride: size_of::<GpuTextQuad>(),
        align: align_of::<GpuTextQuad>(),
        attributes: TEXT_ATTRIBUTES,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        GpuBufferLayout, GpuVertexFormat, arrowhead_buffer_layout, edge_buffer_layout,
        node_buffer_layout, offset_of, text_buffer_layout,
    };
    use crate::gpu_plan::GpuArrowheadInstance;
    use crate::gpu_plan::{ARROWHEAD_WGSL, EDGE_WGSL, NODE_SDF_WGSL, TEXT_ATLAS_WGSL};

    // WGSL / layout ABI agreement (bd-2u0.2).
    //
    // `offset_of!` keeps the Rust side honest with itself, but the SHADER is a second hand-written
    // description of the same interface in another language, and nothing compared the two. A
    // `@location` added to one and not the other is invisible to the compiler and to every test on
    // this host: there is no GPU here and no `naga`. It would surface as a shader reading a stroke
    // colour out of the bytes of a shape enum -- confidently wrong pixels, not an error.
    //
    // These live INSIDE the crate rather than in `tests/` because `gpu_layout` and `gpu_plan` are
    // private modules; publishing them to give an integration test access would widen the crate's
    // API to serve a test, which is the wrong trade.

    /// Pull `(location, name, wgsl_type)` out of the named WGSL struct.
    ///
    /// Deliberately scoped to ONE struct rather than scanning the whole source: `VertexOut` in the same
    /// shader also numbers its members from `@location(0)`, and a whole-file scan would match those and
    /// report a mismatch that does not exist. The vertex-INPUT struct is the only one describing the
    /// buffer ABI.
    fn wgsl_struct_locations(source: &str, struct_name: &str) -> Vec<(u32, String, String)> {
        let header = format!("struct {struct_name} {{");
        let start = source
            .find(&header)
            .unwrap_or_else(|| panic!("{struct_name} is not in the shader source"))
            + header.len();
        let body = &source[start..];
        let end = body.find('}').expect("unterminated struct");

        let mut out = Vec::new();
        for line in body[..end].lines() {
            let line = line.trim().trim_end_matches(',');
            let Some(rest) = line.strip_prefix("@location(") else {
                continue;
            };
            let (location, rest) = rest.split_once(')').expect("malformed @location");
            let (name, ty) = rest.split_once(':').expect("malformed member");
            out.push((
                location
                    .trim()
                    .parse::<u32>()
                    .expect("location is a number"),
                name.trim().to_string(),
                ty.trim().to_string(),
            ));
        }
        out
    }

    /// The WGSL type a given vertex format must be declared as.
    fn expected_wgsl_type(format: GpuVertexFormat) -> &'static str {
        match format {
            super::GpuVertexFormat::Float32 => "f32",
            super::GpuVertexFormat::Float32x2 => "vec2<f32>",
            super::GpuVertexFormat::Float32x4 => "vec4<f32>",
            super::GpuVertexFormat::Uint32 => "u32",
        }
    }

    /// ⚠️ EVERY buffer/shader PAIR, not just the node one.
    ///
    /// This gate covered `NodeInstance` alone for three days while three other pairs -- edge,
    /// arrowhead, text -- went unchecked. A gate that covers one member of a family and READS as
    /// covering the family is its own hazard: it is green, it is named after the general property,
    /// and the uncovered members look guarded. The same shape as the resolver gate that scanned
    /// `resolve_node_*` while the cluster resolvers sat unconsumed.
    ///
    /// Driven from a table so adding a fifth pipeline means adding a row, not remembering to write
    /// a fourth copy of the assertions.
    #[test]
    fn every_shader_and_its_buffer_layout_agree() {
        let pairs: &[(&str, &str, GpuBufferLayout, &str)] = &[
            (NODE_SDF_WGSL, "NodeInstance", node_buffer_layout(), "node"),
            (EDGE_WGSL, "EdgeSegment", edge_buffer_layout(), "edge"),
            (
                ARROWHEAD_WGSL,
                "Arrowhead",
                arrowhead_buffer_layout(),
                "arrowhead",
            ),
            (TEXT_ATLAS_WGSL, "TextQuad", text_buffer_layout(), "text"),
        ];

        for (source, struct_name, layout, label) in pairs {
            let shader = wgsl_struct_locations(source, struct_name);
            assert!(
                !shader.is_empty(),
                "{label}: no @location members parsed out of {struct_name}, so this pair is unchecked"
            );
            assert_eq!(
                shader.len(),
                layout.attributes.len(),
                "{label}: shader declares {} instance members, layout describes {}: {shader:?}",
                shader.len(),
                layout.attributes.len()
            );
            for attribute in layout.attributes {
                let (_, name, ty) = shader
                    .iter()
                    .find(|(location, _, _)| *location == attribute.shader_location)
                    .unwrap_or_else(|| {
                        panic!(
                            "{label}: layout has @location({}) and the shader does not: {shader:?}",
                            attribute.shader_location
                        )
                    });
                assert_eq!(
                    ty,
                    expected_wgsl_type(attribute.format),
                    "{label}: @location({}) `{name}` is `{ty}` in the shader but {:?} in the layout",
                    attribute.shader_location,
                    attribute.format
                );
            }
        }
    }

    #[test]
    fn the_node_shader_and_the_node_buffer_layout_agree() {
        let shader = wgsl_struct_locations(NODE_SDF_WGSL, "NodeInstance");
        let layout = node_buffer_layout();

        assert!(
            !shader.is_empty(),
            "no @location members were parsed out of NodeInstance, so this test proves nothing"
        );
        assert_eq!(
            shader.len(),
            layout.attributes.len(),
            "the shader declares {} instance members and the layout describes {}: {shader:?}",
            shader.len(),
            layout.attributes.len()
        );

        for attribute in layout.attributes {
            let (_, name, ty) = shader
                .iter()
                .find(|(location, _, _)| *location == attribute.shader_location)
                .unwrap_or_else(|| {
                    panic!(
                        "the layout has @location({}) and the shader does not: {shader:?}",
                        attribute.shader_location
                    )
                });
            assert_eq!(
                ty,
                expected_wgsl_type(attribute.format),
                "@location({}) `{name}` is `{ty}` in the shader but {:?} in the layout",
                attribute.shader_location,
                attribute.format
            );
        }
    }

    /// CONTROL: the parser finds what is actually there, and is not fooled by the OTHER struct.
    ///
    /// If `wgsl_struct_locations` silently returned an empty list -- a renamed struct, a reformatted
    /// source -- the test above would still pass its per-attribute loop while checking nothing, which is
    /// the vacuous-gate failure this project has hit repeatedly. Pinning the known members makes that
    /// impossible, and pinning `VertexOut` separately proves the scoping works: it also starts at
    /// `@location(0)` and would corrupt a whole-file scan.
    #[test]
    fn the_wgsl_parser_reads_the_struct_it_was_asked_for() {
        let instance = wgsl_struct_locations(NODE_SDF_WGSL, "NodeInstance");
        let names: Vec<&str> = instance.iter().map(|(_, name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "center",
                "half_extent",
                "fill",
                "stroke",
                "shape",
                "node_index",
                "stroke_width"
            ],
            "the NodeInstance members moved; update the layout table with them"
        );

        let vertex_out = wgsl_struct_locations(NODE_SDF_WGSL, "VertexOut");
        assert!(
            vertex_out.iter().any(|(location, _, _)| *location == 0),
            "VertexOut also numbers from @location(0), which is exactly why the scan is struct-scoped"
        );
    }

    /// Every attribute must fit inside the instance it describes.
    ///
    /// An attribute running past the stride makes the device read the NEXT instance's bytes, which
    /// renders a diagram whose every element is shifted by one — a picture wrong in a way that looks
    /// like a layout bug rather than a buffer bug.
    fn assert_attributes_fit(layout: GpuBufferLayout, name: &str) {
        assert!(layout.stride > 0, "{name}: zero stride");
        for attribute in layout.attributes {
            let end = attribute.offset + attribute.format.size_bytes();
            assert!(
                end <= layout.stride,
                "{name}: attribute {} ends at {end}, past the {} byte stride",
                attribute.field,
                layout.stride
            );
        }
    }

    /// No two attributes may overlap.
    ///
    /// Overlap means two shader locations read the same bytes, so one of them is silently wrong.
    /// Checked by sorting on offset rather than by trusting declaration order.
    fn assert_no_overlap(layout: GpuBufferLayout, name: &str) {
        let mut spans: Vec<(usize, usize, &str)> = layout
            .attributes
            .iter()
            .map(|a| (a.offset, a.offset + a.format.size_bytes(), a.field))
            .collect();
        spans.sort_unstable();
        for pair in spans.windows(2) {
            let [(_, prev_end, prev_field), (next_start, _, next_field)] = pair else {
                continue;
            };
            assert!(
                prev_end <= next_start,
                "{name}: {prev_field} overlaps {next_field} ({prev_end} > {next_start})"
            );
        }
    }

    /// Shader locations must be unique and contiguous from zero.
    ///
    /// A gap is not a compile error in WGSL; it is a location nothing writes, so the shader reads
    /// undefined data for that attribute.
    fn assert_locations_dense(layout: GpuBufferLayout, name: &str) {
        let mut locations: Vec<u32> = layout
            .attributes
            .iter()
            .map(|a| a.shader_location)
            .collect();
        locations.sort_unstable();
        for (index, location) in locations.iter().enumerate() {
            let expected = u32::try_from(index).unwrap_or(u32::MAX);
            assert_eq!(
                *location, expected,
                "{name}: shader locations are not dense from 0: {locations:?}"
            );
        }
    }

    /// Each declared location must actually appear in the shader that consumes the buffer.
    ///
    /// This is the join the whole module exists for: the Rust struct, this descriptor and the WGSL
    /// are three independent statements of one layout, and nothing else compares them.
    fn assert_shader_declares_locations(layout: GpuBufferLayout, wgsl: &str, name: &str) {
        for attribute in layout.attributes {
            let needle = format!("@location({})", attribute.shader_location);
            assert!(
                wgsl.contains(&needle),
                "{name}: the shader has no {needle} for attribute {}",
                attribute.field
            );
        }
    }

    #[test]
    fn every_buffer_layout_is_internally_consistent() {
        for (layout, wgsl, name) in [
            (node_buffer_layout(), NODE_SDF_WGSL, "node"),
            (edge_buffer_layout(), EDGE_WGSL, "edge"),
            (arrowhead_buffer_layout(), ARROWHEAD_WGSL, "arrowhead"),
            (text_buffer_layout(), TEXT_ATLAS_WGSL, "text"),
        ] {
            assert_attributes_fit(layout, name);
            assert_no_overlap(layout, name);
            assert_locations_dense(layout, name);
            assert_shader_declares_locations(layout, wgsl, name);
        }
    }

    /// The declared attributes must account for the whole instance, give or take tail padding.
    ///
    /// Catches a field ADDED to an instance struct and never described here: the struct grows, the
    /// descriptor does not, and the new field is simply never uploaded. Nothing else would notice —
    /// the code compiles, the buffer uploads, and the shader reads a field it was never given.
    #[test]
    fn the_attributes_cover_the_whole_instance() {
        for (layout, name) in [
            (node_buffer_layout(), "node"),
            (edge_buffer_layout(), "edge"),
            (arrowhead_buffer_layout(), "arrowhead"),
            (text_buffer_layout(), "text"),
        ] {
            let described: usize = layout
                .attributes
                .iter()
                .map(|a| a.format.size_bytes())
                .sum();
            assert!(
                described + layout.align > layout.stride,
                "{name}: attributes describe {described} of {} bytes — a field is missing from the \
                 descriptor and would never reach the GPU",
                layout.stride
            );
            assert!(
                described <= layout.stride,
                "{name}: attributes describe {described} bytes, more than the {} byte instance",
                layout.stride
            );
        }
    }

    /// The arrowhead descriptor, field by field — NOT by byte total (bd-2u0.2).
    ///
    /// The defect this pins shipped on main behind four green sibling gates. `kind` was absent and
    /// `color` sat at `@location(4)` in its place, so the shader's `kind: u32` read the first four
    /// bytes of an RGBA vec4 as an integer — a marker whose FORM was chosen by a colour channel —
    /// while `color` and `fill` were never uploaded at all.
    ///
    /// ⚠️ WHY A BYTE SUM COULD NOT BE TRUSTED TO CATCH IT. `the_attributes_cover_the_whole_instance`
    /// compares a SUM OF SIZES against the stride. Two missing 4-byte fields and one spurious
    /// 8-byte one sum identically, and the layout passes while describing the wrong interface. This
    /// asserts the (location, field, offset, format) tuples themselves, so agreement has to be in
    /// the CONTENT and not merely in the total.
    ///
    /// `assert_locations_dense` and `assert_shader_declares_locations` both passed throughout: 0..4
    /// IS dense, and the shader DOES declare a location 4. Density and existence are not identity.
    #[test]
    fn the_arrowhead_descriptor_names_every_field_of_its_instance() {
        let expected: &[(u32, &str, usize, GpuVertexFormat)] = &[
            (
                0,
                "position",
                offset_of!(GpuArrowheadInstance, position),
                GpuVertexFormat::Float32x2,
            ),
            (
                1,
                "angle",
                offset_of!(GpuArrowheadInstance, angle),
                GpuVertexFormat::Float32,
            ),
            (
                2,
                "size",
                offset_of!(GpuArrowheadInstance, size),
                GpuVertexFormat::Float32,
            ),
            (
                3,
                "edge_index",
                offset_of!(GpuArrowheadInstance, edge_index),
                GpuVertexFormat::Uint32,
            ),
            (
                4,
                "kind",
                offset_of!(GpuArrowheadInstance, kind),
                GpuVertexFormat::Uint32,
            ),
            (
                5,
                "color",
                offset_of!(GpuArrowheadInstance, color),
                GpuVertexFormat::Float32x4,
            ),
            (
                6,
                "fill",
                offset_of!(GpuArrowheadInstance, fill),
                GpuVertexFormat::Float32x4,
            ),
        ];

        let layout = arrowhead_buffer_layout();
        assert_eq!(
            layout.attributes.len(),
            expected.len(),
            "the arrowhead descriptor describes {} of {} instance fields",
            layout.attributes.len(),
            expected.len()
        );
        for (location, field, offset, format) in expected {
            let attribute = layout
                .attributes
                .iter()
                .find(|a| a.shader_location == *location)
                .unwrap_or_else(|| panic!("no attribute holds @location({location}) for `{field}`"));
            assert_eq!(
                attribute.field, *field,
                "@location({location}) describes `{}` where the shader reads `{field}`",
                attribute.field
            );
            assert_eq!(
                attribute.offset, *offset,
                "`{field}` is described at byte {} but sits at byte {offset}",
                attribute.offset
            );
            assert_eq!(
                attribute.format, *format,
                "`{field}` is described as {:?} but the shader reads {format:?}",
                attribute.format
            );
        }
    }

    /// CONTROL: no descriptor may name the same field twice.
    ///
    /// This is the other half of the byte-sum blind spot. A duplicated entry keeps the total right
    /// and the locations dense while one real field goes undescribed — which is very close to how
    /// the arrowhead descriptor was wrong in the first place.
    #[test]
    fn no_layout_describes_the_same_field_twice() {
        for (layout, name) in [
            (node_buffer_layout(), "node"),
            (edge_buffer_layout(), "edge"),
            (arrowhead_buffer_layout(), "arrowhead"),
            (text_buffer_layout(), "text"),
        ] {
            for (index, attribute) in layout.attributes.iter().enumerate() {
                let duplicate = layout.attributes[..index]
                    .iter()
                    .find(|earlier| earlier.field == attribute.field);
                assert!(
                    duplicate.is_none(),
                    "{name}: `{}` is described twice, so some other field is described not at all",
                    attribute.field
                );
            }
        }
    }
}
