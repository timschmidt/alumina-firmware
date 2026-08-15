//! Canonical machine configuration fixtures shared by the simulator and browser CAM.

use alumina_board::{OwnerDomain, ResourceId};
use alumina_config::{
    BindingFlags, BindingRole, ConfigurationError, ConfigurationFlags, ConfigurationHeader,
    ConfigurationRecord, ExactScalar, FactEvidence, Rational, ResourceBinding, ScalarFact,
    SignalPolarity,
};
use alumina_protocol::Digest;

fn binding(
    instance: u16,
    role: BindingRole,
    resource: ResourceId,
    polarity: SignalPolarity,
) -> ConfigurationRecord {
    let safety = role == BindingRole::EmergencyStop;
    ConfigurationRecord::Binding(ResourceBinding {
        instance,
        role,
        resource,
        owner: OwnerDomain::Realtime,
        polarity,
        flags: BindingFlags(if safety {
            BindingFlags::REQUIRED_INTERLOCK
        } else {
            0
        }),
        minimum_active_cycles: 48,
        minimum_inactive_cycles: 48,
        maximum_frequency_hz: if safety { 0 } else { 100_000 },
        watchdog_cycles: 240_000,
    })
}

fn scalar(
    instance: u16,
    fact: ScalarFact,
    numerator: i64,
    denominator: u64,
    uncertainty_numerator: i64,
    uncertainty_denominator: u64,
) -> Result<ConfigurationRecord, ConfigurationError> {
    Ok(ConfigurationRecord::Scalar(ExactScalar {
        instance,
        fact,
        value: Rational::new(numerator, denominator)?,
        uncertainty: Rational::new(uncertainty_numerator, uncertainty_denominator)?,
        evidence: FactEvidence::Declared,
    }))
}

fn axis_scalars(instance: u16) -> Result<Vec<ConfigurationRecord>, ConfigurationError> {
    Ok(vec![
        scalar(instance, ScalarFact::AxisFullStepsPerTurn, 200, 1, 0, 1)?,
        scalar(instance, ScalarFact::AxisMicrosteps, 16, 1, 0, 1)?,
        scalar(
            instance,
            ScalarFact::AxisMotorTurnsPerOutputTurn,
            1,
            1,
            0,
            1,
        )?,
        scalar(
            instance,
            ScalarFact::AxisTravelMetresPerOutputTurn,
            1,
            500,
            0,
            1,
        )?,
        scalar(
            instance,
            ScalarFact::AxisCalibrationScale,
            1,
            1,
            1,
            1_000_000,
        )?,
        scalar(instance, ScalarFact::AxisPositionMinimumMetres, 0, 1, 0, 1)?,
        scalar(instance, ScalarFact::AxisPositionMaximumMetres, 3, 10, 0, 1)?,
        scalar(
            instance,
            ScalarFact::AxisVelocityLimitMetresPerSecond,
            1,
            20,
            1,
            1_000,
        )?,
        scalar(
            instance,
            ScalarFact::AxisAccelerationLimitMetresPerSecondSquared,
            1,
            2,
            1,
            100,
        )?,
        scalar(
            instance,
            ScalarFact::AxisJerkLimitMetresPerSecondCubed,
            5,
            1,
            1,
            10,
        )?,
        scalar(
            instance,
            ScalarFact::AxisFollowingErrorMetres,
            1,
            100_000,
            1,
            500_000,
        )?,
    ])
}

/// Builds the canonical two-axis TinyBee fixture used for deterministic CAM and HTTP simulation.
///
/// The capability digest is an explicit argument because the physical and host-only packages
/// share topology while retaining distinct immutable capability identities.
///
/// # Errors
///
/// Returns the configuration schema error if any exact fact or canonical record fails encoding.
pub fn representative_tinybee_configuration_bytes(
    capability_digest: Digest,
) -> Result<Vec<u8>, ConfigurationError> {
    let mut records = vec![
        binding(
            0,
            BindingRole::AxisStep,
            ResourceId::I2sOut { engine: 0, bit: 1 },
            SignalPolarity::ActiveHigh,
        ),
        binding(
            0,
            BindingRole::AxisDirection,
            ResourceId::I2sOut { engine: 0, bit: 2 },
            SignalPolarity::ActiveHigh,
        ),
        binding(
            0,
            BindingRole::AxisDisable,
            ResourceId::I2sOut { engine: 0, bit: 0 },
            SignalPolarity::ActiveHigh,
        ),
        binding(
            0,
            BindingRole::EmergencyStop,
            ResourceId::Gpio(33),
            SignalPolarity::ActiveLow,
        ),
        binding(
            1,
            BindingRole::AxisStep,
            ResourceId::I2sOut { engine: 0, bit: 4 },
            SignalPolarity::ActiveHigh,
        ),
        binding(
            1,
            BindingRole::AxisDirection,
            ResourceId::I2sOut { engine: 0, bit: 5 },
            SignalPolarity::ActiveHigh,
        ),
        binding(
            1,
            BindingRole::AxisDisable,
            ResourceId::I2sOut { engine: 0, bit: 3 },
            SignalPolarity::ActiveHigh,
        ),
    ];
    records.extend(axis_scalars(0)?);
    records.push(scalar(0, ScalarFact::TimerTickHertz, 1_000_000, 1, 0, 1)?);
    records.push(scalar(
        0,
        ScalarFact::StepperOutputQuantumCycles,
        1,
        1,
        0,
        1,
    )?);
    records.extend(axis_scalars(1)?);
    records.sort_by_key(|record| record.canonical_order_key());
    let realtime_record_count = records
        .iter()
        .filter(|record| record.realtime_relevant())
        .count();
    let header = ConfigurationHeader {
        capability_digest,
        record_count: u16::try_from(records.len()).map_err(|_| ConfigurationError::RecordCount)?,
        realtime_record_count: u16::try_from(realtime_record_count)
            .map_err(|_| ConfigurationError::RecordCount)?,
        flags: ConfigurationFlags(
            ConfigurationFlags::MOTION | ConfigurationFlags::CACHED_AUTONOMOUS,
        ),
    };
    let mut bytes = Vec::from(header.encode()?);
    for record in records {
        bytes.extend_from_slice(&record.encode()?);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use alumina_config::{ConfigurationDocumentView, MAX_CONFIGURATION_RECORDS};
    use alumina_storage::sha256;

    use super::*;

    #[test]
    fn one_builder_validates_against_physical_and_simulated_packages() {
        for package in [board_mks_tinybee::PACKAGE, crate::capability::package()] {
            let bytes = representative_tinybee_configuration_bytes(package.board.capability_digest)
                .unwrap();
            let digest = sha256(&bytes).digest;
            let view = ConfigurationDocumentView::decode::<MAX_CONFIGURATION_RECORDS>(
                &package, &bytes, digest,
            )
            .unwrap();
            assert_eq!(view.identity().digest, digest);
            assert_eq!(view.identity().summary.stepper_axes, 2);
            assert!(view.identity().summary.safety_binding);
        }
    }
}
