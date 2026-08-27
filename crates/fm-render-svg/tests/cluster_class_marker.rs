//! A `classDef` name applied to a subgraph reaches the cluster's class list (bd-6cdzy).
//!
//! REFERENCE BEHAVIOR, measured in Chromium 151 against the pinned mermaid 11.15.0 bundle, reading
//! the `class` attribute off the rendered groups:
//!
//! ```text
//!   class one hot   ->  <g class="cluster hot">        subgraph
//!   class a   hot   ->  <g class="node default hot">   node, for comparison
//!   (no class)      ->  <g class="cluster">
//! ```
//!
//! mermaid emits the name for BOTH. We emitted it for the node (`fm-node-user-hot`) and dropped it
//! for the subgraph, so an author's own stylesheet targeting `.hot` applied to half their diagram.
//!
//! ⚠️ THIS IS NOT A COLOUR BUG AND THE PAINT WAS NEVER BROKEN. bd-xfmm's channel resolves
//! `classDef`+`class` to an inline `style` on the cluster rect, verified 5/5 by
//! `scripts/headtohead/cluster_paint_diff.mjs`. What was missing is the NAME.

fn render(source: &str) -> String {
    fm_render_svg::render_svg(&fm_parser::parse(source).ir)
}

/// The `class="…"` value of the cluster rect.
fn cluster_class(svg: &str) -> String {
    let at = svg
        .find("class=\"fm-cluster ")
        .or_else(|| svg.find("class=\"fm-cluster\""))
        .expect("a cluster rect");
    let start = at + "class=\"".len();
    svg[start..][..svg[start..].find('"').expect("closing quote")].to_string()
}

fn node_class(svg: &str) -> String {
    let at = svg.find("class=\"fm-node ").expect("a node group");
    let start = at + "class=\"".len();
    svg[start..][..svg[start..].find('"').expect("closing quote")].to_string()
}

const STYLED: &str = "flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n  classDef hot fill:#ff0000\n  class one hot\n";

#[test]
fn a_classdef_name_applied_to_a_subgraph_reaches_the_cluster() {
    let svg = render(STYLED);
    assert!(
        cluster_class(&svg).contains("fm-cluster-user-hot"),
        "the subgraph's class name was dropped: {:?}",
        cluster_class(&svg)
    );
}

/// ⚠️ THE CONTROL bd-6cdzy REQUIRES: the SAME `classDef` must mark BOTH a node and a subgraph.
///
/// Asserting only the subgraph passes on an implementation that broke the node path getting there —
/// and the node path is the one that already worked, so a regression in it is the likeliest damage
/// this change could do. The defect was an ASYMMETRY; a test that looks at one side cannot see one.
#[test]
fn the_same_classdef_marks_both_a_node_and_a_subgraph() {
    let svg = render(
        "flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n  classDef hot fill:#ff0000\n  class one hot\n  class a hot\n",
    );
    assert!(
        cluster_class(&svg).contains("fm-cluster-user-hot"),
        "the subgraph lost the class: {:?}",
        cluster_class(&svg)
    );
    assert!(
        node_class(&svg).contains("fm-node-user-hot"),
        "the node lost the class: {:?}",
        node_class(&svg)
    );
}

/// ⚠️ NEGATIVE CASE: a subgraph with no `class` directive gains no marker.
///
/// This is what separates "the name is carried through" from "a marker is stamped on every cluster".
/// The latter would satisfy every `contains` assertion above while making the class meaningless.
#[test]
fn an_unclassed_subgraph_gains_no_marker() {
    let svg = render("flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n");
    assert_eq!(
        cluster_class(&svg),
        "fm-cluster",
        "a subgraph that declared no class gained one"
    );
}

/// ⚠️ NEGATIVE CASE: a name no `classDef` declares is warned about and NOT emitted.
///
/// mermaid's `setClass` pushes the name regardless; this parser warns and ignores it, and emitting a
/// marker for a name it just said it ignored would contradict the warning. Deliberate divergence,
/// pinned so it cannot drift silently.
#[test]
fn an_undeclared_class_name_is_not_emitted() {
    let parsed = fm_parser::parse(
        "flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n  class one nosuchclass\n",
    );
    let svg = fm_render_svg::render_svg(&parsed.ir);
    assert_eq!(
        cluster_class(&svg),
        "fm-cluster",
        "a class no classDef declares was still emitted"
    );
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|d| d.message.contains("nosuchclass")),
        "the ignored class produced no warning, so it vanished in silence"
    );
}

/// Several classes are several `class` lines — a comma separates TARGETS in mermaid's `setClass`,
/// not class names. Each declared class gets its own marker, so both are valid CSS selectors.
#[test]
fn two_class_lines_produce_two_markers() {
    let svg = render(
        "flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n  classDef hot fill:#ff0000\n  classDef big stroke-width:4px\n  class one hot\n  class one big\n",
    );
    let class = cluster_class(&svg);
    assert!(
        class.contains("fm-cluster-user-hot") && class.contains("fm-cluster-user-big"),
        "both declared classes must be marked: {class:?}"
    );
}

/// The same `class` line twice must not emit the marker twice — the cluster path dedupes exactly as
/// `add_class_to_node` does.
#[test]
fn a_repeated_class_is_not_marked_twice() {
    let svg = render(
        "flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n  classDef hot fill:#ff0000\n  class one hot\n  class one hot\n",
    );
    assert_eq!(
        cluster_class(&svg).matches("fm-cluster-user-hot").count(),
        1,
        "the marker was emitted twice: {:?}",
        cluster_class(&svg)
    );
}

/// A name that is not a valid CSS token is sanitized, exactly as a node's is: `my.odd~name` becomes
/// `fm-cluster-user-my-odd-name`. An unsanitized name would emit markup that no selector can match
/// and that a `.` in the middle silently turns into a compound selector.
#[test]
fn a_class_name_is_sanitized_into_a_css_token() {
    let svg = render(
        "flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n  classDef my.odd~name fill:#ff0000\n  class one my.odd~name\n",
    );
    assert!(
        cluster_class(&svg).contains("fm-cluster-user-my-odd-name"),
        "the class name was not sanitized into a token: {:?}",
        cluster_class(&svg)
    );
}

/// ⚠️ THE MARKER IS NOT LOAD-BEARING, which is what makes it safe to emit with no generated CSS rule
/// behind it.
///
/// The node path emits `.fm-node-user-{slug} .fm-node-shape { … }` rules because a node's paint is
/// delivered BY that rule. A cluster's paint arrives as an inline `style` on the rect instead
/// (bd-xfmm), so this marker carries no styling load and is purely a hook for the author's own CSS —
/// which is what mermaid's bare class is too. That distinguishes it from the lib.rs:3320 defect,
/// where a class was emitted that the renderer's own stylesheet was meant to target and did not.
#[test]
fn a_cluster_marker_class_is_not_load_bearing() {
    let svg = render(STYLED);
    assert!(
        svg.contains("fill:#ff0000"),
        "the declared fill vanished: {svg}"
    );
    assert!(
        !svg.contains(".fm-cluster-user-"),
        "a CSS RULE was generated for the marker; if the paint now depends on it, say so — this \
         test exists because the inline style is supposed to carry it"
    );
}
