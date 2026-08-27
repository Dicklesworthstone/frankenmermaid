//! A block-beta group's IDENTIFIER is not a caption.
//!
//! THE DIVERGENCE. `block:header:3` declares a container with the id `header` spanning three
//! columns. There is no label syntax for a group — mermaid's grammar has none, and our own
//! `BlockBetaDocumentItem::Group` carries no label field, because there is nothing to carry. We
//! passed the id as the cluster title anyway, so the container was captioned with its own
//! identifier and `header` / `footer` appeared in the drawing. mermaid draws no caption there.
//!
//! Measured against the pinned 11.15.0 bundle in Chromium: incumbent 4 drawn runs against our 6, the
//! surplus being exactly `header` and `footer`. block_basic now reports AGREE, 4 runs.
//!
//! ⚠️ SAME FAMILY AS TWO EARLIER FIXES — the requirement `<<element>>` header and the journey actor
//! legend. A display slot wanted a value, the identifier was the nearest one to hand, and a
//! machine-facing token reached a reader. It is worth naming because the fix is always the same: the
//! slot should be EMPTY, not filled with the closest available string.

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

const BLOCKS: &str = "block-beta\n  columns 3\n\n  block:header:3\n    A[\"Header spans 3 columns\"]\n  end\n\n  B[\"Left\"]\n  space\n  C[\"Right\"]\n";

/// ⚠️ THE NEGATIVE CONTROL, and the defect exactly as it shipped.
#[test]
fn a_block_group_identifier_is_never_drawn() {
    let drawn = runs(BLOCKS);
    assert!(
        !drawn.iter().any(|run| run == "header"),
        "the group's identifier reached the drawing as a caption: {drawn:?}"
    );
}

/// NON-VACUITY: the blocks the group contains must still be drawn. Suppressing the whole group would
/// satisfy the assertion above and lose the content.
#[test]
fn the_blocks_inside_the_group_survive() {
    let drawn = runs(BLOCKS);
    for expected in ["Header spans 3 columns", "Left", "Right"] {
        assert!(
            drawn.iter().any(|run| run == expected),
            "{expected:?} went missing with the caption: {drawn:?}"
        );
    }
}

/// ⚠️ THE CONTROL THAT SCOPES IT. A flowchart subgraph DOES have a label syntax, and its caption is
/// the author's text — it must keep it. A fix that stopped captioning every cluster would pass the
/// first test and silently strip real titles.
#[test]
fn a_flowchart_subgraph_keeps_its_declared_title() {
    let flow = "flowchart TD\n  subgraph Backend[\"Backend services\"]\n    a --> b\n  end\n";
    let drawn = runs(flow);
    assert!(
        drawn.iter().any(|run| run == "Backend services"),
        "a declared subgraph title was suppressed: {drawn:?}"
    );
}

/// CONTROL: a subgraph declared with only an id still shows that id, because in a FLOWCHART the bare
/// form `subgraph Backend` is how an author names the group — there the identifier IS the label.
/// The block-beta case is different precisely because its grammar offers no label at all.
#[test]
fn a_bare_flowchart_subgraph_still_shows_its_name() {
    let flow = "flowchart TD\n  subgraph Backend\n    a --> b\n  end\n";
    let drawn = runs(flow);
    assert!(
        drawn.iter().any(|run| run == "Backend"),
        "the bare subgraph name was suppressed along with the block-beta ids: {drawn:?}"
    );
}
