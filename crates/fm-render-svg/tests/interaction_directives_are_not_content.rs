//! `click` / `link` / `callback` / `cssClass` must never be DRAWN as diagram content (bd-rnc6l).
//!
//! THE REPORTED CASE was `click`, in six families. Sweeping its SIBLINGS first is what made this
//! worth doing: `link`, `callback` and `cssClass` reach drawn output in EIGHT families — including
//! `flowchart`, the family that handles `click` correctly. That is the asymmetric-sibling shape this
//! parser has produced before, and fixing only the reported keyword would have left its three twins
//! in place in the most-used diagram type.
//!
//! ⚠️ AND THE COMMENT IN THE SOURCE SAID OTHERWISE. `parse_flowchart_document_items` carried
//! "the flowchart path already HANDLES the rest — … click/link/callback as real
//! `FlowAst::ClickDirective` items". True of the `click …` spellings only: both grammars are
//! `just("click")`. A bare `link A "url"`, `callback A fn "tip"` or `cssClass "A" mine` fell
//! straight through to the node parser. The comment is corrected in place.
//!
//! REFERENCE, drawn text measured in Chromium 151 against the pinned mermaid 11.15.0 bundle:
//!
//! ```text
//!   flowchart  click, click callback     not drawn / not drawn   (control: already right)
//!   flowchart  link, callback, cssClass  PARSE ERROR / DRAWN     <== gap x3
//!   pie        click, link               PARSE ERROR / DRAWN     <== gap x2
//!   quadrant   click, link               PARSE ERROR / DRAWN     <== gap x2
//!   mindmap    all five                  PARSE ERROR / DRAWN     <== gap x5
//!   packet     click cb, callback, cssClass  PARSE ERROR / DRAWN <== gap x3
//!   gantt      link                      PARSE ERROR / DRAWN     <== gap
//!   block      cssClass                  PARSE ERROR / DRAWN     <== gap
//! ```
//!
//! The `cssClass` rows were re-measured WITH the `classDef` its documentation pairs it with, so a
//! reference parse error could not be blamed on naming a class that does not exist. Same verdict.
//!
//! ⚠️ `journey` AND `timeline` ARE DELIBERATELY UNTOUCHED, and this is the same call `block-beta`
//! got for `direction` in bd-92kw1. Their grammars treat essentially any line as content and MERMAID
//! AGREES on the headline cases — journey draws `click A "url"` and `link A "url"`, timeline draws
//! `cssClass "A" mine`, in both engines. Silencing them would replace agreement with a divergence
//! for the sake of tidiness. Their remaining cells are left for a separate decision rather than
//! folded in here, and the agreement is pinned below so a later sweep cannot "finish the job".

/// Drawn text only: `<text>` content, with `<title>`/`<desc>` removed first.
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

fn draws(source: &str, needle: &str) -> bool {
    drawn_text(source).iter().any(|t| t.contains(needle))
}

const ARROW: &str = "-->";

/// (family, header, body) for the seven families this fixes.
fn families() -> Vec<(&'static str, String, String)> {
    vec![
        (
            "flowchart",
            "flowchart LR\n".to_string(),
            format!("  A {ARROW} B\n"),
        ),
        (
            "pie",
            "pie title V\n".to_string(),
            "  \"A\" : 40\n".to_string(),
        ),
        (
            "quadrant",
            "quadrantChart\n  title T\n".to_string(),
            format!("  x-axis A {ARROW} B\n"),
        ),
        (
            "mindmap",
            "mindmap\n".to_string(),
            "  root((r))\n    a\n".to_string(),
        ),
        (
            "packet",
            "packet-beta\n".to_string(),
            "  0-7: \"a\"\n".to_string(),
        ),
        (
            "gantt",
            "gantt\n".to_string(),
            "  dateFormat YYYY-MM-DD\n  section S\n  T :a1, 2024-01-01, 30d\n".to_string(),
        ),
        (
            "block",
            "block-beta\n".to_string(),
            "  columns 1\n  a\n".to_string(),
        ),
    ]
}

/// The four directives, and the text that must not appear in the drawing.
const DIRECTIVES: [(&str, &str); 5] = [
    ("  click A \"https://example.com\"\n", "click A"),
    ("  click A callback \"tip\"\n", "click A callback"),
    ("  link A \"https://example.com\"\n", "link A"),
    ("  callback A myFunc \"tip\"\n", "callback A"),
    ("  cssClass \"A\" mine\n", "cssClass"),
];

/// ⚠️ THE NEGATIVE CASE: the directive is not drawn, and the diagram still is.
///
/// Both halves. "Nothing drawn" also satisfies the first assertion and would mean the guard ate the
/// diagram — the same trade in the other direction.
#[test]
fn no_interaction_directive_is_drawn_as_content() {
    let mut checked = 0;
    for (family, header, body) in families() {
        for (snippet, needle) in DIRECTIVES {
            let source = format!("{header}{snippet}{body}");
            let texts = drawn_text(&source);
            assert!(
                !texts.iter().any(|t| t.contains(needle)),
                "{family} draws `{}` as content: {texts:?}",
                snippet.trim()
            );
            assert!(
                !texts.is_empty(),
                "{family} drew nothing at all for `{}`, so the guard swallowed the diagram",
                snippet.trim()
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 35, "the family/directive table lost an entry");
}

/// The diagram's own content survives the guard.
#[test]
fn the_diagram_is_unchanged_apart_from_the_directive() {
    for (family, header, body) in families() {
        let baseline = drawn_text(&format!("{header}{body}"));
        for (snippet, _) in DIRECTIVES {
            let with_directive = drawn_text(&format!("{header}{snippet}{body}"));
            for text in &baseline {
                assert!(
                    with_directive.contains(text),
                    "{family}: `{}` cost the diagram its {text:?}",
                    snippet.trim()
                );
            }
        }
    }
}

/// ⚠️ FLOWCHART STILL PARSES `click` INTO A REAL DIRECTIVE.
///
/// This is the half the fix could most easily break, and it has been broken before: bd-ij0f applied
/// the whole shared predicate on this path and swallowed `click` before it was parsed, failing eight
/// tests. `click` is deliberately absent from the flowchart guard's keyword list, and this is what
/// proves it — the node keeps its link, so the directive was acted on rather than ignored.
#[test]
fn flowchart_click_is_still_parsed_not_swallowed() {
    let source = format!("flowchart LR\n  A {ARROW} B\n  click A \"https://example.com\"\n");
    let parsed = fm_parser::parse(&source);

    let node_a = parsed
        .ir
        .nodes
        .iter()
        .find(|n| n.id == "A")
        .expect("node A survived");
    // ⚠️ ASSERTED ON THE IR, NOT ON THE RENDERED URL. `MermaidLinkMode` defaults to `Off`, so a
    // default render emits no href at all and a `contains("https://…")` check would pass or fail on
    // the RENDER CONFIG rather than on whether the directive was parsed.
    assert!(
        node_a.interaction.is_some(),
        "`click` was swallowed instead of parsed: node A carries no interaction"
    );

    // The control: without the directive the same node has none, so the assertion above is about
    // the `click` line and not about nodes always carrying one.
    let bare = fm_parser::parse(&format!("flowchart LR\n  A {ARROW} B\n"));
    assert!(
        bare.ir
            .nodes
            .iter()
            .find(|n| n.id == "A")
            .expect("node A")
            .interaction
            .is_none(),
        "every flowchart node carries an interaction, so this test proves nothing"
    );

    assert!(
        !draws(&source, "click A"),
        "the directive text itself is drawn"
    );
}

/// ⚠️ `journey` AND `timeline` KEEP DRAWING WHAT THE REFERENCE DRAWS.
///
/// Measured: mermaid's journey draws `click A "url"` and `link A "url"`, and its timeline draws
/// `cssClass "A" mine`. Those are agreements, and this pins them — a family left out on purpose is
/// otherwise indistinguishable from one that was forgotten, which is exactly how block-beta would
/// have been "tidied" in bd-92kw1.
#[test]
fn the_families_that_agree_with_the_reference_are_untouched() {
    for (name, source, needle) in [
        (
            "journey click",
            "journey\n  title D\n  click A \"https://example.com\"\n  section M\n    Wake: 3: Me\n",
            "click A",
        ),
        (
            "journey link",
            "journey\n  title D\n  link A \"https://example.com\"\n  section M\n    Wake: 3: Me\n",
            "link A",
        ),
        (
            "timeline cssClass",
            "timeline\n  title T\n  cssClass \"A\" mine\n  2024 : x\n",
            "cssClass",
        ),
    ] {
        assert!(
            draws(source, needle),
            "{name} stopped drawing a line the reference also draws"
        );
    }
}

/// ⚠️ A NODE WHOSE NAME MERELY BEGINS WITH A KEYWORD IS STILL A NODE.
///
/// `starts_with("link")` alone swallows `linkage`; `starts_with("click")` swallows `clickable`. The
/// guard requires the keyword to be followed by whitespace, and these are the ids that prove it.
#[test]
fn a_name_that_only_starts_with_a_keyword_is_still_content() {
    for (name, source, needle) in [
        (
            "flowchart linkage",
            format!("flowchart LR\n  linkage {ARROW} B\n"),
            "linkage",
        ),
        (
            "flowchart clickable",
            format!("flowchart LR\n  clickable {ARROW} B\n"),
            "clickable",
        ),
        (
            "mindmap callbacks",
            "mindmap\n  root((r))\n    callbacks\n".to_string(),
            "callbacks",
        ),
    ] {
        assert!(
            draws(&source, needle),
            "{name}: `{needle}` was swallowed, so the guard matches on prefix rather than keyword"
        );
    }
}

/// The guard adds no node, which the drawn-text check cannot see on its own.
///
/// `parse_er`'s own guard records a phantom that had no label to draw while still taking layout
/// space and being announced as a key node. Counting is what catches that one.
#[test]
fn the_directive_adds_no_node() {
    for (family, header, body) in families() {
        let baseline = fm_parser::parse(&format!("{header}{body}")).ir.nodes.len();
        for (snippet, _) in DIRECTIVES {
            let count = fm_parser::parse(&format!("{header}{snippet}{body}"))
                .ir
                .nodes
                .len();
            assert_eq!(
                count,
                baseline,
                "{family}: `{}` added a node ({baseline} -> {count})",
                snippet.trim()
            );
        }
    }
}
