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

/// GH #12 item 3: non-finite Wardley coordinates (`nan`, `inf`) in components, evolve, and size
/// must be rejected with warnings and never leak NaN/Inf into the IR.
#[test]
fn non_finite_wardley_coordinates_and_size_are_rejected_with_warnings() {
    let source = concat!(
        "wardley-beta\n",
        "size [inf, 500]\n",
        "component Good [0.5, 0.5]\n",
        "component Bad [nan, 0.5]\n",
        "evolve Good nan\n",
        "annotations [inf, 0.5]\n",
    );
    let parsed = fm_parser::parse(source);
    assert!(
        parsed.warnings.len() >= 4,
        "expected warnings for non-finite size, component, evolve, and annotations; got {:?}",
        parsed.warnings
    );
    let meta = parsed.ir.wardley_meta.as_ref().expect("wardley metadata");
    assert_eq!(
        (meta.width, meta.height),
        (900.0, 600.0),
        "non-finite size must fall back to default 900x600"
    );
    assert!(
        meta.width.is_finite() && meta.height.is_finite(),
        "wardley dimensions must be finite"
    );
    assert!(
        meta.points
            .iter()
            .all(|p| p.x.is_finite() && p.y.is_finite()),
        "all Wardley points must be finite: {:?}",
        meta.points
    );
    assert!(
        !parsed.ir.nodes.iter().any(|n| n.id.contains("Bad")),
        "component with NaN coordinate must not enter IR"
    );
    assert!(
        parsed.ir.edges.is_empty(),
        "evolve with NaN target must not produce an evolution edge"
    );
}
