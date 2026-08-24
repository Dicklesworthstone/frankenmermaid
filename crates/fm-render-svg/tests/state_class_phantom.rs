//! `class A bad` styles state A — it must not ALSO be drawn as a state (bd-0audg).
//!
//! The styling worked perfectly; the line was DOUBLE-handled. Style directives are extracted
//! globally for every diagram type before the state parser runs, so `A` really did come out
//! carrying the class — and then the same line fell through to the node parser, where
//! `normalize_identifier` interned it as a state keyed `class_A_bad`. The rendered SVG contained a
//! box captioned `class A bad`: the reader saw text nobody wrote.
//!
//! Third instance of a documented family in this parser — bd-871ka (`hide empty description` drew a
//! box), bd-xfmm (subgraph phantom), bd-yrxu (an invented `A_e1`) — and scoped to state diagrams
//! only: flowchart and classDiagram already handled the same statement correctly, so our own three
//! diagram types disagreed with each other.
//!
//! These assert on the RENDERED document, not just the IR, because "phantom node" is only a defect
//! insofar as someone sees it.

/// Every `<text>` run in the document.
///
/// Structural rather than `svg.contains(...)`: the directive text also reaches the accessibility
/// `<desc>`, so a substring check reports the phantom as present even after it stops being drawn,
/// and reports it as absent for the wrong reason if the `<desc>` wording ever changes.
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
        out.push(rest[..close].to_string());
        rest = &rest[close + "</text>".len()..];
    }
    out
}

fn node_ids(ir: &fm_core::MermaidDiagramIr) -> Vec<&str> {
    ir.nodes.iter().map(|node| node.id.as_str()).collect()
}

/// THE DIRECTIVE IS NOT A STATE, and the style it carries still lands.
///
/// Both halves in one test on purpose: the cheap way to remove a phantom is to stop processing the
/// line at all, which would silently drop the styling. Asserting the class is still applied is what
/// makes that shortcut fail.
#[test]
fn a_state_class_directive_is_styled_but_never_drawn() {
    let source = "stateDiagram-v2\n  [*] --> A\n  classDef bad fill:#f00\n  class A bad\n";
    let ir = fm_parser::parse(source).ir;

    assert!(
        !node_ids(&ir).iter().any(|id| id.contains("class")),
        "the directive was interned as a state: {:?}",
        node_ids(&ir)
    );

    // CONTROL, and the half a naive fix breaks: the style must still reach the target.
    let styled = ir
        .nodes
        .iter()
        .find(|node| node.id.as_str() == "A")
        .expect("CONTROL FAILED: state A was not declared, so nothing here is under test");
    assert!(
        styled.classes.iter().any(|applied| applied == "bad"),
        "the class stopped being applied to its target; A carries {:?}",
        styled.classes
    );

    // RENDERED, because a phantom is only a defect insofar as it is drawn.
    let runs = text_runs(&fm_render_svg::render_svg(&ir));
    assert!(
        !runs.iter().any(|run| run.contains("class A bad")),
        "the SVG drew a box captioned with the author's own directive; text runs were {runs:?}"
    );
    // NON-VACUITY: the diagram must actually have drawn something, or "no phantom" is a statement
    // about an empty picture.
    assert!(
        runs.iter().any(|run| run == "A"),
        "CONTROL FAILED: the real state was not drawn either; text runs were {runs:?}"
    );
}

/// A multi-target directive styles every target and still declares nothing.
#[test]
fn a_multi_target_state_class_directive_styles_each_and_declares_none() {
    let source =
        "stateDiagram-v2\n  [*] --> A\n  A --> B\n  classDef bad fill:#f00\n  class A,B bad\n";
    let ir = fm_parser::parse(source).ir;

    for target in ["A", "B"] {
        let node = ir
            .nodes
            .iter()
            .find(|node| node.id.as_str() == target)
            .unwrap_or_else(|| panic!("CONTROL FAILED: state {target} was not declared"));
        assert!(
            node.classes.iter().any(|applied| applied == "bad"),
            "{target} lost its class; it carries {:?}",
            node.classes
        );
    }
    assert!(
        !node_ids(&ir).iter().any(|id| id.contains("class")),
        "the multi-target directive was interned as a state: {:?}",
        node_ids(&ir)
    );
}

/// CONTROL: a state legitimately NAMED `class` is still a state.
///
/// The guard bails on a transition arrow for exactly this case. Without it, `class --> Idle` would
/// read as "apply class `Idle`", and the fix for a phantom box would have deleted a real one — the
/// bd-ij0f shape, where a widened filter ate valid input.
#[test]
fn a_state_named_class_is_not_swallowed_by_the_directive_guard() {
    let ir = fm_parser::parse("stateDiagram-v2\n  [*] --> class\n  class --> Idle\n").ir;
    let ids = node_ids(&ir);
    assert!(
        ids.contains(&"class"),
        "a state named `class` was swallowed as a directive: {ids:?}"
    );
    assert!(
        ids.contains(&"Idle"),
        "the transition's target was lost: {ids:?}"
    );
}

/// CONTROL: a bare `class A`, with no class list, is left alone for its own path.
///
/// The guard requires TWO tokens. This pins that boundary, so tightening or loosening the token
/// count is a deliberate change rather than an accident.
#[test]
fn a_bare_class_statement_is_not_treated_as_a_style_directive() {
    let ir = fm_parser::parse("stateDiagram-v2\n  [*] --> A\n  class A\n").ir;
    // Whatever the bare form means, it must not have been silently discarded as a directive: the
    // real states are still here.
    let ids = node_ids(&ir);
    assert!(ids.contains(&"A"), "state A was lost: {ids:?}");
}
