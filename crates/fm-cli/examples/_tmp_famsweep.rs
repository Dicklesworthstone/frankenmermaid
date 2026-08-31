// Sweep the diagram families with NO cross-engine corpus coverage for the phantom-node signature:
// a directive/keyword line that ends up as a drawn node id instead of being consumed.
fn main() {
    let cases: &[(&str, &str)] = &[
        (
            "gantt",
            "gantt\n  title T\n  dateFormat YYYY-MM-DD\n  section S\n  Task A :a1, 2024-01-01, 30d\n",
        ),
        ("pie", "pie title Pets\n  \"Dogs\" : 386\n  \"Cats\" : 85\n"),
        (
            "journey",
            "journey\n  title My day\n  section Go\n    Wake: 5: Me\n    Eat: 3: Me\n",
        ),
        (
            "gitGraph",
            "gitGraph\n  commit\n  branch dev\n  checkout dev\n  commit\n  checkout main\n  merge dev\n",
        ),
        ("mindmap", "mindmap\n  root((Root))\n    A\n    B\n"),
        (
            "timeline",
            "timeline\n  title History\n  2001 : First\n  2002 : Second\n",
        ),
        (
            "quadrant",
            "quadrantChart\n  title Q\n  x-axis Low --> High\n  y-axis Bad --> Good\n  A: [0.3, 0.6]\n",
        ),
        (
            "requirement",
            "requirementDiagram\n  requirement r {\n    id: 1\n    text: t\n    risk: high\n    verifymethod: test\n  }\n  element e {\n    type: simulation\n  }\n  e - satisfies -> r\n",
        ),
        ("sankey", "sankey-beta\n\nA,B,10\nB,C,5\n"),
        (
            "xychart",
            "xychart-beta\n  title \"X\"\n  x-axis [a, b, c]\n  y-axis \"Y\" 0 --> 100\n  bar [10, 20, 30]\n",
        ),
        ("block", "block-beta\n  columns 2\n  A B\n  C:2\n"),
        ("packet", "packet-beta\n  0-7: \"A\"\n  8-15: \"B\"\n"),
        (
            "c4",
            "C4Context\n  title C\n  Person(a, \"A\", \"d\")\n  System(s, \"S\", \"d\")\n  Rel(a, s, \"uses\")\n",
        ),
        (
            "kanban",
            "kanban\n  Todo\n    t1[Task one]\n  Doing\n    t2[Task two]\n",
        ),
        (
            "treemap",
            "treemap-beta\n\"Root\"\n  \"A\": 10\n  \"B\": 20\n",
        ),
        (
            "radar",
            "radar-beta\n  title R\n  axis a, b, c\n  curve x{1,2,3}\n",
        ),
        (
            "architecture",
            "architecture-beta\n  group g(cloud)[G]\n  service s1(disk)[S1] in g\n  service s2(server)[S2] in g\n  s1:R -- L:s2\n",
        ),
    ];
    for (name, src) in cases {
        let parsed = fm_parser::parse(src);
        let ir = &parsed.ir;
        // A phantom is an id that still contains a directive keyword or punctuation from the source.
        let suspicious: Vec<&str> = ir
            .nodes
            .iter()
            .map(|n| n.id.as_str())
            .filter(|id| {
                let l = id.to_ascii_lowercase();
                [
                    "title",
                    "axis",
                    "section",
                    "dateformat",
                    "columns",
                    "id_",
                    "risk",
                    "type_",
                    "curve",
                    "accdescr",
                    "acctitle",
                ]
                .iter()
                .any(|k| l.contains(k))
            })
            .collect();
        println!(
            "{name:<13} type={:<14} nodes={:<3} edges={:<3} diags={:<2} suspicious={suspicious:?}",
            format!("{:?}", ir.diagram_type),
            ir.nodes.len(),
            ir.edges.len(),
            ir.diagnostics.len()
        );
    }
}
