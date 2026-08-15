//! Bench inputs that MIRROR the head-to-head corpus, byte for byte (bd-9cma).
//!
//! These three generators are the only ones in `pipeline_bench.rs` that claim to reproduce a
//! `scripts/headtohead/corpus.mjs` item. That claim used to be a doc comment and nothing else, and
//! two of the three were false: `gen_dense_dag` laid out `TD` where the corpus lays out `LR`, with
//! different node ids and an edge step of 5 where the corpus uses 4; `gen_cyclic_scc` had the right
//! edges under labels wide enough to change node sizing. So a number from the criterion harness and
//! a number from the head-to-head harness were never measuring the same document, which is a
//! stronger failure than the fleet's harness-disagreement finding — those harnesses at least shared
//! an input.
//!
//! They live in their own file so a test target can compile the SAME SOURCE the bench does, via
//! `#[path]`, and assert each one reproduces a committed corpus fixture. A criterion bench runs with
//! `harness = false`, so a `#[cfg(test)]` module inside the bench would never execute — the parity
//! test has to live somewhere that `cargo test` actually reaches.
//!
//! Keep these functions byte-exact against `corpus.mjs`. If you need a different shape for a
//! profiling reason, add a NEW generator with a name that does not claim to mirror anything;
//! `gen_flowchart` in the bench is the example to follow, and it is documented as deliberately not
//! being the corpus flowchart.

/// Mirrors `wide(layers, width)` in `corpus.mjs` — corpus ids `wide_8x16`, `wide_12x24`,
/// `wide_16x32`.
///
/// `layers` ranks of `width` nodes; every node fans out to the node below it and to that node's
/// right-hand neighbour, wrapping with `% width`. Ranks with many nodes are the realistic shape for
/// fan-out pipelines, ER/state diagrams and org charts, and they exercise the crossing-minimization
/// barycenter sweep far more than a linear chain, where every rank holds a single node.
pub fn gen_wide(layers: usize, width: usize) -> String {
    let mut lines = vec![String::from("flowchart TD")];
    for layer in 0..layers {
        for w in 0..width {
            lines.push(format!("  N{layer}_{w}[L{layer} W{w}]"));
        }
    }
    for layer in 0..layers.saturating_sub(1) {
        for w in 0..width {
            lines.push(format!("  N{layer}_{w}-->N{}_{w}", layer + 1));
            lines.push(format!(
                "  N{layer}_{w}-->N{}_{}",
                layer + 1,
                (w + 1) % width
            ));
        }
    }
    lines.join("\n")
}

/// Mirrors `denseDag(n, fanout)` in `corpus.mjs` — corpus id `dense_dag_200` (`n=200, fanout=4`).
///
/// Every node points at the next `fanout` nodes, so `edges ≈ fanout·nodes` while the obstacle count
/// stays `= nodes`. This is the shape the obstacle-index work-gate targets: the count-only
/// `DENSE_INDEX_OBSTACLES` floor excludes it, but its O(edges·obstacles) linear scan is the cost
/// driver.
///
/// No cycles, high edge density. Three things here
/// were wrong before bd-9cma and all three are load-bearing: the direction was `TD` against the
/// corpus's `LR`, which changes ranking outright rather than merely moving geometry; the ids were
/// `N{i}[Node {i}]` against `D{i}[D{i}]`, and label width feeds node sizing; and the edge steps were
/// `[1, 2, 3, 5]` against `1..=fanout`, the same edge COUNT pointing at different targets.
pub fn gen_dense_dag(n: usize, fanout: usize) -> String {
    let mut lines = vec![String::from("flowchart LR")];
    for i in 0..n {
        lines.push(format!("  D{i}[D{i}]"));
    }
    for i in 0..n {
        for k in 1..=fanout {
            if i + k < n {
                lines.push(format!("  D{i}-->D{}", i + k));
            }
        }
    }
    lines.join("\n")
}

/// Mirrors `cyclic(n, ring)` in `corpus.mjs` — corpus id `cyclic_scc_100` (`n=100, ring=5`).
///
/// Rings of `ring` nodes, each ring fully cyclic, chained forward to the next ring. Exercises cycle
/// removal followed by crossing minimization. The labels are `C{i}[C{i}]`, not `C{i}[Node {i}]`:
/// the wider label was the whole of this generator's divergence from the corpus, and it silently
/// changed every node's width.
pub fn gen_cyclic_scc(node_count: usize, ring: usize) -> String {
    let mut lines = vec![String::from("flowchart TD")];
    for i in 0..node_count {
        lines.push(format!("  C{i}[C{i}]"));
    }
    for i in 0..node_count {
        let ring_start = (i / ring) * ring;
        let next = ring_start + ((i - ring_start + 1) % ring);
        if next < node_count {
            lines.push(format!("  C{i}-->C{next}"));
        }
        if i + ring < node_count {
            lines.push(format!("  C{i}-->C{}", i + ring));
        }
    }
    lines.join("\n")
}
