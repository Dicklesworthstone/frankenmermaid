//! GENERIC INVARIANT: text the user declared must reach the SVG, for every diagram type.
//!
//! The sibling gate in fm-render-term (`declared_text_reaches_the_terminal_for_every_diagram_type`)
//! asserts this for the terminal. This is the same question asked of the renderer that is actually
//! compared against mermaid-js, so a drop here is a divergence a user would see.
//!
//! It found bd-jgco on its first run.

/// Diagram types and the strings the source declares that must appear in the rendered SVG.
///
/// The check is `svg.contains(want)`, which is OPTIMISTIC: a declared string can also appear in an
/// `id`/`class` attribute, so this under-reports drops. That is deliberate — it makes the gate
/// cheap and keeps it free of false alarms, and anything it DOES report is a real drop. Use the
/// cross-engine equivalence oracle, not this, for a strict comparison.
const CASES: &[(&str, &str, &[&str])] = &[
    ("flowchart", "flowchart TD\n  a[Alpha] -->|yes| b[Beta]\n", &["Alpha", "Beta", "yes"]),
    (
        "sequence",
        "sequenceDiagram\n  participant Al as Alpha\n  Al->>Bob: Hello\n  Note over Al: Ponder\n",
        &["Alpha", "Bob", "Hello", "Ponder"],
    ),
    (
        "class",
        "classDiagram\n  class Alpha {\n    +String name\n    +run()\n  }\n  note for Alpha \"Careful\"\n",
        &["Alpha", "name", "run", "Careful"],
    ),
    (
        "state",
        "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy : go\n  note right of Idle : Waiting\n",
        &["Idle", "Busy", "go", "Waiting"],
    ),
    (
        "er",
        "erDiagram\n  CUSTOMER {\n    string name\n    int age\n  }\n  CUSTOMER ||--o{ ORDER : places\n",
        &["CUSTOMER", "ORDER", "places", "name", "age"],
    ),
    ("gantt", "gantt\n  title Plan\n  section Build\n    Task1 : t1, 2024-01-01, 3d\n", &["Plan", "Build", "Task1"]),
    ("pie", "pie title Share\n  \"Alpha\" : 60\n  \"Beta\" : 40\n", &["Share", "Alpha", "Beta"]),
    ("timeline", "timeline\n  title Hist\n  2001 : Alpha\n  2002 : Beta\n", &["Hist", "Alpha", "Beta", "2001"]),
    ("mindmap", "mindmap\n  root((Core))\n    Alpha\n    Beta\n", &["Core", "Alpha", "Beta"]),
    (
        "quadrant",
        "quadrantChart\n  title Reach\n  x-axis Low --> High\n  y-axis Bot --> Top\n  Alpha: [0.3, 0.6]\n",
        &["Reach", "Low", "High", "Bot", "Top", "Alpha"],
    ),
    (
        "xychart",
        "xychart-beta\n  title Sales\n  x-axis [jan, feb]\n  y-axis \"Rev\" 0 --> 100\n  bar [50, 60]\n",
        &["Sales", "jan", "feb", "Rev"],
    ),
    (
        "requirement",
        "requirementDiagram\n  requirement Alpha {\n  id: 1\n  text: hello\n  risk: high\n  }\n",
        &["Alpha", "hello", "high"],
    ),
    ("sankey", "sankey-beta\n\nAlpha,Beta,5\n", &["Alpha", "Beta"]),
    ("block", "block-beta\n  columns 2\n  Alpha[\"Alpha\"] Beta[\"Beta\"]\n", &["Alpha", "Beta"]),
    ("kanban", "kanban\n  Alpha\n    t1[Beta]\n", &["Alpha", "Beta"]),
    ("packet", "packet-beta\n  0-7: \"Alpha\"\n  8-15: \"Beta\"\n", &["Alpha", "Beta"]),
    (
        "architecture",
        "architecture-beta\n  group api(cloud)[API]\n  service db(database)[Database] in api\n",
        &["API", "Database"],
    ),
    ("c4", "C4Context\n  title Sys\n  Person(alice, \"Alice\", \"A user\")\n", &["Sys", "Alice"]),
    // `dev` is the BRANCH NAME — the drop this gate found (bd-jgco), now fixed by the branch bands
    // in `layout_diagram_gitgraph_traced`. It stays asserted here rather than in a layout unit test
    // because the user-visible claim is that the name reaches the rendered document.
    (
        "gitgraph",
        // The explicit `checkout dev` is load-bearing, and it is NOT how a mermaid user would have
        // to write this. mermaid's `branch` creates AND checks out — its `createBranch` ends by
        // calling the same function its `checkout` uses, verified in the pinned 11.15.0 bundle —
        // while our `parse_git_branch` never sets `current_branch`. Without the checkout, `Beta`
        // lands on `main`, lane `dev` has no commits and therefore no band, and this case would
        // fail for a reason with nothing to do with band labels. Tracked as bd-6oz7; when that
        // lands the checkout line becomes redundant here but stays harmless.
        "gitGraph\n  commit id: \"Alpha\"\n  branch dev\n  checkout dev\n  commit id: \"Beta\" tag: \"v1\"\n",
        &["Alpha", "Beta", "v1", "dev"],
    ),
];

/// Declared text the renderer is KNOWN to drop, each entry naming the bead that tracks it.
///
/// An allowlist, not a silence: a NEW drop still fails, and closing a bead means deleting its line
/// here, so the gate cannot quietly stay satisfied by a defect. Entries are `(case, wanted text)`.
const KNOWN_GAPS: &[(&str, &str, &str)] = &[];

#[test]
fn declared_text_reaches_the_svg_for_every_diagram_type() {
    let mut failures: Vec<String> = Vec::new();
    let mut stale_gaps: Vec<String> = Vec::new();

    for (name, source, wants) in CASES {
        let ir = fm_parser::parse(source).ir;
        let svg = fm_render_svg::render_svg(&ir);

        // ANTI-BLINDNESS CONTROL. `contains` over a whole document is a weak probe: if a type ever
        // embedded its own source verbatim, or the SVG carried a debug dump, every check below
        // would pass for the wrong reason. A string the user never wrote must be absent first.
        assert!(
            !svg.contains("zznotdeclaredzz"),
            "{name}: the SVG contains text that was never declared, so the checks below are not \
             evidence"
        );
        // The renderer must actually have produced a document — an empty string trivially satisfies
        // the control above.
        assert!(
            svg.contains("<svg"),
            "{name}: no SVG document was produced, so this case proves nothing"
        );

        for want in *wants {
            if !svg.contains(want) {
                failures.push(format!("{name}: declared text {want:?} never reached the SVG"));
            }
        }
    }

    // A known gap that has started passing must be REMOVED from the list, not left to rot. Without
    // this, a fixed defect leaves a permanent hole in the gate that a regression could slip back
    // through unnoticed.
    for (case, want, note) in KNOWN_GAPS {
        let Some((_, source, _)) = CASES.iter().find(|(name, _, _)| name == case) else {
            stale_gaps.push(format!("{case}: no such case"));
            continue;
        };
        let ir = fm_parser::parse(source).ir;
        if fm_render_svg::render_svg(&ir).contains(want) {
            stale_gaps.push(format!(
                "{case}: {want:?} now reaches the SVG — delete this KNOWN_GAPS entry and add it to \
                 the case's wants. Entry said: {note}"
            ));
        }
    }

    assert!(
        stale_gaps.is_empty(),
        "KNOWN_GAPS is out of date:\n  {}",
        stale_gaps.join("\n  ")
    );
    assert!(
        failures.is_empty(),
        "declared text never reached the SVG:\n  {}",
        failures.join("\n  ")
    );
}
