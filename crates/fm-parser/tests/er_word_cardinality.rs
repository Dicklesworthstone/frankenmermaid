//! ER cardinality written in WORDS is a relationship, not a phantom node (bd-wyyb3).
//!
//! THE DEFECT, measured before the fix. `erDiagram\n A one or more to zero or more B` produced:
//!
//! ```text
//!   nodes = ["A_one_or_more_to_zero_or_more_B"]   edges = 0
//! ```
//!
//! One box captioned with the author's whole sentence: both entities gone, the relationship gone.
//! Every word spelling did the same, and `1+ to 0+` degraded to a node literally named `A_1`. This
//! is a data-loss defect, strictly worse than the class lollipop (bd-lkm9i), which at least kept its
//! two nodes.
//!
//! WHY IT EXISTED. `ER_OPERATORS` fuses cardinality and relation type into ONE literal (`||--o{`),
//! which can only ever cover the symbolic spellings — the word forms contain SPACES, so they are not
//! tokens that scan can reach at all, and enumerating them is a ~1176-entry cross product.
//!
//! THE INCUMBENT'S GRAMMAR, read off the pinned 11.15.0 bundle by aligning each `rules[]` regex with
//! its own `case N: return` token: FOURTEEN cardinality spellings over FOUR shapes, plus `MD_PARENT`
//! (`u`), and SIX relation types (`--`, `to` identifying; `..`, `optionally to`, `.-`, `-.` not).
//! Because the spelling is only how a shape was written, the fix CANONICALISES word forms into the
//! symbolic notation this engine already carries — `one or more` and `}|` become the same string —
//! so fm-core's `parse_er_cardinality_forms`, `er_marker_form` and all three renderers are untouched.
//! That is exactly what the incumbent does: both spellings return `ONE_OR_MORE`.
//!
//! ⚠️ The assertions below are on the canonical NOTATION rather than on a rendered marker, because
//! that string is the contract the renderers consume; `er_cardinality_markers.rs` in fm-render-svg
//! is what pins the notation-to-marker half.

use fm_core::ArrowType;

/// `(sorted node ids, er notation, arrow)` for a one-relationship diagram.
fn relation(body: &str) -> (Vec<String>, Option<String>, Option<ArrowType>) {
    let ir = fm_parser::parse(&format!("erDiagram\n  {body}\n")).ir;
    let mut ids: Vec<String> = ir.nodes.iter().map(|n| n.id.clone()).collect();
    ids.sort();
    (
        ids,
        ir.edges
            .first()
            .and_then(|e| e.er_notation())
            .map(str::to_string),
        ir.edges.first().map(|e| e.arrow),
    )
}

/// Every word spelling the incumbent accepts resolves to the SHAPE it names.
///
/// Driven by a table rather than one case, because the defect was uniform across all of them and a
/// single example would not show that `1+` and `one or many` land on the same shape as `}|`.
#[test]
fn every_word_spelling_resolves_to_its_shape() {
    for (body, expected) in [
        ("A one or more to zero or more B : r", "}|--o{"),
        ("A one or zero to one or many B : r", "|o--|{"),
        ("A zero or many to many(1) B : r", "}o--|{"),
        ("A 1+ to 0+ B : r", "}|--o{"),
        ("A only one to many B : r", "||--o{"),
        ("A one to many(0) B : r", "||--o{"),
        ("A zero or one to one or more B : r", "|o--|{"),
    ] {
        let (ids, notation, _) = relation(body);
        assert_eq!(
            ids,
            vec!["A".to_string(), "B".to_string()],
            "`{body}` did not build exactly the two entities the author declared"
        );
        assert_eq!(
            notation.as_deref(),
            Some(expected),
            "`{body}` resolved to the wrong cardinality shape"
        );
    }
}

/// ⚠️ THE ORDERING TRAPS, which a table written from what the syntax LOOKS like gets wrong.
///
/// `many` is ZERO_OR_MORE but `many(1)` is ONE_OR_MORE — the OPPOSITE cardinality — so a prefix
/// match on `many` silently reverses the second. `one` is ONLY_ONE but `one or more` is not, so a
/// bare `one` tried first swallows the phrase's first token. Both directions are asserted, since
/// each is a different reordering mistake.
#[test]
fn the_longer_spellings_win_over_the_shorter_ones_they_contain() {
    assert_eq!(relation("A many(1) to many(0) B : r").1.as_deref(), Some("}|--o{"));
    assert_eq!(relation("A many to many B : r").1.as_deref(), Some("}o--o{"));
    assert_eq!(relation("A one or more to one B : r").1.as_deref(), Some("}|--||"));
    assert_eq!(relation("A one to one B : r").1.as_deref(), Some("||--||"));
}

/// ⚠️ THE NEGATIVE CASE THAT MATTERS MOST: an entity NAMED like a cardinality is not eaten.
///
/// `ONE ||--|| TWO` is a real diagram. A strip that consumed the whole token would delete an entity
/// in order to name its own cardinality — the exact defect class this work exists to fix, reproduced
/// by the fix itself. `TOTAL` guards the other direction: `to` is a relation type, and matching it
/// inside a word would split an entity in half.
#[test]
fn an_entity_named_like_a_cardinality_survives() {
    for (body, ids) in [
        ("ONE ||--|| TWO : r", vec!["ONE", "TWO"]),
        ("MANY ||--|| OTHER : r", vec!["MANY", "OTHER"]),
        ("TOTAL ||--|| PART : r", vec!["PART", "TOTAL"]),
    ] {
        let (got, _, _) = relation(body);
        assert_eq!(
            got,
            ids.iter().map(ToString::to_string).collect::<Vec<String>>(),
            "`{body}` lost an entity to the cardinality lexer"
        );
    }
}

/// `to` is IDENTIFYING (solid) and `optionally to` is NON_IDENTIFYING (dashed).
///
/// ⚠️ Worth pinning because a first reading of the minified bundle got this pair INVERTED, mapping
/// `..` to identifying and `--` to non-identifying. Aligning each rule index to its own token return
/// gives the conventional mapping; this test is what fails if someone re-derives it the loose way.
#[test]
fn the_word_relation_types_carry_the_right_line() {
    let (_, solid, solid_arrow) = relation("A one to many B : r");
    assert_eq!(solid.as_deref(), Some("||--o{"));
    assert_eq!(solid_arrow, Some(ArrowType::Line));

    let (_, dashed, dashed_arrow) = relation("A many optionally to one B : r");
    assert_eq!(dashed.as_deref(), Some("}o..||"));
    assert_eq!(dashed_arrow, Some(ArrowType::DottedLine));
}

/// `.-` and `-.` are relation types, and they used to drop the ENTIRE line — second entity included.
#[test]
fn the_mixed_dash_dot_relation_types_are_dashed_and_keep_both_entities() {
    for body in ["A ||.-o{ B : r", "A ||-.o{ B : r"] {
        let (ids, notation, arrow) = relation(body);
        assert_eq!(
            ids,
            vec!["A".to_string(), "B".to_string()],
            "`{body}` dropped an entity"
        );
        assert_eq!(notation.as_deref(), Some("||..o{"), "`{body}`");
        assert_eq!(arrow, Some(ArrowType::DottedLine), "`{body}` is not dashed");
    }
}

/// MD_PARENT `u` is a cardinality, so it must not be absorbed into the entity id.
///
/// `A u--|| B` minted the phantom `A_u` — the same leading-marker-into-the-id defect as the
/// flowchart `o--o` (bd-zdpwd) and the class `o--` (bd-92b6), in a third diagram type. It names no
/// crow's-foot shape, so it draws nothing; the `||` on the far side must still survive, which is
/// what separates "recognised and drawn blank" from "silently swallowed along with its neighbour".
#[test]
fn md_parent_is_recognised_and_mints_no_phantom() {
    let (ids, notation, _) = relation("A u--|| B : r");
    assert_eq!(ids, vec!["A".to_string(), "B".to_string()]);
    assert_eq!(
        notation.as_deref(),
        Some("--||"),
        "the `u` end should draw nothing while the `||` end stays only-one"
    );
}

/// ⚠️ CONTROL: the symbolic path is untouched, byte for byte.
///
/// Every ER row in ci_docs_2000/5000 (165 and 442, all equivalent) takes the fused-operator path.
/// The word-form support is additive, and this is the assertion that says so — if a refactor ever
/// routes these through the new lexer and canonicalises them differently, this fails.
#[test]
fn the_symbolic_spellings_are_unchanged() {
    for (body, expected) in [
        ("CUSTOMER ||--o{ ORDER : places", "||--o{"),
        ("A }o--o{ B : r", "}o--o{"),
        ("A ||..|| B : r", "||..||"),
        ("A |o--o| B : r", "|o--o|"),
        ("A -- B : r", "--"),
    ] {
        assert_eq!(
            relation(body).1.as_deref(),
            Some(expected),
            "`{body}` changed shape"
        );
    }
}

/// A relation with no cardinality on either side still names both entities.
///
/// `STOCK to WAREHOUSE` is the minimal use of the new relation type: no cardinality to canonicalise,
/// so the notation is the bare connector and the value of the case is that neither entity is lost.
#[test]
fn a_word_relation_without_cardinality_keeps_both_entities() {
    let (ids, notation, arrow) = relation("STOCK to WAREHOUSE : r");
    assert_eq!(ids, vec!["STOCK".to_string(), "WAREHOUSE".to_string()]);
    assert_eq!(notation.as_deref(), Some("--"));
    assert_eq!(arrow, Some(ArrowType::Line));
}
