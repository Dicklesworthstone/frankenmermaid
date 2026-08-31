//! The C4 stereotype label must be legible against its own theme background (bd-4rlrx).
//!
//! Mermaid 11.15.0's pinned default bundle paints the C4 Person surface with a navy card and a
//! white stereotype. It must stay legible and must not drift to a generic document-text color.
//!
//!   default  #475569 on #08427B  =  1.73:1   effectively invisible
//!
//! Both fail WCAG AA (4.5:1) and the 3:1 large-text floor.
//!
//! ⚠️ IT WAS ALREADY THEMED. The colour really did move between light and dark, so every check of
//! the form "does this follow the theme?" — including the one I wrote for bd-7hgxu — passes on the
//! broken code. Only a MEASURED contrast catches it. That is the whole reason these tests compute a
//! ratio instead of comparing colours.
//!
//! Its siblings in the same box, `fm-c4-name` and `fm-c4-description`, already used `colors.text`.
//! The visual hierarchy is carried by size (0.78x) and weight (600), not by making the label
//! unreadable.
//!
//! BOTH RENDER PATHS are exercised. The streaming fast path and the `Element` path each had their
//! own copy of the wrong fill, and a fix to one would leave the other broken for whichever diagram
//! happens to take it.

use fm_render_svg::{SvgRenderConfig, ThemePreset, render_svg_with_config};

const C4: &str = "C4Context\n  title C\n  Person(a, \"A\", \"desc\")\n";

fn luminance(hex: &str) -> f64 {
    let hex = hex.trim_start_matches('#');
    let channel = |i: usize| {
        let v = u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0) as f64 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4)
}

fn contrast(a: &str, b: &str) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

fn fill_of<'a>(svg: &'a str, class: &str) -> Option<&'a str> {
    let marker = format!("class=\"{class}\"");
    for chunk in svg.split("<text").skip(1) {
        let end = chunk.find('>')?;
        let attrs = &chunk[..end];
        if attrs.contains(&marker) {
            let start = attrs.find("fill=\"")? + "fill=\"".len();
            let stop = attrs[start..].find('"')? + start;
            return Some(&attrs[start..stop]);
        }
    }
    None
}

fn person_card_fill(svg: &str) -> Option<&str> {
    let node = svg.find("data-id=\"a\"")?;
    let rect = svg[node..].find("<rect ")? + node;
    let attrs_end = svg[rect..].find('>')? + rect;
    let attrs = &svg[rect..attrs_end];
    if let Some(style) = attrs.find("style=\"") {
        let style = &attrs[style + "style=\"".len()..];
        if let Some(fill) = style.strip_prefix("fill:") {
            return Some(&fill[..fill.find(';').unwrap_or(fill.len())]);
        }
    }
    let start = attrs.find("fill=\"")? + "fill=\"".len();
    let stop = attrs[start..].find('"')? + start;
    Some(&attrs[start..stop])
}

fn document_background(svg: &str) -> Option<&str> {
    let start = svg.find("--fm-bg:")? + "--fm-bg:".len();
    let rest = svg[start..].trim_start();
    let offset = svg.len() - rest.len();
    let end = rest.find(';')?;
    Some(svg[offset..offset + end].trim())
}

/// `streaming` selects the fast path; the slow `Element` path is reached by asking for source spans,
/// which every fast-path gate in this crate excludes.
fn render(theme: ThemePreset, streaming: bool) -> String {
    render_svg_with_config(
        &fm_parser::parse(C4).ir,
        &SvgRenderConfig {
            theme,
            include_source_spans: !streaming,
            ..SvgRenderConfig::default()
        },
    )
}

/// THE CONTRACT, in both themes and BOTH render paths.
#[test]
fn the_c4_stereotype_label_meets_wcag_aa_on_both_paths() {
    for streaming in [true, false] {
        for theme in [ThemePreset::Default, ThemePreset::Dark] {
            let svg = render(theme, streaming);
            let bg = if theme == ThemePreset::Default {
                person_card_fill(&svg).expect("the default C4 Person card is rendered")
            } else {
                document_background(&svg).expect("the document declares a background")
            };
            let fill = fill_of(&svg, "fm-c4-type-label").unwrap_or_else(|| {
                panic!("CONTROL FAILED: no stereotype label rendered (streaming={streaming})")
            });
            let ratio = contrast(fill, bg);
            assert!(
                ratio >= 4.5,
                "streaming={streaming} {theme:?}: the stereotype is {fill} on {bg} = {ratio:.2}:1, \
                 below the WCAG AA floor of 4.5:1"
            );

            // Planted negative: the old dark muted-text class is below AA against the actual C4
            // Person card. This proves the measured threshold can reject the defect rather than
            // merely accepting the current color.
            let planted_bad = "#475569";
            let planted_ratio = contrast(planted_bad, bg);
            assert!(
                planted_ratio < 4.5,
                "CONTROL FAILED: planted bad color {planted_bad} on {bg} unexpectedly passed at \
                 {planted_ratio:.2}:1"
            );
        }
    }
}

/// Mermaid's pinned 11.15.0 default colors are exact contracts in addition to the contrast floor.
#[test]
fn default_theme_stereotype_matches_mermaid_primary_text_color() {
    for streaming in [true, false] {
        let svg = render(ThemePreset::Default, streaming);
        assert_eq!(
            fill_of(&svg, "fm-c4-type-label"),
            Some("#FFFFFF"),
            "streaming={streaming}: C4 stereotype must use Mermaid 11.15.0 default #FFFFFF"
        );
        assert_eq!(
            person_card_fill(&svg),
            Some("#08427B"),
            "streaming={streaming}: C4 Person card must use Mermaid 11.15.0 default #08427B"
        );
    }
}

/// Non-default themes continue to derive the stereotype from the local theme text slot.
///
/// Mermaid's default C4 contract is deliberately more specific (`#333`), so it has its own exact
/// assertion above rather than accidentally inheriting FrankenMermaid's broader default palette.
#[test]
fn non_default_stereotype_is_painted_like_the_name_beside_it() {
    for streaming in [true, false] {
        for theme in [ThemePreset::Dark] {
            let svg = render(theme, streaming);
            assert_eq!(
                fill_of(&svg, "fm-c4-type-label"),
                fill_of(&svg, "fm-c4-name"),
                "streaming={streaming} {theme:?}: the stereotype and the name are painted differently"
            );
        }
    }
}

/// CONTROL: the colour is THEME-DERIVED, not a literal that happens to pass.
///
/// Weaker than it looks, and deliberately kept anyway: the ORIGINAL bug would have passed this test,
/// because `cluster_stroke` is themed too. It guards the other failure mode — someone "fixing"
/// contrast by hardcoding black — and the file header says why it cannot stand alone.
#[test]
fn the_stereotype_colour_still_follows_the_theme() {
    assert_ne!(
        fill_of(&render(ThemePreset::Default, true), "fm-c4-type-label"),
        fill_of(&render(ThemePreset::Dark, true), "fm-c4-type-label"),
        "the stereotype renders the same colour in both themes, so it is hardcoded"
    );
}

/// NON-VACUITY: the label is actually present, with its `<<…>>` delimiters.
///
/// Every assertion above is about a colour; if the label stopped rendering they would all fail on
/// the lookup instead, which is a confusing way to learn the element is gone.
#[test]
fn the_stereotype_label_is_rendered_at_all() {
    for streaming in [true, false] {
        let svg = render(ThemePreset::Default, streaming);
        assert!(
            svg.contains("class=\"fm-c4-type-label\"") && svg.contains(">&lt;&lt;person>></text>"),
            "streaming={streaming}: the stereotype label is missing or reworded"
        );
    }
}
