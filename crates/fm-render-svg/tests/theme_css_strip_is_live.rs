//! `strip_unused_theme_css` must actually STRIP, not silently no-op.
//!
//! The lever removes theme rule blocks a diagram cannot use — the `.fm-cluster*` rules when there
//! are no clusters, the special-node-shape rules when none are present, the `.fm-edge-dashed` /
//! `.fm-edge-thick` rules when no edge is dotted or thick. It works by finding each block as an
//! EXACT substring of the emitted stylesheet, and its constants carry a deliberate
//! safe-no-op-if-it-drifts contract: an unmatched constant strips nothing rather than corrupting
//! the CSS.
//!
//! THAT CONTRACT IS SAFE AND SILENT, WHICH IS THE PROBLEM. When the theme's own CSS was restyled —
//! `.fm-cluster` moving to `stroke-dasharray: 4 4` and `rx: 10`, `.fm-edge-dashed` to
//! `stroke-dasharray: 5 5`, `.fm-edge-thick` to `stroke-width: 2.25` — the constants stopped
//! matching and two of the four blocks quietly stopped stripping. Nothing failed. Every SVG began
//! shipping ~660 B of rules whose selectors match no element in it, and the only thing that
//! noticed was fm-cli's minimizer canary, which found that even the EMPTY input now "reproduced" a
//! `fm-edge-dashed` signature.
//!
//! So the gate cannot be "the constant is a substring of the theme" — that is the same string
//! comparison the lever already does, and it would drift with it. It has to be behavioural: render
//! a diagram that uses NONE of these features and assert the selectors are absent from the output.
//! A constant that stops matching then fails loudly here.

use fm_render_svg::{SvgRenderConfig, ThemePreset, render_svg_with_config};

/// A plain flowchart: no subgraphs, no note/cloud/cylinder/star/pentagon shape, no dotted or thick
/// edge. Every gated block is dead weight in its output.
const PLAIN: &str = "flowchart LR\n  A[Alpha] --> B[Beta]\n  B --> C[Gamma]\n";

fn render(source: &str, theme: ThemePreset) -> String {
    render_svg_with_config(
        &fm_parser::parse(source).ir,
        &SvgRenderConfig {
            theme,
            ..SvgRenderConfig::default()
        },
    )
}

/// Selectors, in the MINIFIED spelling the `<style>` block actually carries. Matching the pretty
/// form here would make the test vacuous against a minified stylesheet — it would find nothing and
/// pass no matter what shipped.
/// ⚠️ EACH NEEDLE MUST BE UNIQUE TO ITS GATED BLOCK. A bare `.fm-cluster{` fails here for the wrong
/// reason — the ungated state CSS carries its own `.fm-cluster{fill-opacity: …}`, and
/// `.fm-cluster-label{` appears inside an ungated `.fm-node text, .fm-edge-labeled text,
/// .fm-cluster-label{` rule. A needle that matches a rule the diagram legitimately keeps turns this
/// gate into a permanent false alarm, which is how a gate gets weakened to shut it up.
const DEAD_SELECTORS: &[&str] = &[
    ".fm-cluster{fill: var(--fm-cluster-fill)",
    ".fm-cluster-c4{",
    ".fm-cluster-swimlane{",
    "--fm-cluster-c4-fill",
    "--fm-cluster-swimlane-fill",
    ".fm-node-shape-note",
    ".fm-node-shape-cloud",
    ".fm-node-shape-cylinder",
    ".fm-edge-dashed{",
    ".fm-edge-thick{",
];

#[test]
fn a_plain_flowchart_ships_none_of_the_gated_theme_blocks() {
    for theme in [ThemePreset::Default, ThemePreset::Dark] {
        let svg = render(PLAIN, theme);
        // CONTROL: the stylesheet must actually be there, or every assertion below passes for the
        // wrong reason. This is the failure mode the whole test exists to avoid.
        assert!(
            svg.contains(".fm-node{") || svg.contains(".fm-node {"),
            "{theme:?}: no theme stylesheet in the output — the assertions below prove nothing"
        );
        for selector in DEAD_SELECTORS {
            assert!(
                !svg.contains(selector),
                "{theme:?}: `{selector}` survived into a diagram that cannot use it — a strip \
                 constant has drifted from the theme CSS and is silently matching nothing"
            );
        }
    }
}

/// The other direction: a diagram that DOES use a feature must keep its rules. A "strip everything"
/// regression would satisfy the test above and break every such diagram, so the gate needs both
/// halves.
#[test]
fn a_diagram_that_uses_a_gated_feature_keeps_its_rules() {
    let clustered = render(
        "flowchart LR\n  subgraph one[One]\n    A --> B\n  end\n  B --> C\n",
        ThemePreset::Default,
    );
    // The SAME unique needle the strip half uses — a bare `.fm-cluster{` would be satisfied by the
    // ungated state rule and pass even with the theme block stripped, making this half tautological.
    assert!(
        // Shipped spelling: the stylesheet is whitespace-minified.
        clustered.contains(".fm-cluster{fill:var(--fm-cluster-fill)"),
        "a diagram WITH a subgraph lost its cluster rules"
    );

    let dotted = render("flowchart LR\n  A -.-> B\n", ThemePreset::Default);
    assert!(
        dotted.contains(".fm-edge-dashed{"),
        "a diagram WITH a dotted edge lost the rule that dashes it"
    );

    let thick = render("flowchart LR\n  A ==> B\n", ThemePreset::Default);
    assert!(
        thick.contains(".fm-edge-thick{"),
        "a diagram WITH a thick edge lost the rule that thickens it"
    );

    let cylinder = render("flowchart LR\n  A[(Store)] --> B\n", ThemePreset::Default);
    assert!(
        cylinder.contains(".fm-node-shape-cylinder"),
        "a diagram WITH a cylinder shape lost the rule that tints it"
    );
}
