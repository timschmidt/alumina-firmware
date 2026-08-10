use alumina_board::BoardPackage;
use alumina_sd_spi::{Config as SdConfig, SdSpiCard};
use alumina_service::UnprovisionedStorageBackend;
use defmt::{info, warn};
use embassy_time::Delay;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::peripherals::{
    ADC1, DMA_I2S0, GPIO0, GPIO1, GPIO2, GPIO3, GPIO4, GPIO5, GPIO12, GPIO13, GPIO14, GPIO15,
    GPIO16, GPIO17, GPIO18, GPIO19, GPIO21, GPIO22, GPIO23, GPIO25, GPIO26, GPIO27, GPIO32, GPIO33,
    GPIO34, GPIO35, GPIO36, GPIO39, I2S0, Peripherals, SPI2, TIMG1, UART0, UART2, WIFI,
};
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::time::Rate;

use super::RuntimeResources;
use crate::storage::EspSdSpiBus;

pub type StorageCard = SdSpiCard<EspSdSpiBus, Output<'static>, Delay>;
pub type StorageBackend = UnprovisionedStorageBackend<StorageCard>;

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
                UnprovisionedStorageBackend::identified(card, card_info.block_count)
            }
            Err(_) => {
                warn!("SD card identification failed; cache remains faulted");
                UnprovisionedStorageBackend::faulted(card)
            }
        }
    }
}

/// Core-1 tokens. The hazardous I²S engine remains unconfigured/non-armable.
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

pub struct SplitResources {
    pub runtime: RuntimeResources,
    pub service: ServiceResources,
    pub realtime: RealtimeResources,
}

pub const PACKAGE: &BoardPackage<'static> = &board_mks_tinybee::PACKAGE;

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
