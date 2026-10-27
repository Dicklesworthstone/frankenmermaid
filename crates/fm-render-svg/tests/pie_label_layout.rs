//! Percentages, wrapped legends and the exported frame must agree on the same measured content.

use fm_render_svg::{SvgBackend, SvgRenderConfig, ThemePreset, render_svg_with_config};

const CROWDED: &str = "pie showData\n  title Annual platform investment\n  \"Core platform and shared infrastructure\" : 90\n  \"Customer identity and access management\" : 1\n  \"International analytics and reporting\" : 1\n  \"Developer productivity and delivery tooling\" : 1\n  \"Enterprise integrations and partner enablement\" : 1\n  \"Compliance evidence and audit automation\" : 1\n  \"Regional customer support operations\" : 1\n  \"Security monitoring and incident response\" : 1\n  \"Data retention and recovery services\" : 1\n  \"Partner onboarding and documentation\" : 1\n  \"Accessibility and localization\" : 1\n";

fn attr<'a>(tag: &'a str, name: &str) -> &'a str {
    let key = format!("{name}=\"");
    let tail = tag
        .split_once(&key)
        .unwrap_or_else(|| panic!("missing {name}: {tag}"))
        .1;
    tail.split_once('"').unwrap().0
}

fn css_binding<'a>(svg: &'a str, name: &str) -> &'a str {
    let declaration = format!("{name}:");
    svg.split_once(&declaration)
        .unwrap_or_else(|| panic!("missing theme binding {name}"))
        .1
        .split_once(';')
        .expect("theme declaration must end with a semicolon")
        .0
        .trim()
}

fn resolved_color<'a>(svg: &'a str, paint: &'a str, embedded_css: bool) -> &'a str {
    if let Some(variable) = paint.strip_prefix("var(") {
        assert!(
            embedded_css,
            "portable text must not depend on CSS variables"
        );
        let variable = variable
            .strip_suffix(')')
            .expect("complete var() expression");
        let name = variable.split(',').next().unwrap().trim();
        // A fallback alone cannot certify the selected theme: require the actual binding.
        css_binding(svg, name)
    } else {
        paint
    }
}

fn text_contrast(foreground: &str, background: &str) -> f64 {
    let luminance = |hex: &str| {
        let hex = hex.strip_prefix('#').expect("theme colors are hex");
        assert_eq!(hex.len(), 6);
        let channel = |index| {
            let value = f64::from(u8::from_str_radix(&hex[index..index + 2], 16).unwrap()) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4)
    };
    let foreground = luminance(foreground);
    let background = luminance(background);
    (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
}

#[derive(Debug)]
struct Box2d {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

fn text_boxes(svg: &str, config: &SvgRenderConfig) -> Vec<Box2d> {
    let metrics = config.font_metrics();
    svg.split("<text ")
        .skip(1)
        .filter_map(|tail| {
            let (tag, body) = tail.split_once('>')?;
            if !tag.contains("class=\"fm-pie-") {
                return None;
            }
            let text = body.split_once("</text>")?.0;
            assert!(!text.contains('<'), "pie rows must be complete text leaves");
            let x: f32 = attr(tag, "x").parse().unwrap();
            let y: f32 = attr(tag, "y").parse().unwrap();
            let font: f32 = attr(tag, "font-size").parse().unwrap();
            let w = metrics.estimate_preserved_width(text, font, config.avg_char_width);
            let x = match attr(tag, "text-anchor") {
                "middle" => x - w * 0.5,
                "end" => x - w,
                _ => x,
            };
            Some(Box2d {
                x,
                y: y - font * 0.65,
                w,
                h: font * 1.3,
            })
        })
        .collect()
}

#[test]
fn exported_pie_text_is_visible_and_disjoint_with_custom_fonts_and_portable_themes() {
    let ir = fm_parser::parse(CROWDED).ir;
    for font_size in [15.0, 24.0, 36.0] {
        for theme in [ThemePreset::Default, ThemePreset::Dark] {
            for embed_theme_css in [true, false] {
                let config = SvgRenderConfig {
                    font_size,
                    theme,
                    embed_theme_css,
                    ..Default::default()
                };
                let svg = render_svg_with_config(&ir, &config);
                let tag = svg
                    .split_once("<svg ")
                    .unwrap()
                    .1
                    .split_once('>')
                    .unwrap()
                    .0;
                let viewbox: Vec<f32> = attr(tag, "viewBox")
                    .split_whitespace()
                    .map(|value| value.parse().unwrap())
                    .collect();
                let boxes = text_boxes(&svg, &config);
                assert!(
                    boxes.len() > 30,
                    "long legend entries must wrap into complete rows"
                );
                for (index, bounds) in boxes.iter().enumerate() {
                    assert!(
                        bounds.x >= -0.1
                            && bounds.y >= -0.1
                            && bounds.x + bounds.w <= viewbox[2] + 0.1
                            && bounds.y + bounds.h <= viewbox[3] + 0.1,
                        "text escapes export: {bounds:?}, {viewbox:?}"
                    );
                    for other in &boxes[index + 1..] {
                        assert!(
                            !(bounds.x < other.x + other.w - 0.1
                                && other.x < bounds.x + bounds.w - 0.1
                                && bounds.y < other.y + other.h - 0.1
                                && other.y < bounds.y + bounds.h - 0.1),
                            "text rows collide: {bounds:?}, {other:?}"
                        );
                    }
                }
                assert_eq!(svg.matches(">90%</text>").count(), 1);
                assert_eq!(svg.matches(">1%</text>").count(), 10);
                assert_eq!(svg.matches("class=\"fm-pie-label-leader\"").count(), 11);
                assert!(svg.contains("[90]"));
                assert_eq!(svg, render_svg_with_config(&ir, &config));
            }
        }
    }
}

#[test]
fn pie_scene_backend_has_real_wedges_and_retains_every_legend_word() {
    let ir = fm_parser::parse(CROWDED).ir;
    let config = SvgRenderConfig {
        backend: SvgBackend::Scene,
        ..Default::default()
    };
    let svg = render_svg_with_config(&ir, &config);
    let layout = fm_layout::layout_diagram(&ir);
    let scene = fm_layout::build_render_scene(&ir, &layout);
    let scene_svg = fm_render_svg::render_scene_to_svg(&scene, &config);
    for output in [&svg, &scene_svg] {
        for word in [
            "investment",
            "infrastructure",
            "identity",
            "International",
            "productivity",
            "enablement",
            "Compliance",
            "Regional",
            "incident",
            "retention",
            "documentation",
            "localization",
        ] {
            assert!(output.contains(word), "pie content was lost: {word}");
        }
        assert!(output.contains("90%"));
        assert!(output.contains("[90]"));
        assert!(output.contains("<path"));
    }
    assert!(
        scene_svg.contains('C'),
        "shared scene must carry circular wedge curves"
    );
    assert!(!scene_svg.contains("diagram-nodes"));
}

#[test]
fn pie_near_full_sector_does_not_paint_over_its_positive_neighbor() {
    let ir = fm_parser::parse("pie\n\"Small\":0.00001\n\"Large\":1\n").ir;
    let svg = render_svg_with_config(&ir, &SvgRenderConfig::default());
    assert!(!svg.contains("fm-pie-slice-full"));
    assert_eq!(svg.matches("class=\"fm-pie-slice\"").count(), 2);
}

#[test]
fn pie_scene_uses_the_selected_theme_and_preserves_explicit_slice_colors() {
    let mut ir = fm_parser::parse(CROWDED).ir;
    ir.meta
        .theme_overrides
        .theme_variables
        .insert("pie1".to_string(), "#123abc".to_string());
    for embed_theme_css in [true, false] {
        let config = SvgRenderConfig {
            backend: SvgBackend::Scene,
            theme: ThemePreset::Dark,
            embed_theme_css,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        let theme = fm_render_svg::Theme::from_preset(ThemePreset::Dark);
        assert!(svg.contains("fill=\"#123abc\""));
        if embed_theme_css {
            assert_eq!(css_binding(&svg, "--fm-text-color"), theme.colors.text);
            assert_eq!(css_binding(&svg, "--fm-bg"), theme.colors.background);
        }
        let mut visible_titles = 0;
        for text in svg.split("<text ").skip(1) {
            let (tag, body) = text.split_once('>').unwrap();
            let fill = resolved_color(&svg, attr(tag, "fill"), embed_theme_css);
            assert_eq!(fill, theme.colors.text);
            assert!(text_contrast(fill, &theme.colors.background) >= 4.5);
            if body.split_once("</text>").unwrap().0 == "Annual platform investment" {
                visible_titles += 1;
            }
        }
        assert_eq!(
            visible_titles, 1,
            "the measured pie title must not gain a second generic heading"
        );
        assert!(!svg.contains("class=\"fm-diagram-title\""));
        assert!(svg.contains(&format!("stroke=\"{}\"", theme.colors.background)));
        assert!(svg.contains(&format!("stroke=\"{}\"", theme.colors.text)));
    }
}
