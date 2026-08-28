//! Scratch probe kept only because RULE 1 forbids deleting it without express permission.
//!
//! It briefly imported a crate-private type and broke `clippy --all-targets`; reduced to a
//! compiling no-op. Its subject is covered permanently by `forward_reference_to_subgraph.rs`.

#[test]
fn probe_is_inert() {
    assert!(fm_parser::parse("flowchart LR\n  A\n").ir.nodes.len() == 1);
}
