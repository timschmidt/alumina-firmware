#![no_std]
#![doc = "Compile-time facts for the dual-core MKS TinyBee V1.x package."]

use alumina_board::{
    AliasDescriptor, BoardDescriptor, BoardPackage, BusDescriptor, BusKind, Chip, CoreAssignment,
    DeviceDescriptor, DeviceRoute, MemoryDescriptor, OwnerDomain, Qualification,
    ResourceDescriptor, ResourceId, SafeOutputImage, SafeValue, SupportLevel,
};
use alumina_protocol::Digest;

/// Stable board selection ID.
pub const BOARD_ID: &str = "mks-tinybee-v1";
/// ESP Rust target required by this package.
pub const TARGET: &str = "xtensa-esp32-none-elf";
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
];

static SPI_PINS: &[ResourceId] = &[
    ResourceId::Gpio(19),
    ResourceId::Gpio(23),
    ResourceId::Gpio(18),
];

pub static BUSES: &[BusDescriptor<'static>] = &[BusDescriptor {
    resource: ResourceId::Spi(2),
    kind: BusKind::Spi,
    owner: OwnerDomain::Service,
    pins: SPI_PINS,
    maximum_frequency_hz: 20_000_000,
}];

pub static DEVICES: &[DeviceDescriptor] = &[DeviceDescriptor {
    resource: ResourceId::Storage(0),
    owner: OwnerDomain::Service,
    bus: Some(ResourceId::Spi(2)),
    route: DeviceRoute::SpiChipSelect(ResourceId::Gpio(5)),
    support: SupportLevel::Described,
}];

pub static SAFE_IMAGES: &[SafeOutputImage] = &[SafeOutputImage {
    engine: 0,
    defined_mask: 0x00ff_7fff,
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
        capability_digest: Digest::ZERO,
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
    safe_output_images: SAFE_IMAGES,
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
}
