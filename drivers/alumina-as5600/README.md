# alumina-as5600

`alumina-as5600` is a small `no_std`, allocation-free, asynchronous driver for
the ams OSRAM AS5600 absolute magnetic angle sensor. It is independently
authored from the public device datasheet and uses only the
`embedded-hal-async` seven-bit I2C contract.

The driver is intentionally read-only:

- `read_raw_angle` addresses `RAW ANGLE` and returns its unscaled 12-bit count;
- `read_status` returns the magnet-detected, too-weak, and too-strong flags
  without inventing policy for them; and
- `read_observation` brackets one raw-angle read with two status reads and
  reports whether the defined status flags stayed unchanged.

The one-byte writes used by `write_read` select the AS5600 address pointer. No
configuration, zero/range programming, OTP, or burn command is exposed. The
driver does not turn a count into an electrical angle: board/machine
calibration, direction, pole pairs, alignment error, and the sensor error bound
belong to `alumina-foc`'s digest-bound calibration layer.

Primary reference:

- [AS5600 datasheet, revision v1-06](https://look.ams-osram.com/m/7059eac7531a86fd/original/AS5600-DS000365.pdf)

The datasheet establishes seven-bit address `0x36`, a 12-bit unscaled
`RAW ANGLE` value at registers `0x0C`/`0x0D`, status at `0x0B`, and support for
I2C through Fast-mode Plus. Board composition may select a lower bus rate.

Licensed `MIT OR Apache-2.0`.
