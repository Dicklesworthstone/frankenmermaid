//! Deep-nesting resilience: parsing and laying out pathologically nested input must
//! terminate with bounded stack usage instead of aborting the process.
//!
//! A stack overflow aborts (SIGABRT) and cannot be caught by any caller, so for a library
//! that consumes untrusted markup this is a denial-of-service surface, not just a crash.
//! Each case here runs on a thread sized like a normal Rust worker (2 MiB) so that
//! unbounded recursion fails here rather than surviving on the much larger main stack.

/// Run `body` on a thread with the stack size Rust gives a normal spawned worker.
fn on_worker_sized_stack<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(body)
        .expect("spawn pipeline thread")
        .join()
        .expect("pipeline thread should not overflow its stack")
}

/// A 10,000-level mindmap: depth grows with indentation, so this is the quadratic-size
/// shape. Parse builds a 10,000-node chain and layout must walk it iteratively.
#[test]
fn deep_mindmap_chain_parses_and_lays_out_without_aborting() {
    let depth = 10_000;

    let (node_count, layout_node_count) = on_worker_sized_stack(move || {
        use std::fmt::Write;
        let mut input = String::from("mindmap\n");
        let mut indent = String::new();
        for i in 0..depth {
            writeln!(input, "{indent}A{i}").unwrap();
            indent.push_str("  ");
        }

        let parsed = fm_parser::parse(&input);
        let layout = fm_layout::layout_diagram(&parsed.ir);
        (parsed.ir.nodes.len(), layout.nodes.len())
    });

    // Every level became a node -- the input was neither truncated nor rejected.
    assert_eq!(node_count, depth);
    assert_eq!(layout_node_count, depth);
}

/// Nested `subgraph` blocks with no indentation: depth costs a constant ~14 bytes per
/// level, so an input-size limit does not bound it. This is the shape that overflowed the
/// stack at under 1 MB of input before the parser's nesting cap existed.
#[test]
fn deeply_nested_subgraphs_parse_and_lay_out_without_aborting() {
    let nesting = 50_000;

    let (input_len, edge_count, cluster_count) = on_worker_sized_stack(move || {
        use std::fmt::Write;
        let mut input = String::from("flowchart TD\n");
        for i in 0..nesting {
            writeln!(input, "subgraph S{i}").unwrap();
        }
        input.push_str("X --> Y\n");
        for _ in 0..nesting {
            input.push_str("end\n");
        }

        let parsed = fm_parser::parse(&input);
        let layout = fm_layout::layout_diagram(&parsed.ir);
        (input.len(), parsed.ir.edges.len(), layout.clusters.len())
    });

    assert!(
        input_len < 1024 * 1024,
        "{nesting} levels should fit under 1 MiB, got {input_len} bytes"
    );
    // Surplus nesting is flattened, but the innermost edge survives it.
    assert_eq!(edge_count, 1);
    // The cap bounds the cluster hierarchy well below the requested nesting.
    assert!(
        cluster_count < nesting,
        "nesting cap should bound clusters, got {cluster_count}"
    );
}
