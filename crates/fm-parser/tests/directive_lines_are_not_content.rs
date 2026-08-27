//! A `direction`/`title` line must not become a NODE either (bd-92kw1) — the IR-level half.
//!
//! Its sibling `fm-render-svg/tests/directive_lines_are_not_content.rs` asserts that these lines are
//! not DRAWN. This one asserts that they never become nodes at all, and the two are not the same
//! claim.
//!
//! ⚠️ A PHANTOM NODE CAN BE INVISIBLE AND STILL DO DAMAGE, which this repo has already measured. The
//! comment on `parse_er`'s own guard records it for `class CUSTOMER bad`: the phantom entity had no
//! label to draw, so a drawn-text check saw nothing — while `data-nodes` went 2 to 3, it got its own
//! group, it took LAYOUT SPACE (the viewBox grew from 326x437 to 395x623, shifting the real
//! entities), and the accessibility description announced it as a key node. A screen reader read the
//! author's directive out as content.
//!
//! So: drawn-text alone would let exactly that case back in, and node-count alone would have missed
//! the kanban false positives that a drawn-text re-run cleared. Both halves, deliberately.

use fm_core::{DiagramType, GraphDirection};

/// Sources whose directive line must leave no trace in the node list.
fn cases() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        (
            "class",
            "classDiagram\n  direction TB\n  class Animal\n",
            "direction",
        ),
        (
            "er",
            "erDiagram\n  direction TB\n  A ||--o{ B : r\n",
            "direction",
        ),
        (
            "timeline",
            "timeline\n  title T\n  direction TB\n  2024 : x\n",
            "direction",
        ),
        (
            "journey",
            "journey\n  title D\n  direction TB\n  section M\n    Wake: 3: Me\n",
            "direction",
        ),
        (
            "mindmap",
            "mindmap\n  direction TB\n  root((r))\n    a\n",
            "direction",
        ),
        (
            "packet",
            "packet-beta\n  direction TB\n  0-7: \"a\"\n",
            "direction",
        ),
        (
            "mindmap title",
            "mindmap\n  title A Title\n  root((r))\n    a\n",
            "title",
        ),
        (
            "packet title",
            "packet-beta\n  title A Title\n  0-7: \"a\"\n",
            "title",
        ),
    ]
}

/// Every text the IR holds for a node — its id and its label.
///
/// Both, because a phantom can arrive under either: `hide empty description` became a state keyed
/// `hide_empty_description` with no label at all (bd-871ka), so checking labels alone would have
/// missed it.
fn node_texts(source: &str) -> Vec<String> {
    let parsed = fm_parser::parse(source);
    let ir = &parsed.ir;
    let mut out = Vec::new();
    for node in &ir.nodes {
        out.push(node.id.to_string());
        if let Some(label) = node.label.and_then(|id| ir.labels.get(id.0)) {
            out.push(label.text.clone());
        }
    }
    out
}

/// ⚠️ THE NEGATIVE CASE: the directive is not a node, under any spelling of its id.
#[test]
fn no_directive_line_becomes_a_node() {
    for (name, source, needle) in cases() {
        let texts = node_texts(source);
        assert!(
            !texts
                .iter()
                .any(|t| t.to_ascii_lowercase().contains(needle)),
            "{name} interned the directive as a node: {texts:?}"
        );
        assert!(
            !texts.is_empty(),
            "{name} has no nodes at all, so the skip took the diagram with it"
        );
    }
}

/// The node COUNT is unchanged, which the id/label scan cannot see on its own.
///
/// A phantom with no label and a generated id — `hide_empty_description`'s failure mode — can slip
/// past a text scan. Counting against the same source without the directive cannot.
#[test]
fn the_directive_adds_no_node() {
    for (name, source, _) in cases() {
        let without: String = source
            .lines()
            .filter(|l| {
                let t = l.trim();
                !(t.starts_with("direction ") || t == "title A Title")
            })
            .map(|l| format!("{l}\n"))
            .collect();
        let with_count = fm_parser::parse(source).ir.nodes.len();
        let without_count = fm_parser::parse(&without).ir.nodes.len();
        assert_eq!(
            with_count, without_count,
            "{name}: the directive line added a node ({without_count} -> {with_count})"
        );
    }
}

/// The families that HONOUR the directive apply it, and are still themselves.
#[test]
fn class_and_er_apply_the_direction() {
    for (name, ty, tb, lr) in [
        (
            "class",
            DiagramType::Class,
            "classDiagram\n  direction TB\n  class Animal\n",
            "classDiagram\n  direction LR\n  class Animal\n",
        ),
        (
            "er",
            DiagramType::Er,
            "erDiagram\n  direction TB\n  A ||--o{ B : r\n",
            "erDiagram\n  direction LR\n  A ||--o{ B : r\n",
        ),
    ] {
        let parsed_tb = fm_parser::parse(tb);
        assert_eq!(parsed_tb.ir.diagram_type, ty, "{name} changed family");
        assert_eq!(
            parsed_tb.ir.direction,
            GraphDirection::TB,
            "{name} did not apply `direction TB`"
        );
        assert_ne!(
            parsed_tb.ir.direction,
            fm_parser::parse(lr).ir.direction,
            "{name} reports one direction whatever is written, so it is returning a default"
        );
    }
}

/// And neither of them warns about a line it now understands.
#[test]
fn honouring_the_direction_is_silent() {
    for (name, source) in [
        ("class", "classDiagram\n  direction TB\n  class Animal\n"),
        ("er", "erDiagram\n  direction TB\n  A ||--o{ B : r\n"),
    ] {
        let warnings = fm_parser::parse(source).warnings;
        assert!(
            !warnings.iter().any(|w| w.contains("direction")),
            "{name} warns about a directive it applies: {warnings:?}"
        );
    }
}

/// ⚠️ A NODE WHOSE NAME MERELY BEGINS WITH THE KEYWORD IS STILL A NODE.
///
/// `starts_with("direction")` alone swallows `directionality`. The predicate requires the keyword to
/// be followed by whitespace; this is what proves it still does.
#[test]
fn a_name_that_only_starts_with_the_keyword_is_still_a_node() {
    for (name, source, needle) in [
        (
            "class",
            "classDiagram\n  class directionality\n",
            "directionality",
        ),
        (
            "mindmap",
            "mindmap\n  root((r))\n    titleholder\n",
            "titleholder",
        ),
    ] {
        let texts = node_texts(source);
        assert!(
            texts.iter().any(|t| t.contains(needle)),
            "{name} swallowed `{needle}`: the skip matches on prefix rather than on the keyword"
        );
    }
}

/// `block-beta` is deliberately excluded from the fix, because the reference draws the line too.
#[test]
fn block_beta_still_takes_it_as_content() {
    let texts = node_texts("block-beta\n  direction TB\n  columns 1\n  a\n");
    assert!(
        texts.iter().any(|t| t.contains("direction")),
        "block-beta stopped taking a line the reference also takes: {texts:?}"
    );
}
