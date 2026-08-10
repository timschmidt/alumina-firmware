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
    /// I²S/parallel engine, distinct from its routed GPIOs and virtual bits.
    I2s(u8),
    /// RMT channel.
    Rmt(u8),
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
    /// DMA channel from the chip-specific DMA domain.
    Dma(u8),
    /// TWAI/CAN controller.
    Twai(u8),
    /// Board storage device or volume.
    Storage(u8),
    /// Chip or fitted radio device.
    Radio(u8),
    /// Board-local safety input chain.
    SafetyInput(u8),
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

/// Compile-time flash and RAM facts for one PCB/module revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryDescriptor {
    /// Executable/data flash bytes fitted to the selected module.
    pub flash_bytes: usize,
    /// Internal SRAM bytes before static/runtime reservations.
    pub internal_sram_bytes: usize,
    /// External PSRAM bytes, or zero when absent.
    pub psram_bytes: usize,
    /// PSRAM is never admitted for the deterministic core's active state.
    pub realtime_psram_allowed: bool,
}

/// Fixed application-core assignment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoreAssignment {
    /// Wi-Fi, storage, UI, and background executor.
    pub service_core: u8,
    /// Safety, motion, and hardware-timed executor.
    pub realtime_core: u8,
}

/// One canonical or silkscreen alias resolved before real-time admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AliasDescriptor<'a> {
    /// Case-sensitive stable alias.
    pub name: &'a str,
    /// Typed target; aliases never change the target namespace.
    pub resource: ResourceId,
}

/// Board-level serial bus family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BusKind {
    /// Inter-integrated circuit controller.
    I2c,
    /// Serial peripheral interface controller.
    Spi,
    /// Asynchronous serial controller.
    Uart,
}

/// Routed bus and its exclusive pin ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BusDescriptor<'a> {
    /// Typed I²C/SPI/UART controller resource.
    pub resource: ResourceId,
    /// Expected controller family.
    pub kind: BusKind,
    /// Core domain that constructs and owns the controller and interrupts.
    pub owner: OwnerDomain,
    /// GPIO resources forming the bus, in board-documented order.
    pub pins: &'a [ResourceId],
    /// Highest board-admitted bus frequency before machine-specific reduction.
    pub maximum_frequency_hz: u32,
}

/// Addressing/routing mechanism for a fitted device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceRoute {
    /// No serial-bus route; device uses dedicated resources.
    Dedicated,
    /// Seven-bit I²C address on the referenced bus.
    I2cAddress(u8),
    /// Independent active-low SPI chip-select GPIO.
    SpiChipSelect(ResourceId),
    /// Point-to-point UART route.
    Uart,
}

/// Implementation evidence for a routed device or engine.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SupportLevel {
    /// Fact is described, but no implementation claim exists.
    Described,
    /// A driver exists and compiles for the selected chip.
    Compiles,
    /// Driver passes a named physical smoke test.
    Bench,
    /// Driver passes its declared deterministic timing envelope.
    Qualified,
}

/// One fitted board device and its bus relationship.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceDescriptor {
    /// Must be a `ResourceId::Device` or `ResourceId::Storage` in `resources`.
    pub resource: ResourceId,
    /// Exclusive executor domain.
    pub owner: OwnerDomain,
    /// Controller resource, if any.
    pub bus: Option<ResourceId>,
    /// Address or chip-select routing within the bus.
    pub route: DeviceRoute,
    /// Current compile/bench evidence.
    pub support: SupportLevel,
}

/// Complete safe image for one serialized output engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafeOutputImage {
    /// I²S engine whose virtual bits are covered.
    pub engine: u8,
    /// Bits defined by this image. Unrouted/reserved bits remain excluded.
    pub defined_mask: u32,
    /// Values driven at boot, unconfigured state, fault, and watchdog expiry.
    pub safe_bits: u32,
    /// False until physical polarity and reset behavior have been reconciled.
    pub bench_verified: bool,
}

/// Full immutable package exported to firmware, simulator, and capability tools.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardPackage<'a> {
    /// Stable identity and flat typed-resource inventory.
    pub board: BoardDescriptor<'a>,
    /// Memory fitted to this exact module/revision.
    pub memory: MemoryDescriptor,
    /// Physical executor assignment.
    pub cores: CoreAssignment,
    /// Canonical configuration aliases.
    pub aliases: &'a [AliasDescriptor<'a>],
    /// Routed controller/pin groups.
    pub buses: &'a [BusDescriptor<'a>],
    /// Fitted device topology.
    pub devices: &'a [DeviceDescriptor],
    /// Serialized-engine boot/fault images.
    pub safe_output_images: &'a [SafeOutputImage],
    /// False prevents arming even when metadata validation succeeds.
    pub armable: bool,
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

impl BoardPackage<'_> {
    /// Validates cross-table topology, ownership, safe images, and arming claims.
    pub fn validate(&self) -> Result<(), BoardError> {
        self.board.validate()?;
        if self.memory.flash_bytes == 0 || self.memory.internal_sram_bytes == 0 {
            return Err(BoardError::MissingMemoryFacts);
        }
        if self.memory.realtime_psram_allowed {
            return Err(BoardError::RealtimePsramForbidden);
        }
        if self.cores.service_core == self.cores.realtime_core
            || self.cores.service_core >= self.board.application_cores
            || self.cores.realtime_core >= self.board.application_cores
        {
            return Err(BoardError::InvalidCoreAssignment {
                service: self.cores.service_core,
                realtime: self.cores.realtime_core,
                available: self.board.application_cores,
            });
        }

        for (index, alias) in self.aliases.iter().enumerate() {
            if alias.name.is_empty() {
                return Err(BoardError::EmptyAlias { index });
            }
            if self.resource(alias.resource).is_none() {
                return Err(BoardError::AliasMissingResource {
                    index,
                    resource: alias.resource,
                });
            }
            for (other_index, other) in self.aliases[..index].iter().enumerate() {
                if alias.name == other.name {
                    return Err(BoardError::DuplicateAlias {
                        first: other_index,
                        second: index,
                    });
                }
            }
        }

        for (index, bus) in self.buses.iter().enumerate() {
            let resource = self
                .resource(bus.resource)
                .ok_or(BoardError::BusMissingResource {
                    index,
                    resource: bus.resource,
                })?;
            if !bus_kind_matches(bus.kind, bus.resource) {
                return Err(BoardError::BusKindMismatch {
                    index,
                    resource: bus.resource,
                });
            }
            if resource.owner != bus.owner {
                return Err(BoardError::OwnershipMismatch {
                    resource: bus.resource,
                });
            }
            if bus.pins.is_empty() || bus.maximum_frequency_hz == 0 {
                return Err(BoardError::IncompleteBus { index });
            }
            for pin in bus.pins {
                let pin_descriptor = self.resource(*pin).ok_or(BoardError::BusMissingResource {
                    index,
                    resource: *pin,
                })?;
                if !matches!(pin, ResourceId::Gpio(_)) || pin_descriptor.owner != bus.owner {
                    return Err(BoardError::OwnershipMismatch { resource: *pin });
                }
            }
        }

        for (index, device) in self.devices.iter().enumerate() {
            let descriptor =
                self.resource(device.resource)
                    .ok_or(BoardError::DeviceMissingResource {
                        index,
                        resource: device.resource,
                    })?;
            if !matches!(
                device.resource,
                ResourceId::Device(_) | ResourceId::Storage(_)
            ) {
                return Err(BoardError::InvalidDeviceResource {
                    index,
                    resource: device.resource,
                });
            }
            if descriptor.owner != device.owner {
                return Err(BoardError::OwnershipMismatch {
                    resource: device.resource,
                });
            }
            match (device.bus, device.route) {
                (None, DeviceRoute::Dedicated) => {}
                (Some(bus), route) => {
                    let bus_descriptor = self
                        .buses
                        .iter()
                        .find(|candidate| candidate.resource == bus)
                        .ok_or(BoardError::DeviceMissingBus { index, bus })?;
                    if bus_descriptor.owner != device.owner
                        || !route_matches_bus(route, bus_descriptor.kind)
                    {
                        return Err(BoardError::DeviceRouteMismatch { index, bus });
                    }
                    if let DeviceRoute::SpiChipSelect(chip_select) = route {
                        let chip_select_descriptor = self.resource(chip_select).ok_or(
                            BoardError::DeviceMissingResource {
                                index,
                                resource: chip_select,
                            },
                        )?;
                        if !matches!(chip_select, ResourceId::Gpio(_))
                            || chip_select_descriptor.owner != device.owner
                        {
                            return Err(BoardError::DeviceRouteMismatch { index, bus });
                        }
                    }
                }
                _ => {
                    return Err(BoardError::DeviceRouteMismatch {
                        index,
                        bus: device.bus.unwrap_or(device.resource),
                    });
                }
            }
        }

        for (index, image) in self.safe_output_images.iter().enumerate() {
            if image.safe_bits & !image.defined_mask != 0 {
                return Err(BoardError::SafeImageOutsideMask {
                    engine: image.engine,
                });
            }
            let engine = ResourceId::I2s(image.engine);
            let descriptor = self
                .resource(engine)
                .ok_or(BoardError::SafeImageMissingEngine {
                    engine: image.engine,
                })?;
            if descriptor.owner != OwnerDomain::Realtime {
                return Err(BoardError::OwnershipMismatch { resource: engine });
            }
            if self.safe_output_images[..index]
                .iter()
                .any(|other| other.engine == image.engine)
            {
                return Err(BoardError::DuplicateSafeImage {
                    engine: image.engine,
                });
            }
            if self.armable && !image.bench_verified {
                return Err(BoardError::UnverifiedSafeImage {
                    engine: image.engine,
                });
            }
        }

        for resource in self.board.resources {
            let ResourceId::I2sOut { engine, bit } = resource.id else {
                continue;
            };
            if !resource.hazardous_output || resource.safe_value != SafeValue::EngineImage {
                continue;
            }
            if bit >= 32 {
                return Err(BoardError::SafeImageMissingBit { engine, bit });
            }
            let covered = self
                .safe_output_images
                .iter()
                .find(|image| image.engine == engine)
                .is_some_and(|image| image.defined_mask & (1_u32 << bit) != 0);
            if !covered {
                return Err(BoardError::SafeImageMissingBit { engine, bit });
            }
        }

        if self.armable && self.board.capability_digest.is_zero() {
            return Err(BoardError::ArmableWithoutCapabilityDigest);
        }
        Ok(())
    }

    /// Finds one flat resource descriptor by typed ID.
    pub fn resource(&self, id: ResourceId) -> Option<&ResourceDescriptor> {
        self.board
            .resources
            .iter()
            .find(|resource| resource.id == id)
    }
}

const fn bus_kind_matches(kind: BusKind, resource: ResourceId) -> bool {
    matches!(
        (kind, resource),
        (BusKind::I2c, ResourceId::I2c(_))
            | (BusKind::Spi, ResourceId::Spi(_))
            | (BusKind::Uart, ResourceId::Uart(_))
    )
}

const fn route_matches_bus(route: DeviceRoute, kind: BusKind) -> bool {
    matches!(
        (route, kind),
        (DeviceRoute::I2cAddress(0..=0x7f), BusKind::I2c)
            | (DeviceRoute::SpiChipSelect(_), BusKind::Spi)
            | (DeviceRoute::Uart, BusKind::Uart)
    )
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
    /// Flash or internal-SRAM size was not established.
    MissingMemoryFacts,
    /// Real-time state may never be placed in PSRAM.
    RealtimePsramForbidden,
    /// Service/realtime cores overlap or exceed the available core count.
    InvalidCoreAssignment {
        /// Selected service core.
        service: u8,
        /// Selected real-time core.
        realtime: u8,
        /// Available application cores.
        available: u8,
    },
    /// Alias was empty.
    EmptyAlias {
        /// Alias table index.
        index: usize,
    },
    /// Alias references a resource absent from the package.
    AliasMissingResource {
        /// Alias table index.
        index: usize,
        /// Missing typed resource.
        resource: ResourceId,
    },
    /// Two aliases use the same stable name.
    DuplicateAlias {
        /// Earlier table index.
        first: usize,
        /// Conflicting table index.
        second: usize,
    },
    /// Bus controller or one of its pins is absent.
    BusMissingResource {
        /// Bus table index.
        index: usize,
        /// Missing resource.
        resource: ResourceId,
    },
    /// Bus kind does not match its typed controller ID.
    BusKindMismatch {
        /// Bus table index.
        index: usize,
        /// Mismatched controller.
        resource: ResourceId,
    },
    /// Bus lacks pins or a nonzero admitted rate.
    IncompleteBus {
        /// Bus table index.
        index: usize,
    },
    /// A referenced resource belongs to another executor domain.
    OwnershipMismatch {
        /// Misowned resource.
        resource: ResourceId,
    },
    /// Device is absent from the flat resource inventory.
    DeviceMissingResource {
        /// Device table index.
        index: usize,
        /// Missing resource.
        resource: ResourceId,
    },
    /// Device table entry did not use a device/storage namespace.
    InvalidDeviceResource {
        /// Device table index.
        index: usize,
        /// Invalid resource.
        resource: ResourceId,
    },
    /// Device references a bus absent from the bus table.
    DeviceMissingBus {
        /// Device table index.
        index: usize,
        /// Missing bus.
        bus: ResourceId,
    },
    /// Device route/address is incompatible with its bus.
    DeviceRouteMismatch {
        /// Device table index.
        index: usize,
        /// Associated or expected bus.
        bus: ResourceId,
    },
    /// Safe image contains values outside its declared routed mask.
    SafeImageOutsideMask {
        /// Shift engine.
        engine: u8,
    },
    /// Safe image references no declared I²S engine.
    SafeImageMissingEngine {
        /// Missing shift engine.
        engine: u8,
    },
    /// Two safe images claim one engine.
    DuplicateSafeImage {
        /// Duplicate shift engine.
        engine: u8,
    },
    /// A hazardous shifted bit is not covered by a complete safe image.
    SafeImageMissingBit {
        /// Shift engine.
        engine: u8,
        /// Missing bit.
        bit: u8,
    },
    /// An armable board claimed a safe image without bench polarity evidence.
    UnverifiedSafeImage {
        /// Unverified shift engine.
        engine: u8,
    },
    /// Armable packages require a canonical nonzero capability digest.
    ArmableWithoutCapabilityDigest,
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

    fn package<'a>(
        resources: &'a [ResourceDescriptor],
        aliases: &'a [AliasDescriptor<'a>],
        buses: &'a [BusDescriptor<'a>],
        devices: &'a [DeviceDescriptor],
        safe_output_images: &'a [SafeOutputImage],
    ) -> BoardPackage<'a> {
        BoardPackage {
            board: board(resources),
            memory: MemoryDescriptor {
                flash_bytes: 8 * 1_024 * 1_024,
                internal_sram_bytes: 520 * 1_024,
                psram_bytes: 0,
                realtime_psram_allowed: false,
            },
            cores: CoreAssignment {
                service_core: 0,
                realtime_core: 1,
            },
            aliases,
            buses,
            devices,
            safe_output_images,
            armable: false,
        }
    }

    #[test]
    fn complete_shift_image_and_typed_bus_topology_validate() {
        let resources = [
            ResourceDescriptor {
                id: ResourceId::I2s(0),
                owner: OwnerDomain::Realtime,
                safe_value: SafeValue::EngineImage,
                hazardous_output: false,
            },
            STEP,
            ResourceDescriptor {
                id: ResourceId::Spi(2),
                owner: OwnerDomain::Service,
                safe_value: SafeValue::NotApplicable,
                hazardous_output: false,
            },
            ResourceDescriptor {
                id: ResourceId::Gpio(18),
                owner: OwnerDomain::Service,
                safe_value: SafeValue::Low,
                hazardous_output: false,
            },
            ResourceDescriptor {
                id: ResourceId::Gpio(5),
                owner: OwnerDomain::Service,
                safe_value: SafeValue::High,
                hazardous_output: false,
            },
            ResourceDescriptor {
                id: ResourceId::Storage(0),
                owner: OwnerDomain::Service,
                safe_value: SafeValue::NotApplicable,
                hazardous_output: false,
            },
        ];
        let aliases = [AliasDescriptor {
            name: "io129",
            resource: STEP.id,
        }];
        let bus_pins = [ResourceId::Gpio(18)];
        let buses = [BusDescriptor {
            resource: ResourceId::Spi(2),
            kind: BusKind::Spi,
            owner: OwnerDomain::Service,
            pins: &bus_pins,
            maximum_frequency_hz: 20_000_000,
        }];
        let devices = [DeviceDescriptor {
            resource: ResourceId::Storage(0),
            owner: OwnerDomain::Service,
            bus: Some(ResourceId::Spi(2)),
            route: DeviceRoute::SpiChipSelect(ResourceId::Gpio(5)),
            support: SupportLevel::Described,
        }];
        let images = [SafeOutputImage {
            engine: 0,
            defined_mask: 1 << 1,
            safe_bits: 0,
            bench_verified: false,
        }];

        assert_eq!(
            package(&resources, &aliases, &buses, &devices, &images).validate(),
            Ok(())
        );
    }

    #[test]
    fn hazardous_shift_bit_requires_safe_image_coverage() {
        let resources = [
            ResourceDescriptor {
                id: ResourceId::I2s(0),
                owner: OwnerDomain::Realtime,
                safe_value: SafeValue::EngineImage,
                hazardous_output: false,
            },
            STEP,
        ];
        let images = [SafeOutputImage {
            engine: 0,
            defined_mask: 1,
            safe_bits: 0,
            bench_verified: false,
        }];

        assert_eq!(
            package(&resources, &[], &[], &[], &images).validate(),
            Err(BoardError::SafeImageMissingBit { engine: 0, bit: 1 })
        );
    }

    #[test]
    fn duplicate_alias_and_cross_domain_device_bus_reject_atomically() {
        let resources = [
            ResourceDescriptor {
                id: ResourceId::I2c(0),
                owner: OwnerDomain::Service,
                safe_value: SafeValue::NotApplicable,
                hazardous_output: false,
            },
            ResourceDescriptor {
                id: ResourceId::Gpio(13),
                owner: OwnerDomain::Service,
                safe_value: SafeValue::HighImpedance,
                hazardous_output: false,
            },
            ResourceDescriptor {
                id: ResourceId::Device(0),
                owner: OwnerDomain::Realtime,
                safe_value: SafeValue::NotApplicable,
                hazardous_output: false,
            },
        ];
        let duplicate_aliases = [
            AliasDescriptor {
                name: "sda",
                resource: ResourceId::Gpio(13),
            },
            AliasDescriptor {
                name: "sda",
                resource: ResourceId::I2c(0),
            },
        ];
        assert_eq!(
            package(&resources, &duplicate_aliases, &[], &[], &[]).validate(),
            Err(BoardError::DuplicateAlias {
                first: 0,
                second: 1,
            })
        );

        let bus_pins = [ResourceId::Gpio(13)];
        let buses = [BusDescriptor {
            resource: ResourceId::I2c(0),
            kind: BusKind::I2c,
            owner: OwnerDomain::Service,
            pins: &bus_pins,
            maximum_frequency_hz: 400_000,
        }];
        let devices = [DeviceDescriptor {
            resource: ResourceId::Device(0),
            owner: OwnerDomain::Realtime,
            bus: Some(ResourceId::I2c(0)),
            route: DeviceRoute::I2cAddress(0x34),
            support: SupportLevel::Compiles,
        }];
        assert_eq!(
            package(&resources, &[], &buses, &devices, &[]).validate(),
            Err(BoardError::DeviceRouteMismatch {
                index: 0,
                bus: ResourceId::I2c(0),
            })
        );
    }
}
