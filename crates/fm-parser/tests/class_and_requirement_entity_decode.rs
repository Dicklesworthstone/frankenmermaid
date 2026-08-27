//! Class members and requirement fields decode their entities (bd-rbwov).
//!
//! `decode_mermaid_entities` has three callers, and neither of these paths was one. Both build their
//! text with a bare `to_string()`, so `+x&amp;y` in a class body and `text: x&amp;y` in a
//! requirement put the ENTITY on screen where every other position shows the `&` it denotes.
//!
//! Measured as drawn text in a Chromium 151 render of the pinned mermaid 11.15.0 bundle, both
//! engines read through the same DOM: the reference decodes both, we drew them raw.
//!
//! ⚠️ THE SWEEP FOUND A SECOND, DIFFERENT DEFECT AND IT IS NOT FIXED HERE. `subgraph "Plain Title"`
//! comes out as the identifier `Plain_Title` — spaces turned to underscores — and
//! `subgraph "x&amp;y"` collapses to `x`. That is the quoted title being normalized as an ID rather
//! than kept as a label, which is a different fault from a missing decode and a more visible one;
//! `subgraph one["Plain Title"]` is unaffected. It is filed rather than folded in, and pinned below
//! so the follow-up starts from an assertion.
//!
//! ⚠️ THE RETURN TYPE IS DECODED TOO, THOUGH THE BEAD NAMED ONLY THE MEMBER NAME. It is drawn beside
//! the name from the same parse, and fixing one of a drawn pair is the asymmetric-sibling shape this
//! parser keeps producing.

fn class_members(source: &str) -> Vec<String> {
    let ir = fm_parser::parse(source).ir;
    ir.nodes
        .iter()
        .filter_map(|node| node.class_meta.as_ref())
        .flat_map(|meta| meta.attributes.iter().chain(meta.methods.iter()))
        .map(|m| {
            m.return_type
                .as_ref()
                .map_or_else(|| m.name.clone(), |ret| format!("{} : {ret}", m.name))
        })
        .collect()
}

/// ⚠️ THE NEGATIVE CASE: the entity is DECODED, and it is still the author's text.
///
/// Asserting only "no `&amp;` remains" passes if the member was dropped, or truncated at the `&`.
/// The decoded form is asserted exactly, so a member that lost its tail fails.
#[test]
fn a_class_member_decodes_its_entities() {
    for (raw, expected) in [
        ("+x&amp;y", "x&y"),
        ("+a&lt;b", "a<b"),
        ("+C#35;44", "C#44"),
        ("+plain", "plain"),
    ] {
        let members = class_members(&format!("classDiagram\n  class A {{\n    {raw}\n  }}\n"));
        assert_eq!(
            members,
            vec![expected.to_string()],
            "the member {raw:?} was not decoded to {expected:?}"
        );
    }
}

/// The return type decodes as well, and both halves survive together.
#[test]
fn a_member_return_type_decodes_with_its_name() {
    let members = class_members("classDiagram\n  class A {\n    +run(x&amp;y) r&amp;t\n  }\n");
    assert_eq!(members.len(), 1, "the member was lost: {members:?}");
    let member = &members[0];
    assert!(
        member.contains("x&y"),
        "the parameter list kept its entity: {member}"
    );
    assert!(
        member.contains("r&t"),
        "the return type kept its entity: {member}"
    );
    assert!(
        !member.contains("&amp;"),
        "an entity survived somewhere in the member: {member}"
    );
}

/// Requirement fields decode.
#[test]
fn requirement_fields_decode_their_entities() {
    let parsed = fm_parser::parse(
        "requirementDiagram\n  requirement r {\n  id: 1\n  text: x&amp;y\n  risk: low\n  verifymethod: test\n  }\n",
    );
    let meta = parsed
        .ir
        .nodes
        .iter()
        .find_map(|n| n.requirement_meta.as_ref())
        .expect("the requirement carries its meta");
    assert_eq!(
        meta.text.as_deref(),
        Some("x&y"),
        "the requirement text kept its entity"
    );
}

/// A quoted requirement field decodes AFTER its quotes are stripped, not instead of.
///
/// The two steps run in one expression; doing them in the other order would leave `"x&y"` with its
/// quotes, and doing only one would leave the entity.
#[test]
fn a_quoted_requirement_field_is_both_unquoted_and_decoded() {
    let parsed = fm_parser::parse(
        "requirementDiagram\n  requirement r {\n  id: 1\n  text: \"x&amp;y\"\n  risk: low\n  verifymethod: test\n  }\n",
    );
    let meta = parsed
        .ir
        .nodes
        .iter()
        .find_map(|n| n.requirement_meta.as_ref())
        .expect("the requirement carries its meta");
    assert_eq!(
        meta.text.as_deref(),
        Some("x&y"),
        "the quoted field kept its quotes or its entity"
    );
}

/// CONTROL: text with nothing to decode comes through unchanged.
///
/// `decode_mermaid_entities` scans for `&` and `#`, and a member can legitimately hold either — a
/// `C#` return type, a `#count` name, a bare `&`. None has a closing `;`, so none may be rewritten.
///
/// ⚠️ EXPECTATIONS ARE SPELLED OUT PER ROW, because deriving them from the input is how this test
/// first failed: it stripped `()` from the raw and compared against the joined member, which no
/// correct parse could satisfy. `+run() C#` parses to the name `run()` and the return type `C#`,
/// and that IS right — the assertion was wrong.
#[test]
fn text_with_no_entity_is_unchanged() {
    for (raw, expected) in [
        ("+plain", "plain"),
        ("+list#count", "list#count"),
        ("+a & b", "a & b"),
        ("+run() C#", "run() : C#"),
    ] {
        let members = class_members(&format!("classDiagram\n  class A {{\n    {raw}\n  }}\n"));
        assert_eq!(
            members,
            vec![expected.to_string()],
            "{raw:?} was rewritten by the decode"
        );
    }
}

/// ⚠️ THE SECOND DEFECT THE SWEEP FOUND, PINNED RATHER THAN LEFT UNRECORDED.
///
/// `subgraph "Plain Title"` is normalized as an IDENTIFIER, so its spaces become underscores and
/// `subgraph "x&amp;y"` collapses to `x`. The bracketed form `subgraph one["Plain Title"]` is fine,
/// which is why this went unnoticed. It is a different fault from a missing decode — the title is
/// being run through the wrong normalizer entirely — and it is filed separately.
///
/// Asserted at its CURRENT value so the follow-up fails here and has to update the note, the way
/// bd-j06n2's pin forced bd-idjwr's.
#[test]
fn a_quoted_subgraph_title_is_still_id_normalized() {
    let arrow = "-->";
    let parsed = fm_parser::parse(&format!(
        "flowchart LR\n  subgraph \"Plain Title\"\n    A {arrow} B\n  end\n"
    ));
    let ir = &parsed.ir;
    let title = ir
        .clusters
        .first()
        .and_then(|c| c.title)
        .and_then(|id| ir.labels.get(id.0))
        .map(|l| l.text.clone())
        .expect("the subgraph has a title");
    assert_eq!(
        title, "Plain_Title",
        "the quoted subgraph title is no longer id-normalized. That is an improvement — update \
         this test and the notes in this file rather than deleting them"
    );
}
