// bd-h6gxf: dump the full SELECTION picture — dispatch (quality model) and guard (cost model) —
// for every golden fixture and for every revision of a corpus item, so a change to either model
// can be diffed decision-by-decision rather than judged by a stopwatch.
//
// Prints one stable line per case. Diff two runs of this to see exactly which selections move.
use fm_layout::LayoutAlgorithm;

fn report(name: &str, src: &str) {
    let ir = fm_parser::parse(src).ir;
    let t = fm_layout::layout_diagram_traced(&ir);
    let d = &t.trace.dispatch;
    let g = &t.trace.guard;
    println!(
        "{name:<44} {:>4}n {:>4}e init={:<9} sel={:<9} est={:<6} sel_est={:<6} reason={:<32} \
         loss(s/t/f)={}/{}/{} post(t/d/l)={}/{}/{} w={:.1} h={:.1}",
        ir.nodes.len(),
        ir.edges.len(),
        format!("{:?}", g.initial_algorithm),
        format!("{:?}", g.selected_algorithm),
        g.estimated_layout_time_ms,
        g.selected_estimated_layout_time_ms,
        g.reason,
        d.sugiyama_expected_loss_permille,
        d.tree_expected_loss_permille,
        d.force_expected_loss_permille,
        d.posterior_tree_like_permille,
        d.posterior_dense_graph_permille,
        d.posterior_layered_permille,
        t.layout.bounds.width,
        t.layout.bounds.height,
    );
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "goldens".to_string());
    match mode.as_str() {
        "goldens" => {
            let dir = "crates/fm-cli/tests/golden";
            let mut names: Vec<_> = std::fs::read_dir(dir)
                .unwrap()
                .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
                .filter(|n| n.ends_with(".mmd"))
                .collect();
            names.sort();
            for n in names {
                let Ok(src) = std::fs::read_to_string(format!("{dir}/{n}")) else { continue };
                report(n.trim_end_matches(".mmd"), &src);
            }
        }
        "corpus" => {
            // argv[2] = path to an emit_corpus.mjs JSON: [{id, texts, reps, warmup}]
            let path = std::env::args().nth(2).expect("corpus path");
            let raw = std::fs::read_to_string(&path).unwrap();
            let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
            let item = &v[0];
            let id = item["id"].as_str().unwrap();
            for (i, t) in item["texts"].as_array().unwrap().iter().enumerate() {
                report(&format!("{id}#{i:02}"), t.as_str().unwrap());
            }
        }
        "unguarded" => {
            // argv[2] = corpus path; compare the two general algorithms with the guard disabled,
            // so "Tree" in the output is Tree and not a guard-substituted Sugiyama.
            let path = std::env::args().nth(2).expect("corpus path");
            let raw = std::fs::read_to_string(&path).unwrap();
            let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
            let item = &v[0];
            let big = fm_layout::LayoutGuardrails {
                max_layout_time_ms: usize::MAX / 4,
                max_layout_iterations: usize::MAX / 4,
                max_route_ops: usize::MAX / 4,
            };
            for (i, txt) in item["texts"].as_array().unwrap().iter().enumerate() {
                let ir = fm_parser::parse(txt.as_str().unwrap()).ir;
                let tr =
                    fm_layout::layout_diagram_traced_with_algorithm_and_guardrails(&ir, LayoutAlgorithm::Tree, big);
                let su = fm_layout::layout_diagram_traced_with_algorithm_and_guardrails(
                    &ir,
                    LayoutAlgorithm::Sugiyama,
                    big,
                );
                println!(
                    "rev{i:02} {:>4}n {:>4}e  tree {:>9.1}x{:<9.1} [{:?}]  sugi {:>9.1}x{:<9.1} [{:?}]",
                    ir.nodes.len(),
                    ir.edges.len(),
                    tr.layout.bounds.width,
                    tr.layout.bounds.height,
                    tr.trace.guard.selected_algorithm,
                    su.layout.bounds.width,
                    su.layout.bounds.height,
                    su.trace.guard.selected_algorithm,
                );
            }
        }
        "time" => {
            // Within-process, best-of-N A/B of the two ALGORITHMS on the same IR, with the guard
            // disabled so the arm named is the arm that ran (a guarded request is silently
            // overridden, which contaminated an earlier pass on this bead).
            //
            // Best-of-N, not mean: on a shared host the minimum is the sample least contaminated by
            // co-tenants, and both arms are drawn from the same interleaved sweep.
            let path = std::env::args().nth(2).expect("corpus path");
            let rounds: usize = std::env::args().nth(3).map_or(7, |s| s.parse().unwrap());
            let raw = std::fs::read_to_string(&path).unwrap();
            let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
            let item = &v[0];
            let id = item["id"].as_str().unwrap();
            let big = fm_layout::LayoutGuardrails {
                max_layout_time_ms: usize::MAX / 4,
                max_layout_iterations: usize::MAX / 4,
                max_route_ops: usize::MAX / 4,
            };
            for (i, txt) in item["texts"].as_array().unwrap().iter().enumerate() {
                let ir = fm_parser::parse(txt.as_str().unwrap()).ir;
                // THREE arms, not two. Arm 2 is a NULL: it requests Tree exactly as arm 0 does, so
                // its ratio against arm 0 is the floor this measurement can resolve. Without it a
                // reported "Nx" has nothing to be large relative to, and on a shared host that floor
                // is not assumed to be 1.000 — it is measured in the same interleaved sweep.
                let mut best = [u128::MAX; 3];
                for round in 0..rounds {
                    // Rotate arm order so any monotone drift lands on all three arms equally.
                    let order = match round % 3 {
                        0 => [0usize, 1, 2],
                        1 => [1, 2, 0],
                        _ => [2, 0, 1],
                    };
                    for arm in order {
                        let algo = if arm == 1 {
                            LayoutAlgorithm::Sugiyama
                        } else {
                            LayoutAlgorithm::Tree
                        };
                        let start = std::time::Instant::now();
                        let t = fm_layout::layout_diagram_traced_with_algorithm_and_guardrails(
                            &ir, algo, big,
                        );
                        let ns = start.elapsed().as_nanos();
                        assert_eq!(
                            t.trace.guard.selected_algorithm, algo,
                            "the guard overrode the requested arm; the timing would name the wrong algorithm",
                        );
                        best[arm] = best[arm].min(ns);
                    }
                }
                println!(
                    "{id}#{i:02} {:>5}n {:>5}e  tree_ns={:>12} sugi_ns={:>12} null_ns={:>12}  \
                     sugi/tree={:>8.3}  AA_null={:.3}",
                    ir.nodes.len(),
                    ir.edges.len(),
                    best[0],
                    best[1],
                    best[2],
                    best[1] as f64 / best[0] as f64,
                    best[2] as f64 / best[0] as f64,
                );
            }
        }
        other => panic!("unknown mode {other}"),
    }
}
