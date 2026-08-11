use alumina_board::BoardPackage;
use alumina_config::RealtimeConfigurationProfile;
use alumina_motion::{ShiftImageContract, ShiftImageUpdate};
use alumina_protocol::DeviceCycle;
use alumina_safety::{MAX_SAFETY_INPUTS, SafetyContractId, SafetyInputMonitor};
use alumina_sd_spi::{Config as SdConfig, SdSpiCard};
use alumina_service::CACHE_LIMITS;
use alumina_storage::provisioning::ProvisionedCache;
use defmt::{info, warn};
use embassy_time::Delay;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig};
use esp_hal::peripherals::{
    DMA_CH0, DMA_CH1, GPIO0, GPIO1, GPIO2, GPIO3, GPIO4, GPIO5, GPIO6, GPIO12, GPIO13, GPIO14,
    GPIO15, GPIO16, GPIO17, GPIO18, GPIO21, GPIO33, GPIO34, GPIO35, GPIO36, GPIO37, GPIO38, GPIO39,
    GPIO42, GPIO43, GPIO44, GPIO45, GPIO46, GPIO47, GPIO48, I2C0, I2S0, Peripherals, SPI2, TIMG1,
    UART1, WIFI,
};
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::time::Rate;

use super::RuntimeResources;
use super::safety_inputs::{SafetyInputBackendError, SafetyInputBank, SafetyInputScan};
use crate::storage::EspSdSpiBus;

pub type StorageCard = SdSpiCard<EspSdSpiBus, Output<'static>, Delay>;
pub type StorageBackend = ProvisionedCache<StorageCard>;
/// Protocol-only width for the sole RT-owned vibration output.
///
/// T-Deck Pro remains non-armable and no machine block drives this output.
pub const JOB_AXES: usize = 1;
/// Maximum unique resource claims retained by each configuration validator.
pub const CONFIGURATION_BINDINGS: usize = 64;
/// T-Deck Pro exposes no machine step/dir output backend.
pub const MOTION_OUTPUT_IMPLEMENTED: bool = false;
/// No machine output can authorize arming on this board package.
pub const MOTION_OUTPUT_QUALIFIED: bool = false;
/// No physical commit-lateness claim exists.
pub const MOTION_MAXIMUM_COMMIT_LATENESS_CYCLES: u32 = 0;

/// T-Deck Pro has no shifted machine-output contract.
pub const fn motion_shift_contract() -> Option<ShiftImageContract> {
    None
}

/// Semantic identity of the current RT hazard contract: GPIO2 held high-Z.
pub const SAFE_OUTPUT_CONTRACT: SafetyContractId =
    SafetyContractId([b'T', b'D', b'S', b'C', 1, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

/// Core-0 tokens for every currently imported T-Deck peripheral path.
#[allow(dead_code, reason = "tokens are reserved for staged service actors")]
pub struct ServiceResources {
    wifi: Option<WIFI<'static>>,
    i2c0: I2C0<'static>,
    i2c_scl: GPIO14<'static>,
    i2c_sda: GPIO13<'static>,
    touch_interrupt: GPIO12<'static>,
    keyboard_interrupt: GPIO15<'static>,
    keyboard_led: GPIO42<'static>,
    touch_reset: GPIO45<'static>,
    ambient_light_interrupt: GPIO16<'static>,
    imu_interrupt: GPIO21<'static>,
    sensor_1v8_enable: GPIO38<'static>,
    spi2: Option<SPI2<'static>>,
    spi_dma: DMA_CH0<'static>,
    spi_clock: Option<GPIO36<'static>>,
    spi_mosi: Option<GPIO33<'static>>,
    spi_miso: Option<GPIO47<'static>>,
    epd_chip_select: Option<GPIO34<'static>>,
    epd_data_command: GPIO35<'static>,
    epd_busy: GPIO37<'static>,
    lora_chip_select: Option<GPIO3<'static>>,
    lora_busy: GPIO6<'static>,
    lora_reset: GPIO4<'static>,
    lora_dio1: GPIO5<'static>,
    lora_power: Option<GPIO46<'static>>,
    sd_chip_select: Option<GPIO48<'static>>,
    gps_uart: UART1<'static>,
    gps_rx: GPIO44<'static>,
    gps_tx: GPIO43<'static>,
    gps_enable: GPIO39<'static>,
    gps_pps: GPIO1<'static>,
    microphone_i2s: I2S0<'static>,
    microphone_dma: DMA_CH1<'static>,
    microphone_data: GPIO17<'static>,
    microphone_clock: GPIO18<'static>,
    boot_button: GPIO0<'static>,
}

impl ServiceResources {
    /// Moves the singleton radio token into core-0 network initialization once.
    pub fn take_wifi(&mut self) -> WIFI<'static> {
        self.wifi.take().expect("Wi-Fi token already consumed")
    }

    /// Identifies SD while retaining inactive EPD/LoRa selects on the shared
    /// bus. No cache region is selected or formatted implicitly.
    pub async fn initialize_storage(&mut self) -> StorageBackend {
        let sd_chip_select = Output::new(
            self.sd_chip_select
                .take()
                .expect("SD chip-select token already consumed"),
            Level::High,
            OutputConfig::default(),
        );
        let epd_chip_select = Output::new(
            self.epd_chip_select
                .take()
                .expect("EPD chip-select token already consumed"),
            Level::High,
            OutputConfig::default(),
        );
        let lora_chip_select = Output::new(
            self.lora_chip_select
                .take()
                .expect("LoRa chip-select token already consumed"),
            Level::High,
            OutputConfig::default(),
        );
        let lora_power = Output::new(
            self.lora_power
                .take()
                .expect("LoRa power token already consumed"),
            Level::Low,
            OutputConfig::default(),
        );
        let spi = Spi::new(
            self.spi2.take().expect("SPI2 token already consumed"),
            SpiConfig::default()
                .with_frequency(Rate::from_khz(400))
                .with_mode(Mode::_0),
        )
        .unwrap_or_else(|_| panic!("invalid fixed T-Deck SD SPI configuration"))
        .with_sck(
            self.spi_clock
                .take()
                .expect("SPI clock token already consumed"),
        )
        .with_mosi(
            self.spi_mosi
                .take()
                .expect("SPI MOSI token already consumed"),
        )
        .with_miso(
            self.spi_miso
                .take()
                .expect("SPI MISO token already consumed"),
        )
        .into_async();
        let bus = EspSdSpiBus::new(
            spi,
            [
                Some(epd_chip_select),
                Some(lora_chip_select),
                Some(lora_power),
            ],
        );
        let mut card = SdSpiCard::new(bus, sd_chip_select, Delay, SdConfig::DEFAULT)
            .unwrap_or_else(|_| panic!("invalid fixed T-Deck SD transport policy"));
        match card.initialize().await {
            Ok(card_info) => {
                info!("SD card identified: blocks={}", card_info.block_count);
                let mut cache = ProvisionedCache::new(card, CACHE_LIMITS);
                match cache.discover().await {
                    Ok(status) => info!(
                        "SD cache discovery complete: generation={}",
                        status.locator_generation
                    ),
                    Err(_) => warn!("SD cache discovery failed closed"),
                }
                cache
            }
            Err(_) => {
                warn!("SD card identification failed; cache remains faulted");
                ProvisionedCache::transport_faulted(card, CACHE_LIMITS)
            }
        }
    }
}

/// T-Deck RT tokens before vibration GPIO high-impedance establishment.
#[allow(
    dead_code,
    reason = "timer is reserved for the deadline probe and later RT I/O"
)]
pub struct RealtimeResources {
    timer_group1: TIMG1<'static>,
    vibration_motor: GPIO2<'static>,
}

/// T-Deck RT resources after explicitly disabling the GPIO2 output driver.
#[allow(dead_code, reason = "tokens remain reserved for staged RT drivers")]
pub struct EstablishedRealtimeResources {
    timer_group1: TIMG1<'static>,
    vibration_motor: Input<'static>,
    safety_inputs: SafetyInputBank<0>,
}

/// Static board-contract defect detected before core 1 may publish `Safe`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafeOutputError {
    /// Fixed board safety-input routes were internally inconsistent.
    SafetyInput(SafetyInputBackendError),
    /// This board package has no machine-output engine.
    MotionUnsupported,
}

impl RealtimeResources {
    /// Establishes the only RT-owned hazardous pin as a retained input/high-Z.
    pub fn establish_safe_outputs(self) -> Result<EstablishedRealtimeResources, SafeOutputError> {
        let safety_inputs = SafetyInputBank::<0>::new([]).map_err(SafeOutputError::SafetyInput)?;
        Ok(EstablishedRealtimeResources {
            timer_group1: self.timer_group1,
            vibration_motor: Input::new(self.vibration_motor, InputConfig::default()),
            safety_inputs,
        })
    }
}

impl EstablishedRealtimeResources {
    /// Rejects any configured safety route because T-Deck Pro exposes none to
    /// the real-time machine domain in this board package.
    pub fn configure_safety_inputs(
        &mut self,
        profile: &RealtimeConfigurationProfile,
        nominal_scan_period_cycles: u64,
    ) -> Result<Option<SafetyInputMonitor<MAX_SAFETY_INPUTS>>, SafetyInputBackendError> {
        self.safety_inputs
            .configure(profile, nominal_scan_period_cycles)
    }

    /// Samples the active monitor; an admitted T-Deck profile is necessarily empty.
    pub fn scan_safety_inputs(
        &self,
        monitor: &mut SafetyInputMonitor<MAX_SAFETY_INPUTS>,
        at: DeviceCycle,
    ) -> Result<SafetyInputScan, SafetyInputBackendError> {
        self.safety_inputs.scan(monitor, at)
    }

    /// Keeps the empty route set in its canonical state.
    pub fn clear_safety_inputs(&mut self) {
        self.safety_inputs.clear();
    }

    /// Reasserts GPIO2 as an input/high-impedance safe state.
    pub fn force_safe_outputs(&mut self) -> Result<(), SafeOutputError> {
        self.vibration_motor.apply_config(&InputConfig::default());
        Ok(())
    }

    /// Rejects every machine-output update on this non-motion board.
    pub fn apply_motion_image(&mut self, _update: ShiftImageUpdate) -> Result<(), SafeOutputError> {
        Err(SafeOutputError::MotionUnsupported)
    }
}

pub struct SplitResources {
    pub runtime: RuntimeResources,
    pub service: ServiceResources,
    pub realtime: RealtimeResources,
}

pub const PACKAGE: &BoardPackage<'static> = &board_t_deck_pro::PACKAGE;

/// Consumes the HAL singleton once and creates physically disjoint domains.
pub fn split(peripherals: Peripherals) -> SplitResources {
    let Peripherals {
        TIMG0: timer_group0,
        TIMG1: timer_group1,
        CPU_CTRL: cpu_control,
        SW_INTERRUPT: software_interrupt,
        WIFI: wifi,
        I2C0: i2c0,
        GPIO14: i2c_scl,
        GPIO13: i2c_sda,
        GPIO12: touch_interrupt,
        GPIO15: keyboard_interrupt,
        GPIO42: keyboard_led,
        GPIO45: touch_reset,
        GPIO16: ambient_light_interrupt,
        GPIO21: imu_interrupt,
        GPIO38: sensor_1v8_enable,
        SPI2: spi2,
        DMA_CH0: spi_dma,
        GPIO36: spi_clock,
        GPIO33: spi_mosi,
        GPIO47: spi_miso,
        GPIO34: epd_chip_select,
        GPIO35: epd_data_command,
        GPIO37: epd_busy,
        GPIO3: lora_chip_select,
        GPIO6: lora_busy,
        GPIO4: lora_reset,
        GPIO5: lora_dio1,
        GPIO46: lora_power,
        GPIO48: sd_chip_select,
        UART1: gps_uart,
        GPIO44: gps_rx,
        GPIO43: gps_tx,
        GPIO39: gps_enable,
        GPIO1: gps_pps,
        I2S0: microphone_i2s,
        DMA_CH1: microphone_dma,
        GPIO17: microphone_data,
        GPIO18: microphone_clock,
        GPIO0: boot_button,
        GPIO2: vibration_motor,
        ..
    } = peripherals;

    SplitResources {
        runtime: RuntimeResources {
            timer_group0,
            cpu_control,
            software_interrupt,
        },
        service: ServiceResources {
            wifi: Some(wifi),
            i2c0,
            i2c_scl,
            i2c_sda,
            touch_interrupt,
            keyboard_interrupt,
            keyboard_led,
            touch_reset,
            ambient_light_interrupt,
            imu_interrupt,
            sensor_1v8_enable,
            spi2: Some(spi2),
            spi_dma,
            spi_clock: Some(spi_clock),
            spi_mosi: Some(spi_mosi),
            spi_miso: Some(spi_miso),
            epd_chip_select: Some(epd_chip_select),
            epd_data_command,
            epd_busy,
            lora_chip_select: Some(lora_chip_select),
            lora_busy,
            lora_reset,
            lora_dio1,
            lora_power: Some(lora_power),
            sd_chip_select: Some(sd_chip_select),
            gps_uart,
            gps_rx,
            gps_tx,
            gps_enable,
            gps_pps,
            microphone_i2s,
            microphone_dma,
            microphone_data,
            microphone_clock,
            boot_button,
        },
        realtime: RealtimeResources {
            timer_group1,
            vibration_motor,
        },
    }
}
