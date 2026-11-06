//! Quadrant data points must carry an accessible name (bd-0eoa6).
//!
//! Third of the four chart types that emitted ZERO per-element accessibility affordances (pie was
//! bd-uf3p1, xychart bd-sdhzh). Gantt is the last, and is blocked from a clean golden bless while
//! bd-c7ijh's gantt_basic stays stale.
//!
//! This one differs from its siblings: a quadrant point ALREADY has a visible text label beside it,
//! so it is not nameless to a sighted reader. What the position conveys — WHICH QUADRANT the point
//! fell in — is what a non-visual reader cannot get, so that is what the name adds:
//! `Alpha: Do first`, or the coordinates when the author declared no quadrant labels.
//!
//! ⚠️ THE QUADRANT INDEX WAS MEASURED, NOT ASSUMED. Announcing the wrong quadrant is worse than
//! announcing none, and canvas `y` grows DOWNWARD while data `y` grows upward — an inversion that is
//! easy to get backwards and impossible for the person relying on it to detect. Before writing the
//! mapping I rendered a point at `[0.9, 0.9]` and confirmed it lands at canvas y 138 against the
//! `quadrant-1` label's y 186, i.e. the TOP half. These tests pin all four corners so the inversion
//! cannot silently flip.

use fm_render_svg::{A11yConfig, SvgBackend, SvgRenderConfig, render_svg_with_config};

/// All four quadrants named, with one point in each corner.
const NAMED: &str = "quadrantChart\n  title Q\n  quadrant-1 TopRight\n  quadrant-2 TopLeft\n  \
                     quadrant-3 BottomLeft\n  quadrant-4 BottomRight\n  \
                     HiHi: [0.9, 0.9]\n  LoHi: [0.1, 0.9]\n  LoLo: [0.1, 0.1]\n  HiLo: [0.9, 0.1]\n";
const UNNAMED: &str = "quadrantChart\n  title Q\n  Alpha: [0.9, 0.25]\n";

/// `embedded` selects the streaming writer; the `Element` path is the non-embedded-CSS export.
fn render(source: &str, embedded: bool, a11y: A11yConfig) -> String {
    render_svg_with_config(
        &fm_parser::parse(source).ir,
        &SvgRenderConfig {
            embed_theme_css: embedded,
            a11y,
            ..SvgRenderConfig::default()
        },
    )
}

fn point_names(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(at) = rest.find("class=\"fm-quadrant-point\"") {
        rest = &rest[at..];
        let Some(close) = rest.find('>') else { break };
        let after = &rest[close + 1..];
        if let Some(stripped) = after.strip_prefix("<title>")
            && let Some(end) = stripped.find("</title>")
        {
            out.push(stripped[..end].to_string());
        }
        rest = after;
    }
    out
}

/// ALL FOUR CORNERS map to the right quadrant, on both render paths.
///
/// The whole test: a point in each corner, each expecting a DIFFERENT quadrant name. A flipped
/// axis, a transposed pair or an off-by-one in the index swaps at least two of these.
#[test]
fn each_point_is_named_with_the_quadrant_it_falls_in() {
    for embedded in [true, false] {
        assert_eq!(
            point_names(&render(NAMED, embedded, A11yConfig::full())),
            vec![
                "HiHi: TopRight",
                "LoHi: TopLeft",
                "LoLo: BottomLeft",
                "HiLo: BottomRight"
            ],
            "embedded={embedded}: points are named with the wrong quadrant"
        );
    }
}

/// With no quadrant labels declared, the name falls back to the COORDINATES.
///
/// Position is the information the name exists to supply; with nothing to call the region, the
/// numbers are what is left. An empty or bare-label name would leave the point conveying nothing.
#[test]
fn a_point_without_quadrant_labels_is_named_by_its_coordinates() {
    for embedded in [true, false] {
        assert_eq!(
            point_names(&render(UNNAMED, embedded, A11yConfig::full())),
            vec!["Alpha: x 0.90, y 0.25"],
            "embedded={embedded}"
        );
    }
}

/// THE TWO RENDER PATHS AGREE.
///
/// Points are emitted twice — a streaming writer under embedded CSS, an `Element` build otherwise.
/// Pinning them to each other catches a fix applied to only one.
#[test]
fn the_streaming_and_element_paths_name_points_identically() {
    for source in [NAMED, UNNAMED] {
        assert_eq!(
            point_names(&render(source, true, A11yConfig::full())),
            point_names(&render(source, false, A11yConfig::full())),
            "the two quadrant point paths disagree about accessible names"
        );
    }
}

/// CONTROL: the visible label is NOT replaced by the accessible name.
///
/// The name is additional information, not a substitute. A change that moved the quadrant into the
/// drawn label would satisfy every assertion above while cluttering the chart.
#[test]
fn the_visible_point_label_is_unchanged() {
    let svg = render(NAMED, true, A11yConfig::full());
    assert!(
        svg.contains(">HiHi</text>"),
        "the visible point label was rewritten"
    );
    assert!(
        !svg.contains(">HiHi: TopRight</text>"),
        "the accessible name leaked into the drawn label"
    );
}

/// CONTROL: with text alternatives OFF nothing is named and the circle stays self-closing.
#[test]
fn no_names_are_emitted_when_text_alternatives_are_off() {
    for embedded in [true, false] {
        let svg = render(NAMED, embedded, A11yConfig::none());
        assert!(
            point_names(&svg).is_empty(),
            "embedded={embedded}: a title was emitted with accessibility output disabled"
        );
        // NON-VACUITY: the points are still drawn.
        assert!(
            svg.contains("class=\"fm-quadrant-point\""),
            "embedded={embedded}: CONTROL FAILED — no points rendered at all"
        );
    }
}

#[test]
fn scene_points_keep_the_same_quadrant_names_and_accessibility_controls() {
    for source in [NAMED, UNNAMED] {
        let ir = fm_parser::parse(source).ir;
        for embedded in [false, true] {
            for alternatives in [false, true] {
                let a11y = if alternatives {
                    A11yConfig::full()
                } else {
                    A11yConfig::none()
                };
                let scene = render_svg_with_config(
                    &ir,
                    &SvgRenderConfig {
                        backend: SvgBackend::Scene,
                        embed_theme_css: embedded,
                        a11y: a11y.clone(),
                        ..Default::default()
                    },
                );
                assert_eq!(
                    scene.matches("class=\"fm-quadrant-point\"").count(),
                    ir.nodes.len()
                );
                assert_eq!(
                    point_names(&scene),
                    point_names(&render(source, embedded, a11y))
                );
                assert!(!scene.contains(">HiHi: TopRight</text>"));
            }
        }
    }
}

#[test]
fn sparse_quadrant_heading_keeps_its_region_and_unnamed_points_keep_coordinates() {
    let ir = fm_parser::parse(concat!(
        "quadrantChart\nquadrant-4 Research\n",
        "HiHi: [0.9, 0.9]\nLoHi: [0.1, 0.9]\n",
        "LoLo: [0.1, 0.1]\nHiLo: [0.9, 0.1]\n",
    ))
    .ir;
    let attribute = |element: &str, name: &str| -> String {
        element
            .split_once(&format!("{name}=\""))
            .unwrap_or_else(|| panic!("missing {name}: {element}"))
            .1
            .split_once('"')
            .unwrap()
            .0
            .to_owned()
    };
    for backend in [SvgBackend::LegacyLayout, SvgBackend::Scene] {
        for embed_theme_css in [false, true] {
            let svg = render_svg_with_config(
                &ir,
                &SvgRenderConfig {
                    backend,
                    embed_theme_css,
                    a11y: A11yConfig::full(),
                    ..Default::default()
                },
            );
            assert_eq!(
                point_names(&svg),
                [
                    "HiHi: x 0.90, y 0.90",
                    "LoHi: x 0.10, y 0.90",
                    "LoLo: x 0.10, y 0.10",
                    "HiLo: Research",
                ],
                "{backend:?}, embedded={embed_theme_css}"
            );
            let centers: Vec<_> = svg
                .split('<')
                .filter(|tag| {
                    (tag.starts_with("circle ") || tag.starts_with("path "))
                        && tag.split_once('>').is_some_and(|(opening, _)| {
                            opening.contains("class=\"fm-quadrant-point\"")
                        })
                })
                .map(|tag| {
                    if tag.starts_with("circle ") {
                        (
                            attribute(tag, "cx").parse::<f32>().unwrap(),
                            attribute(tag, "cy").parse::<f32>().unwrap(),
                        )
                    } else {
                        // The scene's smooth circle starts at the right cardinal point.
                        let path = attribute(tag, "d");
                        let coordinates: Vec<f32> = path
                            .split(|ch: char| {
                                ch.is_ascii_alphabetic() || ch.is_whitespace() || ch == ','
                            })
                            .filter_map(|part| part.parse().ok())
                            .collect();
                        (coordinates[0] - 6.0, coordinates[1])
                    }
                })
                .collect();
            assert_eq!(centers.len(), 4);
            let heading: Vec<_> = svg
                .split("<text")
                .filter_map(|text| {
                    let (opening, body) = text.split_once('>')?;
                    body.starts_with("Research</text>").then_some(opening)
                })
                .collect();
            assert_eq!(
                heading.len(),
                1,
                "sparse slots must not add or duplicate headings"
            );
            let x = attribute(heading[0], "x").parse::<f32>().unwrap();
            let y = attribute(heading[0], "y").parse::<f32>().unwrap();
            assert!(
                x > (centers[0].0 + centers[2].0) / 2.0,
                "Q4 heading must be on the right"
            );
            assert!(
                y > (centers[0].1 + centers[2].1) / 2.0,
                "Q4 heading must be at the bottom"
            );
        }
    }
}

/// Ignore SVG serialization boundaries, retaining every visible non-whitespace character.
/// The direct renderer may put rows in tspans while the scene emits measured rows separately.
fn visible_content(svg: &str) -> String {
    let mut visible = String::new();
    for text in svg.split("<text").skip(1) {
        let Some((_, body)) = text.split_once('>') else {
            continue;
        };
        let Some((body, _)) = body.split_once("</text>") else {
            continue;
        };
        let mut in_tag = false;
        for ch in body.chars() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag && !ch.is_whitespace() => visible.push(ch),
                _ => {}
            }
        }
    }
    visible
}

#[test]
fn scene_quadrants_keep_complete_chart_content_and_authored_marker_styles() {
    let ir = fm_parser::parse(concat!(
        "quadrantChart\n",
        "title Product investment priorities\n",
        "x-axis Low effort --> High effort\n",
        "y-axis Low impact --> High impact\n",
        "quadrant-1 Invest next\nquadrant-2 Quick wins\nquadrant-3 Reconsider\nquadrant-4 Research first\n",
        "Customer onboarding 東京: [0.52, 0.52] radius: 18, color: #ef4444, stroke-color: #111827, stroke-width: 4px\n",
        "Reliable customer exports: [0.52, 0.52]\n",
        "Accessible product labels: [0.52, 0.52]\n",
    )).ir;
    assert_eq!(ir.nodes.len(), 3);
    for font_size in [15.0, 30.0] {
        for embedded in [false, true] {
            for theme in [
                fm_render_svg::ThemePreset::Default,
                fm_render_svg::ThemePreset::Dark,
            ] {
                let config = SvgRenderConfig {
                    font_size,
                    avg_char_width: font_size * 0.5,
                    embed_theme_css: embedded,
                    theme,
                    ..Default::default()
                };
                let direct = render_svg_with_config(&ir, &config);
                let scene_config = SvgRenderConfig {
                    backend: SvgBackend::Scene,
                    ..config
                };
                let scene = render_svg_with_config(&ir, &scene_config);
                assert_eq!(
                    visible_content(&scene),
                    visible_content(&direct),
                    "font={font_size}, embedded={embedded}, {theme:?}"
                );
                assert_eq!(
                    scene
                        .matches(">Product investment priorities</text>")
                        .count(),
                    1
                );
                for id in [
                    "quadrant-backgrounds",
                    "quadrant-axes",
                    "quadrant-grid",
                    "quadrant-leaders",
                ] {
                    assert!(scene.contains(&format!("id=\"{id}\"")), "missing {id}");
                }
                let styled_point = scene
                    .split("<path")
                    .find(|path| {
                        path.split_once('>').is_some_and(|(tag, _)| {
                            tag.contains("class=\"fm-quadrant-point\"")
                                && tag.contains("fill=\"#ef4444\"")
                        })
                    })
                    .expect("authored marker fill was lost");
                let opening = styled_point.split_once('>').unwrap().0;
                assert!(opening.contains("stroke=\"#111827\""));
                assert!(opening.contains("stroke-width=\"4\""));
                assert!(scene.contains("xml:space=\"preserve\""));
                assert_eq!(scene, render_svg_with_config(&ir, &scene_config));
            }
        }
    }
}
