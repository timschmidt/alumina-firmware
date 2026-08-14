//! Canonical FOC record payloads and digest-bound real-time lowering.

use alumina_foc::{
    CountUncertainty, CurrentChannelCalibration, ElectricalPhase, FocParameterSnapshot,
    FocTimingProfile, PiConfig, PwmAdcSynchronization, PwmCompareContract, Q30, RotationPrecision,
    RotorCalibration, RotorCountDirection, ServoCascadeConfig, ServoEncoderProfile,
    ServoEncoderScale, ServoLoopGrid, ServoPosition, TwoShuntCurrentCalibration, TwoShuntPhasePair,
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

/// Digest-free cascaded position/velocity controller parameters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocServoParameters {
    pub instance: u16,
    pub position_proportional_gain: Q30,
    pub velocity_controller: PiConfig,
    pub maximum_velocity: Q30,
    pub maximum_current: Q30,
    pub direct_current_target: Q30,
    pub maximum_following_error_bits: u64,
    pub maximum_sample_age_cycles: u64,
}

impl FocServoParameters {
    pub(crate) fn validate_shape(self) -> Result<(), ConfigurationError> {
        if usize::from(self.instance) >= MAX_EXECUTABLE_FOC_AXES
            || self.position_proportional_gain.bits() < 0
            || self.maximum_velocity.bits() <= 0
            || self.maximum_velocity.bits() > Q30::ONE.bits()
            || self.maximum_current.bits() <= 0
            || self.maximum_current.bits() > Q30::ONE.bits()
            || self.maximum_following_error_bits == 0
            || self.velocity_controller.validate().is_err()
        {
            return Err(ConfigurationError::FocServo);
        }
        Ok(())
    }
}

/// Exact mechanical position and normalized count-rate scales for one encoder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocEncoderScaleParameters {
    pub instance: u16,
    pub position_at_reference: ServoPosition,
    pub scale: ServoEncoderScale,
    pub evidence: FactEvidence,
}

impl FocEncoderScaleParameters {
    pub(crate) fn validate_shape(self) -> Result<(), ConfigurationError> {
        if usize::from(self.instance) >= MAX_EXECUTABLE_FOC_AXES
            || self.evidence == FactEvidence::Declared
        {
            return Err(ConfigurationError::FocEncoder);
        }
        Ok(())
    }
}

/// Qualified exact timing, ambiguity, and precision policy for one encoder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocEncoderPolicyParameters {
    pub instance: u16,
    pub device_cycle_hz: u32,
    pub sample_period_cycles: u64,
    pub maximum_observation_latency_cycles: u64,
    pub maximum_trackable_velocity: Q30,
    pub maximum_admitted_velocity: Q30,
    pub maximum_velocity_estimation_error: Q30,
    pub maximum_position_interval_width_ulps: u64,
    pub maximum_velocity_interval_width_ulps: u32,
    pub evidence: FactEvidence,
}

impl FocEncoderPolicyParameters {
    pub(crate) fn validate_shape(self) -> Result<(), ConfigurationError> {
        if usize::from(self.instance) >= MAX_EXECUTABLE_FOC_AXES
            || self.device_cycle_hz == 0
            || self.sample_period_cycles == 0
            || self.maximum_observation_latency_cycles > self.sample_period_cycles
            || self.maximum_trackable_velocity.bits() <= 0
            || self.maximum_trackable_velocity.bits() > Q30::ONE.bits()
            || self.maximum_admitted_velocity.bits() <= 0
            || self.maximum_admitted_velocity.bits() > self.maximum_trackable_velocity.bits()
            || self.maximum_velocity_estimation_error.bits() <= 0
            || self.maximum_velocity_estimation_error.bits() > self.maximum_admitted_velocity.bits()
            || self.maximum_position_interval_width_ulps == 0
            || self.maximum_velocity_interval_width_ulps == 0
            || self.evidence != FactEvidence::Qualified
        {
            return Err(ConfigurationError::FocEncoder);
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
    pub servo_parameters: FocServoParameters,
    pub encoder_scale_parameters: FocEncoderScaleParameters,
    pub encoder_policy_parameters: FocEncoderPolicyParameters,
    pub servo_grid: ServoLoopGrid,
    pub servo: ServoCascadeConfig,
    pub encoder: ServoEncoderProfile,
}

impl LoweredFocAxisConfiguration {
    /// Replays every digest, calibration, timer, and hardware-selection invariant.
    ///
    /// This lets simulator and target boundaries reject a copied or manually
    /// assembled bundle without weakening the private `RealtimeConfiguration`
    /// construction path used to create an executable profile.
    pub fn validate(self) -> Result<(), ConfigurationError> {
        let synchronization = self.current.snapshot().synchronization;
        if self.instance != self.adc_channel0.instance
            || self.instance != self.adc_channel1.instance
            || self.instance != self.pwm_hardware.instance
            || self.instance != self.servo_parameters.instance
            || self.instance != self.encoder_scale_parameters.instance
            || self.instance != self.encoder_policy_parameters.instance
            || self.adc_channel0.channel != FocCurrentChannel::Channel0
            || self.adc_channel1.channel != FocCurrentChannel::Channel1
        {
            return Err(ConfigurationError::FocHardware);
        }
        self.adc_channel0.validate_shape()?;
        self.adc_channel1.validate_shape()?;
        self.pwm_hardware.validate_shape()?;
        self.servo_parameters.validate_shape()?;
        self.encoder_scale_parameters.validate_shape()?;
        self.encoder_policy_parameters.validate_shape()?;
        self.parameters
            .validate()
            .map_err(|_| ConfigurationError::FocRuntime)?;
        self.rotor
            .observe(
                self.rotor.count_at_reference,
                DeviceCycle(0),
                self.rotation_precision,
            )
            .map_err(|_| ConfigurationError::FocRotor)?;
        self.current
            .snapshot()
            .validate()
            .map_err(|_| ConfigurationError::FocCurrent)?;
        if self.parameters.configuration_digest.is_zero()
            || self.rotor.configuration_digest != self.parameters.configuration_digest
            || self.current.snapshot().configuration_digest != self.parameters.configuration_digest
            || self.rotor.pole_pairs != self.parameters.pole_pairs
            || self.current.snapshot().maximum_phase_current
                != self.parameters.maximum_phase_current
            || self.pwm_compare.configuration_digest() != self.parameters.configuration_digest
            || self.pwm_compare.counter_clock_hz() != self.pwm_hardware.counter_clock_hz
            || self.pwm_compare.timer_peak_ticks() != self.pwm_hardware.timer_peak_ticks
            || self.pwm_compare.minimum_active_ticks() != self.pwm_hardware.minimum_active_ticks
            || self.pwm_compare.maximum_quantization_error_ulps()
                != self.pwm_hardware.maximum_quantization_error_ulps
            || self.pwm_dead_time_cycles == 0
            || self.pwm_dead_time_cycles >= synchronization.pwm_period_cycles / 2
            || synchronization.minimum_switching_guard_cycles < self.pwm_dead_time_cycles
        {
            return Err(ConfigurationError::FocHardware);
        }
        self.pwm_compare
            .validate_for(&self.parameters, synchronization)
            .map_err(|_| ConfigurationError::FocHardware)?;
        if u64::from(self.pwm_hardware.minimum_active_ticks)
            .checked_mul(u64::from(synchronization.device_cycle_hz))
            .ok_or(ConfigurationError::FocHardware)?
            < u64::from(self.pwm_dead_time_cycles)
                .checked_mul(u64::from(self.pwm_hardware.counter_clock_hz))
                .ok_or(ConfigurationError::FocHardware)?
        {
            return Err(ConfigurationError::FocHardware);
        }
        let (servo_grid, servo, encoder) = lower_servo_profiles(
            self.parameters,
            self.rotor,
            synchronization,
            self.servo_parameters,
            self.encoder_scale_parameters,
            self.encoder_policy_parameters,
        )?;
        if self.servo_grid != servo_grid || self.servo != servo {
            return Err(ConfigurationError::FocServo);
        }
        if self.encoder != encoder {
            return Err(ConfigurationError::FocEncoder);
        }
        Ok(())
    }
}

fn lower_servo_profiles(
    parameters: FocParameterSnapshot,
    rotor: RotorCalibration,
    synchronization: PwmAdcSynchronization,
    servo_parameters: FocServoParameters,
    encoder_scale: FocEncoderScaleParameters,
    encoder_policy: FocEncoderPolicyParameters,
) -> Result<(ServoLoopGrid, ServoCascadeConfig, ServoEncoderProfile), ConfigurationError> {
    servo_parameters.validate_shape()?;
    encoder_scale.validate_shape()?;
    encoder_policy.validate_shape()?;
    if servo_parameters.instance != encoder_scale.instance
        || servo_parameters.instance != encoder_policy.instance
        || encoder_policy.device_cycle_hz != synchronization.device_cycle_hz
    {
        return Err(ConfigurationError::FocEncoder);
    }
    let servo_grid = ServoLoopGrid::new(
        DeviceCycle(0),
        encoder_policy.device_cycle_hz,
        parameters.timing,
    )
    .map_err(|_| ConfigurationError::FocServo)?;
    if encoder_policy.sample_period_cycles != servo_grid.velocity_period_cycles() {
        return Err(ConfigurationError::FocEncoder);
    }
    let servo = ServoCascadeConfig {
        configuration_digest: parameters.configuration_digest,
        position_proportional_gain: servo_parameters.position_proportional_gain,
        velocity_controller: servo_parameters.velocity_controller,
        maximum_velocity: servo_parameters.maximum_velocity,
        maximum_current: servo_parameters.maximum_current,
        direct_current_target: servo_parameters.direct_current_target,
        maximum_following_error_bits: servo_parameters.maximum_following_error_bits,
        maximum_sample_age_cycles: servo_parameters.maximum_sample_age_cycles,
    };
    servo
        .validate_for(servo_grid, &parameters)
        .map_err(|_| ConfigurationError::FocServo)?;
    if encoder_policy.maximum_admitted_velocity != servo.maximum_velocity
        || servo.maximum_sample_age_cycles < encoder_policy.maximum_observation_latency_cycles
    {
        return Err(ConfigurationError::FocServo);
    }
    let encoder = ServoEncoderProfile {
        configuration_digest: parameters.configuration_digest,
        counts_per_mechanical_turn: rotor.counts_per_mechanical_turn,
        count_at_reference: rotor.count_at_reference,
        direction: rotor.direction,
        maximum_count_error: rotor.maximum_count_error,
        position_at_reference: encoder_scale.position_at_reference,
        scale: encoder_scale.scale,
        device_cycle_hz: encoder_policy.device_cycle_hz,
        sample_period_cycles: encoder_policy.sample_period_cycles,
        maximum_observation_latency_cycles: encoder_policy.maximum_observation_latency_cycles,
        maximum_trackable_velocity: encoder_policy.maximum_trackable_velocity,
        maximum_admitted_velocity: encoder_policy.maximum_admitted_velocity,
        maximum_velocity_estimation_error: encoder_policy.maximum_velocity_estimation_error,
        maximum_position_interval_width_ulps: encoder_policy.maximum_position_interval_width_ulps,
        maximum_velocity_interval_width_ulps: encoder_policy.maximum_velocity_interval_width_ulps,
    };
    encoder
        .validate()
        .map_err(|_| ConfigurationError::FocEncoder)?;
    Ok((servo_grid, servo, encoder))
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
        profile.servo.instance,
        profile.encoder_scale.instance,
        profile.encoder_policy.instance,
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

    let (servo_grid, servo, encoder) = lower_servo_profiles(
        parameters,
        rotor,
        synchronization,
        profile.servo,
        profile.encoder_scale,
        profile.encoder_policy,
    )?;

    let lowered = LoweredFocAxisConfiguration {
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
        servo_parameters: profile.servo,
        encoder_scale_parameters: profile.encoder_scale,
        encoder_policy_parameters: profile.encoder_policy,
        servo_grid,
        servo,
        encoder,
    };
    lowered.validate()?;
    Ok(lowered)
}

pub(crate) fn validate_profile(profile: FocAxisProfile) -> Result<(), ConfigurationError> {
    lower_profile(profile, SHAPE_DIGEST).map(|_| ())
}
