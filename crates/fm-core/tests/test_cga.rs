use fm_core::cga::Multivector;

#[test]
fn test_geometric_product_scalar_part() {
    let mut e12p = Multivector::zero();
    e12p.components[11] = 1.0;
    assert_eq!(
        e12p.geometric_product(e12p).scalar_part(),
        -1.0,
        "e12+^2 should be -1"
    );

    let mut e12m = Multivector::zero();
    e12m.components[12] = 1.0;
    assert_eq!(
        e12m.geometric_product(e12m).scalar_part(),
        1.0,
        "e12-^2 should be +1"
    );

    let mut e1pm = Multivector::zero();
    e1pm.components[13] = 1.0;
    assert_eq!(
        e1pm.geometric_product(e1pm).scalar_part(),
        1.0,
        "e1+-^2 should be +1"
    );

    let mut e2pm = Multivector::zero();
    e2pm.components[14] = 1.0;
    assert_eq!(
        e2pm.geometric_product(e2pm).scalar_part(),
        1.0,
        "e2+-^2 should be +1"
    );
}

#[test]
fn geometric_product_preserves_grade_two_and_three_terms() {
    let mut e1 = Multivector::zero();
    e1.components[1] = 1.0;
    let mut e_plus = Multivector::zero();
    e_plus.components[3] = 1.0;
    let mut e_minus = Multivector::zero();
    e_minus.components[4] = 1.0;

    let e1_plus = e1.geometric_product(e_plus);
    assert_eq!(e1_plus.components[6], 1.0, "e1 * e+ should be e1+");

    let e1_plus_minus = e1_plus.geometric_product(e_minus);
    assert_eq!(
        e1_plus_minus.components[13], 1.0,
        "(e1 * e+) * e- should be e1+-"
    );
    assert_eq!(
        e1_plus_minus.components.iter().sum::<f64>(),
        1.0,
        "the product should not leak into another blade"
    );
}

#[test]
fn geometric_product_respects_antisymmetry_and_negative_basis_square() {
    let mut e1 = Multivector::zero();
    e1.components[1] = 1.0;
    let mut e2 = Multivector::zero();
    e2.components[2] = 1.0;
    let mut e_minus = Multivector::zero();
    e_minus.components[4] = 1.0;

    assert_eq!(e1.geometric_product(e2).components[5], 1.0);
    assert_eq!(e2.geometric_product(e1).components[5], -1.0);
    assert_eq!(e_minus.geometric_product(e_minus).scalar_part(), -1.0);

    let minus_then_minus_e1 = e_minus.geometric_product(e_minus.geometric_product(e1));
    assert_eq!(
        minus_then_minus_e1.components[1], -1.0,
        "e- * (e- * e1) should preserve the negative e- square"
    );
}
