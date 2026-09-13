# M3 exact Brotli interface bundle

Date: 2026-08-26

Status: the authoritative interface is retained at maximum standard Brotli
settings and passes exact crate, simulator HTTP, and real Chromium checks. An
earlier physical TinyBee image transferred its then-current Brotli payload
byte-exactly through the AP. The current two-admission 8 MiB primary builds and
fits; the opportunistic 4 MiB variant links but is rejected as 114,400 bytes
oversize. The current exact image has no physical browser qualification.

## Exact bundle authority

The bundle was captured from the `alumina-interface` working tree based on
commit `0a111d2e5243db0e356e9befa1ac28c77785753a`. The exact source hashes below,
rather than the base commit alone, identify the uncommitted application state.
Its manifest digest is
`8588ad72e8a4b516d1299060f9ad470d9d1db397bf6a4c96a3fb99b18cef9e49`.

| Route | Stored representation | Wire bytes | Wire SHA-256 | Source bytes |
| --- | --- | ---: | --- | ---: |
| `/` | identity | 1,077 | `3ccd883461eb1c786c2418512f8cb6094a141a8459a2dd597fad00cccf5a3a2b` | 1,077 |
| `/alumina-bootstrap.js` | identity | 16,235 | `2d3d556f85918dd4d4a93490b40a6d957c55bce5449274220a898171307fef18` | 16,235 |
| `/alumina-brotli-decoder_bg.wasm` | identity | 196,134 | `8a656d9cb18dfce82838f06f6153005a6cfa0c22c3cf575ef5cf30932fb46cab` | 196,134 |
| `/alumina-interface.js.br` | Brotli q11/w23 | 10,909 | `5d6515aa29962f29b17b25496c0911c3ba9b4ca619fe9fdbaef60c35f55b10c3` | 91,816 |
| `/alumina-interface_bg.wasm.br` | Brotli q11/w23 | 2,820,967 | `9c6310d1993a7facbfd97d84be6185b20cb72af2d6997433e3bee40b1e20a415` | 8,421,767 |
| `/alumina-worker.js` | identity | 678 | `2d23324fa4fe34169abcc34046dcbcadbd13a02b99958f465ab3878a709c9a75` | 678 |
| `/favicon.ico` | identity | 196 | `311f2089f900726358e69d1c490236942ea17aa418f198764a5662c8eaeeb5af` | 196 |

The 2,755-byte manifest is served as the eighth route. Reproducible gzip level
9 over these exact JS/WASM sources uses 3,635,709 bytes; q11/w23 Brotli uses
2,831,876 bytes, an 803,833-byte (22.11%) reduction. There is no gzip copy or
content-negotiation compatibility path.

An exhaustive quality-11 standard-window sweep retained the byte minimum, not
merely the numerically largest setting. The JS stream falls from 14,136 bytes
at window 10 to its 10,909-byte minimum at automatic selection and windows 17
through 24. The WASM stream falls from 2,873,032 bytes at window 20 to its
2,820,967-byte minimum at automatic selection and windows 23 through 24. The
incompatible large-window extension ties at window 24 and grows to 10,910 JS
bytes and 2,820,968 WASM bytes above it. Standard q11/w23 is therefore a
minimum-size winner for both exact source assets, uses the smallest winning
decoder window, and remains RFC 7932 compatible.

Plain-HTTP Chromium does not reliably advertise Brotli. The firmware therefore
serves opaque `.br` resources with identity HTTP coding. An integrity-pinned
bootstrap loads a 196,134-byte dedicated permissively licensed WASM decoder,
validates wire and source SHA-256 on insecure LAN origins, and imports only the
verified decoded module. The generator uses `brotli -q 11 -w 23 -n`: maximum
quality with the smallest standard window attaining the exhaustive minimum,
without the nonstandard large-window extension.

## Bounded TinyBee browser schedule

Physical tracing showed that one listener serves `/` but refuses Chromium's
external bootstrap connection while that first socket is still owned. Three
permanent listeners are too expensive on classic ESP32. The finite browser
schedule now has a peak of two:

1. the document declares a data-URL favicon, eliminating Chromium's speculative
   `/favicon.ico` request;
2. the external bootstrap is the only document subresource;
3. bootstrap fetches the decoder first, then downloads only JS and WASM in
   parallel; and
4. the worker is requested after both payloads have completed.

TinyBee consequently publishes two independently listening 1,536-byte RX/TX
socket pairs and two HTTP task slots. Its exact graph arenas were reduced to
1 KiB service state, 1 KiB realtime state, 2 KiB service channels, 2 KiB
realtime channels, and 2 KiB cross-core channels. Those are reported capability
limits, not hidden truncation. T-Deck Pro retains its larger graph envelope and
three HTTP workers; the unmeasured MKS FOC target retains one worker for now.

## Startup safety admission

The physical log from the preceding image exposed a core-0-generated E-stop.
Static startup-path analysis identified the expected expired snapshot backlog
after radio initialization as the matching trigger path. Safety observation
had also been coupled to the optional diagnostic-overview freshness value,
which is zero on targets without that provider. The final image uses an
independent 500 ms safety window and two explicit rendezvous: one fresh
matching `Safe` snapshot before radio initialization, then another after the
expected expired startup FIFO has drained and before the service actor can
admit requests.

`StartupSafetyGate` is allocation-free and portable. Its tests prove that an
expired valid snapshot grants no authority and remains `AwaitingFresh`, a later
fresh matching snapshot establishes the gate, a fresh real-time fault is
surfaced as `Faulted`, and malformed or contract-substituted evidence remains a
rejection. Strict ESP32 and ESP32-S3 lint plus release links cover TinyBee 8
MiB, TinyBee 4 MiB, MKS ESP32 FOC V1, T-Deck Pro, and the current T-LoRa Pager
stub. This is software evidence; the interrupted TinyBee write prevents a
physical claim for the final startup sequence.

## Simulator browser qualification

The checked-in simulator served the same table at `127.0.0.1:8098` to a fresh
Chromium 151 profile. The strict harness passed all checks: exact resources and
headers, bootstrap evidence, installed WASM bindings, 1440×1000 canvas,
nontrivial screenshot, no network favicon, no load failure, and no exception.
Bootstrap completed in 2,780.5 ms.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| result JSON | 9,954 | `b288d4735e1bc11ecdfdfec55fa0e27e7127ee753db5500df06e837071191ae6` |
| visually inspected PNG | 229,558 | `be566144c6a9d214e12264173ad329ddc9fa6f3bbba8756853031024a61fccf5` |

The rendered application showed its intentional offline, non-armable TinyBee
fixture and exact Hypercurve diagnostic projection. This proves delivery and
execution, not control authority or a physical MCU session.

## Physical boundary

On the bare, USB-powered MKS TinyBee V1.0 with no motor or motor power, the
immediately preceding one-worker Brotli image associated successfully over
`wlp2s0f0u14`. Direct requests verified that image's then-current resource
identities; its 2,362,454-byte WASM Brotli stream completed in 45.800784 seconds
with its exact SHA-256 and no firmware reset. Browser tracing then proved that the
sole occupied listener refused the bootstrap and favicon connections. That is
the evidence which drove the finite two-connection schedule.

On 2026-08-26 the fixture re-enumerated and `espflash board-info` reconfirmed an
ESP32 revision 1.0 with 8 MiB flash. Flashing that then-current primary image reached
the exact 4,166,032-byte app/partition report, then `/dev/ttyUSB0` disappeared
before `espflash` reported write verification. The stalled process was stopped
without claiming completion. The fixture has not re-enumerated again, so
neither the write nor any boot/network behavior of this exact image is claimed.

## Linked images and flash fit

The primary release ELF is
`6805cc1458a158c0315315df3f5ae50d29b27a74dc28c111020c100f0bed8acb`.
Its linked `.bss` is 162,660 bytes, residual core-0 `.stack` is 21,084 bytes,
and bootloader-reclaimed `.dram2_uninit` is 98,304 bytes. These are static link
facts, not runtime stack-watermark or sustained allocator evidence.

`espflash save-image` reports 4,243,168 app bytes for the primary image and
rejects the independently linked 4 MiB variant at the same app length:

| Variant | App partition | Use | Merged image SHA-256 |
| --- | ---: | ---: | --- |
| 8 MiB primary | 8,323,072 | 50.98% | `7eaf3d9f01859f9c251d376cae177455eea5def1c71baf282bc34deba8873346` |
| 4 MiB opportunistic | 4,128,768 | oversize by 114,400 bytes | none |

The unpadded merged primary image is 4,308,704 bytes. No 4 MiB merged image is
produced. The complete UI remains intact in the primary image; the 4 MiB result
is compile-only and no physical 4 MiB fixture exists.

## Reproduction and license boundary

```console
bash tools/alumina-brotli-decoder/build-assets.sh
bash tools/alumina-brotli-decoder/verify-maximum-compression.sh
cargo test -p alumina-web-assets -p board-mks-tinybee
cargo test -p alumina-safety
cargo test -p alumina-sim http_fixture
cargo xtask build --board mks-tinybee-v1 --profile release
cargo xtask build --board mks-tinybee-v1-4mb --profile release
ALUMINA_DEVICE_ORIGIN=http://127.0.0.1:8098 \
  node tests/browser/qualify-embedded-interface.mjs
```

The captured interface is MIT. Firmware, embedding, decoder, simulator, and
tests are `MIT OR Apache-2.0`. No GPL/AGPL/LGPL/SSPL-family implementation,
dependency, or asset was added.
