//! The sequence autonumber element must be THEMED (bd-7hgxu).
//!
//! When bd-o02wn first emitted the number as its own `<text class="fm-sequence-number">`, it carried
//! no `fill` and no CSS rule existed for that class anywhere in the crate. The number therefore fell
//! back to the SVG default of black — on a diagram whose every other text run is themed, and
//! invisible against a dark theme's background.
//!
//! A class with no rule behind it is not styling, it is a name. That is the failure this file pins,
//! and the reason it asserts the RESOLVED colour rather than the presence of a class attribute:
//! the class was present the whole time the defect existed.
//!
//! ⚠️ THE DIGITS SIT ON A DISC. mermaid draws them on a `sequencenumber` circle of the LINE colour
//! and computes `sequenceNumberColor` to contrast with it. When the disc was added here the digits
//! kept `colors.text`, the colour of every other label, and text colour on line colour was dark on
//! dark: themed, present and unreadable. They now take the background colour, which contrasts with
//! the disc in every theme. That is readability, not a claim of palette parity.

use fm_render_svg::{SvgRenderConfig, render_svg_with_config};

const AUTONUMBERED: &str = "sequenceDiagram\n  autonumber\n  Alice->>Bob: Ping\n";

fn number_element(svg: &str) -> Option<&str> {
    let start = svg.find("<text")?;
    let mut rest = &svg[start..];
    loop {
        let end = rest.find("</text>")? + "</text>".len();
        let element = &rest[..end];
        if element.contains("class=\"fm-sequence-number\"") {
            return Some(element);
        }
        rest = &rest[end..];
        let next = rest.find("<text")?;
        rest = &rest[next..];
    }
}

fn fill_of(element: &str) -> Option<&str> {
    let start = element.find("fill=\"")? + "fill=\"".len();
    let end = element[start..].find('"')? + start;
    Some(&element[start..end])
}

fn render(theme: fm_render_svg::ThemePreset) -> String {
    let ir = fm_parser::parse(AUTONUMBERED).ir;
    render_svg_with_config(
        &ir,
        &SvgRenderConfig {
            theme,
            ..SvgRenderConfig::default()
        },
    )
}

/// THE DEFECT: the number carries an explicit fill, not the browser's default.
#[test]
fn the_sequence_number_carries_an_explicit_fill() {
    let svg = render(fm_render_svg::ThemePreset::Default);
    let element = number_element(&svg).expect("the autonumber element is emitted");

    // CONTROL ON THE FIXTURE: it really is the number element, not some other text run.
    assert!(
        element.ends_with(">1</text>"),
        "the element under test is not the autonumber: {element:?}"
    );
    assert!(
        fill_of(element).is_some(),
        "the number has no fill and will render as the SVG default black: {element:?}"
    );
}

/// IT ACTUALLY FOLLOWS THE THEME: the resolved colour differs between light and dark.
///
/// This is the assertion that would have failed before the fix and that a hardcoded fill would still
/// fail. Asserting merely that *a* fill exists is not enough — a literal `fill="#000"` satisfies
/// that while reproducing the bug on every dark theme.
#[test]
fn the_sequence_number_colour_follows_the_theme() {
    let light = render(fm_render_svg::ThemePreset::Default);
    let dark = render(fm_render_svg::ThemePreset::Dark);

    let light_fill = fill_of(number_element(&light).expect("light autonumber element"))
        .expect("light fill")
        .to_string();
    let dark_fill = fill_of(number_element(&dark).expect("dark autonumber element"))
        .expect("dark fill")
        .to_string();

    assert_ne!(
        light_fill, dark_fill,
        "the number renders the same colour in both themes, so it is hardcoded rather than themed"
    );
}

/// The digits contrast with the disc they are drawn on, which is what makes them readable.
///
/// Joined to the disc in the SAME document rather than to hardcoded hex values: pinning literals
/// would turn any palette change into a failure here for no reason, and would not assert the
/// property that matters. The digits take the diagram's background colour, so they are also
/// asserted against the label colour they must NOT copy, which is the dark-on-dark defect.
#[test]
fn the_sequence_number_contrasts_with_its_disc() {
    for theme in [
        fm_render_svg::ThemePreset::Default,
        fm_render_svg::ThemePreset::Dark,
    ] {
        let svg = render(theme);
        let number = number_element(&svg).expect("autonumber element");
        let number_fill = fill_of(number).expect("number fill");

        let disc_fill = svg
            .split("<circle")
            .find(|chunk| chunk.contains("fm-sequence-number-background"))
            .and_then(|chunk| fill_of(chunk))
            .expect("CONTROL FAILED: no disc behind the number");
        let label_fill = svg
            .split("<text")
            .find(|chunk| chunk.contains(">Ping</text>"))
            .and_then(|chunk| fill_of(chunk))
            .expect("CONTROL FAILED: the message label carries no fill to compare against");

        assert_ne!(
            number_fill, disc_fill,
            "{theme:?}: the digits are the colour of their own disc"
        );
        assert_ne!(
            number_fill, label_fill,
            "{theme:?}: the digits copy the label colour, which is dark on the dark disc"
        );
    }
}
