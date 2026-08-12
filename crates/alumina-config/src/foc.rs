//! Canonical FOC record payloads and digest-bound real-time lowering.

use alumina_foc::{
    CountUncertainty, CurrentChannelCalibration, ElectricalPhase, FocParameterSnapshot,
    FocTimingProfile, PiConfig, PwmAdcSynchronization, PwmCompareContract, Q30, RotationPrecision,
    RotorCalibration, RotorCountDirection, TwoShuntCurrentCalibration, TwoShuntPhasePair,
    ValidatedTwoShuntCurrentCalibration,
};
use alumina_protocol::{DeviceCycle, Digest};

use crate::{
    ConfigurationError, FactEvidence, FocAxisProfile, MAX_EXECUTABLE_FOC_AXES,
    RealtimeConfiguration,
};

const SHAPE_DIGEST: Digest = Digest([0xa5; 32]);

/// Direct or quadrature PI record selector.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u16)]
pub enum FocControllerAxis {
    Direct = 1,
    Quadrature = 2,
}

impl FocControllerAxis {
    pub(crate) const fn from_wire(value: u16) -> Option<Self> {
        match value {
            1 => Some(Self::Direct),
            2 => Some(Self::Quadrature),
            _ => None,
        }
    }
}

/// Channel-zero or channel-one calibration selector.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u16)]
pub enum FocCurrentChannel {
    Channel0 = 1,
    Channel1 = 2,
}

impl FocCurrentChannel {
    pub(crate) const fn from_wire(value: u16) -> Option<Self> {
        match value {
            1 => Some(Self::Channel0),
            2 => Some(Self::Channel1),
            _ => None,
        }
    }
}

/// Explicit analog attenuation programmed for one FOC current channel.
///
/// The values are the discrete classic-ESP32 ADC hardware settings. They do
/// not imply an exact voltage range: the separately qualified current-channel
/// calibration remains the sole code-to-current authority.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum FocAdcAttenuation {
    Db0 = 1,
    Db2p5 = 2,
    Db6 = 3,
    Db11 = 4,
}

impl FocAdcAttenuation {
    pub(crate) const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Db0),
            2 => Some(Self::Db2p5),
            3 => Some(Self::Db6),
            4 => Some(Self::Db11),
            _ => None,
        }
    }
}

/// Digest-free normalized FOC parameters stored once in the canonical document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocRuntimeParameters {
    pub instance: u16,
    pub pole_pairs: u16,
    pub timing: FocTimingProfile,
    /// Exactly one means the physical current-limit scalar is the normalized scale.
    pub maximum_phase_current: Q30,
    /// Exactly one means the physical voltage-limit scalar is the normalized scale.
    pub maximum_phase_voltage: Q30,
}

impl FocRuntimeParameters {
    pub(crate) fn validate_shape(self) -> Result<(), ConfigurationError> {
        if usize::from(self.instance) >= MAX_EXECUTABLE_FOC_AXES
            || self.pole_pairs == 0
            || self.maximum_phase_current != Q30::ONE
            || self.maximum_phase_voltage != Q30::ONE
            || self.timing.validate().is_err()
        {
            return Err(ConfigurationError::FocRuntime);
        }
        Ok(())
    }
}

/// One normalized fixed-period PI controller record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocControllerParameters {
    pub instance: u16,
    pub axis: FocControllerAxis,
    pub parameters: PiConfig,
}

impl FocControllerParameters {
    pub(crate) fn validate_shape(self) -> Result<(), ConfigurationError> {
        if usize::from(self.instance) >= MAX_EXECUTABLE_FOC_AXES
            || self.parameters.validate().is_err()
        {
            return Err(ConfigurationError::FocRuntime);
        }
        Ok(())
    }
}

/// Exact absolute-count rotor calibration and certified rotation policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocRotorParameters {
    pub instance: u16,
    pub counts_per_mechanical_turn: u32,
    pub count_at_reference: u32,
    pub electrical_phase_at_reference: ElectricalPhase,
    pub direction: RotorCountDirection,
    pub maximum_alignment_error_bits: u32,
    pub maximum_count_error: CountUncertainty,
    pub rotation_precision: RotationPrecision,
    pub evidence: FactEvidence,
}

impl FocRotorParameters {
    pub(crate) fn validate_shape(self) -> Result<(), ConfigurationError> {
        if usize::from(self.instance) >= MAX_EXECUTABLE_FOC_AXES
            || self.counts_per_mechanical_turn < 2
            || self.count_at_reference >= self.counts_per_mechanical_turn
            || self.rotation_precision.maximum_component_width_ulps == 0
            || self.rotation_precision.maximum_norm_error_ulps == 0
            || self.rotation_precision.maximum_norm_error_ulps > i32::MAX as u32
            || self.evidence == FactEvidence::Declared
        {
            return Err(ConfigurationError::FocRotor);
        }
        CountUncertainty::new(
            self.maximum_count_error.numerator(),
            self.maximum_count_error.denominator(),
        )
        .map_err(|_| ConfigurationError::FocRotor)?;
        Ok(())
    }
}

/// One measured ADC-code mapping selected as channel zero or channel one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocCurrentChannelParameters {
    pub instance: u16,
    pub channel: FocCurrentChannel,
    pub calibration: CurrentChannelCalibration,
    pub evidence: FactEvidence,
}

impl FocCurrentChannelParameters {
    pub(crate) fn validate_shape(self) -> Result<(), ConfigurationError> {
        if usize::from(self.instance) >= MAX_EXECUTABLE_FOC_AXES
            || self.evidence == FactEvidence::Declared
            || self.calibration.validate().is_err()
        {
            return Err(ConfigurationError::FocCurrent);
        }
        Ok(())
    }
}

/// Qualified hardware selection for one stored current-channel calibration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocAdcFrontendParameters {
    pub instance: u16,
    pub channel: FocCurrentChannel,
    pub attenuation: FocAdcAttenuation,
    pub evidence: FactEvidence,
}

impl FocAdcFrontendParameters {
    pub(crate) fn validate_shape(self) -> Result<(), ConfigurationError> {
        if usize::from(self.instance) >= MAX_EXECUTABLE_FOC_AXES
            || self.evidence != FactEvidence::Qualified
        {
            return Err(ConfigurationError::FocHardware);
        }
        Ok(())
    }
}

/// Exact integer PWM/ADC timing and two-shunt uncertainty policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocPwmAdcTimingParameters {
    pub instance: u16,
    pub phase_pair: TwoShuntPhasePair,
    pub device_cycle_hz: u32,
    pub pwm_period_cycles: u32,
    pub nominal_acquisition_offset_cycles: u32,
    pub maximum_trigger_jitter_cycles: u32,
    pub maximum_acquisition_cycles: u32,
    pub maximum_channel_skew_cycles: u32,
    pub maximum_conversion_cycles: u32,
    pub minimum_switching_guard_cycles: u32,
    pub maximum_normalized_current_slew_per_cycle: Q30,
    pub maximum_interchannel_skew_error: Q30,
    pub maximum_phase_current: Q30,
    pub maximum_phase_interval_width_ulps: u32,
    pub pwm_dead_time_cycles: u32,
    pub evidence: FactEvidence,
}

impl FocPwmAdcTimingParameters {
    pub(crate) fn validate_shape(self) -> Result<(), ConfigurationError> {
        let synchronization = PwmAdcSynchronization {
            configuration_digest: SHAPE_DIGEST,
            device_cycle_hz: self.device_cycle_hz,
            pwm_period_cycles: self.pwm_period_cycles,
            nominal_acquisition_offset_cycles: self.nominal_acquisition_offset_cycles,
            maximum_trigger_jitter_cycles: self.maximum_trigger_jitter_cycles,
            maximum_acquisition_cycles: self.maximum_acquisition_cycles,
            maximum_channel_skew_cycles: self.maximum_channel_skew_cycles,
            maximum_conversion_cycles: self.maximum_conversion_cycles,
            minimum_switching_guard_cycles: self.minimum_switching_guard_cycles,
        };
        if usize::from(self.instance) >= MAX_EXECUTABLE_FOC_AXES
            || self.evidence != FactEvidence::Qualified
            || self.maximum_phase_current != Q30::ONE
            || self.pwm_dead_time_cycles == 0
            || self.pwm_dead_time_cycles >= self.pwm_period_cycles / 2
            || self.minimum_switching_guard_cycles < self.pwm_dead_time_cycles
            || synchronization.validate().is_err()
        {
            return Err(ConfigurationError::FocCurrent);
        }
        Ok(())
    }
}

/// Qualified integer timer and HAL-divider selection for one PWM owner.
///
/// The two prescalers retain the raw zero-based values consumed by the ESP32
/// MCPWM HAL. `counter_clock_hz` is the exact clock after both dividers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocPwmHardwareParameters {
    pub instance: u16,
    pub peripheral_source_clock_hz: u32,
    pub counter_clock_hz: u32,
    pub timer_peak_ticks: u16,
    pub minimum_active_ticks: u16,
    pub maximum_quantization_error_ulps: u32,
    pub peripheral_prescaler: u8,
    pub timer_prescaler: u8,
    pub evidence: FactEvidence,
}

impl FocPwmHardwareParameters {
    pub(crate) fn validate_shape(self) -> Result<(), ConfigurationError> {
        if usize::from(self.instance) >= MAX_EXECUTABLE_FOC_AXES
            || self.peripheral_source_clock_hz == 0
            || self.counter_clock_hz == 0
            || self.timer_peak_ticks < 3
            || self.minimum_active_ticks == 0
            || u32::from(self.minimum_active_ticks) * 2 >= u32::from(self.timer_peak_ticks)
            || self.maximum_quantization_error_ulps == 0
            || self.evidence != FactEvidence::Qualified
        {
            return Err(ConfigurationError::FocHardware);
        }
        let complete_divisor = u64::from(self.peripheral_prescaler) + 1;
        let complete_divisor = complete_divisor
            .checked_mul(u64::from(self.timer_prescaler) + 1)
            .ok_or(ConfigurationError::FocHardware)?;
        if u64::from(self.counter_clock_hz).checked_mul(complete_divisor)
            != Some(u64::from(self.peripheral_source_clock_hz))
        {
            return Err(ConfigurationError::FocHardware);
        }
        Ok(())
    }
}

/// Complete controller-facing result derived from one validated configuration digest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoweredFocAxisConfiguration {
    pub instance: u16,
    pub parameters: FocParameterSnapshot,
    pub rotor: RotorCalibration,
    pub rotation_precision: RotationPrecision,
    pub current: ValidatedTwoShuntCurrentCalibration,
    pub pwm_dead_time_cycles: u32,
    pub adc_channel0: FocAdcFrontendParameters,
    pub adc_channel1: FocAdcFrontendParameters,
    pub pwm_hardware: FocPwmHardwareParameters,
    pub pwm_compare: PwmCompareContract,
}

impl RealtimeConfiguration {
    /// Lowers one retained FOC slot only under this validated document identity.
    ///
    /// # Errors
    ///
    /// Returns a configuration/FOC error for an absent slot, inconsistent
    /// summary, zero digest, or any record combination that cannot construct
    /// all of the controller, rotor, and synchronized-current snapshots.
    pub fn lower_foc_axis(
        &self,
        slot: usize,
    ) -> Result<LoweredFocAxisConfiguration, ConfigurationError> {
        if self.identity.digest.is_zero()
            || usize::from(self.identity.summary.foc_axes) != self.profile.foc_axis_count()
        {
            return Err(ConfigurationError::ConfigurationIdentity);
        }
        let profile = self
            .profile
            .foc_axis(slot)
            .ok_or(ConfigurationError::IncompleteAxis)?;
        lower_profile(profile, self.identity.digest)
    }
}

fn lower_profile(
    profile: FocAxisProfile,
    configuration_digest: Digest,
) -> Result<LoweredFocAxisConfiguration, ConfigurationError> {
    let instance = profile.instance;
    if [
        profile.runtime.instance,
        profile.direct_controller.instance,
        profile.quadrature_controller.instance,
        profile.rotor.instance,
        profile.current_channel0.instance,
        profile.current_channel1.instance,
        profile.pwm_adc_timing.instance,
        profile.adc_channel0.instance,
        profile.adc_channel1.instance,
        profile.pwm_hardware.instance,
    ]
    .iter()
    .any(|record_instance| *record_instance != instance)
    {
        return Err(ConfigurationError::IncompleteAxis);
    }
    let parameters = FocParameterSnapshot {
        configuration_digest,
        pole_pairs: profile.runtime.pole_pairs,
        timing: profile.runtime.timing,
        maximum_phase_current: profile.runtime.maximum_phase_current,
        maximum_phase_voltage: profile.runtime.maximum_phase_voltage,
        d_current: profile.direct_controller.parameters,
        q_current: profile.quadrature_controller.parameters,
    };
    parameters
        .validate()
        .map_err(|_| ConfigurationError::FocRuntime)?;

    let rotor = RotorCalibration {
        configuration_digest,
        counts_per_mechanical_turn: profile.rotor.counts_per_mechanical_turn,
        count_at_reference: profile.rotor.count_at_reference,
        electrical_phase_at_reference: profile.rotor.electrical_phase_at_reference,
        pole_pairs: parameters.pole_pairs,
        direction: profile.rotor.direction,
        maximum_alignment_error_bits: profile.rotor.maximum_alignment_error_bits,
        maximum_count_error: profile.rotor.maximum_count_error,
    };
    rotor
        .observe(
            rotor.count_at_reference,
            DeviceCycle(0),
            profile.rotor.rotation_precision,
        )
        .map_err(|_| ConfigurationError::FocRotor)?;

    let synchronization = PwmAdcSynchronization {
        configuration_digest,
        device_cycle_hz: profile.pwm_adc_timing.device_cycle_hz,
        pwm_period_cycles: profile.pwm_adc_timing.pwm_period_cycles,
        nominal_acquisition_offset_cycles: profile.pwm_adc_timing.nominal_acquisition_offset_cycles,
        maximum_trigger_jitter_cycles: profile.pwm_adc_timing.maximum_trigger_jitter_cycles,
        maximum_acquisition_cycles: profile.pwm_adc_timing.maximum_acquisition_cycles,
        maximum_channel_skew_cycles: profile.pwm_adc_timing.maximum_channel_skew_cycles,
        maximum_conversion_cycles: profile.pwm_adc_timing.maximum_conversion_cycles,
        minimum_switching_guard_cycles: profile.pwm_adc_timing.minimum_switching_guard_cycles,
    };
    if u64::from(parameters.timing.pwm_hz).checked_mul(u64::from(synchronization.pwm_period_cycles))
        != Some(u64::from(synchronization.device_cycle_hz))
        || profile.pwm_adc_timing.maximum_phase_current != parameters.maximum_phase_current
    {
        return Err(ConfigurationError::FocCurrent);
    }
    let current = TwoShuntCurrentCalibration {
        configuration_digest,
        phase_pair: profile.pwm_adc_timing.phase_pair,
        channel0: profile.current_channel0.calibration,
        channel1: profile.current_channel1.calibration,
        synchronization,
        maximum_normalized_current_slew_per_cycle: profile
            .pwm_adc_timing
            .maximum_normalized_current_slew_per_cycle,
        maximum_interchannel_skew_error: profile.pwm_adc_timing.maximum_interchannel_skew_error,
        maximum_phase_current: profile.pwm_adc_timing.maximum_phase_current,
        maximum_phase_interval_width_ulps: profile.pwm_adc_timing.maximum_phase_interval_width_ulps,
    }
    .validated()
    .map_err(|_| ConfigurationError::FocCurrent)?;

    if profile.adc_channel0.channel != FocCurrentChannel::Channel0
        || profile.adc_channel1.channel != FocCurrentChannel::Channel1
    {
        return Err(ConfigurationError::FocHardware);
    }
    let pwm_compare = PwmCompareContract::new(
        configuration_digest,
        profile.pwm_adc_timing.device_cycle_hz,
        profile.pwm_adc_timing.pwm_period_cycles,
        profile.pwm_hardware.counter_clock_hz,
        profile.pwm_hardware.timer_peak_ticks,
        profile.pwm_hardware.minimum_active_ticks,
        profile.pwm_hardware.maximum_quantization_error_ulps,
    )
    .map_err(|_| ConfigurationError::FocHardware)?;
    pwm_compare
        .validate_for(&parameters, synchronization)
        .map_err(|_| ConfigurationError::FocHardware)?;
    if u64::from(profile.pwm_hardware.minimum_active_ticks)
        .checked_mul(u64::from(profile.pwm_adc_timing.device_cycle_hz))
        .ok_or(ConfigurationError::FocHardware)?
        < u64::from(profile.pwm_adc_timing.pwm_dead_time_cycles)
            .checked_mul(u64::from(profile.pwm_hardware.counter_clock_hz))
            .ok_or(ConfigurationError::FocHardware)?
    {
        return Err(ConfigurationError::FocHardware);
    }

    Ok(LoweredFocAxisConfiguration {
        instance,
        parameters,
        rotor,
        rotation_precision: profile.rotor.rotation_precision,
        current,
        pwm_dead_time_cycles: profile.pwm_adc_timing.pwm_dead_time_cycles,
        adc_channel0: profile.adc_channel0,
        adc_channel1: profile.adc_channel1,
        pwm_hardware: profile.pwm_hardware,
        pwm_compare,
    })
}

pub(crate) fn validate_profile(profile: FocAxisProfile) -> Result<(), ConfigurationError> {
    lower_profile(profile, SHAPE_DIGEST).map(|_| ())
}
