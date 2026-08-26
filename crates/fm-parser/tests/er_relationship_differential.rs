//! Differential test: ER relationship cardinality, against what mermaid-js ACTUALLY stores.
//!
//! THIS ONE PINS A CORRECT BEHAVIOUR AGAINST A LIVE TRAP, rather than repairing a defect. Probing
//! the incumbent for `bd-nryyc`'s neighbouring surface turned up no divergence here — all 32
//! cross-product spellings parse in both engines and we place both labels on the right ends — but
//! it turned up the reason a future change would break it:
//!
//! ⚠️ **mermaid STORES THE TWO CARDINALITIES CROSSED OVER.** Its grammar builds the relationship
//! spec as `{ cardA: <RIGHT marker>, relType, cardB: <LEFT marker> }`. Measured on `A ||..o| B`,
//! whose left marker is `||` and right marker is `o|`, it reports `cardA=ZERO_OR_ONE,
//! cardB=ONLY_ONE` — the RIGHT marker under `cardA`. Anyone wiring our IR to mermaid's field names,
//! or hand-writing a table from what the syntax looks like, gets every ASYMMETRIC relationship
//! backwards while every symmetric one keeps passing.
//!
//! `fm_core::parse_er_cardinality` avoids it by construction: it splits the notation on its
//! connector and maps each marker to the end it is written on, never consulting a `cardA`. This
//! test is what stops that being "simplified" into agreement with the incumbent's field names.
//!
//! THE ORACLE is `tests/fixtures/mermaid_er_relationships.tsv`, produced by
//! `scripts/headtohead/er_relationship_battery.mjs` from the pinned 11.15.0 bundle — the cross
//! product of left marker × body × right marker, because an asymmetric combination is the only kind
//! that can catch a swap.
//!
//! NOT ASSERTED: identifying vs non-identifying (`--` against `..`). mermaid carries it as
//! `relType`; we keep the raw notation and the renderer dashes the line from it, so there is no
//! comparable field to check and a row asserting one would be asserting against ourselves.

use std::{fs, path::Path};

struct Row {
    token: String,
    verdict: String,
    /// mermaid's `cardA` — which holds the RIGHT-hand marker. See the header.
    card_a: String,
    /// mermaid's `cardB` — which holds the LEFT-hand marker.
    card_b: String,
}

fn fixture() -> Vec<Row> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mermaid_er_relationships.tsv");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("fixture {} unreadable: {err}", path.display()));
    let rows: Vec<Row> = text
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let mut columns = line.split('\t');
            Row {
                token: columns.next().expect("token column").to_string(),
                verdict: columns.next().expect("verdict column").to_string(),
                card_a: columns.next().unwrap_or("").to_string(),
                card_b: columns.next().unwrap_or("").to_string(),
            }
        })
        .collect();
    assert!(rows.len() >= 30, "fixture holds only {} rows", rows.len());
    // Symmetric rows cannot catch a swap. The fixture is only an instrument if most of it is
    // asymmetric.
    assert!(
        rows.iter()
            .filter(|row| row.verdict == "PARSED" && row.card_a != row.card_b)
            .count()
            >= 20,
        "fixture is mostly symmetric — it cannot catch a crossed mapping"
    );
    rows
}

/// mermaid's cardinality enum in this renderer's spelling.
fn our_spelling(mermaid: &str) -> &'static str {
    match mermaid {
        "ONLY_ONE" => "1",
        "ZERO_OR_ONE" => "0..1",
        "ZERO_OR_MORE" => "0..*",
        "ONE_OR_MORE" => "1..*",
        other => panic!("unmapped mermaid cardinality {other:?}"),
    }
}

#[test]
fn every_spelling_puts_each_cardinality_on_the_end_mermaid_puts_it() {
    let mut divergent = Vec::new();
    let mut compared = 0;
    for row in fixture() {
        if row.verdict != "PARSED" {
            continue;
        }
        compared += 1;
        let (left, right) = fm_core::parse_er_cardinality(&row.token);
        // THE SWAP IS HERE AND IT IS DELIBERATE: mermaid's `cardB` is the LEFT marker and its
        // `cardA` is the RIGHT one.
        let (expected_left, expected_right) = (our_spelling(&row.card_b), our_spelling(&row.card_a));
        if (left, right) != (expected_left, expected_right) {
            divergent.push(format!(
                "{:?}: ours ({left:?}, {right:?}), mermaid ({expected_left:?}, {expected_right:?})",
                row.token
            ));
        }
    }
    assert!(compared >= 30, "only {compared} spellings were compared");
    assert!(
        divergent.is_empty(),
        "{} spelling(s) diverge from mermaid 11.15.0:\n  {}",
        divergent.len(),
        divergent.join("\n  ")
    );
}

/// THE FIXTURE HAS TO BE ABLE TO SAY NO. Reading mermaid's `cardA` as the LEFT end — the mapping
/// anyone gets from the field names, and the one this test exists to prevent — must contradict it.
#[test]
fn the_fixture_rejects_the_uncrossed_mapping() {
    let caught = fixture()
        .into_iter()
        .filter(|row| row.verdict == "PARSED")
        .filter(|row| {
            let (left, right) = fm_core::parse_er_cardinality(&row.token);
            // The wrong wiring: cardA to the left, cardB to the right.
            (left, right) != (our_spelling(&row.card_a), our_spelling(&row.card_b))
        })
        .count();
    assert!(
        caught >= 20,
        "the uncrossed mapping contradicts only {caught} row(s) — this fixture cannot discriminate"
    );
}

/// END TO END on the documented spelling, so the contract is not only about a helper: `||--o{`
/// means exactly one on the left and zero-or-more on the right.
#[test]
fn the_documented_one_to_many_spelling_reads_left_to_right() {
    assert_eq!(fm_core::parse_er_cardinality("||--o{"), ("1", "0..*"));
    assert_eq!(fm_core::parse_er_cardinality("}o--||"), ("0..*", "1"));
    assert_eq!(fm_core::parse_er_cardinality("}|..|{"), ("1..*", "1..*"));
}
