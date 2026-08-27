//! Differential test: the `[type]` row mermaid draws beneath a C4 boundary's label.
//!
//! TWO DIVERGENCES, one cause.
//!
//! 1. mermaid draws every boundary as TWO rows — the bold label, then its type in square brackets.
//!    We drew only the label, so `System_Boundary(sb, "Sys")` lost `[SYSTEM]` and
//!    `Boundary(gb, "Generic", "custom")` silently discarded the author's own third argument.
//!
//! 2. `fm-cluster-c4` was applied to ZERO elements. The renderer decided "is this a C4 boundary?"
//!    by asking whether the cluster TITLE contained `System_Boundary` / `Container_Boundary` /
//!    `Enterprise_Boundary` / `Deployment_Node` — true only while the parser titled a boundary with
//!    a reconstruction of its own source syntax. bd-039t replaced that with the author's label, and
//!    the predicate became permanently false: C4 boundaries lost their fill, stroke, dash and corner
//!    radius, and the stylesheet kept a rule nothing used. Measured before the fix: `c4_container`,
//!    `c4_deployment`, `c4_component` and `c4_basic` each emitted 0 elements carrying the class.
//!
//! REFERENCE BEHAVIOUR. mermaid's `drawInsideBoundary` rewrites the stored type before drawing:
//!
//! ```text
//!   if (l.type && l.type.text !== "") { l.type.text = "[" + l.type.text + "]"; ... }
//! ```
//!
//! and `drawBoundary` hands `t.type.text` to the same text helper it used for `t.label.text` one
//! line earlier.
//!
//! ⚠️ THIS OVERTURNS A DOCUMENTED "PARITY, NOT A GAP" FINDING, so it was settled by the oracle that
//! finding itself named as decisive — a Chromium render. C4 renders under neither cheap oracle (no
//! head-to-head corpus item; its renderer will not run under jsdom), so the pinned 11.15.0 bundle
//! was driven in Chromium 151 over CDP. Its drawn runs, verbatim:
//!
//! ```text
//!   ["<<person>>","A","Sys","[SYSTEM]","<<person>>","C","Generic","[custom]",
//!    "<<person>>","D","Cont","[CONTAINER]","Corp","[ENTERPRISE]","T"]
//! ```
//!
//! and for a deployment diagram: `N2 / [node]`, `DN / [node]`, `DN3 / [Ubuntu]`.
//!
//! The prior analysis was not careless — it read `drawBoundary` closely and correctly reported that
//! no bracket literal appears in it. The bracketing is in the CALLER. Auditing one function and
//! concluding about a value that a different function mutates is the failure mode worth remembering.

fn cluster_runs(source: &str) -> Vec<String> {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(source).ir);
    let mut out = Vec::new();
    let mut rest = svg.as_str();
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        let Some(open) = rest.find('>') else { break };
        let Some(close) = rest.find("</text>") else {
            break;
        };
        let attrs = &rest[..open];
        let body = rest[open + 1..close]
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&");
        let body = body.trim().to_string();
        if !body.is_empty() && attrs.contains("fm-cluster") {
            out.push(body);
        }
        rest = &rest[close + "</text>".len()..];
    }
    out
}

/// Count of ELEMENTS carrying the class, not occurrences of the string.
///
/// ⚠️ `svg.contains("fm-cluster-c4")` is true in every themed document, because the stylesheet
/// declares a `.fm-cluster-c4` rule. That is exactly how a class applied to nothing reads as
/// present — match the `class` ATTRIBUTE instead.
fn elements_with_class(source: &str, class_name: &str) -> usize {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(source).ir);
    let needle = "class=\"";
    let mut count = 0;
    let mut rest = svg.as_str();
    while let Some(at) = rest.find(needle) {
        rest = &rest[at + needle.len()..];
        if let Some(end) = rest.find('"')
            && rest[..end].split_whitespace().any(|c| c == class_name)
        {
            count += 1;
        }
    }
    count
}

fn boundary(declaration: &str) -> String {
    format!("C4Context\n    title T\n    {declaration} {{\n      Person(a, \"A\")\n    }}\n")
}

/// Every boundary macro and the type mermaid draws for it, bracketed as mermaid brackets it.
///
/// ⚠️ THE ARITY IS NOT UNIFORM, and each row was measured rather than assumed: the three named
/// `*_Boundary` macros IGNORE a third argument and always draw their constant, while the generic
/// `Boundary` and the `Node` family take the third argument as the type.
const TABLE: &[(&str, &str)] = &[
    ("System_Boundary(b, \"L\")", "[SYSTEM]"),
    ("Container_Boundary(b, \"L\")", "[CONTAINER]"),
    ("Enterprise_Boundary(b, \"L\")", "[ENTERPRISE]"),
    ("Boundary(b, \"L\")", "[system]"),
    ("Boundary(b, \"L\", \"custom\")", "[custom]"),
    ("Node(b, \"L\")", "[node]"),
    ("Node_L(b, \"L\")", "[node]"),
    ("Node_R(b, \"L\")", "[node]"),
    ("Deployment_Node(b, \"L\")", "[node]"),
    ("Deployment_Node(b, \"L\", \"Ubuntu\")", "[Ubuntu]"),
    ("Node_L(b, \"L\", \"third\")", "[third]"),
];

#[test]
fn every_boundary_draws_the_type_row_mermaid_draws() {
    for (declaration, expected) in TABLE {
        let runs = cluster_runs(&boundary(declaration));
        assert!(
            runs.iter().any(|run| run == expected),
            "{declaration} must draw {expected:?}; drew {runs:?}"
        );
    }
}

/// ⚠️ THE NEGATIVE CONTROL FOR DEFECT 1: the label alone is not enough.
///
/// This is what shipped — the boundary rendered its caption and nothing else — and it is what an
/// implementation that forgets the second row produces.
#[test]
fn the_label_alone_is_not_a_complete_boundary() {
    for (declaration, _) in TABLE {
        let runs = cluster_runs(&boundary(declaration));
        assert!(
            runs.len() >= 2,
            "{declaration} drew only {runs:?}; mermaid draws the label AND its bracketed type"
        );
    }
}

/// ⚠️ CONTROL FOR THE CASE, which mermaid is not consistent about.
///
/// The three named macros yield UPPERCASE constants and the generic families yield lowercase. An
/// implementation that tidied this — lowercasing everything, or title-casing it — would satisfy
/// "there are two rows" and still be wrong in eight of the eleven rows above.
#[test]
fn the_type_keeps_mermaids_own_case() {
    let upper = cluster_runs(&boundary("System_Boundary(b, \"L\")"));
    assert!(
        upper.iter().any(|run| run == "[SYSTEM]") && !upper.iter().any(|run| run == "[system]"),
        "System_Boundary must draw the uppercase [SYSTEM]: {upper:?}"
    );
    let lower = cluster_runs(&boundary("Boundary(b, \"L\")"));
    assert!(
        lower.iter().any(|run| run == "[system]") && !lower.iter().any(|run| run == "[SYSTEM]"),
        "a generic Boundary must draw the lowercase [system]: {lower:?}"
    );
}

/// ⚠️ CONTROL FOR THE ARITY. A named boundary macro must IGNORE a third argument.
///
/// An implementation that read the third argument uniformly would pass every row of the table that
/// has no third argument, and quietly retitle the ones that do.
#[test]
fn a_named_boundary_macro_ignores_a_third_argument() {
    for (declaration, constant) in [
        ("System_Boundary(b, \"L\", \"third\")", "[SYSTEM]"),
        ("Container_Boundary(b, \"L\", \"third\")", "[CONTAINER]"),
        ("Enterprise_Boundary(b, \"L\", \"third\")", "[ENTERPRISE]"),
    ] {
        let runs = cluster_runs(&boundary(declaration));
        assert!(
            runs.iter().any(|run| run == constant),
            "{declaration} must still draw {constant:?}; drew {runs:?}"
        );
        assert!(
            !runs.iter().any(|run| run == "[third]"),
            "{declaration} took a third argument mermaid ignores: {runs:?}"
        );
    }
}

/// ⚠️ THE NEGATIVE CONTROL FOR DEFECT 2, and the one that would have caught it years earlier.
///
/// `fm-cluster-c4` was applied to zero elements while its stylesheet rule stayed in every themed
/// document, so every "is the class there?" check written as a substring search said yes.
#[test]
fn a_c4_boundary_actually_carries_the_c4_cluster_class() {
    let source = boundary("System_Boundary(b, \"L\")");
    assert!(
        elements_with_class(&source, "fm-cluster-c4") >= 1,
        "no element carries fm-cluster-c4, so C4 boundary styling is dead again"
    );
}

/// CONTROL: an ordinary flowchart subgraph is NOT a C4 boundary.
///
/// Guards the opposite failure — a fix that marked every cluster — which would give plain subgraphs
/// C4 styling and a spurious bracketed row.
#[test]
fn a_plain_subgraph_gets_neither_the_class_nor_a_type_row() {
    let source = "flowchart TD\n  subgraph one[One]\n    a --> b\n  end\n";
    assert_eq!(
        elements_with_class(source, "fm-cluster-c4"),
        0,
        "a flowchart subgraph was marked as a C4 boundary"
    );
    let runs = cluster_runs(source);
    assert!(
        !runs
            .iter()
            .any(|run| run.starts_with('[') && run.ends_with(']')),
        "a flowchart subgraph drew a C4 boundary type row: {runs:?}"
    );
}

/// CONTROL ON GEOMETRY: the new row must sit between the label and the cluster's contents.
///
/// A second row of text is only a fix if it lands in the padding band. Emitting it over the first
/// contained node would be a different defect wearing this one's fix.
#[test]
fn the_type_row_sits_below_the_label_and_above_the_contents() {
    let source = boundary("System_Boundary(b, \"L\")");
    let svg = fm_render_svg::render_svg(&fm_parser::parse(&source).ir);

    let y_of = |needle: &str| -> f32 {
        let at = svg.find(needle).expect("text run present");
        let open = svg[..at].rfind("<text").expect("enclosing text element");
        let attrs = &svg[open..at];
        let y_at = attrs.rfind(" y=\"").expect("y attribute");
        let rest = &attrs[y_at + 4..];
        let end = rest.find('"').expect("terminated y attribute");
        rest[..end].parse().expect("numeric y")
    };

    let label_y = y_of(">L</text>");
    let type_y = y_of(">[SYSTEM]</text>");
    let node_y = y_of(">A</text>");
    assert!(
        label_y < type_y,
        "the type row ({type_y}) must sit below the label ({label_y})"
    );
    assert!(
        type_y < node_y,
        "the type row ({type_y}) must sit above the contained node ({node_y})"
    );
}
