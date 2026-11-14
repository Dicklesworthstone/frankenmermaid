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
    (
        "flowchart",
        "flowchart TD\n  a[Alpha] -->|yes| b[Beta]\n",
        &["Alpha", "Beta", "yes"],
    ),
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
        // The quoted attribute COMMENT is load-bearing: it was parsed into
        // `IrEntityAttribute::comment` and drawn by nothing, so this entity rendered byte-identical
        // with and without it (bd-jerh). mermaid draws it as a fourth column.
        "erDiagram\n  CUSTOMER {\n    string name \"who they are\"\n    int age\n  }\n  CUSTOMER ||--o{ ORDER : places\n",
        &["CUSTOMER", "ORDER", "places", "name", "age", "who they are"],
    ),
    (
        "gantt",
        "gantt\n  title Plan\n  section Build\n    Task1 : t1, 2024-01-01, 3d\n",
        &["Plan", "Build", "Task1"],
    ),
    (
        "pie",
        "pie title Share\n  \"Alpha\" : 60\n  \"Beta\" : 40\n",
        &["Share", "Alpha", "Beta"],
    ),
    (
        "timeline",
        "timeline\n  title Hist\n  2001 : Alpha\n  2002 : Beta\n",
        &["Hist", "Alpha", "Beta", "2001"],
    ),
    (
        "mindmap",
        "mindmap\n  root((Core))\n    Alpha\n    Beta\n",
        &["Core", "Alpha", "Beta"],
    ),
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
    (
        "sankey",
        "sankey-beta\n\nAlpha,Beta,5\n",
        &["Alpha", "Beta"],
    ),
    (
        "block",
        "block-beta\n  columns 2\n  Alpha[\"Alpha\"] Beta[\"Beta\"]\n",
        &["Alpha", "Beta"],
    ),
    (
        "kanban",
        "kanban\n  Alpha\n    t1[Beta]\n",
        &["Alpha", "Beta"],
    ),
    (
        "packet",
        "packet-beta\n  0-7: \"Alpha\"\n  8-15: \"Beta\"\n",
        &["Alpha", "Beta"],
    ),
    (
        "architecture",
        "architecture-beta\n  group api(cloud)[API]\n  service db(database)[Database] in api\n",
        &["API", "Database"],
    ),
    (
        "c4",
        "C4Context\n  title Sys\n  Person(alice, \"Alice\", \"A user\")\n",
        &["Sys", "Alice"],
    ),
    // `dev` is the BRANCH NAME — the drop this gate found (bd-jgco), now fixed by the branch bands
    // in `layout_diagram_gitgraph_traced`. It stays asserted here rather than in a layout unit test
    // because the user-visible claim is that the name reaches the rendered document.
    (
        "gitgraph",
        // NO `checkout dev` — deliberately. mermaid's `branch` creates AND checks out, so this is
        // how a mermaid user writes it, and it now works here too (bd-6oz7). This case therefore
        // covers both fixes at once: `dev` only appears if the branch got a lane band (bd-jgco)
        // AND the commit after the bare `branch` actually landed on that lane (bd-6oz7). An
        // explicit checkout here would have hidden the second one, which is exactly how it hid in
        // the golden corpus for so long.
        "gitGraph\n  commit id: \"Alpha\"\n  branch dev\n  commit id: \"Beta\" tag: \"v1\"\n",
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
                failures.push(format!(
                    "{name}: declared text {want:?} never reached the SVG"
                ));
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


mod kanban {
    use fm_render_svg::{SvgBackend, SvgRenderConfig};

    #[derive(Debug)]
    struct PaintedRow<'a> {
        attributes: &'a str,
        text: String,
    }

    // Read painted text only. Metadata in an aria-label, CSS class or source attribute cannot
    // satisfy these checks: that was the original parsed-but-invisible Kanban failure.
    fn painted_rows(svg: &str) -> Vec<PaintedRow<'_>> {
        svg.split("<text")
            .skip(1)
            .filter_map(|rest| {
                let (attributes, body) = rest.split_once('>')?;
                let (body, _) = body.split_once("</text>")?;
                let mut text = String::new();
                let mut in_tag = false;
                for ch in body.chars() {
                    match ch {
                        '<' => in_tag = true,
                        '>' if in_tag => in_tag = false,
                        _ if !in_tag => text.push(ch),
                        _ => {}
                    }
                }
                Some(PaintedRow {
                    attributes,
                    text: text
                        .replace("&lt;", "<")
                        .replace("&gt;", ">")
                        .replace("&quot;", "\"")
                        .replace("&apos;", "'")
                        .replace("&amp;", "&"),
                })
            })
            .collect()
    }

    fn attribute<'a>(attributes: &'a str, name: &str) -> Option<&'a str> {
        let prefix = format!(" {name}=\"");
        attributes
            .split_once(prefix.as_str())?
            .1
            .split_once('"')
            .map(|pair| pair.0)
    }

    fn css_value<'a>(attributes: &'a str, name: &str) -> Option<&'a str> {
        attribute(attributes, "style")?
            .split(';')
            .filter_map(|declaration| declaration.split_once(':'))
            .filter(|(key, _)| key.trim() == name)
            .map(|(_, value)| value.trim())
            .next_back()
    }

    fn opacity(attributes: &str, property: &str) -> f32 {
        css_value(attributes, property)
            .or_else(|| attribute(attributes, property))
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(1.0)
    }

    #[test]
    fn kanban_metadata_and_wip_are_painted_in_both_svg_backends() {
        let source = r#"kanban
  todo[To do] @{ wip: 1 }
    a[Ship release] @{ assigned: "A & B", priority: High, ticket: "APP-42", tags: ["UI", "x<y"] }
    b[Second card]
  done[Done]
  empty[Empty] @{ wip: 0 }
"#;
        let parsed = fm_parser::parse(source);
        assert_eq!(parsed.ir.nodes.len(), 2);
        assert_eq!(parsed.ir.clusters.len(), 3);
        for backend in [SvgBackend::LegacyLayout, SvgBackend::Scene] {
            for embed_theme_css in [false, true] {
                let config = SvgRenderConfig {
                    backend,
                    embed_theme_css,
                    ..Default::default()
                };
                let svg = fm_render_svg::render_svg_with_config(&parsed.ir, &config);
                let rows = painted_rows(&svg);
                let text = rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>().join(" ");
                for expected in [
                    "Ship release",
                    "Second card",
                    "Assigned: A & B",
                    "Priority: High",
                    "Ticket: APP-42",
                    "Tag: UI",
                    "Tag: x<y",
                    "WIP: 2 / 1 (over limit)",
                    "WIP: 0 / 0",
                    "0 cards",
                ] {
                    assert_eq!(
                        text.matches(expected).count(),
                        1,
                        "{backend:?}, embedded CSS {embed_theme_css}: {expected:?} must be painted once; {rows:?}"
                    );
                }
                assert!(!svg.contains("<y"), "authored metadata must be XML text");
                assert!(svg.contains("x&lt;y"), "metadata text must be escaped");
                assert_eq!(
                    svg,
                    fm_render_svg::render_svg_with_config(&parsed.ir, &config),
                    "Kanban rendering must remain deterministic"
                );
            }
        }
    }

    #[test]
    fn kanban_empty_columns_retain_names_and_zero_wip() {
        let source = "kanban\n  todo[Todo] @{ wip: 0 }\n  done[Done]\n";
        let ir = fm_parser::parse(source).ir;
        assert!(ir.nodes.is_empty());
        assert_eq!(ir.clusters.len(), 2);
        for backend in [SvgBackend::LegacyLayout, SvgBackend::Scene] {
            let svg = fm_render_svg::render_svg_with_config(
                &ir,
                &SvgRenderConfig {
                    backend,
                    ..Default::default()
                },
            );
            let rows = painted_rows(&svg);
            for expected in ["Todo", "Done", "WIP: 0 / 0", "0 cards"] {
                assert_eq!(
                    rows.iter().filter(|row| row.text == expected).count(),
                    1,
                    "{backend:?}: empty column content {expected:?} was lost; {rows:?}"
                );
            }
            assert!(!svg.contains("over limit"), "an empty zero-limit lane is at its limit");
        }
    }

    #[test]
    fn kanban_card_rows_preserve_author_styles_and_interactions() {
        let source = r#"kanban
  todo[Todo]
    a[Release] @{ assigned: Ada, priority: High, ticket: "R-7", tags: ["UX"] }
style a fill:#ffffff,color:#123456,font-size:24px,font-family:monospace,font-weight:700,font-style:italic,opacity:0.5
click a "https://example.com/task" "Open task" _self
"#;
        let ir = fm_parser::parse(source).ir;
        assert_eq!(ir.nodes.len(), 1, "style and click directives are not extra cards");
        let description = fm_render_svg::describe_node(&ir.nodes[0], &ir);
        for expected in ["assigned: Ada", "priority: High", "ticket: R-7", "tags: UX"] {
            assert!(description.contains(expected), "missing accessible metadata: {description}");
        }
        let node_id = fm_core::mermaid_node_element_id(&ir.nodes[0].id, 0);
        for backend in [SvgBackend::LegacyLayout, SvgBackend::Scene] {
            for embed_theme_css in [false, true] {
                let svg = fm_render_svg::render_svg_with_config(
                    &ir,
                    &SvgRenderConfig {
                        backend,
                        embed_theme_css,
                        include_source_spans: true,
                        ..Default::default()
                    },
                );
                let rows = painted_rows(&svg);
                let card_rows: Vec<_> = rows
                    .iter()
                    .filter(|row| {
                        attribute(row.attributes, "class").is_some_and(|classes| {
                            classes
                                .split_whitespace()
                                .any(|class| class == "fm-kanban-row")
                        })
                    })
                    .collect();
                assert!(card_rows.len() >= 5, "{backend:?}: complete card content is required");
                for row in card_rows {
                    assert_eq!(attribute(row.attributes, "font-size"), Some("24"));
                    assert_eq!(
                        css_value(row.attributes, "fill")
                            .or_else(|| attribute(row.attributes, "fill")),
                        Some("#123456"),
                        "{row:?}",
                    );
                    assert_eq!(
                        css_value(row.attributes, "font-family"),
                        Some("monospace"),
                        "{row:?}"
                    );
                    assert_eq!(css_value(row.attributes, "font-weight"), Some("700"), "{row:?}");
                    assert_eq!(css_value(row.attributes, "font-style"), Some("italic"), "{row:?}");
                    let alpha =
                        opacity(row.attributes, "opacity") * opacity(row.attributes, "fill-opacity");
                    assert!((alpha - 0.5).abs() < 0.001, "author opacity must apply once: {row:?}");
                    assert_eq!(attribute(row.attributes, "xml:space"), Some("preserve"));
                }
                assert!(svg.contains("href=\"https://example.com/task\""));
                assert!(svg.contains("target=\"_self\""));
                assert!(svg.contains("title=\"Open task\""));
                assert!(svg.contains("data-fm-source-span="));
                let identity = format!("id=\"{node_id}\"");
                assert_eq!(svg.matches(identity.as_str()).count(), 1);
                assert!(
                    svg.contains("assigned: Ada"),
                    "accessible names must retain card metadata"
                );
            }
        }
    }

    #[test]
    fn kanban_priority_edits_override_stale_generated_classes() {
        let mut ir = fm_parser::parse(
            "kanban\n  todo[Todo]\n    a[Task] @{ priority: High }\n",
        )
        .ir;
        assert!(ir.nodes[0].classes.iter().any(|class| class == "kanban-priority-high"));
        ir.kanban_meta.as_mut().unwrap().cards[0].priority = Some("Custom".to_owned());
        for backend in [SvgBackend::LegacyLayout, SvgBackend::Scene] {
            let svg = fm_render_svg::render_svg_with_config(
                &ir,
                &SvgRenderConfig {
                    backend,
                    ..Default::default()
                },
            );
            assert!(painted_rows(&svg).iter().any(|row| row.text == "Priority: Custom"));
            assert!(!svg.contains("fill=\"#fca5a5\""));
            assert!(!svg.contains("fill:#fca5a5"));
        }
    }

    #[test]
    fn kanban_scene_keeps_text_readable_on_its_actual_card_fill() {
        for (style, expected) in [("", "#1e293b"), ("style a fill:#111827\n", "#ffffff")] {
            let ir = fm_parser::parse(&format!(
                "kanban\n  todo[Todo]\n    a[Plain card]\n{style}"
            ))
            .ir;
            for embed_theme_css in [false, true] {
                let svg = fm_render_svg::render_svg_with_config(
                    &ir,
                    &SvgRenderConfig {
                        backend: SvgBackend::Scene,
                        theme: fm_render_svg::ThemePreset::Dark,
                        embed_theme_css,
                        ..Default::default()
                    },
                );
                let rows = painted_rows(&svg);
                let title = rows.iter().find(|row| row.text == "Plain card").unwrap();
                assert_eq!(
                    css_value(title.attributes, "fill")
                        .or_else(|| attribute(title.attributes, "fill")),
                    Some(expected),
                    "a theme must not replace contrast resolved against the painted card",
                );
            }
        }
    }

    #[test]
    fn kanban_repeated_card_titles_keep_their_own_metadata() {
        let source = r#"kanban
  todo[Todo] @{ wip: 1 }
    [Review] @{ assigned: Ada, ticket: A1 }
    [Review] @{ assigned: Grace, ticket: B2 }
"#;
        let ir = fm_parser::parse(source).ir;
        assert_eq!(ir.nodes.len(), 2);
        for backend in [SvgBackend::LegacyLayout, SvgBackend::Scene] {
            let svg = fm_render_svg::render_svg_with_config(
                &ir,
                &SvgRenderConfig {
                    backend,
                    ..Default::default()
                },
            );
            let rows = painted_rows(&svg);
            assert_eq!(rows.iter().filter(|row| row.text == "Review").count(), 2);
            let text = rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>().join(" ");
            for expected in ["Assigned: Ada", "Assigned: Grace", "Ticket: A1", "Ticket: B2"] {
                assert_eq!(text.matches(expected).count(), 1, "{backend:?}: {rows:?}");
            }
            assert!(text.contains("WIP: 2 / 1 (over limit)"));
        }
    }
}
