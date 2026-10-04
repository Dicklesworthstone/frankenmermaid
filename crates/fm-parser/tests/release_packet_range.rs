//! Release regression: a full u32 bit range has a 2^32-bit width.
#[test]
fn packet_full_u32_range_preserves_width() {
    let result = fm_parser::parse("packet-beta\n0-4294967295: \"Full range\"\n");
    assert_eq!(result.ir.nodes.len(), 1);
    assert!(
        result.ir.nodes[0]
            .classes
            .iter()
            .any(|class| class == "packet-bits-4294967296")
    );
}
