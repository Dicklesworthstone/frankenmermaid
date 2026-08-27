//! `<br/>` in a label becomes a line break, not literal text (bd-kmtia).
//!
//! THE DEFECT, and it is an asymmetric sibling inside ONE function. `replace_br_with_newlines` has
//! existed and been correct all along, and `parse_label_inner` called it from its markdown-string
//! branch and not from its plain branch. So ``A["`one<br/>two`"]`` drew two lines and
//! `A["one<br/>two"]` — the same label without the backticks — drew the tag. Measured across 18
//! label sites in a Chromium 151 render of the pinned mermaid 11.15.0 bundle, the tag reached drawn
//! text in SEVENTEEN of them; only sequence messages were right, because they are the one other
//! caller of that helper.
//!
//! ⚠️ THE FAST PATH WOULD HAVE MADE THE FIX INERT. `parse_label_inner` returns the raw label
//! untouched when it holds none of `" ' ` & #`, and `one<br/>two` holds none of them — so the
//! conversion below it was unreachable for exactly the common case. `<` had to join that byte set,
//! and a slow-path-only test would have passed against a fix that did nothing.
//!
//! ⚠️ FIVE OF THIS SWEEP'S FIRST EIGHT "DIFFERENCES" WERE THE INSTRUMENT. It compared mermaid's
//! decoded `textContent` against our raw SVG markup, so `a &amp; b` — the correct XML spelling of
//! `a & b` — read as a divergence, as did `&lt;` and `&nbsp;`. Reading both engines through the same
//! DOM left three real gaps, of which this is one.
//!
//! ⚠️ TWO CELLS NOW DIVERGE FROM THE REFERENCE ON PURPOSE, AND THAT IS A JUDGEMENT CALL WORTH
//! STATING. mermaid honours `<br/>` in a flowchart node, a class name, a mindmap node, a block and a
//! C4 element — and NOT in a journey task or a gantt task, where it draws the tag. Our label
//! normalizer is shared, so honouring it in the first five honours it in the last two as well.
//!
//! Reproducing mermaid's per-family inconsistency would mean threading a "keep the tag" flag through
//! the shared normalizer to make two families deliberately worse. That is chasing a grammar accident
//! rather than a specification, so the divergence is kept and pinned below instead — it is a
//! SUPERSET (a tag the author wrote is honoured), not a dropped or invented element.
//!
//! This differs from the call bd-umqc6 made for journey/timeline, and the difference is the harm:
//! there the reference and this engine agreed on drawing a DIRECTIVE as content, and matching it
//! meant not inventing a phantom node. Here both engines draw the author's own text; the only
//! question is whether one of its tags is honoured.

fn drawn_text(source: &str) -> Vec<String> {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(source).ir);
    let mut body = svg;
    for (open, close) in [("<title>", "</title>"), ("<desc>", "</desc>")] {
        while let Some(start) = body.find(open) {
            let Some(end) = body[start..].find(close) else {
                break;
            };
            body.replace_range(start..start + end + close.len(), "");
        }
    }
    let mut out = Vec::new();
    let mut rest = body.as_str();
    while let Some(at) = rest.find("<text") {
        rest = &rest[at..];
        let Some(gt) = rest.find('>') else { break };
        rest = &rest[gt + 1..];
        let Some(end) = rest.find("</text>") else {
            break;
        };
        // Inner markup is stripped, so a two-line label reads as its joined runs — which is exactly
        // what distinguishes a honoured `<br/>` from a literal one: the literal keeps the `<br`
        // characters as TEXT, and those survive this stripping because they are escaped in the
        // source and therefore not markup.
        let mut text = String::new();
        let mut in_tag = false;
        for ch in rest[..end].chars() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                c if !in_tag => text.push(c),
                _ => {}
            }
        }
        out.push(text);
        rest = &rest[end + 7..];
    }
    out
}

const BR: &str = "<br/>";
const ARROW: &str = "-->";

/// The sites the reference joins, and which this closes.
fn honoured_sites() -> Vec<(&'static str, String)> {
    vec![
        (
            "flowchart node",
            format!("flowchart LR\n  A[\"one{BR}two\"] {ARROW} B\n"),
        ),
        (
            "class name",
            format!("classDiagram\n  class A[\"one{BR}two\"]\n"),
        ),
        ("mindmap node", format!("mindmap\n  root((one{BR}two))\n")),
        (
            "block",
            format!("block-beta\n  columns 1\n  a[\"one{BR}two\"]\n"),
        ),
        (
            "c4",
            format!("C4Context\n  title T\n  Person(a, \"one{BR}two\")\n"),
        ),
        ("kanban card", format!("kanban\n  col1\n    one{BR}two\n")),
        (
            "sequence message",
            format!("sequenceDiagram\n  Alice->>Bob: one{BR}two\n"),
        ),
    ]
}

/// ⚠️ THE NEGATIVE CASE: the tag is gone AND both halves of the label survive.
///
/// "The tag is gone" passes on its own if the label was dropped entirely, or truncated at the tag —
/// which is what the markdown-newline path actually does elsewhere. Both words must still be drawn.
#[test]
fn the_line_break_tag_is_honoured_not_drawn() {
    let mut checked = 0;
    for (name, source) in honoured_sites() {
        let texts = drawn_text(&source);
        let joined = texts.join("|");
        assert!(
            !joined.contains("&lt;br") && !joined.contains("<br"),
            "{name} draws the tag as literal text: {texts:?}"
        );
        assert!(
            joined.contains("one") && joined.contains("two"),
            "{name} lost half the label: {texts:?}"
        );
        checked += 1;
    }
    assert_eq!(checked, 7, "the site table lost an entry");
}

/// ⚠️ AND IT IS A REAL LINE BREAK, NOT A DELETED TAG.
///
/// `one<br/>two` with the tag simply removed renders as `onetwo` on one line, which passes every
/// assertion above. The label must carry a newline through to the IR, which is what the renderer
/// splits on.
#[test]
fn the_tag_becomes_a_newline_in_the_ir() {
    let parsed = fm_parser::parse(&format!("flowchart LR\n  A[\"one{BR}two\"] {ARROW} B\n"));
    let label = parsed
        .ir
        .nodes
        .iter()
        .find(|n| n.id == "A")
        .and_then(|n| n.label)
        .and_then(|id| parsed.ir.labels.get(id.0))
        .map(|l| l.text.as_str())
        .expect("node A has a label");
    assert_eq!(
        label, "one\ntwo",
        "the tag was removed rather than converted, so the two words run together"
    );
}

/// All three spellings mermaid accepts are converted.
#[test]
fn every_spelling_of_the_tag_is_converted() {
    for spelling in ["<br/>", "<br>", "<br />"] {
        let parsed = fm_parser::parse(&format!(
            "flowchart LR\n  A[\"one{spelling}two\"] {ARROW} B\n"
        ));
        let label = parsed
            .ir
            .nodes
            .iter()
            .find(|n| n.id == "A")
            .and_then(|n| n.label)
            .and_then(|id| parsed.ir.labels.get(id.0))
            .map(|l| l.text.as_str())
            .expect("node A has a label");
        assert_eq!(
            label, "one\ntwo",
            "the {spelling:?} spelling was not converted"
        );
    }
}

/// ⚠️ THE FAST PATH DOES NOT SWALLOW THE TAG.
///
/// A label holding none of `" ' ` & #` returns from `parse_label_inner` before any conversion runs,
/// and `one<br/>two` holds none of them — so this is the case the fix had to reach, and the case a
/// quoted-label test would silently miss. The label here is UNQUOTED on purpose.
#[test]
fn the_unquoted_fast_path_label_is_still_converted() {
    let parsed = fm_parser::parse(&format!("flowchart LR\n  A[one{BR}two] {ARROW} B\n"));
    let label = parsed
        .ir
        .nodes
        .iter()
        .find(|n| n.id == "A")
        .and_then(|n| n.label)
        .and_then(|id| parsed.ir.labels.get(id.0))
        .map(|l| l.text.as_str())
        .expect("node A has a label");
    assert_eq!(
        label, "one\ntwo",
        "the fast path returned the label untouched, so the conversion is unreachable for \
         unquoted labels — which is most of them"
    );
}

/// A label with no tag is untouched, and still takes the fast path's answer.
#[test]
fn a_label_without_the_tag_is_unchanged() {
    for label in ["hello", "a < b", "plain text here"] {
        let parsed = fm_parser::parse(&format!("flowchart LR\n  A[\"{label}\"] {ARROW} B\n"));
        let drawn = parsed
            .ir
            .nodes
            .iter()
            .find(|n| n.id == "A")
            .and_then(|n| n.label)
            .and_then(|id| parsed.ir.labels.get(id.0))
            .map(|l| l.text.as_str())
            .expect("node A has a label");
        assert_eq!(drawn, label, "an untagged label was rewritten");
    }
}

/// ⚠️ THE TWO DELIBERATE SUPERSETS, PINNED SO THEY STAY VISIBLE.
///
/// mermaid draws the raw tag in a journey task and a gantt task; we honour it, because the label
/// normalizer is shared and making these two worse on purpose would mean threading a flag through it
/// to reproduce a grammar accident. Asserted rather than left unstated: a future change that
/// reverses this has to fail here and say so.
#[test]
fn journey_and_gantt_honour_the_tag_where_the_reference_does_not() {
    for (name, source) in [
        (
            "journey",
            format!("journey\n  title D\n  section M\n    one{BR}two: 3: Me\n"),
        ),
        (
            "gantt",
            format!(
                "gantt\n  dateFormat YYYY-MM-DD\n  section S\n  one{BR}two :a1, 2024-01-01, 30d\n"
            ),
        ),
    ] {
        let texts = drawn_text(&source);
        let joined = texts.join("|");
        assert!(
            !joined.contains("&lt;br") && !joined.contains("<br"),
            "{name} draws the tag; if that is now intended, update the notes in this file and in \
             bd-kmtia rather than deleting this test: {texts:?}"
        );
    }
}

/// CONTROL: a `<` that is not a line-break tag is still ordinary text.
///
/// Adding `<` to the fast path's byte set routes every label containing one through the slow path.
/// That must change nothing except the tag itself — a label like `a < b` must survive intact and
/// must not acquire a newline.
#[test]
fn an_angle_bracket_that_is_not_a_tag_is_left_alone() {
    for label in ["a < b", "x <y> z", "1<2"] {
        let parsed = fm_parser::parse(&format!("flowchart LR\n  A[\"{label}\"] {ARROW} B\n"));
        let text = parsed
            .ir
            .nodes
            .iter()
            .find(|n| n.id == "A")
            .and_then(|n| n.label)
            .and_then(|id| parsed.ir.labels.get(id.0))
            .map(|l| l.text.as_str())
            .expect("node A has a label");
        assert_eq!(text, label, "a non-tag `<` label was rewritten");
        assert!(
            !text.contains('\n'),
            "a non-tag `<` label gained a line break"
        );
    }
}
