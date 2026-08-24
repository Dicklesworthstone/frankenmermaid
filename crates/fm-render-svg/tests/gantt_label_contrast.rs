//! A gantt task label must contrast with WHATEVER IT SITS ON (bd-u0x67).
//!
//! Task bars are FIXED pastels — `#93c5fd` normal, `#fca5a5` critical, `#86efac` done, `#94a3b8`
//! active — identical in every theme. The label used `colors.text`, which flips to near-white on a
//! dark theme. So in dark mode every task label in every gantt chart measured against its own bar:
//!
//!   normal 1.72:1   critical 1.81:1   done 1.34:1   active 2.45:1
//!
//! against 6.65–12.15:1 in light. All four task types, every chart — not an edge case.
//!
//! ⚠️ WHY THE EARLIER AUDITS MISSED IT. The contrast sweeps behind bd-c14jf and bd-4rlrx compare
//! every label to the PAGE background. That is right for free-floating text and wrong for a label
//! drawn inside a filled shape, where the background is the shape's own fill. Measured against the
//! page, these labels looked perfect (17.06:1) while being invisible on screen.
//!
//! PLACEMENT DECIDES WHICH TEST APPLIES, and both are asserted here. A short bar pushes its label
//! OUTSIDE, onto the page, where `colors.text` is correct — colouring every label for the bar would
//! have fixed the inside case by breaking the outside one.

use fm_render_svg::{SvgRenderConfig, ThemePreset, render_svg_with_config};

/// One task of each type, all long enough to hold their labels inside.
const ALL_TYPES: &str = "gantt\n  dateFormat YYYY-MM-DD\n  title T\n  section S\n  \
                         Alpha :a1, 2024-01-01, 30d\n  Crit :crit, c1, 2024-02-01, 30d\n  \
                         Done :done, d1, 2024-03-01, 30d\n  Active :active, ac1, 2024-04-01, 30d\n";

/// A one-day task beside a long one: its label cannot fit and is placed outside the bar.
const OUTSIDE: &str = "gantt\n  dateFormat YYYY-MM-DD\n  title T\n  section S\n  \
                       VeryLongTaskNameHere :a1, 2024-01-01, 1d\n  Normal :a2, 2024-01-02, 60d\n";

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

fn attr(chunk: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=\"");
    let start = chunk.find(&needle)? + needle.len();
    let end = chunk[start..].find('"')? + start;
    Some(chunk[start..end].to_string())
}

fn number(chunk: &str, key: &str) -> Option<f64> {
    attr(chunk, key)?.parse().ok()
}

/// Every task bar as `(x, y, w, h, fill)`.
fn bars(svg: &str) -> Vec<(f64, f64, f64, f64, String)> {
    svg.split("<rect")
        .skip(1)
        .filter_map(|chunk| {
            let head = &chunk[..chunk.find('>')?];
            if !head.contains("class=\"fm-gantt-task ") {
                return None;
            }
            Some((
                number(head, "x")?,
                number(head, "y")?,
                number(head, "width")?,
                number(head, "height")?,
                attr(head, "fill")?,
            ))
        })
        .collect()
}

/// Every task label as `(x, y, fill, text)`.
fn labels(svg: &str) -> Vec<(f64, f64, String, String)> {
    svg.split("<text")
        .skip(1)
        .filter_map(|chunk| {
            let close = chunk.find('>')?;
            let head = &chunk[..close];
            if !head.contains("class=\"fm-gantt-task-label\"") {
                return None;
            }
            let body = &chunk[close + 1..chunk.find("</text>")?];
            Some((
                number(head, "x")?,
                number(head, "y")?,
                attr(head, "fill")?,
                body.to_string(),
            ))
        })
        .collect()
}

fn page_background(svg: &str) -> String {
    let start = svg.find("--fm-bg:").expect("theme declares a background") + "--fm-bg:".len();
    let rest = svg[start..].trim_start();
    let offset = svg.len() - rest.len();
    let end = rest.find(';').expect("declaration ends");
    svg[offset..offset + end].trim().to_string()
}

fn render(source: &str, theme: ThemePreset) -> String {
    render_svg_with_config(
        &fm_parser::parse(source).ir,
        &SvgRenderConfig {
            theme,
            ..SvgRenderConfig::default()
        },
    )
}

/// EVERY task type, in BOTH themes, measured against ITS OWN BAR.
#[test]
fn task_labels_meet_wcag_aa_against_their_own_bar() {
    for theme in [ThemePreset::Default, ThemePreset::Dark] {
        let svg = render(ALL_TYPES, theme);
        let bars = bars(&svg);
        assert!(!bars.is_empty(), "CONTROL FAILED: no task bars rendered");

        let mut measured = 0;
        for (x, y, fill, text) in labels(&svg) {
            let Some(bar) = bars
                .iter()
                .find(|(bx, by, bw, bh, _)| *bx <= x && x <= bx + bw && *by <= y && y <= by + bh)
            else {
                continue; // outside the bar: covered by the sibling test below
            };
            let ratio = contrast(&fill, &bar.4);
            assert!(
                ratio >= 4.5,
                "{theme:?}: label {text:?} is {fill} on its {} bar = {ratio:.2}:1, below the WCAG \
                 AA floor of 4.5:1",
                bar.4
            );
            measured += 1;
        }
        assert_eq!(
            measured, 4,
            "{theme:?}: expected all four task types to hold their label inside the bar, measured \
             {measured}"
        );
    }
}

/// CONTROL: a label placed OUTSIDE its bar is measured against the PAGE, and still passes.
///
/// This is the case a naive fix breaks. Colouring every label to contrast with the bar makes an
/// outside label the page's own background colour — a 1:1 ratio, invisible — while every assertion
/// in the test above still passes.
#[test]
fn a_label_placed_outside_its_bar_contrasts_with_the_page() {
    for theme in [ThemePreset::Default, ThemePreset::Dark] {
        let svg = render(OUTSIDE, theme);
        let bars = bars(&svg);
        let background = page_background(&svg);

        let mut outside = 0;
        for (x, y, fill, text) in labels(&svg) {
            let inside = bars
                .iter()
                .any(|(bx, by, bw, bh, _)| *bx <= x && x <= bx + bw && *by <= y && y <= by + bh);
            if inside {
                continue;
            }
            let ratio = contrast(&fill, &background);
            assert!(
                ratio >= 4.5,
                "{theme:?}: outside label {text:?} is {fill} on the page {background} = \
                 {ratio:.2}:1, below the WCAG AA floor"
            );
            outside += 1;
        }
        assert_eq!(
            outside, 1,
            "{theme:?}: CONTROL FAILED — expected exactly one label pushed outside its bar, got \
             {outside}; the fixture no longer exercises the outside path"
        );
    }
}

/// CONTROL: the light theme's labels are UNCHANGED by this fix.
///
/// The defect was dark-only. Pinning the light colour means a future change to the picker cannot
/// quietly restyle every gantt chart in the default theme.
///
/// RE-PINNED, not relaxed: a52d0587 moved the Default preset's `text` from `#1a1a2e` to `#1e293b`
/// as a deliberate palette change, and this control did its job by failing. It still asserts ONE
/// exact colour for EVERY label, which is the whole property — what changed is which colour the
/// theme declares, not how tightly this holds the picker to it.
#[test]
fn the_default_theme_label_colour_is_unchanged() {
    let svg = render(ALL_TYPES, ThemePreset::Default);
    for (_, _, fill, text) in labels(&svg) {
        assert_eq!(
            fill, "#1e293b",
            "the default theme's label colour moved for {text:?}"
        );
    }
}

/// CONTROL: the chosen colour is THEME-DERIVED, not a hardcoded black.
///
/// Hardcoding `#000000` would satisfy every contrast assertion above while ignoring the theme —
/// the failure mode bd-7hgxu was about.
#[test]
fn the_inside_label_colour_comes_from_the_theme() {
    let dark = render(ALL_TYPES, ThemePreset::Dark);
    let background = page_background(&dark);
    let inside_fill = labels(&dark).first().expect("a label").2.clone();
    assert_eq!(
        inside_fill, background,
        "the dark theme's inside-label colour is not the theme's own background colour"
    );
}
