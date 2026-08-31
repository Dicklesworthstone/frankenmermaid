fn main() {
    let source = format!(
        "classDiagram\n  class Shape {{\n    {}interface{}\n  }}\n",
        "<<", ">>"
    );
    let ir = fm_parser::parse(&source).ir;
    let meta_ok = ir
        .nodes
        .iter()
        .find_map(|n| n.class_meta.as_deref())
        .map(|m| m.stereotype.clone());
    println!("IR stereotype = {meta_ok:?}");
    let svg = fm_render_svg::render_svg(&ir);
    for probe in [
        "&lt;&lt;interface>>",
        "&lt;&lt;interface&gt;&gt;",
        "<<interface>>",
        "interface",
    ] {
        println!("svg contains {probe:?} = {}", svg.contains(probe));
    }
    // Show the actual emitted run around 'interface'
    if let Some(i) = svg.find("interface") {
        let s = i.saturating_sub(40);
        let e = (i + 40).min(svg.len());
        println!("context: ...{}...", &svg[s..e]);
    }

    // What does the CANVAS draw for the same stereotype?
    let mut ctx = fm_render_canvas::MockCanvas2dContext::new(1200.0, 800.0);
    fm_render_canvas::render_to_canvas(
        &ir,
        &mut ctx,
        &fm_render_canvas::CanvasRenderConfig::default(),
    );
    let ops = format!("{:?}", ctx.operations());
    for probe in ["\u{ab}interface\u{bb}", "<<interface>>"] {
        println!("canvas ops contain {probe:?} = {}", ops.contains(probe));
    }
}
