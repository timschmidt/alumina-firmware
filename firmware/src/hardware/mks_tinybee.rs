use alumina_board::{BoardPackage, ResourceId};
use alumina_config::RealtimeConfigurationProfile;
use alumina_motion::{
    OutputCommitToken, ScheduledShiftOutput, ShiftImageContract, ShiftImageUpdate,
};
use alumina_protocol::DeviceCycle;
use alumina_safety::{MAX_SAFETY_INPUTS, SafetyContractId, SafetyInputMonitor};
use alumina_sd_spi::{Config as SdConfig, SdSpiCard};
use alumina_service::CACHE_LIMITS;
use alumina_shift_register::{
    BitOrder, CompleteImage, Error as ShiftError, ImageError, StaticShiftRegister, Timing,
    TimingError,
};
use alumina_storage::provisioning::ProvisionedCache;
use defmt::{info, warn};
use embassy_time::Delay;
use esp_hal::delay::Delay as BlockingDelay;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig};
use esp_hal::peripherals::{
    ADC1, DMA_I2S0, GPIO0, GPIO1, GPIO2, GPIO3, GPIO4, GPIO5, GPIO12, GPIO13, GPIO14, GPIO15,
    GPIO16, GPIO17, GPIO18, GPIO19, GPIO21, GPIO22, GPIO23, GPIO25, GPIO26, GPIO27, GPIO32, GPIO33,
    GPIO34, GPIO35, GPIO36, GPIO39, I2S0, Peripherals, SPI2, TIMG1, UART0, UART2, WIFI,
};
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::time::Rate;

use super::RuntimeResources;
use super::safety_inputs::{
    BiasCapability, SafetyInputBackendError, SafetyInputBank, SafetyInputRoute, SafetyInputScan,
};
use crate::storage::EspSdSpiBus;

#[path = "mks_tinybee_pcm_short.rs"]
pub(crate) mod pcm_short;

pub type StorageCard = SdSpiCard<EspSdSpiBus, Output<'static>, Delay>;
pub type StorageBackend = ProvisionedCache<StorageCard>;
/// Initial canonical motion-stream width for the three exposed XYZ axes.
pub const JOB_AXES: usize = 3;
/// Maximum unique resource claims retained by each configuration validator.
pub const CONFIGURATION_BINDINGS: usize = 64;
/// A complete-image writer exists, but its blocking GPIO timing has not been
/// qualified as a motion serializer and therefore cannot authorize arming.
pub const MOTION_OUTPUT_IMPLEMENTED: bool = true;
/// Physical step/dir output remains closed until I²S/DMA HIL evidence exists.
pub const MOTION_OUTPUT_QUALIFIED: bool = false;
/// Placeholder one-cycle grid for the unqualified static target. The future
/// I²S backend must replace this with its measured continuous frame quantum.
pub const MOTION_OUTPUT_QUANTUM_CYCLES: u32 = 1;
/// No nonzero commit-lateness claim is made before serializer qualification.
pub const MOTION_MAXIMUM_COMMIT_LATENESS_CYCLES: u32 = 0;
/// No prestart lead is qualified. The impossible value reinforces the closed
/// package/serializer arm gates until target DMA timing is measured.
pub const MOTION_MINIMUM_PRIME_LEAD_CYCLES: u64 = u64::MAX;
/// Fixed portable generated-image capacity compiled into the future owner.
pub const MOTION_OUTPUT_RING_IMAGES: usize = 64;
/// Structural prefill request used only after a qualified target backend
/// replaces the currently unreachable streaming methods.
pub const MOTION_PRIME_HORIZON_CYCLES: u64 = 20_000;

/// Exact full-width shifted-output mapping consumed by the portable executor.
pub const fn motion_shift_contract() -> Option<ShiftImageContract> {
    Some(ShiftImageContract {
        engine: 0,
        width: board_mks_tinybee::SHIFT_CHAIN_WIDTH,
        defined_mask: board_mks_tinybee::COMPLETE_SHIFT_MASK,
        safe_image: board_mks_tinybee::DESCRIBED_SAFE_I2S_IMAGE,
    })
}

/// Semantic identity of GPIO2 and the four sampled digital inputs retained
/// with output drivers disabled, plus the exact 24-bit static image.
///
/// Byte fields are `TBSC`, version 2, width 24, MSB-first, image LE, five
/// input-mode GPIOs (2, 22, 32, 33, 35), static GPIO bootstrap, and a 100 ns
/// timing floor. Configuration-owned pull bias does not enable an output
/// driver. Change this whenever that transaction changes; HIL qualification
/// remains independently false in the board package.
pub const SAFE_OUTPUT_CONTRACT: SafetyContractId = SafetyContractId([
    b'T', b'B', b'S', b'C', 2, 24, 1, 0x49, 0x12, 0, 0, 5, 1, 100, 0, 0,
]);

/// Core-0 tokens. They cannot be constructed again or moved from this owner.
#[allow(dead_code, reason = "tokens are reserved for staged service drivers")]
pub struct ServiceResources {
    wifi: Option<WIFI<'static>>,
    spi2: Option<SPI2<'static>>,
    spi_miso: Option<GPIO19<'static>>,
    spi_mosi: Option<GPIO23<'static>>,
    spi_clock: Option<GPIO18<'static>>,
    sd_chip_select: Option<GPIO5<'static>>,
    uart0: UART0<'static>,
    uart0_tx: GPIO1<'static>,
    uart0_rx: GPIO3<'static>,
    uart2: UART2<'static>,
    uart2_tx_lcd_d7: GPIO17<'static>,
    uart2_rx_lcd_d5: GPIO16<'static>,
    lcd_d4_boot: GPIO0<'static>,
    lcd_register_select: GPIO4<'static>,
    encoder_b: GPIO12<'static>,
    encoder_press: GPIO13<'static>,
    encoder_a: GPIO14<'static>,
    lcd_d6: GPIO15<'static>,
    lcd_enable: GPIO21<'static>,
}

impl ServiceResources {
    /// Consumes otherwise dormant service tokens and establishes the
    /// disconnected-load HIL marker on nonhazardous GPIO4/LCD_RS.
    ///
    /// The marker starts at the package's safe-low value. The isolated HIL
    /// artifact is its only caller; production service composition cannot
    /// select this feature or recover the discarded peripheral tokens.
    #[cfg(feature = "hil-mks-tinybee-pcm-short-safe")]
    pub fn into_hil_capture_marker(self) -> Output<'static> {
        Output::new(
            self.lcd_register_select,
            Level::Low,
            OutputConfig::default(),
        )
    }

    /// Consumes otherwise dormant service tokens and establishes the two
    /// disconnected-load graph HIL observations on EXP1 pins 4 and 3.
    ///
    /// GPIO4/LCD_RS marks only the fixed graph release call. GPIO21/LCD_EN
    /// mirrors the value observed at the graph sink after that call. Both
    /// outputs start low, every display cable must remain disconnected, and
    /// the returned pins may be transferred to core 1 only by the isolated
    /// HIL artifact.
    #[cfg(feature = "hil-mks-tinybee-graph-input-timing-safe")]
    pub fn into_graph_hil_markers(self) -> (Output<'static>, Output<'static>) {
        (
            Output::new(
                self.lcd_register_select,
                Level::Low,
                OutputConfig::default(),
            ),
            Output::new(self.lcd_enable, Level::Low, OutputConfig::default()),
        )
    }

    /// Moves the singleton radio token into core-0 network initialization once.
    pub fn take_wifi(&mut self) -> WIFI<'static> {
        self.wifi.take().expect("Wi-Fi token already consumed")
    }

    /// Identifies the fitted card but leaves cache-region selection to an
    /// explicit later provisioning transaction.
    pub async fn initialize_storage(&mut self) -> StorageBackend {
        let chip_select = Output::new(
            self.sd_chip_select
                .take()
                .expect("SD chip-select token already consumed"),
            Level::High,
            OutputConfig::default(),
        );
        let spi = Spi::new(
            self.spi2.take().expect("SPI2 token already consumed"),
            SpiConfig::default()
                .with_frequency(Rate::from_khz(400))
                .with_mode(Mode::_0),
        )
        .unwrap_or_else(|_| panic!("invalid fixed TinyBee SD SPI configuration"))
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
        let bus = EspSdSpiBus::new(spi, [None, None, None]);
        let mut card = SdSpiCard::new(bus, chip_select, Delay, SdConfig::DEFAULT)
            .unwrap_or_else(|_| panic!("invalid fixed TinyBee SD transport policy"));
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

/// Core-1 singleton tokens before the hazardous safe-output transaction.
#[allow(
    dead_code,
    reason = "tokens are reserved for safe I2S and limit bring-up"
)]
pub struct RealtimeResources {
    timer_group1: TIMG1<'static>,
    i2s0: I2S0<'static>,
    i2s_dma: DMA_I2S0<'static>,
    i2s_clock: GPIO25<'static>,
    i2s_data: GPIO27<'static>,
    i2s_word_select: GPIO26<'static>,
    limit_x: GPIO33<'static>,
    limit_y: GPIO32<'static>,
    limit_z: GPIO22<'static>,
    probe_servo: GPIO2<'static>,
    thermistor_1_sd_detect: GPIO34<'static>,
    material_detect: GPIO35<'static>,
    thermistor_0: GPIO36<'static>,
    thermistor_bed: GPIO39<'static>,
    adc1: ADC1<'static>,
}

type SafeShift =
    StaticShiftRegister<Output<'static>, Output<'static>, Output<'static>, BlockingDelay>;

/// Core-1 resources after the complete static shift image and GPIO2 high-Z mode.
///
/// The I²S/DMA tokens remain owned but deliberately unconfigured. A later
/// streaming backend must perform an explicit no-glitch handoff from `safe_shift`.
#[allow(dead_code, reason = "tokens remain reserved for staged RT drivers")]
pub struct EstablishedRealtimeResources {
    timer_group1: TIMG1<'static>,
    i2s0: I2S0<'static>,
    i2s_dma: DMA_I2S0<'static>,
    safe_shift: SafeShift,
    probe_servo: Input<'static>,
    safety_inputs: SafetyInputBank<4>,
    thermistor_1_sd_detect: GPIO34<'static>,
    thermistor_0: GPIO36<'static>,
    thermistor_bed: GPIO39<'static>,
    adc1: ADC1<'static>,
}

/// Static board-contract defect detected before core 1 may publish `Safe`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafeOutputError {
    /// Complete-image width/mask/value contract was invalid.
    Image(ImageError),
    /// Bounded shift/latch timing contract was invalid.
    Timing(TimingError),
    /// Fixed board safety-input routes were internally inconsistent.
    SafetyInput(SafetyInputBackendError),
    /// No qualified continuous output timeline is reachable in this package.
    MotionStreamingUnsupported,
}

impl RealtimeResources {
    /// Establishes every RT-owned hazardous output before any task await point.
    pub fn establish_safe_outputs(self) -> Result<EstablishedRealtimeResources, SafeOutputError> {
        let probe_servo = Input::new(self.probe_servo, InputConfig::default());

        // RCLK/WS is configured low first so constructing the other two output
        // drivers cannot commit an unknown storage-register image.
        let latch = Output::new(self.i2s_word_select, Level::Low, OutputConfig::default());
        let clock = Output::new(self.i2s_clock, Level::Low, OutputConfig::default());
        let data = Output::new(self.i2s_data, Level::Low, OutputConfig::default());
        let mut safe_shift = infallible_pins(StaticShiftRegister::new(
            clock,
            data,
            latch,
            BlockingDelay::new(),
        ))?;
        infallible_pins(
            safe_shift.write_complete(tinybee_safe_image(), Timing::CONSERVATIVE_100NS),
        )?;
        let safety_inputs = SafetyInputBank::new([
            SafetyInputRoute::new(
                ResourceId::Gpio(33),
                self.limit_x,
                BiasCapability::PullUpDown,
            ),
            SafetyInputRoute::new(
                ResourceId::Gpio(32),
                self.limit_y,
                BiasCapability::PullUpDown,
            ),
            SafetyInputRoute::new(
                ResourceId::Gpio(22),
                self.limit_z,
                BiasCapability::PullUpDown,
            ),
            SafetyInputRoute::new(
                ResourceId::Gpio(35),
                self.material_detect,
                BiasCapability::FloatingOnly,
            ),
        ])
        .map_err(SafeOutputError::SafetyInput)?;

        Ok(EstablishedRealtimeResources {
            timer_group1: self.timer_group1,
            i2s0: self.i2s0,
            i2s_dma: self.i2s_dma,
            safe_shift,
            probe_servo,
            safety_inputs,
            thermistor_1_sd_detect: self.thermistor_1_sd_detect,
            thermistor_0: self.thermistor_0,
            thermistor_bed: self.thermistor_bed,
            adc1: self.adc1,
        })
    }
}

impl EstablishedRealtimeResources {
    /// Applies a complete configuration-derived GPIO-input transaction.
    pub fn configure_safety_inputs(
        &mut self,
        profile: &RealtimeConfigurationProfile,
        nominal_scan_period_cycles: u64,
    ) -> Result<Option<SafetyInputMonitor<MAX_SAFETY_INPUTS>>, SafetyInputBackendError> {
        self.safety_inputs
            .configure(profile, nominal_scan_period_cycles)
    }

    /// Samples all active safety inputs from their sole core-1 owner.
    pub fn scan_safety_inputs(
        &self,
        monitor: &mut SafetyInputMonitor<MAX_SAFETY_INPUTS>,
        at: DeviceCycle,
    ) -> Result<SafetyInputScan, SafetyInputBackendError> {
        self.safety_inputs.scan(monitor, at)
    }

    /// Removes configuration-specific pulls without relinquishing pin ownership.
    pub fn clear_safety_inputs(&mut self) {
        self.safety_inputs.clear();
    }

    /// Reapplies the complete hazardous-output image synchronously on core 1.
    pub fn force_safe_outputs(&mut self) -> Result<(), SafeOutputError> {
        self.probe_servo.apply_config(&InputConfig::default());
        infallible_pins(
            self.safe_shift
                .write_complete(tinybee_safe_image(), Timing::CONSERVATIVE_100NS),
        )
    }

    /// Applies one complete mapped image through the bootstrap transport.
    /// This path exists for compile-time integration and eventual low-rate HIL;
    /// [`MOTION_OUTPUT_QUALIFIED`] keeps it outside arm authority.
    #[allow(
        dead_code,
        reason = "retained only for disconnected-load static-image HIL"
    )]
    pub fn apply_motion_image(&mut self, update: ShiftImageUpdate) -> Result<(), SafeOutputError> {
        infallible_pins(self.safe_shift.write_complete(
            CompleteImage {
                bits: update.image,
                ..tinybee_safe_image()
            },
            Timing::CONSERVATIVE_100NS,
        ))
    }

    /// Rejects future-timeline capacity queries until the compile-only
    /// PCM-short owner has physical phase, refill, observation, and stop
    /// evidence.
    pub fn motion_output_writable_horizon(
        &self,
        _observed: DeviceCycle,
    ) -> Result<DeviceCycle, SafeOutputError> {
        Err(SafeOutputError::MotionStreamingUnsupported)
    }

    /// Rejects ordered future-plan staging on the retained static GPIO owner.
    pub fn stage_motion_output(
        &mut self,
        _output: ScheduledShiftOutput,
    ) -> Result<(), SafeOutputError> {
        Err(SafeOutputError::MotionStreamingUnsupported)
    }

    /// Rejects continuous-horizon sealing on the retained static GPIO owner.
    /// A qualified implementation must make every changed and unchanged frame
    /// through `through` hardware-owned before returning success.
    pub fn seal_motion_output_horizon(
        &mut self,
        _through: DeviceCycle,
    ) -> Result<DeviceCycle, SafeOutputError> {
        Err(SafeOutputError::MotionStreamingUnsupported)
    }

    /// Rejects commit observation because DMA/WS is not the active owner.
    pub fn take_motion_commit(
        &mut self,
    ) -> Result<Option<(OutputCommitToken, DeviceCycle)>, SafeOutputError> {
        Err(SafeOutputError::MotionStreamingUnsupported)
    }
}

const fn tinybee_safe_image() -> CompleteImage {
    CompleteImage {
        width: board_mks_tinybee::SHIFT_CHAIN_WIDTH,
        defined_mask: board_mks_tinybee::COMPLETE_SHIFT_MASK,
        bits: board_mks_tinybee::DESCRIBED_SAFE_I2S_IMAGE,
        // Vendor schematic U1 receives serial data and cascades U1→U2→U3.
        // Q128/bit 0 therefore enters last; bit 23 enters first.
        order: BitOrder::MostSignificantFirst,
    }
}

fn infallible_pins<T>(
    result: Result<T, ShiftError<core::convert::Infallible>>,
) -> Result<T, SafeOutputError> {
    match result {
        Ok(value) => Ok(value),
        Err(ShiftError::Image(error)) => Err(SafeOutputError::Image(error)),
        Err(ShiftError::Timing(error)) => Err(SafeOutputError::Timing(error)),
        Err(ShiftError::Pin(unreachable)) => match unreachable {},
    }
}

pub struct SplitResources {
    pub runtime: RuntimeResources,
    pub service: ServiceResources,
    pub realtime: RealtimeResources,
}

#[cfg(feature = "board-mks-tinybee")]
pub const PACKAGE: &BoardPackage<'static> = &board_mks_tinybee::PACKAGE;
#[cfg(feature = "board-mks-tinybee-4mb")]
pub const PACKAGE: &BoardPackage<'static> = &board_mks_tinybee::PACKAGE_4_MIB;

/// Consumes the HAL singleton once and creates physically disjoint domains.
pub fn split(peripherals: Peripherals) -> SplitResources {
    let Peripherals {
        TIMG0: timer_group0,
        TIMG1: timer_group1,
        CPU_CTRL: cpu_control,
        SW_INTERRUPT: software_interrupt,
        WIFI: wifi,
        SPI2: spi2,
        GPIO19: spi_miso,
        GPIO23: spi_mosi,
        GPIO18: spi_clock,
        GPIO5: sd_chip_select,
        UART0: uart0,
        GPIO1: uart0_tx,
        GPIO3: uart0_rx,
        UART2: uart2,
        GPIO17: uart2_tx_lcd_d7,
        GPIO16: uart2_rx_lcd_d5,
        GPIO0: lcd_d4_boot,
        GPIO4: lcd_register_select,
        GPIO12: encoder_b,
        GPIO13: encoder_press,
        GPIO14: encoder_a,
        GPIO15: lcd_d6,
        GPIO21: lcd_enable,
        I2S0: i2s0,
        DMA_I2S0: i2s_dma,
        GPIO25: i2s_clock,
        GPIO27: i2s_data,
        GPIO26: i2s_word_select,
        GPIO33: limit_x,
        GPIO32: limit_y,
        GPIO22: limit_z,
        GPIO2: probe_servo,
        GPIO34: thermistor_1_sd_detect,
        GPIO35: material_detect,
        GPIO36: thermistor_0,
        GPIO39: thermistor_bed,
        ADC1: adc1,
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
            spi2: Some(spi2),
            spi_miso: Some(spi_miso),
            spi_mosi: Some(spi_mosi),
            spi_clock: Some(spi_clock),
            sd_chip_select: Some(sd_chip_select),
            uart0,
            uart0_tx,
            uart0_rx,
            uart2,
            uart2_tx_lcd_d7,
            uart2_rx_lcd_d5,
            lcd_d4_boot,
            lcd_register_select,
            encoder_b,
            encoder_press,
            encoder_a,
            lcd_d6,
            lcd_enable,
        },
        realtime: RealtimeResources {
            timer_group1,
            i2s0,
            i2s_dma,
            i2s_clock,
            i2s_data,
            i2s_word_select,
            limit_x,
            limit_y,
            limit_z,
            probe_servo,
            thermistor_1_sd_detect,
            material_detect,
            thermistor_0,
            thermistor_bed,
            adc1,
        },
    }
}
