use alumina_protocol::{DeviceCycle, Digest};

use super::{
    Duty3, FocParameterSnapshot, PowerStageCommit, PwmAdcSynchronization, Q30_SCALE, Q30Interval,
    round_ties_even,
};

/// Maximum number of physical PWM stages correlated by one commit barrier.
pub const MAX_PWM_COMMIT_BANK_AXES: usize = 4;
/// Portable upper bound reserved for a four-axis commit barrier.
pub const MAX_PWM_COMMIT_BANK_BARRIER_BYTES: usize = 1_024;
/// Portable upper bound reserved for one four-axis completion value.
pub const MAX_PWM_COMMIT_BANK_COMPLETION_BYTES: usize = 256;

/// Rejection while lowering exact duty intervals onto an integer PWM lattice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PwmCompareError {
    /// Static clock, period, pulse-width, identity, or precision facts are invalid.
    Contract,
    /// The contract does not describe the supplied FOC/current snapshot.
    Configuration,
    /// A duty interval lies outside the normalized inclusive `[0, 1]` domain.
    DutyRange,
    /// The selected comparison would violate the minimum active or inactive pulse.
    PulseWidth,
    /// The complete interval-to-lattice error exceeds configured policy.
    Quantization,
    /// A zero command correlation token was supplied.
    ZeroToken,
    /// A retained image does not replay to its canonical lowering.
    Image,
}

/// Immutable relationship between device cycles and one center-aligned timer.
///
/// `timer_peak_ticks` is the up/down timer's period register. One complete PWM
/// period therefore contains exactly `2 * timer_peak_ticks` counter ticks. The
/// counter and device-cycle clocks may differ; no timestamp conversion is
/// inferred merely because both complete one PWM period exactly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PwmCompareContract {
    configuration_digest: Digest,
    device_cycle_hz: u32,
    pwm_period_device_cycles: u32,
    counter_clock_hz: u32,
    timer_peak_ticks: u16,
    minimum_active_ticks: u16,
    maximum_quantization_error_ulps: u32,
}

impl PwmCompareContract {
    /// Constructs and validates one exact clock/compare policy.
    #[allow(
        clippy::too_many_arguments,
        reason = "every independent hardware fact is explicit"
    )]
    pub fn new(
        configuration_digest: Digest,
        device_cycle_hz: u32,
        pwm_period_device_cycles: u32,
        counter_clock_hz: u32,
        timer_peak_ticks: u16,
        minimum_active_ticks: u16,
        maximum_quantization_error_ulps: u32,
    ) -> Result<Self, PwmCompareError> {
        let contract = Self {
            configuration_digest,
            device_cycle_hz,
            pwm_period_device_cycles,
            counter_clock_hz,
            timer_peak_ticks,
            minimum_active_ticks,
            maximum_quantization_error_ulps,
        };
        contract.validate()?;
        Ok(contract)
    }

    /// Checks exact device/PWM/counter ratios and a nonempty pulse domain.
    pub fn validate(self) -> Result<(), PwmCompareError> {
        if self.configuration_digest.is_zero()
            || self.device_cycle_hz == 0
            || self.pwm_period_device_cycles == 0
            || self.counter_clock_hz == 0
            || self.timer_peak_ticks < 3
            || self.minimum_active_ticks == 0
            || u32::from(self.minimum_active_ticks) * 2 >= u32::from(self.timer_peak_ticks)
            || self.maximum_quantization_error_ulps == 0
            || !self
                .device_cycle_hz
                .is_multiple_of(self.pwm_period_device_cycles)
        {
            return Err(PwmCompareError::Contract);
        }
        let pwm_hz = self.device_cycle_hz / self.pwm_period_device_cycles;
        if u64::from(pwm_hz)
            .checked_mul(2)
            .and_then(|value| value.checked_mul(u64::from(self.timer_peak_ticks)))
            != Some(u64::from(self.counter_clock_hz))
        {
            return Err(PwmCompareError::Contract);
        }
        Ok(())
    }

    /// Revalidates identity and both exact PWM-period representations.
    pub fn validate_for(
        self,
        parameters: &FocParameterSnapshot,
        synchronization: PwmAdcSynchronization,
    ) -> Result<(), PwmCompareError> {
        self.validate()?;
        parameters
            .validate()
            .map_err(|_| PwmCompareError::Configuration)?;
        synchronization
            .validate()
            .map_err(|_| PwmCompareError::Configuration)?;
        if self.configuration_digest != parameters.configuration_digest
            || self.configuration_digest != synchronization.configuration_digest
            || self.device_cycle_hz != synchronization.device_cycle_hz
            || self.pwm_period_device_cycles != synchronization.pwm_period_cycles
            || self.pwm_hz() != parameters.timing.pwm_hz
        {
            return Err(PwmCompareError::Configuration);
        }
        Ok(())
    }

    /// Configuration identity shared with the canonical FOC snapshot.
    pub const fn configuration_digest(self) -> Digest {
        self.configuration_digest
    }

    /// Frequency of the boot-local device-cycle lattice.
    pub const fn device_cycle_hz(self) -> u32 {
        self.device_cycle_hz
    }

    /// Complete PWM period expressed in device cycles.
    pub const fn pwm_period_device_cycles(self) -> u32 {
        self.pwm_period_device_cycles
    }

    /// Center-aligned counter-tick frequency after all timer prescalers.
    pub const fn counter_clock_hz(self) -> u32 {
        self.counter_clock_hz
    }

    /// Up/down timer peak and maximum comparison value.
    pub const fn timer_peak_ticks(self) -> u16 {
        self.timer_peak_ticks
    }

    /// Minimum permitted high and low duration, in counter ticks per half-period.
    pub const fn minimum_active_ticks(self) -> u16 {
        self.minimum_active_ticks
    }

    /// Maximum whole-Q2.30-ULP error from the complete input interval.
    pub const fn maximum_quantization_error_ulps(self) -> u32 {
        self.maximum_quantization_error_ulps
    }

    /// Exact PWM frequency implied by the validated device-cycle facts.
    pub const fn pwm_hz(self) -> u32 {
        self.device_cycle_hz / self.pwm_period_device_cycles
    }

    /// Lowers one complete duty image using midpoint selection and ties-to-even.
    ///
    /// The selected compare minimizes maximum distance to the two Q2.30
    /// interval endpoints on the integer lattice. `maximum_error_ulps` retains
    /// both input interval width and hardware quantization; it is never merely
    /// a half-tick estimate.
    pub fn lower(
        self,
        parameters: &FocParameterSnapshot,
        synchronization: PwmAdcSynchronization,
        token: u32,
        scheduled_at: DeviceCycle,
        duties: Duty3,
    ) -> Result<PwmCompareImage, PwmCompareError> {
        self.validate_for(parameters, synchronization)?;
        if token == 0 {
            return Err(PwmCompareError::ZeroToken);
        }
        Ok(PwmCompareImage {
            configuration_digest: self.configuration_digest,
            token,
            scheduled_at,
            requested_duties: duties,
            a: self.lower_one(duties.a)?,
            b: self.lower_one(duties.b)?,
            c: self.lower_one(duties.c)?,
        })
    }

    fn lower_one(self, duty: Q30Interval) -> Result<PwmCompareValue, PwmCompareError> {
        let lower = i128::from(duty.lower().bits());
        let upper = i128::from(duty.upper().bits());
        let scale = i128::from(Q30_SCALE);
        if lower < 0 || upper > scale {
            return Err(PwmCompareError::DutyRange);
        }
        let peak = i128::from(self.timer_peak_ticks);
        let compare = round_ties_even((lower + upper) * peak, 2 * scale);
        let compare = u16::try_from(compare).map_err(|_| PwmCompareError::DutyRange)?;
        if compare < self.minimum_active_ticks
            || compare > self.timer_peak_ticks - self.minimum_active_ticks
        {
            return Err(PwmCompareError::PulseWidth);
        }

        let represented_numerator = i128::from(compare) * scale;
        let lower_distance = (represented_numerator - lower * peak).abs();
        let upper_distance = (represented_numerator - upper * peak).abs();
        let maximum_error_ulps = div_ceil_nonnegative(lower_distance.max(upper_distance), peak);
        let maximum_error_ulps =
            u32::try_from(maximum_error_ulps).map_err(|_| PwmCompareError::Quantization)?;
        if maximum_error_ulps > self.maximum_quantization_error_ulps {
            return Err(PwmCompareError::Quantization);
        }

        let represented_duty =
            Q30Interval::from_ratio(i64::from(compare), i64::from(self.timer_peak_ticks))
                .map_err(|_| PwmCompareError::Quantization)?;
        Ok(PwmCompareValue {
            compare_ticks: compare,
            represented_duty,
            maximum_error_ulps,
            first_switching_edge_ticks: u32::from(compare),
            second_switching_edge_ticks: 2 * u32::from(self.timer_peak_ticks) - u32::from(compare),
        })
    }
}

/// One selected comparison and its replayable center-aligned consequences.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PwmCompareValue {
    compare_ticks: u16,
    represented_duty: Q30Interval,
    maximum_error_ulps: u32,
    first_switching_edge_ticks: u32,
    second_switching_edge_ticks: u32,
}

impl PwmCompareValue {
    pub const fn compare_ticks(self) -> u16 {
        self.compare_ticks
    }

    /// Outward Q2.30 enclosure of the exact `compare / timer_peak` ratio.
    pub const fn represented_duty(self) -> Q30Interval {
        self.represented_duty
    }

    /// Maximum distance from either requested interval endpoint, in Q2.30 ULPs.
    pub const fn maximum_error_ulps(self) -> u32 {
        self.maximum_error_ulps
    }

    /// First compare edge after timer zero, in counter ticks.
    pub const fn first_switching_edge_ticks(self) -> u32 {
        self.first_switching_edge_ticks
    }

    /// Mirrored compare edge before the next timer zero, in counter ticks.
    pub const fn second_switching_edge_ticks(self) -> u32 {
        self.second_switching_edge_ticks
    }
}

/// Complete canonical three-phase compare image staged for a future boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PwmCompareImage {
    configuration_digest: Digest,
    token: u32,
    scheduled_at: DeviceCycle,
    requested_duties: Duty3,
    a: PwmCompareValue,
    b: PwmCompareValue,
    c: PwmCompareValue,
}

impl PwmCompareImage {
    pub const fn configuration_digest(self) -> Digest {
        self.configuration_digest
    }

    pub const fn token(self) -> u32 {
        self.token
    }

    pub const fn scheduled_at(self) -> DeviceCycle {
        self.scheduled_at
    }

    pub const fn requested_duties(self) -> Duty3 {
        self.requested_duties
    }

    pub const fn comparisons(self) -> [PwmCompareValue; 3] {
        [self.a, self.b, self.c]
    }

    /// Replays the complete interval lowering and requires exact image equality.
    pub fn validate_for(
        self,
        contract: PwmCompareContract,
        parameters: &FocParameterSnapshot,
        synchronization: PwmAdcSynchronization,
    ) -> Result<(), PwmCompareError> {
        let replay = contract.lower(
            parameters,
            synchronization,
            self.token,
            self.scheduled_at,
            self.requested_duties,
        )?;
        if replay != self {
            return Err(PwmCompareError::Image);
        }
        Ok(())
    }
}

/// Fail-closed staging or timer-boundary error for a complete compare image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PwmCompareLatchError {
    /// The contract or complete image failed exact replay.
    Compare(PwmCompareError),
    /// One complete image already awaits its boundary.
    Busy,
    /// The requested boundary precedes the next observable zero or is off-grid.
    Schedule,
    /// Timer-zero observations were missing, duplicated, or out of order.
    Boundary,
    /// The device-cycle boundary cannot advance without overflow.
    CycleOverflow,
    /// A prior terminal error has latched this owner.
    Faulted,
}

/// Observation that one complete image became active at one timer-zero boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PwmCompareLatch {
    /// Monotonic zero-based sequence since owner construction.
    pub period_sequence: u64,
    /// Exact observed timer-zero cycle; equal to the image schedule.
    pub observed_at: DeviceCycle,
    /// Complete replayable image latched at this boundary.
    pub image: PwmCompareImage,
}

/// Allocation-free complete-image owner for an exact timer-zero sequence.
///
/// This is a portable scheduling model, not proof of a hardware register
/// write. It accepts at most one future image, requires every observed zero in
/// exact device-cycle order, and latches a terminal fault on image, schedule,
/// boundary, or arithmetic failure. Missing images are permitted so a prior
/// comparison can remain active; a higher-level control-rate policy may require
/// one image per period.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PwmCompareLatchOwner {
    contract: PwmCompareContract,
    next_boundary: DeviceCycle,
    next_period_sequence: u64,
    pending: Option<PwmCompareImage>,
    active: Option<PwmCompareImage>,
    faulted: bool,
}

impl PwmCompareLatchOwner {
    /// Starts an owner whose first accepted zero is `first_boundary`.
    pub fn new(
        contract: PwmCompareContract,
        first_boundary: DeviceCycle,
    ) -> Result<Self, PwmCompareLatchError> {
        contract.validate().map_err(PwmCompareLatchError::Compare)?;
        Ok(Self {
            contract,
            next_boundary: first_boundary,
            next_period_sequence: 0,
            pending: None,
            active: None,
            faulted: false,
        })
    }

    /// Validates and retains one complete future image.
    ///
    /// `Busy` is retryable and leaves the existing image intact. Every other
    /// rejection is terminal because it indicates an identity or schedule
    /// disagreement at the sole hardware-owner boundary.
    pub fn stage(
        &mut self,
        image: PwmCompareImage,
        parameters: &FocParameterSnapshot,
        synchronization: PwmAdcSynchronization,
    ) -> Result<(), PwmCompareLatchError> {
        if self.faulted {
            return Err(PwmCompareLatchError::Faulted);
        }
        if self.pending.is_some() {
            return Err(PwmCompareLatchError::Busy);
        }
        if let Err(error) = image.validate_for(self.contract, parameters, synchronization) {
            return self.fail(PwmCompareLatchError::Compare(error));
        }
        let Some(offset) = image.scheduled_at.0.checked_sub(self.next_boundary.0) else {
            return self.fail(PwmCompareLatchError::Schedule);
        };
        if !offset.is_multiple_of(u64::from(self.contract.pwm_period_device_cycles())) {
            return self.fail(PwmCompareLatchError::Schedule);
        }
        self.pending = Some(image);
        Ok(())
    }

    /// Observes the next exact timer-zero boundary and optionally latches an image.
    pub fn observe_timer_zero(
        &mut self,
        observed_at: DeviceCycle,
    ) -> Result<Option<PwmCompareLatch>, PwmCompareLatchError> {
        if self.faulted {
            return Err(PwmCompareLatchError::Faulted);
        }
        if observed_at != self.next_boundary {
            return self.fail(PwmCompareLatchError::Boundary);
        }
        let Some(next_boundary) = self
            .next_boundary
            .0
            .checked_add(u64::from(self.contract.pwm_period_device_cycles()))
        else {
            return self.fail(PwmCompareLatchError::CycleOverflow);
        };
        let Some(next_period_sequence) = self.next_period_sequence.checked_add(1) else {
            return self.fail(PwmCompareLatchError::CycleOverflow);
        };
        if self
            .pending
            .is_some_and(|image| image.scheduled_at.0 < observed_at.0)
        {
            return self.fail(PwmCompareLatchError::Schedule);
        }
        let latched = if self
            .pending
            .is_some_and(|image| image.scheduled_at == observed_at)
        {
            let Some(image) = self.pending.take() else {
                return self.fail(PwmCompareLatchError::Boundary);
            };
            self.active = Some(image);
            Some(PwmCompareLatch {
                period_sequence: self.next_period_sequence,
                observed_at,
                image,
            })
        } else {
            None
        };
        self.next_boundary = DeviceCycle(next_boundary);
        self.next_period_sequence = next_period_sequence;
        Ok(latched)
    }

    /// Next timer-zero cycle that must be observed exactly once.
    pub const fn next_boundary(&self) -> DeviceCycle {
        self.next_boundary
    }

    /// Complete image currently modeled as active, if one has latched.
    pub const fn active_image(&self) -> Option<PwmCompareImage> {
        self.active
    }

    /// Whether a terminal mismatch has latched the owner closed.
    pub const fn is_faulted(&self) -> bool {
        self.faulted
    }

    fn fail<T>(&mut self, error: PwmCompareLatchError) -> Result<T, PwmCompareLatchError> {
        self.pending = None;
        self.faulted = true;
        Err(error)
    }
}

/// First-cause rejection at a simultaneous physical PWM-commit boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PwmCommitBarrierError {
    /// The const-generic bank width is zero or exceeds the supported bound.
    AxisCount,
    /// The barrier has no usable configuration identity or PWM period.
    Configuration,
    /// A staged image carries a foreign or absent configuration identity.
    ImageConfiguration { axis: usize },
    /// One complete image bank already awaits physical observations.
    Busy,
    /// A staged image does not select the sole next common timer-zero boundary.
    Schedule { axis: usize },
    /// The enclosing owner closed a different timer-zero boundary.
    Boundary,
    /// No image bank exists for the supplied observation or completion action.
    Sequence,
    /// A physical observation selected an axis outside the configured bank.
    AxisRange { axis: usize },
    /// One physical stage reported the same boundary more than once.
    DuplicateObservation { axis: usize },
    /// A stage report substituted its token, schedule, or exact observed boundary.
    Observation { axis: usize },
    /// The boundary closed without one stage's exact latch report.
    MissingObservation { axis: usize },
    /// The next boundary or physical-period sequence cannot be represented.
    CycleOverflow,
    /// The enclosing target completed its separately qualified safe transaction.
    SafetyInvalidated,
    /// A prior terminal error has closed the barrier.
    Faulted,
}

/// Complete simultaneous physical acknowledgement released exactly once.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PwmCommitBankCompletion<const AXES: usize> {
    configuration_digest: Digest,
    period_sequence: u64,
    observed_at: DeviceCycle,
    commits: [PowerStageCommit; AXES],
}

impl<const AXES: usize> PwmCommitBankCompletion<AXES> {
    /// Immutable configuration identity admitted by the barrier.
    pub const fn configuration_digest(self) -> Digest {
        self.configuration_digest
    }

    /// Common physical-period sequence accepted by this barrier.
    pub const fn period_sequence(self) -> u64 {
        self.period_sequence
    }

    /// Exact common timer-zero boundary observed by every stage.
    pub const fn observed_at(self) -> DeviceCycle {
        self.observed_at
    }

    /// Complete physical commit vector in physical-axis order.
    pub const fn commits(self) -> [PowerStageCommit; AXES] {
        self.commits
    }
}

/// Fixed-memory correlation barrier for simultaneous PWM stage commits.
///
/// The barrier does not write a compare register and does not claim that two
/// physical timers are synchronized. A target first stages the complete image
/// vector, writes those exact images through its separately qualified owners,
/// records each physical latch report in any order, and closes the boundary
/// only after servicing all stage status. No partial commit vector is exposed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PwmCommitBankBarrier<const AXES: usize> {
    configuration_digest: Digest,
    pwm_period_cycles: u32,
    next_boundary: DeviceCycle,
    next_period_sequence: u64,
    staged: Option<[PwmCompareImage; AXES]>,
    observations: [Option<PowerStageCommit>; AXES],
    fault: Option<PwmCommitBarrierError>,
}

impl<const AXES: usize> PwmCommitBankBarrier<AXES> {
    /// Creates an empty barrier for the sole next common physical boundary.
    pub fn new(
        configuration_digest: Digest,
        pwm_period_cycles: u32,
        next_boundary: DeviceCycle,
        next_period_sequence: u64,
    ) -> Result<Self, PwmCommitBarrierError> {
        if AXES == 0 || AXES > MAX_PWM_COMMIT_BANK_AXES {
            return Err(PwmCommitBarrierError::AxisCount);
        }
        if configuration_digest.is_zero() || pwm_period_cycles == 0 {
            return Err(PwmCommitBarrierError::Configuration);
        }
        Ok(Self {
            configuration_digest,
            pwm_period_cycles,
            next_boundary,
            next_period_sequence,
            staged: None,
            observations: [None; AXES],
            fault: None,
        })
    }

    /// Transactionally retains one complete future physical image vector.
    ///
    /// `Busy` is retryable and preserves the original vector. Identity or
    /// boundary substitution is terminal at this sole hardware-owner seam.
    pub fn stage(&mut self, images: [PwmCompareImage; AXES]) -> Result<(), PwmCommitBarrierError> {
        if self.fault.is_some() {
            return Err(PwmCommitBarrierError::Faulted);
        }
        if self.staged.is_some() {
            return Err(PwmCommitBarrierError::Busy);
        }
        for (axis, image) in images.iter().enumerate() {
            if image.configuration_digest() != self.configuration_digest {
                return self.fail(PwmCommitBarrierError::ImageConfiguration { axis });
            }
            if image.scheduled_at() != self.next_boundary {
                return self.fail(PwmCommitBarrierError::Schedule { axis });
            }
        }
        self.staged = Some(images);
        Ok(())
    }

    /// Records one stage's exact latch witness without releasing a partial set.
    pub fn record_latch(
        &mut self,
        axis: usize,
        commit: PowerStageCommit,
    ) -> Result<(), PwmCommitBarrierError> {
        if self.fault.is_some() {
            return Err(PwmCommitBarrierError::Faulted);
        }
        let Some(images) = self.staged.as_ref() else {
            return self.fail(PwmCommitBarrierError::Sequence);
        };
        let Some(image) = images.get(axis).copied() else {
            return self.fail(PwmCommitBarrierError::AxisRange { axis });
        };
        if self.observations[axis].is_some() {
            return self.fail(PwmCommitBarrierError::DuplicateObservation { axis });
        }
        if commit.token != image.token()
            || commit.scheduled_at != image.scheduled_at()
            || commit.observed_at != self.next_boundary
        {
            return self.fail(PwmCommitBarrierError::Observation { axis });
        }
        self.observations[axis] = Some(commit);
        Ok(())
    }

    /// Closes one exact timer-zero after all stage status has been serviced.
    pub fn finish_boundary(
        &mut self,
        observed_at: DeviceCycle,
    ) -> Result<PwmCommitBankCompletion<AXES>, PwmCommitBarrierError> {
        if self.fault.is_some() {
            return Err(PwmCommitBarrierError::Faulted);
        }
        if self.staged.is_none() {
            return self.fail(PwmCommitBarrierError::Sequence);
        }
        if observed_at != self.next_boundary {
            return self.fail(PwmCommitBarrierError::Boundary);
        }
        if let Some(axis) = self
            .observations
            .iter()
            .position(|observation| observation.is_none())
        {
            return self.fail(PwmCommitBarrierError::MissingObservation { axis });
        }
        let Some(next_boundary) = self
            .next_boundary
            .0
            .checked_add(u64::from(self.pwm_period_cycles))
        else {
            return self.fail(PwmCommitBarrierError::CycleOverflow);
        };
        let Some(next_period_sequence) = self.next_period_sequence.checked_add(1) else {
            return self.fail(PwmCommitBarrierError::CycleOverflow);
        };
        self.staged.take().expect("staged PWM commit bank");
        let observations = core::mem::replace(&mut self.observations, [None; AXES]);
        let commits = observations.map(|commit| commit.expect("complete PWM commit bank"));
        let completion = PwmCommitBankCompletion {
            configuration_digest: self.configuration_digest,
            period_sequence: self.next_period_sequence,
            observed_at,
            commits,
        };
        self.next_boundary = DeviceCycle(next_boundary);
        self.next_period_sequence = next_period_sequence;
        Ok(completion)
    }

    /// Complete image vector currently awaiting physical latch reports.
    pub const fn staged_images(&self) -> Option<&[PwmCompareImage; AXES]> {
        self.staged.as_ref()
    }

    /// Whether every stage report exists for the pending boundary.
    pub fn ready(&self) -> bool {
        self.fault.is_none()
            && self.staged.is_some()
            && self
                .observations
                .iter()
                .all(|observation| observation.is_some())
    }

    /// Sole next physical timer-zero admitted by the barrier.
    pub const fn next_boundary(&self) -> DeviceCycle {
        self.next_boundary
    }

    /// Sequence assigned to the next complete physical commit set.
    pub const fn next_period_sequence(&self) -> u64 {
        self.next_period_sequence
    }

    /// First terminal mismatch retained by the barrier.
    pub const fn fault(&self) -> Option<PwmCommitBarrierError> {
        self.fault
    }

    /// Clears pending physical authority after a separately qualified safe action.
    pub fn invalidate_after_safe(&mut self) {
        self.staged = None;
        self.observations = [None; AXES];
        if self.fault.is_none() {
            self.fault = Some(PwmCommitBarrierError::SafetyInvalidated);
        }
    }

    fn fail<T>(&mut self, error: PwmCommitBarrierError) -> Result<T, PwmCommitBarrierError> {
        self.staged = None;
        self.observations = [None; AXES];
        self.fault = Some(error);
        Err(error)
    }
}

fn div_ceil_nonnegative(numerator: i128, denominator: i128) -> i128 {
    debug_assert!(numerator >= 0 && denominator > 0);
    numerator / denominator + i128::from(numerator % denominator != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FocTimingProfile, PiConfig, Q30};

    const DIGEST: Digest = Digest([0x71; 32]);

    fn parameters() -> FocParameterSnapshot {
        let controller = PiConfig {
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
            d_current: controller,
            q_current: controller,
        }
    }

    fn synchronization() -> PwmAdcSynchronization {
        PwmAdcSynchronization {
            configuration_digest: DIGEST,
            device_cycle_hz: 80_000_000,
            pwm_period_cycles: 4_000,
            nominal_acquisition_offset_cycles: 2_000,
            maximum_trigger_jitter_cycles: 2,
            maximum_acquisition_cycles: 40,
            maximum_channel_skew_cycles: 20,
            maximum_conversion_cycles: 80,
            minimum_switching_guard_cycles: 100,
        }
    }

    fn contract(maximum_error: u32) -> PwmCompareContract {
        PwmCompareContract::new(
            DIGEST,
            80_000_000,
            4_000,
            160_000_000,
            4_000,
            8,
            maximum_error,
        )
        .unwrap()
    }

    fn duties(value: Q30Interval) -> Duty3 {
        Duty3 {
            a: value,
            b: value,
            c: value,
        }
    }

    fn bank_images(at: DeviceCycle) -> [PwmCompareImage; 2] {
        [5_u32, 6_u32].map(|token| {
            contract(u32::MAX)
                .lower(
                    &parameters(),
                    synchronization(),
                    token,
                    at,
                    duties(Q30Interval::HALF),
                )
                .unwrap()
        })
    }

    fn physical_commit(image: PwmCompareImage) -> PowerStageCommit {
        PowerStageCommit {
            token: image.token(),
            scheduled_at: image.scheduled_at(),
            observed_at: image.scheduled_at(),
        }
    }

    #[test]
    fn exact_half_duty_has_center_aligned_integer_edges() {
        let image = contract(1)
            .lower(
                &parameters(),
                synchronization(),
                9,
                DeviceCycle(12_000),
                duties(Q30Interval::HALF),
            )
            .unwrap();
        assert_eq!(image.configuration_digest(), DIGEST);
        assert_eq!(image.token(), 9);
        assert_eq!(image.scheduled_at(), DeviceCycle(12_000));
        for value in image.comparisons() {
            assert_eq!(value.compare_ticks(), 2_000);
            assert_eq!(value.represented_duty(), Q30Interval::HALF);
            assert_eq!(value.maximum_error_ulps(), 0);
            assert_eq!(value.first_switching_edge_ticks(), 2_000);
            assert_eq!(value.second_switching_edge_ticks(), 6_000);
        }
        assert_eq!(
            image.validate_for(contract(1), &parameters(), synchronization()),
            Ok(())
        );
    }

    #[test]
    fn midpoint_comparison_rounds_halfway_ties_to_even() {
        let eighth = Q30_SCALE / 8;
        let even_low = Q30Interval::point(Q30::from_bits((3 * eighth) as i32));
        let even_high = Q30Interval::point(Q30::from_bits((5 * eighth) as i32));
        let small = PwmCompareContract::new(DIGEST, 80, 4, 80, 2, 0, u32::MAX);
        assert_eq!(small, Err(PwmCompareError::Contract));

        let contract = PwmCompareContract::new(DIGEST, 80, 4, 160, 4, 1, u32::MAX).unwrap();
        assert_eq!(contract.lower_one(even_low).unwrap().compare_ticks(), 2);
        assert_eq!(contract.lower_one(even_high).unwrap().compare_ticks(), 2);
    }

    #[test]
    fn range_pulse_and_quantization_policies_fail_closed() {
        let outside = Q30Interval::new(Q30::from_bits(-1), Q30::ZERO).unwrap();
        assert_eq!(
            contract(u32::MAX).lower(
                &parameters(),
                synchronization(),
                1,
                DeviceCycle(0),
                duties(outside),
            ),
            Err(PwmCompareError::DutyRange)
        );
        assert_eq!(
            contract(u32::MAX).lower(
                &parameters(),
                synchronization(),
                1,
                DeviceCycle(0),
                duties(Q30Interval::ZERO),
            ),
            Err(PwmCompareError::PulseWidth)
        );

        let one_third = Q30Interval::from_ratio(1, 3).unwrap();
        assert_eq!(
            contract(1).lower(
                &parameters(),
                synchronization(),
                1,
                DeviceCycle(0),
                duties(one_third),
            ),
            Err(PwmCompareError::Quantization)
        );
        assert_eq!(
            contract(u32::MAX).lower(
                &parameters(),
                synchronization(),
                0,
                DeviceCycle(0),
                duties(Q30Interval::HALF),
            ),
            Err(PwmCompareError::ZeroToken)
        );
    }

    #[test]
    fn clock_and_snapshot_identity_must_be_exact() {
        assert_eq!(
            PwmCompareContract::new(DIGEST, 80_000_001, 4_000, 160_000_000, 4_000, 8, 1),
            Err(PwmCompareError::Contract)
        );
        assert_eq!(
            PwmCompareContract::new(DIGEST, 80_000_000, 4_000, 159_999_999, 4_000, 8, 1),
            Err(PwmCompareError::Contract)
        );
        let mut foreign = parameters();
        foreign.configuration_digest = Digest([0x72; 32]);
        assert_eq!(
            contract(1).validate_for(&foreign, synchronization()),
            Err(PwmCompareError::Configuration)
        );
        let mut wrong_period = synchronization();
        wrong_period.pwm_period_cycles = 2_000;
        assert_eq!(
            contract(1).validate_for(&parameters(), wrong_period),
            Err(PwmCompareError::Configuration)
        );
    }

    #[test]
    fn image_replay_detects_substituted_compare_or_duty() {
        let contract = contract(u32::MAX);
        let mut image = contract
            .lower(
                &parameters(),
                synchronization(),
                5,
                DeviceCycle(8_000),
                duties(Q30Interval::HALF),
            )
            .unwrap();
        image.a.compare_ticks += 1;
        assert_eq!(
            image.validate_for(contract, &parameters(), synchronization()),
            Err(PwmCompareError::Image)
        );
    }

    #[test]
    fn complete_images_latch_only_on_their_exact_timer_zero() {
        let contract = contract(u32::MAX);
        let image = contract
            .lower(
                &parameters(),
                synchronization(),
                5,
                DeviceCycle(8_000),
                duties(Q30Interval::HALF),
            )
            .unwrap();
        let later = contract
            .lower(
                &parameters(),
                synchronization(),
                6,
                DeviceCycle(12_000),
                duties(Q30Interval::HALF),
            )
            .unwrap();
        let mut owner = PwmCompareLatchOwner::new(contract, DeviceCycle(8_000)).unwrap();
        owner
            .stage(image, &parameters(), synchronization())
            .unwrap();
        assert_eq!(
            owner.stage(later, &parameters(), synchronization()),
            Err(PwmCompareLatchError::Busy)
        );
        assert!(!owner.is_faulted());
        assert_eq!(
            owner.observe_timer_zero(DeviceCycle(8_000)),
            Ok(Some(PwmCompareLatch {
                period_sequence: 0,
                observed_at: DeviceCycle(8_000),
                image,
            }))
        );
        assert_eq!(owner.active_image(), Some(image));
        assert_eq!(owner.next_boundary(), DeviceCycle(12_000));
        owner
            .stage(later, &parameters(), synchronization())
            .unwrap();
        assert_eq!(
            owner.observe_timer_zero(DeviceCycle(12_000)),
            Ok(Some(PwmCompareLatch {
                period_sequence: 1,
                observed_at: DeviceCycle(12_000),
                image: later,
            }))
        );
        assert_eq!(owner.active_image(), Some(later));
    }

    #[test]
    fn bank_barrier_releases_only_one_complete_out_of_order_commit_set() {
        let images = bank_images(DeviceCycle(8_000));
        let commits = images.map(physical_commit);
        let mut barrier =
            PwmCommitBankBarrier::<2>::new(DIGEST, 4_000, DeviceCycle(8_000), 7).unwrap();
        barrier.stage(images).unwrap();
        assert_eq!(barrier.staged_images(), Some(&images));
        assert_eq!(barrier.stage(images), Err(PwmCommitBarrierError::Busy));
        assert_eq!(barrier.fault(), None);

        barrier.record_latch(1, commits[1]).unwrap();
        assert!(!barrier.ready());
        barrier.record_latch(0, commits[0]).unwrap();
        assert!(barrier.ready());
        let completion = barrier.finish_boundary(DeviceCycle(8_000)).unwrap();
        assert_eq!(completion.configuration_digest(), DIGEST);
        assert_eq!(completion.period_sequence(), 7);
        assert_eq!(completion.observed_at(), DeviceCycle(8_000));
        assert_eq!(completion.commits(), commits);
        assert_eq!(barrier.staged_images(), None);
        assert_eq!(barrier.next_boundary(), DeviceCycle(12_000));
        assert_eq!(barrier.next_period_sequence(), 8);
        assert_eq!(barrier.fault(), None);
    }

    #[test]
    fn bank_barrier_staging_is_transactional_and_identity_exact() {
        assert!(
            core::mem::size_of::<PwmCommitBankBarrier<4>>() <= MAX_PWM_COMMIT_BANK_BARRIER_BYTES
        );
        assert!(
            core::mem::size_of::<PwmCommitBankCompletion<4>>()
                <= MAX_PWM_COMMIT_BANK_COMPLETION_BYTES
        );
        assert_eq!(
            PwmCommitBankBarrier::<0>::new(DIGEST, 4_000, DeviceCycle(8_000), 1),
            Err(PwmCommitBarrierError::AxisCount)
        );
        assert_eq!(
            PwmCommitBankBarrier::<5>::new(DIGEST, 4_000, DeviceCycle(8_000), 1),
            Err(PwmCommitBarrierError::AxisCount)
        );
        assert_eq!(
            PwmCommitBankBarrier::<2>::new(Digest::ZERO, 4_000, DeviceCycle(8_000), 1),
            Err(PwmCommitBarrierError::Configuration)
        );

        let images = bank_images(DeviceCycle(8_000));
        let mut foreign = images;
        foreign[1].configuration_digest = Digest([0x72; 32]);
        let mut barrier =
            PwmCommitBankBarrier::<2>::new(DIGEST, 4_000, DeviceCycle(8_000), 1).unwrap();
        assert_eq!(
            barrier.stage(foreign),
            Err(PwmCommitBarrierError::ImageConfiguration { axis: 1 })
        );
        assert_eq!(barrier.staged_images(), None);
        assert_eq!(barrier.stage(images), Err(PwmCommitBarrierError::Faulted));

        let mut off_boundary = images;
        off_boundary[1] = contract(u32::MAX)
            .lower(
                &parameters(),
                synchronization(),
                6,
                DeviceCycle(12_000),
                duties(Q30Interval::HALF),
            )
            .unwrap();
        barrier = PwmCommitBankBarrier::<2>::new(DIGEST, 4_000, DeviceCycle(8_000), 1).unwrap();
        assert_eq!(
            barrier.stage(off_boundary),
            Err(PwmCommitBarrierError::Schedule { axis: 1 })
        );
        assert_eq!(barrier.staged_images(), None);
    }

    #[test]
    fn bank_barrier_latches_duplicate_substituted_and_missing_reports() {
        let images = bank_images(DeviceCycle(8_000));
        let commits = images.map(physical_commit);

        let mut duplicate =
            PwmCommitBankBarrier::<2>::new(DIGEST, 4_000, DeviceCycle(8_000), 1).unwrap();
        duplicate.stage(images).unwrap();
        duplicate.record_latch(0, commits[0]).unwrap();
        assert_eq!(
            duplicate.record_latch(0, commits[0]),
            Err(PwmCommitBarrierError::DuplicateObservation { axis: 0 })
        );
        assert_eq!(duplicate.staged_images(), None);

        let mut substituted =
            PwmCommitBankBarrier::<2>::new(DIGEST, 4_000, DeviceCycle(8_000), 1).unwrap();
        substituted.stage(images).unwrap();
        let mut wrong_token = commits[1];
        wrong_token.token += 1;
        assert_eq!(
            substituted.record_latch(1, wrong_token),
            Err(PwmCommitBarrierError::Observation { axis: 1 })
        );

        let mut missing =
            PwmCommitBankBarrier::<2>::new(DIGEST, 4_000, DeviceCycle(8_000), 1).unwrap();
        missing.stage(images).unwrap();
        missing.record_latch(0, commits[0]).unwrap();
        assert_eq!(
            missing.finish_boundary(DeviceCycle(8_000)),
            Err(PwmCommitBarrierError::MissingObservation { axis: 1 })
        );
        assert_eq!(missing.next_boundary(), DeviceCycle(8_000));

        let mut wrong_axis =
            PwmCommitBankBarrier::<2>::new(DIGEST, 4_000, DeviceCycle(8_000), 1).unwrap();
        wrong_axis.stage(images).unwrap();
        assert_eq!(
            wrong_axis.record_latch(2, commits[0]),
            Err(PwmCommitBarrierError::AxisRange { axis: 2 })
        );

        let mut wrong_boundary =
            PwmCommitBankBarrier::<2>::new(DIGEST, 4_000, DeviceCycle(8_000), 1).unwrap();
        wrong_boundary.stage(images).unwrap();
        wrong_boundary.record_latch(0, commits[0]).unwrap();
        wrong_boundary.record_latch(1, commits[1]).unwrap();
        assert_eq!(
            wrong_boundary.finish_boundary(DeviceCycle(8_001)),
            Err(PwmCommitBarrierError::Boundary)
        );
    }

    #[test]
    fn bank_barrier_overflow_and_safe_invalidation_release_no_commit() {
        let boundary = DeviceCycle(u64::MAX - 3_999);
        let images = bank_images(boundary);
        let commits = images.map(physical_commit);
        let mut overflow =
            PwmCommitBankBarrier::<2>::new(DIGEST, 4_000, boundary, u64::MAX).unwrap();
        overflow.stage(images).unwrap();
        overflow.record_latch(0, commits[0]).unwrap();
        overflow.record_latch(1, commits[1]).unwrap();
        assert_eq!(
            overflow.finish_boundary(boundary),
            Err(PwmCommitBarrierError::CycleOverflow)
        );
        assert_eq!(overflow.staged_images(), None);
        assert_eq!(overflow.next_boundary(), boundary);

        let mut safe =
            PwmCommitBankBarrier::<2>::new(DIGEST, 4_000, DeviceCycle(8_000), 1).unwrap();
        safe.stage(bank_images(DeviceCycle(8_000))).unwrap();
        safe.invalidate_after_safe();
        assert_eq!(safe.staged_images(), None);
        assert_eq!(safe.fault(), Some(PwmCommitBarrierError::SafetyInvalidated));
        assert_eq!(
            safe.finish_boundary(DeviceCycle(8_000)),
            Err(PwmCommitBarrierError::Faulted)
        );
    }

    #[test]
    fn latch_owner_faults_on_off_grid_foreign_and_missing_boundaries() {
        let contract = contract(u32::MAX);
        let off_grid = contract
            .lower(
                &parameters(),
                synchronization(),
                7,
                DeviceCycle(8_001),
                duties(Q30Interval::HALF),
            )
            .unwrap();
        let mut owner = PwmCompareLatchOwner::new(contract, DeviceCycle(8_000)).unwrap();
        assert_eq!(
            owner.stage(off_grid, &parameters(), synchronization()),
            Err(PwmCompareLatchError::Schedule)
        );
        assert!(owner.is_faulted());
        assert_eq!(
            owner.observe_timer_zero(DeviceCycle(8_000)),
            Err(PwmCompareLatchError::Faulted)
        );

        owner = PwmCompareLatchOwner::new(contract, DeviceCycle(8_000)).unwrap();
        let mut forged = contract
            .lower(
                &parameters(),
                synchronization(),
                8,
                DeviceCycle(8_000),
                duties(Q30Interval::HALF),
            )
            .unwrap();
        forged.b.compare_ticks += 1;
        assert_eq!(
            owner.stage(forged, &parameters(), synchronization()),
            Err(PwmCompareLatchError::Compare(PwmCompareError::Image))
        );
        assert!(owner.is_faulted());

        owner = PwmCompareLatchOwner::new(contract, DeviceCycle(8_000)).unwrap();
        assert_eq!(
            owner.observe_timer_zero(DeviceCycle(12_000)),
            Err(PwmCompareLatchError::Boundary)
        );
        assert!(owner.is_faulted());
    }

    #[test]
    fn timer_zero_cycle_overflow_faults_before_state_advances() {
        let contract = contract(u32::MAX);
        let first = DeviceCycle(u64::MAX - 3_999);
        let mut owner = PwmCompareLatchOwner::new(contract, first).unwrap();
        assert_eq!(
            owner.observe_timer_zero(first),
            Err(PwmCompareLatchError::CycleOverflow)
        );
        assert!(owner.is_faulted());
        assert_eq!(owner.active_image(), None);
    }

    #[test]
    fn selected_compare_and_reported_error_cover_independent_integer_grid() {
        let contract = PwmCompareContract::new(DIGEST, 80, 4, 240, 6, 1, u32::MAX).unwrap();
        for lower_bits in (Q30::from_bits(1).bits()..=Q30::ONE.bits()).step_by(16_777_216) {
            for width in [0_i32, 1, 17, 1_024] {
                let upper_bits = lower_bits.saturating_add(width).min(Q30::ONE.bits());
                let interval =
                    Q30Interval::new(Q30::from_bits(lower_bits), Q30::from_bits(upper_bits))
                        .unwrap();
                let Ok(value) = contract.lower_one(interval) else {
                    continue;
                };
                let compare = i128::from(value.compare_ticks());
                let peak = i128::from(contract.timer_peak_ticks());
                let scale = i128::from(Q30_SCALE);
                for endpoint in [lower_bits, upper_bits] {
                    let distance = (compare * scale - i128::from(endpoint) * peak).abs();
                    assert!(distance <= i128::from(value.maximum_error_ulps()) * peak);
                }
                assert!(
                    value.represented_duty().contains(
                        Q30Interval::from_ratio(
                            i64::from(value.compare_ticks()),
                            i64::from(contract.timer_peak_ticks()),
                        )
                        .unwrap()
                        .midpoint()
                    )
                );
            }
        }
    }
}
