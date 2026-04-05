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
