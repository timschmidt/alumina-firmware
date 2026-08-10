# MIA-M10Q communication reference

The T-Deck Pro connects its u-blox MIA-M10Q receiver to ESP32-S3 UART1. The module emits line-oriented NMEA sentences for navigation data and accepts binary UBX messages for configuration and control.

## Board connection

- ESP32 GPIO43 transmits to the module receiver.
- ESP32 GPIO44 receives from the module transmitter.
- GPIO39 enables module power.
- `Gps::new` configures 38,400 baud, 8 data bits, no parity, and one stop bit.

## NMEA data

`Gps::read_message` collects newline-terminated NMEA sentences and feeds them to the `nmea` crate. A returned `GpsData` may contain date, time, fix type, latitude, longitude, and speed over ground, depending on which sentences have arrived.

Always call `GpsData::has_fix` or inspect `fix_type` before using coordinates. NMEA fields can remain absent until the receiver has decoded the corresponding sentence and acquired a valid solution.

## UBX framing

A UBX packet contains:

1. sync bytes `0xB5 0x62`;
2. one-byte class and message ID;
3. a little-endian 16-bit payload length;
4. the payload; and
5. the two-byte rolling Fletcher checksum over class through payload.

The driver constructs this frame in `send_ubx_message`, limits payloads to its fixed 256-byte packet buffer, and recognizes ten-byte `UBX-ACK-ACK` and `UBX-ACK-NAK` responses. Its `recovery` method sends `UBX-CFG-RST`; its power methods are summarized in [GPS Power modes.md](GPS%20Power%20modes.md).

NMEA traffic can arrive while waiting for UBX acknowledgements. Protocol extensions should synchronize on the UBX header and validate class, ID, length, and checksum rather than assuming the next ten UART bytes form the expected response.

## References

- [Bundled MIA-M10Q integration manual](docs/mia-m10q-integration-manual.pdf)
- [u-blox MIA-M10 product resources](https://www.u-blox.com/en/product/mia-m10-series)
- [NMEA 0183 sentence overview](https://gpsd.gitlab.io/gpsd/NMEA.html)
- [`Gps` API and example](README.md)
