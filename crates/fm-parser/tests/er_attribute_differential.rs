//! Differential test: ER entity attributes, against what mermaid-js ACTUALLY parses.
//!
//! THE DIVERGENCE THIS PINS (bd-nryyc). mermaid's ER attribute carries `keys` as a LIST. This
//! parser carried one key and split the line on whitespace, matching `parts[2..]` against the exact
//! strings `PK`/`FK`/`UK` — so the token `PK,` from `string a PK, FK` matched nothing and fell into
//! the COMMENT branch. Measured before the fix:
//!
//!     string a PK, FK       mermaid keys ['PK','FK']       we drew  `FK string a PK,`
//!     string b PK,FK        mermaid keys ['PK','FK']       we drew  `string b PK,FK`  (no key)
//!     string h UK, PK, FK   mermaid keys ['UK','PK','FK']  we drew  `FK string h UK,`
//!
//! A column that is both a primary and a foreign key is how a junction table is spelled, and ER is
//! the family that dominates the repo's worst certified incumbent ratio (`schema_catalog_25`), so
//! this was wrong output on the highest-traffic workload.
//!
//! THE ORACLE is `tests/fixtures/mermaid_er_attributes.tsv`, produced by
//! `scripts/headtohead/er_attribute_battery.mjs` from the pinned 11.15.0 bundle. Its `keys` column
//! is comma-joined because that is how mermaid DRAWS the cell — `attribute.keys.join()`, which is
//! `Array.prototype.join` with no separator argument. The gate compares tokens, so `PK,FK` being one
//! token rather than two is part of the contract, not cosmetics.
//!
//! TWO MEASURED FACTS THE FIXTURE RECORDS SO THEY ARE NOT REDISCOVERED:
//!   * `string m PK FK` — the space-separated form — is REJECTED by mermaid's grammar. We stay
//!     lenient and read it as two keys; that is this parser's recovery contract, and the row is in
//!     the fixture as `REJECTED` rather than being quietly left out.
//!   * `string d pk` keeps mermaid's lower case (`keys: ['pk']`). Our `IrAttributeKey` is an enum,
//!     so we normalise to `PK`. Keys are compared case-insensitively here for that reason, and it
//!     is a real if minor divergence rather than something this test is pretending away.

use std::{fs, path::Path};

struct Row {
    attribute: String,
    verdict: String,
    data_type: String,
    name: String,
    /// mermaid's `keys` array, comma-joined; empty when it has none.
    keys: String,
    comment: String,
}

fn fixture() -> Vec<Row> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mermaid_er_attributes.tsv");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("fixture {} unreadable: {err}", path.display()));
    let rows: Vec<Row> = text
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let mut columns = line.split('\t');
            Row {
                attribute: columns.next().expect("attribute column").to_string(),
                verdict: columns.next().expect("verdict column").to_string(),
                data_type: columns.next().unwrap_or("").to_string(),
                name: columns.next().unwrap_or("").to_string(),
                keys: columns.next().unwrap_or("").to_string(),
                comment: columns.next().unwrap_or("").to_string(),
            }
        })
        .collect();
    // Check the instrument before reading it: without composite rows this whole test is about a
    // case that already worked.
    assert!(rows.len() >= 15, "fixture holds only {} rows", rows.len());
    assert!(
        rows.iter().filter(|row| row.keys.contains(',')).count() >= 4,
        "fixture has no composite-key rows — it cannot pin bd-nryyc"
    );
    assert!(
        rows.iter()
            .filter(|row| row.keys.is_empty() && row.verdict == "PARSED")
            .count()
            >= 4,
        "fixture has no key-less rows — it cannot catch a key parser that fires too often"
    );
    rows
}

/// The one attribute this entity declares, as `(type, name, keys joined, comment)`.
fn parsed(attribute: &str) -> Option<(String, String, String, String)> {
    let source = format!("erDiagram\n A {{\n  {attribute}\n }}\n");
    let ir = fm_parser::parse(&source).ir;
    let member = ir.nodes.iter().find_map(|node| node.members.first())?;
    Some((
        member.data_type.clone(),
        member.name.clone(),
        member.key_prefix().trim_end().to_string(),
        member.comment.clone().unwrap_or_default(),
    ))
}

#[test]
fn every_attribute_parses_the_way_mermaid_parses_it() {
    let mut divergent = Vec::new();
    let mut compared = 0;
    for row in fixture() {
        if row.verdict != "PARSED" {
            continue;
        }
        compared += 1;
        let Some((data_type, name, keys, comment)) = parsed(&row.attribute) else {
            divergent.push(format!(
                "{:?}: we parsed no attribute at all",
                row.attribute
            ));
            continue;
        };
        // Keys case-insensitively; see the header note on `string d pk`.
        if data_type != row.data_type
            || name != row.name
            || !keys.eq_ignore_ascii_case(&row.keys)
            || comment != row.comment
        {
            divergent.push(format!(
                "{:?}: ours ({data_type:?}, {name:?}, {keys:?}, {comment:?}), mermaid ({:?}, {:?}, {:?}, {:?})",
                row.attribute, row.data_type, row.name, row.keys, row.comment
            ));
        }
    }
    assert!(
        compared >= 14,
        "only {compared} rows were actually compared"
    );
    assert!(
        divergent.is_empty(),
        "{} attribute(s) diverge from mermaid 11.15.0:\n  {}",
        divergent.len(),
        divergent.join("\n  ")
    );
}

/// THE FIXTURE HAS TO BE ABLE TO SAY NO. Both implementations this codebase could plausibly have
/// shipped — and the one it did ship, which kept the LAST key it recognised — must contradict it.
#[test]
fn the_fixture_rejects_a_single_key_implementation() {
    let rows = fixture();
    for (name, pick) in [("first-key-wins", 0_usize), ("last-key-wins", usize::MAX)] {
        let caught = rows
            .iter()
            .filter(|row| row.verdict == "PARSED" && !row.keys.is_empty())
            .filter(|row| {
                let keys: Vec<&str> = row.keys.split(',').collect();
                let chosen = if pick == 0 {
                    keys[0]
                } else {
                    keys[keys.len() - 1]
                };
                chosen != row.keys
            })
            .count();
        assert!(
            caught >= 4,
            "the {name} implementation contradicts only {caught} row(s) — this fixture cannot discriminate"
        );
    }
}

/// A COMMENT CONTAINING A COMMA IS STILL A COMMENT. The over-firing control for the key-list
/// parser: it accepts a token only when EVERY non-empty comma piece is a key, so `"one, two"` must
/// not be read as two keys named `one` and `two`.
#[test]
fn a_comma_inside_a_comment_is_not_a_key_list() {
    let (_, name, keys, comment) = parsed("string l \"one, two\"").expect("attribute parses");
    assert_eq!(name, "l");
    assert_eq!(keys, "", "a comment was read as keys");
    assert_eq!(comment, "one, two");
}

/// END TO END: the composite key reaches the drawn text as ONE token, in the key cell, with no
/// dangling comma left behind as a comment.
#[test]
fn a_rendered_entity_draws_every_key_in_one_token() {
    let source = "erDiagram\n A {\n  string a PK, FK\n  string b PK,FK\n }\n";
    let ir = fm_parser::parse(source).ir;
    let attributes = &ir
        .nodes
        .iter()
        .find(|node| !node.members.is_empty())
        .expect("entity")
        .members;
    assert_eq!(attributes.len(), 2);
    for attribute in attributes {
        assert_eq!(attribute.key_prefix(), "PK,FK ", "{:?}", attribute.name);
        assert_eq!(attribute.comment, None, "a key leaked into the comment");
    }
}
