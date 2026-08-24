//! Pie slices must carry an accessible name (bd-uf3p1).
//!
//! They shipped as bare self-closing shapes — no `data-id`, no `role`, no `aria-label`, no
//! `<title>`. Measured across the corpus, FOUR chart types (gantt, pie, quadrant, xychart) emitted
//! zero per-element accessibility affordances while the other FIFTEEN did, including the chart-like
//! sankey, journey, timeline, packet and kanban. A screen reader got the document `<desc>` and
//! nothing per wedge.
//!
//! ⚠️ THE NAME MIRRORS THE LEGEND, `showData` included. I first made the share unconditional,
//! reasoning that a wedge's angle conveys its proportion to a sighted reader and an accessible name
//! should carry what the visual conveys. That broke
//! `pie_without_showdata_omits_value_and_percentage_labels`, which asserts DOCUMENT-WIDE that
//! `showData: false` publishes no numbers anywhere — a real pre-existing contract about what the
//! author chose to disclose. Narrowing that gate to fit my preference would have been exactly the
//! move this project forbids, so the name follows it instead and the product question is raised on
//! the bead. The test below pins the resulting behaviour so the decision is visible, not implicit.
//!
//! Only pie is addressed here. gantt, quadrant and xychart have the same gap and are NOT covered by
//! this bead — stated rather than left for someone to assume from the file's existence.

use fm_render_svg::{A11yConfig, SvgRenderConfig, render_svg_with_config};

const WITH_DATA: &str = "pie showData\n  title P\n  \"Alpha\" : 10\n  \"Beta\" : 5\n";
const WITHOUT_DATA: &str = "pie title P\n  \"Alpha\" : 10\n  \"Beta\" : 5\n";
const WHOLE_PIE: &str = "pie showData\n  title P\n  \"Only\" : 7\n";

fn render(source: &str, a11y: A11yConfig) -> String {
    render_svg_with_config(
        &fm_parser::parse(source).ir,
        &SvgRenderConfig {
            a11y,
            ..SvgRenderConfig::default()
        },
    )
}

/// The `<title>` text of every pie slice, in document order.
fn slice_names(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(at) = rest.find("class=\"fm-pie-slice") {
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

/// THE CAPABILITY: every wedge is named, with its label, value and share.
#[test]
fn every_pie_slice_carries_an_accessible_name() {
    let svg = render(WITH_DATA, A11yConfig::full());
    assert_eq!(
        slice_names(&svg),
        vec!["Alpha: 10 (66.7%)", "Beta: 5 (33.3%)"],
        "pie slices are missing or mis-formatting their accessible names"
    );
}

/// A single slice fills the circle and is emitted as a `<circle>`, not a `<path>`.
///
/// A separate code path, and therefore a separate test — naming the wedge shape only would have
/// left a one-slice chart silent.
#[test]
fn a_whole_pie_slice_is_named_too() {
    let svg = render(WHOLE_PIE, A11yConfig::full());
    assert_eq!(slice_names(&svg), vec!["Only: 7 (100.0%)"]);
    // CONTROL: it really is the full-circle path being exercised.
    assert!(
        svg.contains("fm-pie-slice-full"),
        "CONTROL FAILED: the single-slice chart did not take the full-circle path"
    );
}

/// `showData: false` names the slice by LABEL ONLY, publishing no numbers.
///
/// Pins the decision described in the header: the accessible name honours the author's disclosure
/// choice rather than overriding it. If that is ever revisited as a product question, this is the
/// test that has to change, deliberately.
#[test]
fn the_name_omits_numbers_when_show_data_is_off() {
    let svg = render(WITHOUT_DATA, A11yConfig::full());
    assert_eq!(
        slice_names(&svg),
        vec!["Alpha", "Beta"],
        "the accessible name published numbers the author asked to withhold"
    );
    // CONTROL: no number reaches the document at all, which is the contract the existing
    // `pie_without_showdata_omits_value_and_percentage_labels` gate asserts.
    assert!(
        !svg.contains("66.7%"),
        "a percentage leaked into a showData:false chart"
    );
}

/// The spoken name and the printed legend AGREE digit for digit when both are shown.
///
/// `write_number_into` renders 66.7 as `66.70`; the legend uses `{:.1}`. Pinning them to each other
/// catches a formatting drift that would make a screen reader read different numbers than the chart
/// displays.
#[test]
fn the_accessible_name_matches_the_printed_legend() {
    let svg = render(WITH_DATA, A11yConfig::full());
    for name in slice_names(&svg) {
        assert!(
            svg.contains(&format!(">{name}</text>")),
            "the slice is named {name:?} but the legend prints something else"
        );
    }
}

/// CONTROL: with text alternatives OFF the shape closes exactly as before.
///
/// The gate matters: a `<title>` is an accessible name, and emitting one when the caller asked for
/// no accessibility output would change that configuration's bytes.
#[test]
fn no_accessible_name_is_emitted_when_text_alternatives_are_off() {
    let svg = render(WITH_DATA, A11yConfig::none());
    assert!(
        slice_names(&svg).is_empty(),
        "a title was emitted with accessibility output disabled"
    );
    // NON-VACUITY: the slices are still drawn, so this is not passing on an empty chart.
    assert!(
        svg.contains("class=\"fm-pie-slice\""),
        "CONTROL FAILED: no slices rendered at all"
    );
    assert!(
        svg.contains("fm-pie-slice\"/>"),
        "the slice should stay self-closing when it carries no title"
    );
}
