use alumina_board::BoardPackage;
use esp_hal::peripherals::{
    DMA_CH0, DMA_CH1, GPIO0, GPIO1, GPIO2, GPIO3, GPIO4, GPIO5, GPIO6, GPIO12, GPIO13, GPIO14,
    GPIO15, GPIO16, GPIO17, GPIO18, GPIO21, GPIO33, GPIO34, GPIO35, GPIO36, GPIO37, GPIO38, GPIO39,
    GPIO42, GPIO43, GPIO44, GPIO45, GPIO46, GPIO47, GPIO48, I2C0, I2S0, Peripherals, SPI2, TIMG1,
    UART1, WIFI,
};

use super::RuntimeResources;

/// Core-0 tokens for every currently imported T-Deck peripheral path.
#[allow(dead_code, reason = "tokens are reserved for staged service actors")]
pub struct ServiceResources {
    wifi: WIFI<'static>,
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
    spi2: SPI2<'static>,
    spi_dma: DMA_CH0<'static>,
    spi_clock: GPIO36<'static>,
    spi_mosi: GPIO33<'static>,
    spi_miso: GPIO47<'static>,
    epd_chip_select: GPIO34<'static>,
    epd_data_command: GPIO35<'static>,
    epd_busy: GPIO37<'static>,
    lora_chip_select: GPIO3<'static>,
    lora_busy: GPIO6<'static>,
    lora_reset: GPIO4<'static>,
    lora_dio1: GPIO5<'static>,
    lora_power: GPIO46<'static>,
    sd_chip_select: GPIO48<'static>,
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

/// T-Deck has no qualified hazardous output engine yet; core 1 owns its timer.
#[allow(
    dead_code,
    reason = "timer is reserved for the deadline probe and later RT I/O"
)]
pub struct RealtimeResources {
    timer_group1: TIMG1<'static>,
    vibration_motor: GPIO2<'static>,
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
            wifi,
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
            spi2,
            spi_dma,
            spi_clock,
            spi_mosi,
            spi_miso,
            epd_chip_select,
            epd_data_command,
            epd_busy,
            lora_chip_select,
            lora_busy,
            lora_reset,
            lora_dio1,
            lora_power,
            sd_chip_select,
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
