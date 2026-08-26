//! Differential test: gitGraph commit text has to reach the DRAWN SVG, not just the IR (bd-3cj8v).
//!
//! `crates/fm-parser/tests/gitgraph_commit_differential.rs` pins the semantics against the 32-row
//! fixture the pinned mermaid 11.15.0 bundle generated. This is its wiring half, and it exists
//! because two of the three defects it covers are invisible one layer up:
//!
//!   * the id suppression that blanked `type: REVERSE` and `type: HIGHLIGHT` commits lived in the
//!     RENDERER, matching on node shape;
//!   * once those commits started drawing text, a filled circle was still SIZED 20x20 by
//!     fm-layout, so `commit id: "three" type: REVERSE` rendered as the elided `th…` — text
//!     present, content destroyed. Nothing at the IR level can see that.
//!
//! ⚠️ THE RUNS ARE READ, NOT `svg.contains(..)`. Every label also reaches the accessibility
//! `<desc>`, so a substring check passes on a diagram whose commit dots are empty — the exact bug
//! this pins would go undetected. `three` occurs 10 times in the correct output and 9 times in the
//! broken one.

/// Every `<text>` run in the document, inner markup stripped and XML escapes restored.
///
/// ⚠️ The scan takes the tag's own `>`, which is safe only because an escaped `>` inside an
/// attribute value is written `&gt;` and never appears raw. Splitting the document on `>` instead
/// reads an escaped entity as a tag close and reports present text as empty.
fn text_runs(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        let Some(open_end) = rest.find('>') else {
            break;
        };
        rest = &rest[open_end + 1..];
        let Some(close) = rest.find("</text>") else {
            break;
        };
        let mut text = String::new();
        let mut inner = &rest[..close];
        while let Some(tag) = inner.find('<') {
            text.push_str(&inner[..tag]);
            let Some(tag_end) = inner[tag..].find('>') else {
                break;
            };
            inner = &inner[tag + tag_end + 1..];
        }
        text.push_str(inner);
        out.push(
            text.replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"")
                .replace("&#39;", "'")
                .replace("&amp;", "&")
                .trim()
                .to_string(),
        );
        rest = &rest[close + "</text>".len()..];
    }
    out
}

fn drawn(source: &str) -> Vec<String> {
    let ir = fm_parser::parse(source).ir;
    text_runs(&fm_render_svg::render_svg(&ir))
}

/// The same four-commit diagram that first exposed the blank `<text>`: one plain commit, one of
/// each styled type, and an explicit `NORMAL` to prove the styled ones are the odd cases.
#[test]
fn every_commit_type_draws_its_id() {
    let runs = drawn(
        "gitGraph\n\
             commit id: \"plain\"\n\
             commit id: \"rev\" type: REVERSE\n\
             commit id: \"high\" type: HIGHLIGHT\n\
             commit id: \"norm\" type: NORMAL\n",
    );
    for id in ["plain", "rev", "high", "norm"] {
        assert!(
            runs.iter().any(|run| run == id),
            "commit {id:?} is not among the drawn text runs {runs:?}"
        );
    }
    // NEGATIVE CONTROL. Suppressing the id fallback by shape — the state-diagram rule gitGraph
    // inherited — draws `["main", "plain", "", "", "norm"]`: the two styled commits become empty
    // text elements, still present in the document and still counted by a `contains` check.
    assert!(
        !runs.iter().any(String::is_empty),
        "a commit drew an EMPTY text element: {runs:?}"
    );
}

/// Both halves of a tagged commit — the id and every tag — in one drawn run, untruncated.
#[test]
fn a_tagged_commit_draws_its_id_and_all_of_its_tags() {
    let runs = drawn(
        "gitGraph\n\
             commit id: \"one\"\n\
             commit id: \"two\" tag: \"v1.0\" tag: \"stable\"\n",
    );
    assert!(
        runs.iter().any(|run| run == "two [v1.0] [stable]"),
        "expected the id and BOTH tags in one run, got {runs:?}"
    );
    // NEGATIVE CONTROL for the last-tag-wins parse: it draws `[stable]` alone, which contains
    // neither `two` nor `v1.0`.
    assert!(
        runs.iter().any(|run| run.contains("v1.0")),
        "the first of two tags never reached the drawing: {runs:?}"
    );
}

/// ⚠️ THE SIZING HALF. A filled circle was pinned to 20x20 whatever text it carried, so the moment
/// REVERSE commits started drawing their ids the renderer elided them to fit the dot. The text was
/// there and its content was gone — the failure mode a presence check cannot see, which is why this
/// asserts the WHOLE string rather than a prefix.
#[test]
fn a_reverse_commit_label_is_not_elided_to_fit_the_dot() {
    let runs = drawn(
        "gitGraph\n\
             commit id: \"one\"\n\
             commit id: \"three\" type: REVERSE\n",
    );
    assert!(
        runs.iter().any(|run| run == "three"),
        "expected the full id `three`; a 20x20 filled circle elides it to `th…`. Runs: {runs:?}"
    );
    assert!(
        !runs.iter().any(|run| run.contains('…')),
        "a commit label was truncated: {runs:?}"
    );
}

/// ⚠️ THE OTHER DIRECTION. The suppression this change narrowed is load-bearing everywhere it was
/// already right, and a fix that simply deleted it would resurrect internal ids as visible text.
/// State `[*]` pseudo-states carry generated ids and a flowchart `junction` is a 7px dot whose
/// label mermaid's own handler clears with a literal `t.label = ""`.
#[test]
fn ornament_nodes_still_draw_nothing() {
    let state = drawn("stateDiagram-v2\n    [*] --> Working\n    Working --> [*]\n");
    assert!(
        !state.iter().any(|run| run.contains("__state")),
        "a state pseudo-state leaked its generated id into the drawing: {state:?}"
    );
    assert!(
        state.iter().any(|run| run == "Working"),
        "the real state stopped being drawn, so the check above proves nothing: {state:?}"
    );

    let flowchart = drawn("flowchart LR\n    A@{ shape: f-circ }\n    A --> B\n");
    assert!(
        !flowchart.iter().any(|run| run == "A"),
        "a flowchart junction drew its id; mermaid's handler clears that label: {flowchart:?}"
    );
    assert!(
        flowchart.iter().any(|run| run == "B"),
        "the ordinary node stopped being drawn, so the check above proves nothing: {flowchart:?}"
    );
}
