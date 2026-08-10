#![no_std]
#![doc = "Typed compile-time board facts and validation for Alumina firmware."]

use alumina_protocol::Digest;

/// ESP application MCU families currently admitted by the architecture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Chip {
    /// Original dual-core Xtensa ESP32.
    Esp32,
    /// Dual-core Xtensa ESP32-S3.
    Esp32S3,
}

/// Evidence-backed support level for one exact board revision.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Qualification {
    /// Metadata validates, with no build claim.
    Described,
    /// Firmware compiles for the exact target.
    Compiles,
    /// Safe-state and named-peripheral hardware smoke tests pass.
    Bench,
    /// Real-time timing and fault tests pass a declared motion envelope.
    MotionQualified,
    /// Full production evidence passes.
    ProductionQualified,
}

/// Physical executor domain that exclusively owns a resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OwnerDomain {
    /// Wi-Fi, filesystem, web, display, and background executor on core 0.
    Service,
    /// Safety, motion, control loops, and deterministic I/O on core 1.
    Realtime,
}

/// Stable typed resource identifier. Numeric GPIO aliases never cross namespaces.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ResourceId {
    /// Physical GPIO number.
    Gpio(u8),
    /// Bit in a specific I²S-driven shifted-output engine.
    I2sOut { engine: u8, bit: u8 },
    /// ADC unit and channel.
    Adc { unit: u8, channel: u8 },
    /// Timer group and timer index.
    Timer { group: u8, index: u8 },
    /// LEDC/MCPWM/RMT-like timed-output engine and channel.
    TimedOutput { engine: u8, channel: u8 },
    /// I²C controller.
    I2c(u8),
    /// SPI controller.
    Spi(u8),
    /// UART controller.
    Uart(u8),
    /// PCNT unit.
    Pcnt(u8),
    /// Board-fitted device from a board-local stable namespace.
    Device(u16),
}

/// Value driven before configuration, after a fault, and during reset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafeValue {
    /// Resource cannot be driven by firmware.
    NotApplicable,
    /// Input/high-impedance mode.
    HighImpedance,
    /// Logical inactive/low state.
    Low,
    /// Logical active/high state.
    High,
    /// Complete shifted-engine image is supplied elsewhere by the board package.
    EngineImage,
}

/// One physically routed resource and its immutable ownership facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceDescriptor {
    /// Stable typed identity.
    pub id: ResourceId,
    /// Exclusive executor domain.
    pub owner: OwnerDomain,
    /// Reset/fault behavior.
    pub safe_value: SafeValue,
    /// True when the resource can energize motion, torque, heat, or process power.
    pub hazardous_output: bool,
}

/// Immutable facts for one PCB revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardDescriptor<'a> {
    /// Stable board/revision ID used in manifests.
    pub id: &'a str,
    /// Human-readable revision string.
    pub revision: &'a str,
    /// Application MCU.
    pub chip: Chip,
    /// Number of application cores available to Alumina.
    pub application_cores: u8,
    /// Current evidence level.
    pub qualification: Qualification,
    /// Canonical serialized capability digest, once generated.
    pub capability_digest: Digest,
    /// Complete set of routed resources advertised by this package.
    pub resources: &'a [ResourceDescriptor],
}

impl BoardDescriptor<'_> {
    /// Validates architecture-wide board invariants without allocation.
    pub fn validate(&self) -> Result<(), BoardError> {
        if self.id.is_empty() {
            return Err(BoardError::MissingId);
        }
        if self.revision.is_empty() {
            return Err(BoardError::MissingRevision);
        }
        if self.application_cores < 2 {
            return Err(BoardError::InsufficientCores {
                found: self.application_cores,
            });
        }

        let mut outer = 0;
        while outer < self.resources.len() {
            let resource = self.resources[outer];
            if resource.hazardous_output {
                if resource.owner != OwnerDomain::Realtime {
                    return Err(BoardError::HazardOwnedByService { id: resource.id });
                }
                if matches!(resource.safe_value, SafeValue::NotApplicable) {
                    return Err(BoardError::HazardMissingSafeValue { id: resource.id });
                }
            }

            let mut inner = outer + 1;
            while inner < self.resources.len() {
                if resource.id == self.resources[inner].id {
                    return Err(BoardError::DuplicateResource { id: resource.id });
                }
                inner += 1;
            }
            outer += 1;
        }
        Ok(())
    }
}

/// Board-package validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoardError {
    /// Stable board ID was empty.
    MissingId,
    /// Revision was empty.
    MissingRevision,
    /// Architecture requires two application cores.
    InsufficientCores {
        /// Core count advertised by the package.
        found: u8,
    },
    /// Two physical facts claimed the same typed resource.
    DuplicateResource {
        /// Conflicting resource.
        id: ResourceId,
    },
    /// A hazardous output was assigned to the service executor.
    HazardOwnedByService {
        /// Misassigned resource.
        id: ResourceId,
    },
    /// A hazardous output lacked a defined safe state.
    HazardMissingSafeValue {
        /// Incomplete resource.
        id: ResourceId,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEP: ResourceDescriptor = ResourceDescriptor {
        id: ResourceId::I2sOut { engine: 0, bit: 1 },
        owner: OwnerDomain::Realtime,
        safe_value: SafeValue::EngineImage,
        hazardous_output: true,
    };

    fn board(resources: &[ResourceDescriptor]) -> BoardDescriptor<'_> {
        BoardDescriptor {
            id: "mks-tinybee-v1",
            revision: "1.x",
            chip: Chip::Esp32,
            application_cores: 2,
            qualification: Qualification::Described,
            capability_digest: Digest::ZERO,
            resources,
        }
    }

    #[test]
    fn valid_dual_core_board_is_accepted() {
        assert_eq!(board(&[STEP]).validate(), Ok(()));
    }

    #[test]
    fn single_core_board_is_rejected() {
        let mut descriptor = board(&[]);
        descriptor.application_cores = 1;
        assert_eq!(
            descriptor.validate(),
            Err(BoardError::InsufficientCores { found: 1 })
        );
    }

    #[test]
    fn shifted_output_and_gpio_are_distinct_namespaces() {
        let resources = [
            STEP,
            ResourceDescriptor {
                id: ResourceId::Gpio(1),
                owner: OwnerDomain::Service,
                safe_value: SafeValue::Low,
                hazardous_output: false,
            },
        ];
        assert_eq!(board(&resources).validate(), Ok(()));
    }

    #[test]
    fn duplicate_and_service_owned_hazards_are_rejected() {
        let duplicate = [STEP, STEP];
        assert_eq!(
            board(&duplicate).validate(),
            Err(BoardError::DuplicateResource { id: STEP.id })
        );

        let service_hazard = [ResourceDescriptor {
            owner: OwnerDomain::Service,
            ..STEP
        }];
        assert_eq!(
            board(&service_hazard).validate(),
            Err(BoardError::HazardOwnedByService { id: STEP.id })
        );
    }
}
