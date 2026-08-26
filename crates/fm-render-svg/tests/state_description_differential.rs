//! Differential test: state-diagram descriptions, against what mermaid-js ACTUALLY records.
//!
//! THE DIVERGENCE THIS PINS (bd-xm62h). `s1 : text` is mermaid's documented state-description
//! syntax. This parser had no branch for it: a line with no edge operator fell through to the
//! node-token parser, so the WHOLE line became the state's label and the box was drawn captioned
//! `s1 : text` — the author's own id and colon, text the incumbent never draws. Worse, a second
//! description line for the same state was SILENTLY DROPPED, because `intern_node` fills a label
//! only when the node has none. Measured before the fix:
//!
//!   s1: no space before colon   ->  mermaid `no space before colon`, ours `s1: no space before colon`
//!   s3 : first / s3 : second    ->  mermaid `first` + `second`,      ours `s3 : first`
//!
//! THE ORACLE is `tests/fixtures/mermaid_state_descriptions.tsv`, produced by
//! `scripts/headtohead/state_description_battery.mjs` from the pinned 11.15.0 bundle by reading its
//! diagram db — not its error strings, and not its documentation.
//!
//! WHAT IS ASSERTED, and why each half is needed:
//!
//! 1. STATES WITH DESCRIPTIONS. Our label must equal the incumbent's `descriptions` joined with a
//!    newline, which is what `fm_render_svg::wrap_node_label_lines` draws as separate lines.
//! 2. STATES WITHOUT. Our label must be ABSENT. Half of this defect class is drawing a description
//!    the incumbent does not have, so the fixture carries every state the incumbent built — the
//!    `A:::bad` shorthand, a `C --> D: edge label` transition, a `note right of A: hello`, a
//!    `classDef bad fill:#f00` — each of which puts a top-level colon in front of a splitter that
//!    fires too eagerly.
//! 3. THAT THE FIXTURE CAN SAY NO. Three implementations this codebase could plausibly have
//!    shipped — first-description-wins, last-description-wins, and a splitter with no `:::` guard —
//!    are each checked to DISAGREE with the fixture on at least one row. A differential test a
//!    wrong implementation also passes is not evidence.
//!
//! NOT ASSERTED: mermaid's `root_start`/`root_end` pseudo-state names (we scope and spell ours
//! differently, which is its own contract), and state id CASE — the fixture is matched
//! case-insensitively because `normalize_identifier` lowercases, a separate pre-existing behaviour.

use std::{fs, path::Path};

struct Row {
    case: String,
    diagram: String,
    state: String,
    /// The incumbent's `descriptions` array; empty when it recorded none.
    descriptions: Vec<String>,
}

fn fixture() -> Vec<Row> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mermaid_state_descriptions.tsv");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("fixture {} unreadable: {err}", path.display()));
    let rows: Vec<Row> = text
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let mut columns = line.split('\t');
            let case = columns.next().expect("case column").to_string();
            let diagram = columns.next().expect("diagram column").replace("\\n", "\n");
            let state = columns.next().expect("state column").to_string();
            let raw = columns.next().unwrap_or("");
            let descriptions = if raw.is_empty() {
                Vec::new()
            } else {
                raw.split('|').map(str::to_string).collect()
            };
            Row { case, diagram, state, descriptions }
        })
        .collect();
    // Check the instrument before reading it: a fixture that regenerated empty, or with only
    // described states, would make one whole half of the assertion vacuous.
    assert!(rows.len() >= 20, "fixture holds only {} rows", rows.len());
    assert!(
        rows.iter().filter(|row| !row.descriptions.is_empty()).count() >= 8,
        "fixture has no described states"
    );
    assert!(
        rows.iter().filter(|row| row.descriptions.is_empty()).count() >= 8,
        "fixture has no undescribed states — it cannot catch a splitter that fires too often"
    );
    assert!(
        rows.iter().filter(|row| row.descriptions.len() > 1).count() >= 3,
        "fixture has no multi-description state — it cannot catch a first-wins implementation"
    );
    rows
}

/// The label text our parser attached to `state`, matched case-insensitively (see the note above).
fn our_label(diagram: &str, state: &str) -> Option<String> {
    let ir = fm_parser::parse(diagram).ir;
    let node = ir
        .nodes
        .iter()
        .find(|node| node.id.eq_ignore_ascii_case(state))
        .unwrap_or_else(|| {
            panic!(
                "state {state:?} is absent from our IR; we built {:?}",
                ir.nodes.iter().map(|node| node.id.as_str()).collect::<Vec<_>>()
            )
        });
    node.label
        .and_then(|label_id| ir.labels.get(label_id.0))
        .map(|label| label.text.clone())
}

#[test]
fn every_state_carries_the_description_mermaid_recorded() {
    let mut divergent = Vec::new();
    for row in fixture() {
        let ours = our_label(&row.diagram, &row.state);
        let expected = if row.descriptions.is_empty() {
            None
        } else {
            Some(row.descriptions.join("\n"))
        };
        if ours != expected {
            divergent.push(format!(
                "{}/{}: ours {:?}, mermaid {:?}",
                row.case, row.state, ours, expected
            ));
        }
    }
    assert!(
        divergent.is_empty(),
        "{} state(s) diverge from mermaid 11.15.0:\n  {}",
        divergent.len(),
        divergent.join("\n  ")
    );
}

/// THE FIXTURE HAS TO BE ABLE TO SAY NO. A gate never observed to fail is not evidence.
///
/// Each closure is a label a plausible-but-wrong implementation would produce for a state, given
/// the incumbent's own description list. Every one must contradict the fixture somewhere.
#[test]
fn the_fixture_rejects_the_plausible_wrong_implementations() {
    let rows = fixture();
    /// The label a candidate implementation would attach, given the incumbent's own list.
    type WrongLabel = fn(&Row) -> Option<String>;
    let wrong: [(&str, WrongLabel); 3] = [
        // What `intern_node`'s first-writer-wins does: keep description one, drop the rest.
        ("first-description-wins", |row| row.descriptions.first().cloned()),
        // The obvious "fix" that overwrites instead of appending.
        ("last-description-wins", |row| row.descriptions.last().cloned()),
        // A splitter with no `:::` guard: `A:::bad` becomes a description `::bad` on `A`.
        ("no-triple-colon-guard", |row| {
            if row.case == "class_shorthand_is_not_a_description" && row.state == "A" {
                Some("::bad".to_string())
            } else if row.descriptions.is_empty() {
                None
            } else {
                Some(row.descriptions.join("\n"))
            }
        }),
    ];

    for (name, produce) in wrong {
        let caught = rows
            .iter()
            .filter(|row| {
                let expected = if row.descriptions.is_empty() {
                    None
                } else {
                    Some(row.descriptions.join("\n"))
                };
                produce(row) != expected
            })
            .count();
        assert!(
            caught > 0,
            "the {name} implementation passes every fixture row — this fixture cannot discriminate"
        );
    }
}

/// END TO END: the description reaches the drawn text, and the id and colon do not.
#[test]
fn a_rendered_state_box_draws_the_description_and_not_the_source_line() {
    let source = "stateDiagram-v2\ns1 : first\ns1 : second\n[*] --> s1\n";
    let ir = fm_parser::parse(source).ir;
    let svg = fm_render_svg::render_svg(&ir);

    // A multi-line label is drawn as ONE `<text>` holding a `<tspan>` per line, so the runs have to
    // be read as LEAF text: taking each element's inner markup whole would compare the description
    // against a string full of `<tspan …>` and never match. Leaf segments only, so nesting cannot
    // double-count.
    let mut runs = Vec::new();
    let mut rest = svg.as_str();
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        let Some(open_end) = rest.find('>') else { break };
        rest = &rest[open_end + 1..];
        let Some(close) = rest.find("</text>") else { break };
        let inner = &rest[..close];
        let mut cursor = inner;
        while let Some(open) = cursor.find('<') {
            let leaf = &cursor[..open];
            if !leaf.is_empty() {
                runs.push(leaf.to_string());
            }
            cursor = &cursor[open..];
            let Some(tag_end) = cursor.find('>') else { break };
            cursor = &cursor[tag_end + 1..];
        }
        if !cursor.is_empty() {
            runs.push(cursor.to_string());
        }
        rest = &rest[close + "</text>".len()..];
    }
    let runs: Vec<String> = runs
        .into_iter()
        .map(|run| {
            run.replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&amp;", "&")
        })
        .filter(|run| !run.trim().is_empty())
        .collect();

    for expected in ["first", "second"] {
        assert!(
            runs.iter().any(|run| run == expected),
            "no <text> run drew {expected:?}; runs were {runs:?}"
        );
    }
    assert!(
        !runs.iter().any(|run| run.contains("s1 :")),
        "the source line is still being drawn as a caption: {runs:?}"
    );
}
