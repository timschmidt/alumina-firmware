# Canonical board capability document V2

The `ALMCAP02` document is the immutable byte authority shared by firmware,
`xtask`, simulation, and the browser/WASM interface. It serializes the complete
typed `BoardPackage` without Rust layout, JSON key ordering, allocation, or
platform-width dependence. The package's declared capability digest is excluded
from its own bytes; SHA-256 over the complete document is compiled back into the
package and independently recomputed by board checks and firmware before Wi-Fi
starts.

Changing any serialized board fact, array order, string byte, visual digest,
qualification, or armability requires a new capability digest. This is a
deliberately conservative V2 compatibility identity. Cached machine partitions
bind that exact digest.

V2 additionally publishes the exact fixed graph-executor arenas, implemented
opcode palette, and graph-addressable physical-resource palette. A browser may
lower only against the complete authenticated document for the selected device;
an opcode existing in source code, a GPIO appearing in the general resource
inventory, or spare nominal RAM is not deployment authority.

Installed flash capacity is therefore an identity fact, not a boot-time hint.
For example, the primary 8 MiB TinyBee and its opportunistic 4 MiB variant have
different board IDs and capability digests even though they share routed PCB
resources. Firmware cannot probe capacity and substitute one canonical package
for another; configuration and cached jobs must bind the package actually
compiled into the image.

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
| 0 | 8 | ASCII `ALMCAP02` |
| 8 | 2 | exact schema version (`2`) |
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
6. fixed graph-executor section;
7. resources, aliases, buses, devices, flash regions, clocks, electrical
   constraints, interrupts, safe-output images, visuals, then HIL requirements.

The graph-executor section begins with this fixed 72-byte header:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 0 | 8 | ASCII `ALMGRC02` |
| 8 | 2 | exact deployed graph-IR version |
| 10 | 1 | executor support level |
| 11 | 1 | reserved zero |
| 12 | 4 | exact fixed graph-package bytes |
| 16 | 2 | maximum node records |
| 18 | 2 | maximum channel records |
| 20 | 4 | maximum queue items per channel |
| 24 | 4 | permanently reserved service node-state bytes |
| 28 | 4 | permanently reserved realtime node-state bytes |
| 32 | 4 | service-local channel-arena bytes |
| 36 | 4 | realtime-local channel-arena bytes |
| 40 | 4 | one-way service-to-realtime bridge-arena bytes |
| 44 | 2 | opcode-record count |
| 46 | 2 | graph-resource-record count |
| 48 | 24 | reserved zero |

Each following 12-byte opcode record is opcode, owner domain, support, resource
access, resource class `u32`, then four reserved zero bytes. Resource access and
class are both zero for resource-free operations or both nonzero for a physical
operation. Records are strictly ordered by unique nonzero opcode.

Each 12-byte graph-resource record is a four-byte typed resource ID, access,
support, two reserved zero bytes, and nonzero resource class `u32`. A resource
record is authority only for the exact class/selector/access tuple and must be
backed by a matching realtime opcode. V2 access value `1` means a read of the
fresh debounced semantic state of a configured safety input. It grants no raw
GPIO read and no output authority. TinyBee currently publishes class `1` for
GPIO33, GPIO32, GPIO22, and GPIO35; T-Deck Pro and MKS ESP32 FOC publish no
graph-addressable physical resources yet.

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

## Independent board-explorer decoding

`decode_board_capability` is the allocation-free consumer boundary for a
complete untrusted V2 document. It applies caller-owned limits before exposing
borrowed summary, resource, alias, visual, hotspot, and polygon iterators. The
interactive policy permits at most 4 MiB of document bytes, 64 KiB per string,
4,096 ordinary records per section, 32 visuals, 4,096 hotspots per visual, and
4,096 points per hotspot.

The decoder walks the exact complete section order and rejects malformed UTF-8,
zero-required facts, noncanonical Booleans/enums/options/routes/reserved bytes,
unknown or duplicate typed resources, missing resource references, invalid
core ownership, unsafe shifted-output image coverage, duplicate
aliases/interrupts/images/visuals/hotspots, out-of-plane points, strict prefixes,
trailing bytes, and all caller-limit violations. It retains the existing
independent graph-executor view as a separate, narrower access authority.

The returned SHA-256 is content identity, never transport or device
authentication. A live UI must compare it with the capability identity obtained
from its authenticated session. Merely decoding a GPIO, peripheral, alias, or
visual record grants no resource operation, diagnostic lease, telemetry,
configuration, arming, or deployment authority.

## Authenticated range transport

The public identity JSON reports the compiled digest and total document bytes.
The UI then sends `CapabilitiesGet` through authenticated
`POST /api/v1/control`. The outer native frame must use the capabilities family
and a zero configuration digest.

The exact 56-byte request body is:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 0 | 8 | ASCII `ALMCPQ02` |
| 8 | 2 | version `2` |
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
| 0 | 8 | ASCII `ALMCPR02` |
| 8 | 2 | version `2` |
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
| MKS TinyBee V1.x, 8 MiB primary | 3,435 | `0e82513896e52e0a58fb92de9130c446d590bf649fbc22742209b2d04c8cb0a5` |
| MKS TinyBee V1.x, 4 MiB variant | 3,448 | `ba06ffad44125a4cf5b72ba1a14296a0fb3d91f4364af050616bdf0cebb0ed04` |
| T-Deck Pro | 2,725 | `6c37b509080f40a0ea54e86b9f9aadfed4d284c97494f7e3275af6b3905a8061` |
| MKS ESP32 FOC V1.0 | 2,928 | `627b2c018f44013dec83f1ff118f4158b4de31bb2db6d022c2979a8fd053107f` |
