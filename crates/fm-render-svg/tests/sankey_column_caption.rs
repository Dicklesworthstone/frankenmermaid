//! A sankey's columns are not a labelled axis.
//!
//! THE DIVERGENCE. `layout_diagram_sankey_traced` labelled every rank band with a name it INVENTED
//! from the rank index — `column 1`, `column 2`, `column 3`. Nothing the author wrote, and nothing
//! mermaid draws. A sankey's columns are a consequence of the flow graph, not an axis, so a
//! generated positional name is a machine-facing token dressed as a heading.
//!
//! Measured against the pinned 11.15.0 bundle in Chromium: the three `column N` runs were surplus on
//! our side with no counterpart on the incumbent's.
//!
//! ⚠️ THE BANDS STAY — they tint each rank. Only the generated caption goes. A fix that removed the
//! bands would satisfy the assertion below and change the diagram's appearance, which is not what was
//! measured.
//!
//! ⚠️ SAME FAMILY, THIRD TIME: the block-beta group id, the journey section repeat, and now this.
//! A display slot wanted a value and something internal was nearest to hand.
//!
//! ⚠️ WHAT THIS DELIBERATELY DOES NOT TOUCH. We also draw each flow's VALUE as an edge label
//! (`100`, `75`, …) where mermaid draws none — mermaid encodes magnitude only as link WIDTH, and so
//! do we (a flow's stroke width is its value). So the number is redundant against our own width
//! encoding and absent from the incumbent. It is nonetheless real information the author supplied,
//! unlike a generated column index, so removing it is a product decision rather than a parity fix
//! and is left to be asked rather than assumed. The test below pins only the generated caption.

fn runs(source: &str) -> Vec<String> {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(source).ir);
    let mut out = Vec::new();
    let mut rest = svg.as_str();
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        let Some(open) = rest.find('>') else { break };
        let Some(close) = rest.find("</text>") else {
            break;
        };
        let body = &rest[open + 1..close];
        let mut stripped = String::new();
        let mut in_tag = false;
        for ch in body.chars() {
            match ch {
                '<' => in_tag = true,
                '>' if in_tag => in_tag = false,
                _ if !in_tag => stripped.push(ch),
                _ => {}
            }
        }
        let text = stripped
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&");
        let text = text.trim().to_string();
        if !text.is_empty() {
            out.push(text);
        }
        rest = &rest[close + "</text>".len()..];
    }
    out
}

const SANKEY: &str =
    "sankey-beta\n\nSource A,Process X,100\nSource A,Process Y,50\nProcess X,Output 1,120\n";

/// ⚠️ THE NEGATIVE CONTROL, and the defect as it shipped.
#[test]
fn a_sankey_draws_no_generated_column_caption() {
    let drawn = runs(SANKEY);
    assert!(
        !drawn.iter().any(|run| run.starts_with("column ")),
        "a generated column caption reached the drawing: {drawn:?}"
    );
}

/// NON-VACUITY: the node labels — which mermaid DOES draw, carrying each node's total — must still
/// be there. Suppressing all text would satisfy the assertion above.
#[test]
fn the_sankey_node_labels_survive() {
    let drawn = runs(SANKEY);
    for node in ["Source A", "Process X", "Output 1"] {
        assert!(
            drawn.iter().any(|run| run.contains(node)),
            "the node label {node:?} went with the column caption: {drawn:?}"
        );
    }
}

/// ⚠️ THE BANDS SURVIVE. Removing the rank bands along with their captions would change how the
/// diagram is tinted, which is a different change from the one measured.
#[test]
fn the_sankey_rank_bands_are_still_drawn() {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(SANKEY).ir);
    assert!(
        svg.contains("fm-band-column"),
        "the rank bands were removed along with their generated captions"
    );
}

/// CONTROL: a diagram whose band labels ARE author text keeps them. gitgraph's lane band label is
/// the only carrier of its branch name, so a fix that emptied every band label would delete
/// information rather than an invented caption.
#[test]
fn author_written_band_labels_are_untouched() {
    let git = "gitGraph\n  commit\n  branch develop\n  checkout develop\n  commit\n";
    let drawn = runs(git);
    for branch in ["main", "develop"] {
        assert!(
            drawn.iter().any(|run| run == branch),
            "the gitgraph branch label {branch:?} was emptied with the sankey captions: {drawn:?}"
        );
    }
}
