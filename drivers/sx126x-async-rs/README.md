# sx126x_async

Low-level asynchronous `no_std` driver for Semtech SX1261 and SX1262 radios. It accepts any `embedded-hal-async` `SpiDevice`, reset and antenna-switch outputs, and BUSY and DIO1 inputs that implement asynchronous `Wait`.

This fork began as an async port of [`sx126x-rs`](https://github.com/tweedegolf/sx126x-rs). LoRa operation is the exercised path; GFSK is represented in parts of the command API but is not yet complete.

## Core API

- `SX126x<SPI, RESET, BUSY, ANT, DIO1>` owns the radio transport and pins.
- `conf::Config` describes the initialization sequence, including modulation, PA, IRQ, RF, and optional TCXO settings.
- `op::LoraModParams`, `op::LoRaPacketParams`, `op::TxParams`, and `op::PaConfig` provide typed builders for modem commands.
- `calc_rf_freq` converts a desired frequency and crystal frequency into the SX126x PLL register value.
- `write_bytes`, `set_rx`, `get_rx_buffer_status`, `read_buffer`, and the IRQ methods form the packet transmit/receive path.
- Register, status, calibration, and error types are available through `reg`, `op`, and the crate root.

## Initialization

Construct a device from a HAL-provided SPI device and four pins, then supply the complete configuration:

```rust,ignore
use sx126x_async::{SX126x, calc_rf_freq, conf::Config, op::*};

let mut radio = SX126x::new(spi, (reset, busy, antenna_enable, dio1));

let dio1 = IrqMask::none()
    .combine(IrqMaskBit::TxDone)
    .combine(IrqMaskBit::RxDone)
    .combine(IrqMaskBit::Timeout);
let rf_hz = 868_000_000;
let config = Config {
    packet_type: PacketType::LoRa,
    sync_word: 0x1424,
    calib_param: CalibParam::all(),
    mod_params: LoraModParams::default().into(),
    pa_config: PaConfig::default().set_device_sel(DeviceSel::SX1262),
    packet_params: Some(LoRaPacketParams::default().into()),
    tx_params: TxParams::default(),
    dio1_irq_mask: dio1,
    dio2_irq_mask: IrqMask::none(),
    dio3_irq_mask: IrqMask::none(),
    rf_freq: calc_rf_freq(rf_hz as f32, 32_000_000.0),
    rf_frequency: rf_hz,
    tcxo_opts: None,
};
radio.init(config).await?;
```

`rf_freq` is the raw PLL command value; `rf_frequency` is the frequency in hertz used to select image-calibration parameters. Set both consistently. Public and private LoRa sync words are `0x3444` and `0x1424`, respectively.

Transmit a variable-length packet with:

```rust,ignore
radio
    .write_bytes(
        b"hello",
        RxTxTimeout::from_ms(5_000),
        15,
        LoRaCrcType::CrcOn,
    )
    .await?;
```

For the board-specific wiring and a shorter configuration surface, use [`t-deck-pro-lora-async`](../t-deck-pro-lora-async/README.md).

## References

- [Semtech SX1261/SX1262 product documentation](https://www.semtech.com/products/wireless-rf/lora-connect/sx1262)
- [SX1261/SX1262 datasheet](https://resource.semtech.com/document/sx1261-sx1262-datasheet)
- [Original `sx126x-rs` API](https://docs.rs/sx126x)
- [`embedded-hal-async` SPI and digital traits](https://docs.rs/embedded-hal-async/1/embedded_hal_async/)

## License

Licensed under MIT or Apache-2.0, matching the upstream crate.
