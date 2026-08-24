//! A journey step must announce its SCORE and ACTORS (bd-fsj42).
//!
//! A journey step declares two things beyond its name — a score and who performs it — and neither
//! was reachable by anyone. The parser stores them as class hooks (`journey-score-5`,
//! `journey-actor-Alice`), which is a STYLING affordance, not information:
//!
//!   - no CSS rule in the crate targets those classes;
//!   - no `<text>` run carries them, so nothing is drawn;
//!   - the accessible name said only `Node: TaskOne, rounded rectangle`.
//!
//! So a reader could not tell who performs a task or how it scored, in any modality. mermaid draws
//! both: a face for the score and a coloured circle per actor.
//!
//! Found by sweeping every diagram type for declared text that never reaches a rendered `<text>` run
//! — the same sweep that produced bd-am6a2. The classic parsed-stored-drawn-by-nobody shape that
//! bd-bk7h named.
//!
//! ⚠️ SCOPE, stated so it is not overclaimed: this makes the information REACHABLE, not VISIBLE.
//! mermaid's actor circles and score face need layout space this renderer does not reserve, so
//! drawing them is a layout change and a separate bead. What lands here is the accessible name.

use fm_render_svg::{SvgRenderConfig, render_svg, render_svg_with_config};

fn node_titles(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(at) = rest.find("<title>") {
        rest = &rest[at + "<title>".len()..];
        let Some(end) = rest.find("</title>") else {
            break;
        };
        let text = &rest[..end];
        if text.starts_with("Node: ") {
            out.push(text.to_string());
        }
        rest = &rest[end..];
    }
    out
}

fn render(source: &str) -> String {
    render_svg(&fm_parser::parse(source).ir)
}

/// The same diagram under the three configurations that select DIFFERENT node render paths.
fn render_all_paths(source: &str) -> Vec<(&'static str, String)> {
    let ir = fm_parser::parse(source).ir;
    vec![
        ("default", render_svg(&ir)),
        (
            "spans",
            render_svg_with_config(
                &ir,
                &SvgRenderConfig {
                    include_source_spans: true,
                    ..SvgRenderConfig::default()
                },
            ),
        ),
        (
            "no-embed",
            render_svg_with_config(
                &ir,
                &SvgRenderConfig {
                    embed_theme_css: false,
                    ..SvgRenderConfig::default()
                },
            ),
        ),
    ]
}

/// EVERY RENDER PATH agrees, the shipped DEFAULT included.
///
/// This is the test the bug most needed. The common node fragment is reachable through TWO callers,
/// and my first fix enriched only `describe_node` — correct under `include_source_spans`, INERT in
/// the default configuration that actually ships. Excluding journey from one caller was still inert,
/// because the other caller kept serving it. Asserting the three configurations against EACH OTHER
/// catches that class of miss; asserting any one of them against a literal does not.
#[test]
fn every_render_path_describes_a_journey_step_identically() {
    for (name, svg) in
        render_all_paths("journey\n  title J\n  section Go\n    TaskOne: 5: Alice, Bob\n")
    {
        assert_eq!(
            node_titles(&svg),
            vec!["Node: TaskOne, rounded rectangle, score 5, actors: Alice, Bob"],
            "config {name} describes the step differently"
        );
    }
}

/// THE CAPABILITY: score and actors are announced, with the author's own casing.
#[test]
fn a_journey_step_announces_its_score_and_actors() {
    let svg = render(
        "journey\n  title J\n  section Go\n    TaskOne: 5: Alice, Bob\n    TaskTwo: 3: Carol\n",
    );
    assert_eq!(
        node_titles(&svg),
        vec![
            "Node: TaskOne, rounded rectangle, score 5, actors: Alice, Bob",
            "Node: TaskTwo, rounded rectangle, score 3, actors: Carol",
        ]
    );
}

/// CASING IS THE AUTHOR'S, not normalised.
///
/// The class hook is `journey-actor-Alice`, capital preserved — I checked that before relying on it,
/// because an earlier grep of mine lowercased the match and made it look as though the name had been
/// normalised away. Announcing `alice` for an author who wrote `Alice` would be a quiet downgrade.
#[test]
fn actor_names_keep_the_authors_casing() {
    let svg = render("journey\n  title J\n  section Go\n    T: 4: McDonald, o'Brien\n");
    let title = node_titles(&svg).first().cloned().unwrap_or_default();
    assert!(
        title.contains("McDonald"),
        "mixed-case actor was normalised: {title:?}"
    );
}

/// A step with a score but NO actors says so, without a dangling `actors:`.
#[test]
fn a_step_without_actors_announces_only_its_score() {
    let svg = render("journey\n  title J\n  section Go\n    TaskOne: 5\n");
    assert_eq!(
        node_titles(&svg),
        vec!["Node: TaskOne, rounded rectangle, score 5"]
    );
}

/// CONTROL: a step with neither gains nothing.
///
/// Without this, appending an empty `, score , actors: ` to every step would satisfy the tests above
/// while making every other journey description worse.
#[test]
fn a_step_with_neither_score_nor_actors_is_unchanged() {
    let svg = render("journey\n  title J\n  section Go\n    TaskOne\n");
    assert_eq!(node_titles(&svg), vec!["Node: TaskOne, rounded rectangle"]);
}

/// CONTROL: NO OTHER DIAGRAM TYPE is affected.
///
/// `describe_node` is shared by every renderer in this crate. The journey branch is gated on the
/// `journey-step` class precisely so it cannot leak; this is what proves the gate holds.
#[test]
fn other_diagram_types_keep_their_descriptions() {
    for (source, expected) in [
        (
            "flowchart LR\n  A[Alpha] --> B[Beta]\n",
            vec!["Node: Alpha, rectangle", "Node: Beta, rectangle"],
        ),
        (
            "stateDiagram-v2\n  Idle --> Busy\n",
            // MEASURED, not guessed: state nodes describe as `rectangle`. My first expectation
            // here said `rounded rectangle` and failed — the control caught my own assumption
            // rather than a regression, which is what a control is for.
            vec!["Node: Idle, rectangle", "Node: Busy, rectangle"],
        ),
    ] {
        assert_eq!(
            node_titles(&render(source)),
            expected,
            "a non-journey diagram's node description changed"
        );
    }
}

/// CONTROL: the bare `journey-actor` marker class is not announced as an actor.
///
/// The parser adds `journey-actor` alongside the per-actor classes to flag that a step HAS actors.
/// Reading it as a name would announce an actor called nothing at all.
#[test]
fn the_bare_actor_marker_class_is_not_announced() {
    let svg = render("journey\n  title J\n  section Go\n    TaskOne: 5: Alice\n");
    let title = node_titles(&svg).first().cloned().unwrap_or_default();
    assert!(
        title.ends_with("actors: Alice"),
        "the marker class leaked into the actor list: {title:?}"
    );
}
