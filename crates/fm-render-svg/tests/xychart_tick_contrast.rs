//! xychart axis tick labels must be legible against their own theme background (bd-c14jf).
//!
//! Mermaid 11.15.0's pinned default bundle derives y-axis labels from `primaryTextColor`, whose
//! exact default is `#333`; the old edge color is not a substitute.
//!
//! MEASURED against the theme's own background, before the fix:
//!
//!   default  #94a3b8 on #fafbfc  =  2.47:1   <- fails WCAG AA (4.5:1) and the 3:1 large-text floor
//!   dark     #94a3b8 on #0f172a  =  6.96:1
//!   x-tick, both themes                        16.46:1 / 17.06:1
//!
//! So on the SHIPPED DEFAULT theme one axis was legible and the other was not.
//!
//! These tests assert the COMPUTED CONTRAST RATIO, not a colour string. A hex assertion would pin
//! today's palette and fail the next time anyone retunes a theme, while saying nothing about the
//! property that actually matters — that the label can be read.

use fm_render_svg::{SvgRenderConfig, ThemePreset, render_svg_with_config};

const CHART: &str = "xychart-beta\n  title \"X\"\n  x-axis [a, b]\n  bar [1, 2]\n";

/// WCAG 2.x relative luminance.
fn luminance(hex: &str) -> f64 {
    let hex = hex.trim_start_matches('#');
    let expanded;
    let hex = if hex.len() == 3 {
        expanded = hex
            .chars()
            .flat_map(|channel| [channel, channel])
            .collect::<String>();
        expanded.as_str()
    } else {
        hex
    };
    let channel = |index: usize| {
        let value = u8::from_str_radix(&hex[index..index + 2], 16).unwrap_or(0) as f64 / 255.0;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4)
}

fn contrast(a: &str, b: &str) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

fn render(theme: ThemePreset) -> String {
    render_svg_with_config(
        &fm_parser::parse(CHART).ir,
        &SvgRenderConfig {
            theme,
            ..SvgRenderConfig::default()
        },
    )
}

/// The fill of the first `<text>` carrying `class`.
fn fill_of<'a>(svg: &'a str, class: &str) -> Option<&'a str> {
    let marker = format!("class=\"{class}\"");
    for chunk in svg.split("<text").skip(1) {
        let Some(end) = chunk.find('>') else { continue };
        let attrs = &chunk[..end];
        if attrs.contains(&marker) {
            let start = attrs.find("fill=\"")? + "fill=\"".len();
            let stop = attrs[start..].find('"')? + start;
            return Some(&attrs[start..stop]);
        }
    }
    None
}

/// The theme background the chart is drawn on, read from the document's own stylesheet.
fn background(svg: &str) -> Option<&str> {
    let start = svg.find("--fm-bg:")? + "--fm-bg:".len();
    let rest = svg[start..].trim_start();
    let offset = svg.len() - rest.len();
    let end = rest.find(';')?;
    Some(svg[offset..offset + end].trim())
}

/// BOTH axes must clear WCAG AA for normal text, in BOTH themes.
#[test]
fn axis_tick_labels_meet_wcag_aa_contrast_in_both_themes() {
    for theme in [ThemePreset::Default, ThemePreset::Dark] {
        let svg = render(theme);
        let bg = background(&svg).expect("the document declares a background");

        for class in ["fm-xychart-y-tick", "fm-xychart-x-tick", "fm-xychart-title"] {
            let fill = fill_of(&svg, class)
                .unwrap_or_else(|| panic!("CONTROL FAILED: {class} was not rendered with a fill"));
            let ratio = contrast(fill, bg);
            assert!(
                ratio >= 4.5,
                "{theme:?}: {class} is {fill} on {bg} = {ratio:.2}:1, below the WCAG AA floor of \
                 4.5:1 for normal text"
            );
        }

        // Planted negative: the old edge color must remain below AA in the default theme. This
        // demonstrates that the computed-ratio assertion has a concrete rejecting case.
        if theme == ThemePreset::Default {
            let planted_bad = "#94a3b8";
            let planted_ratio = contrast(planted_bad, bg);
            assert!(
                planted_ratio < 4.5,
                "CONTROL FAILED: planted bad color {planted_bad} on {bg} unexpectedly passed at \
                 {planted_ratio:.2}:1"
            );
        }
    }
}

/// Mermaid's pinned 11.15.0 default y-axis label color is an exact SVG contract.
#[test]
fn default_y_ticks_match_mermaid_primary_text_color() {
    assert_eq!(
        fill_of(&render(ThemePreset::Default), "fm-xychart-y-tick"),
        Some("#333"),
        "xychart y ticks must use Mermaid 11.15.0 default primaryTextColor"
    );
}

/// Non-default themes retain the local theme's shared axis-text color.
///
/// The default y-axis contract is Mermaid-specific (`#333`) and is pinned separately above.
#[test]
fn non_default_axes_paint_their_tick_labels_the_same_colour() {
    for theme in [ThemePreset::Dark] {
        let svg = render(theme);
        assert_eq!(
            fill_of(&svg, "fm-xychart-y-tick"),
            fill_of(&svg, "fm-xychart-x-tick"),
            "{theme:?}: the two axes paint their tick labels differently"
        );
    }
}

/// CONTROL: the colour is THEME-DERIVED, not a literal that happens to pass.
///
/// Without this, hardcoding a high-contrast value — black — would satisfy both tests above while
/// reintroducing the original class of bug on every dark theme.
#[test]
fn the_tick_colour_follows_the_theme() {
    let light = render(ThemePreset::Default);
    let dark = render(ThemePreset::Dark);
    assert_ne!(
        fill_of(&light, "fm-xychart-y-tick"),
        fill_of(&dark, "fm-xychart-y-tick"),
        "the y-axis tick colour is identical in both themes, so it is hardcoded rather than themed"
    );
}
