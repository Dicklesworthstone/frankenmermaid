//! Differential test: how mermaid labels a `packet-beta` field's bit range.
//!
//! THE DIVERGENCE THIS PINS. mermaid draws each field as THREE text runs — the name, the first bit
//! and the last bit — so the numbers read as a scale beside the field. We baked the range into the
//! name and drew one run. Found by `scripts/headtohead/drawn_text_diff.mjs`:
//!
//! ```text
//!   mermaid draws, we do NOT: ["Source Port","0","15","Destination Port","16","31", …]
//!   we draw, mermaid does NOT: ["Source Port\n[0-15]","Destination Port\n[16-31]", …]
//! ```
//!
//! REFERENCE BEHAVIOUR, probed against the pinned 11.15.0 bundle:
//!
//! ```text
//!   0-3: "A"      ->  ["A","0","3"]     name, then start, then end
//!   0: "Flag"     ->  ["Flag","0"]      ONE number when the field is a single bit
//! ```
//!
//! ⚠️ THE SINGLE-BIT CASE IS THE DISCRIMINATING ONE. Emitting both ends unconditionally prints `0`
//! twice on every one-bit flag, and every multi-bit fixture agrees with that wrong rule — including
//! the project's own `packet_basic`, whose eight fields all span more than one bit.
//!
//! ⚠️ THE SAME CHANGE HAD TO STOP ELLIPSIZING THESE LABELS. `fit_node_label_text` shrinks a label to
//! 10px and then cuts it, which is right for a node whose box was sized for its text and wrong for a
//! packet field, whose width is fixed at `bits x bitWidth` by the diagram's own semantics. A one-bit
//! field drew `Fl…` at 10px where mermaid draws `Flag` and lets it overflow. Losing the field's NAME
//! to keep it inside a box the author never chose is the worse trade — the name is the only thing
//! identifying the field — so packet labels are exempt from the fit.

/// Every `<text>` leaf, in document order.
fn runs(source: &str) -> Vec<String> {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(source).ir);
    let mut out = Vec::new();
    let mut rest = svg.as_str();
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        let Some(open) = rest.find('>') else { break };
        let Some(close) = rest.find("</text>") else {
            break;
        };
        let text = rest[open + 1..close].trim();
        if !text.is_empty() && !text.contains('<') {
            out.push(text.to_string());
        }
        rest = &rest[close + "</text>".len()..];
    }
    out
}

fn bit_marks(svg: &str) -> Vec<(&str, &str)> {
    svg.split("<text")
        .skip(1)
        .filter_map(|part| part.split_once('>'))
        .filter(|(tag, _)| tag.contains("fm-packet-bit"))
        .map(|(tag, body)| (tag, body.split_once("</text>").expect("closed bit label").0))
        .collect()
}

fn numeric_attribute(tag: &str, name: &str) -> f32 {
    tag.split_once(&format!(" {name}=\""))
        .unwrap_or_else(|| panic!("missing {name} in {tag}"))
        .1
        .split('"')
        .next()
        .expect("attribute value")
        .parse()
        .expect("finite numeric attribute")
}

#[test]
fn every_packet_row_marks_only_the_bits_that_it_contains() {
    use fm_render_svg::{SvgBackend, SvgRenderConfig, render_svg_with_config};

    for (range, expected) in [
        ("28-70", vec!["28", "31", "32", "63", "64", "70"]),
        ("31-32", vec!["31", "32"]),
        ("0-31", vec!["0", "31"]),
        ("4294967294-4294967295", vec!["4294967294", "4294967295"]),
    ] {
        let ir = fm_parser::parse(&format!("packet-beta\n{range}: \"Payload\"\n")).ir;
        for backend in [SvgBackend::LegacyLayout, SvgBackend::Scene] {
            for embed_theme_css in [true, false] {
                let config = SvgRenderConfig {
                    backend,
                    embed_theme_css,
                    include_source_spans: true,
                    ..Default::default()
                };
                let svg = render_svg_with_config(&ir, &config);
                let marks = bit_marks(&svg);
                assert_eq!(
                    marks.iter().map(|(_, bit)| *bit).collect::<Vec<_>>(),
                    expected,
                    "{backend:?}, {range}"
                );
                for (tag, _) in &marks {
                    assert!(tag.contains("data-fm-source-kind=\"node\""));
                    assert!(tag.contains("data-fm-source-index=\"0\""));
                    assert!(tag.contains("data-fm-source-span="));
                    assert!(tag.contains("aria-hidden=\"true\""));
                }
                if range == "28-70" {
                    let rows: Vec<_> = marks
                        .iter()
                        .map(|(tag, _)| numeric_attribute(tag, "y"))
                        .collect();
                    assert_eq!(rows[0], rows[1]);
                    assert_eq!(rows[2], rows[3]);
                    assert_eq!(rows[4], rows[5]);
                    assert!(rows[0] < rows[2] && rows[2] < rows[4]);
                }
                assert_eq!(svg, render_svg_with_config(&ir, &config));
            }
        }
    }
}

#[test]
fn packet_scale_keeps_configured_typography_and_contrast() {
    use fm_render_svg::{SvgBackend, SvgRenderConfig, ThemePreset, render_svg_with_config};

    let mut ir = fm_parser::parse("packet-beta\n28-70: \"Payload\"\n").ir;
    ir.style_refs.push(fm_core::IrStyleRef {
        target: fm_core::IrStyleTarget::Node(fm_core::IrNodeId(0)),
        style: "font-size:40px,font-family:serif,color:#ff0000,opacity:0.2".to_owned(),
        span: fm_core::Span::default(),
    });
    for backend in [SvgBackend::LegacyLayout, SvgBackend::Scene] {
        for font_size in [8.0, 15.0, 32.0] {
            let config = SvgRenderConfig {
                backend,
                font_size,
                theme: ThemePreset::Dark,
                ..Default::default()
            };
            let svg = render_svg_with_config(&ir, &config);
            let marks = bit_marks(&svg);
            assert_eq!(marks.len(), 6);
            let color = fm_render_svg::ThemeColors::from_preset(ThemePreset::Dark).text;
            for (tag, _) in marks {
                assert_eq!(
                    numeric_attribute(tag, "font-size"),
                    (font_size - 4.0).max(1.0)
                );
                assert!(tag.contains(&format!("fill=\"{color}\"")));
                assert!(!tag.contains("font-size:40") && !tag.contains("font-family:serif"));
                assert!(!tag.contains("#ff0000") && !tag.contains("opacity:0.2"));
            }
        }
    }
}

#[test]
fn a_field_draws_its_name_and_both_bit_ends() {
    let drawn = runs("packet-beta\n0-3: \"A\"\n4-7: \"B\"\n");
    for expected in ["A", "0", "3", "B", "4", "7"] {
        assert!(
            drawn.iter().any(|run| run == expected),
            "{expected:?} is missing from {drawn:?}"
        );
    }
}

/// ⚠️ NEGATIVE CONTROL for the OLD shape: the range must not be baked into the name.
#[test]
fn the_range_is_not_part_of_the_name() {
    let drawn = runs("packet-beta\n0-3: \"A\"\n4-7: \"B\"\n");
    assert!(
        !drawn.iter().any(|run| run.contains('[')),
        "a field name still carries its bracketed range: {drawn:?}"
    );
    assert!(
        drawn.iter().any(|run| run == "A"),
        "the bare field name is gone entirely: {drawn:?}"
    );
}

/// ⚠️ THE DISCRIMINATING CASE. A single-bit field gets ONE number. An implementation that always
/// emits both ends passes every multi-bit fixture — including `packet_basic`, whose eight fields all
/// span more than one bit — and prints `0` twice here.
#[test]
fn a_single_bit_field_is_marked_once() {
    let drawn = runs("packet-beta\n0: \"Flag\"\n1-7: \"Rest\"\n");
    let zeroes = drawn.iter().filter(|run| *run == "0").count();
    assert_eq!(
        zeroes, 1,
        "bit 0 is a single-bit field and must be marked once, not at both ends: {drawn:?}"
    );
    // ...and the neighbouring multi-bit field still gets both of its ends.
    for expected in ["1", "7"] {
        assert!(
            drawn.iter().any(|run| run == expected),
            "the multi-bit field lost its {expected:?} marking: {drawn:?}"
        );
    }
}

/// CONTROL: no other diagram type gains bit markings. A pass that is not gated on the packet meta
/// would stamp stray numbers on every diagram.
#[test]
fn only_packet_diagrams_gain_bit_markings() {
    let svg = fm_render_svg::render_svg(&fm_parser::parse("flowchart LR\n  A-->B\n").ir);
    assert!(
        !svg.contains("fm-packet-bit"),
        "a flowchart emitted packet bit markings"
    );
}

/// ⚠️ A ONE-BIT FIELD KEEPS ITS WHOLE NAME. Its box is one bit wide, so the generic fit shrank the
/// label to 10px and then ellipsized it to `Fl…`. mermaid draws `Flag` and lets it overflow; a box
/// width dictated by the protocol is not a reason to destroy the field's identity.
#[test]
fn a_narrow_field_is_not_ellipsized() {
    let drawn = runs("packet-beta\n0: \"Flag\"\n1-7: \"Rest\"\n");
    assert!(
        drawn.iter().any(|run| run == "Flag"),
        "the one-bit field's name was cut to fit its box: {drawn:?}"
    );
    assert!(
        !drawn.iter().any(|run| run.contains('\u{2026}')),
        "a packet field label was ellipsized: {drawn:?}"
    );
}

/// CONTROL: the exemption is scoped to packet. A long flowchart label in a small box must still be
/// fitted, or this would have traded one family's correctness for every other family's layout.
#[test]
fn other_diagrams_still_fit_their_labels() {
    let svg = fm_render_svg::render_svg(
        &fm_parser::parse(
            "flowchart LR\n  A[\"an extremely long node label that cannot possibly fit\"]-->B\n",
        )
        .ir,
    );
    assert!(
        svg.contains('\u{2026}') || svg.contains("<tspan"),
        "a long flowchart label was neither wrapped nor ellipsized, so the packet exemption leaked"
    );
}

#[test]
fn scene_backend_draws_the_name_on_every_packet_field_segment() {
    use fm_render_svg::{SvgBackend, SvgRenderConfig, render_svg_with_config};

    let config = SvgRenderConfig {
        backend: SvgBackend::Scene,
        embed_theme_css: false,
        ..Default::default()
    };
    for (start, end, segments) in [(0, 63, 2), (28, 70, 3), (31, 32, 2), (0, 31, 1)] {
        let ir = fm_parser::parse(&format!(
            "packet-beta\n{start}-{end}: \"Payload\"\n{}: \"Flag\"\n",
            end + 1
        ))
        .ir;
        let svg = render_svg_with_config(&ir, &config);
        assert_eq!(
            svg.matches(">Payload</text>").count(),
            segments,
            "the field must retain its identity on every row for bits {start}-{end}"
        );
        assert_eq!(svg.matches(">Flag</text>").count(), 1);
        assert_eq!(svg, render_svg_with_config(&ir, &config));
    }
}

#[test]
fn scene_backend_announces_each_packet_field_once_with_all_segments_visible() {
    use fm_render_svg::{SvgBackend, SvgRenderConfig, render_svg_with_config};

    let config = SvgRenderConfig {
        backend: SvgBackend::Scene,
        embed_theme_css: false,
        include_source_spans: true,
        ..Default::default()
    };
    let mut ir = fm_parser::parse("packet-beta\n28-70: \"Payload<br/>Details\"\n71: \"Flag\"\n").ir;
    // Pinned Mermaid's packet renderer uses `.text(block.label)`: HTML-like text is literal.
    // Keep that authored content intact, then supply a multiline IR label explicitly to check
    // that shared-scene consumers retain every row without duplicating accessible field names.
    let literal_svg = render_svg_with_config(&ir, &config);
    assert_eq!(
        literal_svg
            .matches(">Payload&lt;br/>Details</text>")
            .count(),
        3
    );
    let field_label = ir.nodes[0].label.expect("packet field label");
    ir.labels[field_label.0].text = "Payload\nDetails".to_owned();
    let svg = render_svg_with_config(&ir, &config);
    let tags: Vec<_> = svg
        .split('<')
        .filter_map(|part| part.split_once('>').map(|(tag, _)| tag))
        .collect();
    let fields = &ir.packet_meta.as_ref().expect("packet metadata").fields;
    for (field, segments, label_rows) in [(&fields[0], 3, 2), (&fields[1], 1, 1)] {
        let source = format!("data-fm-source-index=\"{}\"", field.node.0);
        let elements: Vec<_> = tags
            .iter()
            .copied()
            .filter(|tag| tag.contains("data-fm-source-kind=\"node\"") && tag.contains(&source))
            .collect();
        assert_eq!(
            elements
                .iter()
                .filter(|tag| tag.starts_with("path "))
                .count(),
            segments,
            "every field segment remains drawn and attributed to its node"
        );
        assert_eq!(
            elements
                .iter()
                .filter(|tag| tag.starts_with("text ") && !tag.contains("fm-packet-bit"))
                .count(),
            segments * label_rows,
            "every segment keeps all label rows and source metadata"
        );
        assert!(
            elements
                .iter()
                .all(|tag| tag.contains("data-fm-source-span="))
        );
        let announced: Vec<_> = elements
            .iter()
            .filter(|tag| {
                tag.contains("role=\"graphics-symbol\"") && !tag.contains("aria-hidden=\"true\"")
            })
            .collect();
        assert_eq!(
            announced.len(),
            1,
            "a field is announced once across all rows"
        );
        assert!(announced[0].starts_with("path "));
        assert!(announced[0].contains("aria-label="));
    }
    assert_eq!(svg.matches(">Payload</text>").count(), 3);
    assert_eq!(svg.matches(">Details</text>").count(), 3);
    assert_eq!(svg.matches(">Flag</text>").count(), 1);
    let mut ids = std::collections::BTreeSet::new();
    for tag in tags {
        if let Some((_, value)) = tag.split_once(" id=\"") {
            let id = value.split('"').next().expect("attribute value");
            assert!(ids.insert(id), "duplicate SVG id: {id}");
        }
    }

    let flowchart = render_svg_with_config(
        &fm_parser::parse("flowchart LR\nA[Visible label] --> B\n").ir,
        &config,
    );
    let flowchart_labels: Vec<_> = flowchart
        .split("<text")
        .skip(1)
        .filter_map(|part| part.split_once('>'))
        .filter(|(tag, _)| tag.contains("data-fm-source-kind=\"node\""))
        .collect();
    assert_eq!(flowchart_labels.len(), 2);
    assert!(
        flowchart_labels
            .iter()
            .all(|(tag, _)| !tag.contains("aria-hidden=\"true\"")),
        "packet deduplication must not change another diagram's label semantics"
    );
}
