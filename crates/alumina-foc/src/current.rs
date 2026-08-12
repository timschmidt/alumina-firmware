use alumina_protocol::{DeviceCycle, Digest};

use super::{AlphaBeta, FocError, FocParameterSnapshot, Phase3, Q30, Q30Interval, clarke};

/// Logical member of one sequential two-channel ADC acquisition.
///
/// This selector describes conversion order only. It does not identify a
/// physical phase until a validated current calibration supplies that mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SequentialAdcChannel {
    Channel0,
    Channel1,
}

/// Request for one software-started, sequential pair of raw ADC conversions.
///
/// The request deliberately carries no PWM token or configuration digest. A
/// result from this path is suitable for commissioning diagnostics and offset
/// collection, but cannot be promoted to [`CurrentSample`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SequentialAdcRequest {
    /// Logical FOC axis whose two routed inputs are selected.
    pub axis: u16,
    /// Nonzero caller token used to correlate the diagnostic result.
    pub token: u32,
    /// Cycle at which software requested the first conversion.
    pub requested_at: DeviceCycle,
}

/// Completed raw pair from a software-started sequential ADC path.
///
/// Completion times bound when software observed each conversion result. They
/// are not sample-and-hold instants, PWM trigger evidence, or switching-edge
/// evidence. Consequently this type cannot be converted into
/// [`PwmAdcSampleStamp`] or [`CurrentSample`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SequentialAdcPair {
    pub axis: u16,
    pub token: u32,
    pub requested_at: DeviceCycle,
    pub channel0_conversion_completed_at: DeviceCycle,
    pub channel1_conversion_completed_at: DeviceCycle,
    pub raw_counts: [u16; 2],
}

/// Deterministic misuse or observation failure in sequential ADC acquisition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SequentialAdcAcquisitionError {
    /// A zero correlation token was supplied.
    ZeroToken,
    /// Another pair is already in progress.
    Busy,
    /// No pair is in progress.
    Idle,
    /// A conversion completed for the channel that is not currently selected.
    ChannelOrder,
    /// A completion observation precedes the request or preceding completion.
    TimeOrder,
    /// A raw conversion exceeds the configured ADC code range.
    RawCount,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingSequentialAdcPair {
    request: SequentialAdcRequest,
    channel0: Option<(u16, DeviceCycle)>,
}

/// Allocation-free ordering owner for software-started two-channel ADC reads.
///
/// The owner admits exactly channel 0 followed by channel 1, checks raw code
/// range and monotonic completion observations, and permits explicit abort.
/// It intentionally models none of the PWM synchronization facts required by
/// [`PwmAdcSynchronization`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SequentialAdcAcquisition {
    adc_maximum_count: u16,
    pending: Option<PendingSequentialAdcPair>,
}

impl SequentialAdcAcquisition {
    /// Constructs an idle sequencer for one nonempty ADC code lattice.
    ///
    /// # Errors
    ///
    /// Returns [`SequentialAdcAcquisitionError::RawCount`] for a zero maximum.
    pub const fn new(adc_maximum_count: u16) -> Result<Self, SequentialAdcAcquisitionError> {
        if adc_maximum_count == 0 {
            Err(SequentialAdcAcquisitionError::RawCount)
        } else {
            Ok(Self {
                adc_maximum_count,
                pending: None,
            })
        }
    }

    /// Starts one pair without starting any hardware conversion itself.
    pub fn begin(
        &mut self,
        request: SequentialAdcRequest,
    ) -> Result<(), SequentialAdcAcquisitionError> {
        if request.token == 0 {
            return Err(SequentialAdcAcquisitionError::ZeroToken);
        }
        if self.pending.is_some() {
            return Err(SequentialAdcAcquisitionError::Busy);
        }
        self.pending = Some(PendingSequentialAdcPair {
            request,
            channel0: None,
        });
        Ok(())
    }

    /// Returns the sole channel whose conversion may currently be polled.
    pub const fn pending_channel(&self) -> Option<SequentialAdcChannel> {
        match self.pending {
            Some(PendingSequentialAdcPair { channel0: None, .. }) => {
                Some(SequentialAdcChannel::Channel0)
            }
            Some(PendingSequentialAdcPair {
                channel0: Some(_), ..
            }) => Some(SequentialAdcChannel::Channel1),
            None => None,
        }
    }

    /// Returns the request retained by an in-progress pair.
    pub const fn pending_request(&self) -> Option<SequentialAdcRequest> {
        match self.pending {
            Some(pending) => Some(pending.request),
            None => None,
        }
    }

    /// Records one completed conversion and returns the pair after channel 1.
    ///
    /// A raw-range or time-order failure aborts the in-progress pair so a
    /// partially invalid observation cannot later complete. Calling with the
    /// wrong channel is treated as caller misuse and leaves the valid pending
    /// conversion selected.
    pub fn record_conversion(
        &mut self,
        channel: SequentialAdcChannel,
        raw_count: u16,
        completed_at: DeviceCycle,
    ) -> Result<Option<SequentialAdcPair>, SequentialAdcAcquisitionError> {
        let Some(mut pending) = self.pending else {
            return Err(SequentialAdcAcquisitionError::Idle);
        };
        let expected = if pending.channel0.is_some() {
            SequentialAdcChannel::Channel1
        } else {
            SequentialAdcChannel::Channel0
        };
        if channel != expected {
            return Err(SequentialAdcAcquisitionError::ChannelOrder);
        }
        if raw_count > self.adc_maximum_count {
            self.pending = None;
            return Err(SequentialAdcAcquisitionError::RawCount);
        }
        let preceding_cycle = pending
            .channel0
            .map_or(pending.request.requested_at, |(_, cycle)| cycle);
        if completed_at.0 < preceding_cycle.0 {
            self.pending = None;
            return Err(SequentialAdcAcquisitionError::TimeOrder);
        }
        match channel {
            SequentialAdcChannel::Channel0 => {
                pending.channel0 = Some((raw_count, completed_at));
                self.pending = Some(pending);
                Ok(None)
            }
            SequentialAdcChannel::Channel1 => {
                let Some((channel0_count, channel0_completed_at)) = pending.channel0 else {
                    return Err(SequentialAdcAcquisitionError::ChannelOrder);
                };
                self.pending = None;
                Ok(Some(SequentialAdcPair {
                    axis: pending.request.axis,
                    token: pending.request.token,
                    requested_at: pending.request.requested_at,
                    channel0_conversion_completed_at: channel0_completed_at,
                    channel1_conversion_completed_at: completed_at,
                    raw_counts: [channel0_count, raw_count],
                }))
            }
        }
    }

    /// Aborts a pending pair and returns its original request, if any.
    pub fn abort(&mut self) -> Option<SequentialAdcRequest> {
        self.pending.take().map(|pending| pending.request)
    }
}

/// Direction in which increasing ADC codes move normalized phase current.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CurrentPolarity {
    Increasing,
    Decreasing,
}

/// One exact affine ADC-code to normalized-current interval mapping.
///
/// The browser derives `normalized_current_per_count` outward from exact analog
/// and machine facts. `maximum_additive_error` then includes every uncertainty
/// not represented by that gain interval: selected-zero rounding, offset drift,
/// ADC quantization/INL/noise, and any admitted analog error. At least half of
/// the upper gain is required so ADC quantization cannot disappear.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CurrentChannelCalibration {
    pub adc_maximum_count: u16,
    pub valid_count_minimum: u16,
    pub valid_count_maximum: u16,
    pub count_at_zero: u16,
    pub polarity: CurrentPolarity,
    pub normalized_current_per_count: Q30Interval,
    pub maximum_additive_error: Q30,
    pub maximum_interval_width_ulps: u32,
}

impl CurrentChannelCalibration {
    /// Validates the complete code window and its worst endpoint intervals.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::CurrentCalibration`] for an empty/out-of-range code
    /// window, a window that admits either ADC rail or fails to bracket zero,
    /// nonpositive gain, insufficient half-count uncertainty, an endpoint
    /// outside normalized `[-1, 1]`, or an interval wider than policy.
    pub fn validate(self) -> Result<(), FocError> {
        if self.adc_maximum_count == 0
            || self.valid_count_minimum == 0
            || self.valid_count_minimum >= self.valid_count_maximum
            || self.valid_count_maximum >= self.adc_maximum_count
            || self.count_at_zero <= self.valid_count_minimum
            || self.count_at_zero >= self.valid_count_maximum
            || self.normalized_current_per_count.lower().bits() <= 0
            || self.normalized_current_per_count.upper().bits() > Q30::ONE.bits()
            || self.maximum_additive_error.bits() < 0
            || self.maximum_interval_width_ulps == 0
        {
            return Err(FocError::CurrentCalibration);
        }
        let minimum_quantization_error =
            (i64::from(self.normalized_current_per_count.upper().bits()) + 1) / 2;
        if i64::from(self.maximum_additive_error.bits()) < minimum_quantization_error {
            return Err(FocError::CurrentCalibration);
        }
        for raw_count in [self.valid_count_minimum, self.valid_count_maximum] {
            let value = self
                .map_unvalidated(raw_count)
                .map_err(|_| FocError::CurrentCalibration)?;
            if !interval_within_limit(value, Q30::ONE)
                || value.width_ulps() > u64::from(self.maximum_interval_width_ulps)
            {
                return Err(FocError::CurrentCalibration);
            }
        }
        Ok(())
    }

    /// Maps one admitted raw ADC code to its conservative normalized interval.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::CurrentRawSample`] outside the declared valid window,
    /// a calibration error for an invalid snapshot, or an arithmetic error.
    pub fn map_raw(self, raw_count: u16) -> Result<Q30Interval, FocError> {
        self.validate()?;
        self.map_unvalidated(raw_count)
    }

    fn map_unvalidated(self, raw_count: u16) -> Result<Q30Interval, FocError> {
        if raw_count < self.valid_count_minimum || raw_count > self.valid_count_maximum {
            return Err(FocError::CurrentRawSample);
        }
        let mut delta = i64::from(raw_count) - i64::from(self.count_at_zero);
        if self.polarity == CurrentPolarity::Decreasing {
            delta = -delta;
        }
        let gain_lower = i64::from(self.normalized_current_per_count.lower().bits());
        let gain_upper = i64::from(self.normalized_current_per_count.upper().bits());
        let (scaled_lower, scaled_upper) = if delta >= 0 {
            (delta * gain_lower, delta * gain_upper)
        } else {
            (delta * gain_upper, delta * gain_lower)
        };
        let error = i64::from(self.maximum_additive_error.bits());
        let value = Q30Interval::new(
            super::q30_from_i64(scaled_lower - error)?,
            super::q30_from_i64(scaled_upper + error)?,
        )?;
        if !interval_within_limit(value, Q30::ONE)
            || value.width_ulps() > u64::from(self.maximum_interval_width_ulps)
        {
            return Err(FocError::CurrentRawSample);
        }
        Ok(value)
    }
}

/// Immutable PWM/ADC timing contract in the firmware's device-cycle domain.
///
/// The dynamic nearest switching edges belong to each sample stamp because
/// their positions depend on the committed duty image. This snapshot bounds
/// trigger jitter, complete acquisition span, interchannel skew, conversion
/// latency, and the required edge-free guard around the acquisition aperture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PwmAdcSynchronization {
    pub configuration_digest: Digest,
    /// Frequency of the boot-local [`DeviceCycle`] lattice.
    pub device_cycle_hz: u32,
    pub pwm_period_cycles: u32,
    pub nominal_acquisition_offset_cycles: u32,
    pub maximum_trigger_jitter_cycles: u32,
    pub maximum_acquisition_cycles: u32,
    pub maximum_channel_skew_cycles: u32,
    pub maximum_conversion_cycles: u32,
    pub minimum_switching_guard_cycles: u32,
}

impl PwmAdcSynchronization {
    /// Checks that every static bound fits inside one PWM period.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::CurrentSynchronization`] for absent identity, empty
    /// bounds, an acquisition that can cross a period boundary, or inconsistent
    /// skew/acquisition limits.
    pub fn validate(self) -> Result<(), FocError> {
        if self.configuration_digest.is_zero()
            || self.device_cycle_hz == 0
            || self.pwm_period_cycles == 0
            || !self.device_cycle_hz.is_multiple_of(self.pwm_period_cycles)
            || self.maximum_acquisition_cycles == 0
            || self.maximum_conversion_cycles == 0
            || self.minimum_switching_guard_cycles == 0
            || self.maximum_channel_skew_cycles > self.maximum_acquisition_cycles
            || self.nominal_acquisition_offset_cycles < self.maximum_trigger_jitter_cycles
        {
            return Err(FocError::CurrentSynchronization);
        }
        let latest_completion = u64::from(self.nominal_acquisition_offset_cycles)
            .checked_add(u64::from(self.maximum_trigger_jitter_cycles))
            .and_then(|value| value.checked_add(u64::from(self.maximum_acquisition_cycles)))
            .and_then(|value| value.checked_add(u64::from(self.maximum_conversion_cycles)))
            .ok_or(FocError::CurrentSynchronization)?;
        if latest_completion > u64::from(self.pwm_period_cycles) {
            return Err(FocError::CurrentSynchronization);
        }
        Ok(())
    }

    /// Validates one hardware-produced timing witness against this snapshot.
    ///
    /// The witness supplies the nearest switching edges around the complete
    /// acquisition aperture. It does not infer them from normalized duties.
    /// The eventual MCPWM backend must generate these facts from the exact
    /// integer compare image and prove them independently in HIL.
    pub fn validate_stamp(self, stamp: PwmAdcSampleStamp) -> Result<(), FocError> {
        self.validate()?;
        if stamp.configuration_digest != self.configuration_digest || stamp.duty_token == 0 {
            return Err(FocError::CurrentSynchronization);
        }
        let period_start = stamp.period_started_at.0;
        let period_end = period_start
            .checked_add(u64::from(self.pwm_period_cycles))
            .ok_or(FocError::CurrentSynchronization)?;
        let acquisition_start = stamp.acquisition_started_at.0;
        let channel0 = stamp.channel0_sampled_at.0;
        let channel1 = stamp.channel1_sampled_at.0;
        let first_sample = channel0.min(channel1);
        let last_sample = channel0.max(channel1);
        let previous_edge = stamp.previous_switching_edge_at.0;
        let next_edge = stamp.next_switching_edge_at.0;
        let conversion_complete = stamp.conversion_completed_at.0;

        if previous_edge < period_start
            || previous_edge > acquisition_start
            || acquisition_start > first_sample
            || last_sample > conversion_complete
            || conversion_complete > period_end
            || next_edge < last_sample
            || next_edge > period_end
        {
            return Err(FocError::CurrentSynchronization);
        }
        let actual_offset = acquisition_start
            .checked_sub(period_start)
            .ok_or(FocError::CurrentSynchronization)?;
        if actual_offset.abs_diff(u64::from(self.nominal_acquisition_offset_cycles))
            > u64::from(self.maximum_trigger_jitter_cycles)
            || last_sample - acquisition_start > u64::from(self.maximum_acquisition_cycles)
            || channel0.abs_diff(channel1) > u64::from(self.maximum_channel_skew_cycles)
            || conversion_complete - last_sample > u64::from(self.maximum_conversion_cycles)
            || acquisition_start - previous_edge < u64::from(self.minimum_switching_guard_cycles)
            || next_edge - last_sample < u64::from(self.minimum_switching_guard_cycles)
        {
            return Err(FocError::CurrentSynchronization);
        }
        Ok(())
    }
}

/// Boot-local evidence correlating two ADC apertures with one committed PWM period.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PwmAdcSampleStamp {
    pub configuration_digest: Digest,
    pub duty_token: u32,
    pub period_sequence: u32,
    pub period_started_at: DeviceCycle,
    pub previous_switching_edge_at: DeviceCycle,
    pub acquisition_started_at: DeviceCycle,
    pub channel0_sampled_at: DeviceCycle,
    pub channel1_sampled_at: DeviceCycle,
    pub conversion_completed_at: DeviceCycle,
    pub next_switching_edge_at: DeviceCycle,
}

impl PwmAdcSampleStamp {
    /// Inclusive cycle window containing both sample-and-hold observations.
    pub const fn sample_window(self) -> (DeviceCycle, DeviceCycle) {
        if self.channel0_sampled_at.0 <= self.channel1_sampled_at.0 {
            (self.channel0_sampled_at, self.channel1_sampled_at)
        } else {
            (self.channel1_sampled_at, self.channel0_sampled_at)
        }
    }
}

/// Physical phase pair represented by channel zero followed by channel one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TwoShuntPhasePair {
    /// Channel zero is phase A and channel one is phase B; C is reconstructed.
    Ab,
    /// Channel zero is phase B and channel one is phase C; A is reconstructed.
    Bc,
    /// Channel zero is phase C and channel one is phase A; B is reconstructed.
    Ca,
}

/// Complete immutable two-shunt calibration and synchronization snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TwoShuntCurrentCalibration {
    pub configuration_digest: Digest,
    pub phase_pair: TwoShuntPhasePair,
    pub channel0: CurrentChannelCalibration,
    pub channel1: CurrentChannelCalibration,
    pub synchronization: PwmAdcSynchronization,
    /// Maximum normalized phase-current change during one device cycle.
    pub maximum_normalized_current_slew_per_cycle: Q30,
    /// Slew-derived error that refers both channels to their whole sample window.
    pub maximum_interchannel_skew_error: Q30,
    pub maximum_phase_current: Q30,
    pub maximum_phase_interval_width_ulps: u32,
}

impl TwoShuntCurrentCalibration {
    /// Validates identity, channel maps, skew accounting, and all endpoint pairs.
    pub fn validate(self) -> Result<(), FocError> {
        if self.configuration_digest.is_zero()
            || self.configuration_digest != self.synchronization.configuration_digest
            || self.channel0.adc_maximum_count != self.channel1.adc_maximum_count
            || self.maximum_normalized_current_slew_per_cycle.bits() < 0
            || self.maximum_normalized_current_slew_per_cycle > Q30::ONE
            || self.maximum_interchannel_skew_error.bits() < 0
            || self.maximum_interchannel_skew_error > Q30::ONE
            || self.maximum_phase_current <= Q30::ZERO
            || self.maximum_phase_current > Q30::ONE
            || self.maximum_phase_interval_width_ulps == 0
            || self.synchronization.maximum_channel_skew_cycles > 0
                && self.maximum_interchannel_skew_error == Q30::ZERO
        {
            return Err(FocError::CurrentCalibration);
        }
        self.channel0.validate()?;
        self.channel1.validate()?;
        self.synchronization.validate()?;
        let required_skew_error = i64::from(self.maximum_normalized_current_slew_per_cycle.bits())
            .checked_mul(i64::from(self.synchronization.maximum_channel_skew_cycles))
            .ok_or(FocError::CurrentCalibration)?;
        if required_skew_error > i64::from(self.maximum_interchannel_skew_error.bits()) {
            return Err(FocError::CurrentCalibration);
        }
        for raw0 in [
            self.channel0.valid_count_minimum,
            self.channel0.valid_count_maximum,
        ] {
            for raw1 in [
                self.channel1.valid_count_minimum,
                self.channel1.valid_count_maximum,
            ] {
                self.derive_phases(raw0, raw1)
                    .map_err(|_| FocError::CurrentCalibration)?;
            }
        }
        Ok(())
    }

    /// Produces the sole sample-capable form after full snapshot validation.
    pub fn validated(self) -> Result<ValidatedTwoShuntCurrentCalibration, FocError> {
        self.validate()?;
        Ok(ValidatedTwoShuntCurrentCalibration { inner: self })
    }

    fn derive_phases(self, raw0: u16, raw1: u16) -> Result<(Phase3, Q30), FocError> {
        let first = widen_interval(
            self.channel0.map_unvalidated(raw0)?,
            self.maximum_interchannel_skew_error,
        )?;
        let second = widen_interval(
            self.channel1.map_unvalidated(raw1)?,
            self.maximum_interchannel_skew_error,
        )?;
        let phases = match self.phase_pair {
            TwoShuntPhasePair::Ab => Phase3 {
                a: first,
                b: second,
                c: first.checked_add(second)?.checked_neg()?,
            },
            TwoShuntPhasePair::Bc => Phase3 {
                a: first.checked_add(second)?.checked_neg()?,
                b: first,
                c: second,
            },
            TwoShuntPhasePair::Ca => Phase3 {
                a: second,
                b: first.checked_add(second)?.checked_neg()?,
                c: first,
            },
        };
        for phase in [phases.a, phases.b, phases.c] {
            if !interval_within_limit(phase, self.maximum_phase_current)
                || phase.width_ulps() > u64::from(self.maximum_phase_interval_width_ulps)
            {
                return Err(FocError::CurrentRawSample);
            }
        }
        let maximum_zero_sequence = zero_sequence_bound(phases)?;
        Ok((phases, maximum_zero_sequence))
    }
}

/// Validated immutable calibration used by the fixed-rate acquisition owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedTwoShuntCurrentCalibration {
    inner: TwoShuntCurrentCalibration,
}

impl ValidatedTwoShuntCurrentCalibration {
    /// Returns the immutable source snapshot retained by this proof wrapper.
    pub const fn snapshot(self) -> TwoShuntCurrentCalibration {
        self.inner
    }

    /// Derives one canonical bounded sample from raw codes and timing evidence.
    pub fn observe(
        self,
        raw_counts: [u16; 2],
        synchronization: PwmAdcSampleStamp,
    ) -> Result<CurrentSample, FocError> {
        self.inner.synchronization.validate_stamp(synchronization)?;
        let (phases, maximum_zero_sequence) =
            self.inner.derive_phases(raw_counts[0], raw_counts[1])?;
        Ok(CurrentSample {
            configuration_digest: self.inner.configuration_digest,
            raw_counts,
            synchronization,
            phases,
            maximum_zero_sequence,
        })
    }
}

/// Canonical bounded two-shunt phase-current observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CurrentSample {
    pub configuration_digest: Digest,
    pub raw_counts: [u16; 2],
    pub synchronization: PwmAdcSampleStamp,
    pub phases: Phase3,
    /// Independent-box residual admitted when the reconstructed phase loses correlation.
    pub maximum_zero_sequence: Q30,
}

impl CurrentSample {
    /// Replays raw conversion/timing and binds this sample to the FOC snapshot.
    pub fn validate_for(
        self,
        snapshot: &FocParameterSnapshot,
        calibration: ValidatedTwoShuntCurrentCalibration,
    ) -> Result<(), FocError> {
        snapshot.validate()?;
        let source = calibration.snapshot();
        if self.configuration_digest.is_zero()
            || self.configuration_digest != snapshot.configuration_digest
            || self.configuration_digest != source.configuration_digest
            || source.maximum_phase_current != snapshot.maximum_phase_current
            || u64::from(snapshot.timing.pwm_hz)
                .checked_mul(u64::from(source.synchronization.pwm_period_cycles))
                != Some(u64::from(source.synchronization.device_cycle_hz))
        {
            return Err(FocError::Configuration);
        }
        if calibration.observe(self.raw_counts, self.synchronization)? != self {
            return Err(FocError::CurrentSample);
        }
        Ok(())
    }

    /// Clarke-transforms the observation using its replayable residual bound.
    pub fn stationary(self) -> Result<AlphaBeta, FocError> {
        clarke(self.phases, self.maximum_zero_sequence)
    }

    /// Inclusive device-cycle window containing both channel observations.
    pub const fn sample_window(self) -> (DeviceCycle, DeviceCycle) {
        self.synchronization.sample_window()
    }
}

fn widen_interval(value: Q30Interval, error: Q30) -> Result<Q30Interval, FocError> {
    if error < Q30::ZERO {
        return Err(FocError::CurrentCalibration);
    }
    Q30Interval::new(
        value.lower().checked_sub(error)?,
        value.upper().checked_add(error)?,
    )
}

fn interval_within_limit(value: Q30Interval, limit: Q30) -> bool {
    if limit < Q30::ZERO {
        return false;
    }
    let Ok(negative_limit) = limit.checked_neg() else {
        return false;
    };
    value.lower() >= negative_limit && value.upper() <= limit
}

fn zero_sequence_bound(phases: Phase3) -> Result<Q30, FocError> {
    let lower = i64::from(phases.a.lower().bits())
        + i64::from(phases.b.lower().bits())
        + i64::from(phases.c.lower().bits());
    let upper = i64::from(phases.a.upper().bits())
        + i64::from(phases.b.upper().bits())
        + i64::from(phases.c.upper().bits());
    super::q30_from_i64(
        i64::try_from(lower.unsigned_abs().max(upper.unsigned_abs()))
            .map_err(|_| FocError::Range)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DqCurrentController, FocTimingProfile, PiConfig};

    const DIGEST: Digest = Digest([0x5a; 32]);

    fn channel(polarity: CurrentPolarity) -> CurrentChannelCalibration {
        CurrentChannelCalibration {
            adc_maximum_count: 4_095,
            valid_count_minimum: 1_200,
            valid_count_maximum: 2_800,
            count_at_zero: 2_000,
            polarity,
            normalized_current_per_count: Q30Interval::point(Q30::from_bits(1 << 19)),
            maximum_additive_error: Q30::from_bits(1 << 18),
            maximum_interval_width_ulps: 1 << 19,
        }
    }

    fn synchronization() -> PwmAdcSynchronization {
        PwmAdcSynchronization {
            configuration_digest: DIGEST,
            device_cycle_hz: 20_000_000,
            pwm_period_cycles: 1_000,
            nominal_acquisition_offset_cycles: 500,
            maximum_trigger_jitter_cycles: 2,
            maximum_acquisition_cycles: 20,
            maximum_channel_skew_cycles: 8,
            maximum_conversion_cycles: 40,
            minimum_switching_guard_cycles: 50,
        }
    }

    fn stamp() -> PwmAdcSampleStamp {
        PwmAdcSampleStamp {
            configuration_digest: DIGEST,
            duty_token: 7,
            period_sequence: 11,
            period_started_at: DeviceCycle(10_000),
            previous_switching_edge_at: DeviceCycle(10_400),
            acquisition_started_at: DeviceCycle(10_501),
            channel0_sampled_at: DeviceCycle(10_506),
            channel1_sampled_at: DeviceCycle(10_512),
            conversion_completed_at: DeviceCycle(10_532),
            next_switching_edge_at: DeviceCycle(10_600),
        }
    }

    fn calibration(pair: TwoShuntPhasePair) -> TwoShuntCurrentCalibration {
        TwoShuntCurrentCalibration {
            configuration_digest: DIGEST,
            phase_pair: pair,
            channel0: channel(CurrentPolarity::Increasing),
            channel1: channel(CurrentPolarity::Increasing),
            synchronization: synchronization(),
            maximum_normalized_current_slew_per_cycle: Q30::from_bits(1 << 14),
            maximum_interchannel_skew_error: Q30::from_bits(1 << 17),
            maximum_phase_current: Q30::ONE,
            maximum_phase_interval_width_ulps: 1 << 22,
        }
    }

    fn parameter_snapshot() -> FocParameterSnapshot {
        let pi = PiConfig {
            proportional_gain: Q30::ZERO,
            integral_gain_per_update: Q30::ZERO,
            integral_minimum: Q30::NEG_ONE,
            integral_maximum: Q30::ONE,
            output_minimum: Q30::from_bits(-Q30::HALF.bits()),
            output_maximum: Q30::HALF,
        };
        FocParameterSnapshot {
            configuration_digest: DIGEST,
            pole_pairs: 7,
            timing: FocTimingProfile {
                pwm_hz: 20_000,
                current_loop_hz: 20_000,
                velocity_loop_divider: 20,
                position_loop_divider: 10,
            },
            maximum_phase_current: Q30::ONE,
            maximum_phase_voltage: Q30::ONE,
            d_current: pi,
            q_current: pi,
        }
    }

    #[test]
    fn sequential_adc_owner_admits_exactly_one_ordered_pair() {
        let request = SequentialAdcRequest {
            axis: 1,
            token: 9,
            requested_at: DeviceCycle(1_000),
        };
        let mut acquisition = SequentialAdcAcquisition::new(4_095).unwrap();
        assert_eq!(acquisition.pending_channel(), None);
        assert_eq!(acquisition.begin(request), Ok(()));
        assert_eq!(
            acquisition.pending_channel(),
            Some(SequentialAdcChannel::Channel0)
        );
        assert_eq!(
            acquisition.record_conversion(
                SequentialAdcChannel::Channel0,
                2_001,
                DeviceCycle(1_004),
            ),
            Ok(None)
        );
        assert_eq!(
            acquisition.pending_channel(),
            Some(SequentialAdcChannel::Channel1)
        );
        assert_eq!(
            acquisition.record_conversion(
                SequentialAdcChannel::Channel1,
                1_999,
                DeviceCycle(1_009),
            ),
            Ok(Some(SequentialAdcPair {
                axis: 1,
                token: 9,
                requested_at: DeviceCycle(1_000),
                channel0_conversion_completed_at: DeviceCycle(1_004),
                channel1_conversion_completed_at: DeviceCycle(1_009),
                raw_counts: [2_001, 1_999],
            }))
        );
        assert_eq!(acquisition.pending_channel(), None);
    }

    #[test]
    fn sequential_adc_owner_rejects_overlap_order_and_zero_token() {
        let request = SequentialAdcRequest {
            axis: 0,
            token: 5,
            requested_at: DeviceCycle(20),
        };
        let mut acquisition = SequentialAdcAcquisition::new(4_095).unwrap();
        let mut zero = request;
        zero.token = 0;
        assert_eq!(
            acquisition.begin(zero),
            Err(SequentialAdcAcquisitionError::ZeroToken)
        );
        acquisition.begin(request).unwrap();
        assert_eq!(
            acquisition.begin(request),
            Err(SequentialAdcAcquisitionError::Busy)
        );
        assert_eq!(
            acquisition.record_conversion(SequentialAdcChannel::Channel1, 2_000, DeviceCycle(21),),
            Err(SequentialAdcAcquisitionError::ChannelOrder)
        );
        assert_eq!(acquisition.abort(), Some(request));
        assert_eq!(acquisition.abort(), None);
    }

    #[test]
    fn sequential_adc_owner_aborts_invalid_observations() {
        let request = SequentialAdcRequest {
            axis: 0,
            token: 6,
            requested_at: DeviceCycle(20),
        };
        assert_eq!(
            SequentialAdcAcquisition::new(0),
            Err(SequentialAdcAcquisitionError::RawCount)
        );
        let mut acquisition = SequentialAdcAcquisition::new(4_095).unwrap();
        acquisition.begin(request).unwrap();
        assert_eq!(
            acquisition.record_conversion(SequentialAdcChannel::Channel0, 4_096, DeviceCycle(21),),
            Err(SequentialAdcAcquisitionError::RawCount)
        );
        assert_eq!(acquisition.pending_channel(), None);

        acquisition.begin(request).unwrap();
        assert_eq!(
            acquisition.record_conversion(SequentialAdcChannel::Channel0, 2_000, DeviceCycle(19),),
            Err(SequentialAdcAcquisitionError::TimeOrder)
        );
        assert_eq!(acquisition.pending_channel(), None);

        acquisition.begin(request).unwrap();
        acquisition
            .record_conversion(SequentialAdcChannel::Channel0, 2_000, DeviceCycle(21))
            .unwrap();
        assert_eq!(
            acquisition.record_conversion(SequentialAdcChannel::Channel1, 2_000, DeviceCycle(20),),
            Err(SequentialAdcAcquisitionError::TimeOrder)
        );
        assert_eq!(acquisition.pending_channel(), None);
    }

    #[test]
    fn channel_mapping_retains_half_count_uncertainty_and_polarity() {
        let increasing = channel(CurrentPolarity::Increasing);
        increasing.validate().unwrap();
        assert_eq!(
            increasing.map_raw(2_000).unwrap(),
            Q30Interval::new(Q30::from_bits(-(1 << 18)), Q30::from_bits(1 << 18)).unwrap()
        );
        assert_eq!(
            increasing.map_raw(2_001).unwrap(),
            Q30Interval::new(Q30::from_bits(1 << 18), Q30::from_bits(3 << 18)).unwrap()
        );
        let decreasing = channel(CurrentPolarity::Decreasing);
        assert_eq!(decreasing.map_raw(1_999), increasing.map_raw(2_001));
        assert_eq!(increasing.map_raw(1_199), Err(FocError::CurrentRawSample));
    }

    #[test]
    fn channel_rejects_missing_quantization_and_overwide_policy() {
        let mut invalid = channel(CurrentPolarity::Increasing);
        invalid.maximum_additive_error = Q30::from_bits((1 << 18) - 1);
        assert_eq!(invalid.validate(), Err(FocError::CurrentCalibration));
        invalid = channel(CurrentPolarity::Increasing);
        invalid.maximum_interval_width_ulps = (1 << 19) - 1;
        assert_eq!(invalid.validate(), Err(FocError::CurrentCalibration));
        invalid = channel(CurrentPolarity::Increasing);
        invalid.valid_count_minimum = 0;
        assert_eq!(invalid.validate(), Err(FocError::CurrentCalibration));
        invalid = channel(CurrentPolarity::Increasing);
        invalid.count_at_zero = invalid.valid_count_maximum;
        assert_eq!(invalid.validate(), Err(FocError::CurrentCalibration));
    }

    #[test]
    fn timing_witness_binds_jitter_skew_conversion_and_switching_guards() {
        let contract = synchronization();
        assert_eq!(contract.validate_stamp(stamp()), Ok(()));

        let mut invalid = stamp();
        invalid.acquisition_started_at = DeviceCycle(10_503);
        assert_eq!(
            contract.validate_stamp(invalid),
            Err(FocError::CurrentSynchronization)
        );
        invalid = stamp();
        invalid.channel1_sampled_at = DeviceCycle(10_515);
        assert_eq!(
            contract.validate_stamp(invalid),
            Err(FocError::CurrentSynchronization)
        );
        invalid = stamp();
        invalid.next_switching_edge_at = DeviceCycle(10_550);
        assert_eq!(
            contract.validate_stamp(invalid),
            Err(FocError::CurrentSynchronization)
        );
        invalid = stamp();
        invalid.conversion_completed_at = DeviceCycle(10_553);
        assert_eq!(
            contract.validate_stamp(invalid),
            Err(FocError::CurrentSynchronization)
        );
    }

    #[test]
    fn timing_snapshot_rejects_static_period_overrun_and_foreign_identity() {
        let mut invalid = synchronization();
        invalid.nominal_acquisition_offset_cycles = 950;
        assert_eq!(invalid.validate(), Err(FocError::CurrentSynchronization));
        invalid = synchronization();
        invalid.maximum_channel_skew_cycles = 21;
        assert_eq!(invalid.validate(), Err(FocError::CurrentSynchronization));
        invalid = synchronization();
        invalid.device_cycle_hz += 1;
        assert_eq!(invalid.validate(), Err(FocError::CurrentSynchronization));
        let mut foreign = stamp();
        foreign.configuration_digest = Digest([0x33; 32]);
        assert_eq!(
            synchronization().validate_stamp(foreign),
            Err(FocError::CurrentSynchronization)
        );
    }

    #[test]
    fn two_shunt_sample_reconstructs_each_named_phase_pair() {
        let raw = [2_100, 1_950];
        let ab = calibration(TwoShuntPhasePair::Ab)
            .validated()
            .unwrap()
            .observe(raw, stamp())
            .unwrap();
        let bc = calibration(TwoShuntPhasePair::Bc)
            .validated()
            .unwrap()
            .observe(raw, stamp())
            .unwrap();
        let ca = calibration(TwoShuntPhasePair::Ca)
            .validated()
            .unwrap()
            .observe(raw, stamp())
            .unwrap();
        assert_eq!(ab.phases.a, bc.phases.b);
        assert_eq!(ab.phases.b, bc.phases.c);
        assert_eq!(bc.phases.a, ca.phases.b);
        assert_eq!(ca.phases.c, ab.phases.a);
        assert!(ab.maximum_zero_sequence.bits() > 0);
        assert!(ab.stationary().is_ok());
        assert_eq!(
            ab.sample_window(),
            (DeviceCycle(10_506), DeviceCycle(10_512))
        );
    }

    #[test]
    fn validated_sample_replays_against_parameter_identity() {
        let calibration = calibration(TwoShuntPhasePair::Ab).validated().unwrap();
        let sample = calibration.observe([2_040, 1_980], stamp()).unwrap();
        let snapshot = parameter_snapshot();
        assert_eq!(sample.validate_for(&snapshot, calibration), Ok(()));
        assert!(DqCurrentController::from_snapshot(&snapshot).is_ok());

        let mut forged = sample;
        forged.phases.a = Q30Interval::ZERO;
        assert_eq!(
            forged.validate_for(&snapshot, calibration),
            Err(FocError::CurrentSample)
        );
        let mut foreign = snapshot;
        foreign.configuration_digest = Digest([0x99; 32]);
        assert_eq!(
            sample.validate_for(&foreign, calibration),
            Err(FocError::Configuration)
        );
        let mut wrong_rate = snapshot;
        wrong_rate.timing.pwm_hz = 10_000;
        wrong_rate.timing.current_loop_hz = 10_000;
        assert_eq!(
            sample.validate_for(&wrong_rate, calibration),
            Err(FocError::Configuration)
        );
    }

    #[test]
    fn skew_requires_nonzero_current_error_and_endpoint_proof() {
        let mut invalid = calibration(TwoShuntPhasePair::Ab);
        invalid.maximum_interchannel_skew_error = Q30::ZERO;
        assert_eq!(invalid.validate(), Err(FocError::CurrentCalibration));

        invalid = calibration(TwoShuntPhasePair::Ab);
        invalid.maximum_normalized_current_slew_per_cycle = Q30::from_bits((1 << 14) + 1);
        assert_eq!(invalid.validate(), Err(FocError::CurrentCalibration));

        invalid = calibration(TwoShuntPhasePair::Ab);
        invalid.maximum_phase_current = Q30::from_bits(1 << 28);
        assert_eq!(invalid.validate(), Err(FocError::CurrentCalibration));
    }

    #[test]
    fn raw_rail_and_bad_edge_stamp_fail_before_sample_creation() {
        let calibration = calibration(TwoShuntPhasePair::Ab).validated().unwrap();
        assert_eq!(
            calibration.observe([0, 2_000], stamp()),
            Err(FocError::CurrentRawSample)
        );
        let mut invalid = stamp();
        invalid.previous_switching_edge_at = DeviceCycle(10_460);
        assert_eq!(
            calibration.observe([2_000, 2_000], invalid),
            Err(FocError::CurrentSynchronization)
        );
    }
}
