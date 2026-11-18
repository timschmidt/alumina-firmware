//! Map a raw ADC count to an interval of normalized current.

use alumina_foc::{CurrentChannelCalibration, CurrentPolarity, FocError, Q30, Q30Interval};

fn main() -> Result<(), FocError> {
    // Gains and errors are integer numerators in units of 2^-30 normalized
    // current. The additive error includes half of the largest per-count gain.
    let calibration = CurrentChannelCalibration {
        adc_maximum_count: 4095,
        valid_count_minimum: 2040,
        valid_count_maximum: 2056,
        count_at_zero: 2048,
        polarity: CurrentPolarity::Increasing,
        normalized_current_per_count: Q30Interval::new(Q30::from_bits(1024), Q30::from_bits(2048))?,
        maximum_additive_error: Q30::from_bits(1024),
        maximum_interval_width_ulps: 10240,
    };
    calibration.validate()?;

    // At +3 counts the gain contributes [3072, 6144] before the ±1024
    // additive error. Negative displacement reverses the gain endpoints.
    for (count, expected) in [(2051, (2048, 7168)), (2045, (-7168, -2048))] {
        let current = calibration.map_raw(count)?;
        let bounds = (current.lower().bits(), current.upper().bits());
        assert_eq!(bounds, expected);
        println!(
            "ADC {count} gives [{}, {}] × 2^-30 normalized current.",
            bounds.0, bounds.1
        );
    }
    assert_eq!(calibration.map_raw(2057), Err(FocError::CurrentRawSample));
    Ok(())
}
