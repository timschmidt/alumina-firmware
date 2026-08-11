#![no_std]
#![doc = "Compile-time facts for the dual-motor MKS ESP32 FOC V1.0 package."]

use alumina_board::{
    AliasDescriptor, BoardDescriptor, BoardPackage, BusDescriptor, BusKind, Chip, ClockDescriptor,
    ClockDomain, ClockSource, CoreAssignment, DeviceDescriptor, DeviceRoute,
    ElectricalConstraintDescriptor, ElectricalConstraintKind, HilKind, HilRequirement,
    InterruptDescriptor, InterruptTrigger, MemoryDescriptor, OwnerDomain, Qualification,
    ResourceDescriptor, ResourceId, SafeValue, SupportLevel,
};
use alumina_protocol::Digest;

/// Stable board/revision selection ID.
pub const BOARD_ID: &str = "mks-esp32-foc-v1";
/// ESP Rust target required by the fitted ESP32-WROOM-32D module.
pub const TARGET: &str = "xtensa-esp32-none-elf";
/// SHA-256 of the canonical `ALMCAP01` V1 document exported by this package.
pub const CAPABILITY_DIGEST: Digest = Digest([
    0x8b, 0x14, 0xc1, 0x7f, 0xc2, 0x78, 0x7b, 0xce, 0x93, 0xe1, 0x06, 0x10, 0xa3, 0x91, 0x57, 0xe3,
    0x3a, 0x5a, 0x10, 0xfa, 0xc4, 0x77, 0xe9, 0x10, 0x02, 0x1f, 0x16, 0x6b, 0x56, 0x25, 0x32, 0xe3,
]);

/// Stable board-local fitted-device namespace.
pub mod device {
    /// Motor-0 three-phase gate-driver and MOSFET stage.
    pub const POWER_STAGE_0: u16 = 0;
    /// Motor-1 three-phase gate-driver and MOSFET stage.
    pub const POWER_STAGE_1: u16 = 1;
    /// AS5600-compatible absolute-angle endpoint on encoder connector 0.
    pub const ENCODER_0: u16 = 2;
    /// AS5600-compatible absolute-angle endpoint on encoder connector 1.
    pub const ENCODER_1: u16 = 3;
}

const fn resource(
    id: ResourceId,
    owner: OwnerDomain,
    safe_value: SafeValue,
    hazardous_output: bool,
) -> ResourceDescriptor {
    ResourceDescriptor {
        id,
        owner,
        safe_value,
        hazardous_output,
    }
}

/// Complete currently established V1.0 resource inventory.
pub static RESOURCES: &[ResourceDescriptor] = &[
    resource(
        ResourceId::Radio(0),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Uart(0),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Gpio(1),
        OwnerDomain::Service,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(3),
        OwnerDomain::Service,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(0),
        OwnerDomain::Service,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(2),
        OwnerDomain::Service,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Timer { group: 0, index: 0 },
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Timer { group: 1, index: 0 },
        OwnerDomain::Realtime,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::TimedOutput {
            engine: 0,
            channel: 0,
        },
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        true,
    ),
    resource(
        ResourceId::TimedOutput {
            engine: 0,
            channel: 1,
        },
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        true,
    ),
    resource(
        ResourceId::TimedOutput {
            engine: 0,
            channel: 2,
        },
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        true,
    ),
    resource(
        ResourceId::Gpio(32),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        true,
    ),
    resource(
        ResourceId::Gpio(33),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        true,
    ),
    resource(
        ResourceId::Gpio(25),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        true,
    ),
    resource(
        ResourceId::TimedOutput {
            engine: 1,
            channel: 0,
        },
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        true,
    ),
    resource(
        ResourceId::TimedOutput {
            engine: 1,
            channel: 1,
        },
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        true,
    ),
    resource(
        ResourceId::TimedOutput {
            engine: 1,
            channel: 2,
        },
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        true,
    ),
    resource(
        ResourceId::Gpio(26),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        true,
    ),
    resource(
        ResourceId::Gpio(27),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        true,
    ),
    resource(
        ResourceId::Gpio(14),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        true,
    ),
    resource(
        ResourceId::Adc {
            unit: 1,
            channel: 3,
        },
        OwnerDomain::Realtime,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Gpio(39),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Adc {
            unit: 1,
            channel: 0,
        },
        OwnerDomain::Realtime,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Gpio(36),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Adc {
            unit: 1,
            channel: 7,
        },
        OwnerDomain::Realtime,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Gpio(35),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Adc {
            unit: 1,
            channel: 6,
        },
        OwnerDomain::Realtime,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Gpio(34),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::I2c(0),
        OwnerDomain::Realtime,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Gpio(18),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(19),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::I2c(1),
        OwnerDomain::Realtime,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Gpio(5),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(23),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(15),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(13),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Device(device::POWER_STAGE_0),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        true,
    ),
    resource(
        ResourceId::Device(device::POWER_STAGE_1),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        true,
    ),
    resource(
        ResourceId::Device(device::ENCODER_0),
        OwnerDomain::Realtime,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Device(device::ENCODER_1),
        OwnerDomain::Realtime,
        SafeValue::NotApplicable,
        false,
    ),
];

/// Canonical names used by configuration and board diagnostics.
pub static ALIASES: &[AliasDescriptor<'static>] = &[
    AliasDescriptor {
        name: "wifi",
        resource: ResourceId::Radio(0),
    },
    AliasDescriptor {
        name: "serial.usb",
        resource: ResourceId::Uart(0),
    },
    AliasDescriptor {
        name: "motor.0.power-stage",
        resource: ResourceId::Device(device::POWER_STAGE_0),
    },
    AliasDescriptor {
        name: "motor.0.phase.u",
        resource: ResourceId::TimedOutput {
            engine: 0,
            channel: 0,
        },
    },
    AliasDescriptor {
        name: "motor.0.phase.v",
        resource: ResourceId::TimedOutput {
            engine: 0,
            channel: 1,
        },
    },
    AliasDescriptor {
        name: "motor.0.phase.w",
        resource: ResourceId::TimedOutput {
            engine: 0,
            channel: 2,
        },
    },
    AliasDescriptor {
        name: "motor.0.current.a",
        resource: ResourceId::Adc {
            unit: 1,
            channel: 3,
        },
    },
    AliasDescriptor {
        name: "motor.0.current.b",
        resource: ResourceId::Adc {
            unit: 1,
            channel: 0,
        },
    },
    AliasDescriptor {
        name: "encoder.0.i2c",
        resource: ResourceId::I2c(0),
    },
    AliasDescriptor {
        name: "encoder.0.sensor",
        resource: ResourceId::Device(device::ENCODER_0),
    },
    AliasDescriptor {
        name: "encoder.0.index",
        resource: ResourceId::Gpio(15),
    },
    AliasDescriptor {
        name: "motor.1.power-stage",
        resource: ResourceId::Device(device::POWER_STAGE_1),
    },
    AliasDescriptor {
        name: "motor.1.phase.u",
        resource: ResourceId::TimedOutput {
            engine: 1,
            channel: 0,
        },
    },
    AliasDescriptor {
        name: "motor.1.phase.v",
        resource: ResourceId::TimedOutput {
            engine: 1,
            channel: 1,
        },
    },
    AliasDescriptor {
        name: "motor.1.phase.w",
        resource: ResourceId::TimedOutput {
            engine: 1,
            channel: 2,
        },
    },
    AliasDescriptor {
        name: "motor.1.current.a",
        resource: ResourceId::Adc {
            unit: 1,
            channel: 7,
        },
    },
    AliasDescriptor {
        name: "motor.1.current.b",
        resource: ResourceId::Adc {
            unit: 1,
            channel: 6,
        },
    },
    AliasDescriptor {
        name: "encoder.1.i2c",
        resource: ResourceId::I2c(1),
    },
    AliasDescriptor {
        name: "encoder.1.sensor",
        resource: ResourceId::Device(device::ENCODER_1),
    },
    AliasDescriptor {
        name: "encoder.1.index",
        resource: ResourceId::Gpio(13),
    },
];

static I2C0_PINS: &[ResourceId] = &[ResourceId::Gpio(18), ResourceId::Gpio(19)];
static I2C1_PINS: &[ResourceId] = &[ResourceId::Gpio(5), ResourceId::Gpio(23)];
static UART0_PINS: &[ResourceId] = &[ResourceId::Gpio(1), ResourceId::Gpio(3)];

/// Routed serial controllers. Encoder buses belong to core 1.
pub static BUSES: &[BusDescriptor<'static>] = &[
    BusDescriptor {
        resource: ResourceId::I2c(0),
        kind: BusKind::I2c,
        owner: OwnerDomain::Realtime,
        pins: I2C0_PINS,
        maximum_frequency_hz: 400_000,
    },
    BusDescriptor {
        resource: ResourceId::I2c(1),
        kind: BusKind::I2c,
        owner: OwnerDomain::Realtime,
        pins: I2C1_PINS,
        maximum_frequency_hz: 400_000,
    },
    BusDescriptor {
        resource: ResourceId::Uart(0),
        kind: BusKind::Uart,
        owner: OwnerDomain::Service,
        pins: UART0_PINS,
        maximum_frequency_hz: 921_600,
    },
];

static MOTOR0_AUXILIARY: &[ResourceId] = &[
    ResourceId::TimedOutput {
        engine: 0,
        channel: 0,
    },
    ResourceId::TimedOutput {
        engine: 0,
        channel: 1,
    },
    ResourceId::TimedOutput {
        engine: 0,
        channel: 2,
    },
    ResourceId::Gpio(32),
    ResourceId::Gpio(33),
    ResourceId::Gpio(25),
    ResourceId::Adc {
        unit: 1,
        channel: 3,
    },
    ResourceId::Adc {
        unit: 1,
        channel: 0,
    },
];
static MOTOR1_AUXILIARY: &[ResourceId] = &[
    ResourceId::TimedOutput {
        engine: 1,
        channel: 0,
    },
    ResourceId::TimedOutput {
        engine: 1,
        channel: 1,
    },
    ResourceId::TimedOutput {
        engine: 1,
        channel: 2,
    },
    ResourceId::Gpio(26),
    ResourceId::Gpio(27),
    ResourceId::Gpio(14),
    ResourceId::Adc {
        unit: 1,
        channel: 7,
    },
    ResourceId::Adc {
        unit: 1,
        channel: 6,
    },
];

static NO_AUXILIARY_RESOURCES: &[ResourceId] = &[];

/// Two fitted but still unqualified power stages and two compile-supported
/// AS5600-compatible encoder endpoints exposed by the board connectors.
pub static DEVICES: &[DeviceDescriptor<'static>] = &[
    DeviceDescriptor {
        resource: ResourceId::Device(device::POWER_STAGE_0),
        owner: OwnerDomain::Realtime,
        bus: None,
        route: DeviceRoute::Dedicated,
        auxiliary_resources: MOTOR0_AUXILIARY,
        support: SupportLevel::Described,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::POWER_STAGE_1),
        owner: OwnerDomain::Realtime,
        bus: None,
        route: DeviceRoute::Dedicated,
        auxiliary_resources: MOTOR1_AUXILIARY,
        support: SupportLevel::Described,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::ENCODER_0),
        owner: OwnerDomain::Realtime,
        bus: Some(ResourceId::I2c(0)),
        route: DeviceRoute::I2cAddress(0x36),
        auxiliary_resources: NO_AUXILIARY_RESOURCES,
        support: SupportLevel::Compiles,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::ENCODER_1),
        owner: OwnerDomain::Realtime,
        bus: Some(ResourceId::I2c(1)),
        route: DeviceRoute::I2cAddress(0x36),
        auxiliary_resources: NO_AUXILIARY_RESOURCES,
        support: SupportLevel::Compiles,
    },
];

/// Declared chip/module clocks; no error bound is yet admitted.
pub static CLOCKS: &[ClockDescriptor<'static>] = &[
    ClockDescriptor {
        name: "xtal",
        source: ClockSource::Crystal,
        nominal_hz: 40_000_000,
        maximum_error_ppm: None,
        domain: ClockDomain::Chip,
        support: SupportLevel::Described,
    },
    ClockDescriptor {
        name: "cpu",
        source: ClockSource::Pll,
        nominal_hz: 240_000_000,
        maximum_error_ppm: None,
        domain: ClockDomain::Chip,
        support: SupportLevel::Compiles,
    },
    ClockDescriptor {
        name: "apb",
        source: ClockSource::PeripheralBus,
        nominal_hz: 80_000_000,
        maximum_error_ppm: None,
        domain: ClockDomain::Realtime,
        support: SupportLevel::Described,
    },
];

static CURRENT_INPUTS: &[ResourceId] = &[
    ResourceId::Gpio(39),
    ResourceId::Gpio(36),
    ResourceId::Gpio(35),
    ResourceId::Gpio(34),
];
static PHASE_OUTPUTS: &[ResourceId] = &[
    ResourceId::Gpio(32),
    ResourceId::Gpio(33),
    ResourceId::Gpio(25),
    ResourceId::Gpio(26),
    ResourceId::Gpio(27),
    ResourceId::Gpio(14),
];
static POWER_STAGES: &[ResourceId] = &[
    ResourceId::Device(device::POWER_STAGE_0),
    ResourceId::Device(device::POWER_STAGE_1),
];
static ENCODER_ROUTES: &[ResourceId] = &[
    ResourceId::I2c(0),
    ResourceId::I2c(1),
    ResourceId::Gpio(18),
    ResourceId::Gpio(19),
    ResourceId::Gpio(5),
    ResourceId::Gpio(23),
    ResourceId::Gpio(15),
    ResourceId::Gpio(13),
];
static BOOT_STRAPS: &[ResourceId] = &[
    ResourceId::Gpio(0),
    ResourceId::Gpio(2),
    ResourceId::Gpio(5),
    ResourceId::Gpio(15),
];
static LOGIC_BUSES: &[ResourceId] = &[ResourceId::I2c(0), ResourceId::I2c(1)];

/// Electrical constraints that block inferred PWM/current/sensor claims.
pub static ELECTRICAL_CONSTRAINTS: &[ElectricalConstraintDescriptor<'static>] = &[
    ElectricalConstraintDescriptor {
        id: "esp32.adc1-input-only",
        kind: ElectricalConstraintKind::InputOnly,
        resources: CURRENT_INPUTS,
        note: "all four current-amplifier outputs route to ADC1-capable input-only GPIOs",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "mks-foc.phase-output-only",
        kind: ElectricalConstraintKind::OutputOnly,
        resources: PHASE_OUTPUTS,
        note: "six direct 3-PWM inputs route to the two fitted gate-driver stages",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "mks-foc.no-independent-enable",
        kind: ElectricalConstraintKind::ResetStateUnverified,
        resources: POWER_STAGES,
        note: "V1.0 schematic marks GPIO22 and GPIO12 unconnected; no independent inverter enable is established",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "mks-foc.phase-reset-state",
        kind: ElectricalConstraintKind::ResetStateUnverified,
        resources: PHASE_OUTPUTS,
        note: "EG2133 HIN pull-down/LIN-bar pull-up make high impedance the documented off candidate; board-level reset and fault behavior still require disconnected-load measurement",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "mks-foc.encoder-mode-multiplex",
        kind: ElectricalConstraintKind::SharedRoute,
        resources: ENCODER_ROUTES,
        note: "each connector is configured as I2C, SPI/ABI, PWM, or Hall rather than simultaneous protocols",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "esp32.boot-straps",
        kind: ElectricalConstraintKind::BootStrap,
        resources: BOOT_STRAPS,
        note: "encoder or expansion loads must preserve reset and download-mode strap levels",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "mks-foc.logic-level",
        kind: ElectricalConstraintKind::Logic3v3,
        resources: LOGIC_BUSES,
        note: "encoder signal pins are ESP32 3.3 V logic even though connector power also exposes 5 V",
        support: SupportLevel::Described,
    },
];

/// Encoder/expansion inputs that may later become hardware-timed observations.
pub static INTERRUPTS: &[InterruptDescriptor] = &[
    InterruptDescriptor {
        source: ResourceId::Gpio(15),
        owner: OwnerDomain::Realtime,
        trigger: InterruptTrigger::AnyEdge,
        maximum_latency_cycles: None,
        support: SupportLevel::Described,
    },
    InterruptDescriptor {
        source: ResourceId::Gpio(13),
        owner: OwnerDomain::Realtime,
        trigger: InterruptTrigger::AnyEdge,
        maximum_latency_cycles: None,
        support: SupportLevel::Described,
    },
];

static SAFE_HIL_RESOURCES: &[ResourceId] = &[
    ResourceId::Device(device::POWER_STAGE_0),
    ResourceId::Device(device::POWER_STAGE_1),
    ResourceId::Gpio(32),
    ResourceId::Gpio(33),
    ResourceId::Gpio(25),
    ResourceId::Gpio(26),
    ResourceId::Gpio(27),
    ResourceId::Gpio(14),
];
static CURRENT_HIL_RESOURCES: &[ResourceId] = &[
    ResourceId::Adc {
        unit: 1,
        channel: 3,
    },
    ResourceId::Adc {
        unit: 1,
        channel: 0,
    },
    ResourceId::Adc {
        unit: 1,
        channel: 7,
    },
    ResourceId::Adc {
        unit: 1,
        channel: 6,
    },
];
static ENCODER_HIL_RESOURCES: &[ResourceId] = &[
    ResourceId::I2c(0),
    ResourceId::I2c(1),
    ResourceId::Device(device::ENCODER_0),
    ResourceId::Device(device::ENCODER_1),
];
static TIMING_HIL_RESOURCES: &[ResourceId] = &[
    ResourceId::Timer { group: 1, index: 0 },
    ResourceId::Device(device::POWER_STAGE_0),
    ResourceId::Device(device::POWER_STAGE_1),
];
static CORE_HIL_RESOURCES: &[ResourceId] = &[
    ResourceId::Timer { group: 0, index: 0 },
    ResourceId::Timer { group: 1, index: 0 },
];

/// Evidence required before the package may advance beyond compile status.
pub static HIL_REQUIREMENTS: &[HilRequirement<'static>] = &[
    HilRequirement {
        id: "identity.v1-module-and-routing",
        kind: HilKind::BoardIdentity,
        resources: &[],
        required_for: Qualification::Bench,
    },
    HilRequirement {
        id: "safe.six-phase-reset-fault-watchdog",
        kind: HilKind::SafeState,
        resources: SAFE_HIL_RESOURCES,
        required_for: Qualification::Bench,
    },
    HilRequirement {
        id: "current.offset-gain-polarity",
        kind: HilKind::PeripheralSmoke,
        resources: CURRENT_HIL_RESOURCES,
        required_for: Qualification::Bench,
    },
    HilRequirement {
        id: "encoder.dual-connectors",
        kind: HilKind::PeripheralSmoke,
        resources: ENCODER_HIL_RESOURCES,
        required_for: Qualification::Bench,
    },
    HilRequirement {
        id: "timing.pwm-adc-phase-and-wcet",
        kind: HilKind::Timing,
        resources: TIMING_HIL_RESOURCES,
        required_for: Qualification::MotionQualified,
    },
    HilRequirement {
        id: "fault.current-sensor-runaway-shutdown",
        kind: HilKind::FaultInjection,
        resources: SAFE_HIL_RESOURCES,
        required_for: Qualification::MotionQualified,
    },
    HilRequirement {
        id: "timing.core0-wifi-saturation",
        kind: HilKind::CoreIsolation,
        resources: CORE_HIL_RESOURCES,
        required_for: Qualification::MotionQualified,
    },
    HilRequirement {
        id: "visual.top-hotspots",
        kind: HilKind::VisualReconciliation,
        resources: &[],
        required_for: Qualification::Bench,
    },
];

/// Canonical non-armable V1.0 package.
pub static PACKAGE: BoardPackage<'static> = BoardPackage {
    board: BoardDescriptor {
        id: BOARD_ID,
        revision: "V1.0; physical module and assembly reconciliation pending",
        chip: Chip::Esp32,
        application_cores: 2,
        qualification: Qualification::Compiles,
        capability_digest: CAPABILITY_DIGEST,
        resources: RESOURCES,
    },
    memory: MemoryDescriptor {
        flash_bytes: 4 * 1_024 * 1_024,
        internal_sram_bytes: 520 * 1_024,
        psram_bytes: 0,
        realtime_psram_allowed: false,
    },
    cores: CoreAssignment {
        service_core: 0,
        realtime_core: 1,
    },
    aliases: ALIASES,
    buses: BUSES,
    devices: DEVICES,
    flash_regions: &[],
    clocks: CLOCKS,
    electrical_constraints: ELECTRICAL_CONSTRAINTS,
    interrupts: INTERRUPTS,
    safe_output_images: &[],
    visuals: &[],
    hil_requirements: HIL_REQUIREMENTS,
    armable: false,
};

#[cfg(test)]
mod tests {
    use alumina_capability::calculate_identity;

    use super::*;

    #[test]
    fn package_validates_but_cannot_arm() {
        assert_eq!(PACKAGE.validate(), Ok(()));
        assert!(!PACKAGE.armable);
        assert_eq!(PACKAGE.board.qualification, Qualification::Compiles);
    }

    #[test]
    fn schematic_routes_two_complete_power_stages() {
        let power_stages = &DEVICES[..2];
        assert_eq!(power_stages.len(), 2);
        assert!(power_stages.iter().all(|stage| {
            stage.owner == OwnerDomain::Realtime
                && stage.support == SupportLevel::Described
                && stage.auxiliary_resources.len() == 8
        }));
        assert!(PHASE_OUTPUTS.iter().all(|id| {
            PACKAGE.resource(*id).is_some_and(|resource| {
                resource.hazardous_output && resource.safe_value == SafeValue::HighImpedance
            })
        }));
        assert!(
            ALIASES
                .iter()
                .filter(|alias| alias.name.contains(".phase."))
                .all(
                    |alias| PACKAGE.resource(alias.resource).is_some_and(|resource| {
                        resource.hazardous_output && resource.safe_value == SafeValue::HighImpedance
                    })
                )
        );
    }

    #[test]
    fn encoder_endpoints_are_distinct_compile_supported_as5600_routes() {
        let encoders = &DEVICES[2..];
        assert_eq!(encoders.len(), 2);
        for (index, encoder) in encoders.iter().enumerate() {
            assert_eq!(encoder.resource, ResourceId::Device(2 + index as u16));
            assert_eq!(encoder.owner, OwnerDomain::Realtime);
            assert_eq!(encoder.bus, Some(ResourceId::I2c(index as u8)));
            assert_eq!(encoder.route, DeviceRoute::I2cAddress(0x36));
            assert_eq!(encoder.support, SupportLevel::Compiles);
        }
    }

    #[test]
    fn current_channels_are_adc1_and_match_schematic_gpio_order() {
        let current_aliases = [
            (
                "motor.0.current.a",
                ResourceId::Adc {
                    unit: 1,
                    channel: 3,
                },
            ),
            (
                "motor.0.current.b",
                ResourceId::Adc {
                    unit: 1,
                    channel: 0,
                },
            ),
            (
                "motor.1.current.a",
                ResourceId::Adc {
                    unit: 1,
                    channel: 7,
                },
            ),
            (
                "motor.1.current.b",
                ResourceId::Adc {
                    unit: 1,
                    channel: 6,
                },
            ),
        ];
        for (name, expected) in current_aliases {
            assert_eq!(
                ALIASES
                    .iter()
                    .find(|alias| alias.name == name)
                    .unwrap()
                    .resource,
                expected
            );
        }
    }

    #[test]
    fn no_fictional_enable_or_storage_resource_is_exposed() {
        assert!(PACKAGE.resource(ResourceId::Gpio(22)).is_none());
        assert!(PACKAGE.resource(ResourceId::Gpio(12)).is_none());
        assert!(PACKAGE.resource(ResourceId::Storage(0)).is_none());
        assert!(!ALIASES.iter().any(|alias| alias.name.contains("enable")));
        assert!(
            PACKAGE
                .resource(ResourceId::Gpio(2))
                .is_some_and(|resource| {
                    resource.owner == OwnerDomain::Service
                        && resource.safe_value == SafeValue::HighImpedance
                        && !resource.hazardous_output
                })
        );
        assert!(
            !INTERRUPTS
                .iter()
                .any(|interrupt| interrupt.source == ResourceId::Gpio(2))
        );
    }

    #[test]
    fn capability_identity_matches_canonical_document() {
        let calculated = calculate_identity(&PACKAGE).unwrap();
        assert_eq!(CAPABILITY_DIGEST.0, calculated.digest.0);
    }
}
