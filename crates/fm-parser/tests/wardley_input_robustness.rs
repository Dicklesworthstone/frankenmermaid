//! Wardley map input that crashed the parser (v0.4.0 release review).

/// The link splitter walked the line byte by byte and sliced at every byte outside quotes, so a
/// multi-byte character before the arrow (`Café -> Tea`) sliced inside it and panicked.
#[test]
fn a_non_ascii_component_name_before_a_link_arrow_parses() {
    let parsed = fm_parser::parse(
        "wardley-beta\ncomponent Café [0.5, 0.5]\ncomponent Tea [0.4, 0.4]\nCafé -> Tea\n",
    );
    assert_eq!(parsed.ir.edges.len(), 1, "the link was not parsed");
}

/// A NaN stage boundary (`A@nan`) became the next stage's start and reached `f32::clamp` as its
/// lower bound, which asserts in release builds too. Non-finite boundaries now count as absent.
#[test]
fn a_nan_stage_boundary_parses_without_panicking() {
    let parsed = fm_parser::parse("wardley-beta\nevolution A@nan -> B@1\n");
    assert!(!parsed.ir.nodes.is_empty());
}
