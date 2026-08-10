#![no_std]
#![doc = "Compile-time facts for the dual-core LILYGO T-Deck Pro package."]

use alumina_board::{
    AliasDescriptor, BoardDescriptor, BoardPackage, BusDescriptor, BusKind, Chip, CoreAssignment,
    DeviceDescriptor, DeviceRoute, MemoryDescriptor, OwnerDomain, Qualification,
    ResourceDescriptor, ResourceId, SafeValue, SupportLevel,
};
use alumina_protocol::Digest;

pub const BOARD_ID: &str = "t-deck-pro";
pub const TARGET: &str = "xtensa-esp32s3-none-elf";

pub mod device {
    pub const BATTERY_CHARGER: u16 = 0;
    pub const KEYBOARD: u16 = 1;
    pub const TOUCH: u16 = 2;
    pub const EPD: u16 = 3;
    pub const GPS: u16 = 4;
    pub const LORA: u16 = 5;
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

pub static RESOURCES: &[ResourceDescriptor] = &[
    resource(
        ResourceId::I2c(0),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Gpio(14),
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
        ResourceId::Spi(2),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Gpio(36),
        OwnerDomain::Service,
        SafeValue::Low,
        false,
    ),
    resource(
        ResourceId::Gpio(33),
        OwnerDomain::Service,
        SafeValue::Low,
        false,
    ),
    resource(
        ResourceId::Gpio(47),
        OwnerDomain::Service,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Uart(1),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Gpio(44),
        OwnerDomain::Service,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(43),
        OwnerDomain::Service,
        SafeValue::High,
        false,
    ),
    resource(
        ResourceId::Gpio(34),
        OwnerDomain::Service,
        SafeValue::High,
        false,
    ),
    resource(
        ResourceId::Gpio(35),
        OwnerDomain::Service,
        SafeValue::Low,
        false,
    ),
    resource(
        ResourceId::Gpio(37),
        OwnerDomain::Service,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(45),
        OwnerDomain::Service,
        SafeValue::High,
        false,
    ),
    resource(
        ResourceId::Gpio(3),
        OwnerDomain::Service,
        SafeValue::High,
        false,
    ),
    resource(
        ResourceId::Gpio(6),
        OwnerDomain::Service,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(4),
        OwnerDomain::Service,
        SafeValue::High,
        false,
    ),
    resource(
        ResourceId::Gpio(5),
        OwnerDomain::Service,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(46),
        OwnerDomain::Service,
        SafeValue::Low,
        false,
    ),
    resource(
        ResourceId::Gpio(48),
        OwnerDomain::Service,
        SafeValue::High,
        false,
    ),
    resource(
        ResourceId::Gpio(12),
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
        ResourceId::Gpio(39),
        OwnerDomain::Service,
        SafeValue::Low,
        false,
    ),
    resource(
        ResourceId::Gpio(1),
        OwnerDomain::Service,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Dma(0),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Storage(0),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Radio(0),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Device(device::BATTERY_CHARGER),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Device(device::KEYBOARD),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Device(device::TOUCH),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Device(device::EPD),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Device(device::GPS),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Device(device::LORA),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
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
        name: "serial.gps",
        resource: ResourceId::Uart(1),
    },
    AliasDescriptor {
        name: "display.epd",
        resource: ResourceId::Device(device::EPD),
    },
    AliasDescriptor {
        name: "input.touch",
        resource: ResourceId::Device(device::TOUCH),
    },
    AliasDescriptor {
        name: "input.keyboard",
        resource: ResourceId::Device(device::KEYBOARD),
    },
    AliasDescriptor {
        name: "power.charger",
        resource: ResourceId::Device(device::BATTERY_CHARGER),
    },
    AliasDescriptor {
        name: "radio.lora",
        resource: ResourceId::Device(device::LORA),
    },
    AliasDescriptor {
        name: "position.gps",
        resource: ResourceId::Device(device::GPS),
    },
    AliasDescriptor {
        name: "storage.sd",
        resource: ResourceId::Storage(0),
    },
];

static I2C_PINS: &[ResourceId] = &[ResourceId::Gpio(14), ResourceId::Gpio(13)];
static SPI_PINS: &[ResourceId] = &[
    ResourceId::Gpio(36),
    ResourceId::Gpio(33),
    ResourceId::Gpio(47),
];
static GPS_UART_PINS: &[ResourceId] = &[ResourceId::Gpio(44), ResourceId::Gpio(43)];

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
        pins: GPS_UART_PINS,
        maximum_frequency_hz: 921_600,
    },
];

pub static DEVICES: &[DeviceDescriptor] = &[
    DeviceDescriptor {
        resource: ResourceId::Device(device::BATTERY_CHARGER),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::I2c(0)),
        route: DeviceRoute::I2cAddress(0x6b),
        support: SupportLevel::Compiles,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::KEYBOARD),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::I2c(0)),
        route: DeviceRoute::I2cAddress(0x34),
        support: SupportLevel::Compiles,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::TOUCH),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::I2c(0)),
        route: DeviceRoute::I2cAddress(0x1a),
        support: SupportLevel::Compiles,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::EPD),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::Spi(2)),
        route: DeviceRoute::SpiChipSelect(ResourceId::Gpio(34)),
        support: SupportLevel::Compiles,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::GPS),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::Uart(1)),
        route: DeviceRoute::Uart,
        support: SupportLevel::Compiles,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::LORA),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::Spi(2)),
        route: DeviceRoute::SpiChipSelect(ResourceId::Gpio(3)),
        support: SupportLevel::Compiles,
    },
    DeviceDescriptor {
        resource: ResourceId::Storage(0),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::Spi(2)),
        route: DeviceRoute::SpiChipSelect(ResourceId::Gpio(48)),
        support: SupportLevel::Described,
    },
];

pub static PACKAGE: BoardPackage<'static> = BoardPackage {
    board: BoardDescriptor {
        id: BOARD_ID,
        revision: "V1.x; fixture revision pending inspection",
        chip: Chip::Esp32S3,
        application_cores: 2,
        qualification: Qualification::Compiles,
        capability_digest: Digest::ZERO,
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
    aliases: ALIASES,
    buses: BUSES,
    devices: DEVICES,
    safe_output_images: &[],
    armable: false,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_device_topology_validates_without_arm_claim() {
        assert_eq!(PACKAGE.validate(), Ok(()));
        assert!(!PACKAGE.armable);
        assert_eq!(DEVICES.len(), 7);
        assert!(
            DEVICES
                .iter()
                .all(|device| device.owner == OwnerDomain::Service)
        );
    }

    #[test]
    fn shared_spi_devices_keep_independent_chip_selects() {
        let epd = DEVICES
            .iter()
            .find(|entry| entry.resource == ResourceId::Device(device::EPD))
            .unwrap();
        let lora = DEVICES
            .iter()
            .find(|entry| entry.resource == ResourceId::Device(device::LORA))
            .unwrap();
        assert_eq!(epd.bus, lora.bus);
        assert_ne!(epd.route, lora.route);
    }
}
