//! Differential test: the actor legend mermaid draws on a user journey (bd-mq273).
//!
//! THE DIVERGENCE THIS PINS. `journey_basic.mmd` declares `User` and `System`; mermaid draws both as
//! text and we drew neither. Found by `scripts/headtohead/drawn_text_diff.mjs`:
//!
//! ```text
//!   mermaid draws, we do NOT: ["System", "User"]
//! ```
//!
//! mermaid's full run order for that fixture is
//! `["System","User","Browse","Visit homepage", … ,"User Shopping Journey"]` — the actors come
//! FIRST, each exactly once.
//!
//! REFERENCE BEHAVIOUR, three rules, each probed against the pinned 11.15.0 bundle and each with its
//! own test below:
//!
//! ```text
//!   One: 3: Zed / Two: 4: Alpha   ->  ["Alpha","Zed"]   SORTED, not source order
//!   One: 3: Bob / Two: 4: Bob     ->  ["Bob"]           DEDUPLICATED
//!   One: 3: Bob, Ann              ->  ["Ann","Bob"]     SPLIT on comma, then sorted
//! ```
//!
//! ⚠️ THE NAMES COME FROM `journey_meta`, NOT FROM THE `journey-actor-*` CLASSES. Those are
//! CSS-normalized — a class cannot contain a space — so a legend built from them draws `Big_Corp`
//! for an author who wrote `Big Corp`, and the mapping is not reversible because a genuine
//! underscore is indistinguishable from a normalized space. The accessible name carried exactly
//! that defect until this change; `an_actor_name_keeps_its_spaces` pins both halves.

/// Every `<text>` leaf, in document order.
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
        let text = rest[open + 1..close]
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&");
        let text = text.trim();
        if !text.is_empty() && !text.contains('<') {
            out.push(text.to_string());
        }
        rest = &rest[close + "</text>".len()..];
    }
    out
}

fn journey(steps: &str) -> String {
    format!("journey\n  title T\n  section S\n{steps}")
}

#[test]
fn the_actors_are_drawn() {
    let drawn = runs(&journey("    One: 3: Zed\n    Two: 4: Alpha\n"));
    for actor in ["Alpha", "Zed"] {
        assert!(
            drawn.iter().any(|run| run == actor),
            "actor {actor} was declared and never drawn: {drawn:?}"
        );
    }
}

/// ⚠️ NEGATIVE CONTROL for the ORDER. mermaid sorts; source order here is Zed then Alpha, so an
/// implementation that preserves declaration order draws them the other way round and fails.
#[test]
fn the_actors_are_sorted_not_in_source_order() {
    let drawn = runs(&journey("    One: 3: Zed\n    Two: 4: Alpha\n"));
    let alpha = drawn.iter().position(|run| run == "Alpha");
    let zed = drawn.iter().position(|run| run == "Zed");
    assert!(
        alpha < zed,
        "mermaid sorts the legend, so Alpha precedes Zed even though Zed is declared first: {drawn:?}"
    );
}

/// ⚠️ NEGATIVE CONTROL for DEDUPLICATION. One actor on two steps is one legend entry.
#[test]
fn a_repeated_actor_appears_once() {
    let drawn = runs(&journey("    One: 3: Bob\n    Two: 4: Bob\n"));
    let count = drawn.iter().filter(|run| *run == "Bob").count();
    assert_eq!(
        count, 1,
        "Bob is declared twice and must be listed once: {drawn:?}"
    );
}

/// ⚠️ NEGATIVE CONTROL for SPLITTING. Several actors on one step are separate entries, not one run.
#[test]
fn actors_on_one_step_are_separate_entries() {
    let drawn = runs(&journey("    One: 3: Bob, Ann\n"));
    assert!(
        drawn.iter().any(|run| run == "Ann") && drawn.iter().any(|run| run == "Bob"),
        "`Bob, Ann` must contribute two entries: {drawn:?}"
    );
    assert!(
        !drawn.iter().any(|run| run.contains(',')),
        "the actor list was drawn as one un-split run: {drawn:?}"
    );
}

/// ⚠️ THE SPELLING CONTROL, covering the defect that made this a two-part change. Building the
/// legend (or the accessible name) from the `journey-actor-*` CSS classes yields `Big_Corp`, because
/// a class name cannot hold a space. Both the drawn text and the `<title>` must show what the author
/// typed.
#[test]
fn an_actor_name_keeps_its_spaces() {
    let source = journey("    One: 3: Big Corp\n");
    let drawn = runs(&source);
    assert!(
        drawn.iter().any(|run| run == "Big Corp"),
        "the legend drew the CSS-normalized spelling instead of the author's: {drawn:?}"
    );
    assert!(
        !drawn.iter().any(|run| run == "Big_Corp"),
        "the legend drew `Big_Corp`: {drawn:?}"
    );
    let svg = fm_render_svg::render_svg(&fm_parser::parse(&source).ir);
    assert!(
        svg.contains("actors: Big Corp"),
        "the accessible name still announces the normalized spelling"
    );
}

/// CONTROL: a journey with no actors gains no legend, and no other diagram type gains one at all.
#[test]
fn no_actors_means_no_legend() {
    let bare = runs(&journey("    One: 3\n"));
    assert!(
        bare.iter()
            .all(|run| run == "T" || run == "S" || run == "One"),
        "a journey with no declared actor drew a legend entry: {bare:?}"
    );
    let flow = runs("flowchart LR\n  A-->B\n");
    assert_eq!(
        flow.len(),
        2,
        "a flowchart gained journey legend text: {flow:?}"
    );
}
