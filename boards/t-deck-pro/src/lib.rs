#![no_std]
#![doc = "Compile-time facts for the dual-core LILYGO T-Deck Pro package."]

use alumina_board::{
    AliasDescriptor, BoardDescriptor, BoardPackage, BusDescriptor, BusKind, Chip, ClockDescriptor,
    ClockDomain, ClockSource, CoreAssignment, DeviceDescriptor, DeviceRoute,
    ElectricalConstraintDescriptor, ElectricalConstraintKind, HilKind, HilRequirement,
    InterruptDescriptor, InterruptTrigger, MemoryDescriptor, OwnerDomain, Qualification,
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
    pub const FUEL_GAUGE: u16 = 6;
    pub const AMBIENT_LIGHT: u16 = 7;
    pub const IMU: u16 = 8;
    pub const MICROPHONE: u16 = 9;
    pub const VIBRATION_MOTOR: u16 = 10;
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
        ResourceId::Gpio(42),
        OwnerDomain::Service,
        SafeValue::Low,
        false,
    ),
    resource(
        ResourceId::Gpio(16),
        OwnerDomain::Service,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(21),
        OwnerDomain::Service,
        SafeValue::HighImpedance,
        false,
    ),
    resource(
        ResourceId::Gpio(38),
        OwnerDomain::Service,
        SafeValue::Low,
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
        ResourceId::I2s(0),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Dma(1),
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
        ResourceId::Gpio(18),
        OwnerDomain::Service,
        SafeValue::Low,
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
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        true,
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
        ResourceId::Device(device::FUEL_GAUGE),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Device(device::AMBIENT_LIGHT),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Device(device::IMU),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Device(device::MICROPHONE),
        OwnerDomain::Service,
        SafeValue::NotApplicable,
        false,
    ),
    resource(
        ResourceId::Device(device::VIBRATION_MOTOR),
        OwnerDomain::Realtime,
        SafeValue::HighImpedance,
        true,
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
    AliasDescriptor {
        name: "power.fuel-gauge",
        resource: ResourceId::Device(device::FUEL_GAUGE),
    },
    AliasDescriptor {
        name: "sensor.ambient-light",
        resource: ResourceId::Device(device::AMBIENT_LIGHT),
    },
    AliasDescriptor {
        name: "sensor.imu",
        resource: ResourceId::Device(device::IMU),
    },
    AliasDescriptor {
        name: "input.microphone",
        resource: ResourceId::Device(device::MICROPHONE),
    },
    AliasDescriptor {
        name: "output.vibration",
        resource: ResourceId::Device(device::VIBRATION_MOTOR),
    },
    AliasDescriptor {
        name: "input.boot",
        resource: ResourceId::Gpio(0),
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

static KEYBOARD_AUXILIARY: &[ResourceId] = &[ResourceId::Gpio(15), ResourceId::Gpio(42)];
static TOUCH_AUXILIARY: &[ResourceId] = &[ResourceId::Gpio(12), ResourceId::Gpio(45)];
static EPD_AUXILIARY: &[ResourceId] = &[ResourceId::Gpio(35), ResourceId::Gpio(37)];
static GPS_AUXILIARY: &[ResourceId] = &[ResourceId::Gpio(39), ResourceId::Gpio(1)];
static LORA_AUXILIARY: &[ResourceId] = &[
    ResourceId::Gpio(6),
    ResourceId::Gpio(4),
    ResourceId::Gpio(5),
    ResourceId::Gpio(46),
];
static ALS_AUXILIARY: &[ResourceId] = &[ResourceId::Gpio(16)];
static IMU_AUXILIARY: &[ResourceId] = &[ResourceId::Gpio(21), ResourceId::Gpio(38)];
static MICROPHONE_AUXILIARY: &[ResourceId] = &[
    ResourceId::I2s(0),
    ResourceId::Dma(1),
    ResourceId::Gpio(17),
    ResourceId::Gpio(18),
];
static VIBRATION_AUXILIARY: &[ResourceId] = &[ResourceId::Gpio(2)];

pub static DEVICES: &[DeviceDescriptor<'static>] = &[
    DeviceDescriptor {
        resource: ResourceId::Device(device::BATTERY_CHARGER),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::I2c(0)),
        route: DeviceRoute::I2cAddress(0x6b),
        auxiliary_resources: &[],
        support: SupportLevel::Compiles,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::KEYBOARD),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::I2c(0)),
        route: DeviceRoute::I2cAddress(0x34),
        auxiliary_resources: KEYBOARD_AUXILIARY,
        support: SupportLevel::Compiles,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::TOUCH),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::I2c(0)),
        route: DeviceRoute::I2cAddress(0x1a),
        auxiliary_resources: TOUCH_AUXILIARY,
        support: SupportLevel::Compiles,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::EPD),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::Spi(2)),
        route: DeviceRoute::SpiChipSelect(ResourceId::Gpio(34)),
        auxiliary_resources: EPD_AUXILIARY,
        support: SupportLevel::Compiles,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::GPS),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::Uart(1)),
        route: DeviceRoute::Uart,
        auxiliary_resources: GPS_AUXILIARY,
        support: SupportLevel::Compiles,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::LORA),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::Spi(2)),
        route: DeviceRoute::SpiChipSelect(ResourceId::Gpio(3)),
        auxiliary_resources: LORA_AUXILIARY,
        support: SupportLevel::Compiles,
    },
    DeviceDescriptor {
        resource: ResourceId::Storage(0),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::Spi(2)),
        route: DeviceRoute::SpiChipSelect(ResourceId::Gpio(48)),
        auxiliary_resources: &[],
        support: SupportLevel::Compiles,
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
        resource: ResourceId::Device(device::AMBIENT_LIGHT),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::I2c(0)),
        route: DeviceRoute::I2cAddress(0x23),
        auxiliary_resources: ALS_AUXILIARY,
        support: SupportLevel::Described,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::IMU),
        owner: OwnerDomain::Service,
        bus: Some(ResourceId::I2c(0)),
        route: DeviceRoute::I2cAddress(0x28),
        auxiliary_resources: IMU_AUXILIARY,
        support: SupportLevel::Described,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::MICROPHONE),
        owner: OwnerDomain::Service,
        bus: None,
        route: DeviceRoute::Dedicated,
        auxiliary_resources: MICROPHONE_AUXILIARY,
        support: SupportLevel::Described,
    },
    DeviceDescriptor {
        resource: ResourceId::Device(device::VIBRATION_MOTOR),
        owner: OwnerDomain::Realtime,
        bus: None,
        route: DeviceRoute::Dedicated,
        auxiliary_resources: VIBRATION_AUXILIARY,
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
static EPD_DEVICE: &[ResourceId] = &[ResourceId::Device(device::EPD)];
static ENABLE_PINS: &[ResourceId] = &[
    ResourceId::Gpio(38),
    ResourceId::Gpio(39),
    ResourceId::Gpio(46),
];
static VIBRATION_OUTPUTS: &[ResourceId] = &[
    ResourceId::Gpio(2),
    ResourceId::Device(device::VIBRATION_MOTOR),
];
static LOGIC_BUSES: &[ResourceId] = &[ResourceId::I2c(0), ResourceId::Spi(2)];

pub static ELECTRICAL_CONSTRAINTS: &[ElectricalConstraintDescriptor<'static>] = &[
    ElectricalConstraintDescriptor {
        id: "esp32s3.boot-straps",
        kind: ElectricalConstraintKind::BootStrap,
        resources: BOOT_STRAP_PINS,
        note: "fitted LoRa/reset routes must preserve ESP32-S3 reset and download-mode strap levels",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "tdeck.shared-spi",
        kind: ElectricalConstraintKind::SharedRoute,
        resources: SHARED_SPI,
        note: "EPD, LoRa, and SD share SPI2 and require independently inactive chip selects",
        support: SupportLevel::Compiles,
    },
    ElectricalConstraintDescriptor {
        id: "tdeck.epd-reset-unconnected",
        kind: ElectricalConstraintKind::ResetStateUnverified,
        resources: EPD_DEVICE,
        note: "the official V1.x map marks EPD reset unconnected; GPIO45 belongs to touch reset",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "tdeck.peripheral-enables-active-high",
        kind: ElectricalConstraintKind::ActiveHigh,
        resources: ENABLE_PINS,
        note: "1V8 sensor, GPS, and LoRa power-enable outputs are inactive low",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "tdeck.vibration-reset-state",
        kind: ElectricalConstraintKind::ResetStateUnverified,
        resources: VIBRATION_OUTPUTS,
        note: "GPIO2 vibration motor route is RT-owned but its transistor polarity/reset state requires HIL",
        support: SupportLevel::Described,
    },
    ElectricalConstraintDescriptor {
        id: "tdeck.logic-level",
        kind: ElectricalConstraintKind::Logic3v3,
        resources: LOGIC_BUSES,
        note: "ESP32-S3 peripheral buses use board-conditioned 3.3 V logic",
        support: SupportLevel::Described,
    },
];

pub static INTERRUPTS: &[InterruptDescriptor] = &[
    InterruptDescriptor {
        source: ResourceId::Gpio(15),
        owner: OwnerDomain::Service,
        trigger: InterruptTrigger::LowLevel,
        maximum_latency_cycles: None,
        support: SupportLevel::Compiles,
    },
    InterruptDescriptor {
        source: ResourceId::Gpio(12),
        owner: OwnerDomain::Service,
        trigger: InterruptTrigger::LowLevel,
        maximum_latency_cycles: None,
        support: SupportLevel::Compiles,
    },
    InterruptDescriptor {
        source: ResourceId::Gpio(16),
        owner: OwnerDomain::Service,
        trigger: InterruptTrigger::Configurable,
        maximum_latency_cycles: None,
        support: SupportLevel::Described,
    },
    InterruptDescriptor {
        source: ResourceId::Gpio(21),
        owner: OwnerDomain::Service,
        trigger: InterruptTrigger::Configurable,
        maximum_latency_cycles: None,
        support: SupportLevel::Described,
    },
    InterruptDescriptor {
        source: ResourceId::Gpio(5),
        owner: OwnerDomain::Service,
        trigger: InterruptTrigger::Rising,
        maximum_latency_cycles: None,
        support: SupportLevel::Compiles,
    },
    InterruptDescriptor {
        source: ResourceId::Gpio(1),
        owner: OwnerDomain::Service,
        trigger: InterruptTrigger::Rising,
        maximum_latency_cycles: None,
        support: SupportLevel::Described,
    },
];

static FITTED_DEVICE_HIL_RESOURCES: &[ResourceId] = &[
    ResourceId::Device(device::BATTERY_CHARGER),
    ResourceId::Device(device::FUEL_GAUGE),
    ResourceId::Device(device::KEYBOARD),
    ResourceId::Device(device::TOUCH),
    ResourceId::Device(device::AMBIENT_LIGHT),
    ResourceId::Device(device::IMU),
    ResourceId::Device(device::EPD),
    ResourceId::Device(device::GPS),
    ResourceId::Device(device::LORA),
    ResourceId::Device(device::MICROPHONE),
    ResourceId::Device(device::VIBRATION_MOTOR),
    ResourceId::Storage(0),
];
static SHARED_BUS_HIL_RESOURCES: &[ResourceId] = &[ResourceId::I2c(0), ResourceId::Spi(2)];
static VIBRATION_HIL_RESOURCES: &[ResourceId] = &[
    ResourceId::Gpio(2),
    ResourceId::Device(device::VIBRATION_MOTOR),
];
static CORE_HIL_RESOURCES: &[ResourceId] = &[
    ResourceId::Timer { group: 0, index: 0 },
    ResourceId::Timer { group: 1, index: 0 },
];

pub static HIL_REQUIREMENTS: &[HilRequirement<'static>] = &[
    HilRequirement {
        id: "identity.revision-and-option",
        kind: HilKind::BoardIdentity,
        resources: &[],
        required_for: Qualification::Bench,
    },
    HilRequirement {
        id: "peripherals.fitted-common",
        kind: HilKind::PeripheralSmoke,
        resources: FITTED_DEVICE_HIL_RESOURCES,
        required_for: Qualification::Bench,
    },
    HilRequirement {
        id: "buses.concurrent-service-load",
        kind: HilKind::PeripheralSmoke,
        resources: SHARED_BUS_HIL_RESOURCES,
        required_for: Qualification::Bench,
    },
    HilRequirement {
        id: "safe.vibration-reset-fault-watchdog",
        kind: HilKind::SafeState,
        resources: VIBRATION_HIL_RESOURCES,
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
    use super::*;

    #[test]
    fn imported_device_topology_validates_without_arm_claim() {
        assert_eq!(PACKAGE.validate(), Ok(()));
        assert!(!PACKAGE.armable);
        assert_eq!(DEVICES.len(), 12);
        assert!(
            DEVICES
                .iter()
                .filter(|device| device.support == SupportLevel::Compiles)
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

    #[test]
    fn gpio45_is_touch_reset_not_epd_reset() {
        let epd = DEVICES
            .iter()
            .find(|entry| entry.resource == ResourceId::Device(device::EPD))
            .unwrap();
        let touch = DEVICES
            .iter()
            .find(|entry| entry.resource == ResourceId::Device(device::TOUCH))
            .unwrap();
        assert!(!epd.auxiliary_resources.contains(&ResourceId::Gpio(45)));
        assert!(touch.auxiliary_resources.contains(&ResourceId::Gpio(45)));
    }
}
