//! The author's own styling must reach the canvas (bd-lvj3).
//!
//! Measured before the fix: the canvas emitted only 3-4 distinct fills for an ENTIRE diagram,
//! whatever the source declared. Counted at the same time — fm-render-svg reads `inline_style` 30
//! times, `classes` 19 and `style_refs` 11; this crate read all three ZERO times. The canvas path is
//! what fm-wasm ships to a browser, so a user who coloured their diagram saw it uncoloured in the
//! preview and correctly coloured in exported SVG — the two outputs disagreeing about the document.

use std::collections::BTreeMap;

use fm_core::MermaidDiagramIr;
use fm_render_canvas::{
    CanvasRenderConfig, DrawOperation, GpuRenderPlan, MockCanvas2dContext, parse_paint_rgba,
    render_to_canvas,
};
use fm_render_svg::{
    SvgBackend, SvgRenderConfig, ThemeColors, ThemePreset, render_svg_with_config,
};

const DEFAULT_CLASS_CASCADE: &str = "flowchart LR\n\
    A[Plain]\n\
    B[Named]:::accent\n\
    C[Direct]:::accent\n\
    classDef accent fill:#cc5500\n\
    classDef default fill:#112233,stroke:#445566,color:#778899,stroke-width:3px\n\
    style C fill:#abcdef,color:#123456\n";

#[derive(Debug, PartialEq)]
struct NodePaint {
    fill: String,
    stroke: String,
    width: f64,
    text: String,
}

/// Observe paint at the actual shape and label draws in the rectangular-node fixtures below.
fn painted_nodes(ir: &MermaidDiagramIr) -> BTreeMap<String, NodePaint> {
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(ir, &mut context, &CanvasRenderConfig::default());
    let mut fill = String::new();
    let mut stroke = String::new();
    let mut width = 0.0;
    let mut last_shape = None;
    let mut paints = BTreeMap::new();
    for operation in context.operations() {
        match operation {
            DrawOperation::SetFillStyle(value) => fill.clone_from(value),
            DrawOperation::SetStrokeStyle(value) => stroke.clone_from(value),
            DrawOperation::SetLineWidth(value) => width = *value,
            DrawOperation::Fill => {
                last_shape = Some((fill.clone(), stroke.clone(), width));
            }
            DrawOperation::FillText(label, _, _) => {
                if let Some((shape_fill, shape_stroke, shape_width)) = &last_shape {
                    paints.insert(
                        label.clone(),
                        NodePaint {
                            fill: shape_fill.clone(),
                            stroke: shape_stroke.clone(),
                            width: *shape_width,
                            text: fill.clone(),
                        },
                    );
                }
            }
            _ => {}
        }
    }
    paints
}

fn attribute<'a>(element: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!(" {name}=\"");
    let value = element.split_once(&prefix)?.1;
    Some(value.split_once('"')?.0)
}

fn scene_node_element<'a>(svg: &'a str, tag: &str, node_index: usize) -> &'a str {
    let index = node_index.to_string();
    svg.split(&format!("<{tag} "))
        .skip(1)
        .filter_map(|tail| tail.split_once('>').map(|(element, _)| element))
        .find(|element| {
            attribute(element, "data-fm-source-kind") == Some("node")
                && attribute(element, "data-fm-source-index") == Some(index.as_str())
        })
        .unwrap_or_else(|| panic!("missing scene {tag} for node {node_index}: {svg}"))
}

fn assert_style(element: &str, property: &str, value: &str) {
    let style = attribute(element, "style").unwrap_or_default();
    let expected = format!("{property}:{value}");
    assert!(
        style
            .split(';')
            .any(|declaration| declaration.trim() == expected),
        "expected {expected} on {element}"
    );
}

fn css_rule_declarations<'a>(svg: &'a str, selector: &str) -> BTreeMap<&'a str, &'a str> {
    let rules: Vec<_> = svg
        .split('}')
        .filter_map(|rule| rule.rsplit_once('{'))
        .filter(|(selectors, _)| selectors.split(',').any(|part| part.trim() == selector))
        .collect();
    assert_eq!(rules.len(), 1, "expected one rule for {selector}: {svg}");
    rules[0]
        .1
        .split(';')
        .filter_map(|declaration| declaration.split_once(':'))
        .map(|(property, value)| (property.trim(), value.trim()))
        .collect()
}

/// Fill styles a canvas run set, in order.
fn fill_styles(ops_debug: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = ops_debug;
    while let Some(i) = rest.find("SetFillStyle(\"") {
        rest = &rest[i + "SetFillStyle(\"".len()..];
        if let Some(end) = rest.find('"') {
            out.push(rest[..end].to_string());
            rest = &rest[end..];
        }
    }
    out
}

/// Stroke styles a canvas run set, in order.
fn stroke_styles(ops_debug: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = ops_debug;
    while let Some(i) = rest.find("SetStrokeStyle(\"") {
        rest = &rest[i + "SetStrokeStyle(\"".len()..];
        if let Some(end) = rest.find('"') {
            out.push(rest[..end].to_string());
            rest = &rest[end..];
        }
    }
    out
}

fn canvas_ops(source: &str) -> String {
    let ir = fm_parser::parse(source).ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    format!("{:?}", context.operations())
}

/// A `style` directive reaches the canvas.
#[test]
fn a_style_directive_colours_its_node() {
    let ops = canvas_ops("flowchart TD\n  a[A] --> b[B]\n  style a fill:#ff0000\n");
    let fills = fill_styles(&ops);

    assert!(
        fills.iter().any(|f| f.to_lowercase().contains("ff0000")),
        "the declared fill never reached the canvas: {fills:?}"
    );
}

/// A `classDef` reaches the canvas.
///
/// This is the channel fm-render-svg gets for free by emitting a CSS class and letting the BROWSER
/// cascade it. A canvas has no cascade, so the class has to be resolved in the renderer — which is
/// why the SVG helper could not simply be reused: it returns CSS strings, not colours.
#[test]
fn a_classdef_colours_the_nodes_that_name_it() {
    let ops = canvas_ops(
        "flowchart TD\n  a[A] --> b[B]\n  classDef hot fill:#ff0000,stroke:#00ff00\n  class a hot\n",
    );

    assert!(
        fill_styles(&ops)
            .iter()
            .any(|f| f.to_lowercase().contains("ff0000")),
        "the classDef fill never reached the canvas: {:?}",
        fill_styles(&ops)
    );
    assert!(
        stroke_styles(&ops)
            .iter()
            .any(|s| s.to_lowercase().contains("00ff00")),
        "the classDef stroke never reached the canvas: {:?}",
        stroke_styles(&ops)
    );
}

/// CONTROL: an UNSTYLED diagram keeps the theme defaults.
///
/// The resolver returns `None` per channel when nothing was declared, so the caller keeps the config
/// colour rather than being handed one the resolver invented. Without this, a bug that always
/// returned some colour would satisfy the tests above while repainting every diagram.
#[test]
fn an_unstyled_diagram_keeps_the_theme_colours() {
    let ops = canvas_ops("flowchart TD\n  a[A] --> b[B]\n");
    let fills = fill_styles(&ops);

    assert!(
        !fills.is_empty(),
        "nothing was filled, so this control proves nothing"
    );
    assert!(
        !fills.iter().any(|f| f.to_lowercase().contains("ff0000")),
        "an unstyled diagram acquired a colour from nowhere: {fills:?}"
    );
    assert!(
        fills
            .iter()
            .any(|f| f == &CanvasRenderConfig::default().node_fill),
        "the default node fill is missing: {fills:?}"
    );
}

/// CONTROL: only the NAMED node is styled.
///
/// `class a hot` styles `a` and not `b`. A resolver that ignored the target and applied every
/// declared style to every node would pass the positive tests above and be badly wrong.
#[test]
fn styling_applies_only_to_the_named_node() {
    let ops =
        canvas_ops("flowchart TD\n  a[A] --> b[B]\n  classDef hot fill:#ff0000\n  class a hot\n");
    let fills = fill_styles(&ops);

    let styled = fills
        .iter()
        .filter(|f| f.to_lowercase().contains("ff0000"))
        .count();
    let theme_fill = CanvasRenderConfig::default().node_fill;
    let default = fills.iter().filter(|f| **f == theme_fill).count();

    assert!(styled >= 1, "the styled node lost its colour: {fills:?}");
    assert!(
        default >= 1,
        "the UNstyled node did not keep the default fill, so the style leaked across nodes: \
         {fills:?}"
    );
}

#[test]
fn default_class_cascade_reaches_canvas_and_gpu_nodes() {
    for renderer_resolves_styles in [false, true] {
        let mut ir = fm_parser::parse(DEFAULT_CLASS_CASCADE).ir;
        if renderer_resolves_styles {
            // Structured IR callers need the same cascade even without the parser's derived map.
            for node in &mut ir.nodes {
                node.inline_style = None;
            }
        }
        let paints = painted_nodes(&ir);
        let layout = fm_layout::layout_diagram(&ir);
        let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.5);
        assert_eq!(plan.node_instances.len(), 3);
        for (id, label, fill, text) in [
            ("A", "Plain", "#112233", "#778899"),
            ("B", "Named", "#cc5500", "#778899"),
            ("C", "Direct", "#abcdef", "#123456"),
        ] {
            assert_eq!(
                paints.get(label),
                Some(&NodePaint {
                    fill: fill.to_string(),
                    stroke: "#445566".to_string(),
                    width: 3.0,
                    text: text.to_string(),
                }),
                "wrong canvas cascade for {id}, renderer resolves: {renderer_resolves_styles}"
            );
            let node_index = ir.nodes.iter().position(|node| node.id == id).unwrap();
            let instance = plan
                .node_instances
                .iter()
                .find(|node| node.node_index as usize == node_index)
                .unwrap();
            assert_eq!(instance.fill, parse_paint_rgba(fill).unwrap(), "{id}");
            assert_eq!(
                instance.stroke,
                parse_paint_rgba("#445566").unwrap(),
                "{id}"
            );
            assert_eq!(instance.stroke_width, 3.0, "{id}");
        }
    }
}

#[test]
fn repeated_default_definitions_merge_without_erasing_other_properties() {
    let ir = fm_parser::parse(
        "flowchart TD\nA[Alpha]\n\
         classDef default fill:#112233,stroke:#445566,color:#778899\n\
         classDef default fill:#abcdef,stroke-width:4px\n",
    )
    .ir;
    assert_eq!(
        painted_nodes(&ir).get("Alpha"),
        Some(&NodePaint {
            fill: "#abcdef".to_string(),
            stroke: "#445566".to_string(),
            width: 4.0,
            text: "#778899".to_string(),
        })
    );
}

#[test]
fn default_class_does_not_repaint_edges_or_subgraph_containers() {
    let source = "flowchart TD\nsubgraph box[Container]\nA[Alpha] --> B[Beta]\nend\n";
    let baseline = fm_parser::parse(source).ir;
    let styled = fm_parser::parse(&format!(
        "{source}classDef default fill:#112233,stroke:#445566,color:#778899,stroke-width:3px\n"
    ))
    .ir;
    let config = CanvasRenderConfig::default();
    let paints = painted_nodes(&styled);
    let container = paints.get("Container").expect("container was drawn");
    assert_eq!(container.fill, config.cluster_fill);
    assert_eq!(container.stroke, config.cluster_stroke);
    assert_ne!(container.text, "#778899");
    assert_eq!(paints["Alpha"].fill, "#112233");
    assert_eq!(paints["Beta"].fill, "#112233");

    // Reuse geometry to isolate author paint from layout changes.
    let layout = fm_layout::layout_diagram(&baseline);
    let baseline_plan = GpuRenderPlan::from_layout(&baseline, &layout, 1.5);
    let styled_plan = GpuRenderPlan::from_layout(&styled, &layout, 1.5);
    assert!(!baseline_plan.cluster_instances.is_empty());
    assert!(!baseline_plan.edge_segments.is_empty());
    assert_eq!(
        styled_plan.cluster_instances,
        baseline_plan.cluster_instances
    );
    assert_eq!(styled_plan.edge_segments, baseline_plan.edge_segments);
    assert_eq!(styled_plan.arrowheads, baseline_plan.arrowheads);
}

#[test]
fn default_class_styles_class_diagram_nodes() {
    let ir = fm_parser::parse(
        "classDiagram\nclass Animal\nclass Mineral\n\
         classDef default fill:#112233,stroke:#445566,color:#778899,stroke-width:3px\n",
    )
    .ir;
    let paints = painted_nodes(&ir);
    for name in ["Animal", "Mineral"] {
        assert_eq!(
            paints.get(name),
            Some(&NodePaint {
                fill: "#112233".to_string(),
                stroke: "#445566".to_string(),
                width: 3.0,
                text: "#778899".to_string(),
            }),
            "{name}"
        );
    }
}

/// Observe paints in effect when the public renderer actually fills and strokes a single node.
/// Labels do not count as shape fills, so text colors cannot make these assertions pass.
fn single_node_paints(source: &str, config: &CanvasRenderConfig) -> (String, String) {
    let ir = fm_parser::parse(source).ir;
    assert_eq!(ir.nodes.len(), 1, "fixture must parse exactly one node");
    assert!(
        ir.edges.is_empty(),
        "edge strokes would contaminate this probe"
    );
    let mut context = MockCanvas2dContext::new(800.0, 600.0);
    let result = render_to_canvas(&ir, &mut context, config);
    assert_eq!(result.nodes_drawn, 1);

    let mut fill = String::new();
    let mut stroke = String::new();
    let mut painted_fill = None;
    let mut painted_stroke = None;
    for operation in context.operations() {
        match operation {
            DrawOperation::SetFillStyle(color) => fill.clone_from(color),
            DrawOperation::SetStrokeStyle(color) => stroke.clone_from(color),
            DrawOperation::Fill | DrawOperation::FillRect(..) if painted_fill.is_none() => {
                painted_fill = Some(fill.clone());
            }
            DrawOperation::Stroke | DrawOperation::StrokeRect(..) if painted_stroke.is_none() => {
                painted_stroke = Some(stroke.clone());
            }
            _ => {}
        }
    }
    (
        painted_fill.expect("the node must actually be filled"),
        painted_stroke.expect("the node must actually be outlined"),
    )
}

fn canvas_theme(preset: ThemePreset) -> CanvasRenderConfig {
    let colors = ThemeColors::from_preset(preset);
    CanvasRenderConfig {
        node_fill: colors.node_fill,
        node_stroke: colors.node_stroke,
        edge_stroke: colors.edge,
        cluster_fill: colors.cluster_fill,
        cluster_stroke: colors.cluster_stroke,
        label_color: colors.text,
        accents: colors.accents,
        ..CanvasRenderConfig::default()
    }
}

#[test]
fn default_shape_roles_are_distinct_and_match_portable_svg_paints() {
    for preset in [ThemePreset::Default, ThemePreset::Dark] {
        let config = canvas_theme(preset);
        let mut roles = std::collections::BTreeSet::new();
        for declaration in [
            "A{Decision}",
            "A([Terminal])",
            "A[(Storage)]",
            "A@{shape: cloud, label: Cloud}",
        ] {
            let source = format!("flowchart TD\n{declaration}\n");
            let (fill, stroke) = single_node_paints(&source, &config);
            assert_ne!(
                fill, config.node_fill,
                "{declaration} must have a role surface"
            );
            assert_ne!(
                stroke, config.node_stroke,
                "{declaration} must have a role border"
            );
            roles.insert(fill.clone());

            let ir = fm_parser::parse(&source).ir;
            let svg = fm_render_svg::render_svg_with_config(
                &ir,
                &SvgRenderConfig {
                    theme: preset,
                    embed_theme_css: false,
                    ..SvgRenderConfig::default()
                },
            );
            assert!(
                svg.contains(&format!("fill=\"{fill}\"")),
                "{preset:?} {declaration}: portable SVG and Canvas must share the role fill"
            );
            assert!(
                svg.contains(&format!("stroke=\"{stroke}\"")),
                "{preset:?} {declaration}: portable SVG and Canvas must share the role border"
            );
        }
        assert_eq!(
            roles.len(),
            4,
            "the four shape roles must remain distinguishable"
        );
        assert_eq!(
            single_node_paints("flowchart TD\nA[Process]\n", &config),
            (config.node_fill.clone(), config.node_stroke.clone()),
            "ordinary processes must retain the quiet base surface"
        );
    }
}

#[test]
fn default_class_svg_rules_apply_before_named_classes() {
    let ir = fm_parser::parse(DEFAULT_CLASS_CASCADE).ir;
    for embed_theme_css in [true, false] {
        let svg = render_svg_with_config(
            &ir,
            &SvgRenderConfig {
                embed_theme_css,
                ..SvgRenderConfig::default()
            },
        );
        let default_rule = svg
            .find(".fm-node .fm-node-shape")
            .expect("implicit default selector");
        let named_rule = svg
            .find(".fm-node-user-accent .fm-node-shape")
            .expect("named class selector");
        assert!(default_rule < named_rule, "named overrides must win: {svg}");
        let default_shape = &svg[default_rule..];
        let default_shape = default_shape.split_once('}').unwrap().0;
        assert!(default_shape.contains("fill:#112233"), "{default_shape}");
        assert!(default_shape.contains("stroke:#445566"), "{default_shape}");
        assert!(
            default_shape.contains("stroke-width:3px"),
            "{default_shape}"
        );
        let default_label = svg
            .split_once(".fm-node .fm-node-label")
            .expect("implicit label selector")
            .1
            .split_once('}')
            .unwrap()
            .0;
        assert!(default_label.contains("fill:#778899"), "{default_label}");

        let groups: Vec<_> = svg
            .split("<g ")
            .skip(1)
            .filter_map(|tail| tail.split_once('>').map(|(group, _)| group))
            .filter(|group| {
                attribute(group, "class")
                    .is_some_and(|classes| classes.split_whitespace().any(|c| c == "fm-node"))
            })
            .collect();
        assert_eq!(groups.len(), 3, "default rule must match every node");
        assert_eq!(
            groups
                .iter()
                .filter(|group| attribute(group, "class")
                    .unwrap()
                    .contains("fm-node-user-accent"))
                .count(),
            2
        );
        assert!(
            svg.contains("class=\"fm-node-shape\""),
            "default rule has a shape target"
        );
        assert!(
            svg.contains("fill:#abcdef"),
            "direct shape style must survive"
        );
        assert!(
            svg.contains("fill:#123456"),
            "direct label style must survive"
        );
    }
}

#[test]
fn author_paint_channels_override_shape_roles_independently() {
    let config = CanvasRenderConfig::default();
    let source = "flowchart TD\nA{Decision}\n";
    let (role_fill, role_stroke) = single_node_paints(source, &config);
    for declaration in [
        "style A fill:#ff0000\n",
        "classDef chosen fill:#ff0000\nclass A chosen\n",
    ] {
        assert_eq!(
            single_node_paints(&format!("{source}{declaration}"), &config),
            ("#ff0000".to_owned(), role_stroke.clone()),
            "a fill-only declaration must retain the semantic outline"
        );
    }
    for declaration in [
        "style A stroke:#008800\n",
        "classDef chosen stroke:#008800\nclass A chosen\n",
    ] {
        assert_eq!(
            single_node_paints(&format!("{source}{declaration}"), &config),
            (role_fill.clone(), "#008800".to_owned()),
            "a stroke-only declaration must retain the semantic surface"
        );
    }
}

#[test]
fn scene_svg_resolves_default_named_and_direct_node_styles() {
    let ir = fm_parser::parse(DEFAULT_CLASS_CASCADE).ir;
    for embed_theme_css in [true, false] {
        let svg = render_svg_with_config(
            &ir,
            &SvgRenderConfig {
                backend: SvgBackend::Scene,
                embed_theme_css,
                ..SvgRenderConfig::default()
            },
        );
        for (id, fill, color) in [
            ("A", "#112233", "#778899"),
            ("B", "#cc5500", "#778899"),
            ("C", "#abcdef", "#123456"),
        ] {
            let index = ir.nodes.iter().position(|node| node.id == id).unwrap();
            let shape = scene_node_element(&svg, "path", index);
            let text = scene_node_element(&svg, "text", index);
            assert_style(shape, "fill", fill);
            assert_style(shape, "stroke", "#445566");
            assert_style(shape, "stroke-width", "3px");
            assert_style(text, "fill", color);
        }
    }
}

#[test]
fn semantic_shape_colors_respect_opt_out_and_other_diagram_families() {
    let config = CanvasRenderConfig::default();
    assert_eq!(
        single_node_paints("stateDiagram-v2\nstate chooser <<choice>>\n", &config),
        (config.node_fill.clone(), config.node_stroke.clone()),
        "a state choice diamond must keep its own diagram's semantics"
    );
    let disabled = CanvasRenderConfig {
        semantic_colors: false,
        ..config.clone()
    };
    assert_eq!(
        single_node_paints("flowchart TD\nA{Decision}\n", &disabled),
        (config.node_fill.clone(), config.node_stroke.clone())
    );
    for preset in [
        ThemePreset::Mermaid,
        ThemePreset::Monochrome,
        ThemePreset::HighContrast,
        ThemePreset::Forest,
    ] {
        let config = canvas_theme(preset);
        let source = format!(
            "%%{{init: {{'theme': '{}'}}}}%%\nflowchart TD\nA{{Decision}}\n",
            preset.as_str()
        );
        assert_eq!(
            single_node_paints(&source, &config),
            (config.node_fill.clone(), config.node_stroke.clone()),
            "the explicit {preset:?} palette must not acquire semantic tints"
        );
    }
}

#[test]
fn scene_svg_honors_named_and_direct_styles_without_a_default_class() {
    let ir = fm_parser::parse(
        "flowchart LR\nA[Styled]:::accent\nB[Plain]\n\
         classDef accent fill:#cc5500,color:#778899\n\
         style A stroke:#445566,color:#123456\n",
    )
    .ir;
    let svg = render_svg_with_config(
        &ir,
        &SvgRenderConfig {
            backend: SvgBackend::Scene,
            ..SvgRenderConfig::default()
        },
    );
    let styled = ir.nodes.iter().position(|node| node.id == "A").unwrap();
    let plain = ir.nodes.iter().position(|node| node.id == "B").unwrap();
    assert_style(scene_node_element(&svg, "path", styled), "fill", "#cc5500");
    assert_style(
        scene_node_element(&svg, "path", styled),
        "stroke",
        "#445566",
    );
    assert_style(scene_node_element(&svg, "text", styled), "fill", "#123456");
    assert_eq!(
        attribute(scene_node_element(&svg, "path", plain), "style"),
        None
    );
    assert_eq!(
        attribute(scene_node_element(&svg, "text", plain), "style"),
        None
    );
}

#[test]
fn scene_svg_sanitizes_programmatic_node_styles() {
    let mut ir = fm_parser::parse("flowchart TD\nA[Safe]\n").ir;
    ir.nodes[0].inline_style = Some(Box::new(fm_core::IrInlineStyle {
        properties: BTreeMap::from([
            ("fill".to_string(), "url(javascript:alert(1))".to_string()),
            ("stroke".to_string(), "#445566".to_string()),
            ("color".to_string(), "#778899".to_string()),
            ("position".to_string(), "fixed".to_string()),
            (
                "stroke-width".to_string(),
                "2px;position:absolute".to_string(),
            ),
            (
                "font-family".to_string(),
                "sans-serif;display:none".to_string(),
            ),
        ]),
    }));
    let svg = render_svg_with_config(
        &ir,
        &SvgRenderConfig {
            backend: SvgBackend::Scene,
            ..SvgRenderConfig::default()
        },
    );
    assert_style(scene_node_element(&svg, "path", 0), "stroke", "#445566");
    assert_style(scene_node_element(&svg, "text", 0), "fill", "#778899");
    assert!(
        !svg.contains("javascript:"),
        "unsafe paint reached scene SVG"
    );
    assert!(
        !svg.contains("position:"),
        "disallowed property reached scene SVG"
    );
    assert_eq!(
        attribute(scene_node_element(&svg, "path", 0), "style"),
        Some("stroke:#445566"),
        "shape values must not inject additional declarations"
    );
    assert_eq!(
        attribute(scene_node_element(&svg, "text", 0), "style"),
        Some("fill:#778899"),
        "label values must not inject additional declarations"
    );
}

#[test]
fn svg_class_stroke_width_overrides_star_and_pentagon_theme_rules() {
    for (shape, shape_class) in [
        (fm_core::NodeShape::Star, "fm-node-shape-star"),
        (fm_core::NodeShape::Pentagon, "fm-node-shape-pentagon"),
    ] {
        for class in ["default", "accent"] {
            let assignment = if class == "default" { "" } else { ":::accent" };
            let source =
                format!("flowchart TD\nA[Styled]{assignment}\nclassDef {class} stroke-width:7px\n");
            let mut ir = fm_parser::parse(&source).ir;
            assert_eq!(ir.nodes.len(), 1);
            ir.nodes[0].shape = shape;
            let svg = render_svg_with_config(&ir, &SvgRenderConfig::default());
            let theme_selector = format!(".fm-node.{shape_class} :where(path)");
            let author_selector = if class == "default" {
                ".fm-node .fm-node-shape"
            } else {
                ".fm-node-user-accent .fm-node-shape"
            };
            // Both selectors have two class components; :where contributes no specificity.
            // The later author rule therefore wins over the themed 1.8px outline.
            assert_eq!(
                css_rule_declarations(&svg, &theme_selector).get("stroke-width"),
                Some(&"1.8")
            );
            assert_eq!(
                css_rule_declarations(&svg, author_selector).get("stroke-width"),
                Some(&"7px")
            );
            assert!(svg.find(&theme_selector).unwrap() < svg.find(author_selector).unwrap());
            let node = svg
                .split("<g ")
                .skip(1)
                .filter_map(|tail| tail.split_once('>'))
                .find(|(group, _)| {
                    attribute(group, "class")
                        .is_some_and(|classes| classes.split_whitespace().any(|c| c == shape_class))
                })
                .expect("the themed shape must actually be drawn");
            let contents = node.1.split_once("</g>").unwrap().0;
            assert_eq!(contents.matches("<path ").count(), 1);
            assert!(contents.contains("class=\"fm-node-shape\""));
            if class == "accent" {
                assert!(
                    attribute(node.0, "class")
                        .unwrap()
                        .contains("fm-node-user-accent")
                );
            }
        }
    }
}

#[test]
fn svg_class_compositing_applies_once_to_grouped_shapes() {
    for class in ["default", "accent"] {
        for embed_theme_css in [true, false] {
            let assignment = if class == "default" { "" } else { ":::accent" };
            let source = format!(
                "flowchart TD\nA[[Subroutine]]{assignment}\n\
                 classDef {class} fill:#abcdef,stroke:#123456,opacity:0.5,filter:blur(1px)\n"
            );
            let ir = fm_parser::parse(&source).ir;
            assert_eq!(ir.nodes.len(), 1);
            assert_eq!(ir.nodes[0].shape, fm_core::NodeShape::Subroutine);
            let svg = render_svg_with_config(
                &ir,
                &SvgRenderConfig {
                    embed_theme_css,
                    ..SvgRenderConfig::default()
                },
            );
            let selector = if class == "default" {
                ".fm-node .fm-node-shape"
            } else {
                ".fm-node-user-accent .fm-node-shape"
            };
            assert_eq!(
                css_rule_declarations(&svg, selector),
                BTreeMap::from([
                    ("fill", "#abcdef"),
                    ("stroke", "#123456"),
                    ("opacity", "0.5"),
                    ("filter", "blur(1px)"),
                ])
            );
            assert_eq!(
                css_rule_declarations(&svg, &format!("{selector} *")),
                BTreeMap::from([("fill", "#abcdef"), ("stroke", "#123456")]),
                "paint reaches child geometry, but opacity/filter must not compound"
            );
            let shape = svg
                .split("<g ")
                .skip(1)
                .filter_map(|tail| tail.split_once('>'))
                .find(|(group, _)| attribute(group, "class") == Some("fm-node-shape"))
                .expect("Subroutine must contain a grouped shape");
            let children = shape.1.split_once("</g>").unwrap().0;
            assert_eq!(children.matches("<rect ").count(), 1);
            assert_eq!(children.matches("<line ").count(), 2);
            assert!(!children.contains("opacity:"));
            assert!(!children.contains("filter:"));
        }
    }
}

#[test]
fn explicit_theme_variables_and_node_styles_beat_role_defaults() {
    let config = CanvasRenderConfig::default();
    let source = "%%{init: {'themeVariables': {'primaryColor':'#ffeecc', 'primaryBorderColor':'#884400'}}}%%\nflowchart TD\nA{Decision}\n";
    assert_eq!(
        single_node_paints(source, &config),
        ("#ffeecc".to_owned(), "#884400".to_owned())
    );
    assert_eq!(
        single_node_paints(&format!("{source}style A fill:#aaffaa\n"), &config),
        ("#aaffaa".to_owned(), "#884400".to_owned()),
        "the node's own fill must override the global theme variable"
    );
    assert_eq!(
        single_node_paints(&format!("{source}style A stroke:#008800\n"), &config),
        ("#ffeecc".to_owned(), "#008800".to_owned()),
        "the node's own border must override the global theme variable"
    );
}

// ---------------------------------------------------------------------------------------------
// EDGES (bd-lvj3, second half).
//
// `draw_edges` passed `config.edge_stroke` to the path, both UML markers and every arrowhead
// unconditionally, so `linkStyle` was discarded exactly as `style`/`classDef` was on nodes. The
// SVG renderer honours `LinkDefault` at lib.rs:2868, so a coloured edge appeared in an export and
// stayed grey in the canvas preview — the same two-outputs-disagree symptom as the node half.
//
// PROVED NON-VACUOUS by disarm, not by argument. Forcing the two resolved values back to the
// pre-fix constants (`stroke = config.edge_stroke`, `stroke_width = legacy_width`) fails exactly
// the four positive tests and none of the controls:
//
//     FAILED: a_linkstyle_index_colours_its_edge, linkstyle_default_colours_all_edges,
//             an_indexed_linkstyle_beats_the_default,
//             an_indexed_linkstyle_does_not_leak_to_other_edges
//     PASSED: the two theme-default controls below, plus the four node tests above
//     observed strokes while disarmed: ["#475569", "#475569", "#94a3b8", "#94a3b8", "#94a3b8"]
//
// That split is the point: a gate whose CONTROLS also fail when disarmed is not measuring the
// thing it names. The controls hold because the disarmed renderer still emits theme colours —
// which is precisely what they assert.
// ---------------------------------------------------------------------------------------------

/// `linkStyle <n>` reaches the canvas.
#[test]
fn a_linkstyle_index_colours_its_edge() {
    let ops = canvas_ops("flowchart TD\n  a[A] --> b[B]\n  linkStyle 0 stroke:#ff0000\n");
    let strokes = stroke_styles(&ops);

    assert!(
        strokes.iter().any(|s| s.to_lowercase().contains("ff0000")),
        "the declared edge stroke never reached the canvas: {strokes:?}"
    );
}

/// `linkStyle default` reaches every edge.
#[test]
fn linkstyle_default_colours_all_edges() {
    let ops = canvas_ops(
        "flowchart TD\n  a[A] --> b[B]\n  b --> c[C]\n  linkStyle default stroke:#00ff00\n",
    );
    let strokes = stroke_styles(&ops);

    let coloured = strokes
        .iter()
        .filter(|s| s.to_lowercase().contains("00ff00"))
        .count();
    assert!(
        coloured >= 2,
        "only {coloured} stroke(s) took the default; both edges should have: {strokes:?}"
    );
}

/// CONTROL: an indexed `linkStyle` OVERRIDES the default rather than losing to it.
///
/// This is the ordering the resolver exists to get right — merging `LinkDefault` after `Link(n)`
/// would still pass both positive tests above while making a per-edge override impossible.
#[test]
fn an_indexed_linkstyle_beats_the_default() {
    let ops = canvas_ops(
        "flowchart TD\n  a[A] --> b[B]\n  b --> c[C]\n         \n  linkStyle default stroke:#00ff00\n  linkStyle 0 stroke:#ff0000\n",
    );
    let strokes = stroke_styles(&ops);

    assert!(
        strokes.iter().any(|s| s.to_lowercase().contains("ff0000")),
        "the indexed override lost to the default: {strokes:?}"
    );
    assert!(
        strokes.iter().any(|s| s.to_lowercase().contains("00ff00")),
        "the override wiped out the default on the OTHER edge: {strokes:?}"
    );
}

/// CONTROL: `linkStyle 0` styles edge 0 and leaves edge 1 alone.
#[test]
fn an_indexed_linkstyle_does_not_leak_to_other_edges() {
    let ops =
        canvas_ops("flowchart TD\n  a[A] --> b[B]\n  b --> c[C]\n  linkStyle 0 stroke:#ff0000\n");
    let strokes = stroke_styles(&ops);

    let styled = strokes
        .iter()
        .filter(|s| s.to_lowercase().contains("ff0000"))
        .count();
    let theme_stroke = CanvasRenderConfig::default().edge_stroke;
    let themed = strokes.iter().filter(|s| **s == theme_stroke).count();

    assert!(styled >= 1, "the styled edge lost its colour: {strokes:?}");
    assert!(
        themed >= 1,
        "the UNstyled edge did not keep the theme stroke, so the style leaked: {strokes:?}"
    );
}

/// CONTROL: an unstyled diagram keeps the theme edge colour.
///
/// Guards the `None` return: a resolver that always produced some colour would satisfy every
/// positive test above while repainting every edge in every diagram.
#[test]
fn an_unstyled_edge_keeps_the_theme_stroke() {
    let ops = canvas_ops("flowchart TD\n  a[A] --> b[B]\n");
    let strokes = stroke_styles(&ops);

    assert!(
        !strokes.is_empty(),
        "nothing was stroked, so this control proves nothing"
    );
    assert!(
        strokes
            .iter()
            .any(|s| s == &CanvasRenderConfig::default().edge_stroke),
        "the default edge stroke is missing: {strokes:?}"
    );
    assert!(
        !strokes.iter().any(|s| s.to_lowercase().contains("ff0000")),
        "an unstyled edge acquired a colour from nowhere: {strokes:?}"
    );
}

/// CONTROL: a malformed `stroke-width` leaves the arrow-derived width standing.
///
/// The width parser filters non-finite and non-positive values on purpose. Without that, a
/// declared `stroke-width:0` or a NaN would make the edge invisible — a styling directive must not
/// be able to delete a line.
#[test]
fn a_malformed_stroke_width_keeps_the_arrow_derived_width() {
    let ops = canvas_ops("flowchart TD\n  a[A] --> b[B]\n  linkStyle 0 stroke-width:banana\n");

    let widths: Vec<f64> = {
        let mut out = Vec::new();
        let mut rest = ops.as_str();
        while let Some(i) = rest.find("SetLineWidth(") {
            rest = &rest[i + "SetLineWidth(".len()..];
            if let Some(end) = rest.find(')') {
                if let Ok(v) = rest[..end].trim().parse::<f64>() {
                    out.push(v);
                }
                rest = &rest[end..];
            }
        }
        out
    };

    assert!(
        !widths.is_empty(),
        "no line widths were set, so this control proves nothing"
    );
    assert!(
        widths.iter().all(|w| w.is_finite() && *w > 0.0),
        "a malformed stroke-width produced an unusable line width: {widths:?}"
    );
}
