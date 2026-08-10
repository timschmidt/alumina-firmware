# MIA-M10Q power and update modes

The MIA-M10Q accepts configuration through u-blox UBX messages. Configuration keys, persistence layers, and power-save behavior are firmware-dependent; use u-blox's interface description for the receiver's protocol version rather than copying unverified hexadecimal messages.

## Driver modes

`Gps::set_power_mode` currently exposes three modes:

| Mode | Current behavior |
| --- | --- |
| `PowerMode::Normal` | Sends `UBX-CFG-VALSET` for a 2,000 ms measurement interval |
| `PowerMode::Eco { update_rate_ms }` | Sends `UBX-CFG-VALSET` for the supplied 16-bit measurement interval |
| `PowerMode::SoftwareStandby` | Sends `UBX-RXM-PMREQ` with the backup flag and zero duration |

Normal and Eco settings target the receiver's RAM layer in the current payload, so they do not persist across a receiver reset. The implementation first attempts to clear the configuration lock and then waits for a UBX ACK or NACK. Eco changes only the measurement interval; it does not yet configure cyclic tracking, static hold, constellation selection, or message filtering.

Software standby remains active until a configured wake source occurs. UART activity may wake the receiver depending on its configuration. Treat RAM-only receiver state as lost across backup transitions.

## Designing lower-power behavior

Power optimization typically combines receiver and host policy:

1. Choose a measurement interval suitable for acquisition conditions and required latency.
2. Disable output sentences the host does not parse.
3. Select a u-blox power-save operating mode only after verifying supported constellations and timing requirements.
4. Validate fix status before accepting position data.
5. Gate or power down the module with the board's enable line when a deeper sleep is appropriate.

Static hold suppresses position drift while stationary; it does not inherently suppress every output message. Applications that report only meaningful movement still need host-side filtering by fix quality, distance, speed, and elapsed time.

## References

- [Bundled MIA-M10Q integration manual](docs/mia-m10q-integration-manual.pdf)
- [u-blox MIA-M10 product resources](https://www.u-blox.com/en/product/mia-m10-series)
- [`Gps`, `GpsData`, and `PowerMode` usage](README.md)
