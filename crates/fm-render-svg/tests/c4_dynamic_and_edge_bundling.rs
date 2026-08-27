//! Two divergences found together on `c4_dynamic`, with different causes.
//!
//! 1. NUMBERING. A C4Dynamic relationship is numbered, 1-based, in declaration order. We drew them
//!    bare — losing the one piece of information the diagram type exists to convey.
//!
//! 2. A DROPPED LABEL. `bundle_parallel_edges` collapsed two relationships between the same pair
//!    into one representative and marked the other `bundled`, so it was never rendered. The
//!    representative gained a `×2` marker and the absorbed edge's LABEL — text the author wrote —
//!    vanished from the document.
//!
//! REFERENCE for (1), from the pinned 11.15.0 bundle's `drawRels`, which applies the prefix in the
//! RENDERER and leaves the stored label alone:
//!
//! ```text
//!   let a = 0;
//!   for (let s of t) { a = a + 1;
//!     i.db.getC4Type() === "C4Dynamic" && (s.label.text = a + ": " + s.label.text); … }
//! ```
//!
//! For (2) the reference is simply that mermaid has no bundling: it draws every relationship it was
//! given. Bundling is our own decluttering device, and it is only sound where it discards nothing.
//!
//! ⚠️ BOTH FOUND BY `scripts/headtohead/chromium_text_diff.mjs`, which reported
//! `mermaid draws, we do not: ["1: Submits credentials to", …, "4: Returns the stored hash"]`
//! against `we draw, mermaid does not: ["Submits credentials to", …, "×2"]`. The `×2` in that list
//! is what made the second defect visible — a run we emit that mermaid never does, sitting exactly
//! where a dropped label used to be. c4_dynamic now reports content-equal.

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

const DYNAMIC: &str = "C4Dynamic\n    title T\n    Person(a, \"A\")\n    System(b, \"B\")\n    Rel(a, b, \"First step\")\n    Rel(b, a, \"Second step\")\n";

#[test]
fn c4_dynamic_numbers_its_relationships_in_declaration_order() {
    let drawn = runs(DYNAMIC);
    assert!(
        drawn.iter().any(|run| run == "1: First step"),
        "the first relationship must be numbered 1; drew {drawn:?}"
    );
    assert!(
        drawn.iter().any(|run| run == "2: Second step"),
        "the second relationship must be numbered 2; drew {drawn:?}"
    );
}

/// ⚠️ THE NEGATIVE CONTROL for the numbering, and the defect as it shipped. The bare label is a
/// SUBSTRING of the numbered one, so `contains("First step")` passes either way. Only the absence of
/// the unnumbered run distinguishes them.
#[test]
fn a_c4_dynamic_relationship_is_never_drawn_unnumbered() {
    let drawn = runs(DYNAMIC);
    assert!(
        !drawn.iter().any(|run| run == "First step"),
        "an unnumbered relationship label reached the drawing: {drawn:?}"
    );
}

/// ⚠️ CONTROL: only C4Dynamic numbers. Prefixing every C4 diagram would satisfy the tests above and
/// silently renumber `C4Context`, which mermaid leaves alone.
#[test]
fn a_non_dynamic_c4_diagram_is_not_numbered() {
    let context = "C4Context\n    title T\n    Person(a, \"A\")\n    System(b, \"B\")\n    Rel(a, b, \"First step\")\n";
    let drawn = runs(context);
    assert!(
        drawn.iter().any(|run| run == "First step"),
        "a C4Context relationship must keep its bare label; drew {drawn:?}"
    );
    assert!(
        !drawn.iter().any(|run| run.starts_with("1: ")),
        "a C4Context relationship was numbered: {drawn:?}"
    );
}

/// ⚠️ THE NEGATIVE CONTROL for the dropped label. Two relationships that resolve to the same
/// direction must BOTH keep their text.
///
/// `Rel_Back(b, a, …)` reverses to a→b, so this pair collides with the `Rel(a, b, …)` above it — the
/// exact shape that made `c4_dynamic` lose "Returns the stored hash". A test that only counted edges
/// would pass while one of the two labels was gone.
#[test]
fn parallel_relationships_each_keep_their_own_label() {
    let colliding = "C4Context\n    title T\n    Person(a, \"A\")\n    System(b, \"B\")\n    Rel(a, b, \"Sends request\")\n    Rel_Back(b, a, \"Returns response\")\n";
    let drawn = runs(colliding);
    assert!(
        drawn.iter().any(|run| run == "Sends request"),
        "the first label is missing: {drawn:?}"
    );
    assert!(
        drawn.iter().any(|run| run == "Returns response"),
        "the second label was absorbed into a bundle and lost: {drawn:?}"
    );
    assert!(
        !drawn.iter().any(|run| run.starts_with('\u{00d7}')),
        "a bundle-count marker replaced a labelled relationship: {drawn:?}"
    );
}

/// CONTROL: bundling still applies where it discards nothing.
///
/// Several IDENTICAL unlabelled connections between the same pair are what the feature was built
/// for — collapsing them declutters and loses no text. Removing bundling wholesale would have been
/// the easy fix and would have thrown away a working feature, so the narrower rule is pinned here.
#[test]
fn unlabelled_parallel_edges_are_still_bundled() {
    let plain = "flowchart TD\n  a --> b\n  a --> b\n  a --> b\n";
    let drawn = runs(plain);
    assert!(
        drawn.iter().any(|run| run.starts_with('\u{00d7}')),
        "unlabelled duplicates are no longer bundled, so the declutter feature is gone: {drawn:?}"
    );
}
