//! The author's own styling must reach the canvas (bd-lvj3).
//!
//! Measured before the fix: the canvas emitted only 3-4 distinct fills for an ENTIRE diagram,
//! whatever the source declared. Counted at the same time — fm-render-svg reads `inline_style` 30
//! times, `classes` 19 and `style_refs` 11; this crate read all three ZERO times. The canvas path is
//! what fm-wasm ships to a browser, so a user who coloured their diagram saw it uncoloured in the
//! preview and correctly coloured in exported SVG — the two outputs disagreeing about the document.

use fm_render_canvas::{CanvasRenderConfig, DrawOperation, MockCanvas2dContext, render_to_canvas};
use fm_render_svg::{SvgRenderConfig, ThemeColors, ThemePreset};

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
