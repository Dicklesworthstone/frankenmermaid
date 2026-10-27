//! `treemap`: a mermaid diagram family this renderer did not have at all (bd-9ghyo).
//!
//! THE GAP. Reading the pinned mermaid 11.15.0 bundle's own diagram registry against our
//! `DiagramType` (`scratchpad/registry_probe.mjs`, which asks the bundle to detect and render 27
//! candidate headers) turned up three families it ships and we did not: `radar`, `treemap` and
//! `info`. All three were being DETECTED AS FLOWCHARTS here — not rejected, not warned about at the
//! render surface, but silently turned into a node-and-edge diagram made out of the author's data
//! lines. This bead builds `treemap`.
//!
//! REFERENCE BEHAVIOUR, measured against that bundle in Chromium 151 (`treemap_probe.mjs`):
//!
//! * Leaf AREA is proportional to value. 10 and 20 in a 960x310 box came out 313x310 and 637x310.
//! * Children are drawn LARGEST FIRST — source order `A: 10`, `B: 20` renders B leftmost.
//! * A section's displayed value is the DEEP sum of its descendants (`R` > `G` > `H` > `x: 5`
//!   shows 5 on all three headers).
//! * Colour is per SECTION and INHERITED by its leaves: `Root`, `G1`, `G2` came out
//!   `rgb(134,134,255)`, `rgb(255,255,120)`, `rgb(215,255,134)` and `G1`'s two leaves both
//!   repeated `G1`'s.
//! * Labels must be QUOTED — a bare `Root` is a syntax error upstream.
//!
//! THE NEGATIVE CASE for a new diagram family is the same shape as this bead's rule for a new node
//! shape: it must render DIFFERENTLY from the fallback it used to collapse into. For a shape that
//! fallback is `Rect`; here it is the flowchart, and
//! `a_treemap_does_not_render_as_the_flowchart_it_used_to_be` pins it.

fn render(source: &str) -> String {
    fm_render_svg::render_svg(&fm_parser::parse(source).ir)
}

const NESTED: &str = "treemap\n\"R\"\n    \"G1\"\n        \"a\": 10\n        \"b\": 20\n    \"G2\"\n        \"c\": 30\n";

/// Each `<rect>` of a treemap tile as `(class, x, y, w, h, fill)`, in document order.
fn tiles(svg: &str) -> Vec<(String, f64, f64, f64, f64, String)> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(at) = rest.find("<g class=\"fm-treemap-tile ") {
        let tail = &rest[at..];
        let end = tail.find("</g>").unwrap_or(tail.len());
        let group = &tail[..end];
        let kind = if group.contains("fm-treemap-leaf") {
            "leaf"
        } else {
            "section"
        };
        let attr = |name: &str| -> String {
            let key = format!("{name}=\"");
            group
                .find(&key)
                .map(|p| {
                    let v = &group[p + key.len()..];
                    v[..v.find('"').expect("unterminated attribute")].to_string()
                })
                .unwrap_or_default()
        };
        let num = |name: &str| attr(name).parse::<f64>().unwrap_or(f64::NAN);
        out.push((
            kind.to_string(),
            num("x"),
            num("y"),
            num("width"),
            num("height"),
            attr("fill"),
        ));
        rest = &tail[end..];
    }
    out
}

/// THE NEGATIVE CASE: a treemap must not render as the flowchart it used to collapse into.
///
/// Before this bead, `treemap` reached `fm-cli detect` as `flowchart` and every data line became a
/// node. Asserted as a DIFFERENCE against the same document rendered as a flowchart, not merely as
/// "some treemap markup is present": a renderer that emitted a tile group and then ALSO fell
/// through to graph rendering would pass a presence check and still be wrong.
#[test]
fn a_treemap_does_not_render_as_the_flowchart_it_used_to_be() {
    let treemap = render("treemap\n\"Root\"\n    \"A\": 10\n    \"B\": 20\n");
    let as_flowchart = render("flowchart TD\n\"Root\"\n    \"A\": 10\n    \"B\": 20\n");
    assert_ne!(
        treemap, as_flowchart,
        "a treemap renders identically to the flowchart fallback"
    );
    assert!(
        !treemap.contains("class=\"fm-edge"),
        "a treemap drew graph edges: it is still being routed as a flowchart"
    );
    assert!(
        !treemap.contains("fm-node-shape-"),
        "a treemap drew graph node shapes: its data lines became nodes"
    );
    assert_eq!(
        tiles(&treemap).len(),
        3,
        "expected one section and two leaves"
    );
}

/// The parsed type is `treemap`, for both spellings upstream accepts.
#[test]
fn both_upstream_spellings_detect_as_treemap() {
    for source in [
        "treemap\n\"R\"\n    \"a\": 1\n",
        "treemap-beta\n\"R\"\n    \"a\": 1\n",
    ] {
        let result = fm_parser::parse(source);
        assert_eq!(
            result.ir.diagram_type,
            fm_core::DiagramType::Treemap,
            "not detected as a treemap: {source}"
        );
        assert!(
            result.warnings.is_empty(),
            "a valid treemap warned: {:?}",
            result.warnings
        );
    }
}

/// THE INVARIANT, at the drawn-rectangle level: area encodes value.
#[test]
fn drawn_leaf_area_is_proportional_to_value() {
    let drawn = tiles(&render("treemap\n\"Root\"\n    \"A\": 10\n    \"B\": 20\n"));
    let leaves: Vec<_> = drawn.iter().filter(|t| t.0 == "leaf").collect();
    assert_eq!(leaves.len(), 2, "expected two leaves: {drawn:?}");
    let big = leaves[0].3 * leaves[0].4;
    let small = leaves[1].3 * leaves[1].4;
    let ratio = big / small;
    assert!(
        (ratio - 2.0).abs() < 0.12,
        "drawn area ratio is {ratio:.3}, not the 2.0 the values ask for: {leaves:?}"
    );
}

/// Colour is per SECTION and inherited by that section's leaves.
///
/// The discriminating half is the INHERITANCE. A palette cycled per tile gives siblings different
/// colours, which passes any "the sections differ" check while destroying the only thing the
/// colour is carrying — which box a leaf belongs to. The first implementation did exactly that.
#[test]
fn colour_is_per_section_and_inherited_by_its_leaves() {
    let drawn = tiles(&render(NESTED));
    let sections: Vec<&String> = drawn
        .iter()
        .filter(|t| t.0 == "section")
        .map(|t| &t.5)
        .collect();
    assert_eq!(sections.len(), 3, "expected R, G1 and G2: {drawn:?}");
    assert!(
        sections[0] != sections[1] && sections[1] != sections[2] && sections[0] != sections[2],
        "two sections share a colour: {sections:?}"
    );

    // G1's leaves follow G1; G2's leaf follows G2. Document order is R, G1, a, b, G2, c.
    let fills: Vec<&String> = drawn.iter().map(|t| &t.5).collect();
    assert_eq!(
        fills[2], fills[1],
        "a leaf did not take its section's colour"
    );
    assert_eq!(
        fills[3], fills[1],
        "a leaf did not take its section's colour"
    );
    assert_eq!(
        fills[5], fills[4],
        "a leaf did not take its section's colour"
    );
    assert_ne!(
        fills[2], fills[5],
        "leaves of different sections share a colour, so the colour says nothing"
    );
}

/// A section shows the DEEP sum of its descendants, not a shallow one and not its own number.
#[test]
fn a_section_shows_the_deep_sum_of_its_descendants() {
    let svg = render("treemap\n\"R\"\n    \"G\"\n        \"H\"\n            \"x\": 5\n");
    // Three section headers and one leaf, every one of them showing 5.
    assert_eq!(
        svg.matches(">5</text>").count(),
        4,
        "not every level reports the descendant sum 5: {svg}"
    );
}

/// Quoted labels are required, exactly as upstream requires them.
///
/// A permissive parser that accepted `Root` would render documents mermaid REFUSES — the failure
/// mode where a diagram works here and breaks everywhere else, which is worse than not supporting
/// it at all because the author has no reason to look.
#[test]
fn an_unquoted_label_is_refused_the_way_upstream_refuses_it() {
    let result = fm_parser::parse("treemap\nRoot\n    A: 10\n");
    assert!(
        !result.warnings.is_empty(),
        "unquoted labels were accepted silently, which upstream does not do"
    );
    let drawn = tiles(&fm_render_svg::render_svg(&result.ir));
    assert!(
        drawn.is_empty(),
        "unquoted labels produced tiles: {drawn:?}"
    );
}

/// Every tile is drawn inside its parent, at the rectangle level rather than the layout level.
#[test]
fn drawn_children_sit_inside_their_drawn_section() {
    let drawn = tiles(&render(NESTED));
    let root = &drawn[0];
    for tile in &drawn[1..] {
        assert!(
            tile.1 >= root.1 - 0.01
                && tile.2 >= root.2 - 0.01
                && tile.1 + tile.3 <= root.1 + root.3 + 0.01
                && tile.2 + tile.4 <= root.2 + root.4 + 0.01,
            "a tile escapes the outermost section: tile={tile:?} root={root:?}"
        );
    }
}

/// Values display the way upstream displays them: no invented trailing zeros.
#[test]
fn values_display_without_trailing_zeros() {
    let svg = render("treemap\n\"Root\"\n    \"A\": 10.5\n    \"B\": 4.25\n");
    for expected in [">10.5</text>", ">4.25</text>", ">14.75</text>"] {
        assert!(svg.contains(expected), "missing {expected}");
    }
    assert!(
        !svg.contains(">10.5000<") && !svg.contains(">30.0<"),
        "a value was padded with trailing zeros"
    );
}

/// A `classDef` an author writes on a tile actually reaches that tile.
///
/// ⚠️ THE MARKER AND THE SELECTOR MUST AGREE ON A NAME. The first implementation emitted the
/// author's class BARE (`class="… hot"`) while the `classDef` rule this crate emits targets
/// `.fm-node-user-hot .fm-node-shape` — so the rule matched nothing, and a `classDef` we parsed,
/// accepted and emitted CSS for was silently dropped at the last step. Asserted as the two halves
/// of the selector being present on the right elements, because "the rule was emitted" and "the
/// class was emitted" were BOTH already true while the styling did nothing.
#[test]
fn a_classdef_reaches_the_tile_it_names() {
    let svg =
        render("treemap\n\"Root\"\n    \"A\": 10:::hot\n    \"B\": 20\nclassDef hot fill:#f00\n");
    assert!(
        svg.contains(".fm-node-user-hot .fm-node-shape"),
        "no classDef rule was emitted"
    );
    assert!(
        svg.contains("fm-treemap-leaf fm-node-user-hot"),
        "the tile does not carry the marker the rule selects on"
    );
    assert!(
        svg.contains("<rect class=\"fm-node-shape\""),
        "the tile's rect does not carry the second half of the selector"
    );
    // The bare author name must NOT be emitted: it matches no rule and collides with a host page.
    assert!(
        !svg.contains("fm-treemap-leaf hot"),
        "the bare author class name is still being emitted"
    );
}

/// A whole number displays as an integer.
#[test]
fn a_whole_value_displays_as_an_integer() {
    let svg = render("treemap\n\"Root\"\n    \"A\": 10\n    \"B\": 20\n");
    assert!(svg.contains(">30</text>"), "the section sum is not '30'");
    assert!(!svg.contains(">30.0<"), "the section sum gained a decimal");
}

#[test]
fn tiny_and_large_treemap_values_survive_both_svg_backends() {
    use fm_render_svg::{SvgBackend, SvgRenderConfig};

    for value in ["0.00001", "100000000000000000000"] {
        let ir = fm_parser::parse(&format!("treemap\n\"Root\"\n    \"Capacity\": {value}\n")).ir;
        for backend in [SvgBackend::LegacyLayout, SvgBackend::Scene] {
            let svg = fm_render_svg::render_svg_with_config(
                &ir,
                &SvgRenderConfig {
                    backend,
                    ..Default::default()
                },
            );
            assert_eq!(
                svg.matches(&format!(">{value}</text>")).count(),
                2,
                "both the leaf and its section must retain {value} in {backend:?}"
            );
            assert!(
                !svg.contains(">0</text>"),
                "a positive quantity became zero"
            );
            assert!(
                !svg.contains(">9223372036854775807</text>"),
                "a large quantity saturated at the signed-integer limit"
            );
        }
    }
}

fn scene_rows(svg: &str) -> Vec<(&str, &str)> {
    svg.split("<text")
        .skip(1)
        .map(|element| {
            let (tag, body) = element.split_once('>').expect("text opening tag");
            let (text, _) = body.split_once("</text>").expect("text closing tag");
            (tag, text)
        })
        .collect()
}

fn scene_attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    tag.split_once(&format!(" {name}=\""))?.1.split('"').next()
}

fn scene_group<'a>(svg: &'a str, id: &str) -> &'a str {
    let marker = format!("id=\"{id}\"");
    svg.split("<g")
        .skip(1)
        .find(|group| group.split('>').next().unwrap_or("").contains(&marker))
        .expect("the chart must retain the requested group")
        .split("</g>")
        .next()
        .unwrap()
}

fn scene_node_rows(svg: &str, node: usize) -> Vec<(&str, &str)> {
    let marker = format!("id=\"treemap-text-{node}-");
    svg.split("<g")
        .skip(1)
        .filter(|group| group.split('>').next().unwrap_or("").contains(&marker))
        .flat_map(|group| scene_rows(group.split("</g>").next().unwrap()))
        .collect()
}

#[test]
fn treemap_scene_keeps_hierarchy_values_theme_and_one_visible_title() {
    use fm_render_svg::{SvgBackend, SvgRenderConfig, ThemeColors, ThemePreset};

    let mut ir = fm_parser::parse(NESTED).ir;
    ir.meta.title = Some("Capacity plan".to_owned());
    let layout = fm_layout::layout_diagram(&ir);
    for theme in [ThemePreset::Default, ThemePreset::Dark] {
        let config = SvgRenderConfig {
            backend: SvgBackend::Scene,
            theme,
            ..Default::default()
        };
        let svg = fm_render_svg::render_svg_with_layout(&ir, &layout, &config);
        let rows = scene_rows(&svg);
        for name in ["R", "G1", "a", "b", "G2", "c", "Capacity plan"] {
            assert!(rows.iter().any(|(_, text)| *text == name), "missing {name}");
        }
        assert_eq!(rows.iter().filter(|(_, text)| *text == "60").count(), 1);
        assert_eq!(rows.iter().filter(|(_, text)| *text == "30").count(), 3);
        assert_eq!(rows.iter().filter(|(_, text)| *text == "20").count(), 1);
        assert_eq!(rows.iter().filter(|(_, text)| *text == "10").count(), 1);
        assert_eq!(
            rows.iter()
                .filter(|(_, text)| *text == "Capacity plan")
                .count(),
            1,
            "the document title must not duplicate the measured title"
        );
        assert_eq!(svg.matches("id=\"treemap-tile-").count(), 6);
        let root = scene_group(&svg, "treemap-chart")
            .split('>')
            .next()
            .unwrap();
        assert_eq!(scene_attribute(root, "xml:space"), Some("preserve"));
        let colors = ThemeColors::from_preset(theme);
        for (tag, _) in &rows {
            assert_eq!(scene_attribute(tag, "fill"), Some(colors.text.as_str()));
        }
        assert!(!svg.contains("data-fm-source-kind=\"node\""));
        assert!(!svg.contains("data-fm-source-kind=\"edge\""));
    }
}

#[test]
fn treemap_scene_retains_tiny_and_zero_entries_in_readable_bounded_details() {
    use fm_render_svg::{SvgBackend, SvgRenderConfig};

    let tiny = "Tiny  東京 service with complete regional coverage";
    let zero = "Zero  capacity";
    let source = format!(
        "treemap\n\"Root\"\n    \"Dominant\": 1000000\n    \"{tiny}\": 0.000001\n    \"{zero}\": 0\n"
    );
    let parsed = fm_parser::parse(&source);
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    let mut ir = parsed.ir;
    ir.meta.title = Some("Measured inventory".to_owned());
    let layout = fm_layout::layout_diagram(&ir);
    let original_tiles = layout.extensions.treemap_tiles.clone();
    let meta = ir.treemap_meta.as_ref().unwrap();
    for font_size in [15.0, 36.0] {
        let config = SvgRenderConfig {
            backend: SvgBackend::Scene,
            font_size,
            font_family: "monospace".to_owned(),
            ..Default::default()
        };
        let svg = fm_render_svg::render_svg_with_layout(&ir, &layout, &config);
        let rows = scene_rows(&svg);
        let complete: String = rows.iter().map(|(_, text)| *text).collect();
        assert!(complete.contains(&format!("Root / {tiny}")));
        assert!(complete.contains(&format!("Root / {zero}")));
        assert!(rows.iter().any(|(_, text)| *text == "0.000001"));
        assert!(rows.iter().any(|(_, text)| *text == "0"));
        assert!(!complete.contains('…'));
        assert_eq!(
            rows.iter()
                .filter(|(_, text)| *text == "Measured inventory")
                .count(),
            1
        );
        for name in [tiny, zero] {
            let node = meta
                .nodes
                .iter()
                .position(|node| node.label == name)
                .unwrap();
            let details = scene_node_rows(&svg, node);
            assert!(
                details.iter().any(|(tag, _)| {
                    scene_attribute(tag, "x").unwrap().parse::<f32>().unwrap()
                        > layout.bounds.x + layout.bounds.width
                }),
                "{name} must remain readable outside its quantitative tile"
            );
        }
        let root_tag = svg.split("<svg").nth(1).unwrap().split('>').next().unwrap();
        let viewbox: Vec<f32> = scene_attribute(root_tag, "viewBox")
            .unwrap()
            .split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect();
        assert_eq!(viewbox.len(), 4);
        assert!(viewbox.iter().all(|value| value.is_finite()));
        assert!(viewbox[2] > 0.0 && viewbox[3] > 0.0);
        let metrics = config.font_metrics();
        for (tag, text) in rows {
            let number = |name| scene_attribute(tag, name).unwrap().parse::<f32>().unwrap();
            let x = number("x");
            let y = number("y");
            let size = number("font-size");
            let width = metrics.estimate_preserved_width(text, size, metrics.avg_char_width());
            let left = match scene_attribute(tag, "text-anchor") {
                Some("middle") => x - width * 0.5,
                Some("end") => x - width,
                _ => x,
            };
            assert!(left >= viewbox[0] - 0.01, "text escapes left: {text}");
            assert!(
                left + width <= viewbox[0] + viewbox[2] + 0.01,
                "text escapes right: {text}"
            );
            assert!(
                y - size * 0.5 >= viewbox[1] - 0.01,
                "text escapes top: {text}"
            );
            assert!(
                y + size * 0.5 <= viewbox[1] + viewbox[3] + 0.01,
                "text escapes bottom: {text}"
            );
        }
        assert_eq!(layout.extensions.treemap_tiles, original_tiles);
    }
}

#[test]
fn treemap_scene_applies_author_typography_and_alpha_exactly_once() {
    use fm_render_svg::{SvgBackend, SvgRenderConfig};

    let source = "treemap\n\"Root\"\n    \"Priority\": 10:::hot\n    \"Other\": 20\nclassDef default font-family:serif,font-weight:400\nclassDef hot fill:#112233,fill-opacity:0.8,stroke:#aabbcc,stroke-opacity:0.5,stroke-width:2px,color:#fedcba,opacity:0.5,font-size:22px,font-family:monospace,font-weight:700,font-style:italic\n";
    let parsed = fm_parser::parse(source);
    assert!(
        parsed.warnings.iter().all(|warning| warning == "style directives (classDef/style/linkStyle) are applied by the SVG and canvas renderers; terminal output has no colour channel and uses theme defaults"),
        "{:?}",
        parsed.warnings
    );
    let ir = parsed.ir;
    let layout = fm_layout::layout_diagram(&ir);
    let node = ir
        .treemap_meta
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .position(|node| node.label == "Priority")
        .unwrap();
    let number =
        |tag: &str, name: &str| scene_attribute(tag, name).unwrap().parse::<f64>().unwrap();
    for embed_theme_css in [false, true] {
        let config = SvgRenderConfig {
            backend: SvgBackend::Scene,
            embed_theme_css,
            ..Default::default()
        };
        let svg = fm_render_svg::render_svg_with_layout(&ir, &layout, &config);
        let group = scene_group(&svg, &format!("treemap-tile-{node}"));
        let opening = group.split('>').next().unwrap();
        assert_eq!(scene_attribute(opening, "opacity"), None);
        let path = group
            .split("<path")
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert_eq!(scene_attribute(path, "fill"), Some("#112233"));
        assert_eq!(scene_attribute(path, "stroke"), Some("#aabbcc"));
        assert_eq!(number(path, "stroke-width"), 2.0);
        assert!((number(path, "fill-opacity") - 0.4).abs() < 1e-6);
        assert!((number(path, "stroke-opacity") - 0.25).abs() < 1e-6);
        assert_eq!(scene_attribute(path, "opacity"), None);
        let rows = scene_node_rows(&svg, node);
        assert_eq!(
            rows.iter().map(|(_, text)| *text).collect::<Vec<_>>(),
            ["Priority", "10"]
        );
        for (tag, _) in rows {
            assert_eq!(number(tag, "font-size"), 22.0);
            assert_eq!(scene_attribute(tag, "font-family"), Some("monospace"));
            assert_eq!(scene_attribute(tag, "font-weight"), Some("700"));
            assert_eq!(scene_attribute(tag, "font-style"), Some("italic"));
            assert_eq!(scene_attribute(tag, "fill"), Some("#fedcba"));
            assert!((number(tag, "fill-opacity") - 0.5).abs() < 1e-6);
            assert_eq!(scene_attribute(tag, "opacity"), None);
        }
    }
}
