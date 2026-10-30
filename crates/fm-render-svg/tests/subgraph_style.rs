//! `style mySubgraph fill:#f00` must colour the subgraph (bd-xfmm).
//!
//! bd-xfmm found that a `style` target resolving to nothing was dropped in silence and taught the
//! parser to warn. It could not do more: a subgraph id is not a node, so `node_id_by_key` misses
//! it, and `IrStyleTarget` had no `Cluster` variant to hold it "even if it were found". The
//! directive was therefore reported and still ignored.
//!
//! The variant exists now, the parser resolves a subgraph target to it, and this is the half that
//! makes it visible. Without a consumer the variant would be one more parsed-stored-drawn-by-nothing
//! field — the class bd-jgco, bd-jerh and bd-bk7h all belong to.

const STYLED: &str =
    "flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n  style one fill:#ff0000\n";

/// The declared fill reaches the cluster.
#[test]
fn a_subgraph_style_reaches_the_svg() {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(STYLED).ir);

    assert!(
        svg.contains("#ff0000"),
        "the subgraph's declared fill never reached the SVG:\n{svg}"
    );
}

/// NON-VACUITY: the parser must record it against a CLUSTER, not merely emit the colour somewhere.
///
/// `#ff0000` appearing in the document is not proof the SUBGRAPH got it — a future change that
/// applied the style to the contained node would satisfy the test above while leaving the subgraph
/// unstyled, which is the defect.
#[test]
fn the_style_is_recorded_against_a_cluster_not_a_node() {
    let ir = fm_parser::parse(STYLED).ir;

    assert!(
        ir.style_refs
            .iter()
            .any(|style_ref| matches!(style_ref.target, fm_core::IrStyleTarget::Cluster(_))),
        "the subgraph style was not recorded against a cluster: {:?}",
        ir.style_refs
    );
}

/// CONTROL: a subgraph with NO style declared must gain none. This is what stops the resolver
/// applying an empty or inherited style attribute to every cluster.
#[test]
fn an_unstyled_subgraph_gains_no_style_attribute() {
    let svg = fm_render_svg::render_svg(
        &fm_parser::parse("flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n").ir,
    );

    // The cluster rect must still be drawn — otherwise "no style attribute" is vacuous.
    assert!(
        svg.contains("fm-cluster"),
        "no cluster was drawn, so this control proves nothing:\n{svg}"
    );
    assert!(
        !svg.contains("#ff0000"),
        "an unstyled subgraph gained a colour from nowhere:\n{svg}"
    );
}

/// CONTROL: an ordinary node style still works and is unaffected by the cluster path. A regression
/// that routed every `style` target to the cluster lookup would satisfy the tests above.
#[test]
fn a_node_style_still_applies() {
    let ir = fm_parser::parse("flowchart TD\n  a[A] --> b[B]\n  style a fill:#00ff00\n").ir;

    assert!(
        ir.style_refs
            .iter()
            .any(|style_ref| matches!(style_ref.target, fm_core::IrStyleTarget::Node(_))),
        "an ordinary node style stopped being recorded against a node: {:?}",
        ir.style_refs
    );
}

/// A DECLARED cluster fill is not dimmed by the theme's cluster fill-opacity (bd-xfmm).
///
/// `cluster_fill_opacity: 0.08` exists so an UNSTYLED subgraph is a faint tint behind its
/// contents. Applied to a colour the author asked for, it rendered `style one fill:#ff0000` as a
/// barely visible pink wash.
///
/// Measured before changing it, and it disagreed in two directions at once: fm-render-canvas
/// paints the same declaration at full strength, so the two backends disagreed about a document
/// the author styled; and the incumbent has no cluster dimming at all — mermaid 11.15.0's only
/// `fill-opacity` values are `1.0`, a curve opacity and a graticule opacity.
#[test]
fn a_declared_cluster_fill_is_not_dimmed_by_the_theme_opacity() {
    let svg = fm_render_svg::render_svg_with_config(
        &fm_parser::parse(STYLED).ir,
        &fm_render_svg::SvgRenderConfig {
            cluster_fill_opacity: 0.08,
            ..Default::default()
        },
    );

    let rect = cluster_rect(&svg).expect("no cluster rect was emitted");
    assert!(
        rect.contains("fill:#ff0000"),
        "the declared cluster fill did not reach the rect: {rect}"
    );
    assert!(!rect.contains("fill-opacity=\"0.08\""), "{rect}");
    assert!(
        rect.contains("fill-opacity:1"),
        "the explicit fill must also override the embedded theme's opacity rule: {rect}"
    );
}

/// CONTROL: an UNSTYLED subgraph keeps the theme's faint tint.
///
/// This is what stops the fix above from being "delete the opacity". The subtle container look is
/// deliberate and applies whenever the author has not asked for something else.
#[test]
fn an_undeclared_cluster_keeps_the_theme_fill_opacity() {
    let svg = fm_render_svg::render_svg_with_config(
        &fm_parser::parse("flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n").ir,
        &fm_render_svg::SvgRenderConfig {
            cluster_fill_opacity: 0.08,
            ..Default::default()
        },
    );

    let rect = cluster_rect(&svg).expect("no cluster rect was emitted");
    assert!(
        rect.contains("fill-opacity"),
        "an unstyled cluster lost the theme's fill-opacity, which is the subtle container look: \
         {rect}"
    );
}

/// CONTROL: a declared STROKE alone does not remove the fill opacity.
///
/// Only a declared FILL competes with the fill-opacity. A subgraph whose border was recoloured
/// still wants the faint body, and an implementation keying off "any declared style" would flatten
/// it.
#[test]
fn a_declared_cluster_stroke_alone_keeps_the_fill_opacity() {
    let svg = fm_render_svg::render_svg_with_config(
        &fm_parser::parse(
            "flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n  style one stroke:#00ff00\n",
        )
        .ir,
        &fm_render_svg::SvgRenderConfig {
            cluster_fill_opacity: 0.08,
            ..Default::default()
        },
    );

    let rect = cluster_rect(&svg).expect("no cluster rect was emitted");
    assert!(
        rect.contains("stroke:#00ff00"),
        "the declared stroke did not reach the rect: {rect}"
    );
    assert!(
        rect.contains("fill-opacity"),
        "declaring only a stroke removed the fill opacity: {rect}"
    );
}

/// The rect element carrying `class="fm-cluster"`, if any.
///
/// ⚠️ `.skip(1)` IS LOad-BEARING. `split("<rect")` yields everything BEFORE the first rect as
/// chunk 0, and that chunk contains the `<style>` block — which mentions `--fm-cluster-fill`. My
/// first version matched that chunk, found no `>`-terminated element in it, and returned `None`,
/// so all three tests failed at their `expect` while the code under test was correct. Matching the
/// stylesheet instead of the element is the same content-versus-identity mistake as grepping a
/// report for a severity word.
fn cluster_rect(svg: &str) -> Option<String> {
    svg.split("<rect")
        .skip(1)
        .map(|chunk| chunk.split('>').next().unwrap_or(chunk))
        .find(|element| element.contains("fm-cluster"))
        .map(|element| format!("<rect{element}>"))
}

// ── The label half of a `style` directive (styles2String parity) ───────────────────────────────

/// A `style` directive on a subgraph is TWO styles, and mermaid says exactly which is which.
///
/// REFERENCE, read out of the pinned mermaid 11.15.0 bundle rather than assumed. `styles2String`
/// partitions the declaration list with an `isLabelStyle` predicate:
///
/// ```text
/// isLabelStyle = p => p === "color" || p === "font-size" || p === "font-family"
///   || p === "font-weight" || p === "font-style" || p === "text-decoration"
///   || p === "text-align" || p === "text-transform" || p === "line-height"
///   || p === "letter-spacing" || p === "word-spacing" || p === "text-shadow"
///   || p === "text-overflow" || p === "white-space" || p === "word-wrap"
///   || p === "word-break" || p === "overflow-wrap" || p === "hyphens"
///
/// styles2String = e => { ... t.forEach(s => { let l = s[0];
///     isLabelStyle(l) ? r.push(...)      // -> labelStyles, applied to the LABEL
///                     : i.push(...) })   // -> nodeStyles,  applied to the SHAPE
///   ... return { labelStyles: r.join(";"), nodeStyles: i.join(";") } }
/// ```
///
/// and its own db confirms the input side: for `style one fill:#ff0000,color:#123456` the group
/// node carries `cssStyles: ["fill:#ff0000", "color:#123456"]`, which that function then splits.
///
/// We drew ALL of it on the `<rect>`. CSS `color` does nothing on a rect, so the title silently
/// kept the theme colour and the author's declaration was inert. The NODE path in this renderer
/// already did the partition (`split_style_properties`, which also maps `color` -> `fill` because
/// SVG text needs `fill`); the cluster path was its unported sibling.
const SPLIT: &str = "flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n  \
                     style one fill:#ff0000,color:#123456,stroke-width:4px\n";

/// Extract the tag text of the first element whose attributes contain `needle`.
fn element_containing<'a>(svg: &'a str, tag: &str, needle: &str) -> &'a str {
    let mut rest = svg;
    while let Some(start) = rest.find(tag) {
        rest = &rest[start..];
        let end = rest.find('>').expect("unterminated element") + 1;
        let element = &rest[..end];
        if element.contains(needle) {
            return element;
        }
        rest = &rest[end..];
    }
    panic!("no {tag} element containing {needle} in:\n{svg}");
}

#[test]
fn a_label_property_styles_the_title_and_not_the_box() {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(SPLIT).ir);
    let rect = element_containing(&svg, "<rect", "fm-cluster");
    let label = element_containing(&svg, "<text", "fm-cluster-label");

    // The shape keeps the shape properties.
    assert!(
        rect.contains("fill:#ff0000") && rect.contains("stroke-width:4px"),
        "the cluster rect lost its shape properties: {rect}"
    );
    // ⚠️ THE NEGATIVE HALF, and the one that fails on the old behaviour: `color` must NOT be on the
    // rect. Asserting only that the label got it would pass while the rect still carried a dead
    // `color` declaration, which is precisely what shipped.
    assert!(
        !rect.contains("color:"),
        "`color` is a LABEL property and does nothing on a rect, but the rect carries it: {rect}"
    );
    // The label gets it, as `fill` — SVG text has no `color` presentation attribute.
    assert!(
        label.contains("fill:#123456"),
        "the declared label colour never reached the cluster title: {label}"
    );
    // ...and must not inherit the shape's properties.
    assert!(
        !label.contains("#ff0000") && !label.contains("stroke-width"),
        "shape properties leaked onto the cluster label: {label}"
    );
}

/// CONTROL for the mapping direction: `color` must arrive as `fill`, not as a literal `color`.
///
/// An implementation that split the list correctly but forwarded the property name verbatim would
/// pass every assertion above except this one, and would still render an unstyled title.
#[test]
fn the_label_colour_is_mapped_to_fill_not_left_as_color() {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(SPLIT).ir);
    let label = element_containing(&svg, "<text", "fm-cluster-label");

    assert!(
        !label.contains("color:#123456"),
        "the label kept the CSS property name `color`, which SVG text ignores: {label}"
    );
}

/// A shape-only declaration must leave the label with no inline style at all — otherwise every
/// styled cluster gains an empty `style=""` and the split is doing something on inputs it should
/// not touch.
#[test]
fn a_shape_only_style_leaves_the_label_alone() {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(STYLED).ir);
    let label = element_containing(&svg, "<text", "fm-cluster-label");

    assert!(
        !label.contains("style="),
        "a fill-only subgraph style put an inline style on the label: {label}"
    );
}

const NESTED: &str = "flowchart LR\n\
    subgraph outer[Platform]\n\
      A[Gateway]\n\
      subgraph inner[Services]\n\
        B[Application]\n\
        subgraph storage[Persistence]\n\
          C[(Database)]\n\
        end\n\
      end\n\
      A --> B --> C\n\
    end\n";

fn frame<'a>(svg: &'a str, ir: &fm_core::MermaidDiagramIr, key: &str) -> &'a str {
    let index = ir
        .graph
        .first_subgraph_by_key(key)
        .expect("named subgraph")
        .cluster
        .expect("framed subgraph")
        .0;
    element_containing(svg, "<rect", &format!("id=\"fm-cluster-{index}\""))
}

#[test]
fn nested_frames_alternate_theme_surfaces_in_browser_and_portable_svg() {
    use fm_render_svg::{SvgRenderConfig, ThemeColors, ThemePreset, render_svg_with_config};

    let ir = fm_parser::parse(NESTED).ir;
    for theme in [ThemePreset::Default, ThemePreset::Dark, ThemePreset::Forest] {
        let colors = ThemeColors::from_preset(theme);
        for portable in [false, true] {
            let mut config = SvgRenderConfig {
                theme,
                ..Default::default()
            };
            if portable {
                config.make_portable();
            }
            let svg = render_svg_with_config(&ir, &config);
            assert_eq!(svg, render_svg_with_config(&ir, &config));
            for (key, expected) in [
                ("outer", colors.cluster_fill.as_str()),
                ("inner", colors.background.as_str()),
                ("storage", colors.cluster_fill.as_str()),
            ] {
                let rect = frame(&svg, &ir, key);
                assert!(rect.contains(&format!("fill=\"{expected}\"")), "{rect}");
                assert!(rect.contains(&format!("stroke=\"{}\"", colors.cluster_stroke)));
                assert!(rect.contains("rx=\"12\""), "{rect}");
                assert_eq!(
                    rect.contains("--fm-cluster-fill:var(--fm-bg)"),
                    key == "inner" && !portable,
                    "the embedded CSS must actually use the nested surface: {rect}"
                );
            }
            let label = element_containing(&svg, "<text", "fm-cluster-label");
            assert!(label.contains("font-weight=\"600\""));
            if portable {
                assert!(label.contains(&format!("fill=\"{}\"", colors.text)));
            }
        }
    }
}

#[test]
fn authored_cluster_palette_and_individual_styles_keep_precedence_over_depth() {
    use fm_render_svg::{SvgRenderConfig, render_svg_with_config};

    let mut ir = fm_parser::parse(&format!(
        "{NESTED}\nstyle inner fill:#c7f9cc,stroke:#225533,fill-opacity:0.7,color:#123456"
    ))
    .ir;
    ir.meta
        .theme_overrides
        .theme_variables
        .insert("clusterBkg".into(), "#fff4d0".into());
    ir.meta
        .theme_overrides
        .theme_variables
        .insert("clusterBorder".into(), "#b45309".into());
    for embed_theme_css in [false, true] {
        let svg = render_svg_with_config(
            &ir,
            &SvgRenderConfig {
                embed_theme_css,
                cluster_fill_opacity: 0.2,
                ..Default::default()
            },
        );
        for key in ["outer", "storage"] {
            let rect = frame(&svg, &ir, key);
            assert!(rect.contains("fill=\"#fff4d0\""));
            assert!(rect.contains("stroke=\"#b45309\""));
            assert!(rect.contains("fill-opacity=\"0.20\""), "{rect}");
        }
        let inner = frame(&svg, &ir, "inner");
        for declaration in ["fill:#c7f9cc", "stroke:#225533", "fill-opacity:0.7"] {
            assert!(inner.contains(declaration), "{inner}");
        }
        assert!(!inner.contains("fill-opacity=\"0.20\""));
        assert!(!inner.contains("var(--fm-bg)"));
        let label = element_containing(&svg, "<text", "fill:#123456");
        assert!(label.contains("fm-cluster-label"));
    }

    // A palette override also applies to an otherwise unstyled nested frame.
    let mut plain = fm_parser::parse(NESTED).ir;
    plain.meta.theme_overrides = ir.meta.theme_overrides;
    let svg = render_svg_with_config(&plain, &SvgRenderConfig::default());
    let inner = frame(&svg, &plain, "inner");
    assert!(inner.contains("fill=\"#fff4d0\""));
    assert!(!inner.contains("var(--fm-bg)"));
}

#[test]
fn sequence_group_colors_reach_the_embedded_css_without_theme_opacity() {
    use fm_render_svg::{SvgRenderConfig, render_svg_with_layout};

    let ir = fm_parser::parse(
        "sequenceDiagram\nbox aqua Backend\nparticipant A\nparticipant B\nend\nA->>B: Query",
    )
    .ir;
    let mut layout = fm_layout::layout_diagram(&ir);
    assert_eq!(layout.clusters.len(), 1);
    for color in ["aqua", "transparent"] {
        layout.clusters[0].color = Some(color.to_string());
        for embed_theme_css in [false, true] {
            let svg = render_svg_with_layout(
                &ir,
                &layout,
                &SvgRenderConfig {
                    embed_theme_css,
                    cluster_fill_opacity: 0.08,
                    ..Default::default()
                },
            );
            let rect = element_containing(&svg, "<rect", "fm-cluster");
            assert!(rect.contains(&format!("fill=\"{color}\"")));
            assert!(!rect.contains("fill-opacity=\"0.08\""));
            assert!(rect.contains("fill-opacity:1"));
            if embed_theme_css {
                assert!(
                    rect.contains(&format!("--fm-cluster-fill:{color}")),
                    "{rect}"
                );
            }
            if color == "transparent" {
                assert!(rect.contains("stroke=\"#cbd5e1\""));
            }
        }
    }
}

#[test]
fn dark_c4_boundaries_keep_readable_type_captions_and_explicit_frame_geometry() {
    use fm_render_svg::{SvgRenderConfig, ThemeColors, ThemePreset, render_svg_with_config};

    let ir = fm_parser::parse(
        "C4Container\nSystem_Boundary(sys, \"System\") {\nContainer(app, \"App\", \"Rust\", \"Handles requests\")\n}",
    )
    .ir;
    for embed_theme_css in [false, true] {
        let svg = render_svg_with_config(
            &ir,
            &SvgRenderConfig {
                theme: ThemePreset::Dark,
                embed_theme_css,
                ..Default::default()
            },
        );
        let rect = element_containing(&svg, "<rect", "fm-cluster-c4");
        assert!(rect.contains("rx=\"0\""));
        assert!(rect.contains("stroke-dasharray=\"4,2\""));
        let label = element_containing(&svg, "<text", "fm-cluster-type-label");
        let colors = ThemeColors::from_preset(ThemePreset::Dark);
        let expected = if embed_theme_css {
            "var(--fm-cluster-label-color)"
        } else {
            colors.text.as_str()
        };
        assert!(label.contains(&format!("fill=\"{expected}\"")), "{label}");
    }
}

#[test]
fn portable_background_covers_the_viewbox_before_content_in_both_backends() {
    use fm_render_svg::{SvgBackend, SvgRenderConfig, render_svg_with_config};

    fn attribute<'a>(tag: &'a str, name: &str) -> &'a str {
        tag.split_once(&format!("{name}=\""))
            .expect("attribute exists")
            .1
            .split_once('"')
            .expect("attribute closes")
            .0
    }

    for source in [
        "---\ntitle: Deployment overview\n---\nflowchart LR\nA --> B",
        "pie\ntitle Revenue\n\"Subscriptions\" : 75\n\"Services\" : 25",
        "xychart-beta\nx-axis [Jan, Feb, Mar]\ny-axis 0 --> 100\nbar [30, 75, 55]",
    ] {
        let mut ir = fm_parser::parse(source).ir;
        for backend in [SvgBackend::LegacyLayout, SvgBackend::Scene] {
            for paint in ["#0f172a", "#102030", "transparent", "none"] {
                ir.meta
                    .theme_overrides
                    .theme_variables
                    .insert("background".into(), paint.into());
                let mut config = SvgRenderConfig {
                    backend,
                    theme: fm_render_svg::ThemePreset::Dark,
                    ..Default::default()
                };
                config.make_portable();
                let svg = render_svg_with_config(&ir, &config);
                let background = element_containing(&svg, "<rect", "fm-background");
                let expected = if paint == "none" {
                    "transparent"
                } else {
                    paint
                };
                assert_eq!(attribute(background, "fill"), expected);
                assert_eq!(attribute(background, "pointer-events"), "none");
                assert_eq!(attribute(background, "aria-hidden"), "true");
                let root = element_containing(&svg, "<svg", "viewBox");
                let viewbox: Vec<f32> = attribute(root, "viewBox")
                    .split_whitespace()
                    .map(|value| value.parse().expect("finite viewBox coordinate"))
                    .collect();
                for (index, name) in ["x", "y", "width", "height"].iter().enumerate() {
                    let actual: f32 = attribute(background, name).parse().expect("coordinate");
                    assert!(
                        (actual - viewbox[index]).abs() <= 0.01,
                        "{backend:?} {name}: {background} must match the viewBox at SVG's 2dp precision"
                    );
                }
                assert!(
                    svg.find(background).unwrap() < svg.find("<text ").unwrap(),
                    "the surface must sit behind all visible text, including the title"
                );
            }
        }
    }
}
