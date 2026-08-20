# Capability-bound visual assets

Date: 2026-08-20

This checkpoint adds the immutable raster transport and browser presentation
boundary needed for annotated board views. It deliberately uses a small,
unmistakably synthetic simulator image. Every physical package remains
visual-empty until a licensed photograph of the exact revision and its hotspot
map have been reviewed against the fixture.

The reviewed implementation commits are:

- `alumina-firmware`:
  `31810498727dcab1d30fdfe8b4871ce1cfd515b2`
- `alumina-interface`:
  `7da8437d30b4776f7e208cd97c81ef8697da4808`

## Immutable service authority

The canonical capability remains the sole declaration authority for a visual.
Its record commits the stable view ID, provenance path, media type, exact pixel
dimensions, SHA-256, SPDX license, attribution, and normalized typed-resource
polygons. The path is descriptive provenance only; firmware and browser never
interpret it as a URL or filesystem grant.

`CapabilityVisualGet` is operation `0x0202` in the Capabilities frame family.
Its canonical V1 request is an 88-byte `ALMVAQ01` body containing the exact
declaring capability digest, exact asset digest, offset, and a nonzero range
ceiling no larger than 240 bytes. Its response is a 96-byte `ALMVAR01` prefix
plus exactly 1 through 240 bytes. The prefix repeats both digests, pins the
complete asset length, reports the exact offset/chunk length, and marks
completion only at the asset end. Reserved bytes, zero identities, zero chunks,
oversized chunks, overflow, out-of-range offsets, trailing bytes, and false
completion are rejected.

`VerifiedCapabilityVisualAssets::try_new` verifies the package's complete
canonical identity, a unique nonzero catalog, complete declaration coverage,
representable nonempty lengths, absence of undeclared entries, and SHA-256 over
every complete asset. The returned borrowed wrapper exposes no mutable bytes or
unchecked constructor and can be retained by a board composition. A range read
requires the same package capability digest and an asset digest declared by
that exact package. A well-formed visual read against a visual-empty package is
`Unsupported`; a foreign digest is not treated as a package asset.

The maximum visual response body is 336 bytes and remains within the existing
fixed 512-byte service response. Retrieval is read-only and grants no resource
lease, GPIO mode, write, configuration mutation, diagnostic acquisition, arm
transition, safety reset, motion, or process-energy authority.

## Explicit simulator fixture

The host-only `sim-mks-tinybee-v1` package now has this distinct identity:

| Fact | Value |
| --- | --- |
| canonical capability bytes | 4,028 |
| capability SHA-256 | `218cc758f430f8897f8c7dbcda6c1af2077fae5fc0062cce2fe7cb49a066aa79` |
| visual ID | `simulated-topology` |
| raster | 109-byte, 40 by 20 RGB PNG |
| asset SHA-256 | `79fcf464e343ffc8f33cdec96a8e2fd87da886c57dd4be080898a5fc59b52412` |
| license | `CC0-1.0` |
| attribution | Alumina deterministic four-quadrant simulator fixture; not a PCB photograph |
| hotspots | GPIO22, GPIO32, GPIO33, and GPIO35 in four quadrants |

The PNG bytes are repository-owned and embedded in the simulator source. The
fixture borrows no physical identity, visual correspondence, connector
placement, photograph, GPIO state, serial endpoint, radio, or device handle.
Its four polygons exist only to make acquisition, picking, and typed-resource
cross-linking mechanically testable.

The simulator HTTP fixture constructs the verified catalog before dispatching
each fixture request. Its authenticated service test reads the exact complete
range, decodes the response identity, and independently hashes the returned
bytes. A deliberately substituted byte string fails catalog construction.

## Worker acquisition and rendering boundary

`VisualAssetDownloadMachine` is a retry-safe contiguous assembler for one exact
capability/asset digest pair. The first accepted range pins total length. Every
later response must retain the same identities and length at the exact next
offset. Ambiguous transport can only repeat the same range. No asset slice is
exposed until every byte is present and its complete SHA-256 matches.

Worker schema V13 derives unique asset digests only after the complete
authenticated capability independently decodes. It advances at most four
240-byte visual ranges after a heartbeat, downloads assets sequentially, caps
one compressed asset at 8 MiB and all compressed assets at 16 MiB, and retains
visual failures independently of clocks, health, configuration, telemetry,
capture, and jobs. A completed credential-free document crosses JSON once; its
private byte field, bounds, identities, and hash are revalidated after decoding.

The rendering supervisor requires the current connection generation and an
already admitted capability that declares the asset. It hashes the bytes again,
stores an immutable `Rc<[u8]>`, and removes stale entries on generation,
identity, capability, or disconnect transitions. The first renderer accepts
PNG only, requires exact declared dimensions, limits one decoded RGBA image to
64 MiB and all unique images in a capability to 128 MiB, and gives the decoder
the declared width/height plus a 128 MiB transient allocation ceiling. A native
test decodes the exact simulator PNG and rejects substituted dimensions.

The egui overlay projects normalized arbitrary polygons only after those gates.
Hover and selection link to the same typed resource used by the board explorer,
passive overview, retained capture, and graph catalog. The selected panel shows
owner, safe value, hazard status, aliases, independent overview/capture/graph
availability, and the latest telemetry value/provenance/quality/cycle when one
exists. A visual-empty physical capability instead displays a generic missing
photograph/evidence gate and draws no inferred silhouette or connector.

## Optimized localhost browser evidence

The final optimized bundle was served from `127.0.0.1:8097`; the authenticated
simulator listened on `127.0.0.1:8098`. The production module worker in headless
Chromium 151 ran `visual-final` with the `qualified` expectation and reached
`passed` with:

- board ID `sim-mks-tinybee-v1`, generation 1;
- exact 4,028-byte capability identity above;
- one selected and one complete visual;
- exact range progress 109/109 bytes;
- the exact capability and asset digests above;
- PNG signature `89 50 4e 47 0d 0a 1a 0a`;
- zero consecutive visual failures and no visual error; and
- qualified clock plus available active configuration in the same snapshot.

The browser, simulator, and static server were stopped after the run. This
qualifies optimized worker acquisition and transfer, not egui pixel output,
operator usability, photograph accuracy, radio transport, or physical I/O.
The native decoder test and warnings-denied WASM build separately cover the PNG
dimension and compiled rendering paths; no pixel-level claim is made.

## Verification

At the implementation commits named above:

- both repositories passed package-scoped formatting and `git diff --check`;
- `cargo test --locked --offline` passed the complete default firmware suite;
- firmware all-target warnings-denied Clippy passed;
- focused protocol/capability/service/simulator tests passed 9, 9, 22, and 65
  tests respectively, including canonical wire, catalog substitution, and
  authenticated byte-exact range coverage;
- `cargo test --workspace --all-targets --locked --offline` passed the complete
  interface suite, including 50 application, 82 client, 125 core, and one
  exact-control integration test;
- native and `wasm32-unknown-unknown` workspace no-dependency warnings-denied
  Clippy passed;
- all five dual-core board variants passed strict ESP-target warnings-denied
  Clippy;
- all five board variants linked optimized release artifacts through `xtask`
  without flashing;
- `scripts/audit-source-policy.sh` reported `source policy: local
  Alumina/CSGRS/Hyper stacks; native and WASM license inventories accepted`;
- locked/offline optimized Trunk build, `wasm-tools validate`, gzip/Brotli
  integrity checks, and exact decompression comparisons passed; and
- the final compact Chromium visual expectation passed as described above.

Tool versions were firmware Rust 1.88.0, interface Cargo 1.97.0, Trunk 0.21.14,
wasm-tools 1.235.0, Chromium 151.0.7922.137, and Node.js 22.22.2.

The committed optimized firmware ELF artifacts are:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-firmware-mks-tinybee-v1` | 11,572,516 | `0001139fc60e5558a5c290d2455c1be4d94fd3e47cd3c984cf212c1132a94e1b` |
| `alumina-firmware-mks-tinybee-v1-4mb` | 11,565,688 | `b3f6ca8c5c2ea2d4896de24f48ac155dab9e972248a79c77726ad1886417d25d` |
| `alumina-firmware-mks-esp32-foc-v1` | 10,843,036 | `d2cd82c45b00933289cd2c942a33a7fe2fe451fea703fc81d43b35a7dbf6faba` |
| `alumina-firmware-t-deck-pro` | 11,175,804 | `f206972d0a2281cf30eb33f25454f9295475664535c7a3ae929c34df2ac6535e` |
| `alumina-firmware-t-lora-pager-current` | 10,658,648 | `c1a4ed6d4c91e0d20175726e5e79d1b4f272ec4918cf4223833d14582821d0e0` |

These are compile/link ELF artifacts, not flash-image byte counts, boot results,
or physical qualifications.

The final optimized browser artifacts are:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 6,279,289 | `3314a048485e929e8fd2bf3e70905df7012acde80eb45213dd809b46bfbb2376` |
| `alumina-interface_bg.wasm.gz` | 2,783,714 | `0ef721db6f53cdd26035b0b964c1a31577b90df1befb038e4a8969421d2e759c` |
| `alumina-interface_bg.wasm.br` | 2,200,322 | `a7e97ac85b01f09b95cad2b68525d8b108e48dc08dcaf8dfe153f9c972b63df7` |
| `alumina-interface.js` | 91,816 | `0be0ab353881b29ea6f6f1a82a1ad00100b1d503e374fdb04a785c8e2dad744c` |
| `alumina-interface.js.gz` | 12,984 | `14eb8046bb090f4fe40945f34f93281aed6ed5f13fcec598d9cb3f0cf5161229` |

## License and moving-Hyper boundary

Firmware/client/service/rendering implementation is repository-owned under the
repositories' existing permissive licenses. The synthetic raster is declared
`CC0-1.0`. The only newly direct registry dependency, `image` 0.25.6 with PNG
and no default features, declares `MIT OR Apache-2.0`; it was already present in
the lock graph through eframe. No GPL/AGPL/LGPL/SSPL-family source, dependency,
asset, or copied implementation was introduced.

Interface verification compiled coherent observed snapshots of the current
sibling CSGRS/Hyper stack. Hypercurve remained user-owned, actively edited, and
strictly read-only. Its current source emitted two pre-existing unused-variable
warnings while dependency builds ran; no Alumina no-dependency warnings-denied
check failed. This checkpoint did not inspect sibling repository status/diffs
or edit, reset, pin, format, stage, or commit any sibling CSGRS/Hyper file.

## Claims deliberately kept closed

This checkpoint does not establish:

- a licensed TinyBee, T-Deck Pro, MKS ESP32 FOC, or Pager photograph;
- physical board identity, connector placement, hotspot coordinates, visual
  accuracy, or operator-usability review;
- browser-to-ESP Wi-Fi, AP association, workstation WLAN behavior, or radio
  service latency;
- physical GPIO levels, telemetry timing, capture timing, core isolation,
  stack headroom, SD behavior, or external-instrument correlation;
- I2S/shift-register, MCPWM, encoder, current-sense, endstop, E-stop, safety
  chain, relay, laser, plasma, heater, or industrial-I/O behavior; or
- machine arm, motion, process energy, interlock reset, production, or physical
  qualification authority.

The connected bare MKS TinyBee V1.0 was not contacted: no flash, reset, serial,
GPIO, Wi-Fi association, or other agent-initiated operation occurred. No motor
or motor power was connected. The SLogic16U3 was not requested or used. The
workstation Wi-Fi configuration was not changed. Every ESP command in this
checkpoint was compile/link only.
