//! gantt `click` is now HONOURED, not merely ignored (bd-gydqv).
//!
//! bd-vc1zp stopped the directive being drawn as a phantom task bar, and said in writing that this
//! left a capability gap: the click was recognised and dropped, because gantt tasks carried no
//! interaction. This closes it. `click <taskId> href "url" "tip"` and `click <taskId> call fn()`
//! now attach to the task they name, and the tooltip reaches the rendered bar.
//!
//! Two details that are easy to get wrong and are therefore pinned here:
//!
//!   - THE KEY IS NOT THE ID YOU WROTE. A click names a TASK id (`a1`), but the node was interned
//!     under a key derived from the task's label (`Alpha_4`). `set_node_link` INTERNS its key, so
//!     handing it the declared id would have minted a phantom node named `a1` — the exact defect
//!     family (bd-xfmm, bd-vc1zp) this feature is built on top of.
//!   - CLICKS MAY PRECEDE THEIR TASK. Application is deferred until the whole chart is parsed, so a
//!     forward reference works. Applying inline would have silently dropped it.
//!
//! The parser reuses the flowchart's own `apply_click_directive`, so the link-safety gate, the
//! callback/link split and the link-target rule cannot drift between the two diagram types.

fn bar_attrs(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find("class=\"fm-gantt-task ") {
        rest = &rest[start..];
        let Some(end) = rest.find("/>") else { break };
        out.push(rest[..end].to_string());
        rest = &rest[end..];
    }
    out
}

fn task_interaction(ir: &fm_core::MermaidDiagramIr) -> Option<&fm_core::IrNodeInteraction> {
    ir.nodes.iter().find_map(|node| node.interaction.as_deref())
}

/// THE CAPABILITY: an href click attaches to its task and its tooltip reaches the bar.
#[test]
fn a_gantt_click_attaches_its_href_and_tooltip_to_the_named_task() {
    let source = "gantt\n  title Sched\n  section S\n  Alpha :a1, 2024-01-01, 30d\n  \
                  click a1 href \"https://example.com\" \"Alpha tip\"\n";
    let ir = fm_parser::parse(source).ir;

    // NO PHANTOM: the click must attach to the existing task, not intern a node named `a1`.
    let ids: Vec<&str> = ir.nodes.iter().map(|node| node.id.as_str()).collect();
    assert_eq!(
        ids.len(),
        1,
        "the click minted a node instead of attaching to the task: {ids:?}"
    );

    let interaction = task_interaction(&ir).expect("the task carries the declared interaction");
    assert_eq!(interaction.href.as_deref(), Some("https://example.com"));
    assert_eq!(interaction.tooltip.as_deref(), Some("Alpha tip"));

    // RENDERED: the tooltip must reach the bar the reader hovers.
    let svg = fm_render_svg::render_svg(&ir);
    let bars = bar_attrs(&svg);
    assert!(!bars.is_empty(), "CONTROL FAILED: no gantt bar was drawn");
    assert!(
        bars.iter().any(|bar| bar.contains("title=\"Alpha tip\"")),
        "the bar carries no tooltip; bars were {bars:?}"
    );
}

/// The callback form attaches too, with its own tooltip.
#[test]
fn a_gantt_click_callback_attaches_to_the_named_task() {
    let ir = fm_parser::parse(
        "gantt\n  section S\n  Alpha :a1, 2024-01-01, 30d\n  click a1 call doThing() \"cb tip\"\n",
    )
    .ir;
    let interaction = task_interaction(&ir).expect("the task carries the declared interaction");
    assert_eq!(interaction.callback.as_deref(), Some("doThing()"));
    assert_eq!(interaction.tooltip.as_deref(), Some("cb tip"));
    assert!(
        interaction.href.is_none(),
        "a callback is not a link: {interaction:?}"
    );
}

/// A click may be written BEFORE the task it targets.
///
/// This is the whole reason application is deferred to the end of the parse. Written as its own
/// test because an inline implementation passes every other test in this file.
#[test]
fn a_gantt_click_declared_before_its_task_still_attaches() {
    let source = "gantt\n  section S\n  click a1 href \"https://example.com\" \"early\"\n  \
                  Alpha :a1, 2024-01-01, 30d\n";
    let ir = fm_parser::parse(source).ir;

    let ids: Vec<&str> = ir.nodes.iter().map(|node| node.id.as_str()).collect();
    assert_eq!(ids.len(), 1, "a forward reference minted a node: {ids:?}");
    let interaction =
        task_interaction(&ir).expect("a click before its task must still attach to it");
    assert_eq!(interaction.tooltip.as_deref(), Some("early"));

    let bars = bar_attrs(&fm_render_svg::render_svg(&ir));
    assert!(
        bars.iter().any(|bar| bar.contains("title=\"early\"")),
        "the forward-referenced tooltip never reached the bar; bars were {bars:?}"
    );
}

/// CONTROL: a click naming a task that does not exist attaches nothing AND mints nothing.
///
/// Interning the unknown id is the tempting implementation and is precisely how phantom tasks got
/// into this diagram type twice. The parser warns instead.
#[test]
fn a_gantt_click_on_an_unknown_task_mints_no_node() {
    let ir = fm_parser::parse(
        "gantt\n  section S\n  Alpha :a1, 2024-01-01, 30d\n  \
         click nope href \"https://example.com\"\n",
    )
    .ir;
    let ids: Vec<&str> = ir.nodes.iter().map(|node| node.id.as_str()).collect();
    assert_eq!(ids.len(), 1, "an unresolved click invented a task: {ids:?}");
    assert!(
        task_interaction(&ir).is_none(),
        "an unresolved click attached an interaction to an unrelated task"
    );
}

/// CONTROL: a chart with NO click gains no tooltip attribute.
///
/// Without this, emitting `title=""` unconditionally — or leaking a previous task's tooltip —
/// would satisfy every assertion above.
#[test]
fn a_gantt_chart_without_a_click_gains_no_tooltip() {
    let ir =
        fm_parser::parse("gantt\n  title Sched\n  section S\n  Alpha :a1, 2024-01-01, 30d\n").ir;
    let bars = bar_attrs(&fm_render_svg::render_svg(&ir));
    assert!(!bars.is_empty(), "CONTROL FAILED: no gantt bar was drawn");
    assert!(
        !bars.iter().any(|bar| bar.contains("title=")),
        "a chart with no click gained a tooltip attribute; bars were {bars:?}"
    );
}

/// CONTROL: only the NAMED task gets the tooltip, not every bar.
#[test]
fn a_gantt_click_tooltips_only_the_task_it_names() {
    let source = "gantt\n  section S\n  Alpha :a1, 2024-01-01, 30d\n  \
                  Beta :b1, 2024-02-01, 10d\n  click a1 href \"https://example.com\" \"only me\"\n";
    let ir = fm_parser::parse(source).ir;
    let bars = bar_attrs(&fm_render_svg::render_svg(&ir));
    assert_eq!(bars.len(), 2, "expected two bars, got {bars:?}");
    assert_eq!(
        bars.iter().filter(|bar| bar.contains("title=")).count(),
        1,
        "the tooltip leaked onto a task the click did not name; bars were {bars:?}"
    );
}
