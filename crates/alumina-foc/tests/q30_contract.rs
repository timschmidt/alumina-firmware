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

#[test]
fn inverse_clarke_and_modulation_preserve_each_rounding_boundary() {
    use alumina_foc::{AlphaBeta, inverse_clarke, space_vector_modulate};

    let interval = |lo, hi| I::new(Q30::from_bits(lo), Q30::from_bits(hi)).unwrap();
    let input = AlphaBeta {
        alpha: interval(-3, 1),
        beta: interval(-1, 3),
    };
    // In lattice units, -alpha/2 rounds to [-1, 2] and beta*sqrt(3)/2
    // to [-1, 3]. The firmware sums and subtracts those rounded terms.
    let phases = inverse_clarke(input).unwrap();
    assert_eq!(phases.a, input.alpha);
    assert_eq!(phases.b, interval(-2, 5));
    assert_eq!(phases.c, interval(-4, 3));
    let modulate = |limit| space_vector_modulate(input, Q30::from_bits(limit));
    let result = modulate(10).unwrap();
    assert_eq!(result.common_mode, interval(-3, 3));
    let centered = result.centered_phases;
    assert_eq!(centered.a, interval(-6, 4));
    assert_eq!(centered.b, interval(-5, 8));
    assert_eq!(centered.c, interval(-7, 6));
    // With limit 10 the exact duty bounds are [4,14]/20, [5,18]/20 and
    // [3,16]/20. Compare both endpoints after rounding to Q2.30.
    let duties = result.duties;
    assert_eq!(duties.a, interval(214_748_364, 751_619_277));
    assert_eq!(duties.b, interval(268_435_456, 966_367_642));
    assert_eq!(duties.c, interval(161_061_273, 858_993_460));
    assert_eq!(modulate(8).unwrap().duties.b.upper(), Q30::ONE);
    assert!(matches!(modulate(7), Err(FocError::ModulationRange)));

    let full_alpha = AlphaBeta {
        alpha: FULL,
        beta: I::ZERO,
    };
    let phases = inverse_clarke(full_alpha).unwrap();
    assert_eq!(phases.a, FULL);
    assert_eq!(phases.b, interval(-(1 << 30), 1 << 30));
    assert_eq!(phases.c, phases.b);
    // The sum used for common mode exceeds the grid before it is halved.
    assert!(matches!(
        space_vector_modulate(full_alpha, Q30::ONE),
        Err(FocError::Range)
    ));
    assert!(matches!(
        space_vector_modulate(full_alpha, Q30::ZERO),
        Err(FocError::ModulationRange)
    ));
}
