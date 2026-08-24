fn stress(n: usize) -> String {
    let mut lines = vec!["flowchart LR".to_string()];
    for i in 0..n {
        lines.push(format!("    N{i}[Widget {i}]"));
    }
    for i in 0..n.saturating_sub(1) {
        lines.push(format!("    N{i} --> N{}", i + 1));
    }
    for i in 0..n.saturating_sub(3) {
        if i % 3 == 0 {
            lines.push(format!("    N{i} --> N{}", i + 3));
        }
    }
    for i in 0..n.saturating_sub(8) {
        if i % 5 == 0 {
            lines.push(format!("    N{i} --> N{}", i + 8));
        }
    }
    lines.join("\n")
}
fn diverges(n: usize, edges: bool) -> Option<(f32, f32)> {
    let input = if edges {
        stress(n)
    } else {
        let mut l = vec!["flowchart LR".to_string()];
        for i in 0..n {
            l.push(format!("    N{i}[Widget {i}]"));
        }
        for i in 0..n.saturating_sub(1) {
            l.push(format!("    N{i} --> N{}", i + 1));
        }
        l.join("\n")
    };
    let ir = fm_parser::parse(&input).ir;
    let c = fm_layout::LayoutConfig::default();
    let g = fm_layout::LayoutGuardrails::default();
    let mut e = fm_layout::IncrementalLayoutEngine::default();
    let _ = e.layout_diagram_traced_with_config_and_guardrails(
        &ir,
        fm_layout::LayoutAlgorithm::Auto,
        c.clone(),
        g,
    );
    let mut edited = ir.clone();
    let idx = 0usize;
    let lid = edited.nodes[idx].label?.0;
    edited.labels[lid].text = format!("Widget {idx} rev 0");
    let inc = e.layout_diagram_traced_with_config_and_guardrails(
        &edited,
        fm_layout::LayoutAlgorithm::Auto,
        c.clone(),
        g,
    );
    let full = fm_layout::layout_diagram_traced_with_config_and_guardrails(
        &edited,
        fm_layout::LayoutAlgorithm::Auto,
        c.clone(),
        g,
    );
    println!(
        "   n={n} inc_alg={:?}/{:?} full_alg={:?}/{:?} inc_reason={:?} full_reason={:?} inc_fallback={} full_fallback={}",
        inc.trace.guard.initial_algorithm,
        inc.trace.guard.selected_algorithm,
        full.trace.guard.initial_algorithm,
        full.trace.guard.selected_algorithm,
        inc.trace.guard.reason,
        full.trace.guard.reason,
        inc.trace.guard.fallback_applied,
        full.trace.guard.fallback_applied
    );
    Some((
        (inc.layout.bounds.width - full.layout.bounds.width).abs(),
        (inc.layout.bounds.height - full.layout.bounds.height).abs(),
    ))
}
fn main() {
    for edges in [true] {
        for n in [48usize, 60, 66, 70, 71, 72] {
            if let Some((dw, dh)) = diverges(n, edges) {
                let tag = if edges { "dense" } else { "chain" };
                println!(
                    "{tag:<6} n={n:<3} dw={dw:9.3} dh={dh:8.3} {}",
                    if dw > 1.0 || dh > 1.0 {
                        "DIVERGES"
                    } else {
                        "ok"
                    }
                );
            }
        }
    }
}
