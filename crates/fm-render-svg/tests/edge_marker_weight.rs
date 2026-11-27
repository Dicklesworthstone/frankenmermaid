//! A flowchart link's WEIGHT and its END MARKER are independent, and the SVG must carry both.
//!
//! mermaid's link grammar is a product: a head marker, a body run that sets the stroke weight, and
//! a tail marker. `A ==o B` is a thick line ending in a circle; `A -.-o B` is a dotted one. The
//! parser learned the whole product in bd-lrl48, and this is the renderer half — the half the
//! bead was originally filed for, "o==o / x==x render a solid stroke".
//!
//! Three things went wrong at once and each needs its own assertion:
//!
//! 1. The stroke-width match listed only `ThickArrow` / `DoubleThickArrow` / `ThickLine`, so every
//!    `==` body that ended in a circle or a cross fell through to the default 1.8. The `==` was
//!    parsed, stored, and then thrown away at the last step.
//! 2. The style-class match likewise fell through to `fm-edge-solid`, so the edge was never
//!    classed thick. Since f1c2fe2 `fm-edge-thick` is a RULE-LESS weight marker: there is no
//!    `.fm-edge-thick{}` rule and there must not be one — the thick weight lives in the resolved
//!    inline `stroke-width`, which a class-level CSS constant would override (the keeps_its_rules
//!    contract). This test asserts the inline weight is present and the rule is absent.
//! 3. The streaming path spelled the dotted class `fm-edge-dotted`, WHICH IS DECLARED NOWHERE. It
//!    was a class name with no rule behind it: the dashes came from the inline `stroke-dasharray`
//!    alone, and the `.fm-animations-enabled .fm-edge-dashed` flow animation skipped those edges.

use fm_render_svg::{SvgRenderConfig, render_svg_with_config};

fn render(source: &str) -> String {
    render_svg_with_config(&fm_parser::parse(source).ir, &SvgRenderConfig::default())
}

/// The `<path>` that IS the edge, as a string, so the assertions read against the markup that
/// actually shipped rather than against a whole document.
///
/// Anchored on `<path`, not on the first `class="fm-edge`: the edge is wrapped in a
/// `<g class="fm-edge">` that carries neither the width nor the style class, and matching that
/// wrapper made every assertion below fail against markup that was already correct. The CONTROL
/// assertions in each test are what caught it.
fn edge_element(svg: &str) -> String {
    let group = svg
        .find("id=\"fm-edge-0\"")
        .unwrap_or_else(|| panic!("no edge in:\n{svg}"));
    let start = group + svg[group..].find("<path").expect("the edge is a path");
    let end = svg[start..].find("/>").expect("the path closes");
    svg[start..start + end + 2].to_string()
}

#[test]
fn a_thick_body_stays_thick_whatever_marker_ends_it() {
    // The reference: `==>`, whose weight was never in question.
    let reference = edge_element(&render("flowchart LR\n  A ==> B\n"));
    assert!(
        reference.contains("stroke-width=\"2.50\""),
        "CONTROL: `==>` is the yardstick and must itself be 2.5: {reference}"
    );

    for source in [
        "flowchart LR\n  A ==o B\n",
        "flowchart LR\n  A ==x B\n",
        "flowchart LR\n  A o==o B\n",
        "flowchart LR\n  A x==x B\n",
    ] {
        let element = edge_element(&render(source));
        assert!(
            element.contains("stroke-width=\"2.50\""),
            "{source:?} lost its `==` weight: {element}"
        );
        assert!(
            element.contains("fm-edge-thick"),
            "{source:?} is not classed thick, so `.fm-edge-thick` cannot reach it: {element}"
        );
    }
}

#[test]
fn a_dotted_body_stays_dotted_whatever_marker_ends_it() {
    let reference = edge_element(&render("flowchart LR\n  A -.-> B\n"));
    assert!(
        reference.contains("fm-edge-dashed"),
        "CONTROL: `-.->` is the yardstick and must itself be dashed: {reference}"
    );

    for source in [
        "flowchart LR\n  A -.-o B\n",
        "flowchart LR\n  A o-.-o B\n",
        "flowchart LR\n  A x-.-x B\n",
    ] {
        let element = edge_element(&render(source));
        assert!(
            element.contains("fm-edge-dashed"),
            "{source:?} lost its dotted class: {element}"
        );
    }
}

/// A class the stylesheet never declares is dead markup, and a substring assertion on the edge
/// element alone would not catch it — the class would still be "present", just inert. Assert
/// against the WHOLE document, which contains the `<style>` too.
#[test]
fn no_edge_carries_a_class_the_stylesheet_never_declares() {
    for source in [
        "flowchart LR\n  A -.-o B\n",
        "flowchart LR\n  A o-.-o B\n",
        "flowchart LR\n  A x-.-x B\n",
        "flowchart LR\n  A ==o B\n",
    ] {
        let svg = render(source);
        assert!(
            !svg.contains("fm-edge-dotted"),
            "{source:?} emits `fm-edge-dotted`, which no stylesheet in this repo declares"
        );
        // `fm-edge-dashed` is theme-styled: if an edge carries it, the stylesheet must declare it.
        if svg.contains("class=\"fm-edge fm-edge-dashed") || svg.contains(" fm-edge-dashed\"") {
            assert!(
                svg.contains(".fm-edge-dashed{"),
                "{source:?} uses `fm-edge-dashed` but the stylesheet block declaring it was stripped"
            );
        }
        // Since f1c2fe2, `fm-edge-thick` is a RULE-LESS weight marker: the thick weight lives in the
        // resolved inline `stroke-width`, and a class-level `.fm-edge-thick{}` constant would override
        // a proportional/sankey width (the keeps_its_rules contract). Assert the inline weight is
        // present AND that no `.fm-edge-thick{` rule exists — never that one does.
        if svg.contains("class=\"fm-edge fm-edge-thick") || svg.contains(" fm-edge-thick\"") {
            assert!(
                edge_element(&svg).contains("stroke-width=\"2.50\""),
                "{source:?} is classed thick but lost its resolved inline weight: {svg}"
            );
            assert!(
                !svg.contains(".fm-edge-thick{"),
                "{source:?} reintroduced a `.fm-edge-thick{{` constant that would override the resolved inline width (f1c2fe2 / keeps_its_rules)"
            );
        }
    }
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    tag.split_once(&format!(" {name}=\""))?
        .1
        .split_once('"')
        .map(|(value, _)| value)
}

fn edge_path(svg: &str, index: usize) -> &str {
    let index = index.to_string();
    svg.split('<')
        .filter_map(|fragment| fragment.split_once('>').map(|(tag, _)| tag))
        .find(|tag| {
            tag.starts_with("path ")
                && (attribute(tag, "data-fm-edge-id") == Some(index.as_str())
                    || (attribute(tag, "data-fm-source-kind") == Some("edge")
                        && attribute(tag, "data-fm-source-index") == Some(index.as_str())))
        })
        .unwrap_or_else(|| panic!("missing edge {index} in {svg}"))
}

fn referenced_marker<'a>(svg: &'a str, edge: &str, slot: &str) -> Option<&'a str> {
    let reference = attribute(edge, slot)?;
    let id = reference.strip_prefix("url(#")?.strip_suffix(')')?;
    Some(
        svg.split("<marker ")
            .skip(1)
            .find_map(|fragment| {
                let (marker, _) = fragment.split_once("</marker>")?;
                marker
                    .starts_with(&format!("id=\"{id}\""))
                    .then_some(marker)
            })
            .unwrap_or_else(|| panic!("dangling {slot}={reference} in {svg}")),
    )
}

fn marker_path_geometry(path: &str) -> (String, Vec<f32>) {
    // These marker fixtures use absolute commands and whitespace-separated coordinates. Compare
    // their geometry independently of whether an SVG backend writes a number as 3.5 or 3.50.
    (
        path.chars().filter(|c| c.is_ascii_alphabetic()).collect(),
        path.split(|c: char| c.is_ascii_alphabetic() || c.is_ascii_whitespace() || c == ',')
            .filter(|value| !value.is_empty())
            .map(|value| value.parse().expect("numeric marker coordinate"))
            .collect(),
    )
}

#[test]
fn dependency_start_class_relations_render_both_authored_markers() {
    use fm_render_svg::SvgBackend;

    for (operator, start, end) in [
        ("<--*", "arrow-diamond", "arrow-end"),
        ("<--o", "arrow-diamond-open", "arrow-end"),
        ("<--|>", "start-arrow-triangle-open", "arrow-end"),
        ("<--()", "start-arrow-lollipop", "arrow-end"),
        ("<-->", "arrow-start", "arrow-end"),
        ("<..*", "arrow-diamond", "arrow-end"),
        ("<..o", "arrow-diamond-open", "arrow-end"),
        ("<..|>", "start-arrow-triangle-open", "arrow-end"),
        ("<..()", "start-arrow-lollipop", "arrow-end"),
        ("<..>", "arrow-start", "arrow-end"),
    ] {
        let ir = fm_parser::parse(&format!(
            "classDiagram\nAlpha \"one\" {operator} \"many\" Beta : uses\n"
        ))
        .ir;
        for backend in [SvgBackend::LegacyLayout, SvgBackend::Scene] {
            let svg = render_svg_with_config(
                &ir,
                &SvgRenderConfig {
                    backend,
                    ..Default::default()
                },
            );
            let edge = edge_path(&svg, 0);
            for (slot, marker) in [("marker-start", start), ("marker-end", end)] {
                let reference =
                    attribute(edge, slot).expect("both authored markers are referenced");
                if backend == SvgBackend::LegacyLayout {
                    assert_eq!(reference, format!("url(#{marker})"), "{operator}: {edge}");
                } else {
                    // Scene IDs include resolved paint and endpoint identity. Its source arrow
                    // uses the same base name as a target arrow, with reversed marker orientation.
                    let base = if marker == "arrow-start" {
                        "arrow-end"
                    } else {
                        marker
                    };
                    assert!(reference.starts_with("url(#fm-scene-"), "{reference}");
                    assert!(
                        reference.ends_with(&format!("-{base})")),
                        "{operator}: {reference}"
                    );
                }
                let definition = referenced_marker(&svg, edge, slot)
                    .expect("each authored marker has a definition");
                let (ref_x, orient, geometry, hollow) = match marker {
                    "arrow-start" => (
                        8.0,
                        "auto-start-reverse",
                        "M0 0 L8 3.5 L0 7 L2 3.5 Z",
                        false,
                    ),
                    "arrow-end" => (8.0, "auto", "M0 0 L8 3.5 L0 7 L2 3.5 Z", false),
                    "arrow-diamond" => (8.0, "auto-start-reverse", "M4 0 L8 4 L4 8 L0 4 Z", false),
                    "arrow-diamond-open" => {
                        (8.0, "auto-start-reverse", "M4 0 L8 4 L4 8 L0 4 Z", true)
                    }
                    "start-arrow-triangle-open" => {
                        (10.0, "auto-start-reverse", "M0 0 L10 5 L0 10 Z", true)
                    }
                    "start-arrow-lollipop" => {
                        (13.0, "auto", "M13 7 A6 6 0 0 1 1 7 A6 6 0 0 1 13 7 Z", true)
                    }
                    _ => unreachable!("all class endpoint shapes are covered"),
                };
                assert_eq!(
                    attribute(definition, "orient"),
                    Some(orient),
                    "{operator}/{backend:?}: {definition}"
                );
                assert_eq!(
                    attribute(definition, "refX")
                        .unwrap()
                        .parse::<f32>()
                        .unwrap(),
                    ref_x,
                    "{operator}/{backend:?}: {definition}"
                );
                assert_eq!(
                    marker_path_geometry(attribute(definition, "d").expect("marker path")),
                    marker_path_geometry(geometry),
                    "{operator}/{backend:?}: {definition}"
                );
                assert_eq!(
                    attribute(definition, "fill").unwrap() == "none",
                    hollow,
                    "{operator}/{backend:?}: {definition}"
                );
            }
            let dashed = attribute(edge, "stroke-dasharray")
                .or_else(|| style_property(edge, "stroke-dasharray"));
            assert_eq!(
                dashed.is_some(),
                operator.contains(".."),
                "{operator}/{backend:?}: {edge}"
            );
        }
    }
}

fn style_property<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    attribute(tag, "style")?
        .split(';')
        .filter_map(|declaration| declaration.trim().split_once(':'))
        .filter(|(property, _)| *property == name)
        .map(|(_, value)| value.trim())
        .next_back()
}

fn marker_opacity(marker: &str) -> f32 {
    marker
        .split('<')
        .skip(1)
        .find_map(|tag| attribute(tag, "opacity"))
        .map_or(1.0, |value| value.parse().expect("numeric marker opacity"))
}

#[test]
fn link_styles_paint_each_endpoint_independently_in_both_svg_backends() {
    use fm_render_svg::SvgBackend;

    let ir = fm_parser::parse(
        "flowchart LR\nA -->|request| B\nA -.-> C\nA --o D\nA --x E\n\
         linkStyle default stroke:#112233,stroke-opacity:50%\n\
         linkStyle 1 stroke:#aa0000,stroke-opacity:25%\n\
         linkStyle 2 stroke:none\nlinkStyle 3 stroke-opacity:0\n",
    )
    .ir;
    assert_eq!(ir.edges.len(), 4);
    for backend in [SvgBackend::LegacyLayout, SvgBackend::Scene] {
        for embed_theme_css in [false, true] {
            let config = SvgRenderConfig {
                backend,
                embed_theme_css,
                ..Default::default()
            };
            let svg = render_svg_with_config(&ir, &config);
            for (index, color, opacity) in [(0, "#112233", 0.5_f32), (1, "#aa0000", 0.25)] {
                let marker = referenced_marker(&svg, edge_path(&svg, index), "marker-end")
                    .expect("the authored direction survives styling");
                assert!(marker.contains(&format!("fill=\"{color}\"")), "{marker}");
                assert!((marker_opacity(marker) - opacity).abs() < 0.001, "{marker}");
            }
            let disabled = edge_path(&svg, 2);
            assert_eq!(attribute(disabled, "marker-start"), None, "{disabled}");
            assert_eq!(attribute(disabled, "marker-end"), None, "{disabled}");
            let invisible = edge_path(&svg, 3);
            if let Some(marker) = referenced_marker(&svg, invisible, "marker-end") {
                assert_eq!(marker_opacity(marker), 0.0, "{marker}");
            }
            for marker in svg.split("<marker ").skip(1) {
                let id = marker
                    .strip_prefix("id=\"")
                    .unwrap()
                    .split('"')
                    .next()
                    .unwrap();
                assert!(
                    svg.contains(&format!("url(#{id})")),
                    "unused theme marker survived its authored replacement: {id}"
                );
            }
            assert_eq!(svg, render_svg_with_config(&ir, &config));
        }
    }
}

#[test]
fn marker_stroke_opacity_and_edge_opacity_compose_once() {
    use fm_render_svg::SvgBackend;

    for arrow in ["-->", "--x"] {
        let ir = fm_parser::parse(&format!(
            "flowchart LR\nA {arrow} B\n\
             linkStyle 0 stroke:#b03456,opacity:0.4,stroke-opacity:0.5,fill-opacity:0.1\n"
        ))
        .ir;
        for backend in [SvgBackend::LegacyLayout, SvgBackend::Scene] {
            for embed_theme_css in [false, true] {
                let svg = render_svg_with_config(
                    &ir,
                    &SvgRenderConfig {
                        backend,
                        embed_theme_css,
                        ..Default::default()
                    },
                );
                let path = edge_path(&svg, 0);
                let marker = referenced_marker(&svg, path, "marker-end").unwrap();
                let paint = if arrow == "-->" { "fill" } else { "stroke" };
                assert!(marker.contains(&format!("{paint}=\"#b03456\"")), "{marker}");
                let edge_opacity =
                    style_property(path, "opacity").map_or(1.0_f32, |value| value.parse().unwrap());
                assert!(
                    (marker_opacity(marker) * edge_opacity - 0.2).abs() < 0.001,
                    "marker and shaft must share effective opacity, independently of fill-opacity: {path} {marker}"
                );
                if backend == SvgBackend::LegacyLayout {
                    assert_eq!(marker_opacity(marker), 0.5, "{marker}");
                    assert_eq!(edge_opacity, 0.4, "{path}");
                } else {
                    assert_eq!(
                        edge_opacity, 1.0,
                        "resolved opacity was applied again: {path}"
                    );
                }
            }
        }
    }
}

#[test]
fn styled_uml_and_er_relationships_keep_both_endpoint_shapes() {
    use fm_render_svg::SvgBackend;

    for (source, start, end, user_space) in [
        (
            "classDiagram\nA o--|> B : contract\n",
            "arrow-diamond-open",
            "arrow-triangle-open",
            false,
        ),
        (
            "erDiagram\nA ||--o{ B : owns\n",
            "er-onlyOneStart",
            "er-zeroOrMoreEnd",
            true,
        ),
    ] {
        let mut ir = fm_parser::parse(source).ir;
        assert_eq!(ir.edges.len(), 1, "{source}");
        ir.edges[0].inline_style = Some(Box::new(fm_core::parse_style_string(
            "stroke:#5e2381,stroke-opacity:0.6,opacity:0.5",
        )));
        for backend in [SvgBackend::LegacyLayout, SvgBackend::Scene] {
            for embed_theme_css in [false, true] {
                let svg = render_svg_with_config(
                    &ir,
                    &SvgRenderConfig {
                        backend,
                        embed_theme_css,
                        ..Default::default()
                    },
                );
                let path = edge_path(&svg, 0);
                for (slot, shape) in [("marker-start", start), ("marker-end", end)] {
                    let marker = referenced_marker(&svg, path, slot).unwrap();
                    assert!(
                        attribute(path, slot)
                            .unwrap()
                            .ends_with(&format!("{shape})"))
                    );
                    assert!(marker.contains("stroke=\"#5e2381\""), "{marker}");
                    let expected_opacity = if backend == SvgBackend::LegacyLayout {
                        0.6
                    } else {
                        0.3
                    };
                    assert!(
                        (marker_opacity(marker) - expected_opacity).abs() < 0.001,
                        "{marker}"
                    );
                    if user_space {
                        assert!(
                            marker.contains("markerUnits=\"userSpaceOnUse\""),
                            "{marker}"
                        );
                    }
                }
                if user_space {
                    let marker = referenced_marker(&svg, path, "marker-end").unwrap();
                    assert!(
                        marker.contains("<circle"),
                        "zero cardinality lost its bubble"
                    );
                    assert!(
                        marker.contains("fill=\"#ffffff\""),
                        "the shaft must not cross the bubble"
                    );
                }
            }
        }
    }
}

#[test]
fn styled_sankey_arrowheads_keep_fixed_size_and_source_palette_when_only_alpha_changes() {
    let mut ir = fm_parser::parse("sankey-beta\nSource,Small,2\nSource,Large,20\n").ir;
    for edge in &mut ir.edges {
        edge.inline_style = Some(Box::new(fm_core::parse_style_string("stroke-opacity:0.5")));
    }
    for embed_theme_css in [false, true] {
        let svg = render_svg_with_config(
            &ir,
            &SvgRenderConfig {
                embed_theme_css,
                ..Default::default()
            },
        );
        for index in 0..2 {
            let path = edge_path(&svg, index);
            let color = style_property(path, "stroke").expect("source category paint");
            let marker = referenced_marker(&svg, path, "marker-end").unwrap();
            assert!(marker.contains(&format!("fill=\"{color}\"")), "{marker}");
            assert!(
                marker.contains("markerUnits=\"userSpaceOnUse\""),
                "{marker}"
            );
            for (name, expected) in [("markerWidth", 32.0_f32), ("markerHeight", 28.0)] {
                let size: f32 = attribute(marker, name).unwrap().parse().unwrap();
                assert!((size - expected).abs() < f32::EPSILON, "{marker}");
            }
            assert_eq!(marker_opacity(marker), 0.5, "{marker}");
        }
    }
}

#[test]
fn labels_and_unrelated_styles_do_not_replace_the_shared_markers() {
    for style in [
        "",
        "linkStyle 0 color:#aabbcc,font-weight:700,fill-opacity:0.2\n",
    ] {
        let svg = render(&format!("flowchart LR\nA -->|caption| B\n{style}"));
        let path = edge_path(&svg, 0);
        assert_eq!(attribute(path, "marker-end"), Some("url(#arrow-end)"));
        assert!(!svg.contains("fm-edge-paint-"));
    }
}

#[test]
fn zero_width_connectors_do_not_leave_fixed_size_endpoint_glyphs() {
    use fm_render_svg::SvgBackend;

    for source in [
        "flowchart LR\nA <--> B\n",
        "classDiagram\nA o--|> B : contract\n",
        "erDiagram\nA ||--o{ B : owns\n",
        "sankey-beta\nSource,Target,20\n",
    ] {
        for width in ["0", "0px"] {
            let mut ir = fm_parser::parse(source).ir;
            assert_eq!(ir.edges.len(), 1, "{source}");
            ir.edges[0].inline_style = Some(Box::new(fm_core::parse_style_string(&format!(
                "stroke-width:{width}"
            ))));
            for backend in [SvgBackend::LegacyLayout, SvgBackend::Scene] {
                for embed_theme_css in [false, true] {
                    let svg = render_svg_with_config(
                        &ir,
                        &SvgRenderConfig {
                            backend,
                            embed_theme_css,
                            ..Default::default()
                        },
                    );
                    let path = edge_path(&svg, 0);
                    assert_eq!(attribute(path, "marker-start"), None, "{path}");
                    assert_eq!(attribute(path, "marker-end"), None, "{path}");
                }
            }
        }
    }
}

#[test]
fn marker_paints_reject_untrusted_resource_and_declaration_values() {
    let default_color = fm_render_svg::Theme::from_preset(fm_render_svg::ThemePreset::Default)
        .colors
        .edge;
    for value in [
        "url(https://untrusted.example/paint.svg#x)",
        "#aabbcc;opacity:0",
        "red\"/><script>untrusted</script>",
    ] {
        let mut ir = fm_parser::parse("flowchart LR\nA --> B\n").ir;
        // Structured IR is public too; deliberately bypass the parser's sanitization here.
        ir.edges[0].inline_style = Some(Box::new(fm_core::IrInlineStyle {
            properties: [
                ("stroke".to_owned(), value.to_owned()),
                ("stroke-opacity".to_owned(), "0.5".to_owned()),
            ]
            .into_iter()
            .collect(),
        }));
        let svg = render_svg_with_config(&ir, &SvgRenderConfig::default());
        let marker = referenced_marker(&svg, edge_path(&svg, 0), "marker-end").unwrap();
        assert!(!marker.contains("untrusted"), "{marker}");
        assert!(!marker.contains("<script"), "{marker}");
        assert!(!marker.contains("opacity:0"), "{marker}");
        assert!(
            marker.contains(&format!("fill=\"{default_color}\"")),
            "{marker}"
        );
        assert_eq!(marker_opacity(marker), 0.5, "{marker}");
    }
}

#[test]
fn custom_marker_paints_preserve_every_arrow_shape_and_feedback_orientation() {
    use fm_core::ArrowType;

    let arrows = [
        ArrowType::Line,
        ArrowType::Arrow,
        ArrowType::OpenArrow,
        ArrowType::HalfArrowTop,
        ArrowType::HalfArrowBottom,
        ArrowType::HalfArrowTopReverse,
        ArrowType::HalfArrowBottomReverse,
        ArrowType::StickArrowTop,
        ArrowType::StickArrowBottom,
        ArrowType::StickArrowTopReverse,
        ArrowType::StickArrowBottomReverse,
        ArrowType::ThickArrow,
        ArrowType::DottedArrow,
        ArrowType::DottedOpenArrow,
        ArrowType::DottedCross,
        ArrowType::HalfArrowTopDotted,
        ArrowType::HalfArrowBottomDotted,
        ArrowType::HalfArrowTopReverseDotted,
        ArrowType::HalfArrowBottomReverseDotted,
        ArrowType::StickArrowTopDotted,
        ArrowType::StickArrowBottomDotted,
        ArrowType::StickArrowTopReverseDotted,
        ArrowType::StickArrowBottomReverseDotted,
        ArrowType::Circle,
        ArrowType::Cross,
        ArrowType::ThickLine,
        ArrowType::DottedLine,
        ArrowType::DoubleArrow,
        ArrowType::DoubleThickArrow,
        ArrowType::DoubleDottedArrow,
        ArrowType::CircleBoth,
        ArrowType::CrossBoth,
        ArrowType::ThickCircleBoth,
        ArrowType::ThickCrossBoth,
        ArrowType::ThickCircle,
        ArrowType::ThickCross,
        ArrowType::DottedCircle,
        ArrowType::DottedCircleBoth,
        ArrowType::DottedCrossBoth,
        ArrowType::Aggregation,
        ArrowType::AggregationReverse,
        ArrowType::Composition,
        ArrowType::CompositionReverse,
        ArrowType::Inheritance,
        ArrowType::InheritanceReverse,
        ArrowType::Lollipop,
        ArrowType::LollipopReverse,
    ];
    let config = SvgRenderConfig::default();
    for arrow in arrows {
        for reversed in [false, true] {
            let mut ir = fm_parser::parse("flowchart LR\nA --> B\n").ir;
            ir.edges[0].arrow = arrow;
            let mut layout = fm_layout::layout_diagram(&ir);
            layout.edges[0].reversed = reversed;
            let original = fm_render_svg::render_svg_with_layout(&ir, &layout, &config);
            ir.edges[0].inline_style = Some(Box::new(fm_core::parse_style_string(
                "stroke:#b03456,stroke-opacity:0.4",
            )));
            let styled = fm_render_svg::render_svg_with_layout(&ir, &layout, &config);
            for slot in ["marker-start", "marker-end"] {
                let before = referenced_marker(&original, edge_path(&original, 0), slot);
                let after = referenced_marker(&styled, edge_path(&styled, 0), slot);
                assert_eq!(
                    before.is_some(),
                    after.is_some(),
                    "{arrow:?} {reversed} {slot}"
                );
                let (Some(before), Some(after)) = (before, after) else {
                    continue;
                };
                for name in [
                    "markerWidth",
                    "markerHeight",
                    "refX",
                    "refY",
                    "orient",
                    "markerUnits",
                ] {
                    assert_eq!(
                        attribute(before, name),
                        attribute(after, name),
                        "paint must not change {name}: {arrow:?} {reversed} {slot}"
                    );
                }
                let paths = |marker: &str| {
                    marker
                        .split('<')
                        .filter_map(|tag| attribute(tag, "d"))
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                };
                assert_eq!(paths(before), paths(after), "{arrow:?} {reversed} {slot}");
                assert_eq!(
                    before.contains("fill=\"none\""),
                    after.contains("fill=\"none\""),
                    "hollow and filled markers have different meanings: {arrow:?}"
                );
                assert!(after.contains("#b03456"), "{arrow:?} {after}");
                assert_eq!(marker_opacity(after), 0.4, "{after}");
            }
        }
    }
}
