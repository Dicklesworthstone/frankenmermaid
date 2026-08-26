//! Differential test: which class annotation gets drawn, against what mermaid-js draws (bd-dezf6).
//!
//! THE DIVERGENCE. `<<interface>> Foo` followed by `<<abstract>> Foo` is two annotations on one
//! class. mermaid keeps both — `addAnnotation` does `classes.get(i).annotations.push(r)`, and the db
//! reports `annotations: ['interface', 'abstract']` — but its class renderer draws only the FIRST:
//!
//! ```js
//! if (t.annotations.length > 0) { let b = t.annotations[0]; await G4(l, { text: `«${b}»` }, 0); … }
//! ```
//!
//! We carry one `Option<ClassStereotype>` and each annotation statement ASSIGNED it, so the last
//! write won and we drew `abstract` where mermaid draws `interface`. The two engines therefore
//! disagreed about which annotation the reader sees.
//!
//! ⚠️ THIS IS NOT "WE LOSE A LIST". I first filed it that way, from the db alone, and specified a
//! `Vec` plus drawing every annotation. Reading the RENDERER corrected it: mermaid narrows to one
//! too. Implementing the first reading would have shipped annotations mermaid never draws, plus a
//! layout resize for a line that does not grow — a regression wearing a fix's clothes. The db says
//! what is STORED; only the renderer says what is DRAWN, and a divergence report is about the drawn
//! output.
//!
//! So the fix is first-wins, the IR is unchanged, and the class compartment measurement is
//! untouched because the drawn line is still exactly one annotation.
//!
//! NOT ASSERTED: that mermaid wraps the annotation in guillemets (`«interface»`) where
//! `ClassStereotype::label()` emits `<<interface>>`. That comes from the same renderer line, but no
//! equivalence dump on hand contained an annotated class, so it is unconfirmed against real output
//! and left for a targeted run rather than asserted here on source-reading alone.

use fm_core::ClassStereotype;

/// The stereotype our parser attaches to `Foo`.
fn stereotype_of(source: &str) -> Option<ClassStereotype> {
    let ir = fm_parser::parse(source).ir;
    ir.nodes
        .iter()
        .find(|node| node.id.eq_ignore_ascii_case("Foo"))
        .and_then(|node| node.class_meta.as_deref())
        .and_then(|meta| meta.stereotype.clone())
}

#[test]
fn the_first_annotation_is_the_one_drawn() {
    // mermaid: annotations ['interface','abstract'], renderer draws annotations[0] = interface.
    let stereotype = stereotype_of(
        "classDiagram\nclass Foo\n<<interface>> Foo\n<<abstract>> Foo\nFoo : +x\n",
    );
    assert_eq!(
        stereotype,
        Some(ClassStereotype::Interface),
        "the SECOND annotation won; mermaid draws the first"
    );
}

/// The reverse order must reverse the answer — otherwise the test would pass on an implementation
/// that simply always reported `Interface`.
#[test]
fn the_reverse_order_draws_the_other_one() {
    let stereotype = stereotype_of(
        "classDiagram\nclass Foo\n<<abstract>> Foo\n<<interface>> Foo\nFoo : +x\n",
    );
    assert_eq!(stereotype, Some(ClassStereotype::Abstract));
}

/// LAST-WINS — what shipped before — must contradict at least one of the two orderings. Written as
/// an explicit model rather than left implicit, so the pair above cannot both quietly become
/// order-insensitive.
#[test]
fn last_annotation_wins_disagrees_with_the_incumbent() {
    let forward = ["interface", "abstract"];
    let first_wins = forward[0];
    let last_wins = forward[forward.len() - 1];
    assert_ne!(
        first_wins, last_wins,
        "the fixture ordering is symmetric, so it cannot tell first-wins from last-wins"
    );
    // And the shipping parser must agree with first-wins, not last-wins.
    let stereotype = stereotype_of(
        "classDiagram\nclass Foo\n<<interface>> Foo\n<<abstract>> Foo\nFoo : +x\n",
    );
    assert_eq!(stereotype, Some(ClassStereotype::Interface));
    assert_ne!(stereotype, Some(ClassStereotype::Abstract));
}

/// A single annotation is unaffected — the common case, and the control that this change did not
/// simply stop recording stereotypes.
#[test]
fn a_single_annotation_still_reaches_the_class() {
    for (source, expected) in [
        ("classDiagram\nclass Foo\n<<interface>> Foo\n", ClassStereotype::Interface),
        ("classDiagram\nclass Foo\n<<abstract>> Foo\n", ClassStereotype::Abstract),
        ("classDiagram\nclass Foo {\n<<interface>>\n+x\n}\n", ClassStereotype::Interface),
    ] {
        assert_eq!(stereotype_of(source), Some(expected), "{source:?}");
    }
}
