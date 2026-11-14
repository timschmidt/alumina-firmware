use alumina_foc::{FocError, Q30, Q30Interval as I};

const FULL: I = match I::new(Q30::from_bits(i32::MIN), Q30::from_bits(i32::MAX)) {
    Ok(value) => value,
    Err(_) => panic!("ordered endpoints"),
};
const WIDTH: u64 = FULL.width_ulps();
const MEMBERSHIP: [bool; 3] = [
    FULL.contains(Q30::from_bits(i32::MIN)),
    FULL.contains(Q30::ZERO),
    FULL.contains(Q30::from_bits(i32::MAX)),
];

#[test]
fn const_surface_and_nonempty_layout_survive_delegation() {
    assert_eq!(size_of::<I>(), 8);
    assert_eq!(align_of::<I>(), align_of::<i32>());
    assert_eq!(WIDTH, u64::from(u32::MAX));
    assert_eq!(MEMBERSHIP, [true, true, true]);
    assert_eq!(FULL.midpoint().bits(), -1);
    assert_eq!(FULL.checked_add(I::ZERO), Ok(FULL));
    assert_eq!(FULL.checked_sub(I::ZERO), Ok(FULL));
    assert_eq!(FULL.checked_neg(), Err(FocError::Range));
    assert_eq!(FULL.checked_div(I::ZERO), Err(FocError::DivideByZero));
    assert_eq!(I::from_ratio(i64::MIN, i64::MIN), Ok(I::ONE));
    assert_eq!(I::from_ratio(i64::MIN, 0), Err(FocError::DivideByZero));
    assert_eq!(I::from_ratio(i64::MIN, -1), Err(FocError::Range));
    assert_eq!(I::new(Q30::ONE, Q30::ZERO), Err(FocError::IntervalOrder));
}

#[test]
fn generated_bounds_reenter_arithmetic_without_rounding_nominally() {
    let third = I::from_ratio(1, 3).unwrap();
    assert_eq!(
        (third.lower().bits(), third.upper().bits()),
        (357_913_941, 357_913_942)
    );
    let square = third.checked_square().unwrap();
    // Exact lattice squares bound (1/3)^2; retain both directed endpoints.
    assert_eq!(
        (square.lower().bits(), square.upper().bits()),
        (119_304_646, 119_304_648)
    );
    let reconstructed = I::new(square.lower(), square.upper()).unwrap();
    assert_eq!(reconstructed, square);
    assert_eq!(reconstructed.checked_add(I::ZERO), Ok(square));
    assert_eq!(square.checked_div(I::ONE), Ok(square));
    assert_eq!(square.checked_neg().unwrap().checked_neg(), Ok(square));
    let minimum = I::point(Q30::from_bits(i32::MIN));
    // A later cancellation cannot excuse an earlier unrepresentable product.
    assert_eq!(
        minimum.checked_mul(I::point(Q30::NEG_ONE)),
        Err(FocError::Range)
    );
}
