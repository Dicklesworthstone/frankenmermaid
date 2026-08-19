//! End-to-end floating-point determinism of the layout pipeline (bd-1s1g.6).
//!
//! `fp_determinism_faults.rs` in fm-layout covers node SHAPE geometry — the pure functions in
//! `shapes.rs`. This covers what the bead actually asks about: "all layout coordinates identical",
//! for coordinates produced by the whole solver on real parsed source.
//!
//! It lives in fm-cli because fm-layout does not depend on fm-parser, and a determinism test that
//! hand-builds its IR is testing a graph the parser may never emit. That distinction has already
//! cost this project once: a kanban fix passed its own hand-built fixture while being unreachable
//! from parser output.
//!
//! # What each test is actually looking for
//!
//! Bit-identical repetition is not a tautology in a layout engine. It fails when iteration order
//! leaks into arithmetic — a `HashMap` walked in hash order, a sort that is not total, a
//! parallel reduction that sums in completion order. Every one of those also diverges ACROSS
//! platforms, where the hash seed and the thread schedule differ, so catching it here is the cheap
//! way to catch it there.

use fm_layout::layout_diagram;

const DIAGRAMS: &[(&str, &str)] = &[
    (
        "flowchart",
        "flowchart TD\n  a[Alpha] --> b[Beta]\n  b --> c[Gamma]\n  c -.-> a\n  b --> d[Delta]\n",
    ),
    (
        "sequence",
        "sequenceDiagram\n  participant A\n  participant B\n  A->>B: hello\n  B-->>A: reply\n",
    ),
    (
        "class",
        "classDiagram\n  class Alpha {\n    +String name\n    +run()\n  }\n  Alpha <|-- Beta\n",
    ),
    (
        "state",
        "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy: start\n  Busy --> Idle: done\n",
    ),
];

/// Every coordinate a layout exposes, in a fixed order.
///
/// Deliberately gathers nodes, edges, clusters AND the overall bounds: a divergence confined to
/// edge routing would be invisible to a node-only comparison, and edge routing is the most
/// arithmetic-heavy stage.
fn coordinates(layout: &fm_layout::DiagramLayout) -> Vec<f32> {
    let mut out = Vec::new();

    for node in &layout.nodes {
        out.extend_from_slice(&[
            node.bounds.x,
            node.bounds.y,
            node.bounds.width,
            node.bounds.height,
        ]);
    }
    for edge in &layout.edges {
        for point in edge.points.as_slice() {
            out.extend_from_slice(&[point.x, point.y]);
        }
    }
    for cluster in &layout.clusters {
        out.extend_from_slice(&[
            cluster.bounds.x,
            cluster.bounds.y,
            cluster.bounds.width,
            cluster.bounds.height,
        ]);
    }
    out.extend_from_slice(&[
        layout.bounds.x,
        layout.bounds.y,
        layout.bounds.width,
        layout.bounds.height,
    ]);

    out
}

/// FNV-1a over the BIT PATTERN of every coordinate, in the fixed order `coordinates` emits.
///
/// ⚠️ NOT `DefaultHasher`, and the reason is the entire point of this file. `std`'s default hasher
/// is explicitly not guaranteed stable across Rust versions, and it is randomly seeded per process
/// for HashMap; a "cross-platform golden" built on it would differ between targets for reasons that
/// have nothing to do with floating point, which is precisely the false positive this bead must not
/// produce. FNV-1a is a fixed algorithm over fixed bytes: same input bits, same digest, on any
/// target and any toolchain.
///
/// Hashing `to_bits()` rather than the float: `-0.0 == 0.0` compares true, so a value-based digest
/// would be blind to the exact sign-of-zero divergence that `Rotor::to_affine_matrix` was shipping
/// until bd-1s1g.6 (it built the identity matrix with a `-0.0` in it). Subnormals likewise differ
/// in bits while comparing equal to nothing else in particular.
fn coordinate_digest(values: &[f32]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for value in values {
        for byte in value.to_bits().to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

/// The cross-target comparison artifact the bead asks for (bd-1s1g.6, implementation steps 1-3).
///
/// ⚠️ WHAT THIS TEST DOES AND DOES NOT PROVE, stated because the distinction is the whole reason
/// the bead is still open. Running on x86_64 alone it proves only that these four layouts have not
/// changed. It CANNOT observe aarch64 or wasm32 from here, and a test that claimed to would be
/// worthless — the sibling file in fm-layout makes the same point about its own scope.
///
/// What it does is make step 3 mechanical rather than a project. The digests below are a property
/// of the coordinates, not of this machine, so running this same test under another target is a
/// direct comparison: a target whose floating point diverges fails HERE, on the fixture that
/// diverged, with no bespoke harness and nothing to diff by hand.
///
/// PER-FIXTURE, not one digest for the set. A single combined constant tells you only that
/// something moved; the project has already learned that lesson the expensive way, where 37 goldens
/// changing at once carried no information about which change caused it. These localise it.
///
/// WHEN THIS FAILS ON x86_64 after a deliberate layout change, that is expected: re-derive the
/// constants from the failure message and say in the commit which fixtures moved and why. Do not
/// relax it to a range — a tolerance would make it blind to exactly the one-ULP divergences it
/// exists to catch.
#[test]
fn layout_coordinates_match_their_cross_target_digest() {
    // Derived on x86_64-unknown-linux-gnu at 7d7ec0aa, target-cpu=x86-64-v2 (see .cargo/config.toml
    // — the microarchitecture level is part of what these pin, since it changes which float ops
    // lower to hardware instructions).
    const EXPECTED: &[(&str, u64)] = &[
        ("flowchart", 0x19c8_0a8c_3608_67ca),
        ("sequence", 0x0372_54aa_adab_2295),
        ("class", 0x3123_9435_760e_03d5),
        ("state", 0xa519_e199_b43f_131e),
    ];

    let mut actual = Vec::new();
    for (name, source) in DIAGRAMS {
        let ir = fm_parser::parse(source).ir;
        let digest = coordinate_digest(&coordinates(&layout_diagram(&ir)));
        actual.push((*name, digest));
    }

    // Emitted unconditionally so a run on another target prints its own table even when it passes,
    // which is what makes a cross-target comparison a copy-paste rather than an investigation.
    for (name, digest) in &actual {
        println!("cross-target digest {name} = {digest:#018x}");
    }

    let placeholder = EXPECTED.iter().all(|(_, digest)| *digest == 0);
    assert!(
        !placeholder,
        "the expected digests are still placeholders; fill them in from the table above:\n{}",
        actual
            .iter()
            .map(|(name, digest)| format!("        (\"{name}\", {digest:#018x}),"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    for ((name, expected), (actual_name, actual_digest)) in EXPECTED.iter().zip(&actual) {
        assert_eq!(name, actual_name, "fixture order changed");
        assert_eq!(
            expected, actual_digest,
            "{name}: layout coordinates changed (or this target's floating point diverges)"
        );
    }
}

/// Laying out the same IR twice must give BIT-identical coordinates.
///
/// Compared through `to_bits`, not `==`: f32 equality says `-0.0 == 0.0`, but the two serialise
/// differently, so a sign flip on a zero coordinate would pass an `==` comparison and still show up
/// as a diff in a golden file.
#[test]
fn repeated_layout_is_bit_identical() {
    for (name, source) in DIAGRAMS {
        let ir = fm_parser::parse(source).ir;

        let first = coordinates(&layout_diagram(&ir));
        let second = coordinates(&layout_diagram(&ir));

        assert!(
            !first.is_empty(),
            "{name}: the layout exposed no coordinates, so this comparison proves nothing"
        );
        assert_eq!(
            first.len(),
            second.len(),
            "{name}: two layouts of one IR produced different coordinate counts"
        );

        for (index, (a, b)) in first.iter().zip(second.iter()).enumerate() {
            assert_eq!(
                a.to_bits(),
                b.to_bits(),
                "{name}: coordinate {index} differs between identical runs ({a} vs {b}); an \
                 iteration order or accumulation order is leaking into the arithmetic"
            );
        }
    }
}

/// A CLONED IR must lay out identically to the original.
///
/// The pointed version of the test above. Cloning moves every allocation, so anything keyed on an
/// address — a pointer-hashed map, a sort whose comparator falls back on identity — produces a
/// different order for an equal graph. That is invisible to a repeat-run test, which reuses the
/// same allocations, and it is a direct cause of cross-platform divergence.
#[test]
fn a_cloned_ir_lays_out_identically() {
    for (name, source) in DIAGRAMS {
        let ir = fm_parser::parse(source).ir;
        let cloned = ir.clone();

        let original = coordinates(&layout_diagram(&ir));
        let from_clone = coordinates(&layout_diagram(&cloned));

        assert_eq!(
            original.len(),
            from_clone.len(),
            "{name}: a cloned IR produced a different number of coordinates"
        );
        for (index, (a, b)) in original.iter().zip(from_clone.iter()).enumerate() {
            assert_eq!(
                a.to_bits(),
                b.to_bits(),
                "{name}: coordinate {index} depends on WHERE the IR is allocated ({a} vs {b})"
            );
        }
    }
}

/// No layout coordinate may be NaN, infinite, or subnormal.
///
/// NaN and infinity are broken output anywhere. Subnormals are the portability half: some ARM
/// configurations flush them to zero, so a coordinate that lands in that range renders in one place
/// and not another, and x86_64 CI would never show it.
#[test]
fn layout_coordinates_are_finite_and_normal() {
    for (name, source) in DIAGRAMS {
        let ir = fm_parser::parse(source).ir;
        let layout = layout_diagram(&ir);
        let values = coordinates(&layout);

        assert!(!values.is_empty(), "{name}: nothing to check");
        for (index, value) in values.iter().enumerate() {
            assert!(value.is_finite(), "{name}: coordinate {index} is {value}");
            assert!(
                !value.is_subnormal(),
                "{name}: coordinate {index} is the subnormal {value:e}; ARM configurations that \
                 flush subnormals to zero would place this element differently"
            );
        }
    }
}

/// CONTROL: the fixtures must actually produce nodes AND edges.
///
/// Every assertion above loops over `coordinates(...)`. A fixture that failed to parse would yield
/// an empty layout and satisfy all of them vacuously — and a parser change is exactly the kind of
/// thing that would quietly empty these diagrams.
#[test]
fn the_fixtures_produce_real_layouts() {
    let mut with_edges = 0_usize;

    for (name, source) in DIAGRAMS {
        let ir = fm_parser::parse(source).ir;
        let layout = layout_diagram(&ir);

        assert!(
            !layout.nodes.is_empty(),
            "{name}: parsed to zero nodes, so the determinism assertions are vacuous"
        );
        assert!(
            layout.bounds.width > 0.0 && layout.bounds.height > 0.0,
            "{name}: laid out to an empty canvas ({} x {})",
            layout.bounds.width,
            layout.bounds.height
        );
        with_edges += usize::from(!layout.edges.is_empty());
    }

    // Edge routing is the arithmetic-heaviest stage, so at least one fixture must exercise it or
    // the determinism assertions above never see it. Asserted ACROSS the set rather than per
    // diagram: whether a sequence or class layout expresses its connections as `layout.edges` or in
    // `extensions` is a detail of those layouts, and this test has no business asserting it.
    assert!(
        with_edges > 0,
        "no fixture produced any routed edges; edge routing would go entirely unchecked"
    );
}
