//! Differential test: the y-axis tick values mermaid picks for an xychart.
//!
//! THE DIVERGENCE THIS PINS. We emitted five ticks at quarter points, so `y-axis 4000 --> 11000`
//! labelled 5750 and 9250 — values that appear nowhere on a chart anyone would draw by hand.
//! mermaid labels every multiple of 500 in the range. Found by
//! `scripts/headtohead/drawn_text_diff.mjs`.
//!
//! ⚠️ THE RULE IS d3's `tickStep(min, max, 10)`, NOT A DIVISION OF THE RANGE. Measured on the pinned
//! 11.15.0 bundle across six ranges, and the same rule reproduces all six:
//!
//! ```text
//!   0 -> 10       step 1        0 -> 100        step 10
//!   0 -> 1        step 0.1      0 -> 7          step 0.5
//!   100 -> 900    step 100      4000 -> 11000   step 500
//! ```
//!
//! Regenerate with:
//!   node scripts/headtohead/drawn_text_diff.mjs <chart.mmd>
//! which now reports AGREE for every one of them.
//!
//! ⚠️ THE TICK COUNT IS DELIBERATELY NOT ASSERTED. mermaid's density could depend on available
//! pixels, and the probe renders it under jsdom with `getComputedTextLength` stubbed to 0 (see
//! 080e5f4e), so a count measured there is not trustworthy. The VALUES are data-derived and are.
//! What is pinned is: every tick is a multiple of the step, and the step is the one d3 picks.

/// Every `<text>` that parses as a number, in document order.
fn numeric_runs(source: &str) -> Vec<f64> {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(source).ir);
    let mut out = Vec::new();
    let mut rest = svg.as_str();
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        let Some(open) = rest.find('>') else { break };
        let Some(close) = rest.find("</text>") else {
            break;
        };
        if let Ok(value) = rest[open + 1..close].trim().parse::<f64>() {
            out.push(value);
        }
        rest = &rest[close + "</text>".len()..];
    }
    out
}

fn chart(min: f64, max: f64) -> String {
    format!(
        "xychart-beta\n    x-axis [a, b]\n    y-axis \"v\" {min} --> {max}\n    bar [{min}, {max}]\n"
    )
}

/// `(min, max, step)` as mermaid drew them. The step is what the tick values must all be multiples
/// of; the endpoints are inclusive bounds, not a promised tick count.
const MEASURED: &[(f64, f64, f64)] = &[
    (0.0, 10.0, 1.0),
    (0.0, 100.0, 10.0),
    (0.0, 1.0, 0.1),
    (0.0, 7.0, 0.5),
    (100.0, 900.0, 100.0),
    (4000.0, 11000.0, 500.0),
    // ⚠️ THE TWO DISCRIMINATING RANGES, and they were found by the control FAILING TO FAIL. `0..7`
    // has error exactly 7.0, which both sqrt(50) = 7.071 and a rounded 7.5 reject, so both rules
    // pick factor 5 — it looks like a threshold case and tests nothing. A real discriminator needs
    // an error BETWEEN the two spellings of a cutoff:
    //   0..72  error 7.200 -> d3 factor 10, step 10   (a 7.5 cutoff gives factor 5, step 5)
    //   0..31  error 3.100 -> d3 factor  2, step  2   (a 3.0 cutoff gives factor 5, step 5)
    // Both were then confirmed against the bundle: the probe reports AGREE for each.
    (0.0, 72.0, 10.0),
    (0.0, 31.0, 2.0),
];

#[test]
fn every_y_tick_is_a_multiple_of_the_step_mermaid_chose() {
    for &(min, max, step) in MEASURED {
        let runs = numeric_runs(&chart(min, max));
        let ticks: Vec<f64> = runs
            .iter()
            .copied()
            .filter(|value| *value >= min - step && *value <= max + step)
            .collect();
        assert!(
            ticks.len() >= 3,
            "{min}..{max}: only {} numeric runs, nothing to check",
            ticks.len()
        );
        for tick in &ticks {
            let multiples = tick / step;
            assert!(
                (multiples - multiples.round()).abs() < 1e-6,
                "{min}..{max}: tick {tick} is not a multiple of mermaid's step {step}; drew {runs:?}"
            );
        }
    }
}

/// ⚠️ THE NEGATIVE CONTROL. The quarter-point rule this replaces produces 5750 and 9250 for the
/// documented range — neither a multiple of 500 nor of any 1/2/5 step for that span.
#[test]
fn the_quarter_point_values_are_gone() {
    let runs = numeric_runs(&chart(4000.0, 11000.0));
    for wrong in [5750.0, 9250.0] {
        assert!(
            !runs.contains(&wrong),
            "{wrong} is a quarter point of 4000..11000, not a tick mermaid draws: {runs:?}"
        );
    }
    for expected in [4500.0, 10500.0] {
        assert!(
            runs.contains(&expected),
            "{expected} is a multiple of mermaid's 500 step and is missing: {runs:?}"
        );
    }
}

/// ⚠️ THE THRESHOLD CONTROL. d3's cutoffs are sqrt(50), sqrt(10) and sqrt(2), and a hand-rounded
/// 7.5 / 3.0 / 1.5 agrees with them on almost every range — including `0..7`, whose error is exactly
/// 7.0 and which both spellings reject. These two ranges are the ones that actually part company.
#[test]
fn the_sqrt_thresholds_are_not_rounded() {
    // error 7.200: d3 -> factor 10, step 10. A 7.5 cutoff -> factor 5, step 5, which would tick at 5.
    let wide = numeric_runs(&chart(0.0, 72.0));
    assert!(
        wide.iter().any(|value| (value - 70.0).abs() < 1e-6),
        "0..72 must tick at 70 on a step of 10: {wide:?}"
    );
    assert!(
        !wide.iter().any(|value| (value - 5.0).abs() < 1e-6),
        "0..72 ticked at 5, so the step is 5 — a rounded 7.5 threshold, not sqrt(50): {wide:?}"
    );
    // error 3.100: d3 -> factor 2, step 2. A 3.0 cutoff -> factor 5, step 5.
    let narrow = numeric_runs(&chart(0.0, 31.0));
    assert!(
        narrow.iter().any(|value| (value - 2.0).abs() < 1e-6),
        "0..31 must tick at 2 on a step of 2: {narrow:?}"
    );
    assert!(
        !narrow.iter().any(|value| (value - 5.0).abs() < 1e-6),
        "0..31 ticked at 5, so the step is 5 — a rounded 3.0 threshold, not sqrt(10): {narrow:?}"
    );
}

/// CONTROL: a degenerate axis must not spin or emit a wall of ticks.
#[test]
fn a_flat_axis_is_handled() {
    let runs = numeric_runs(&chart(5.0, 5.0));
    assert!(
        runs.len() < 50,
        "a zero-height axis produced {} numeric runs",
        runs.len()
    );
}
