# TinyBee safe-fixture USB/flash smoke observation

Date: 2026-08-11

Disposition: **inconclusive preparation only**. The isolated safe-image HIL ELF
was identified, flashed, and verified on a user-confirmed bare TinyBee. No logic
analyzer, DSO, meter, or RTT capture was attached, so this is not a waveform,
safe-state, timing, routing, board-revision, or armability result.

## Reproducible identities

- Alumina commit: `e91886a` (`Add exact synchronized current sampling contracts`)
- fixture: `mks-tinybee-pcm-short-safe`
- operator-read PCB marking: `MKS TinyBee v1.0`
- ELF: `target/xtensa-esp32-none-elf/release/alumina-hil-mks-tinybee-pcm-short-safe`
- ELF SHA-256: `db3d2caa48af347445a815bf0507231104f9a1d6ffbfc7a81f1ce449649a46b4`
- linked size: 62,140 text bytes, 3,120 data bytes, 193,488 BSS bytes
- flasher: `espflash 4.3.0`
- stable serial path: `/dev/serial/by-id/usb-1a86_USB_Serial-if00-port0`
- USB bridge: QinHeng CH340, VID:PID `1a86:7523`
- fixture identifier: ESP MAC `c4:de:e2:f8:c4:ac`

The operator stated that this was a bare board with no motors or motor power
connected. No independent photograph or meter record was available to this
run, so the stronger meter-verified precondition in `docs/HIL.md` is not claimed.

## Read-only device observation

`espflash board-info` reported:

```text
Chip type:         esp32 (revision v1.0)
Crystal frequency: 40 MHz
Flash size:        8MB
Features:          WiFi, BT, Dual Core, 240MHz, VRef calibration in efuse, Coding Scheme None
Security features: None
```

The observed 8 MB flash conflicts with the current `mks-tinybee-v1` package's
4 MB declaration even though the PCB is marked MKS TinyBee v1.0. Until the
ESP32 module marking/population is visually identified, this unit must not be
treated as an exact match for that compiled capability identity. The existing
package is not silently changed to fit one incompletely identified specimen.

## Flash observation

The only device mutation was:

```console
espflash flash --chip esp32 \
  --port /dev/serial/by-id/usb-1a86_USB_Serial-if00-port0 \
  --non-interactive --skip-update-check \
  target/xtensa-esp32-none-elf/release/alumina-hil-mks-tinybee-pcm-short-safe
```

`espflash` detected the same ESP32/40 MHz/8 MB facts, reported an application
and partition span of 105,408 / 8,323,072 bytes (1.27%), verified the writes,
hard-reset the device, and returned `Flashing has completed!` with exit status
zero. The fixture can emit only the complete documented disabled image
`0x001249`; it does not initialize Wi-Fi, storage, motion commands, the second
core, or process outputs.

## Missing evidence

The RTT markers were not captured because the CH340 serial connection is not an
RTT transport. No GPIO25 BCLK, GPIO26 WS/RCLK, or GPIO27 DATA probe was attached.
There is therefore no observation of the first latch, frame contents, descriptor
wrap, stop phase, final safe rewrite, reset behavior, current draw, or timing.
A clear annotated board/module photo, meter-verified fixture state, suitable
logic-analyzer capture, raw files, decoded statistics, and human review are all
still required by `docs/HIL.md` before any qualification field can change.
