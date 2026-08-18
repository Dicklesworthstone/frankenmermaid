//! No declared feature may be SILENTLY IGNORED: each must change the rendered SVG.
//!
//! Each case pairs a baseline source with a variant differing only by one feature. If the two render
//! byte-identically, that feature reached the renderer and did nothing — which is what a user sees as
//! "mermaid draws this and frankenmermaid doesn't".
//!
//! THIS METHOD HAS FOUND REAL DEFECTS, twice. bd-jgco (gitGraph branch names) and bd-jerh (ER
//! attribute comments) were both dead IR fields: parsed, stored, drawn by nothing. Both rendered
//! byte-identical to the source without the feature, and both are asserted here now.
//!
//! WHAT THIS GATE DOES **NOT** CATCH, measured rather than assumed. It detects a feature with NO
//! EFFECT AT ALL — not a feature that is undrawn. Disarming only bd-jerh's RENDERER half left the
//! comment still feeding `er_attribute_row_width`, so the entity box changed width, the two
//! documents differed, and this gate PASSED while the comment text was invisible. It fires only on
//! the faithful reproduction, with the layout half disarmed too (verified: it then reports
//! `er_attribute_comment`). So a feature consumed by layout alone slips through here — the
//! declared-TEXT gate in `text_parity.rs` is what covers that direction, and the two are
//! complementary rather than redundant.
//!
//! COMPARE BY CONTENT, NEVER BY SIZE. `class_visibility_private` (`+run()` vs `-run()`) renders at
//! IDENTICAL length with different bytes; a length check would call it ignored. Equal size is not
//! equal content.

/// `(name, baseline, variant)` — the variant differs from the baseline by exactly one feature.
const CASES: &[(&str, &str, &str)] = &[
    // ── class ────────────────────────────────────────────────────────────────────────────────
    ("class_interface_annotation", "classDiagram\n  class Alpha\n  Alpha : +run()\n", "classDiagram\n  class Alpha\n  <<interface>> Alpha\n  Alpha : +run()\n"),
    ("class_abstract_annotation", "classDiagram\n  class Alpha\n  Alpha : +run()\n", "classDiagram\n  class Alpha\n  <<abstract>> Alpha\n  Alpha : +run()\n"),
    ("class_generic", "classDiagram\n  class Alpha\n  Alpha : +items\n", "classDiagram\n  class Alpha~T~\n  Alpha : +items\n"),
    ("class_visibility_private", "classDiagram\n  class Alpha\n  Alpha : +run()\n", "classDiagram\n  class Alpha\n  Alpha : -run()\n"),
    ("class_static", "classDiagram\n  class Alpha\n  Alpha : +run()\n", "classDiagram\n  class Alpha\n  Alpha : +run()$\n"),
    ("class_abstract_method", "classDiagram\n  class Alpha\n  Alpha : +run()\n", "classDiagram\n  class Alpha\n  Alpha : +run()*\n"),
    ("class_cardinality", "classDiagram\n  Alpha --> Beta\n", "classDiagram\n  Alpha \"1\" --> \"many\" Beta\n"),
    // ── state ────────────────────────────────────────────────────────────────────────────────
    ("state_choice", "stateDiagram-v2\n  [*] --> A\n  A --> B\n", "stateDiagram-v2\n  state c <<choice>>\n  [*] --> A\n  A --> c\n  c --> B\n"),
    ("state_fork", "stateDiagram-v2\n  [*] --> A\n  A --> B\n", "stateDiagram-v2\n  state f <<fork>>\n  [*] --> f\n  f --> A\n  f --> B\n"),
    ("state_concurrent", "stateDiagram-v2\n  state S {\n    A --> B\n  }\n", "stateDiagram-v2\n  state S {\n    A --> B\n    --\n    C --> D\n  }\n"),
    // ── er ───────────────────────────────────────────────────────────────────────────────────
    ("er_attribute_pk", "erDiagram\n  A {\n    string name\n  }\n", "erDiagram\n  A {\n    string name PK\n  }\n"),
    // bd-jerh: this pair was byte-identical — the comment was parsed and drawn by nothing.
    ("er_attribute_comment", "erDiagram\n  A {\n    string name\n  }\n", "erDiagram\n  A {\n    string name \"the name\"\n  }\n"),
    ("er_identifying_vs_not", "erDiagram\n  A ||--o{ B : has\n", "erDiagram\n  A ||..o{ B : has\n"),
    // ── flowchart ────────────────────────────────────────────────────────────────────────────
    ("flow_classdef", "flowchart TD\n  a[A] --> b[B]\n", "flowchart TD\n  a[A] --> b[B]\n  classDef big fill:#f00,stroke:#00f\n  class a big\n"),
    ("flow_style_stmt", "flowchart TD\n  a[A] --> b[B]\n", "flowchart TD\n  a[A] --> b[B]\n  style a fill:#f00,stroke:#00f\n"),
    ("flow_linkstyle", "flowchart TD\n  a[A] --> b[B]\n", "flowchart TD\n  a[A] --> b[B]\n  linkStyle 0 stroke:#f00,stroke-width:4px\n"),
    ("flow_subgraph_direction", "flowchart TD\n  subgraph S\n    a[A] --> b[B]\n  end\n", "flowchart TD\n  subgraph S\n    direction LR\n    a[A] --> b[B]\n  end\n"),
    ("flow_click", "flowchart TD\n  a[A] --> b[B]\n", "flowchart TD\n  a[A] --> b[B]\n  click a \"https://example.com\"\n"),
    ("flow_shape_stadium", "flowchart TD\n  a[A]\n", "flowchart TD\n  a([A])\n"),
    ("flow_shape_hexagon", "flowchart TD\n  a[A]\n", "flowchart TD\n  a{{A}}\n"),
    ("flow_shape_cylinder", "flowchart TD\n  a[A]\n", "flowchart TD\n  a[(A)]\n"),
    // ── gantt ────────────────────────────────────────────────────────────────────────────────
    ("gantt_milestone", "gantt\n  section S\n    T : t1, 2024-01-01, 3d\n", "gantt\n  section S\n    T : milestone, t1, 2024-01-01, 0d\n"),
    ("gantt_crit", "gantt\n  section S\n    T : t1, 2024-01-01, 3d\n", "gantt\n  section S\n    T : crit, t1, 2024-01-01, 3d\n"),
    ("gantt_done", "gantt\n  section S\n    T : t1, 2024-01-01, 3d\n", "gantt\n  section S\n    T : done, t1, 2024-01-01, 3d\n"),
    ("gantt_active", "gantt\n  section S\n    T : t1, 2024-01-01, 3d\n", "gantt\n  section S\n    T : active, t1, 2024-01-01, 3d\n"),
    // ── mindmap ──────────────────────────────────────────────────────────────────────────────
    ("mindmap_rounded", "mindmap\n  root((R))\n    A\n", "mindmap\n  root((R))\n    A(A)\n"),
    ("mindmap_circle", "mindmap\n  root((R))\n    A\n", "mindmap\n  root((R))\n    A((A))\n"),
    ("mindmap_cloud", "mindmap\n  root((R))\n    A\n", "mindmap\n  root((R))\n    A)A(\n"),
    ("mindmap_hexagon", "mindmap\n  root((R))\n    A\n", "mindmap\n  root((R))\n    A{{A}}\n"),
    ("mindmap_icon", "mindmap\n  root((R))\n    A\n", "mindmap\n  root((R))\n    A\n    ::icon(fa fa-book)\n"),
    ("mindmap_square", "mindmap\n  root((R))\n    A\n", "mindmap\n  root((R))\n    A[A]\n"),
    // ── gitgraph ─────────────────────────────────────────────────────────────────────────────
    ("git_type_highlight", "gitGraph\n  commit id: \"A\"\n  commit id: \"B\"\n", "gitGraph\n  commit id: \"A\"\n  commit id: \"B\" type: HIGHLIGHT\n"),
    ("git_type_reverse", "gitGraph\n  commit id: \"A\"\n  commit id: \"B\"\n", "gitGraph\n  commit id: \"A\"\n  commit id: \"B\" type: REVERSE\n"),
    ("git_tag", "gitGraph\n  commit id: \"A\"\n  commit id: \"B\"\n", "gitGraph\n  commit id: \"A\"\n  commit id: \"B\" tag: \"v9\"\n"),
    // ── sequence ─────────────────────────────────────────────────────────────────────────────
    ("seq_activate", "sequenceDiagram\n  Alice->>Bob: Hello\n  Bob->>Alice: Hi\n", "sequenceDiagram\n  Alice->>Bob: Hello\n  activate Bob\n  Bob->>Alice: Hi\n  deactivate Bob\n"),
    ("seq_loop", "sequenceDiagram\n  Alice->>Bob: Hello\n  Bob->>Alice: Hi\n", "sequenceDiagram\n  loop Every day\n    Alice->>Bob: Hello\n  end\n  Bob->>Alice: Hi\n"),
    ("seq_par", "sequenceDiagram\n  Alice->>Bob: Hello\n  Bob->>Alice: Hi\n", "sequenceDiagram\n  par One\n    Alice->>Bob: Hello\n  and Two\n    Bob->>Alice: Hi\n  end\n"),
    ("seq_critical", "sequenceDiagram\n  Alice->>Bob: Hello\n  Bob->>Alice: Hi\n", "sequenceDiagram\n  critical Must\n    Alice->>Bob: Hello\n  end\n  Bob->>Alice: Hi\n"),
    ("seq_rect", "sequenceDiagram\n  Alice->>Bob: Hello\n  Bob->>Alice: Hi\n", "sequenceDiagram\n  rect rgb(200,200,200)\n    Alice->>Bob: Hello\n  end\n  Bob->>Alice: Hi\n"),
    ("seq_destroy", "sequenceDiagram\n  Alice->>Bob: Hello\n", "sequenceDiagram\n  Alice->>Bob: Hello\n  destroy Bob\n  Alice->>Bob: Bye\n"),
    ("seq_box", "sequenceDiagram\n  Alice->>Bob: Hello\n  Bob->>Alice: Hi\n", "sequenceDiagram\n  box Team\n    participant Alice\n    participant Bob\n  end\n  Alice->>Bob: Hello\n  Bob->>Alice: Hi\n"),
    // ── misc ─────────────────────────────────────────────────────────────────────────────────
    ("pie_showdata", "pie title P\n  \"A\" : 60\n", "pie showData title P\n  \"A\" : 60\n"),
    // ── architecture-beta ─────────────────────────────────────────────────────────────────────
    ("arch_group_membership", "architecture-beta\n  service a(cloud)[A]\n  service b(cloud)[B]\n", "architecture-beta\n  group g(cloud)[G]\n  service a(cloud)[A] in g\n  service b(cloud)[B]\n"),
    ("arch_edge", "architecture-beta\n  service a(cloud)[A]\n  service b(cloud)[B]\n", "architecture-beta\n  service a(cloud)[A]\n  service b(cloud)[B]\n  a:R --> L:b\n"),
    // bd-zce4: the declared SIDES are a placement grammar, so this pair — identical but for which
    // sides were named — must now render differently. It was a KNOWN_GAP until the
    // direction-aware architecture layout landed.
    ("arch_edge_sides", "architecture-beta\n  service a(cloud)[A]\n  service b(cloud)[B]\n  a:R --> L:b\n", "architecture-beta\n  service a(cloud)[A]\n  service b(cloud)[B]\n  a:T --> B:b\n"),
    ("xychart_line_vs_bar", "xychart-beta\n  x-axis [a, b]\n  bar [50, 60]\n", "xychart-beta\n  x-axis [a, b]\n  line [50, 60]\n"),
];

/// Features KNOWN to render identically to their baseline, each naming the bead that tracks it.
///
/// An allowlist, not a silence. A NEW collision still fails, and an entry that starts DIFFERING
/// fails too (below), so a fixed defect cannot leave a permanent hole here.
/// EMPTY, and that is the point: `mindmap_square` was the only entry and it has been deleted
/// because the gate below caught it going stale on the first build after the freeze, exactly as its
/// own note predicted. `A` and `A[A]` now render differently — a default mindmap node carries the
/// `mindmap-no-border` marker class and a stylesheet rule that drops the stroke, matching how
/// mermaid 11.15.0 draws the distinction (in CSS, not geometry).
///
/// A new entry needs a bead and a reason. An entry that starts DIFFERING fails the test below, so a
/// fixed defect cannot leave a permanent hole here — which is the mechanism that just fired.
const KNOWN_GAPS: &[(&str, &str)] = &[];

#[test]
fn no_declared_feature_renders_identically_to_its_absence() {
    let mut ignored: Vec<String> = Vec::new();
    let mut stale_gaps: Vec<String> = Vec::new();

    for (name, baseline, variant) in CASES {
        let base_svg = fm_render_svg::render_svg(&fm_parser::parse(baseline).ir);
        let variant_svg = fm_render_svg::render_svg(&fm_parser::parse(variant).ir);

        // NON-VACUITY: two EMPTY renders are also "identical", and two failed parses would make
        // every case pass the difference check for the wrong reason. Both documents must be real
        // before their comparison means anything.
        assert!(
            base_svg.contains("<svg") && variant_svg.contains("<svg"),
            "{name}: a source did not render a document, so comparing them proves nothing"
        );

        // CONTENT, not size — `class_visibility_private` differs at identical length.
        let identical = base_svg == variant_svg;
        let known = KNOWN_GAPS.iter().any(|(case, _)| case == name);

        if identical && !known {
            ignored.push(format!("{name}: the variant renders byte-identically to its baseline"));
        }
        if !identical && known {
            stale_gaps.push(format!(
                "{name}: now renders differently — delete its KNOWN_GAPS entry"
            ));
        }
    }

    assert!(
        stale_gaps.is_empty(),
        "KNOWN_GAPS is out of date:\n  {}",
        stale_gaps.join("\n  ")
    );
    assert!(
        ignored.is_empty(),
        "declared features that the renderer silently ignores:\n  {}",
        ignored.join("\n  ")
    );
}
