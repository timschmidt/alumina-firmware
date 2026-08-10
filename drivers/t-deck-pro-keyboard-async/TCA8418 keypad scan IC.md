# TCA8418 keypad reference

The T-Deck Pro uses a TCA8418 keypad scanner at seven-bit I2C address `0x34`. Its active-low, open-drain `INT` signal connects to ESP32 GPIO15. The controller stores press and release transitions in a ten-entry FIFO so the host can service input asynchronously.

## Initialization used by the driver

`KeyboardController::init` configures the board's 4 × 10 matrix:

| Register | Value | Purpose |
| --- | ---: | --- |
| `KP_GPIO1` (`0x1D`) | `0x0F` | Assign rows 0–3 to the keypad |
| `KP_GPIO2` (`0x1E`) | `0xFF` | Assign columns 0–7 |
| `KP_GPIO3` (`0x1F`) | `0x03` | Assign columns 8–9 |
| `CFG` (`0x01`) | `0x09` | Enable keypad-event and overflow interrupts |
| `INT_STAT` (`0x02`) | `0xFF` | Clear stale interrupt state |

The optional reset output is pulsed before configuration. On the T-Deck Pro, GPIO45 may be shared with other devices, so a combined application should coordinate the pulse before transferring pin ownership.

## Reading events

1. Wait for `INT` to assert low.
2. Read `INT_STAT` and confirm the keypad event bit.
3. Read the low nibble of `KEY_LCK_EC` (`0x03`) for the FIFO count.
4. Read `KEY_EVENT_A` (`0x04`) once per event; each read pops the oldest entry.
5. Decode bit 7 as press/release and bits 6–0 as the matrix index.
6. Clear the keypad interrupt after draining the FIFO.

The workspace driver maps matrix indices through its base and Alt key tables. It tracks modifiers internally and emits the first nonmodifier chord member as a coalesced `KeyEvent` at the release boundary; it is not a lossless stream of every raw FIFO transition.

## References

- [Bundled TCA8418 datasheet](docs/tca8418.pdf)
- [Texas Instruments TCA8418 product page](https://www.ti.com/product/TCA8418)
- [`KeyboardController` usage and behavior](README.md)
