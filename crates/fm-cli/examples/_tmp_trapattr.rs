// The other diagonal: families where the reference IGNORES a `title` statement.
fn main() {
    for (name, body) in [
        ("flowchart", "flowchart LR\n  title FMTITLE\n  A --> B"),
        ("class", "classDiagram\n  title FMTITLE\n  class A"),
        ("gitgraph", "gitGraph\n  title FMTITLE\n  commit"),
        ("requirement", "requirementDiagram\n  title FMTITLE\n  requirement r {\n    id: 1\n    text: t\n    risk: high\n    verifymethod: test\n  }"),
    ] {
        let parsed = fm_parser::parse(&format!("{body}\n"));
        let svg = fm_render_svg::render_svg(&parsed.ir);
        println!(
            "{:<13} ir.title={:<7} from_fm={:<6} draws={}  (reference draws: false)",
            name,
            format!("{}", parsed.ir.meta.title.is_some()),
            parsed.ir.meta.title_from_front_matter,
            svg.contains("FMTITLE")
        );
    }
}
