//! ESP GPIO realization of configuration-derived safety inputs.

use alumina_board::ResourceId;
use alumina_config::RealtimeConfigurationProfile;
use alumina_protocol::DeviceCycle;
use alumina_safety::{
    InputBias, MAX_SAFETY_INPUTS, SafetyInputError, SafetyInputMonitor, SafetyInputReaction,
    SafetyInputStatus,
};
use esp_hal::gpio::{Input, InputConfig, InputPin, Pull};

/// On-chip pull modes physically supported by one routed input.
#[allow(
    dead_code,
    reason = "the selected board may intentionally expose zero machine safety inputs"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BiasCapability {
    /// Only high-impedance sampling is admitted.
    FloatingOnly,
    /// Both ESP pull-up and pull-down modes are admitted.
    PullUpDown,
}

/// One uniquely owned target GPIO retained as an input for the lifetime of core 1.
pub struct SafetyInputRoute {
    resource: ResourceId,
    input: Input<'static>,
    bias_capability: BiasCapability,
}

impl SafetyInputRoute {
    /// Immediately disables the output driver and establishes a floating input.
    #[allow(
        dead_code,
        reason = "the selected board may intentionally expose zero machine safety inputs"
    )]
    pub fn new(
        resource: ResourceId,
        pin: impl InputPin + 'static,
        bias_capability: BiasCapability,
    ) -> Self {
        Self {
            resource,
            input: Input::new(pin, InputConfig::default()),
            bias_capability,
        }
    }

    fn supports(&self, bias: InputBias) -> bool {
        bias == InputBias::Floating || self.bias_capability == BiasCapability::PullUpDown
    }

    fn apply_bias(&mut self, bias: InputBias) {
        let pull = match bias {
            InputBias::Floating => Pull::None,
            InputBias::PullUp => Pull::Up,
            InputBias::PullDown => Pull::Down,
        };
        self.input
            .apply_config(&InputConfig::default().with_pull(pull));
    }
}

/// Fixed target-owned routes plus transactional configuration and sampling.
pub struct SafetyInputBank<const ROUTES: usize> {
    routes: [SafetyInputRoute; ROUTES],
}

impl<const ROUTES: usize> SafetyInputBank<ROUTES> {
    /// Validates the board composition after every route has already entered
    /// input/high-impedance mode.
    pub fn new(routes: [SafetyInputRoute; ROUTES]) -> Result<Self, SafetyInputBackendError> {
        let mut route = 0;
        while route < ROUTES {
            if !matches!(routes[route].resource, ResourceId::Gpio(_)) {
                return Err(SafetyInputBackendError::BoardRoute(routes[route].resource));
            }
            let mut previous = 0;
            while previous < route {
                if routes[previous].resource == routes[route].resource {
                    return Err(SafetyInputBackendError::BoardRoute(routes[route].resource));
                }
                previous += 1;
            }
            route += 1;
        }
        Ok(Self { routes })
    }

    /// Validates all routes and timing before atomically replacing applied
    /// pull modes. Empty profiles leave every retained route floating.
    pub fn configure(
        &mut self,
        profile: &RealtimeConfigurationProfile,
        nominal_scan_period_cycles: u64,
    ) -> Result<Option<SafetyInputMonitor<MAX_SAFETY_INPUTS>>, SafetyInputBackendError> {
        if nominal_scan_period_cycles == 0 {
            return Err(SafetyInputBackendError::Cadence {
                maximum_gap_cycles: 0,
                nominal_scan_period_cycles,
            });
        }
        if profile.safety_input_count() == 0 {
            self.clear();
            return Ok(None);
        }
        let monitor = SafetyInputMonitor::<MAX_SAFETY_INPUTS>::from_specs(profile.safety_inputs())
            .map_err(SafetyInputBackendError::Monitor)?;

        for spec in profile.safety_inputs() {
            if u64::from(spec.maximum_sample_gap_cycles) < nominal_scan_period_cycles {
                return Err(SafetyInputBackendError::Cadence {
                    maximum_gap_cycles: spec.maximum_sample_gap_cycles,
                    nominal_scan_period_cycles,
                });
            }
            let route = self
                .route(spec.resource)
                .ok_or(SafetyInputBackendError::MissingRoute(spec.resource))?;
            if !route.supports(spec.bias) {
                return Err(SafetyInputBackendError::UnsupportedBias {
                    resource: spec.resource,
                    bias: spec.bias,
                });
            }
        }

        self.clear();
        for spec in profile.safety_inputs() {
            self.route_mut(spec.resource)
                .expect("all routes were validated before applying hardware state")
                .apply_bias(spec.bias);
        }
        Ok(Some(monitor))
    }

    /// Restores every target-owned route to input/high-impedance mode.
    pub fn clear(&mut self) {
        for route in &mut self.routes {
            route.apply_bias(InputBias::Floating);
        }
    }

    /// Samples every canonical monitor slot once at the same local cycle.
    pub fn scan<const INPUTS: usize>(
        &self,
        monitor: &mut SafetyInputMonitor<INPUTS>,
        at: DeviceCycle,
    ) -> Result<SafetyInputScan, SafetyInputBackendError> {
        let mut transitioned_mask = 0_u32;
        let mut reaction = None;
        let mut reaction_slot = 0_u8;
        let mut slot = 0;
        while slot < monitor.len() {
            let spec = monitor.spec(slot).ok_or(SafetyInputBackendError::Monitor(
                SafetyInputError::Slot { slot },
            ))?;
            let level_high = self
                .route(spec.resource)
                .ok_or(SafetyInputBackendError::MissingRoute(spec.resource))?
                .input
                .is_high();
            let transition = match monitor.observe(slot, level_high, at) {
                Ok(transition) => transition,
                Err(SafetyInputError::SampleGap { slot, .. }) => {
                    return Ok(SafetyInputScan {
                        status: monitor.status(at),
                        transitioned_mask,
                        reaction: Some(SafetyInputReaction::Fault(
                            alumina_safety::FaultCode::Watchdog,
                        )),
                        reaction_slot: u8::try_from(slot)
                            .expect("monitor construction limits slots to 32"),
                    });
                }
                Err(error) => return Err(SafetyInputBackendError::Monitor(error)),
            };
            if let Some(transition) = transition {
                transitioned_mask |= 1_u32 << slot;
                let next = transition.conservative_reaction();
                if should_replace_reaction(reaction, next) {
                    reaction = Some(next);
                    reaction_slot =
                        u8::try_from(slot).expect("monitor construction limits slots to 32");
                }
            }
            slot += 1;
        }

        let status = monitor.status(at);
        if reaction.is_none()
            && let Some(watchdog) = monitor.watchdog_fault(at)
        {
            reaction = Some(watchdog.conservative_reaction());
            reaction_slot = watchdog.slot;
        }
        Ok(SafetyInputScan {
            status,
            transitioned_mask,
            reaction,
            reaction_slot,
        })
    }

    fn route(&self, resource: ResourceId) -> Option<&SafetyInputRoute> {
        self.routes.iter().find(|route| route.resource == resource)
    }

    fn route_mut(&mut self, resource: ResourceId) -> Option<&mut SafetyInputRoute> {
        self.routes
            .iter_mut()
            .find(|route| route.resource == resource)
    }
}

/// One complete target sampling pass and its first conservative local action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafetyInputScan {
    pub status: SafetyInputStatus,
    pub transitioned_mask: u32,
    pub reaction: Option<SafetyInputReaction>,
    /// Canonical slot naming the retained reaction; zero when there is none.
    pub reaction_slot: u8,
}

/// Static route, configuration, or monitor rejection at the target boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafetyInputBackendError {
    BoardRoute(ResourceId),
    MissingRoute(ResourceId),
    UnsupportedBias {
        resource: ResourceId,
        bias: InputBias,
    },
    Cadence {
        maximum_gap_cycles: u32,
        nominal_scan_period_cycles: u64,
    },
    Monitor(SafetyInputError),
}

const fn should_replace_reaction(
    current: Option<SafetyInputReaction>,
    next: SafetyInputReaction,
) -> bool {
    match (current, next) {
        (_, SafetyInputReaction::Clear) => false,
        (None | Some(SafetyInputReaction::Clear), _) => true,
        (Some(SafetyInputReaction::Hold), SafetyInputReaction::Fault(_)) => true,
        (Some(SafetyInputReaction::Hold | SafetyInputReaction::Fault(_)), _) => false,
    }
}
