# Canonical board capability document V1

The `ALMCAP01` document is the immutable byte authority shared by firmware,
`xtask`, simulation, and the browser/WASM interface. It serializes the complete
typed `BoardPackage` without Rust layout, JSON key ordering, allocation, or
platform-width dependence. The package's declared capability digest is excluded
from its own bytes; SHA-256 over the complete document is compiled back into the
package and independently recomputed by board checks and firmware before Wi-Fi
starts.

Changing any serialized board fact, array order, string byte, visual digest,
qualification, or armability requires a new capability digest. This is a
deliberately conservative V1 compatibility identity. Cached machine partitions
bind that exact digest.

## Common encoding

Every integer is little-endian. Booleans are exactly `0` or `1`. Every string is
`length: u32` followed by that many UTF-8 bytes, with no terminator or Unicode
normalization. Every list is `count: u32` followed by entries in declared package
order. Every reserved byte is zero. Unknown enum values, impossible options,
length overflow, a structurally invalid board package, and a declared/calculated
digest mismatch fail closed.

The fixed 16-byte header is:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 0 | 8 | ASCII `ALMCAP01` |
| 8 | 2 | exact schema version (`1`) |
| 10 | 2 | reserved zero |
| 12 | 4 | complete document length including this header |

One typed resource is four bytes: `kind: u8`, `first: u8`, and `second: u16`.
`first` is the engine/unit/group for two-coordinate resources and zero otherwise;
`second` is the channel/bit/index. Resource-kind values are:

| Value | Resource | Value | Resource |
| ---: | --- | ---: | --- |
| 1 | GPIO | 10 | UART |
| 2 | I2S shifted output | 11 | PCNT |
| 3 | ADC channel | 12 | DMA |
| 4 | timer group/index | 13 | TWAI |
| 5 | I2S engine | 14 | storage |
| 6 | RMT | 15 | radio |
| 7 | timed-output engine/channel | 16 | safety input |
| 8 | I2C | 17 | device or routed device endpoint |
| 9 | SPI |  |  |

## Payload order

The header is followed by these fields with no implicit padding:

1. board ID string, revision string;
2. chip, application-core count, qualification, and armable flag as four bytes;
3. flash, internal SRAM, and PSRAM byte counts as three `u64` values;
4. realtime-PSRAM flag as one byte;
5. service-core, realtime-core, and two reserved zero bytes;
6. resources, aliases, buses, devices, flash regions, clocks, electrical
   constraints, interrupts, safe-output images, visuals, then HIL requirements.

The section entries are canonical as follows:

- resource: resource ID, owner, safe value, hazardous flag, reserved zero;
- alias: name string, resource ID;
- bus: resource ID, kind, owner, two reserved bytes, maximum frequency `u32`,
  pin count and resource IDs;
- device: resource ID, owner, support, two reserved bytes, fixed eight-byte
  optional bus, fixed eight-byte route, auxiliary-resource count and IDs;
- flash region: name, offset `u32`, length `u32`, kind, writable-while-armed,
  support, reserved zero;
- clock: name, source, domain, support, error-present flag, nominal Hz `u64`,
  maximum-error PPM `u32` (zero when absent);
- electrical constraint: ID string, kind, support, two reserved bytes, resource
  count and IDs, note string;
- interrupt: resource ID, owner, trigger, support, latency-present flag,
  maximum-latency cycles `u64` (zero when absent);
- safe-output image: engine, bench-verified flag, two reserved bytes,
  defined-mask `u32`, safe bits `u32`;
- visual: ID, asset path, media type, width `u32`, height `u32`, 32-byte asset
  digest, SPDX license string, attribution string, hotspot count; each hotspot
  is ID, resource ID, point count, then `x: u16, y: u16` normalized points;
- HIL requirement: ID, kind, required qualification, two reserved bytes,
  resource count and IDs.

An optional resource is `present: u8`, three reserved bytes, then a resource ID;
the entire eight bytes are zero when absent. A device route is eight bytes:
kind plus three reserved bytes, followed by a resource ID for SPI chip select,
an I2C address followed by three zeros, or four zeros for dedicated/UART routes.
A device entry may represent a fitted component or a board-routed configurable
endpoint for an external component. Its support level proves only the declared
driver/route implementation; machine configuration and physical evidence still
have to establish that an optional device is actually present and calibrated.

Enum values are explicitly assigned in schema order:

| Enum | Values beginning at 1 |
| --- | --- |
| chip | ESP32, ESP32-S3 |
| owner | service, realtime |
| qualification | described, compiles, bench, motion-qualified, production-qualified |
| safe value | not-applicable, high-impedance, low, high, engine-image |
| support | described, compiles, bench, qualified |
| bus | I2C, SPI, UART |
| device route | dedicated, I2C address, SPI chip select, UART |
| flash region | bootloader, partition table, application, configuration, web bundle, update slot, crash log |
| clock source | crystal, PLL, peripheral bus, RTC, external |
| clock domain | chip, service, realtime |
| electrical constraint | input-only, output-only, boot-strap, shared-route, active-high, active-low, not-PWM, logic-3v3, reset-state-unverified |
| interrupt trigger | rising, falling, any-edge, low-level, high-level, configurable |
| HIL kind | board identity, safe state, peripheral smoke, core isolation, timing, fault injection, visual reconciliation |

## Authenticated range transport

The public identity JSON reports the compiled digest and total document bytes.
The UI then sends `CapabilitiesGet` through authenticated
`POST /api/v1/control`. The outer native frame must use the capabilities family
and a zero configuration digest.

The exact 56-byte request body is:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 0 | 8 | ASCII `ALMCPQ01` |
| 8 | 2 | version `1` |
| 10 | 2 | reserved zero |
| 12 | 4 | document offset |
| 16 | 2 | requested bytes, `1..=240` |
| 18 | 6 | reserved zero |
| 24 | 32 | expected digest, or zero only for initial discovery |

An expected nonzero digest must equal the selected board's compiled identity.
The response body is a fixed 64-byte prefix followed by exactly `chunk length`
document bytes:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 0 | 8 | ASCII `ALMCPR01` |
| 8 | 2 | version `1` |
| 10 | 1 | bit 0: this chunk reaches the document end |
| 11 | 5 | reserved zero |
| 16 | 4 | complete document length |
| 20 | 4 | returned offset |
| 24 | 2 | following chunk bytes, `1..=240` |
| 26 | 6 | reserved zero |
| 32 | 32 | complete document SHA-256 digest |
| 64 | variable | exact document bytes |

The prefix and data fit the existing 312-byte native response-body budget. The
UI requires contiguous offsets, stable length/digest on every response, exact
completion, and a final SHA-256 match before caching or interpreting the
document. Range retrieval grants no configuration, arming, or execution
authority.

Current identities are:

| Board | Bytes | SHA-256 |
| --- | ---: | --- |
| MKS TinyBee V1.x | 3,251 | `000f151d9a404a94d82b311ab4033db23fe65e56c48d8a1d43bd773662c7c351` |
| T-Deck Pro | 2,605 | `617a1b62b7e7f68762a8950ebe582f47bfd20b66d8cd052363a4d841e08eec10` |
| MKS ESP32 FOC V1.0 | 2,808 | `8b14c17fc2787bce93e10610a39157e33a5a10fac477e910021f166b562532e3` |
