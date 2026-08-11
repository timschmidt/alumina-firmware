#![no_std]
#![doc = "Compile-time facts for the dual-core MKS TinyBee V1.x package."]

use alumina_board::{
    AliasDescriptor, BoardDescriptor, BoardPackage, BusDescriptor, BusKind, Chip, ClockDescriptor,
    ClockDomain, ClockSource, CoreAssignment, DeviceDescriptor, DeviceRoute,
    ElectricalConstraintDescriptor, ElectricalConstraintKind, HilKind, HilRequirement,
    InterruptDescriptor, InterruptTrigger, MemoryDescriptor, OwnerDomain, Qualification,
    ResourceDescriptor, ResourceId, SafeOutputImage, SafeValue, SupportLevel,
};
use alumina_protocol::Digest;

/// Stable board selection ID.
pub const BOARD_ID: &str = "mks-tinybee-v1";
/// ESP Rust target required by this package.
pub const TARGET: &str = "xtensa-esp32-none-elf";
/// SHA-256 of the canonical `ALMCAP01` V1 document exported by this package.
pub const CAPABILITY_DIGEST: Digest = Digest([
    0x00, 0x0f, 0x15, 0x1d, 0x9a, 0x40, 0x4a, 0x94, 0xd8, 0x2b, 0x31, 0x1a, 0xb4, 0x03, 0x3d, 0xb2,
    0x3f, 0xe6, 0x5e, 0x56, 0xc4, 0x8d, 0x8a, 0x1d, 0x43, 0xbd, 0x77, 0x36, 0x62, 0xc7, 0xc3, 0x51,
]);
/// Shift-register safe image inferred from active-high StepStick disable inputs.
///
/// It is deliberately not bench-verified, so this package remains non-armable.
pub const DESCRIBED_SAFE_I2S_IMAGE: u32 = (1 << X_DISABLE_BIT)
    | (1 << Y_DISABLE_BIT)
    | (1 << Z_DISABLE_BIT)
    | (1 << E0_DISABLE_BIT)
    | (1 << E1_DISABLE_BIT);

pub const X_DISABLE_BIT: u8 = 0;
pub const X_STEP_BIT: u8 = 1;
pub const X_DIRECTION_BIT: u8 = 2;
pub const Y_DISABLE_BIT: u8 = 3;
pub const Y_STEP_BIT: u8 = 4;
pub const Y_DIRECTION_BIT: u8 = 5;
pub const Z_DISABLE_BIT: u8 = 6;
pub const Z_STEP_BIT: u8 = 7;
pub const Z_DIRECTION_BIT: u8 = 8;
pub const E0_DISABLE_BIT: u8 = 9;
pub const E0_STEP_BIT: u8 = 10;
pub const E0_DIRECTION_BIT: u8 = 11;
pub const E1_DISABLE_BIT: u8 = 12;
pub const E1_STEP_BIT: u8 = 13;
pub const E1_DIRECTION_BIT: u8 = 14;
/// Non-hazardous LCD serial output occupying the formerly omitted chain bit.
pub const LCD_MOSI_BIT: u8 = 15;
/// Exact number of cascaded outputs shown by the vendor schematic.
pub const SHIFT_CHAIN_WIDTH: u8 = 24;
/// Every physical U1/U2/U3 output, including LCD and expansion outputs.
pub const COMPLETE_SHIFT_MASK: u32 = 0x00ff_ffff;

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

/// Complete typed inventory currently established from vendor and FluidNC facts.
pub static RESOURCES: &[ResourceDescriptor] = &[
    resource(
        ResourceId::I2s(0),
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        false,
    ),
    resource(
        ResourceId::Dma(0),
        OwnerDomain::Realtime,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Gpio(25),
        OwnerDomain::Realtime,
        SafeValue::Low,
        false,
    ),
    resource(
        ResourceId::Gpio(27),
        OwnerDomain::Realtime,
        SafeValue::Low,
        false,
    ),
    resource(
        ResourceId::Gpio(26),
        OwnerDomain::Realtime,
        SafeValue::Low,
        false,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 0 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        true,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 1 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        true,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 2 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        true,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 3 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        true,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 4 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        true,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 5 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        true,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 6 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        true,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 7 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        true,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 8 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        true,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 9 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        true,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 10 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        true,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 11 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        true,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 12 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        true,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 13 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        true,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 14 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        true,
    ),
    resource(
        ResourceId::I2sOut {
            engine: 0,
            bit: LCD_MOSI_BIT,
        },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        false,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 16 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        true,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 17 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        true,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 18 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        true,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 19 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        true,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 20 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        true,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 21 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        false,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 22 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        false,
    ),
    resource(
        ResourceId::I2sOut { engine: 0, bit: 23 },
        OwnerDomain::Realtime,
        SafeValue::EngineImage,
        false,
    ),
    resource(
        ResourceId::Spi(2),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Gpio(19),
        OwnerDomain::Service,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(23),
        OwnerDomain::Service,
        SafeValue::Low,
        false,
    ),
    resource(
        ResourceId::Gpio(18),
        OwnerDomain::Service,
        SafeValue::Low,
        false,
    ),
    resource(
        ResourceId::Gpio(5),
        OwnerDomain::Service,
        SafeValue::High,
        false,
    ),
    resource(
        ResourceId::Storage(0),
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
        ResourceId::Uart(2),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Gpio(17),
        OwnerDomain::Service,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(16),
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
        ResourceId::Gpio(4),
        OwnerDomain::Service,
        SafeValue::Low,
        false,
    ),
    resource(
        ResourceId::Gpio(12),
        OwnerDomain::Service,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(13),
        OwnerDomain::Service,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(14),
        OwnerDomain::Service,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(15),
        OwnerDomain::Service,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(21),
        OwnerDomain::Service,
        SafeValue::Low,
        false,
    ),
    resource(
        ResourceId::Gpio(33),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(32),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(22),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(2),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        true,
    ),
    resource(
        ResourceId::Gpio(34),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(35),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(36),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
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
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Adc {
            unit: 1,
            channel: 6,
        },
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Adc {
            unit: 1,
            channel: 3,
        },
        OwnerDomain::Realtime,
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
        ResourceId::Radio(0),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
];

/// Stable configuration aliases; virtual outputs never become numeric GPIOs.
pub static ALIASES: &[AliasDescriptor<'static>] = &[
    AliasDescriptor {
        name: "io128",
        resource: ResourceId::I2sOut { engine: 0, bit: 0 },
    },
    AliasDescriptor {
        name: "io129",
        resource: ResourceId::I2sOut { engine: 0, bit: 1 },
    },
    AliasDescriptor {
        name: "io130",
        resource: ResourceId::I2sOut { engine: 0, bit: 2 },
    },
    AliasDescriptor {
        name: "io131",
        resource: ResourceId::I2sOut { engine: 0, bit: 3 },
    },
    AliasDescriptor {
        name: "io132",
        resource: ResourceId::I2sOut { engine: 0, bit: 4 },
    },
    AliasDescriptor {
        name: "io133",
        resource: ResourceId::I2sOut { engine: 0, bit: 5 },
    },
    AliasDescriptor {
        name: "io134",
        resource: ResourceId::I2sOut { engine: 0, bit: 6 },
    },
    AliasDescriptor {
        name: "io135",
        resource: ResourceId::I2sOut { engine: 0, bit: 7 },
    },
    AliasDescriptor {
        name: "io136",
        resource: ResourceId::I2sOut { engine: 0, bit: 8 },
    },
    AliasDescriptor {
        name: "io137",
        resource: ResourceId::I2sOut { engine: 0, bit: 9 },
    },
    AliasDescriptor {
        name: "io138",
        resource: ResourceId::I2sOut { engine: 0, bit: 10 },
    },
    AliasDescriptor {
        name: "io139",
        resource: ResourceId::I2sOut { engine: 0, bit: 11 },
    },
    AliasDescriptor {
        name: "io140",
        resource: ResourceId::I2sOut { engine: 0, bit: 12 },
    },
    AliasDescriptor {
        name: "io141",
        resource: ResourceId::I2sOut { engine: 0, bit: 13 },
    },
    AliasDescriptor {
        name: "io142",
        resource: ResourceId::I2sOut { engine: 0, bit: 14 },
    },
    AliasDescriptor {
        name: "io143",
        resource: ResourceId::I2sOut {
            engine: 0,
            bit: LCD_MOSI_BIT,
        },
    },
    AliasDescriptor {
        name: "io144",
        resource: ResourceId::I2sOut { engine: 0, bit: 16 },
    },
    AliasDescriptor {
        name: "io145",
        resource: ResourceId::I2sOut { engine: 0, bit: 17 },
    },
    AliasDescriptor {
        name: "io146",
        resource: ResourceId::I2sOut { engine: 0, bit: 18 },
    },
    AliasDescriptor {
        name: "io147",
        resource: ResourceId::I2sOut { engine: 0, bit: 19 },
    },
    AliasDescriptor {
        name: "io148",
        resource: ResourceId::I2sOut { engine: 0, bit: 20 },
    },
    AliasDescriptor {
        name: "io149",
        resource: ResourceId::I2sOut { engine: 0, bit: 21 },
    },
    AliasDescriptor {
        name: "axis.x.step",
        resource: ResourceId::I2sOut {
            engine: 0,
            bit: X_STEP_BIT,
        },
    },
    AliasDescriptor {
        name: "axis.x.direction",
        resource: ResourceId::I2sOut {
            engine: 0,
            bit: X_DIRECTION_BIT,
        },
    },
    AliasDescriptor {
        name: "axis.x.enable",
        resource: ResourceId::I2sOut {
            engine: 0,
            bit: X_DISABLE_BIT,
        },
    },
    AliasDescriptor {
        name: "axis.y.step",
        resource: ResourceId::I2sOut {
            engine: 0,
            bit: Y_STEP_BIT,
        },
    },
    AliasDescriptor {
        name: "axis.y.direction",
        resource: ResourceId::I2sOut {
            engine: 0,
            bit: Y_DIRECTION_BIT,
        },
    },
    AliasDescriptor {
        name: "axis.y.enable",
        resource: ResourceId::I2sOut {
            engine: 0,
            bit: Y_DISABLE_BIT,
        },
    },
    AliasDescriptor {
        name: "axis.z.step",
        resource: ResourceId::I2sOut {
            engine: 0,
            bit: Z_STEP_BIT,
        },
    },
    AliasDescriptor {
        name: "axis.z.direction",
        resource: ResourceId::I2sOut {
            engine: 0,
            bit: Z_DIRECTION_BIT,
        },
    },
    AliasDescriptor {
        name: "axis.z.enable",
        resource: ResourceId::I2sOut {
            engine: 0,
            bit: Z_DISABLE_BIT,
        },
    },
    AliasDescriptor {
        name: "limit.x.negative",
        resource: ResourceId::Gpio(33),
    },
    AliasDescriptor {
        name: "limit.y.negative",
        resource: ResourceId::Gpio(32),
    },
    AliasDescriptor {
        name: "limit.z.negative",
        resource: ResourceId::Gpio(22),
    },
    AliasDescriptor {
        name: "storage.sd",
        resource: ResourceId::Storage(0),
    },
    AliasDescriptor {
        name: "heater.bed",
        resource: ResourceId::I2sOut { engine: 0, bit: 16 },
    },
    AliasDescriptor {
        name: "heater.e0",
        resource: ResourceId::I2sOut { engine: 0, bit: 17 },
    },
    AliasDescriptor {
        name: "heater.e1",
        resource: ResourceId::I2sOut { engine: 0, bit: 18 },
    },
    AliasDescriptor {
        name: "fan.1",
        resource: ResourceId::I2sOut { engine: 0, bit: 19 },
    },
    AliasDescriptor {
        name: "fan.2",
        resource: ResourceId::I2sOut { engine: 0, bit: 20 },
    },
    AliasDescriptor {
        name: "ui.beeper",
        resource: ResourceId::I2sOut { engine: 0, bit: 21 },
    },
    AliasDescriptor {
        name: "probe.servo",
        resource: ResourceId::Gpio(2),
    },
    AliasDescriptor {
        name: "material.detect",
        resource: ResourceId::Gpio(35),
    },
    AliasDescriptor {
        name: "temperature.tool0",
        resource: ResourceId::Adc {
            unit: 1,
            channel: 0,
        },
    },
    AliasDescriptor {
        name: "temperature.tool1",
        resource: ResourceId::Adc {
            unit: 1,
            channel: 6,
        },
    },
    AliasDescriptor {
        name: "temperature.bed",
        resource: ResourceId::Adc {
            unit: 1,
            channel: 3,
        },
    },
    AliasDescriptor {
        name: "serial.usb",
        resource: ResourceId::Uart(0),
    },
    AliasDescriptor {
        name: "serial.aux",
        resource: ResourceId::Uart(2),
    },
    AliasDescriptor {
        name: "ui.encoder.press",
        resource: ResourceId::Gpio(13),
    },
    AliasDescriptor {
        name: "ui.encoder.a",
        resource: ResourceId::Gpio(14),
    },
    AliasDescriptor {
        name: "ui.encoder.b",
        resource: ResourceId::Gpio(12),
    },
];

static SPI_PINS: &[ResourceId] = &[
    ResourceId::Gpio(19),
    ResourceId::Gpio(23),
    ResourceId::Gpio(18),
];

static UART0_PINS: &[ResourceId] = &[ResourceId::Gpio(1), ResourceId::Gpio(3)];
static UART2_PINS: &[ResourceId] = &[ResourceId::Gpio(17), ResourceId::Gpio(16)];

pub static BUSES: &[BusDescriptor<'static>] = &[
    BusDescriptor {
        resource: ResourceId::Spi(2),
        kind: BusKind::Spi,
        owner: OwnerDomain::Service,
        pins: SPI_PINS,
        maximum_frequency_hz: 20_000_000,
    },
    BusDescriptor {
        resource: ResourceId::Uart(0),
        kind: BusKind::Uart,
        owner: OwnerDomain::Service,
        pins: UART0_PINS,
        maximum_frequency_hz: 921_600,
    },
    BusDescriptor {
        resource: ResourceId::Uart(2),
        kind: BusKind::Uart,
        owner: OwnerDomain::Service,
        pins: UART2_PINS,
        maximum_frequency_hz: 921_600,
    },
];

static SD_AUXILIARY: &[ResourceId] = &[ResourceId::Gpio(34)];

pub static DEVICES: &[DeviceDescriptor<'static>] = &[DeviceDescriptor {
    resource: ResourceId::Storage(0),
    owner: OwnerDomain::Service,
    bus: Some(ResourceId::Spi(2)),
    route: DeviceRoute::SpiChipSelect(ResourceId::Gpio(5)),
    auxiliary_resources: SD_AUXILIARY,
    support: SupportLevel::Compiles,
}];

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

static INPUT_ONLY_PINS: &[ResourceId] = &[
    ResourceId::Gpio(34),
    ResourceId::Gpio(35),
    ResourceId::Gpio(36),
    ResourceId::Gpio(39),
];
static BOOT_STRAP_PINS: &[ResourceId] = &[
    ResourceId::Gpio(0),
    ResourceId::Gpio(2),
    ResourceId::Gpio(5),
    ResourceId::Gpio(12),
    ResourceId::Gpio(15),
];
static I2S_ENGINE: &[ResourceId] = &[ResourceId::I2s(0)];
static MULTIPLEXED_ADC_SD: &[ResourceId] = &[ResourceId::Gpio(34)];
static MULTIPLEXED_LCD_UART: &[ResourceId] = &[ResourceId::Gpio(16), ResourceId::Gpio(17)];
static STEPPER_DISABLE_LINES: &[ResourceId] = &[
    ResourceId::I2sOut { engine: 0, bit: 0 },
    ResourceId::I2sOut { engine: 0, bit: 3 },
    ResourceId::I2sOut { engine: 0, bit: 6 },
    ResourceId::I2sOut { engine: 0, bit: 9 },
    ResourceId::I2sOut { engine: 0, bit: 12 },
];

pub static ELECTRICAL_CONSTRAINTS: &[ElectricalConstraintDescriptor<'static>] = &[
    ElectricalConstraintDescriptor {
        id: "esp32.input-only",
        kind: ElectricalConstraintKind::InputOnly,
        resources: INPUT_ONLY_PINS,
        note: "ESP32 input-only routes; GPIO34 also has a board-level jumper/multiplex role",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "esp32.boot-straps",
        kind: ElectricalConstraintKind::BootStrap,
        resources: BOOT_STRAP_PINS,
        note: "external loads and pulls must preserve the ESP32 reset/boot strap levels",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "tinybee.shifted-output-only",
        kind: ElectricalConstraintKind::OutputOnly,
        resources: I2S_ENGINE,
        note: "IO128 and above are shift-register outputs, never native GPIO inputs",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "tinybee.shifted-no-independent-pwm",
        kind: ElectricalConstraintKind::NotPwm,
        resources: I2S_ENGINE,
        note: "modulation requires a qualified whole-engine update schedule, not a native PWM channel",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "tinybee.gpio34-jumper-multiplex",
        kind: ElectricalConstraintKind::SharedRoute,
        resources: MULTIPLEXED_ADC_SD,
        note: "GPIO34 is documented for TH2 with jumper selection and as the default SD-detect route",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "tinybee.gpio16-17-multiplex",
        kind: ElectricalConstraintKind::SharedRoute,
        resources: MULTIPLEXED_LCD_UART,
        note: "GPIO16/17 are shared between the parallel LCD connector and USART2",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "tinybee.stepper-disable-active-high",
        kind: ElectricalConstraintKind::ActiveHigh,
        resources: STEPPER_DISABLE_LINES,
        note: "polarity inferred from the official FluidNC plain disable-pin mapping; HIL required",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "tinybee.logic-level",
        kind: ElectricalConstraintKind::Logic3v3,
        resources: I2S_ENGINE,
        note: "MCU/shift-control logic is 3.3 V; connector conditioning must be checked per schematic",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "tinybee.reset-state-unverified",
        kind: ElectricalConstraintKind::ResetStateUnverified,
        resources: I2S_ENGINE,
        note: "shift-register output state before the realtime driver takes control is not bench verified",
        support: SupportLevel::Described,
    },
];

pub static INTERRUPTS: &[InterruptDescriptor] = &[
    InterruptDescriptor {
        source: ResourceId::Gpio(33),
        owner: OwnerDomain::Realtime,
        trigger: InterruptTrigger::Configurable,
        maximum_latency_cycles: None,
        support: SupportLevel::Described,
    },
    InterruptDescriptor {
        source: ResourceId::Gpio(32),
        owner: OwnerDomain::Realtime,
        trigger: InterruptTrigger::Configurable,
        maximum_latency_cycles: None,
        support: SupportLevel::Described,
    },
    InterruptDescriptor {
        source: ResourceId::Gpio(22),
        owner: OwnerDomain::Realtime,
        trigger: InterruptTrigger::Configurable,
        maximum_latency_cycles: None,
        support: SupportLevel::Described,
    },
    InterruptDescriptor {
        source: ResourceId::Gpio(35),
        owner: OwnerDomain::Realtime,
        trigger: InterruptTrigger::Configurable,
        maximum_latency_cycles: None,
        support: SupportLevel::Described,
    },
];

static SAFE_STATE_HIL_RESOURCES: &[ResourceId] = &[ResourceId::I2s(0), ResourceId::Gpio(2)];
static SHIFT_OUTPUT_HIL_RESOURCES: &[ResourceId] = &[ResourceId::I2s(0)];
static LIMIT_HIL_RESOURCES: &[ResourceId] = &[
    ResourceId::Gpio(33),
    ResourceId::Gpio(32),
    ResourceId::Gpio(22),
];
static CORE_HIL_RESOURCES: &[ResourceId] = &[
    ResourceId::Timer { group: 0, index: 0 },
    ResourceId::Timer { group: 1, index: 0 },
];
static SD_HIL_RESOURCES: &[ResourceId] = &[ResourceId::Storage(0)];

pub static HIL_REQUIREMENTS: &[HilRequirement<'static>] = &[
    HilRequirement {
        id: "identity.revision",
        kind: HilKind::BoardIdentity,
        resources: &[],
        required_for: Qualification::Bench,
    },
    HilRequirement {
        id: "safe.i2s-reset-fault-watchdog",
        kind: HilKind::SafeState,
        resources: SAFE_STATE_HIL_RESOURCES,
        required_for: Qualification::Bench,
    },
    HilRequirement {
        id: "routing.i2s-all-bits",
        kind: HilKind::PeripheralSmoke,
        resources: SHIFT_OUTPUT_HIL_RESOURCES,
        required_for: Qualification::Bench,
    },
    HilRequirement {
        id: "input.xyz-limits",
        kind: HilKind::FaultInjection,
        resources: LIMIT_HIL_RESOURCES,
        required_for: Qualification::Bench,
    },
    HilRequirement {
        id: "storage.sd-shared-spi",
        kind: HilKind::PeripheralSmoke,
        resources: SD_HIL_RESOURCES,
        required_for: Qualification::Bench,
    },
    HilRequirement {
        id: "timing.core0-saturation",
        kind: HilKind::CoreIsolation,
        resources: CORE_HIL_RESOURCES,
        required_for: Qualification::MotionQualified,
    },
    HilRequirement {
        id: "timing.realtime-envelope",
        kind: HilKind::Timing,
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

pub static SAFE_IMAGES: &[SafeOutputImage] = &[SafeOutputImage {
    engine: 0,
    defined_mask: COMPLETE_SHIFT_MASK,
    safe_bits: DESCRIBED_SAFE_I2S_IMAGE,
    bench_verified: false,
}];

/// Canonical portable package. Hardware composition remains non-armable until HIL.
pub static PACKAGE: BoardPackage<'static> = BoardPackage {
    board: BoardDescriptor {
        id: BOARD_ID,
        revision: "1.x; exact fixture revision pending inspection",
        chip: Chip::Esp32,
        application_cores: 2,
        qualification: Qualification::Compiles,
        capability_digest: CAPABILITY_DIGEST,
        resources: RESOURCES,
    },
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
    aliases: ALIASES,
    buses: BUSES,
    devices: DEVICES,
    flash_regions: &[],
    clocks: CLOCKS,
    electrical_constraints: ELECTRICAL_CONSTRAINTS,
    interrupts: INTERRUPTS,
    safe_output_images: SAFE_IMAGES,
    visuals: &[],
    hil_requirements: HIL_REQUIREMENTS,
    armable: false,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_validates_but_cannot_arm_without_physical_evidence() {
        assert_eq!(PACKAGE.validate(), Ok(()));
        assert!(!PACKAGE.armable);
        assert!(!SAFE_IMAGES[0].bench_verified);
    }

    #[test]
    fn fluidnc_virtual_alias_remains_in_i2s_namespace() {
        let io129 = ALIASES.iter().find(|alias| alias.name == "io129").unwrap();
        assert_eq!(
            io129.resource,
            ResourceId::I2sOut {
                engine: 0,
                bit: X_STEP_BIT,
            }
        );
        assert_ne!(io129.resource, ResourceId::Gpio(129));
    }

    #[test]
    fn safe_image_defines_all_three_cascaded_registers() {
        assert_eq!(SHIFT_CHAIN_WIDTH, 24);
        assert_eq!(SAFE_IMAGES[0].defined_mask, (1 << SHIFT_CHAIN_WIDTH) - 1);
        assert_eq!(
            ALIASES
                .iter()
                .find(|alias| alias.name == "io143")
                .unwrap()
                .resource,
            ResourceId::I2sOut {
                engine: 0,
                bit: LCD_MOSI_BIT,
            }
        );
    }
}
