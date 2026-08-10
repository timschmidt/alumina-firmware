# t-deck-pro-touch-async

Experimental asynchronous `no_std` driver for the T-Deck Pro's CST328 touch controller. It uses a generic seven-bit `embedded-hal-async` I2C handle, an active-low ESP32 interrupt input, and an optional reset output.

## API

- `TouchController::new` owns the I2C and GPIO resources.
- `init` performs an optional hardware reset, briefly enters debug-information mode, then selects normal mode.
- `is_touch_active` reports whether the active-low interrupt line is asserted.
- `read_touches` waits up to one second for an interrupt, reads the five hardware touch slots, and returns `heapless::Vec<Option<TouchPoint>, 5>`.
- `TouchPoint` contains a slot ID, `TouchEvent`, and 12-bit X/Y coordinates.

The returned vector currently always contains five entries; use `.into_iter().flatten()` to visit active points. Only controller state `0x06` is decoded, as `TouchEvent::Down`; the other public event variants are reserved for more complete state decoding. Clearing the controller's interrupt/status condition is currently best effort.

## Usage

```rust,ignore
use t_deck_pro_touch_async::touch::TouchController;

let mut touch = TouchController::new(i2c, interrupt, reset);
touch.init().await?;

loop {
    let slots = touch.read_touches().await?;
    for point in slots.into_iter().flatten() {
        // Use point.id, point.event, point.x, and point.y.
    }
}
```

The touch controller shares I2C0 with the keyboard and battery charger. See [`embedded-bus-async`](../embedded-bus-async/README.md) for one compatible sharing strategy and [`simple_touch.rs`](examples/simple_touch.rs) for complete board setup.

## References

- [CST328 datasheet](docs/touch-CST328_V2.2.pdf)
- [ESPHome CST328 reference driver](https://github.com/BluetriX/esphome-CST328-Touch)
- [CIRCUITSTATE CST328 library](https://github.com/CIRCUITSTATE/CSE_CST328)
- [LILYGO T-Deck Pro touch example](https://github.com/Xinyuan-LilyGO/T-Deck-Pro)
- [`embedded-hal-async` I2C trait](https://docs.rs/embedded-hal-async/1/embedded_hal_async/i2c/trait.I2c.html)

## License

Licensed under [Apache-2.0](../LICENSE).
