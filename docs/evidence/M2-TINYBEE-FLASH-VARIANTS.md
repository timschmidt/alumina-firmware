# M2 TinyBee flash-variant evidence

Date: 2026-08-26

Status: 8 MiB remains the primary TinyBee target. The separately identified
4 MiB variant still links the complete maximum-compression Brotli interface,
but its app no longer fits the physical partition. No asset was pruned and no
compatibility build was created. Both remain non-armable; there is no physical
4 MiB fixture.

## Exact identities

| Selector | Board ID | Flash bytes | Capability digest |
| --- | --- | ---: | --- |
| `mks-tinybee-v1` | `mks-tinybee-v1` | 8,388,608 | `a185e405f814a3b61153b4db4a784a6a4fbfa73040ab7ece2b3f20e481495c0c` |
| `mks-tinybee-v1-4mb` | `mks-tinybee-v1-4mb` | 4,194,304 | `0ed7a49c0f8b47ca65c697d4335afe8f83a343318f78e33840e9cff1517f58b7` |

The packages share routed V1.x PCB facts but have different board IDs, flash
capacities, and canonical capability documents. Both now publish the measured
TinyBee graph envelope: 1 KiB service state, 1 KiB realtime state, 2 KiB per
local channel arena, and 2 KiB cross-core channels. Firmware selects exactly
one package at compile time and never probes flash to substitute the other.

## Full-image fit

Both release targets link their full seven-asset plus manifest bundle.
`espflash 4.3.0 save-image` encodes the 8 MiB primary and rejects the oversized
4 MiB variant:

| Variant | ELF SHA-256 | App bytes / partition | Merged bytes | Merged SHA-256 |
| --- | --- | ---: | ---: | --- |
| 8 MiB | `6805cc1458a158c0315315df3f5ae50d29b27a74dc28c111020c100f0bed8acb` | 4,243,168 / 8,323,072 (50.98%) | 4,308,704 | `7eaf3d9f01859f9c251d376cae177455eea5def1c71baf282bc34deba8873346` |
| 4 MiB | `9e418eaf59e45272c8d2c05c4b550f38e5c1953148d0adccad79ac28537e8dea` | 4,243,168 / 4,128,768 (oversize) | rejected | none |

The 4 MiB result exceeds the current app partition by 114,400 bytes. It is
compile evidence only, not a flashable image or a production partition
commitment. The fitting 8 MiB image remains primary.

## Reproduction and boundary

```console
cargo xtask board check mks-tinybee-v1
cargo xtask board check mks-tinybee-v1-4mb
cargo xtask capabilities --board mks-tinybee-v1 --json
cargo xtask capabilities --board mks-tinybee-v1-4mb --json
cargo xtask build --board mks-tinybee-v1 --profile release
cargo xtask build --board mks-tinybee-v1-4mb --profile release
espflash save-image --chip esp32 --flash-size 8mb --merge --skip-padding \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1 \
  /tmp/alumina-tinybee-8mb.bin
espflash save-image --chip esp32 --flash-size 4mb --merge --skip-padding \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1-4mb \
  /tmp/alumina-tinybee-4mb.bin
```

On 2026-08-26 the connected V1.0 fixture re-enumerated and `espflash board-info`
reconfirmed ESP32 revision 1.0, dual-core operation, and 8 MiB flash. A write of
the then-current primary image was then attempted, but the USB serial device
disappeared after image sizing and before `espflash` reported write
verification. The stalled process was stopped, the resulting device state is
deliberately unqualified, and the 4 MiB image was not flashed. No motor or motor
power was connected. All implementation is `MIT OR Apache-2.0`; no GPL-family
source, dependency, or asset was introduced.
