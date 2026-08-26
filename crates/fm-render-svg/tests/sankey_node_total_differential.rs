//! Differential test: the throughput mermaid prints under each sankey node.
//!
//! THE DIVERGENCE THIS PINS. mermaid labels a sankey node with its name AND its throughput on a
//! second line — `Source A` / `150`. We drew the name alone. Found by
//! `scripts/headtohead/drawn_text_diff.mjs`, which renders both engines and diffs the drawn
//! `<text>`: mermaid drew `["Source A\n150","Process X\n175",…]` that we did not.
//!
//! ⚠️ THE TOTAL IS max(INFLOW, OUTFLOW) — NOT THE SUM, AND NOT EITHER SIDE ALONE. Measured on the
//! pinned 11.15.0 bundle with a node that does not conserve flow:
//!
//! ```text
//!   sankey-beta
//!   A,M,10        M has 10 in and 3 out
//!   M,B,3         mermaid draws `M` / `10`
//! ```
//!
//! sum would be 13, outflow alone 3, inflow alone 10. Only an UNBALANCED node separates the three
//! rules; every balanced diagram makes them agree, which is why one is carried below. The
//! project's own `sankey_basic` fixture is fully balanced and cannot tell them apart.
//!
//! Formatting is plain decimal with no padding: `A,B,1.5` + `A,C,2.25` gives `A` / `3.75`.
//!
//! NOT ASSERTED HERE: the column headers and per-link values we draw and mermaid does not. Those
//! are text we ADD, measured in the same sweep; this file is about content that was missing.

/// The lines of every `<text>`, with a `<tspan>` boundary treated as the line break it is.
fn text_blocks(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        let Some(open) = rest.find('>') else { break };
        let Some(close) = rest.find("</text>") else {
            break;
        };
        let inner = &rest[open + 1..close];
        // A multi-line label is one `<text>` holding a `<tspan>` per line, so the boundary IS a
        // newline. Joining without one turns `A` + `10` into `A10`.
        let mut text = String::new();
        let mut cursor = inner;
        let mut first = true;
        while let Some(tag) = cursor.find('<') {
            let leaf = &cursor[..tag];
            if !leaf.trim().is_empty() {
                if !first {
                    text.push('\n');
                }
                text.push_str(leaf.trim());
                first = false;
            }
            let Some(end) = cursor[tag..].find('>') else {
                break;
            };
            cursor = &cursor[tag + end + 1..];
        }
        if !cursor.trim().is_empty() {
            if !first {
                text.push('\n');
            }
            text.push_str(cursor.trim());
        }
        if !text.is_empty() {
            out.push(text);
        }
        rest = &rest[close + "</text>".len()..];
    }
    out
}

fn blocks_for(source: &str) -> Vec<String> {
    text_blocks(&fm_render_svg::render_svg(&fm_parser::parse(source).ir))
}

/// The unbalanced case that separates max from sum, measured against mermaid 11.15.0.
const UNBALANCED: &str = "sankey-beta\n\nA,M,10\nM,B,3\n";

#[test]
fn a_node_is_labelled_with_its_throughput() {
    let blocks = blocks_for("sankey-beta\n\nSource A,Process X,100\nSource A,Process Y,50\n");
    for expected in ["Source A\n150", "Process X\n100", "Process Y\n50"] {
        assert!(
            blocks.iter().any(|block| block == expected),
            "expected the node block {expected:?}; drew {blocks:?}"
        );
    }
}

/// ⚠️ THE NEGATIVE CONTROL. A `sum` implementation labels M with 13 and an `outflow` one with 3;
/// mermaid labels it 10. Every balanced diagram agrees under all three rules, so without an
/// unbalanced node this whole file would pass on any of them.
#[test]
fn the_total_is_the_max_of_inflow_and_outflow() {
    let blocks = blocks_for(UNBALANCED);
    assert!(
        blocks.iter().any(|block| block == "M\n10"),
        "M has 10 in and 3 out; mermaid labels it 10 (sum would be 13, outflow 3). Drew {blocks:?}"
    );
    assert!(
        !blocks.iter().any(|block| block == "M\n13"),
        "the node total is the SUM of both directions, not the max: {blocks:?}"
    );
    assert!(
        !blocks.iter().any(|block| block == "M\n3"),
        "the node total is the OUTFLOW alone, not the max: {blocks:?}"
    );
}

/// A fractional total keeps its digits and gains no padding.
#[test]
fn a_fractional_total_is_printed_plainly() {
    let blocks = blocks_for("sankey-beta\n\nA,B,1.5\nA,C,2.25\n");
    assert!(
        blocks.iter().any(|block| block == "A\n3.75"),
        "expected `A` / `3.75`; drew {blocks:?}"
    );
    for wrong in ["A\n3.8", "A\n4", "A\n3.750000"] {
        assert!(
            !blocks.iter().any(|block| block == wrong),
            "the total was reformatted as {wrong:?}: {blocks:?}"
        );
    }
}

/// CONTROL: the second line must be the TOTAL, not merely present. A node whose name already ends
/// in digits would satisfy a naive "contains a number" assertion.
#[test]
fn the_second_line_is_the_total_and_not_part_of_the_name() {
    let blocks = blocks_for("sankey-beta\n\nNode1,Node2,7\n");
    assert!(
        blocks.iter().any(|block| block == "Node1\n7"),
        "expected `Node1` / `7`; drew {blocks:?}"
    );
}

/// CONTROL: no other diagram type gains a second line. The helper is gated on the diagram type, and
/// a gate that never rejects would put a stray total under every flowchart node.
#[test]
fn only_sankey_nodes_gain_a_total() {
    let blocks = blocks_for("flowchart LR\n  A-->B\n");
    for block in &blocks {
        assert!(
            !block.contains('\n'),
            "a flowchart node gained a second line: {block:?}"
        );
    }
}
