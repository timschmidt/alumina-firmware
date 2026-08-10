#![no_std]
#![doc = "Small deterministic safety-state core for Alumina devices."]

/// Top-level real-time safety state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SafetyState {
    /// Hardware initialization has not yet established safe outputs.
    #[default]
    Boot,
    /// Outputs are at board-declared safe values and no configuration is active.
    Safe,
    /// A complete configuration is active but cannot yet produce motion or energy.
    Configured,
    /// Local preconditions pass and the selected job may be started.
    Armed,
    /// Scheduled real-time work is executing.
    Running,
    /// Motion/process work is stopped or decelerating under a validated hold policy.
    Hold,
    /// A latched fault requires an explicit physically meaningful reset.
    Fault,
}

/// Stable high-level reason for a latched fault.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FaultCode {
    /// Physical emergency-stop input asserted.
    EmergencyStop,
    /// Hard limit asserted outside its allowed operation.
    HardLimit,
    /// Driver or power-stage fault input asserted.
    Driver,
    /// Command or SD-prefetch horizon could not be maintained safely.
    QueueUnderrun,
    /// A real-time deadline was missed.
    Deadline,
    /// Configuration or job identity changed unexpectedly.
    Identity,
    /// Motor current exceeded its qualified bound.
    OverCurrent,
    /// Sensor plausibility or following-error check failed.
    Feedback,
    /// Watchdog or maximum-output-duration limit expired.
    Watchdog,
}

/// Preconditions sampled and validated before a state transition.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Conditions {
    /// Board initialization has installed every safe output.
    pub safe_outputs_established: bool,
    /// Configuration has passed service- and real-time-side validation.
    pub configuration_valid: bool,
    /// Physical safety chain is closed and plausible.
    pub interlocks_closed: bool,
    /// Sufficient verified work is buffered for the requested operation.
    pub buffer_ready: bool,
    /// A local reset action satisfying machine policy has occurred.
    pub physical_reset_confirmed: bool,
}

/// Requested transition from the service boundary, job executor, or safety ISR.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    /// Finish boot-time safe-output establishment.
    Initialize,
    /// Commit a completely validated configuration.
    Configure,
    /// Enter the armed state.
    Arm,
    /// Begin scheduled work.
    Start,
    /// Request a controlled hold.
    Hold,
    /// Resume held work.
    Resume,
    /// Finish or cancel work after all outputs reached their declared terminal state.
    Finish,
    /// Disarm without executing work.
    Disarm,
    /// Latch an asynchronous fault.
    Fault(FaultCode),
    /// Reset a latched fault to the unconfigured safe state.
    ResetFault,
}

/// Deterministic state plus its retained fault reason.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SafetyMachine {
    state: SafetyState,
    fault: Option<FaultCode>,
}

impl SafetyMachine {
    /// Creates a machine in [`SafetyState::Boot`].
    pub const fn new() -> Self {
        Self {
            state: SafetyState::Boot,
            fault: None,
        }
    }

    /// Returns the current state.
    pub const fn state(self) -> SafetyState {
        self.state
    }

    /// Returns the retained fault reason, if any.
    pub const fn fault(self) -> Option<FaultCode> {
        self.fault
    }

    /// Applies one event atomically or leaves the machine unchanged on failure.
    pub fn apply(&mut self, event: Event, conditions: Conditions) -> Result<SafetyState, Error> {
        if let Event::Fault(code) = event {
            self.state = SafetyState::Fault;
            self.fault = Some(code);
            return Ok(self.state);
        }

        let next = match (self.state, event) {
            (SafetyState::Boot, Event::Initialize) if conditions.safe_outputs_established => {
                SafetyState::Safe
            }
            (SafetyState::Safe, Event::Configure) if conditions.configuration_valid => {
                SafetyState::Configured
            }
            (SafetyState::Configured, Event::Configure) if conditions.configuration_valid => {
                SafetyState::Configured
            }
            (SafetyState::Configured, Event::Arm)
                if conditions.configuration_valid
                    && conditions.interlocks_closed
                    && conditions.buffer_ready =>
            {
                SafetyState::Armed
            }
            (SafetyState::Armed, Event::Start)
                if conditions.configuration_valid
                    && conditions.interlocks_closed
                    && conditions.buffer_ready =>
            {
                SafetyState::Running
            }
            (SafetyState::Armed, Event::Disarm) => SafetyState::Configured,
            (SafetyState::Running, Event::Hold) => SafetyState::Hold,
            (SafetyState::Hold, Event::Resume)
                if conditions.configuration_valid
                    && conditions.interlocks_closed
                    && conditions.buffer_ready =>
            {
                SafetyState::Running
            }
            (SafetyState::Running | SafetyState::Hold, Event::Finish) => SafetyState::Configured,
            (SafetyState::Fault, Event::ResetFault)
                if conditions.safe_outputs_established && conditions.physical_reset_confirmed =>
            {
                self.fault = None;
                SafetyState::Safe
            }
            _ => {
                return Err(Error {
                    state: self.state,
                    event,
                });
            }
        };

        self.state = next;
        Ok(next)
    }
}

/// Rejected state transition. The state is unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Error {
    /// State in which the request was rejected.
    pub state: SafetyState,
    /// Rejected event.
    pub event: Event,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready() -> Conditions {
        Conditions {
            safe_outputs_established: true,
            configuration_valid: true,
            interlocks_closed: true,
            buffer_ready: true,
            physical_reset_confirmed: true,
        }
    }

    fn running_machine() -> SafetyMachine {
        let mut machine = SafetyMachine::new();
        machine.apply(Event::Initialize, ready()).unwrap();
        machine.apply(Event::Configure, ready()).unwrap();
        machine.apply(Event::Arm, ready()).unwrap();
        machine.apply(Event::Start, ready()).unwrap();
        machine
    }

    #[test]
    fn nominal_run_requires_every_ordered_gate() {
        let mut machine = running_machine();
        assert_eq!(machine.state(), SafetyState::Running);
        assert_eq!(machine.apply(Event::Hold, ready()), Ok(SafetyState::Hold));
        assert_eq!(
            machine.apply(Event::Resume, ready()),
            Ok(SafetyState::Running)
        );
        assert_eq!(
            machine.apply(Event::Finish, ready()),
            Ok(SafetyState::Configured)
        );
    }

    #[test]
    fn arm_rejects_open_interlock_without_mutating_state() {
        let mut machine = SafetyMachine::new();
        machine.apply(Event::Initialize, ready()).unwrap();
        machine.apply(Event::Configure, ready()).unwrap();
        let mut conditions = ready();
        conditions.interlocks_closed = false;

        assert_eq!(
            machine.apply(Event::Arm, conditions),
            Err(Error {
                state: SafetyState::Configured,
                event: Event::Arm
            })
        );
        assert_eq!(machine.state(), SafetyState::Configured);
    }

    #[test]
    fn asynchronous_fault_latches_from_any_state() {
        for initial in [SafetyMachine::new(), running_machine()] {
            let mut machine = initial;
            assert_eq!(
                machine.apply(
                    Event::Fault(FaultCode::EmergencyStop),
                    Conditions::default()
                ),
                Ok(SafetyState::Fault)
            );
            assert_eq!(machine.fault(), Some(FaultCode::EmergencyStop));
        }
    }

    #[test]
    fn browser_style_reset_without_physical_confirmation_is_rejected() {
        let mut machine = running_machine();
        machine
            .apply(Event::Fault(FaultCode::Watchdog), Conditions::default())
            .unwrap();
        let mut conditions = ready();
        conditions.physical_reset_confirmed = false;

        assert!(machine.apply(Event::ResetFault, conditions).is_err());
        assert_eq!(machine.state(), SafetyState::Fault);
        assert_eq!(machine.fault(), Some(FaultCode::Watchdog));
    }
}
