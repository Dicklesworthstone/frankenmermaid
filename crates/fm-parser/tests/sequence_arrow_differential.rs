//! Differential test: every sequence-message arrow spelling, against what mermaid-js ACTUALLY does.
//!
//! THE DIVERGENCE THIS PINS (bd-u4jiy). `SEQUENCE_OPERATORS` is a table of arrow SPELLINGS and the
//! dash run in front of the head marker is unbounded, so `find_operator_core` — which scans left to
//! right — slid past the head of a long run and matched its TAIL:
//!
//!     A --->> B: m    ->  actors ["A_-", "B"]      A ---->> B: m  ->  actors ["A_--", "B"]
//!     A ---> B: m     ->  actors ["A_-", "B"]      A--->>B: m     ->  actors ["A-", "B"]
//!
//! The leftover dashes stayed on the SOURCE, where `normalize_identifier` made them a participant.
//! That is not a mis-styled message, it is a MIS-WIRED one: the author's single lifeline silently
//! became two and the message hung off the phantom. Exact same failure as bd-lrl48 / bd-92b6 on the
//! flowchart side (`A o=== B` building a node `A_o`), which that side fixed and this one had not.
//!
//! THE ORACLE is `tests/fixtures/mermaid_sequence_arrows.tsv`, produced by
//! `scripts/headtohead/sequence_arrow_battery.mjs` from the pinned 11.15.0 bundle — a CROSS PRODUCT
//! of head/body/tail, not a list of arrows anyone writes, because the combinations nobody writes are
//! where a spelling table loses a byte.
//!
//! WHAT IS ASSERTED, in two halves that catch different failures:
//!
//! 1. THE TEN SPELLINGS MERMAID ACCEPTS. Our `ArrowType` must name the same cell of mermaid's
//!    LINETYPE table, and the endpoints must be `A` and `B`. This is the half that catches a
//!    message drawn solid where mermaid draws it dotted, or with an arrowhead where mermaid draws
//!    an open line — every count still agreeing while the picture says something else.
//! 2. THE TEN IT REJECTS. We stay lenient and still build a message — recovery is this parser's
//!    contract, and the flowchart tokens mermaid refuses are already documented that way — but the
//!    ACTOR SET must be exactly the two the author named. Refusing the line is how mermaid avoids
//!    inventing a participant; recovering must not be how we start.
//!
//! NOT ASSERTED: that we reject what mermaid rejects. That lenience predates this test and is a
//! decision, not an accident; saying so here keeps the silence from reading as coverage. Also not
//! asserted: which `ArrowType` a rejected spelling gets — mermaid has no answer to compare against,
//! so a claim there would be a claim about ourselves.

use std::{collections::BTreeSet, fs, path::Path};

use fm_core::ArrowType;

struct Row {
    token: String,
    verdict: String,
    /// mermaid's own LINETYPE name for the message type, empty unless PARSED.
    name: String,
}

fn fixture() -> Vec<Row> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mermaid_sequence_arrows.tsv");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("fixture {} unreadable: {err}", path.display()));
    let rows: Vec<Row> = text
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let mut columns = line.split('\t');
            let token = columns.next().expect("token column").to_string();
            let verdict = columns.next().expect("verdict column").to_string();
            let _type_code = columns.next().unwrap_or("");
            let name = columns.next().unwrap_or("").to_string();
            Row {
                token,
                verdict,
                name,
            }
        })
        .collect();
    // Check the instrument before reading it. A fixture that regenerated all-REJECTED (a bundle that
    // failed to load would do exactly that) would make half these assertions vacuous.
    let parsed = rows.iter().filter(|row| row.verdict == "PARSED").count();
    let rejected = rows.iter().filter(|row| row.verdict == "REJECTED").count();
    assert!(
        parsed >= 10,
        "fixture holds only {parsed} accepted spellings"
    );
    assert!(
        rejected >= 8,
        "fixture holds only {rejected} rejected spellings"
    );
    rows
}

/// The cell of mermaid's LINETYPE table each `ArrowType` a sequence message can carry occupies.
///
/// Written in this direction — ours to mermaid's — on purpose, exactly as
/// `flowchart_link_differential` does: it makes an `ArrowType` that names no mermaid cell impossible
/// to add silently, and it is the direction the assertion reads in.
fn mermaid_line_type(arrow: ArrowType) -> Option<&'static str> {
    Some(match arrow {
        ArrowType::Line => "SOLID_OPEN",
        ArrowType::DottedLine => "DOTTED_OPEN",
        ArrowType::Arrow => "SOLID",
        ArrowType::DottedArrow => "DOTTED",
        ArrowType::Cross => "SOLID_CROSS",
        ArrowType::DottedCross => "DOTTED_CROSS",
        ArrowType::OpenArrow => "SOLID_POINT",
        ArrowType::DottedOpenArrow => "DOTTED_POINT",
        ArrowType::DoubleArrow => "BIDIRECTIONAL_SOLID",
        ArrowType::DoubleDottedArrow => "BIDIRECTIONAL_DOTTED",
        _ => return None,
    })
}

/// `(actor ids, the one message's arrow)` for `A <token> B: m`.
fn parse_message(token: &str) -> (Vec<String>, Option<ArrowType>) {
    let source = format!("sequenceDiagram\nA {token} B: m\n");
    let ir = fm_parser::parse(&source).ir;
    let actors = ir.nodes.iter().map(|node| node.id.clone()).collect();
    let arrow = (ir.edges.len() == 1).then(|| ir.edges[0].arrow);
    (actors, arrow)
}

#[test]
fn accepted_spellings_mean_what_mermaid_means() {
    let mut divergent = Vec::new();
    for row in fixture().into_iter().filter(|row| row.verdict == "PARSED") {
        let (actors, arrow) = parse_message(&row.token);
        let Some(arrow) = arrow else {
            divergent.push(format!("{:?}: we built no single message", row.token));
            continue;
        };
        match mermaid_line_type(arrow) {
            Some(name) if name == row.name => {}
            Some(name) => divergent.push(format!(
                "{:?}: ours {arrow:?} = {name}, mermaid {}",
                row.token, row.name
            )),
            None => divergent.push(format!(
                "{:?}: ours {arrow:?} names no mermaid LINETYPE cell (mermaid says {})",
                row.token, row.name
            )),
        }
        if actors != ["A", "B"] {
            divergent.push(format!(
                "{:?}: endpoints {actors:?}, mermaid A/B",
                row.token
            ));
        }
    }
    assert!(
        divergent.is_empty(),
        "{} spelling(s) diverge from mermaid 11.15.0:\n  {}",
        divergent.len(),
        divergent.join("\n  ")
    );
}

#[test]
fn a_spelling_mermaid_rejects_still_never_invents_a_participant() {
    let expected: BTreeSet<String> = ["A".to_string(), "B".to_string()].into_iter().collect();
    let mut phantoms = Vec::new();
    let mut checked = 0;
    for row in fixture()
        .into_iter()
        .filter(|row| row.verdict == "REJECTED")
    {
        checked += 1;
        let (actors, _) = parse_message(&row.token);
        let actors: BTreeSet<String> = actors.into_iter().collect();
        if actors != expected {
            phantoms.push(format!("{:?}: actors {actors:?}", row.token));
        }
    }
    // WORK PROOF: every assertion above is vacuous if the loop never ran.
    assert!(
        checked >= 8,
        "only {checked} rejected spellings were checked"
    );
    assert!(
        phantoms.is_empty(),
        "{} spelling(s) invented a participant the author never wrote:\n  {}",
        phantoms.len(),
        phantoms.join("\n  ")
    );
}

/// The run is UNBOUNDED, so the fixture's longest spelling is not the bound. These go past it, and
/// past anything a table could hold, in both the spaced and the unspaced form.
#[test]
fn an_arbitrarily_long_dash_run_still_resolves_to_the_two_named_actors() {
    for token in [
        "----->",
        "------>>",
        "-------x",
        "--------)",
        "------------>>",
    ] {
        let (actors, arrow) = parse_message(token);
        assert_eq!(actors, ["A", "B"], "{token:?} built {actors:?}");
        assert!(arrow.is_some(), "{token:?} built no message");
    }
    // No separator at all: the dashes are adjacent to the actor names on both sides.
    let ir = fm_parser::parse("sequenceDiagram\nA----->>B: m\n").ir;
    assert_eq!(
        ir.nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        ["A", "B"],
        "the unspaced form still leaks dashes onto an actor"
    );
}

/// AN ACTOR MAY LEGITIMATELY BE NAMED WITH A DASH, and the extension must not eat it. This is the
/// over-firing control for the fix: the walk stops at the first non-dash byte, so a separator or a
/// letter ends it.
#[test]
fn a_dash_in_an_actor_name_survives() {
    let ir = fm_parser::parse("sequenceDiagram\nmy-actor ->> other-actor: m\n").ir;
    let actors: Vec<&str> = ir.nodes.iter().map(|node| node.id.as_str()).collect();
    assert_eq!(
        actors,
        ["my-actor", "other-actor"],
        "an actor name lost its dash"
    );

    // A trailing dash separated from the arrow by whitespace belongs to the ACTOR, not the arrow.
    let ir = fm_parser::parse("sequenceDiagram\nA- ->> B: m\n").ir;
    let actors: Vec<&str> = ir.nodes.iter().map(|node| node.id.as_str()).collect();
    assert_eq!(
        actors,
        ["A-", "B"],
        "a trailing dash was absorbed into the arrow"
    );
}
