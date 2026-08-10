#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is unsafe for esp-hal values that borrow transfer buffers"
)]

use alloc::rc::Rc;
use embassy_sync::rwlock::RwLock;
use embedded_bus_async::spi::RwLockDevice;
use esp_hal::clock::CpuClock;
use esp_hal::dma::{DmaRxBuf, DmaTxBuf};
use esp_hal::dma_buffers;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig};
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config, Spi};
use esp_hal::time::Rate;
use esp_hal::timer::systimer::SystemTimer;
use t_deck_pro_lora_async::lora::{LoraConfig, LoraRadio};

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_println::println;
use log::info;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("{}", info);
    loop {}
}

extern crate alloc;

// Emit metadata required by ESP-IDF-compatible bootloaders.
esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    esp_println::logger::init_logger(log::LevelFilter::Debug);

    info!("Logger initialized");

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    info!("Peripherals initialized");

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_rtos::start(timer0.alarm0);

    let lora_cs = Output::new(peripherals.GPIO3, Level::High, OutputConfig::default());
    let lora_busy = Input::new(peripherals.GPIO6, InputConfig::default());
    let lora_rst = Output::new(peripherals.GPIO4, Level::High, OutputConfig::default());
    let lora_int = Input::new(peripherals.GPIO5, InputConfig::default());
    let lora_en = Output::new(peripherals.GPIO46, Level::High, OutputConfig::default());
    let sclk = peripherals.GPIO36;
    let mosi = peripherals.GPIO33;
    let miso = peripherals.GPIO47;
    let dma_channel = peripherals.DMA_CH0;

    Timer::after(Duration::from_secs(1)).await;

    let (rx_buffer, rx_descriptors, tx_buffer, tx_descriptors) = dma_buffers!(32000);
    let dma_rx_buf = DmaRxBuf::new(rx_descriptors, rx_buffer).unwrap();
    let dma_tx_buf = DmaTxBuf::new(tx_descriptors, tx_buffer).unwrap();

    let spi = Spi::new(
        peripherals.SPI2,
        Config::default()
            .with_frequency(Rate::from_mhz(8))
            .with_mode(Mode::_0),
    )
    .unwrap()
    .with_sck(sclk)
    .with_mosi(mosi)
    .with_miso(miso)
    .with_dma(dma_channel)
    .with_buffers(dma_rx_buf, dma_tx_buf)
    .into_async();

    let spi_bus = Rc::new(RwLock::new(spi));
    let lora_spi = RwLockDevice::new(spi_bus, lora_cs, embassy_time::Delay);

    let mut lora = LoraRadio::new(lora_spi, lora_rst, lora_int, lora_busy, lora_en);

    lora.init(&LoraConfig::default()).await.unwrap();

    loop {
        println!("Sending LoRa message");
        lora.send(b"Hello LoRa!").await.unwrap();
        println!("LoRa message sent");
        Timer::after(Duration::from_secs(5)).await;
    }
}
