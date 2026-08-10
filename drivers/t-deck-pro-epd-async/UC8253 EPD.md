# UC8253 update sequence

This note summarizes the two-color update flow used by `EInkDisplay` for the T-Deck Pro's 240 × 320 GDEQ031T10 panel. Register values and waveform tables remain panel-specific; consult the bundled controller and panel datasheets before applying the sequence to other hardware.

## Initialization

1. Pull `RST_N` low, deassert it, and wait at least 1 ms before the first command.
2. Wait for active-low `BUSY_N` to return high.
3. Configure panel mode with `PANEL_SETTING` (`0x00`).
4. When fast full updates are enabled, load the four 43-byte transition tables with `LUTWW` (`0x21`), `LUTKW` (`0x22`), `LUTWK` (`0x23`), and `LUTKK` (`0x24`).

The Rust driver retains initialization state and reinitializes after operations that may reset the controller.

## Full refresh

1. Send `POWER_ON` (`0x04`) and wait for `BUSY_N` to deassert.
2. Send the previous full framebuffer with `DTM1` (`0x10`).
3. Send the next full framebuffer with `DTM2` (`0x13`).
4. Configure the VCOM/data interval with `CDI` (`0x50`). Fast mode also applies its cascade and forced-temperature settings.
5. Send `DISPLAY_REFRESH` (`0x12`) and wait for completion.
6. Send `POWER_OFF` (`0x02`), then copy the new frame into the retained previous-frame buffer.

The old frame is required because a two-color waveform depends on each pixel's previous and next states.

## Partial refresh

1. Send a byte-aligned window with `PARTIAL_WINDOW` (`0x90`). The implementation rounds horizontal bounds to eight-pixel boundaries.
2. Enter partial mode with `PARTIAL_IN` (`0x91`).
3. Send the old and new bytes for each row with `DTM1` and `DTM2`.
4. Refresh with `DISPLAY_REFRESH` and wait for `BUSY_N`.
5. Leave partial mode with `PARTIAL_OUT` (`0x92`), power off, and update the corresponding region of the retained frame.

Callers must keep rectangles within the 240 × 320 framebuffer and use nonzero, byte-compatible widths. The current API does not validate every rectangle before indexing its fixed buffers.

## References

- [Bundled UC8253 controller manual](docs/UC8253.pdf)
- [Bundled GDEQ031T10 panel specification](docs/GDEQ031T10-1.pdf)
- [GxEPD2 reference implementation](https://github.com/ZinggJM/GxEPD2)
- [`EInkDisplay` usage and limitations](README.md)
