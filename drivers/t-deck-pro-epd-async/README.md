# t-deck-pro-epd-async

Asynchronous `no_std` framebuffer driver for the T-Deck Pro's 320 × 240 GDEQ031T10 e-paper panel and UC8253 controller. `EInkDisplay` implements `embedded_graphics::DrawTarget<BinaryColor>` and transfers its one-bit framebuffer through any asynchronous `SpiDevice<u8>`.

## API

- `EInkDisplay::new` takes DC, BUSY, optional RESET, logical width and height, and a fast-full-update flag.
- `init` resets the controller and loads its waveform lookup tables.
- `DrawTarget` methods update the in-memory framebuffer without touching the panel.
- `refresh_display` initializes when necessary, transfers old and new full-frame buffers, refreshes, powers down, and records the displayed frame.
- `refresh_partial_display` transfers and refreshes one rectangular region.
- `power_on`, `power_off`, `reset`, and `wait_for_idle` expose controller lifecycle operations.

The backing storage and clipping bounds are currently fixed at 240 × 320 pixels. Use those dimensions even though the constructor accepts width and height. Partial rectangles must lie inside the display and should start and end on byte-aligned X coordinates; the current low-level implementation does not validate every rectangle before slicing the framebuffer.

## Usage

```rust,ignore
use embedded_graphics::{
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
};
use t_deck_pro_epd_async::EInkDisplay;

let mut display = EInkDisplay::new(dc, busy, Some(reset), 240, 320, false);
display.init(&mut spi_device).await?;

display.clear(BinaryColor::Off).unwrap();
Rectangle::new(Point::new(16, 24), Size::new(96, 48))
    .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
    .draw(&mut display)
    .unwrap();

display.refresh_display(&mut spi_device).await?;
```

Drawing is synchronous because it only mutates RAM; refresh methods are asynchronous because they perform SPI I/O and wait for BUSY. The current public error type reports controller timeouts as `()`, while lower-level SPI failures panic. Applications should treat this driver as experimental and keep a hardware reset available for recovery.

The complete ESP32-S3 SPI and GPIO setup is in [`simple_example.rs`](examples/simple_example.rs). When sharing SPI2 with LoRa or storage, wrap the bus with [`embedded-bus-async`](../embedded-bus-async/README.md) and give each device a distinct CS pin.

## References

- [UC8253 controller notes](UC8253%20EPD.md)
- [UC8253 datasheet](docs/UC8253.pdf)
- [GDEQ031T10 panel datasheet](docs/GDEQ031T10-1.pdf)
- [`embedded-graphics` `DrawTarget`](https://docs.rs/embedded-graphics/0.8/embedded_graphics/draw_target/trait.DrawTarget.html)
- [GxEPD2 reference implementation](https://github.com/ZinggJM/GxEPD2)
- [LILYGO T-Deck Pro display example](https://github.com/Xinyuan-LilyGO/T-Deck-Pro)

## License

Licensed under [Apache-2.0](../LICENSE).
