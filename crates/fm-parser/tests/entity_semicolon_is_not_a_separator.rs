//! A `;` that closes an entity is not a statement separator (bd-idjwr).
//!
//! THE DEFECT. `;` is mermaid's statement separator AND the last byte of every entity, and
//! `split_statements` knew only the first meaning. It is quote- and bracket-aware, so
//! `A["a &amp; b"]` and `A[a &amp; b]` were safe — the `;` sits inside a quote or inside `[ ]`. Every
//! context with NEITHER was cut in half, and the pieces were interned as diagram content:
//!
//! ```text
//!   A --|a &amp; b| B     four fragments; `|a`, `amp` and `b| B` became NODES
//!   s1 : a &amp; b         `a &amp` plus a second item `b`
//!   s1 : C#35;44 x         `C#35` plus `44 x`
//! ```
//!
//! This is not a missing decode. It is the author's own text being split and the halves drawn as
//! diagram elements — in the most-used family, on a label form the reference renders correctly.
//!
//! ⚠️ THE FIRST FIX WAS RIGHT AND INCOMPLETE, WHICH THE SWEEP CAUGHT. Keying on a leading `&` fixed
//! the first two rows and left the third: mermaid writes entities BOTH as `&amp;` and as a bare
//! `#35;`. Widening to "or a leading `#`" would then have stopped splitting `a #b; c`, which is not
//! an entity at all. The predicate now hands the token to the SAME decoder the label pass uses, so
//! the two cannot disagree about what an entity is.
//!
//! ⚠️ NOT FIXED HERE, and deliberately not conflated: class members and ER attributes still show
//! `&amp;` undecoded. That is a missing decode on those paths — the text is intact — which is a
//! different defect from this one and is filed separately.

fn node_texts(source: &str) -> Vec<String> {
    let parsed = fm_parser::parse(source);
    let ir = &parsed.ir;
    ir.nodes
        .iter()
        .map(|node| {
            node.label
                .and_then(|id| ir.labels.get(id.0))
                .map_or_else(|| node.id.clone(), |l| l.text.clone())
        })
        .collect()
}

const ARROW: &str = "-->";

/// ⚠️ THE NEGATIVE CASE: the label survives whole AND no phantom appears.
///
/// "The label is intact" passes on its own while the fragments are still interned beside it — which
/// is what the defect actually did: `A -->|a &amp; b| B` produced the right-looking text somewhere
/// AND three extra nodes. The node COUNT is what catches that, and it is the half a text assertion
/// cannot see.
#[test]
fn an_entity_in_an_unquoted_label_neither_splits_nor_spawns_nodes() {
    for (name, source, expected_nodes) in [
        (
            "flowchart edge label",
            format!("flowchart LR\n  A {ARROW}|a &amp; b| B\n"),
            2,
        ),
        (
            "flowchart numeric code",
            format!("flowchart LR\n  A {ARROW}|C#35;44| B\n"),
            2,
        ),
        (
            "state description",
            "stateDiagram-v2\n  s1 : a &amp; b\n".to_string(),
            1,
        ),
        (
            "state numeric code",
            "stateDiagram-v2\n  s1 : C#35;44 x\n".to_string(),
            1,
        ),
    ] {
        let texts = node_texts(&source);
        assert_eq!(
            texts.len(),
            expected_nodes,
            "{name}: the entity split the statement and the fragments became nodes: {texts:?}"
        );
        assert!(
            !texts.iter().any(|t| t.contains("amp") && !t.contains('&')),
            "{name}: a fragment of the entity was interned on its own: {texts:?}"
        );
    }
}

/// The label text itself is whole, and decoded.
#[test]
fn the_label_keeps_its_whole_text() {
    let parsed = fm_parser::parse(&format!("flowchart LR\n  A {ARROW}|a &amp; b| B\n"));
    let ir = &parsed.ir;
    let label = ir
        .edges
        .iter()
        .find_map(|e| e.label)
        .and_then(|id| ir.labels.get(id.0))
        .map(|l| l.text.as_str())
        .expect("the edge has a label");
    assert_eq!(label, "a & b", "the edge label lost its tail at the `;`");

    let parsed = fm_parser::parse("stateDiagram-v2\n  s1 : C#35;44 x\n");
    let ir = &parsed.ir;
    let text = ir
        .nodes
        .iter()
        .find(|n| n.id == "s1")
        .and_then(|n| n.label)
        .and_then(|id| ir.labels.get(id.0))
        .map(|l| l.text.as_str())
        .expect("state s1 has a description");
    assert_eq!(text, "C#44 x", "the numeric code was cut at its own `;`");
}

/// ⚠️ THE CONTROL THAT KEEPS THIS FROM BEING A REGRESSION: a real `;` still separates.
///
/// The cheap version of this fix is to stop splitting on `;` near any `&` or `#`, or to stop
/// splitting at all. Both pass every assertion above. Multi-statement lines are documented mermaid
/// and must keep working.
#[test]
fn a_genuine_semicolon_still_separates_statements() {
    let texts = node_texts(&format!("flowchart LR\n  A {ARROW} B; B {ARROW} C\n"));
    assert_eq!(
        texts.len(),
        3,
        "a real statement separator stopped separating: {texts:?}"
    );
    for id in ["A", "B", "C"] {
        assert!(
            texts.iter().any(|t| t == id),
            "node {id} is missing after the split: {texts:?}"
        );
    }
}

/// ⚠️ AND A `;` THAT LOOKS ENTITY-ISH BUT IS NOT ONE STILL SEPARATES.
///
/// This is the case that rules out the shape-matching version of the predicate. `#b;` has a `#` and
/// a `;` and is not an entity; `a & b; c` has an `&` but whitespace in the run. A rule keyed on
/// "there is an `&` or `#` behind the `;`" would swallow both, silently joining statements the
/// author separated.
#[test]
fn a_semicolon_that_closes_no_real_entity_still_separates() {
    for (name, source, expected) in [
        (
            "hash that is not a code",
            format!("flowchart LR\n  A {ARROW} B; C#b {ARROW} D\n"),
            4,
        ),
        (
            // ⚠️ FIVE, NOT FOUR, AND THE REASON IS WORTH RECORDING: `&` is mermaid's AND operator in
            // a flowchart, so `a & b --> D` declares TWO sources and yields `a`, `b`, `D`. The
            // fixture was first written expecting four and the parser was right — the `;` had split
            // exactly as it should. What the row tests is unchanged: an `&` with whitespace in the
            // run is not an entity, so the `;` must still separate. Had it not, the two statements
            // would have merged into one mangled node instead of five clean ones.
            "ampersand with whitespace",
            format!("flowchart LR\n  A {ARROW} B; a & b {ARROW} D\n"),
            5,
        ),
    ] {
        let texts = node_texts(&source);
        assert_eq!(
            texts.len(),
            expected,
            "{name}: the `;` stopped separating, so two statements were joined: {texts:?}"
        );
    }
}

/// The splitter and the decoder agree about what an entity is.
///
/// Expressed from outside: a token the decoder turns into a character must not split, and one it
/// leaves alone must. This is the property the implementation gets by calling that decoder rather
/// than re-describing it, and it is what a shape-matching predicate fails.
#[test]
fn the_splitter_and_the_decoder_agree() {
    // Decodable — must not split.
    for token in ["&amp;", "&lt;", "&gt;", "&nbsp;", "#35;", "#59;"] {
        let source = format!("flowchart LR\n  A {ARROW}|x{token}y| B\n");
        let texts = node_texts(&source);
        assert_eq!(
            texts.len(),
            2,
            "{token} is decodable but still split the statement: {texts:?}"
        );
    }
    // Not decodable — must still split.
    for token in ["&notanentity;", "#zz;"] {
        let source = format!("flowchart LR\n  A {ARROW} B; x{token}y {ARROW} D\n");
        let texts = node_texts(&source);
        assert!(
            texts.len() > 2,
            "{token} is not decodable but stopped the split: {texts:?}"
        );
    }
}

/// ⚠️ THE TWO FIXTURES THE NEGATIVE CONTROLS DEMANDED.
///
/// The controls for this bead disarm the predicate into its plausible WRONG forms. Two of them —
/// shape-matching instead of asking the decoder, and allowing whitespace inside the run — came back
/// INERT against the first version of this file: every test still passed. The tests were too weak,
/// not the controls wrong, and these are the cases that separate the four candidate predicates.
///
/// ```text
///   A --> B#zz; C --> D       `#zz` LOOKS like a numeric code and decodes to nothing.
///                             Shape-matching swallows the `;` and merges two statements.
///   A[x & y] --> B; C --> D   an `&` sits ten bytes back, with `]` and spaces between.
///                             Allowing whitespace in the run swallows this `;` too.
/// ```
///
/// Both must still split, and the node count is what says so: a swallowed separator merges the two
/// statements into one mangled node instead of leaving four clean ones.
#[test]
fn the_predicate_is_neither_shape_matching_nor_whitespace_tolerant() {
    for (name, source) in [
        (
            "a non-decodable token that looks like a code",
            format!("flowchart LR\n  A {ARROW} B#zz; C {ARROW} D\n"),
        ),
        (
            "an ampersand further back in the line",
            format!("flowchart LR\n  A[x & y] {ARROW} B; C {ARROW} D\n"),
        ),
    ] {
        let texts = node_texts(&source);
        assert_eq!(
            texts.len(),
            4,
            "{name}: the `;` was swallowed and two statements merged: {texts:?}"
        );
        for id in ["C", "D"] {
            assert!(
                texts.iter().any(|t| t == id),
                "{name}: `{id}` from the second statement is missing: {texts:?}"
            );
        }
    }
}

/// CONTROL: the quoted and bracketed forms, which were never broken, still work.
///
/// They are the reason this went unnoticed — most real labels are one or the other — so a change to
/// the splitter must be shown not to have disturbed them.
#[test]
fn the_quoted_and_bracketed_forms_are_unchanged() {
    for source in [
        format!("flowchart LR\n  A[\"a &amp; b\"] {ARROW} B\n"),
        format!("flowchart LR\n  A[a &amp; b] {ARROW} B\n"),
        format!("flowchart LR\n  A {ARROW}|\"a &amp; b\"| B\n"),
    ] {
        let texts = node_texts(&source);
        assert_eq!(texts.len(), 2, "a safe form gained a node: {texts:?}");
    }
}
