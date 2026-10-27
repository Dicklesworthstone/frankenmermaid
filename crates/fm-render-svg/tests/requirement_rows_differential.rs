//! Differential test: the field rows and relationship labels mermaid draws in a requirement diagram.
//!
//! FOUR DIVERGENCES, all found the same way and all measured in a real browser.
//!
//! ```text
//!   risk + verification   we drew ONE row `Risk: High | Verify: Test`; mermaid draws TWO
//!   the verification label  we wrote `Verify:`; mermaid writes `Verification:`
//!   the doc-ref label       we wrote `Doc:`;    mermaid writes `Doc Ref:`
//!   an element's header     we drew `<<element>>`; mermaid draws the literal `<<Element>>`
//!   a relationship label    we drew `satisfies`;  mermaid draws `<<satisfies>>`
//! ```
//!
//! REFERENCE, from the pinned 11.15.0 bundle's requirement renderer. Each field is its own line and
//! nothing is ever joined:
//!
//! ```text
//!   u ? $u(m, `<<${n.type}>>`, 0, …) : $u(m, "<<Element>>", 0, …)
//!   $u(m, n.name, y, … "; font-weight: bold;")
//!   `ID: ${n.requirementId}`   `Text: ${n.text}`
//!   `Risk: ${n.risk}`          `Verification: ${n.verifyMethod}`
//!   `Type: ${a.type}`          `Doc Ref: ${a.docRef}`
//! ```
//!
//! and its relationship edges carry `` label: `<<${n.type}>>`, classes: "relationshipLine" ``.
//!
//! ⚠️ HOW THESE WERE FOUND, because it is the reusable part. The requirement family has NO
//! head-to-head corpus item and its renderer will not run under jsdom, so `equivalence.mjs` has
//! nothing to compare and `drawn_text_diff.mjs` reports INCUMBENT-DNF. Both cheap oracles are blind
//! here. `scripts/headtohead/chromium_text_diff.mjs` renders the pinned bundle in real Chromium over
//! CDP and diffs the drawn text as a multiset; it took `requirement_basic` from four divergences to
//! `AGREE 17 runs`.

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
        // ⚠️ STRIP MARKUP FIRST, UNESCAPE SECOND. Unescaping `&lt;&lt;` into `<<` before removing
        // tags hands the tag stripper a `<<Requirement>>` that looks exactly like markup, and it
        // deletes the very runs this file exists to check — every `<<…>>` assertion then fails
        // against a renderer that is drawing them correctly. I wrote it the wrong way round first
        // and it accused two working fixes of being broken.
        let body = rest[open + 1..close].replace("</tspan><tspan", "\u{1}<tspan");
        let mut stripped = String::new();
        let mut in_tag = false;
        for ch in body.chars() {
            // ⚠️ `>` IS ONLY A DELIMITER INSIDE A TAG. The renderer escapes `<` (to `&lt;`) but emits
            // `>` literally, so a drawn `<<Element>>` reaches here as `&lt;&lt;Element>>`. Treating
            // every `>` as a tag close swallows the closing angles and yields `<<Element` — which
            // reads as a truncation bug in the renderer and is a bug in this reader.
            match ch {
                '<' => in_tag = true,
                '>' if in_tag => in_tag = false,
                _ if !in_tag => stripped.push(ch),
                _ => {}
            }
        }
        let text = stripped
            .replace('\u{1}', " ")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&");
        let text = text.trim().to_string();
        if !text.is_empty() {
            out.push(text);
        }
        rest = &rest[close + "</text>".len()..];
    }
    out
}

const DIAGRAM: &str = "requirementDiagram\n    requirement AuthReq {\n        id: \"REQ-001\"\n        text: Users must authenticate\n        risk: High\n        verifymethod: Test\n    }\n    element LoginModule {\n        type: Module\n        docref: spec.md\n    }\n    LoginModule - satisfies -> AuthReq\n";

#[test]
fn every_field_is_its_own_row_with_mermaids_label() {
    let drawn = runs(DIAGRAM);
    for expected in [
        "ID: REQ-001",
        "Text: Users must authenticate",
        "Risk: High",
        "Verification: Test",
        "Type: Module",
        "Doc Ref: spec.md",
    ] {
        assert!(
            drawn.iter().any(|run| run == expected),
            "{expected:?} is not drawn as its own row; drew {drawn:?}"
        );
    }
}

/// ⚠️ THE NEGATIVE CONTROL FOR THE FUSED ROW, and the defect exactly as it shipped.
///
/// `Risk: High | Verify: Test` satisfies "the risk reached the drawing" and "the verify method
/// reached the drawing" — a substring check for either would pass on it. What it never satisfies is
/// mermaid drawing them as two independent lines with no separator between them.
#[test]
fn risk_and_verification_are_never_joined_into_one_row() {
    let drawn = runs(DIAGRAM);
    assert!(
        !drawn.iter().any(|run| run.contains('|')),
        "a row still carries the `|` separator mermaid never draws: {drawn:?}"
    );
    assert!(
        !drawn.iter().any(|run| run.contains("Verify:")),
        "the label is `Verification:`, not the abbreviation we invented: {drawn:?}"
    );
    assert!(
        !drawn
            .iter()
            .any(|run| run.starts_with("Doc: ") || run.contains("| Verify")),
        "the label is `Doc Ref:`, not `Doc:`: {drawn:?}"
    );
}

/// ⚠️ CONTROL FOR THE ELEMENT HEADER. mermaid hardcodes a capitalised literal here rather than
/// echoing the `element` keyword the author typed — the same machine-token-reaching-a-reader defect
/// the requirement TYPE header had.
#[test]
fn an_element_draws_mermaids_capitalised_literal() {
    let drawn = runs(DIAGRAM);
    assert!(
        drawn.iter().any(|run| run == "<<Element>>"),
        "an element must draw the literal <<Element>>; drew {drawn:?}"
    );
    assert!(
        !drawn.iter().any(|run| run == "<<element>>"),
        "the authored keyword reached the drawing: {drawn:?}"
    );
}

/// ⚠️ CONTROL FOR THE RELATIONSHIP LABEL. mermaid stores the WRAPPED string on the edge, so a bare
/// `satisfies` is a divergence even though the word itself is right.
#[test]
fn a_relationship_label_is_wrapped_in_angles() {
    let drawn = runs(DIAGRAM);
    assert!(
        drawn.iter().any(|run| run == "<<satisfies>>"),
        "the relationship must draw <<satisfies>>; drew {drawn:?}"
    );
    assert!(
        !drawn.iter().any(|run| run == "satisfies"),
        "the bare relationship keyword reached the drawing: {drawn:?}"
    );
}

/// CONTROL ON ABSENCE: a field the author did not declare must produce NO row.
///
/// Guards the opposite failure — emitting `Risk: ` or `Doc Ref: ` with an empty value — which every
/// assertion above would tolerate. mermaid guards each row on its own field
/// (`${n.risk ? ... : ""}`), and an empty label is worse than a missing one because it reads as a
/// declared-but-blank field.
#[test]
fn an_undeclared_field_draws_no_row() {
    let sparse =
        "requirementDiagram\n    requirement Bare {\n        id: \"R\"\n        text: t\n    }\n";
    let drawn = runs(sparse);
    for absent in ["Risk:", "Verification:", "Type:", "Doc Ref:"] {
        assert!(
            !drawn.iter().any(|run| run.starts_with(absent)),
            "{absent:?} was drawn for a requirement that never declared it: {drawn:?}"
        );
    }
    // Non-vacuity: the rows that WERE declared are still there, so the check above is not passing
    // because nothing renders at all.
    assert!(
        drawn.iter().any(|run| run == "ID: R"),
        "the declared rows vanished too, so this test proves nothing: {drawn:?}"
    );
}

fn numeric_attr(tag: &str, name: &str) -> f32 {
    let key = format!("{name}=\"");
    tag.strip_prefix(&key)
        .or_else(|| tag.split_once(&format!(" {key}")).map(|(_, value)| value))
        .unwrap_or_else(|| panic!("missing {name} in {tag}"))
        .split('"')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn requirement_rows_stay_separate_and_inside_the_drawn_box_in_both_svg_paths() {
    let ir = fm_parser::parse(DIAGRAM).ir;
    for font_size in [15.0, 24.0] {
        for embed_theme_css in [true, false] {
            let config = fm_render_svg::SvgRenderConfig {
                font_size,
                avg_char_width: font_size / 2.0,
                preserve_labels: true,
                embed_theme_css,
                ..Default::default()
            };
            let svg = fm_render_svg::render_svg_with_config(&ir, &config);
            for (node, expected_rows) in [("AuthReq", 6), ("LoginModule", 4)] {
                let group = svg
                    .split_once(&format!("data-id=\"{node}\""))
                    .unwrap()
                    .1
                    .split_once("</g>")
                    .unwrap()
                    .0;
                let rect = group
                    .split_once("<rect")
                    .unwrap()
                    .1
                    .split('>')
                    .next()
                    .unwrap();
                let top = numeric_attr(rect, "y");
                let bottom = top + numeric_attr(rect, "height");
                let mut previous_bottom = top;
                let mut rows = 0;
                for text in group.split("<text ").skip(1) {
                    let tag = text.split('>').next().unwrap();
                    let y = numeric_attr(tag, "y");
                    let size = numeric_attr(tag, "font-size");
                    // Subtitle coordinates are central; the main name uses an alphabetic baseline.
                    // Include ascenders and descenders rather than merely checking baseline points.
                    let (row_top, row_bottom) = if tag.contains("dominant-baseline=\"central\"") {
                        (y - size * 0.6, y + size * 0.6)
                    } else {
                        (y - size * 0.85, y + size * 0.25)
                    };
                    assert!(
                        row_top >= previous_bottom,
                        "{node}: row overlaps its predecessor at {font_size}px: {tag}"
                    );
                    assert!(
                        row_bottom <= bottom,
                        "{node}: row escapes the rectangle at {font_size}px: {tag}"
                    );
                    previous_bottom = row_bottom;
                    rows += 1;
                }
                assert_eq!(
                    rows, expected_rows,
                    "every declared row must be drawn for {node}"
                );
            }
        }
    }
}

fn decoded_text(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn card_rows(svg: &str) -> Vec<(&str, String)> {
    svg.split("<text ")
        .skip(1)
        .map(|fragment| {
            let (tag, body) = fragment.split_once('>').expect("text opening tag");
            let body = body.split_once("</text>").expect("text closing tag").0;
            assert!(
                !body.contains('<'),
                "a measured card row must be one text run"
            );
            (tag, decoded_text(body))
        })
        .collect()
}

fn field_rows(svg: &str, class: &str) -> Vec<String> {
    svg.split(&format!("class=\"{class}"))
        .skip(1)
        .map(|fragment| {
            let body = fragment.split_once('>').expect("row or group tag").1;
            // The scene backend carries the semantic class on the enclosing row group.
            let body = if body.starts_with("<text ") {
                body.split_once('>').expect("scene row text tag").1
            } else {
                body
            };
            decoded_text(body.split_once("</text>").expect("row text end").0)
        })
        .collect()
}

#[test]
fn long_requirement_fields_are_lossless_and_share_geometry_between_svg_backends() {
    let mut ir = fm_parser::parse(DIAGRAM).ir;
    let requirement = ir
        .nodes
        .iter()
        .position(|node| node.id == "AuthReq")
        .unwrap();
    let element = ir
        .nodes
        .iter()
        .position(|node| node.id == "LoginModule")
        .unwrap();
    // No edge caption in this measurement: every text run belongs to one of the two cards.
    ir.edges.clear();
    ir.graph.edges.clear();
    let name = "Safety  assurance for café equipment and 👩‍🔬 operators over repeated observations";
    let text = "The system  must preserve every authored word, including café, 🦀, and unusually_long_unbroken_identifiers_that_need_a_grapheme_safe_wrap.\nThe second  paragraph remains a separate line.";
    let docref = "specifications/安全性/repeated  measurements/very_long_document_reference_that_must_remain_complete.md";
    let name_id = fm_core::IrLabelId(ir.labels.len());
    ir.labels.push(fm_core::IrLabel {
        text: name.to_owned(),
        span: ir.nodes[requirement].span_primary,
    });
    ir.nodes[requirement].label = Some(name_id);
    ir.nodes[requirement]
        .requirement_meta
        .as_mut()
        .unwrap()
        .text = Some(text.to_owned());
    ir.nodes[element].requirement_meta.as_mut().unwrap().doc_ref = Some(docref.to_owned());

    for embed_theme_css in [false, true] {
        let mut reference: Option<Vec<(String, f32, f32, f32)>> = None;
        for backend in [
            fm_render_svg::SvgBackend::LegacyLayout,
            fm_render_svg::SvgBackend::Scene,
        ] {
            let config = fm_render_svg::SvgRenderConfig {
                backend,
                embed_theme_css,
                font_size: 20.0,
                font_family: "monospace".to_owned(),
                padding: 0.0,
                ..Default::default()
            };
            let svg = fm_render_svg::render_svg_with_config(&ir, &config);
            assert_eq!(
                field_rows(&svg, "fm-req-text").concat(),
                format!("Text: {}", text.replace('\n', ""))
            );
            assert_eq!(
                field_rows(&svg, "fm-req-docref").concat(),
                format!("Doc Ref: {docref}")
            );
            assert!(
                field_rows(&svg, "fm-req-text").len() > 3,
                "the requirement must actually wrap"
            );
            assert!(field_rows(&svg, "fm-req-name").concat().starts_with(name));
            let viewbox: Vec<f32> = svg
                .split_once("viewBox=\"")
                .unwrap()
                .1
                .split('"')
                .next()
                .unwrap()
                .split_whitespace()
                .map(|value| value.parse().unwrap())
                .collect();
            let rows: Vec<_> = card_rows(&svg)
                .into_iter()
                .map(|(tag, text)| {
                    assert!(tag.contains("xml:space=\"preserve\""));
                    assert!(!text.contains('…'), "authored fields cannot be truncated");
                    (
                        text,
                        numeric_attr(tag, "x") - viewbox[0],
                        numeric_attr(tag, "y") - viewbox[1],
                        numeric_attr(tag, "font-size"),
                    )
                })
                .collect();
            if let Some(reference) = &reference {
                assert_eq!(
                    reference.len(),
                    rows.len(),
                    "both SVG paths must render every row"
                );
                for (before, after) in reference.iter().zip(&rows) {
                    assert_eq!(before.0, after.0);
                    assert!(
                        (before.1 - after.1).abs() < 0.02,
                        "row x changed: {before:?} / {after:?}"
                    );
                    assert!(
                        (before.2 - after.2).abs() < 0.02,
                        "row y changed: {before:?} / {after:?}"
                    );
                    assert_eq!(before.3, after.3, "row font size changed");
                }
            } else {
                reference = Some(rows);
            }
        }
    }
}

fn inline_property<'a>(tag: &'a str, property: &str) -> Option<&'a str> {
    tag.split_once(" style=\"")?
        .1
        .split('"')
        .next()?
        .split(';')
        .filter_map(|entry| entry.trim().split_once(':'))
        .filter(|(key, _)| *key == property)
        .map(|(_, value)| value.trim())
        .next_back()
}

#[test]
fn requirement_rows_honor_author_color_font_and_opacity_in_both_backends() {
    let source = "requirementDiagram\n  requirement Safety {\n    id: REQ-42\n    text: Preserve every field\n    risk: high\n    verifymethod: test\n  }\n  style Safety fill:#102030,color:#fedcba,font-size:24px,font-family:monospace,opacity:0.4\n";
    let ir = fm_parser::parse(source).ir;
    for backend in [
        fm_render_svg::SvgBackend::LegacyLayout,
        fm_render_svg::SvgBackend::Scene,
    ] {
        for embed_theme_css in [false, true] {
            let config = fm_render_svg::SvgRenderConfig {
                backend,
                embed_theme_css,
                ..Default::default()
            };
            let svg = fm_render_svg::render_svg_with_config(&ir, &config);
            let rows = card_rows(&svg);
            assert_eq!(rows.len(), 6, "all short fields must remain visible");
            for (tag, _) in rows {
                assert_eq!(numeric_attr(tag, "font-size"), 24.0);
                assert_eq!(inline_property(tag, "fill"), Some("#fedcba"));
                assert_eq!(inline_property(tag, "font-family"), Some("monospace"));
                assert_eq!(inline_property(tag, "opacity"), Some("0.4"));
                assert_eq!(inline_property(tag, "font-size"), Some("24px!important"));
                if tag.contains(" fill-opacity=\"") {
                    assert_eq!(
                        numeric_attr(tag, "fill-opacity"),
                        1.0,
                        "node opacity must be applied once"
                    );
                }
            }
            assert!(svg.contains("fill: #102030") || svg.contains("fill:#102030"));
            if backend == fm_render_svg::SvgBackend::LegacyLayout {
                let marker = svg.find("data-id=\"Safety\"").unwrap();
                let start = svg[..marker].rfind("<g ").unwrap();
                let group = &svg[start..];
                let opening = group.split('>').next().unwrap();
                assert_eq!(inline_property(opening, "opacity"), None);
                assert!(!opening.contains(" opacity=\""));
                let divider = group
                    .split_once("<line ")
                    .unwrap()
                    .1
                    .split('>')
                    .next()
                    .unwrap();
                assert_eq!(inline_property(divider, "opacity"), Some("0.4"));
                assert!(!divider.contains(" stroke-opacity=\""));
            }
            for fragment in svg.split("<path ").skip(1) {
                let tag = fragment.split('>').next().unwrap();
                if tag.contains("data-fm-source-kind=\"node\"") {
                    assert_eq!(inline_property(tag, "opacity"), Some("0.4"));
                    if tag.contains(" fill-opacity=\"") {
                        assert_eq!(numeric_attr(tag, "fill-opacity"), 1.0);
                    }
                    if tag.contains(" stroke-opacity=\"") {
                        assert_eq!(numeric_attr(tag, "stroke-opacity"), 1.0);
                    }
                }
            }
        }
    }
}

#[test]
fn requirement_risk_cards_remain_readable_on_dark_themes_with_stroke_only_styles() {
    let ir = fm_parser::parse("requirementDiagram\n  requirement Safety {\n    id: REQ-42\n    text: Read every field\n    risk: high\n    verifymethod: test\n  }\n  style Safety stroke:#abcdef\n").ir;
    let luminance = |color: &str| {
        let hex = color
            .strip_prefix('#')
            .expect("resolved opaque theme paint");
        assert_eq!(hex.len(), 6);
        let channel = |offset| {
            let value =
                f32::from(u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap()) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4)
    };
    for backend in [
        fm_render_svg::SvgBackend::LegacyLayout,
        fm_render_svg::SvgBackend::Scene,
    ] {
        for embed_theme_css in [false, true] {
            let config = fm_render_svg::SvgRenderConfig {
                backend,
                embed_theme_css,
                theme: fm_render_svg::ThemePreset::Dark,
                ..Default::default()
            };
            let svg = fm_render_svg::render_svg_with_config(&ir, &config);
            assert!(
                svg.contains("#fca5a5"),
                "a stroke-only style must retain the risk fill"
            );
            let rows = card_rows(&svg);
            assert_eq!(rows.len(), 6);
            for (tag, text) in rows {
                let color = inline_property(tag, "fill").unwrap_or_else(|| {
                    tag.split_once("fill=\"")
                        .unwrap()
                        .1
                        .split('"')
                        .next()
                        .unwrap()
                });
                let foreground = luminance(color);
                let background = luminance("#fca5a5");
                let contrast =
                    (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05);
                assert!(
                    contrast >= 4.5,
                    "unreadable requirement row {text:?}: {color}, ratio {contrast}"
                );
                assert_ne!(inline_property(tag, "opacity"), Some("0.7"));
            }
        }
    }
}

#[test]
fn compact_precomputed_requirement_layout_keeps_default_render_rows_inside_its_box() {
    let ir = fm_parser::parse(
        "requirementDiagram\n  requirement Safety {\n    id: REQ-42\n    text: Preserve every field\n    risk: high\n    verifymethod: test\n  }\n",
    )
    .ir;
    let config = fm_render_svg::SvgRenderConfig {
        padding: 0.0,
        ..Default::default()
    };
    let metrics = fm_core::FontMetrics::new(fm_core::FontMetricsConfig {
        line_height: 1.0,
        ..config.font_metrics().config().clone()
    });
    let layout = fm_layout::layout_diagram_with_config(
        &ir,
        fm_layout::LayoutConfig {
            font_metrics: Some(metrics),
            ..Default::default()
        },
    );
    let bounds = layout.nodes[0].bounds;
    let top = bounds.y - layout.bounds.y;
    let bottom = top + bounds.height;
    for backend in [
        fm_render_svg::SvgBackend::LegacyLayout,
        fm_render_svg::SvgBackend::Scene,
    ] {
        let config = fm_render_svg::SvgRenderConfig {
            backend,
            ..config.clone()
        };
        let svg = fm_render_svg::render_svg_with_layout(&ir, &layout, &config);
        let view_y: f32 = svg
            .split_once("viewBox=\"")
            .unwrap()
            .1
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        let rows = card_rows(&svg);
        assert_eq!(rows.len(), 6, "no field may be omitted to fit the card");
        let mut previous_bottom = top;
        for (tag, text) in rows {
            let y = numeric_attr(tag, "y") - view_y;
            let size = numeric_attr(tag, "font-size");
            let expected_size = if text == "Safety" {
                config.font_size
            } else {
                config.font_size * 0.85
            };
            assert!(
                (size - expected_size).abs() < 0.01,
                "fitting must not shrink font sizes"
            );
            assert!(
                y - size * 0.6 >= previous_bottom - 0.01,
                "row overlap: {text}"
            );
            assert!(y + size * 0.6 <= bottom + 0.01, "row outside card: {text}");
            previous_bottom = y + size * 0.6;
        }
    }
}
