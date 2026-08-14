//! Deterministic replay of the exact configured FOC hardware boundary.
//!
//! This simulator converts integer compare edges into the device-cycle domain,
//! constructs synchronized two-shunt samples, executes the portable current
//! controller, and commits the next complete compare image. It is not an
//! electrical motor model and makes no physical timing or energization claim.

use alumina_config::{ConfigurationError, LoweredFocAxisConfiguration, RealtimeConfiguration};
use alumina_foc::{
    AlphaBeta, CurrentSample, DqControlUpdate, DqCurrentController, DqInterval, DqPoint,
    FocCurrentCommand, FocError, ModulationResult, PwmAdcSampleStamp, PwmAdcSynchronization,
    PwmCompareError, PwmCompareImage, PwmCompareLatch, PwmCompareLatchError, PwmCompareLatchOwner,
    Q30Interval, RotorSample, inverse_park, park, space_vector_modulate,
};
use alumina_protocol::DeviceCycle;

const INITIAL_DUTY_TOKEN: u32 = 1;
const INITIAL_COMMAND_ID: u32 = 1;

/// Configuration, timing, acquisition, or replay rejection in the virtual loop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocHardwareLoopError {
    Configuration(ConfigurationError),
    Foc(FocError),
    Compare(PwmCompareError),
    Latch(PwmCompareLatchError),
    /// The first simulator supports one current update per PWM period.
    Rate,
    /// Command identity, time, token, or sequence was not the sole expected value.
    Sequence,
    /// A counter edge did not map to an integer device cycle.
    ClockDomain,
    /// Device-cycle or sequence arithmetic overflowed.
    Overflow,
    /// A prior rejection terminally closed this virtual owner.
    Faulted,
}

impl From<ConfigurationError> for FocHardwareLoopError {
    fn from(error: ConfigurationError) -> Self {
        Self::Configuration(error)
    }
}

impl From<FocError> for FocHardwareLoopError {
    fn from(error: FocError) -> Self {
        Self::Foc(error)
    }
}

impl From<PwmCompareError> for FocHardwareLoopError {
    fn from(error: PwmCompareError) -> Self {
        Self::Compare(error)
    }
}

impl From<PwmCompareLatchError> for FocHardwareLoopError {
    fn from(error: PwmCompareLatchError) -> Self {
        Self::Latch(error)
    }
}

/// One exact command plus the integer sensor observations for its PWM period.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocHardwareLoopInput {
    pub command: FocCurrentCommand,
    pub raw_current_counts: [u16; 2],
    pub raw_rotor_count: u32,
}

/// Complete replayable result of one virtual hardware/current-loop transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocHardwareLoopSample {
    pub input: FocHardwareLoopInput,
    pub active_image: PwmCompareImage,
    pub current: CurrentSample,
    pub rotor: RotorSample,
    pub measured_interval: DqInterval,
    pub measured: DqPoint,
    pub control: DqControlUpdate,
    pub modulation: ModulationResult,
    pub committed: PwmCompareLatch,
}

/// Allocation-free virtual owner for one configuration-derived FOC axis.
#[derive(Debug, Eq, PartialEq)]
pub struct ConfiguredFocHardwareLoop {
    lowered: LoweredFocAxisConfiguration,
    controller: DqCurrentController,
    compare_owner: PwmCompareLatchOwner,
    period_started_at: DeviceCycle,
    period_sequence: u32,
    next_duty_token: u32,
    next_command_id: u32,
    faulted: bool,
}

impl ConfiguredFocHardwareLoop {
    /// Constructs only from the private, independently validated configuration container.
    pub fn from_configuration(
        configuration: &RealtimeConfiguration,
        slot: usize,
        first_boundary: DeviceCycle,
    ) -> Result<Self, FocHardwareLoopError> {
        let lowered = configuration.lower_foc_axis(slot)?;
        Self::from_lowered(lowered, first_boundary)
    }

    fn from_lowered(
        lowered: LoweredFocAxisConfiguration,
        first_boundary: DeviceCycle,
    ) -> Result<Self, FocHardwareLoopError> {
        lowered.validate()?;
        let parameters = lowered.parameters;
        let synchronization = lowered.current.snapshot().synchronization;
        if parameters.timing.current_loop_hz != parameters.timing.pwm_hz {
            return Err(FocHardwareLoopError::Rate);
        }
        let _next_boundary = first_boundary
            .0
            .checked_add(u64::from(synchronization.pwm_period_cycles))
            .ok_or(FocHardwareLoopError::Overflow)?;

        let mut compare_owner = PwmCompareLatchOwner::new(lowered.pwm_compare, first_boundary)?;
        let neutral = space_vector_modulate(
            AlphaBeta {
                alpha: Q30Interval::ZERO,
                beta: Q30Interval::ZERO,
            },
            parameters.maximum_phase_voltage,
        )?;
        let image = lowered.pwm_compare.lower(
            &parameters,
            synchronization,
            INITIAL_DUTY_TOKEN,
            first_boundary,
            neutral.duties,
        )?;
        compare_owner.stage(image, &parameters, synchronization)?;
        let initial = compare_owner
            .observe_timer_zero(first_boundary)?
            .ok_or(FocHardwareLoopError::Sequence)?;
        let period_sequence =
            u32::try_from(initial.period_sequence).map_err(|_| FocHardwareLoopError::Overflow)?;

        Ok(Self {
            lowered,
            controller: DqCurrentController::from_snapshot(&parameters)?,
            compare_owner,
            period_started_at: first_boundary,
            period_sequence,
            next_duty_token: INITIAL_DUTY_TOKEN + 1,
            next_command_id: INITIAL_COMMAND_ID,
            faulted: false,
        })
    }

    /// Exact configuration-derived facts retained by this virtual owner.
    pub const fn lowered(&self) -> LoweredFocAxisConfiguration {
        self.lowered
    }

    /// Start of the active PWM period and required command schedule.
    pub const fn period_started_at(&self) -> DeviceCycle {
        self.period_started_at
    }

    /// Whether any prior loop, edge, timing, or sequence rejection latched closed.
    pub const fn is_faulted(&self) -> bool {
        self.faulted
    }

    /// Executes one period and observes the exact next timer-zero boundary.
    pub fn step(
        &mut self,
        input: FocHardwareLoopInput,
    ) -> Result<FocHardwareLoopSample, FocHardwareLoopError> {
        let observed = self.compare_owner.next_boundary();
        self.step_with_timer_zero(input, observed)
    }

    /// Executes one period with an explicit timer-zero observation for fault injection.
    pub fn step_with_timer_zero(
        &mut self,
        input: FocHardwareLoopInput,
        observed_timer_zero: DeviceCycle,
    ) -> Result<FocHardwareLoopSample, FocHardwareLoopError> {
        if self.faulted {
            return Err(FocHardwareLoopError::Faulted);
        }
        let result = self.step_inner(input, observed_timer_zero);
        if result.is_err() {
            self.faulted = true;
        }
        result
    }

    fn step_inner(
        &mut self,
        input: FocHardwareLoopInput,
        observed_timer_zero: DeviceCycle,
    ) -> Result<FocHardwareLoopSample, FocHardwareLoopError> {
        let parameters = self.lowered.parameters;
        let synchronization = self.lowered.current.snapshot().synchronization;
        if input.command.command_id != self.next_command_id
            || input.command.scheduled_at != self.period_started_at
        {
            return Err(FocHardwareLoopError::Sequence);
        }
        input.command.validate(&parameters)?;
        let next_command_id = self
            .next_command_id
            .checked_add(1)
            .ok_or(FocHardwareLoopError::Overflow)?;
        let next_duty_token = self
            .next_duty_token
            .checked_add(1)
            .ok_or(FocHardwareLoopError::Overflow)?;
        let active_image = self
            .compare_owner
            .active_image()
            .ok_or(FocHardwareLoopError::Sequence)?;
        let stamp = exact_sample_stamp(
            self.lowered.pwm_compare,
            &parameters,
            synchronization,
            active_image,
            self.period_sequence,
            self.period_started_at,
        )?;
        let current = self
            .lowered
            .current
            .observe(input.raw_current_counts, stamp)?;
        current.validate_for(&parameters, self.lowered.current)?;
        let rotor = self.lowered.rotor.observe(
            input.raw_rotor_count,
            stamp.channel0_sampled_at,
            self.lowered.rotation_precision,
        )?;
        rotor.validate_for(&parameters, self.lowered.rotation_precision)?;
        let measured_interval = park(current.stationary()?, rotor.rotation)?;
        let measured = DqPoint {
            d: measured_interval.d.midpoint(),
            q: measured_interval.q.midpoint(),
        };
        let mut next_controller = self.controller;
        let control = next_controller.update(
            input.command.target,
            measured,
            input.command.voltage_feed_forward,
        )?;
        let stationary_voltage = inverse_park(
            DqInterval {
                d: Q30Interval::point(control.voltage.d),
                q: Q30Interval::point(control.voltage.q),
            },
            rotor.rotation,
        )?;
        let modulation =
            space_vector_modulate(stationary_voltage, parameters.maximum_phase_voltage)?;
        let scheduled_at = self.compare_owner.next_boundary();
        let image = self.lowered.pwm_compare.lower(
            &parameters,
            synchronization,
            self.next_duty_token,
            scheduled_at,
            modulation.duties,
        )?;
        self.compare_owner
            .stage(image, &parameters, synchronization)?;
        let committed = self
            .compare_owner
            .observe_timer_zero(observed_timer_zero)?
            .ok_or(FocHardwareLoopError::Sequence)?;
        let period_sequence =
            u32::try_from(committed.period_sequence).map_err(|_| FocHardwareLoopError::Overflow)?;

        self.controller = next_controller;
        self.period_started_at = committed.observed_at;
        self.period_sequence = period_sequence;
        self.next_duty_token = next_duty_token;
        self.next_command_id = next_command_id;
        Ok(FocHardwareLoopSample {
            input,
            active_image,
            current,
            rotor,
            measured_interval,
            measured,
            control,
            modulation,
            committed,
        })
    }
}

fn exact_sample_stamp(
    contract: alumina_foc::PwmCompareContract,
    parameters: &alumina_foc::FocParameterSnapshot,
    synchronization: PwmAdcSynchronization,
    active_image: PwmCompareImage,
    period_sequence: u32,
    period_started_at: DeviceCycle,
) -> Result<PwmAdcSampleStamp, FocHardwareLoopError> {
    active_image.validate_for(contract, parameters, synchronization)?;
    let acquisition_offset = u64::from(synchronization.nominal_acquisition_offset_cycles);
    let channel_skew = u64::from(synchronization.maximum_channel_skew_cycles != 0);
    let channel0_offset = acquisition_offset;
    let channel1_offset = acquisition_offset
        .checked_add(channel_skew)
        .ok_or(FocHardwareLoopError::Overflow)?;
    let last_sample_offset = channel0_offset.max(channel1_offset);
    let conversion_offset = last_sample_offset
        .checked_add(1)
        .ok_or(FocHardwareLoopError::Overflow)?;

    let mut previous_offset = None;
    let mut next_offset = None;
    for comparison in active_image.comparisons() {
        for edge_ticks in [
            comparison.first_switching_edge_ticks(),
            comparison.second_switching_edge_ticks(),
        ] {
            let offset = counter_ticks_to_device_cycles(contract, edge_ticks)?;
            if offset <= acquisition_offset {
                previous_offset =
                    Some(previous_offset.map_or(offset, |prior: u64| prior.max(offset)));
            }
            if offset >= last_sample_offset {
                next_offset = Some(next_offset.map_or(offset, |next: u64| next.min(offset)));
            }
        }
    }
    let previous_offset = previous_offset.ok_or(FocHardwareLoopError::ClockDomain)?;
    let next_offset = next_offset.ok_or(FocHardwareLoopError::ClockDomain)?;
    let stamp = PwmAdcSampleStamp {
        configuration_digest: contract.configuration_digest(),
        duty_token: active_image.token(),
        period_sequence,
        period_started_at,
        previous_switching_edge_at: add_cycles(period_started_at, previous_offset)?,
        acquisition_started_at: add_cycles(period_started_at, acquisition_offset)?,
        channel0_sampled_at: add_cycles(period_started_at, channel0_offset)?,
        channel1_sampled_at: add_cycles(period_started_at, channel1_offset)?,
        conversion_completed_at: add_cycles(period_started_at, conversion_offset)?,
        next_switching_edge_at: add_cycles(period_started_at, next_offset)?,
    };
    synchronization.validate_stamp(stamp)?;
    Ok(stamp)
}

fn counter_ticks_to_device_cycles(
    contract: alumina_foc::PwmCompareContract,
    edge_ticks: u32,
) -> Result<u64, FocHardwareLoopError> {
    let numerator = u64::from(edge_ticks)
        .checked_mul(u64::from(contract.device_cycle_hz()))
        .ok_or(FocHardwareLoopError::Overflow)?;
    let divisor = u64::from(contract.counter_clock_hz());
    if !numerator.is_multiple_of(divisor) {
        return Err(FocHardwareLoopError::ClockDomain);
    }
    Ok(numerator / divisor)
}

fn add_cycles(base: DeviceCycle, offset: u64) -> Result<DeviceCycle, FocHardwareLoopError> {
    base.0
        .checked_add(offset)
        .map(DeviceCycle)
        .ok_or(FocHardwareLoopError::Overflow)
}

#[cfg(test)]
mod tests {
    use alumina_config::{
        FactEvidence, FocAdcAttenuation, FocAdcFrontendParameters, FocCurrentChannel,
        FocEncoderPolicyParameters, FocEncoderScaleParameters, FocPwmHardwareParameters,
        FocServoParameters,
    };
    use alumina_foc::{
        CountUncertainty, CurrentChannelCalibration, CurrentPolarity, ElectricalPhase,
        FocParameterSnapshot, FocTimingProfile, PiConfig, PwmAdcSynchronization,
        PwmCompareContract, Q30, RotationPrecision, RotorCalibration, RotorCountDirection,
        ServoCascadeConfig, ServoEncoderProfile, ServoEncoderScale, ServoLoopGrid, ServoPosition,
        TwoShuntCurrentCalibration, TwoShuntPhasePair,
    };
    use alumina_protocol::Digest;

    use super::*;

    const DIGEST: Digest = Digest([0x94; 32]);
    const ACCEPTED_COMPARE_ERROR_ULPS: u32 = 1_200_000;

    fn point(numerator: i64, denominator: i64) -> Q30 {
        Q30Interval::from_ratio(numerator, denominator)
            .unwrap()
            .midpoint()
    }

    fn current_channel() -> CurrentChannelCalibration {
        CurrentChannelCalibration {
            adc_maximum_count: 4_095,
            valid_count_minimum: 1_200,
            valid_count_maximum: 2_800,
            count_at_zero: 2_000,
            polarity: CurrentPolarity::Increasing,
            normalized_current_per_count: Q30Interval::point(Q30::from_bits(1 << 19)),
            maximum_additive_error: Q30::from_bits(1 << 18),
            maximum_interval_width_ulps: 1 << 19,
        }
    }

    fn lowered_fixture() -> LoweredFocAxisConfiguration {
        lowered_with_lattices(
            80_000_000,
            4_000,
            80_000_000,
            2_000,
            8,
            8,
            ACCEPTED_COMPARE_ERROR_ULPS,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "the test fixture exposes each independent timer and precision fact"
    )]
    fn lowered_with_lattices(
        device_cycle_hz: u32,
        pwm_period_cycles: u32,
        counter_clock_hz: u32,
        timer_peak_ticks: u16,
        minimum_active_ticks: u16,
        dead_time_cycles: u32,
        maximum_quantization_error_ulps: u32,
    ) -> LoweredFocAxisConfiguration {
        let pwm_hz = device_cycle_hz / pwm_period_cycles;
        let controller = PiConfig {
            proportional_gain: Q30::HALF,
            integral_gain_per_update: point(1, 32),
            integral_minimum: Q30::from_bits(-Q30::HALF.bits()),
            integral_maximum: Q30::HALF,
            output_minimum: Q30::from_bits(-Q30::HALF.bits()),
            output_maximum: Q30::HALF,
        };
        let parameters = FocParameterSnapshot {
            configuration_digest: DIGEST,
            pole_pairs: 7,
            timing: FocTimingProfile {
                pwm_hz,
                current_loop_hz: pwm_hz,
                velocity_loop_divider: 20,
                position_loop_divider: 10,
            },
            maximum_phase_current: Q30::ONE,
            maximum_phase_voltage: Q30::ONE,
            d_current: controller,
            q_current: controller,
        };
        let rotor = RotorCalibration {
            configuration_digest: DIGEST,
            counts_per_mechanical_turn: 4_096,
            count_at_reference: 0,
            electrical_phase_at_reference: ElectricalPhase::ZERO,
            pole_pairs: 7,
            direction: RotorCountDirection::Increasing,
            maximum_alignment_error_bits: 1_024,
            maximum_count_error: CountUncertainty::new(1, 2).unwrap(),
        };
        let synchronization = if pwm_period_cycles == 4_000 {
            PwmAdcSynchronization {
                configuration_digest: DIGEST,
                device_cycle_hz,
                pwm_period_cycles,
                nominal_acquisition_offset_cycles: 2_000,
                maximum_trigger_jitter_cycles: 2,
                maximum_acquisition_cycles: 20,
                maximum_channel_skew_cycles: 8,
                maximum_conversion_cycles: 40,
                minimum_switching_guard_cycles: 50,
            }
        } else {
            PwmAdcSynchronization {
                configuration_digest: DIGEST,
                device_cycle_hz,
                pwm_period_cycles,
                nominal_acquisition_offset_cycles: pwm_period_cycles / 2,
                maximum_trigger_jitter_cycles: 0,
                maximum_acquisition_cycles: 1,
                maximum_channel_skew_cycles: 0,
                maximum_conversion_cycles: 1,
                minimum_switching_guard_cycles: 1,
            }
        };
        let current = TwoShuntCurrentCalibration {
            configuration_digest: DIGEST,
            phase_pair: TwoShuntPhasePair::Ab,
            channel0: current_channel(),
            channel1: current_channel(),
            synchronization,
            maximum_normalized_current_slew_per_cycle: Q30::from_bits(1 << 14),
            maximum_interchannel_skew_error: if synchronization.maximum_channel_skew_cycles == 0 {
                Q30::ZERO
            } else {
                Q30::from_bits(1 << 17)
            },
            maximum_phase_current: Q30::ONE,
            maximum_phase_interval_width_ulps: 1 << 22,
        }
        .validated()
        .unwrap();
        let pwm_hardware = FocPwmHardwareParameters {
            instance: 0,
            peripheral_source_clock_hz: counter_clock_hz,
            counter_clock_hz,
            timer_peak_ticks,
            minimum_active_ticks,
            maximum_quantization_error_ulps,
            peripheral_prescaler: 0,
            timer_prescaler: 0,
            evidence: FactEvidence::Qualified,
        };
        let pwm_compare = PwmCompareContract::new(
            DIGEST,
            device_cycle_hz,
            pwm_period_cycles,
            counter_clock_hz,
            timer_peak_ticks,
            minimum_active_ticks,
            maximum_quantization_error_ulps,
        )
        .unwrap();
        let servo_grid =
            ServoLoopGrid::new(DeviceCycle(0), device_cycle_hz, parameters.timing).unwrap();
        let servo_parameters = FocServoParameters {
            instance: 0,
            position_proportional_gain: Q30::ZERO,
            velocity_controller: controller,
            maximum_velocity: Q30::ONE,
            maximum_current: Q30::ONE,
            direct_current_target: Q30::ZERO,
            maximum_following_error_bits: u64::MAX,
            maximum_sample_age_cycles: servo_grid.velocity_period_cycles(),
        };
        let servo = ServoCascadeConfig {
            configuration_digest: DIGEST,
            position_proportional_gain: servo_parameters.position_proportional_gain,
            velocity_controller: servo_parameters.velocity_controller,
            maximum_velocity: servo_parameters.maximum_velocity,
            maximum_current: servo_parameters.maximum_current,
            direct_current_target: servo_parameters.direct_current_target,
            maximum_following_error_bits: servo_parameters.maximum_following_error_bits,
            maximum_sample_age_cycles: servo_parameters.maximum_sample_age_cycles,
        };
        let encoder_scale_parameters = FocEncoderScaleParameters {
            instance: 0,
            position_at_reference: ServoPosition::ZERO,
            scale: ServoEncoderScale::new(1, 1, 1, 1).unwrap(),
            evidence: FactEvidence::Measured,
        };
        let encoder_policy_parameters = FocEncoderPolicyParameters {
            instance: 0,
            device_cycle_hz,
            sample_period_cycles: servo_grid.velocity_period_cycles(),
            maximum_observation_latency_cycles: 0,
            maximum_trackable_velocity: Q30::ONE,
            maximum_admitted_velocity: Q30::ONE,
            maximum_velocity_estimation_error: Q30::from_bits(1),
            maximum_position_interval_width_ulps: u64::MAX,
            maximum_velocity_interval_width_ulps: u32::MAX,
            evidence: FactEvidence::Qualified,
        };
        let encoder = ServoEncoderProfile {
            configuration_digest: DIGEST,
            counts_per_mechanical_turn: rotor.counts_per_mechanical_turn,
            count_at_reference: rotor.count_at_reference,
            direction: rotor.direction,
            maximum_count_error: rotor.maximum_count_error,
            position_at_reference: encoder_scale_parameters.position_at_reference,
            scale: encoder_scale_parameters.scale,
            device_cycle_hz: encoder_policy_parameters.device_cycle_hz,
            sample_period_cycles: encoder_policy_parameters.sample_period_cycles,
            maximum_observation_latency_cycles: encoder_policy_parameters
                .maximum_observation_latency_cycles,
            maximum_trackable_velocity: encoder_policy_parameters.maximum_trackable_velocity,
            maximum_admitted_velocity: encoder_policy_parameters.maximum_admitted_velocity,
            maximum_velocity_estimation_error: encoder_policy_parameters
                .maximum_velocity_estimation_error,
            maximum_position_interval_width_ulps: encoder_policy_parameters
                .maximum_position_interval_width_ulps,
            maximum_velocity_interval_width_ulps: encoder_policy_parameters
                .maximum_velocity_interval_width_ulps,
        };
        let lowered = LoweredFocAxisConfiguration {
            instance: 0,
            parameters,
            rotor,
            rotation_precision: RotationPrecision {
                maximum_component_width_ulps: 12_000_000,
                maximum_norm_error_ulps: 12_000_000,
            },
            current,
            pwm_dead_time_cycles: dead_time_cycles,
            adc_channel0: FocAdcFrontendParameters {
                instance: 0,
                channel: FocCurrentChannel::Channel0,
                attenuation: FocAdcAttenuation::Db11,
                evidence: FactEvidence::Qualified,
            },
            adc_channel1: FocAdcFrontendParameters {
                instance: 0,
                channel: FocCurrentChannel::Channel1,
                attenuation: FocAdcAttenuation::Db11,
                evidence: FactEvidence::Qualified,
            },
            pwm_hardware,
            pwm_compare,
            servo_parameters,
            encoder_scale_parameters,
            encoder_policy_parameters,
            servo_grid,
            servo,
            encoder,
        };
        lowered.validate().unwrap();
        lowered
    }

    fn input(period: DeviceCycle, command_id: u32) -> FocHardwareLoopInput {
        FocHardwareLoopInput {
            command: FocCurrentCommand {
                command_id,
                scheduled_at: period,
                configuration_digest: DIGEST,
                target: DqPoint {
                    d: Q30::ZERO,
                    q: point(1, 4),
                },
                voltage_feed_forward: DqPoint::default(),
            },
            raw_current_counts: [2_000, 2_000],
            raw_rotor_count: 0,
        }
    }

    #[test]
    fn configured_integer_loop_replays_current_angle_control_and_compare_images() {
        let lowered = lowered_fixture();
        let start = DeviceCycle(8_000);
        let mut first = ConfiguredFocHardwareLoop::from_lowered(lowered, start).unwrap();
        let mut replay = ConfiguredFocHardwareLoop::from_lowered(lowered, start).unwrap();
        let mut observed_maximum_error = 0;

        for command_id in 1..=8 {
            let command = input(first.period_started_at(), command_id);
            let sample = first.step(command).unwrap();
            let replayed = replay.step(command).unwrap();
            assert_eq!(sample, replayed);
            assert_eq!(sample.active_image.token(), command_id);
            assert_eq!(sample.committed.image.token(), command_id + 1);
            assert_eq!(
                sample.committed.observed_at.0,
                sample.current.synchronization.period_started_at.0 + 4_000
            );
            assert_eq!(sample.current.synchronization.duty_token, command_id);
            sample
                .current
                .validate_for(&lowered.parameters, lowered.current)
                .unwrap();
            sample
                .rotor
                .validate_for(&lowered.parameters, lowered.rotation_precision)
                .unwrap();
            sample
                .committed
                .image
                .validate_for(
                    lowered.pwm_compare,
                    &lowered.parameters,
                    lowered.current.snapshot().synchronization,
                )
                .unwrap();
            observed_maximum_error = observed_maximum_error.max(
                sample
                    .committed
                    .image
                    .comparisons()
                    .map(|value| value.maximum_error_ulps())
                    .into_iter()
                    .max()
                    .unwrap(),
            );
        }
        assert!(observed_maximum_error > 1_000_000);
        assert!(observed_maximum_error <= ACCEPTED_COMPARE_ERROR_ULPS);
        assert!(!first.is_faulted());
        assert_eq!(first.period_started_at(), DeviceCycle(40_000));
    }

    #[test]
    fn configured_precision_policy_rejects_an_overwide_control_result() {
        let lowered = lowered_with_lattices(80_000_000, 4_000, 80_000_000, 2_000, 8, 8, 1_000_000);
        let start = DeviceCycle(8_000);
        let mut simulator = ConfiguredFocHardwareLoop::from_lowered(lowered, start).unwrap();
        simulator.step(input(start, 1)).unwrap();
        let next = simulator.period_started_at();
        assert_eq!(
            simulator.step(input(next, 2)),
            Err(FocHardwareLoopError::Compare(PwmCompareError::Quantization))
        );
        assert!(simulator.is_faulted());
        assert_eq!(
            simulator.step(input(next, 2)),
            Err(FocHardwareLoopError::Faulted)
        );
    }

    #[test]
    fn timer_zero_and_raw_sample_faults_latch_before_reuse() {
        let lowered = lowered_fixture();
        let start = DeviceCycle(8_000);
        let mut boundary = ConfiguredFocHardwareLoop::from_lowered(lowered, start).unwrap();
        assert_eq!(
            boundary.step_with_timer_zero(input(start, 1), DeviceCycle(12_001)),
            Err(FocHardwareLoopError::Latch(PwmCompareLatchError::Boundary))
        );
        assert!(boundary.is_faulted());
        assert_eq!(
            boundary.step(input(start, 1)),
            Err(FocHardwareLoopError::Faulted)
        );

        let mut raw = ConfiguredFocHardwareLoop::from_lowered(lowered, start).unwrap();
        let mut invalid = input(start, 1);
        invalid.raw_current_counts[0] = 0;
        assert_eq!(
            raw.step(invalid),
            Err(FocHardwareLoopError::Foc(FocError::CurrentRawSample))
        );
        assert!(raw.is_faulted());
        assert_eq!(raw.period_started_at(), start);
    }

    #[test]
    fn stale_bundle_rate_and_fractional_edge_cycle_are_rejected_exactly() {
        let mut stale = lowered_fixture();
        stale.pwm_hardware.timer_peak_ticks = 1_999;
        assert_eq!(
            ConfiguredFocHardwareLoop::from_lowered(stale, DeviceCycle(0)),
            Err(FocHardwareLoopError::Configuration(
                ConfigurationError::FocHardware
            ))
        );

        let mut slower = lowered_fixture();
        slower.parameters.timing.current_loop_hz = 10_000;
        slower.servo_grid = ServoLoopGrid::new(
            DeviceCycle(0),
            slower.encoder_policy_parameters.device_cycle_hz,
            slower.parameters.timing,
        )
        .unwrap();
        slower.encoder_policy_parameters.sample_period_cycles =
            slower.servo_grid.velocity_period_cycles();
        slower.encoder.sample_period_cycles = slower.servo_grid.velocity_period_cycles();
        slower.validate().unwrap();
        assert_eq!(
            ConfiguredFocHardwareLoop::from_lowered(slower, DeviceCycle(0)),
            Err(FocHardwareLoopError::Rate)
        );

        let fractional = lowered_with_lattices(100, 10, 80, 4, 1, 1, u32::MAX);
        let mut simulator =
            ConfiguredFocHardwareLoop::from_lowered(fractional, DeviceCycle(100)).unwrap();
        assert_eq!(
            simulator.step(input(DeviceCycle(100), 1)),
            Err(FocHardwareLoopError::ClockDomain)
        );
        assert!(simulator.is_faulted());
    }
}
