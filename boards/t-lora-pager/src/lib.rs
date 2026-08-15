#![no_std]
#![doc = "Compile-time facts for the non-armable current LILYGO T-LoRa Pager stub."]

use alumina_board::{
    AliasDescriptor, BoardDescriptor, BoardPackage, BusDescriptor, BusKind, Chip, ClockDescriptor,
    ClockDomain, ClockSource, CoreAssignment, DeviceDescriptor, DeviceRoute,
    DiagnosticOverviewDescriptor, DigitalCaptureDescriptor, ElectricalConstraintDescriptor,
    ElectricalConstraintKind, GraphExecutorDescriptor, GraphOpcodeDescriptor, GraphResourceAccess,
    GraphResourceClass, HilKind, HilRequirement, InterruptDescriptor, InterruptTrigger,
    MemoryDescriptor, OwnerDomain, Qualification, ResourceDescriptor, ResourceId, SafeValue,
    SupportLevel,
};
use alumina_protocol::Digest;

/// Stable identity for the current product-family definition.
pub const BOARD_ID: &str = "t-lora-pager-current";
/// Rust target required by the fitted dual-core ESP32-S3.
pub const TARGET: &str = "xtensa-esp32s3-none-elf";
/// Upstream LilyGoLib revision used only as a hardware fact source.
pub const HARDWARE_SOURCE_REVISION: &str = "38e6f8dee3ba78b340512af9a013365ef248a7d0";

pub const GRAPH_STABLE_BOOLEAN_INPUT_CLASS: GraphResourceClass = GraphResourceClass::new(1);
pub static GRAPH_OPCODES: &[GraphOpcodeDescriptor] = &[
    GraphOpcodeDescriptor {
        opcode: 1,
        domain: OwnerDomain::Service,
        support: SupportLevel::Compiles,
        resource_class: None,
        resource_access: None,
    },
    GraphOpcodeDescriptor {
        opcode: 2,
        domain: OwnerDomain::Realtime,
        support: SupportLevel::Compiles,
        resource_class: None,
        resource_access: None,
    },
    GraphOpcodeDescriptor {
        opcode: 3,
        domain: OwnerDomain::Realtime,
        support: SupportLevel::Compiles,
        resource_class: None,
        resource_access: None,
    },
    GraphOpcodeDescriptor {
        opcode: 4,
        domain: OwnerDomain::Realtime,
        support: SupportLevel::Compiles,
        resource_class: Some(GRAPH_STABLE_BOOLEAN_INPUT_CLASS),
        resource_access: Some(GraphResourceAccess::StableBooleanInput),
    },
];
pub const GRAPH_EXECUTOR: GraphExecutorDescriptor<'static> = GraphExecutorDescriptor {
    ir_version: 2,
    package_bytes: 4_096,
    maximum_nodes: 32,
    maximum_channels: 64,
    maximum_queue_items: 4_096,
    service_state_bytes: 2 * 1_024,
    realtime_state_bytes: 2 * 1_024,
    service_channel_bytes: 4 * 1_024,
    realtime_channel_bytes: 4 * 1_024,
    bridge_channel_bytes: 4 * 1_024,
    support: SupportLevel::Compiles,
    opcodes: GRAPH_OPCODES,
    resources: &[],
};

/// No physical overview provider is composed by this compile-only image.
pub const DIAGNOSTIC_OVERVIEW: DiagnosticOverviewDescriptor<'static> =
    DiagnosticOverviewDescriptor::NONE;
/// No physical waveform-acquisition backend is composed by this image.
pub const DIGITAL_CAPTURE: DigitalCaptureDescriptor<'static> = DigitalCaptureDescriptor::NONE;
/// SHA-256 of the canonical `ALMCAP04` V4 document exported by this package.
pub const CAPABILITY_DIGEST: Digest = Digest([
    0x38, 0xb4, 0x50, 0x49, 0x6c, 0xb2, 0xa5, 0x3d, 0x18, 0x8e, 0xff, 0x6f, 0x06, 0x06, 0x1b, 0x68,
    0xdf, 0xfc, 0x6a, 0x29, 0x57, 0x3a, 0x09, 0x3d, 0x0e, 0x01, 0x2a, 0xc1, 0xe7, 0x67, 0x2d, 0x1a,
]);

/// Stable board-local fitted-device namespace.
pub mod device {
    pub const AUDIO_CODEC: u16 = 0;
    pub const GPIO_EXPANDER: u16 = 1;
    pub const SMART_SENSOR: u16 = 2;
    pub const KEYBOARD: u16 = 3;
    pub const RTC: u16 = 4;
    pub const FUEL_GAUGE: u16 = 5;
    pub const HAPTIC_DRIVER: u16 = 6;
    pub const BATTERY_CHARGER: u16 = 7;
    pub const DISPLAY: u16 = 8;
    pub const GNSS: u16 = 9;
    pub const NFC: u16 = 10;
    pub const LORA_MODULE: u16 = 11;
    pub const AUDIO_AMPLIFIER: u16 = 12;
    pub const BACKLIGHT_DRIVER: u16 = 13;
    pub const ROTARY_ENCODER: u16 = 14;
}

const fn resource(id: ResourceId, owner: OwnerDomain, safe_value: SafeValue) -> ResourceDescriptor {
    ResourceDescriptor {
        id,
        owner,
        safe_value,
        hazardous_output: false,
    }
}

const fn service_gpio(pin: u8) -> ResourceDescriptor {
    resource(
        ResourceId::Gpio(pin),
        OwnerDomain::Service,
        SafeValue::HighImpedance,
    )
}

pub static RESOURCES: &[ResourceDescriptor] = &[
    resource(
        ResourceId::I2c(0),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Spi(2),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Uart(1),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Uart(2),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::I2s(0),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Storage(0),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Radio(0),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    service_gpio(0),
    service_gpio(1),
    service_gpio(2),
    service_gpio(3),
    service_gpio(4),
    service_gpio(5),
    service_gpio(6),
    service_gpio(7),
    service_gpio(8),
    service_gpio(10),
    service_gpio(11),
    service_gpio(12),
    service_gpio(13),
    service_gpio(14),
    service_gpio(17),
    service_gpio(18),
    service_gpio(21),
    service_gpio(33),
    service_gpio(34),
    service_gpio(35),
    service_gpio(36),
    service_gpio(37),
    service_gpio(38),
    service_gpio(39),
    service_gpio(40),
    service_gpio(41),
    service_gpio(42),
    service_gpio(43),
    service_gpio(44),
    service_gpio(45),
    service_gpio(46),
    service_gpio(47),
    service_gpio(48),
    resource(
        ResourceId::Device(device::AUDIO_CODEC),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Device(device::GPIO_EXPANDER),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Device(device::SMART_SENSOR),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Device(device::KEYBOARD),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Device(device::RTC),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Device(device::FUEL_GAUGE),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Device(device::HAPTIC_DRIVER),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Device(device::BATTERY_CHARGER),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Device(device::DISPLAY),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Device(device::GNSS),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Device(device::NFC),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Device(device::LORA_MODULE),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Device(device::AUDIO_AMPLIFIER),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Device(device::BACKLIGHT_DRIVER),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Device(device::ROTARY_ENCODER),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Timer { group: 0, index: 0 },
        OwnerDomain::Service,
        SafeValue::NotApplicable,
    ),
    resource(
        ResourceId::Timer { group: 1, index: 0 },
        OwnerDomain::Realtime,
        SafeValue::NotApplicable,
    ),
];

pub static ALIASES: &[AliasDescriptor<'static>] = &[
    AliasDescriptor {
        name: "i2c.shared",
        resource: ResourceId::I2c(0),
    },
    AliasDescriptor {
        name: "spi.shared",
        resource: ResourceId::Spi(2),
    },
    AliasDescriptor {
        name: "serial.external",
        resource: ResourceId::Uart(1),
    },
    AliasDescriptor {
        name: "serial.gnss",
        resource: ResourceId::Uart(2),
    },
    AliasDescriptor {
        name: "audio.i2s",
        resource: ResourceId::I2s(0),
    },
    AliasDescriptor {
        name: "storage.sd",
        resource: ResourceId::Storage(0),
    },
    AliasDescriptor {
        name: "display.lcd",
        resource: ResourceId::Device(device::DISPLAY),
    },
    AliasDescriptor {
        name: "position.gnss",
        resource: ResourceId::Device(device::GNSS),
    },
    AliasDescriptor {
        name: "radio.lora-module",
        resource: ResourceId::Device(device::LORA_MODULE),
    },
    AliasDescriptor {
        name: "radio.nfc",
        resource: ResourceId::Device(device::NFC),
    },
    AliasDescriptor {
        name: "input.keyboard",
        resource: ResourceId::Device(device::KEYBOARD),
    },
    AliasDescriptor {
        name: "input.rotary",
        resource: ResourceId::Device(device::ROTARY_ENCODER),
    },
    AliasDescriptor {
        name: "sensor.imu",
        resource: ResourceId::Device(device::SMART_SENSOR),
    },
    AliasDescriptor {
        name: "power.charger",
        resource: ResourceId::Device(device::BATTERY_CHARGER),
    },
    AliasDescriptor {
        name: "power.fuel-gauge",
        resource: ResourceId::Device(device::FUEL_GAUGE),
    },
    AliasDescriptor {
        name: "output.haptic",
        resource: ResourceId::Device(device::HAPTIC_DRIVER),
    },
    AliasDescriptor {
        name: "audio.codec",
        resource: ResourceId::Device(device::AUDIO_CODEC),
    },
    AliasDescriptor {
        name: "audio.amplifier",
        resource: ResourceId::Device(device::AUDIO_AMPLIFIER),
    },
    AliasDescriptor {
        name: "input.boot",
        resource: ResourceId::Gpio(0),
    },
];

static I2C_PINS: &[ResourceId] = &[ResourceId::Gpio(2), ResourceId::Gpio(3)];
static SPI_PINS: &[ResourceId] = &[
    ResourceId::Gpio(35),
    ResourceId::Gpio(34),
    ResourceId::Gpio(33),
];
static EXTERNAL_UART_PINS: &[ResourceId] = &[ResourceId::Gpio(43), ResourceId::Gpio(44)];
static GNSS_UART_PINS: &[ResourceId] = &[ResourceId::Gpio(12), ResourceId::Gpio(4)];

pub static BUSES: &[BusDescriptor<'static>] = &[
    BusDescriptor {
        resource: ResourceId::I2c(0),
        kind: BusKind::I2c,
        owner: OwnerDomain::Service,
        pins: I2C_PINS,
        maximum_frequency_hz: 400_000,
    },
    BusDescriptor {
        resource: ResourceId::Spi(2),
        kind: BusKind::Spi,
        owner: OwnerDomain::Service,
        pins: SPI_PINS,
        maximum_frequency_hz: 20_000_000,
    },
    BusDescriptor {
        resource: ResourceId::Uart(1),
        kind: BusKind::Uart,
        owner: OwnerDomain::Service,
        pins: EXTERNAL_UART_PINS,
        maximum_frequency_hz: 921_600,
    },
    BusDescriptor {
        resource: ResourceId::Uart(2),
        kind: BusKind::Uart,
        owner: OwnerDomain::Service,
        pins: GNSS_UART_PINS,
        maximum_frequency_hz: 115_200,
    },
];

static AUDIO_AUXILIARY: &[ResourceId] = &[
    ResourceId::I2s(0),
    ResourceId::Gpio(10),
    ResourceId::Gpio(11),
    ResourceId::Gpio(17),
    ResourceId::Gpio(18),
    ResourceId::Gpio(45),
];
static SMART_SENSOR_AUXILIARY: &[ResourceId] = &[ResourceId::Gpio(8)];
static KEYBOARD_AUXILIARY: &[ResourceId] = &[
    ResourceId::Gpio(6),
    ResourceId::Gpio(46),
    ResourceId::Device(device::GPIO_EXPANDER),
];
static RTC_AUXILIARY: &[ResourceId] = &[ResourceId::Gpio(1)];
static HAPTIC_AUXILIARY: &[ResourceId] = &[ResourceId::Device(device::GPIO_EXPANDER)];
static DISPLAY_AUXILIARY: &[ResourceId] = &[ResourceId::Gpio(37), ResourceId::Gpio(42)];
static GNSS_AUXILIARY: &[ResourceId] = &[
    ResourceId::Gpio(13),
    ResourceId::Device(device::GPIO_EXPANDER),
];
static NFC_AUXILIARY: &[ResourceId] = &[
    ResourceId::Gpio(5),
    ResourceId::Device(device::GPIO_EXPANDER),
];
static LORA_AUXILIARY: &[ResourceId] = &[
    ResourceId::Gpio(14),
    ResourceId::Gpio(47),
    ResourceId::Gpio(48),
    ResourceId::Device(device::GPIO_EXPANDER),
];
static SD_AUXILIARY: &[ResourceId] = &[ResourceId::Device(device::GPIO_EXPANDER)];
static AMPLIFIER_AUXILIARY: &[ResourceId] = &[
    ResourceId::I2s(0),
    ResourceId::Device(device::GPIO_EXPANDER),
];
static BACKLIGHT_AUXILIARY: &[ResourceId] = &[ResourceId::Gpio(42)];
static ROTARY_AUXILIARY: &[ResourceId] = &[
    ResourceId::Gpio(40),
    ResourceId::Gpio(41),
    ResourceId::Gpio(7),
];

pub static DEVICES: &[DeviceDescriptor<'static>] = &[
    DeviceDescriptor {
        resource: ResourceId::Device(device::AUDIO_CODEC),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::I2c(0)),
        route: DeviceRoute::I2cAddress(0x18),
        auxiliary_resources: AUDIO_AUXILIARY,
        support: SupportLevel::Described,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::GPIO_EXPANDER),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::I2c(0)),
        route: DeviceRoute::I2cAddress(0x20),
        auxiliary_resources: &[],
        support: SupportLevel::Described,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::SMART_SENSOR),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::I2c(0)),
        route: DeviceRoute::I2cAddress(0x28),
        auxiliary_resources: SMART_SENSOR_AUXILIARY,
        support: SupportLevel::Described,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::KEYBOARD),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::I2c(0)),
        route: DeviceRoute::I2cAddress(0x34),
        auxiliary_resources: KEYBOARD_AUXILIARY,
        support: SupportLevel::Described,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::RTC),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::I2c(0)),
        route: DeviceRoute::I2cAddress(0x51),
        auxiliary_resources: RTC_AUXILIARY,
        support: SupportLevel::Described,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::FUEL_GAUGE),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::I2c(0)),
        route: DeviceRoute::I2cAddress(0x55),
        auxiliary_resources: &[],
        support: SupportLevel::Described,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::HAPTIC_DRIVER),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::I2c(0)),
        route: DeviceRoute::I2cAddress(0x5a),
        auxiliary_resources: HAPTIC_AUXILIARY,
        support: SupportLevel::Described,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::BATTERY_CHARGER),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::I2c(0)),
        route: DeviceRoute::I2cAddress(0x6b),
        auxiliary_resources: &[],
        support: SupportLevel::Described,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::DISPLAY),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::Spi(2)),
        route: DeviceRoute::SpiChipSelect(ResourceId::Gpio(38)),
        auxiliary_resources: DISPLAY_AUXILIARY,
        support: SupportLevel::Described,
    },
    DeviceDescriptor {
        resource: ResourceId::Storage(0),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::Spi(2)),
        route: DeviceRoute::SpiChipSelect(ResourceId::Gpio(21)),
        auxiliary_resources: SD_AUXILIARY,
        support: SupportLevel::Described,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::GNSS),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::Uart(2)),
        route: DeviceRoute::Uart,
        auxiliary_resources: GNSS_AUXILIARY,
        support: SupportLevel::Described,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::NFC),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::Spi(2)),
        route: DeviceRoute::SpiChipSelect(ResourceId::Gpio(39)),
        auxiliary_resources: NFC_AUXILIARY,
        support: SupportLevel::Described,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::LORA_MODULE),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::Spi(2)),
        route: DeviceRoute::SpiChipSelect(ResourceId::Gpio(36)),
        auxiliary_resources: LORA_AUXILIARY,
        support: SupportLevel::Described,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::AUDIO_AMPLIFIER),
        owner: OwnerDomain::Service,
        bus: None,
        route: DeviceRoute::Dedicated,
        auxiliary_resources: AMPLIFIER_AUXILIARY,
        support: SupportLevel::Described,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::BACKLIGHT_DRIVER),
        owner: OwnerDomain::Service,
        bus: None,
        route: DeviceRoute::Dedicated,
        auxiliary_resources: BACKLIGHT_AUXILIARY,
        support: SupportLevel::Described,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::ROTARY_ENCODER),
        owner: OwnerDomain::Service,
        bus: None,
        route: DeviceRoute::Dedicated,
        auxiliary_resources: ROTARY_AUXILIARY,
        support: SupportLevel::Described,
    },
];

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
];

static BOOT_STRAP_PINS: &[ResourceId] = &[
    ResourceId::Gpio(0),
    ResourceId::Gpio(3),
    ResourceId::Gpio(45),
    ResourceId::Gpio(46),
];
static SHARED_SPI: &[ResourceId] = &[ResourceId::Spi(2)];
static EXPANDER_POWER_RESOURCES: &[ResourceId] = &[
    ResourceId::Device(device::GPIO_EXPANDER),
    ResourceId::Storage(0),
    ResourceId::Device(device::GNSS),
    ResourceId::Device(device::NFC),
    ResourceId::Device(device::LORA_MODULE),
    ResourceId::Device(device::HAPTIC_DRIVER),
    ResourceId::Device(device::AUDIO_AMPLIFIER),
];
static LORA_OPTION: &[ResourceId] = &[ResourceId::Device(device::LORA_MODULE)];
static LOGIC_BUSES: &[ResourceId] = &[
    ResourceId::I2c(0),
    ResourceId::Spi(2),
    ResourceId::Uart(1),
    ResourceId::Uart(2),
];

pub static ELECTRICAL_CONSTRAINTS: &[ElectricalConstraintDescriptor<'static>] = &[
    ElectricalConstraintDescriptor {
        id: "esp32s3.boot-straps",
        kind: ElectricalConstraintKind::BootStrap,
        resources: BOOT_STRAP_PINS,
        note: "I2C SDA, audio data, and keyboard-light routes share ESP32-S3 strap-capable GPIOs",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "pager.shared-spi",
        kind: ElectricalConstraintKind::SharedRoute,
        resources: SHARED_SPI,
        note: "display, SD, NFC, and LoRa share SPI2 and require independently inactive chip selects",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "pager.expander-power-reset",
        kind: ElectricalConstraintKind::ResetStateUnverified,
        resources: EXPANDER_POWER_RESOURCES,
        note: "XL9555 power, reset, speaker, haptic, and SD control bits are described but no driver or reset transaction is composed",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "pager.radio-fitted-option",
        kind: ElectricalConstraintKind::SharedRoute,
        resources: LORA_OPTION,
        note: "the current family definition permits SX1262 or SX1280 hardware; the fitted RF option is not inferred",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "pager.logic-level",
        kind: ElectricalConstraintKind::Logic3v3,
        resources: LOGIC_BUSES,
        note: "ESP32-S3 peripheral and external serial routes are admitted only as 3.3 V logic",
        support: SupportLevel::Described,
    },
];

pub static INTERRUPTS: &[InterruptDescriptor] = &[
    InterruptDescriptor {
        source: ResourceId::Gpio(1),
        owner: OwnerDomain::Service,
        trigger: InterruptTrigger::Configurable,
        maximum_latency_cycles: None,
        support: SupportLevel::Described,
    },
    InterruptDescriptor {
        source: ResourceId::Gpio(5),
        owner: OwnerDomain::Service,
        trigger: InterruptTrigger::Configurable,
        maximum_latency_cycles: None,
        support: SupportLevel::Described,
    },
    InterruptDescriptor {
        source: ResourceId::Gpio(6),
        owner: OwnerDomain::Service,
        trigger: InterruptTrigger::Configurable,
        maximum_latency_cycles: None,
        support: SupportLevel::Described,
    },
    InterruptDescriptor {
        source: ResourceId::Gpio(8),
        owner: OwnerDomain::Service,
        trigger: InterruptTrigger::Configurable,
        maximum_latency_cycles: None,
        support: SupportLevel::Described,
    },
    InterruptDescriptor {
        source: ResourceId::Gpio(13),
        owner: OwnerDomain::Service,
        trigger: InterruptTrigger::Rising,
        maximum_latency_cycles: None,
        support: SupportLevel::Described,
    },
    InterruptDescriptor {
        source: ResourceId::Gpio(14),
        owner: OwnerDomain::Service,
        trigger: InterruptTrigger::Configurable,
        maximum_latency_cycles: None,
        support: SupportLevel::Described,
    },
    InterruptDescriptor {
        source: ResourceId::Gpio(40),
        owner: OwnerDomain::Service,
        trigger: InterruptTrigger::AnyEdge,
        maximum_latency_cycles: None,
        support: SupportLevel::Described,
    },
    InterruptDescriptor {
        source: ResourceId::Gpio(41),
        owner: OwnerDomain::Service,
        trigger: InterruptTrigger::AnyEdge,
        maximum_latency_cycles: None,
        support: SupportLevel::Described,
    },
];

static FITTED_DEVICE_HIL_RESOURCES: &[ResourceId] = &[
    ResourceId::Device(device::AUDIO_CODEC),
    ResourceId::Device(device::GPIO_EXPANDER),
    ResourceId::Device(device::SMART_SENSOR),
    ResourceId::Device(device::KEYBOARD),
    ResourceId::Device(device::RTC),
    ResourceId::Device(device::FUEL_GAUGE),
    ResourceId::Device(device::HAPTIC_DRIVER),
    ResourceId::Device(device::BATTERY_CHARGER),
    ResourceId::Device(device::DISPLAY),
    ResourceId::Storage(0),
    ResourceId::Device(device::GNSS),
    ResourceId::Device(device::NFC),
    ResourceId::Device(device::LORA_MODULE),
    ResourceId::Device(device::AUDIO_AMPLIFIER),
    ResourceId::Device(device::BACKLIGHT_DRIVER),
    ResourceId::Device(device::ROTARY_ENCODER),
];
static BUS_HIL_RESOURCES: &[ResourceId] = &[
    ResourceId::I2c(0),
    ResourceId::Spi(2),
    ResourceId::Uart(1),
    ResourceId::Uart(2),
    ResourceId::I2s(0),
];
static CORE_HIL_RESOURCES: &[ResourceId] = &[
    ResourceId::Timer { group: 0, index: 0 },
    ResourceId::Timer { group: 1, index: 0 },
];

pub static HIL_REQUIREMENTS: &[HilRequirement<'static>] = &[
    HilRequirement {
        id: "identity.pcb-and-rf-option",
        kind: HilKind::BoardIdentity,
        resources: LORA_OPTION,
        required_for: Qualification::Bench,
    },
    HilRequirement {
        id: "peripherals.fitted-current",
        kind: HilKind::PeripheralSmoke,
        resources: FITTED_DEVICE_HIL_RESOURCES,
        required_for: Qualification::Bench,
    },
    HilRequirement {
        id: "buses.service-topology",
        kind: HilKind::PeripheralSmoke,
        resources: BUS_HIL_RESOURCES,
        required_for: Qualification::Bench,
    },
    HilRequirement {
        id: "safe.expander-power-reset-watchdog",
        kind: HilKind::SafeState,
        resources: EXPANDER_POWER_RESOURCES,
        required_for: Qualification::Bench,
    },
    HilRequirement {
        id: "timing.core0-saturation",
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

pub static PACKAGE: BoardPackage<'static> = BoardPackage {
    board: BoardDescriptor {
        id: BOARD_ID,
        revision: "LilyGoLib 38e6f8d hardware definition; PCB and RF option unverified",
        chip: Chip::Esp32S3,
        application_cores: 2,
        qualification: Qualification::Compiles,
        capability_digest: CAPABILITY_DIGEST,
        resources: RESOURCES,
    },
    memory: MemoryDescriptor {
        flash_bytes: 16 * 1_024 * 1_024,
        internal_sram_bytes: 512 * 1_024,
        psram_bytes: 8 * 1_024 * 1_024,
        realtime_psram_allowed: false,
    },
    cores: CoreAssignment {
        service_core: 0,
        realtime_core: 1,
    },
    graph: GRAPH_EXECUTOR,
    diagnostic_overview: DIAGNOSTIC_OVERVIEW,
    digital_capture: DIGITAL_CAPTURE,
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
    fn capability_identity_matches_canonical_document() {
        let calculated = calculate_identity(&PACKAGE).unwrap();
        assert_eq!(CAPABILITY_DIGEST.0, calculated.digest.0);
    }

    #[test]
    fn stub_validates_without_operational_claims() {
        assert_eq!(PACKAGE.validate(), Ok(()));
        assert!(!PACKAGE.armable);
        assert!(PACKAGE.visuals.is_empty());
        assert_eq!(DEVICES.len(), 16);
        assert!(
            DEVICES
                .iter()
                .all(|device| device.support == SupportLevel::Described)
        );
        assert!(GRAPH_EXECUTOR.resources.is_empty());
        assert!(!DIAGNOSTIC_OVERVIEW.is_implemented());
        assert!(!DIGITAL_CAPTURE.is_implemented());
    }

    #[test]
    fn current_memory_and_core_facts_are_explicit() {
        assert_eq!(PACKAGE.memory.flash_bytes, 16 * 1_024 * 1_024);
        assert_eq!(PACKAGE.memory.psram_bytes, 8 * 1_024 * 1_024);
        assert!(!PACKAGE.memory.realtime_psram_allowed);
        assert_eq!(PACKAGE.cores.service_core, 0);
        assert_eq!(PACKAGE.cores.realtime_core, 1);
    }

    #[test]
    fn shared_spi_devices_have_independent_selects() {
        let mut routes = DEVICES
            .iter()
            .filter(|device| device.bus == Some(ResourceId::Spi(2)))
            .map(|device| device.route);
        let first = routes.next().unwrap();
        let rest: [DeviceRoute; 3] = core::array::from_fn(|_| routes.next().unwrap());
        assert!(!rest.contains(&first));
        assert_ne!(rest[0], rest[1]);
        assert_ne!(rest[0], rest[2]);
        assert_ne!(rest[1], rest[2]);
        assert!(routes.next().is_none());
    }

    #[test]
    fn no_documented_gpio_is_a_machine_output() {
        assert!(RESOURCES.iter().all(|resource| !resource.hazardous_output));
        assert!(
            RESOURCES
                .iter()
                .filter(|resource| matches!(resource.id, ResourceId::Gpio(_)))
                .all(|resource| {
                    resource.owner == OwnerDomain::Service
                        && resource.safe_value == SafeValue::HighImpedance
                })
        );
    }
}
