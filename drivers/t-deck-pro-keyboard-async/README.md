# t-deck-pro-keyboard-async

Asynchronous `no_std` driver for the T-Deck Pro keyboard's TCA8418 keypad scanner. It uses a generic seven-bit `embedded-hal-async` I2C handle plus ESP32 GPIO inputs and optional reset output.

## API

- `KeyboardController::new` owns the I2C handle, active-low interrupt pin, and optional reset pin.
- `init` resets the controller, configures the four-row/ten-column matrix, enables FIFO interrupts, and clears pending status.
- `is_key_pressed` reports whether the active-low interrupt line is asserted.
- `read_key_events` waits for an interrupt, drains the TCA8418 FIFO, tracks Shift and Alt, and returns a bounded `heapless::Vec<KeyEvent, 10>`.
- `KeyEvent` contains the mapped character, `KeyState`, and `MOD_L_SHIFT`, `MOD_R_SHIFT`, and `MOD_ALT` bitmask.
- `BACKSPACE`, `ENTER`, `MIC`, `SPACE`, and `SYM` expose the T-Deck-specific special-key values.

The driver coalesces a chord into its first nonmodifier key and emits that event as `KeyState::Up` after all keys in the chord are released. It does not currently stream every raw TCA8418 down/up transition.

## Usage

```rust,ignore
use t_deck_pro_keyboard_async::keyboard::{KeyState, KeyboardController};

let mut keyboard = KeyboardController::new(i2c, interrupt, reset);
keyboard.init().await?;

loop {
    for event in keyboard.read_key_events().await? {
        if event.state == KeyState::Up {
            // Handle event.key and event.modifiers.
        }
    }
}
```

The keyboard shares I2C0 with the battery and touch controller. See [`embedded-bus-async`](../embedded-bus-async/README.md) for one compatible sharing strategy and [`simple_keyboard.rs`](examples/simple_keyboard.rs) for full board setup.

## References

- [TCA8418 datasheet](docs/tca8418.pdf)
- [Local TCA8418 and T-Deck keymap notes](TCA8418%20keypad%20scan%20IC.md)
- [Texas Instruments TCA8418 product page](https://www.ti.com/product/TCA8418)
- [LILYGO T-Deck Pro keyboard example](https://github.com/Xinyuan-LilyGO/T-Deck-Pro)
- [`embedded-hal-async` I2C trait](https://docs.rs/embedded-hal-async/1/embedded_hal_async/i2c/trait.I2c.html)

## License

Licensed under [Apache-2.0](../LICENSE).
