//! Independent discrete images and widened-integer boundary expectations.
use super::*;

#[test]
fn widening_preserves_every_grid_member_and_checked_full_width_edges() {
    let mut cases = 0;
    for lower in -3..=3 {
        for upper in lower..=3 {
            let source = Q30Interval::new(Q30::from_bits(lower), Q30::from_bits(upper)).unwrap();
            for error in -1..=3 {
                let actual = widen_interval(source, Q30::from_bits(error));
                if error < 0 {
                    assert_eq!(actual, Err(FocError::CurrentCalibration));
                } else {
                    let actual = actual.unwrap();
                    let mut members = [false; 13];
                    for point in lower..=upper {
                        for displacement in -error..=error {
                            members[(point + displacement + 6) as usize] = true;
                        }
                    }
                    assert!(actual.lower() >= Q30::from_bits(-6));
                    assert!(actual.upper() <= Q30::from_bits(6));
                    for point in -6..=6 {
                        assert_eq!(
                            actual.contains(Q30::from_bits(point)),
                            members[(point + 6) as usize]
                        );
                    }
                    assert_eq!(
                        actual.checked_neg(),
                        widen_interval(source.checked_neg().unwrap(), Q30::from_bits(error))
                    );
                }
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 140);

    let endpoints = [i32::MIN, i32::MIN + 1, -1, 0, 1, i32::MAX - 1, i32::MAX];
    for (index, &lower) in endpoints.iter().enumerate() {
        for &upper in &endpoints[index..] {
            let source = Q30Interval::new(Q30::from_bits(lower), Q30::from_bits(upper)).unwrap();
            for error in [-1, 0, 1, i32::MAX] {
                let expected = if error < 0 {
                    Err(FocError::CurrentCalibration)
                } else {
                    match (
                        i32::try_from(i64::from(lower) - i64::from(error)),
                        i32::try_from(i64::from(upper) + i64::from(error)),
                    ) {
                        (Ok(lower), Ok(upper)) => {
                            Q30Interval::new(Q30::from_bits(lower), Q30::from_bits(upper))
                        }
                        _ => Err(FocError::Range),
                    }
                };
                assert_eq!(widen_interval(source, Q30::from_bits(error)), expected);
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 252);
}
