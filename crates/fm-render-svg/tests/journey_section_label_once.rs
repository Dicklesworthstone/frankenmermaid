//! Differential test: a journey section name is drawn ONCE, not once per task.
//!
//! THE DIVERGENCE. Layout emits one `Lane` band per TASK and labels each with its SECTION name, so a
//! seven-task journey drew `Browse` four times and `Purchase` five — while the section name is
//! already drawn once, correctly, as the cluster label. mermaid draws each section name exactly once.
//!
//! Measured against the pinned 11.15.0 bundle in Chromium: incumbent 12 drawn runs against our 19,
//! the surplus being exactly the seven repeats. `journey_basic` now reports AGREE, 12 runs.
//!
//! ⚠️ THE BANDS THEMSELVES ARE NOT THE BUG and are still emitted — they are what tints each task by
//! its section. Only the repeated caption goes. A fix that removed the bands would make this file's
//! first assertion pass and destroy the diagram's colouring, which is why that is pinned too.
//!
//! ⚠️ AND THE SUPPRESSION IS SCOPED TO JOURNEY, not to `LayoutBandKind::Lane`. A gitgraph's lane band
//! label is the ONLY carrier of its branch name; suppressing that would delete information rather
//! than duplication. Pinned by the gitgraph control below.
//!
//! ⚠️ THIS WAS NEARLY FILED AS A MUCH BIGGER DEFECT. The first sweep reported
//! `mermaid draws, we do not: [all seven task names]`, which reads as "our journey renders no task
//! labels at all". It was the probe: mermaid emits BOTH a `<foreignObject>` HTML label and a hidden
//! `<text>` twin for the same content, and the extractor counted both, so every task appeared twice
//! on the incumbent side. Rendering ours by hand showed all seven task names present. The differ now
//! attaches its host to the document and filters on computed visibility.

fn runs(source: &str) -> Vec<String> {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(source).ir);
    let mut out = Vec::new();
    let mut rest = svg.as_str();
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        let Some(open) = rest.find('>') else { break };
        let Some(close) = rest.find("</text>") else {
            break;
        };
        let body = &rest[open + 1..close];
        let mut stripped = String::new();
        let mut in_tag = false;
        for ch in body.chars() {
            match ch {
                '<' => in_tag = true,
                '>' if in_tag => in_tag = false,
                _ if !in_tag => stripped.push(ch),
                _ => {}
            }
        }
        let text = stripped
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&");
        let text = text.trim().to_string();
        if !text.is_empty() {
            out.push(text);
        }
        rest = &rest[close + "</text>".len()..];
    }
    out
}

const JOURNEY: &str = "journey\n    title T\n    section Browse\n      Visit homepage: 5: User\n      Search products: 4: User\n      View product: 5: User\n    section Purchase\n      Add to cart: 4: User\n      Checkout: 3: User\n";

fn count_of(runs: &[String], needle: &str) -> usize {
    runs.iter().filter(|run| run.as_str() == needle).count()
}

#[test]
fn each_section_name_is_drawn_exactly_once() {
    let drawn = runs(JOURNEY);
    assert_eq!(
        count_of(&drawn, "Browse"),
        1,
        "the section name must be drawn once, not once per task: {drawn:?}"
    );
    assert_eq!(
        count_of(&drawn, "Purchase"),
        1,
        "the section name must be drawn once, not once per task: {drawn:?}"
    );
}

/// ⚠️ NON-VACUITY, and the control that stops the obvious wrong fix. Deleting the bands — or the
/// section captions altogether — would satisfy the assertion above by drawing ZERO. Every task label
/// and both section names must still be present.
#[test]
fn every_task_and_section_still_reaches_the_drawing() {
    let drawn = runs(JOURNEY);
    for expected in [
        "Visit homepage",
        "Search products",
        "View product",
        "Add to cart",
        "Checkout",
        "Browse",
        "Purchase",
    ] {
        assert!(
            drawn.iter().any(|run| run == expected),
            "{expected:?} is missing entirely: {drawn:?}"
        );
    }
}

/// ⚠️ THE BANDS SURVIVE. They tint each task by section, so the fix had to remove the repeated
/// CAPTION and keep the strips. A `fm-band-lane` rect per task must still be emitted.
#[test]
fn the_per_task_lane_bands_are_still_drawn() {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(JOURNEY).ir);
    let lanes = svg.matches("fm-band-lane").count();
    assert!(
        lanes >= 5,
        "the per-task lane bands were removed along with their labels; found {lanes}"
    );
}

/// ⚠️ THE GITGRAPH CONTROL. Its lane band label is the only place a branch name is drawn, so a fix
/// that suppressed every `Lane` label would delete information. Both branch names must survive.
#[test]
fn gitgraph_lane_labels_are_not_suppressed() {
    let git = "gitGraph\n  commit\n  branch develop\n  checkout develop\n  commit\n";
    let drawn = runs(git);
    for branch in ["main", "develop"] {
        assert!(
            drawn.iter().any(|run| run == branch),
            "the gitgraph branch label {branch:?} was suppressed with the journey ones: {drawn:?}"
        );
    }
}
