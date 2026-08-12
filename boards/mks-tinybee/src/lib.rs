#![no_std]
#![doc = "Compile-time facts for dual-core MKS TinyBee V1.x flash variants."]

use alumina_board::{
    AliasDescriptor, BoardDescriptor, BoardPackage, BusDescriptor, BusKind, Chip, ClockDescriptor,
    ClockDomain, ClockSource, CoreAssignment, DeviceDescriptor, DeviceRoute,
    ElectricalConstraintDescriptor, ElectricalConstraintKind, GraphExecutorDescriptor,
    GraphOpcodeDescriptor, GraphResourceAccess, GraphResourceClass, GraphResourceDescriptor,
    HilKind, HilRequirement, InterruptDescriptor, InterruptTrigger, MemoryDescriptor, OwnerDomain,
    Qualification, ResourceDescriptor, ResourceId, SafeOutputImage, SafeValue, SupportLevel,
};
use alumina_protocol::Digest;

/// Stable board selection ID for the primary 8 MiB package.
pub const BOARD_ID: &str = "mks-tinybee-v1";
/// Stable board selection ID for the opportunistic 4 MiB package.
pub const BOARD_ID_4_MIB: &str = "mks-tinybee-v1-4mb";
/// ESP Rust target required by this package.
pub const TARGET: &str = "xtensa-esp32-none-elf";
/// Installed flash declared by the primary package.
pub const PRIMARY_FLASH_BYTES: usize = 8 * 1_024 * 1_024;
/// Installed flash declared by the smaller explicit variant.
pub const FOUR_MIB_FLASH_BYTES: usize = 4 * 1_024 * 1_024;
/// Stable graph-palette class for fresh, debounced configured safety inputs.
pub const GRAPH_STABLE_BOOLEAN_INPUT_CLASS: GraphResourceClass = GraphResourceClass::new(1);
/// Fixed graph opcode palette implemented by both TinyBee image variants.
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
/// Read-only graph resources already owned by the core-1 safety-input bank.
pub static GRAPH_RESOURCES: &[GraphResourceDescriptor] = &[
    GraphResourceDescriptor {
        resource: ResourceId::Gpio(33),
        class: GRAPH_STABLE_BOOLEAN_INPUT_CLASS,
        access: GraphResourceAccess::StableBooleanInput,
        support: SupportLevel::Compiles,
    },
    GraphResourceDescriptor {
        resource: ResourceId::Gpio(32),
        class: GRAPH_STABLE_BOOLEAN_INPUT_CLASS,
        access: GraphResourceAccess::StableBooleanInput,
        support: SupportLevel::Compiles,
    },
    GraphResourceDescriptor {
        resource: ResourceId::Gpio(22),
        class: GRAPH_STABLE_BOOLEAN_INPUT_CLASS,
        access: GraphResourceAccess::StableBooleanInput,
        support: SupportLevel::Compiles,
    },
    GraphResourceDescriptor {
        resource: ResourceId::Gpio(35),
        class: GRAPH_STABLE_BOOLEAN_INPUT_CLASS,
        access: GraphResourceAccess::StableBooleanInput,
        support: SupportLevel::Compiles,
    },
];
/// Exact permanently allocated graph executor published by both variants.
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
    resources: GRAPH_RESOURCES,
};
/// SHA-256 of the primary 8 MiB canonical `ALMCAP02` V2 document.
pub const CAPABILITY_DIGEST: Digest = Digest([
    0x0e, 0x82, 0x51, 0x38, 0x96, 0xe5, 0x2e, 0x0a, 0x58, 0xfb, 0x92, 0xde, 0x91, 0x30, 0xc4, 0x46,
    0xd5, 0x90, 0xbf, 0x64, 0x9f, 0xbc, 0x22, 0x74, 0x22, 0x09, 0xb2, 0xd0, 0x4c, 0x8c, 0xb0, 0xa5,
]);
/// SHA-256 of the 4 MiB canonical `ALMCAP02` V2 document.
pub const CAPABILITY_DIGEST_4_MIB: Digest = Digest([
    0xba, 0x06, 0xff, 0xad, 0x44, 0x12, 0x5a, 0x4c, 0xf5, 0xb7, 0x2b, 0xa1, 0xa1, 0x42, 0x96, 0xa0,
    0xfb, 0x3d, 0x91, 0xf4, 0x36, 0x4a, 0xf0, 0x50, 0x61, 0x6b, 0xdf, 0x0c, 0xeb, 0xb0, 0xed, 0x04,
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

static SAFE_STATE_HIL_RESOURCES: &[ResourceId] = &[
    ResourceId::I2s(0),
    ResourceId::Gpio(2),
    ResourceId::Gpio(22),
    ResourceId::Gpio(32),
    ResourceId::Gpio(33),
    ResourceId::Gpio(35),
];
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

const fn package(
    id: &'static str,
    revision: &'static str,
    capability_digest: Digest,
    flash_bytes: usize,
) -> BoardPackage<'static> {
    BoardPackage {
        board: BoardDescriptor {
            id,
            revision,
            chip: Chip::Esp32,
            application_cores: 2,
            qualification: Qualification::Compiles,
            capability_digest,
            resources: RESOURCES,
        },
        memory: MemoryDescriptor {
            flash_bytes,
            internal_sram_bytes: 520 * 1_024,
            psram_bytes: 0,
            realtime_psram_allowed: false,
        },
        cores: CoreAssignment {
            service_core: 0,
            realtime_core: 1,
        },
        graph: GRAPH_EXECUTOR,
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
    }
}

/// Primary canonical package for 8 MiB TinyBee modules.
///
/// The connected V1.0 fixture reports this capacity. Hardware composition
/// remains non-armable until the remaining visual and electrical HIL gates.
pub static PACKAGE: BoardPackage<'static> = package(
    BOARD_ID,
    "V1.x, 8 MiB primary; connected PCB marked V1.0",
    CAPABILITY_DIGEST,
    PRIMARY_FLASH_BYTES,
);

/// Explicit opportunistic package for TinyBee assemblies fitted with 4 MiB.
///
/// It shares physical routing with the primary package but has a different
/// immutable board ID, flash capacity, and capability digest. No runtime flash
/// probing is allowed to substitute it for the compiled package.
pub static PACKAGE_4_MIB: BoardPackage<'static> = package(
    BOARD_ID_4_MIB,
    "V1.x, 4 MiB flash variant; physical fixture unavailable",
    CAPABILITY_DIGEST_4_MIB,
    FOUR_MIB_FLASH_BYTES,
);

#[cfg(test)]
mod tests {
    use alumina_capability::calculate_identity;

    use super::*;

    #[test]
    fn package_validates_but_cannot_arm_without_physical_evidence() {
        assert_eq!(PACKAGE.validate(), Ok(()));
        assert_eq!(PACKAGE_4_MIB.validate(), Ok(()));
        assert!(!PACKAGE.armable);
        assert!(!PACKAGE_4_MIB.armable);
        assert!(!SAFE_IMAGES[0].bench_verified);
    }

    #[test]
    fn flash_variants_are_exact_and_have_distinct_identities() {
        assert_eq!(PACKAGE.memory.flash_bytes, PRIMARY_FLASH_BYTES);
        assert_eq!(PACKAGE_4_MIB.memory.flash_bytes, FOUR_MIB_FLASH_BYTES);
        assert_ne!(PACKAGE.board.id, PACKAGE_4_MIB.board.id);
        let primary = calculate_identity(&PACKAGE).unwrap();
        let four_mib = calculate_identity(&PACKAGE_4_MIB).unwrap();
        assert_eq!(primary.digest.0, CAPABILITY_DIGEST.0);
        assert_eq!(four_mib.digest.0, CAPABILITY_DIGEST_4_MIB.0);
        assert_ne!(primary.digest, four_mib.digest);
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
