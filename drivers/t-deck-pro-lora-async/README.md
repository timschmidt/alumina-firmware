# t-deck-pro-lora-async

Board-specific asynchronous `no_std` wrapper for the T-Deck Pro's SX1262 LoRa radio. `LoraRadio` combines the generic [`sx126x_async`](../sx126x-async-rs/README.md) driver with ESP32 GPIO ownership and a smaller configuration surface.

## API

- `LoraRadio::new` takes an async `SpiDevice`, RESET, DIO1, BUSY, and module-enable pins.
- `LoraConfig` selects spreading factor, bandwidth, coding rate, PA duty cycle, HP maximum, and sync word.
- `init(&LoraConfig)` resets and configures the radio, then sets 140 mA over-current protection.
- `send` transmits one packet and waits for DIO1.
- `receive` waits up to five seconds, returning `Ok(0)` on timeout, the received length on success, or `Err(())` for an unexpected IRQ or undersized buffer.
- The public `device` field permits lower-level SX126x commands when the wrapper is insufficient.

Current initialization fixes the carrier at 868 MHz, TX power at 22 dBm, preamble at 15 symbols, and packet CRC off. Those settings are not yet fields of `LoraConfig`. Payloads must fit the SX126x 255-byte buffer and comply with local radio regulations.

## Usage

```rust,ignore
use t_deck_pro_lora_async::lora::{LoraConfig, LoraRadio};

let mut radio = LoraRadio::new(spi_device, reset, dio1, busy, enable);
let config = LoraConfig::default();
radio.init(&config).await?;

radio.send(b"hello").await?;

let mut buffer = [0_u8; 255];
let received = radio.receive(&mut buffer).await?;
if received != 0 {
    // Process &buffer[..received].
}
```

SPI2 signals are shared with the e-paper display and SD card. Give every peripheral its own chip-select handle; [`embedded-bus-async`](../embedded-bus-async/README.md) provides the wrapper used by the sender and receiver examples.

## Examples

Build [`example_sender.rs`](examples/example_sender.rs) and [`example_receiver.rs`](examples/example_receiver.rs) for two T-Deck Pro boards. Both must use compatible modulation and sync-word settings.

## References

- [S62F/SX1262 module notes](S62F_SX1262.md)
- [Semtech SX1262 product documentation](https://www.semtech.com/products/wireless-rf/lora-connect/sx1262)
- [SX1261/SX1262 datasheet](https://resource.semtech.com/document/sx1261-sx1262-datasheet)
- [LILYGO T-Deck Pro LoRa example](https://github.com/Xinyuan-LilyGO/T-Deck-Pro)
- [LoRa Alliance regional parameters](https://resources.lora-alliance.org/technical-specifications/lorawan-regional-parameters-v1-0-3reva)

## License

Licensed under [Apache-2.0](../LICENSE).
