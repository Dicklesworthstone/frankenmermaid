//! Differential test: what a gitGraph `commit` MEANS, checked against pinned mermaid 11.15.0.
//!
//! The fixture is generated, not written — `scripts/headtohead/gitgraph_commit_battery.mjs` asks the
//! pinned bundle's own diagram db for every row. Regenerate it there; do not hand-edit it here.
//!
//! Three divergences are pinned (bd-3cj8v), and each has a NEGATIVE CONTROL below stating exactly
//! which wrong implementation the assertion kills:
//!
//!   A  `tag:` is REPEATABLE. mermaid stores `tags: string[]` and draws one flag per entry; we held
//!      a single `Option<String>` and assigned, so the last clause won and every earlier tag left
//!      the output with no diagnostic.
//!   B  A node's label REPLACES its id at render time. Labelling a commit `[v1.0]` therefore made
//!      its id unreachable, where mermaid draws both.
//!   C  gitGraph maps `type: REVERSE` to a filled circle and `type: HIGHLIGHT` to a double circle,
//!      and those shapes carried a STATE-DIAGRAM id suppression, so reverted and highlighted
//!      commits rendered an empty `<text>`.

use fm_parser::parse;

const FIXTURE: &str = include_str!("fixtures/mermaid_gitgraph_commits.tsv");

struct Row {
    spec: String,
    id: String,
    message: String,
    tags: Vec<String>,
    commit_type: String,
}

/// The `tags` column is a JSON array so a tag containing a comma or a pipe survives the round trip.
/// Only string arrays ever appear in it, which is why this hand-rolled reader is enough.
fn parse_json_string_array(raw: &str) -> Vec<String> {
    let inner = raw
        .trim()
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or_else(|| panic!("tags column is not a JSON array: {raw}"));
    let mut out = Vec::new();
    let mut chars = inner.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            ' ' | ',' => {}
            '"' => {
                let mut value = String::new();
                loop {
                    match chars.next() {
                        Some('\\') => value.push(chars.next().expect("dangling escape in tags")),
                        Some('"') | None => break,
                        Some(other) => value.push(other),
                    }
                }
                out.push(value);
            }
            other => panic!("unexpected character {other:?} in tags column {raw}"),
        }
    }
    out
}

fn rows() -> Vec<Row> {
    let mut lines = FIXTURE.lines();
    let header = lines.next().expect("fixture is empty");
    assert_eq!(
        header, "spec\tverdict\tid\tmessage\ttags\ttype",
        "fixture schema moved; regenerate the test alongside the battery"
    );
    let rows: Vec<Row> = lines
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut columns = line.split('\t');
            let spec = columns.next().expect("spec column").to_string();
            let verdict = columns.next().expect("verdict column");
            assert_eq!(
                verdict, "PARSED",
                "the incumbent rejected `commit {spec}`; this battery only builds accepted clauses"
            );
            Row {
                spec,
                id: columns.next().expect("id column").to_string(),
                message: columns.next().expect("message column").to_string(),
                tags: parse_json_string_array(columns.next().expect("tags column")),
                commit_type: columns.next().expect("type column").to_string(),
            }
        })
        .collect();
    assert!(
        rows.len() >= 32,
        "fixture shrank to {} rows; a battery that lost its multi-tag cases proves nothing",
        rows.len()
    );
    rows
}

/// The text our engine actually DRAWS for the single commit in `gitGraph\ncommit <spec>`.
///
/// Deliberately routed through `MermaidDiagramIr::node_display_text` rather than reading
/// `node.label`: that function is the one shared rule fm-layout sizes from and fm-render-svg paints
/// from, so asserting on it is asserting on what reaches the SVG. Reading the label field instead
/// would have been blind to divergence C entirely — the label is `None` there and the whole bug was
/// in the fallback.
fn displayed_text(spec: &str) -> String {
    let source = format!("gitGraph\n    commit {spec}\n");
    let parsed = parse(&source);
    let node = parsed
        .ir
        .nodes
        .iter()
        .find(|node| node.id != "main")
        .unwrap_or_else(|| panic!("no commit node parsed from `commit {spec}`"));
    parsed.ir.node_display_text(node).to_string()
}

#[test]
fn every_tag_reaches_the_output() {
    let mut multi_tag_rows = 0;
    for row in rows() {
        let drawn = displayed_text(&row.spec);
        for tag in &row.tags {
            assert!(
                drawn.contains(tag.as_str()),
                "`commit {}`: mermaid keeps tags {:?}, but {tag:?} does not appear in what we draw \
                 ({drawn:?})",
                row.spec,
                row.tags,
            );
        }
        if row.tags.len() > 1 {
            multi_tag_rows += 1;
        }
    }
    // NEGATIVE CONTROL. An implementation that assigns `options.tag` instead of pushing keeps only
    // the LAST clause, so `v1.0` and `stable` vanish from every three-tag row and this fails 16
    // times. A one-tag-only battery would pass such an implementation outright, which is why the
    // count is asserted rather than assumed.
    assert!(
        multi_tag_rows >= 16,
        "only {multi_tag_rows} rows carry more than one tag; the last-tag-wins control is toothless"
    );
}

#[test]
fn tags_keep_the_order_they_were_written_in() {
    let mut checked = 0;
    for row in rows() {
        if row.tags.len() < 2 {
            continue;
        }
        let drawn = displayed_text(&row.spec);
        let mut cursor = 0usize;
        for tag in &row.tags {
            let found = drawn[cursor..].find(tag.as_str()).unwrap_or_else(|| {
                panic!(
                    "`commit {}`: tag {tag:?} is missing or out of order in {drawn:?} (mermaid \
                     order {:?})",
                    row.spec, row.tags
                )
            });
            cursor += found + tag.len();
        }
        checked += 1;
    }
    // NEGATIVE CONTROL. mermaid's renderer iterates `t.tags.reverse()`, which is a tempting thing to
    // mirror when porting; reversing the list here puts `lts` before `v1.0` and this fails. The
    // reversal is a drawing-order detail of a stack of flags, not the stored order — the db reports
    // `["v1.0","stable","lts"]` in source order.
    assert!(checked >= 16, "only {checked} rows could test ordering");
}

#[test]
fn a_commit_always_shows_its_id() {
    for row in rows() {
        let drawn = displayed_text(&row.spec);
        assert!(
            drawn.contains(row.id.as_str()),
            "`commit {}`: mermaid draws the commit id ({}) for every commit, we drew {drawn:?}",
            row.spec,
            row.id,
        );
    }
    // NEGATIVE CONTROL for B: an implementation whose label is `format!("[{tag}]")` replaces the id
    // rather than accompanying it, and fails all 16 tagged rows.
}

#[test]
fn reverse_and_highlight_commits_are_not_blank() {
    let mut typed_rows = 0;
    for row in rows() {
        if row.commit_type == "0" {
            continue;
        }
        typed_rows += 1;
        let drawn = displayed_text(&row.spec);
        assert!(
            !drawn.is_empty(),
            "`commit {}` is type {} and drew NOTHING; mermaid draws the commit label for every \
             commit type",
            row.spec,
            row.commit_type,
        );
        assert!(
            drawn.contains(row.id.as_str()),
            "`commit {}` is type {} and drew {drawn:?} without its id {}",
            row.spec,
            row.commit_type,
            row.id,
        );
    }
    // NEGATIVE CONTROL for C. Suppressing the id fallback by SHAPE — `FilledCircle | HorizontalBar
    // => None`, `DoubleCircle if label.is_none() => None` — is what shipped, and it blanks exactly
    // the untagged REVERSE and HIGHLIGHT rows. The tagged ones carry an explicit label and survive
    // it, so a battery without bare `type:` rows would have watched this bug go by.
    assert!(
        typed_rows >= 16,
        "only {typed_rows} REVERSE/HIGHLIGHT rows; the shape-suppression control is toothless"
    );
}

#[test]
fn a_message_is_not_confused_with_a_tag() {
    for row in rows() {
        if row.message.is_empty() {
            continue;
        }
        let drawn = displayed_text(&row.spec);
        assert!(
            drawn.contains(row.message.as_str()),
            "`commit {}`: mermaid stores message {:?}, we drew {drawn:?}",
            row.spec,
            row.message,
        );
        // The message is NOT bracketed; the brackets are the tag flag's notation. An implementation
        // that folds `msg:` into the tag list would wrap it and fail here.
        assert!(
            !drawn.contains(&format!("[{}]", row.message)),
            "`commit {}`: the message was drawn as if it were a tag ({drawn:?})",
            row.spec,
        );
    }
}
