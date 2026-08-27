//! A `direction`/`title` line must never be DRAWN as diagram content (bd-92kw1).
//!
//! THE DEFECT. Six families let these lines fall through to their node parser, so the diagram gained
//! a box captioned with the author's own directive — the phantom bd-871ka found for
//! `hide empty description` and bd-9x8r for `shape:`. This is strictly worse than the false warnings
//! bd-xym5x fixed: a warning is noise, a phantom node changes what the diagram SAYS, takes layout
//! space, and is read out by screen readers as content.
//!
//! REFERENCE, drawn text measured in Chromium 151 against the pinned mermaid 11.15.0 bundle, with
//! `flowchart` and `state` as controls that already agree in both engines:
//!
//! ```text
//!   case                  reference       ours (before)
//!   class + direction     not drawn       DRAWN   <== gap
//!   er + direction        not drawn       DRAWN   <== gap
//!   timeline + direction  not drawn       DRAWN   <== gap
//!   packet + title        not drawn       DRAWN   <== gap
//!   journey + direction   parse error     DRAWN
//!   mindmap + direction   parse error     DRAWN
//!   packet + direction    parse error     DRAWN
//!   mindmap + title       parse error     DRAWN
//!   state + direction     not drawn       not drawn   (control)
//!   flowchart + direction not drawn       not drawn   (control)
//!   block + direction     DRAWN           DRAWN       (control — both draw it)
//! ```
//!
//! ⚠️ `block-beta` IS DELIBERATELY UNTOUCHED. mermaid's block renderer draws a stray `direction`
//! line as a block, so both engines already agree; adding the skip there would replace agreement
//! with a divergence. It is asserted below so a later tidy-up cannot "finish the job" and break it.
//!
//! ⚠️ CLASS AND ER HONOUR THE LINE RATHER THAN SWALLOWING IT. `direction` is documented mermaid
//! syntax in both, so consuming it silently would fix the phantom and still ignore the author. They
//! now set the diagram direction, which is what the line means.
//!
//! ⚠️ HOW THIS WAS FOUND, AND THE INSTRUMENT THAT WAS WRONG FIRST. The sweep originally compared
//! `ir.labels` with and without the directive, and flagged five kanban cases. All five were FALSE
//! POSITIVES: kanban interns those labels and its renderer never draws them — and mermaid's kanban
//! DOES draw the same lines as cards, so we were better than the reference exactly where the sweep
//! said we were worse. Re-run against DRAWN `<text>` (with `<title>`/`<desc>` stripped, since that
//! is where these directives belong), the kanban rows vanished and the real six appeared.

/// The drawn text of a rendered diagram: `<text>` content only.
///
/// ⚠️ `<title>`/`<desc>` ARE STRIPPED FIRST, and that is not a convenience. `accTitle` and `title`
/// are SUPPOSED to appear there — it is the accessibility tree. A check that searched the whole
/// document could not tell "the directive was honoured" from "the directive became a node", which
/// is the entire question.
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
        let raw = &rest[..end];
        // Inner markup (`<tspan>`) is stripped so a wrapped label reads as its joined text.
        let mut text = String::new();
        let mut in_tag = false;
        for ch in raw.chars() {
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

fn draws(source: &str, needle: &str) -> bool {
    drawn_text(source).iter().any(|t| t.contains(needle))
}

const ARROW: &str = "-->";

/// The six families and the directive line each of them was drawing.
fn cases() -> Vec<(&'static str, String, &'static str)> {
    vec![
        (
            "class",
            "classDiagram\n  direction TB\n  class Animal\n".to_string(),
            "direction",
        ),
        (
            "er",
            "erDiagram\n  direction TB\n  A ||--o{ B : r\n".to_string(),
            "direction",
        ),
        (
            "timeline",
            "timeline\n  title T\n  direction TB\n  2024 : x\n".to_string(),
            "direction",
        ),
        (
            "journey",
            "journey\n  title D\n  direction TB\n  section M\n    Wake: 3: Me\n".to_string(),
            "direction",
        ),
        (
            "mindmap",
            "mindmap\n  direction TB\n  root((r))\n    a\n".to_string(),
            "direction",
        ),
        (
            "packet",
            "packet-beta\n  direction TB\n  0-7: \"a\"\n".to_string(),
            "direction",
        ),
        (
            "mindmap title",
            "mindmap\n  title A Title\n  root((r))\n    a\n".to_string(),
            "title A Title",
        ),
        (
            "packet title",
            "packet-beta\n  title A Title\n  0-7: \"a\"\n".to_string(),
            "title A Title",
        ),
    ]
}

/// ⚠️ THE NEGATIVE CASE: the directive must not be drawn, and the diagram must still be drawn.
///
/// Both halves matter. "Nothing is drawn" also passes the first assertion, and would mean the skip
/// swallowed the diagram along with the directive — trading a phantom node for an empty picture,
/// which is the same trade in the other direction.
#[test]
fn no_family_draws_a_directive_line_as_content() {
    for (name, source, needle) in cases() {
        let texts = drawn_text(&source);
        assert!(
            !texts.iter().any(|t| t.contains(needle)),
            "{name} draws the directive as content: {texts:?}"
        );
        assert!(
            !texts.is_empty(),
            "{name} drew nothing at all, so the skip swallowed the diagram: {source:?}"
        );
    }
}

/// The diagram's own content survives, item for item.
///
/// A skip that also ate a real line would still pass the "something was drawn" check above. This
/// compares against the same source WITHOUT the directive: nothing the baseline draws may go
/// missing, and nothing added may quote the directive.
///
/// ⚠️ NOT AN EQUALITY, AND THE REASON IS A REAL BEHAVIOUR. It was written as one, and mindmap failed
/// it by drawing `A Title` — because `title A Title` is now PROMOTED to the diagram title and drawn
/// as a title, which is the honoured outcome rather than the phantom. An equality here would forbid
/// the fix from working. What must not appear is the raw directive line, which is what the second
/// assertion pins; loosening it to "the baseline is a subset" without that would have made the test
/// vacuous.
#[test]
fn the_diagram_is_unchanged_apart_from_the_directive() {
    for (name, source, needle) in cases() {
        let with_directive = drawn_text(&source);
        let without: String = source
            .lines()
            .filter(|l| {
                let t = l.trim();
                !(t.starts_with("direction ") || t == "title A Title")
            })
            .map(|l| format!("{l}\n"))
            .collect();
        let baseline = drawn_text(&without);

        for text in &baseline {
            assert!(
                with_directive.contains(text),
                "{name}: the directive cost the diagram its {text:?}"
            );
        }
        for text in &with_directive {
            assert!(
                !text.contains(needle),
                "{name}: the directive line itself was drawn as {text:?}"
            );
        }
    }
}

/// ⚠️ CLASS AND ER HONOUR IT — the half that separates a fix from a swallow.
///
/// Both accept `direction` as documented mermaid syntax, so the line has a meaning and dropping it
/// silently would be a quieter version of the same defect: the author asked for something and got
/// nothing. `TB` is asserted against `LR` so the test cannot pass on a default.
#[test]
fn class_and_er_apply_the_direction_they_used_to_draw() {
    for (name, source_tb, source_lr) in [
        (
            "class",
            "classDiagram\n  direction TB\n  class Animal\n",
            "classDiagram\n  direction LR\n  class Animal\n",
        ),
        (
            "er",
            "erDiagram\n  direction TB\n  A ||--o{ B : r\n",
            "erDiagram\n  direction LR\n  A ||--o{ B : r\n",
        ),
    ] {
        let tb = fm_parser::parse(source_tb).ir.direction;
        let lr = fm_parser::parse(source_lr).ir.direction;
        assert_eq!(
            tb,
            fm_core::GraphDirection::TB,
            "{name} did not apply `direction TB`"
        );
        assert_eq!(
            lr,
            fm_core::GraphDirection::LR,
            "{name} did not apply `direction LR`"
        );
        assert_ne!(
            tb, lr,
            "{name} reports the same direction either way, so it is returning a default"
        );
    }
}

/// ⚠️ `block-beta` MUST KEEP DRAWING IT, because the reference does.
///
/// Measured: mermaid's block renderer draws a stray `direction` line as a block. Adding the skip
/// there would trade agreement for a divergence — so this pins the ONE family deliberately left out
/// of the fix, which is otherwise indistinguishable from one that was forgotten.
#[test]
fn block_beta_still_draws_it_as_the_reference_does() {
    assert!(
        draws(
            "block-beta\n  direction TB\n  columns 1\n  a\n",
            "direction"
        ),
        "block-beta stopped drawing a line the reference draws"
    );
}

/// CONTROLS: the families that were already right stay right.
///
/// `flowchart` and `state` both honour `direction` and draw nothing — they are how we know the
/// correct behaviour was reachable, and a regression here would mean the new predicate reached
/// further than intended.
#[test]
fn the_families_that_were_already_correct_are_unchanged() {
    for (name, source) in [
        (
            "flowchart",
            format!("flowchart LR\n  direction TB\n  A {ARROW} B\n"),
        ),
        (
            "state",
            format!("stateDiagram-v2\n  direction TB\n  [*] {ARROW} Idle\n"),
        ),
    ] {
        assert!(
            !draws(&source, "direction"),
            "{name} started drawing the directive"
        );
        assert_eq!(
            fm_parser::parse(&source).ir.direction,
            fm_core::GraphDirection::TB,
            "{name} stopped applying the direction"
        );
    }
}

/// ⚠️ AND A NODE WHOSE NAME MERELY BEGINS WITH THE KEYWORD IS STILL A NODE.
///
/// `starts_with("direction")` alone would swallow `directionality` and every identifier with that
/// prefix. The predicate keys on the keyword followed by WHITESPACE, and this is what proves it —
/// the same hole the accessibility skip had to avoid in bd-xym5x.
#[test]
fn a_name_that_only_starts_with_the_keyword_is_still_content() {
    for (name, source, needle) in [
        (
            "class",
            "classDiagram\n  class directionality\n".to_string(),
            "directionality",
        ),
        (
            "flowchart",
            format!("flowchart LR\n  directionality {ARROW} B\n"),
            "directionality",
        ),
        (
            "mindmap",
            "mindmap\n  root((r))\n    titleholder\n".to_string(),
            "titleholder",
        ),
    ] {
        assert!(
            draws(&source, needle),
            "{name} swallowed `{needle}`, so the skip matches on prefix rather than on the keyword"
        );
    }
}

/// The title still reaches diagram meta where a family has no title line of its own.
///
/// Skipping the line must not cost the title: `extract_generic_diagram_title` already promotes it,
/// which is why the line does not need to become a node in the first place.
#[test]
fn a_skipped_title_still_becomes_the_diagram_title() {
    for (name, source) in [
        ("mindmap", "mindmap\n  title A Title\n  root((r))\n    a\n"),
        ("packet", "packet-beta\n  title A Title\n  0-7: \"a\"\n"),
    ] {
        assert_eq!(
            fm_parser::parse(source).ir.meta.title.as_deref(),
            Some("A Title"),
            "{name} lost the title along with the phantom node"
        );
    }
}
