use alumina_board::BoardPackage;
use esp_hal::peripherals::{
    ADC1, DMA_I2S0, GPIO5, GPIO18, GPIO19, GPIO22, GPIO23, GPIO25, GPIO26, GPIO27, GPIO32, GPIO33,
    I2S0, Peripherals, SPI2, TIMG1, WIFI,
};

use super::RuntimeResources;

/// Core-0 tokens. They cannot be constructed again or moved from this owner.
#[allow(dead_code, reason = "tokens are reserved for staged service drivers")]
pub struct ServiceResources {
    wifi: WIFI<'static>,
    spi2: SPI2<'static>,
    spi_miso: GPIO19<'static>,
    spi_mosi: GPIO23<'static>,
    spi_clock: GPIO18<'static>,
    sd_chip_select: GPIO5<'static>,
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
        I2S0: i2s0,
        DMA_I2S0: i2s_dma,
        GPIO25: i2s_clock,
        GPIO27: i2s_data,
        GPIO26: i2s_word_select,
        GPIO33: limit_x,
        GPIO32: limit_y,
        GPIO22: limit_z,
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
            wifi,
            spi2,
            spi_miso,
            spi_mosi,
            spi_clock,
            sd_chip_select,
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
            adc1,
        },
    }
}
