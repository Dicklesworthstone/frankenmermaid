// Is there a SHAPE discriminator that separates the two cases the single scalar cannot?
fn main() {
    for (name, path) in [
        ("stress_120_nodes(flowchart)", "crates/fm-cli/tests/golden/stress_120_nodes.mmd"),
        ("schema_catalog_25 rev0(ER)", "/data/tmp/claude-1000/-data-projects-frankenmermaid/1bf20e92-25d9-4a88-88c9-c86807212720/scratchpad/c4probe/er0.mmd"),
        ("schema_catalog_25 rev1(ER)", "/data/tmp/claude-1000/-data-projects-frankenmermaid/1bf20e92-25d9-4a88-88c9-c86807212720/scratchpad/c4probe/erdiff.mmd"),
    ] {
        let src = std::fs::read_to_string(path).unwrap();
        let ir = fm_parser::parse(&src).ir;
        let n = ir.nodes.len();
        // longest directed chain depth via simple DFS over adjacency (DAG-ish; cap to avoid cycles)
        let mut adj = vec![Vec::new(); n];
        let mut indeg = vec![0usize; n];
        for e in &ir.edges {
            if let (fm_core::IrEndpoint::Node(a), fm_core::IrEndpoint::Node(b)) = (&e.from, &e.to) {
                adj[a.0].push(b.0);
                indeg[b.0] += 1;
            }
        }
        // longest path by memo DFS with visited guard
        fn depth(u: usize, adj: &Vec<Vec<usize>>, memo: &mut Vec<i32>, on: &mut Vec<bool>) -> i32 {
            if memo[u] >= 0 { return memo[u]; }
            if on[u] { return 0; }
            on[u] = true;
            let mut best = 0;
            for &v in &adj[u] { best = best.max(1 + depth(v, adj, memo, on)); }
            on[u] = false;
            memo[u] = best;
            best
        }
        let mut memo = vec![-1i32; n];
        let mut on = vec![false; n];
        let d = (0..n).map(|u| depth(u, &adj, &mut memo, &mut on)).max().unwrap_or(0) + 1;
        let breadth = (n as f32 / d as f32).ceil();
        println!("{name:30} nodes={n:>4} edges={:>4} depth={d:>4} breadth~{breadth:>6.1} depth/nodes={:.3}",
            ir.edges.len(), d as f32 / n as f32);
    }
}
