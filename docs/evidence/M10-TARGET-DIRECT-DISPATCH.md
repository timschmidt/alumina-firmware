# M10 target direct execution-kind dispatch — offline evidence

Date: 2026-08-14

Status: implemented structural target checkpoint. The permanent core-1 motion
actor now selects either coordinated integer motion or direct Q31.32 finite-
difference motion from one validated job descriptor, preserves the selected
kind for the complete job, owns direct continuation and terminal-tail choices,
and schedules normal disable before releasing the final block. This is not an
ESP motion-streaming peripheral, physical timing, WCET, Wi-Fi-load, motor,
machine-accuracy, or armability result.

## Result and moving-source isolation

Firmware commit `4f41be89a5c8ee93f0fdc4e6901ba21e2c53d97b` implements and
documents this boundary. Alumina Interface is unchanged and clean at
`330e3ef40426a07962c8b768bbf5ad1911eb27cd`.

This checkpoint has no Hyper/CSGRS build input. `cargo metadata --no-deps`
reports 35 firmware workspace packages, and all 35 manifest paths are beneath
the `alumina-firmware` repository. Neither `Cargo.toml` nor `Cargo.lock` changed.
Continued Hypercurve editing therefore cannot alter these firmware results.
No transient Hypercurve state is frozen or fingerprinted here, and Alumina did
not edit, format, reset, pin, or otherwise constrain that moving worktree.

## Descriptor-bound fixed-memory owner

`scheduled_execution_mode_from_descriptor` validates the complete
`ALMJOBD3` descriptor for the selected axis width and derives exactly one
`ScheduledExecutionMode`. Direct limits come from that same descriptor. The
fixed-memory `ScheduledShiftedExecution<AXES, OUTPUTS>` enum then constructs
only the coordinated `ScheduledShiftedStepper` or direct
`ScheduledShiftedFiniteDifferenceStepper`; it never interprets payload bytes to
switch kinds or converts one record family into the other.

The common owner exposes kind-preserving admission, planning, staging,
physical commit, block completion, status, terminal disable, and fault
operations. A wrong-family block is rejected unchanged without changing the
live position or output ring. Completion preserves the family-specific
terminal check until the unique admitted block returns to `RealtimeJob`.

A host regression bounds `ScheduledShiftedExecution<3, 64>` to no more than
6 KiB. Target `MotionService::configure` retains only the validated stepper
profile and complete-image contract. `prime` constructs exactly one selected
runner, so configuration does not temporarily place two full motion owners on
the core-1 task stack. The runner is discarded after normal job completion,
allowing a later descriptor to select the other kind without compatibility
state.

## Open boundary, owner tail, and physical ownership

`ScheduledExecutionPlan::BlockPlanned` carries an explicit boundary kind:

- coordinated integer blocks are `Sealed`, because no successor can change
  their terminal complete image; and
- direct blocks are `ContinuationOpen`, because an immediate successor may
  contribute direction or enable changes at the same exact output cycle.

Generic direct planning deliberately reports the same open block again without
closing or staging its newest same-cycle image. If lookahead arrives, admission
closes continuation and planning proceeds through the successor at the same
logical boundary. Thus the physical image is independent of core-1 polling or
prefetch latency.

Only a descriptor-proven final block invokes `plan_owner_tail_through`. That
irreversible operation closes continuation and drains every executor-owned
pulse fall. If the next tail event is still in the future or the sparse ring is
full, `MotionService` retains a distinct final-boundary state and resumes the
tail on a later poll; it cannot admit another block or return the final token.

The common coverage calculation exposes only immutable complete-image time:
an open direct block covers through one output quantum before its completion,
a full ring covers through one quantum before the ungenerated deadline, and a
sealed block covers its completion exactly. A completed owner tail proves the
current image through the requested horizon.

After the tail is complete, normal disable is generated and accepted into the
same target timeline while the final block remains retained. The block may
return only after its own generated prefix and terminal cycle are physically
observed and the tail plus disable are already target-owned. Job completion is
separate and remains withheld until the disable token is physically committed.
A staging mismatch, wrong or reordered token, early/late commit, or planning
failure latches the retained window. The caller's fault transition invalidates
that work and requests the complete safe image before any block can be
acknowledged.

## Independent dispatch replay

The existing bit-level PCM-short integration now constructs the real direct
`JobDescriptor`, derives its mode with the production selector, and enters
through `ScheduledShiftedExecution`. It still admits two immutable direct
blocks through `RealtimeJob`, keeps the first boundary open for its successor,
explicitly selects the second block's owner tail, and independently reconstructs
all 64 modeled serial bits before committing a motion token.

The exact sparse image cycles remain:

```text
116, 128, 132, 144, 148, 156
```

They represent start/enable, first rise, cross-block fall, second rise,
terminal fall, and normal disable. The first and second block barriers remain
independent, recurrence-only updates allocate no image, and job completion
appears only after the disable latch at cycle 156.

Focused regressions additionally prove:

- repeated generic polls cannot choose a direct terminal tail;
- a delayed successor retains same-cycle composition;
- an explicit tail may return a future deadline and resume generically to
  `OwnerTailComplete`;
- ordinary blocks report sealed boundaries and reject the direct tail
  operation;
- cross-family substitution returns the unique block without live mutation;
- every plan variant reports only immutable output coverage; and
- the production-sized owner remains inside its fixed host memory bound.

## Verification record

The following completed offline against firmware commit
`4f41be89a5c8ee93f0fdc4e6901ba21e2c53d97b`:

```sh
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked --offline
git diff --check

cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked --offline -- -D warnings
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board t-deck-pro --profile release

llvm-size \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1 \
  target/xtensa-esp32s3-none-elf/release/alumina-firmware-t-deck-pro
sha256sum \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1 \
  target/xtensa-esp32s3-none-elf/release/alumina-firmware-t-deck-pro
```

Observed results:

- all 428 portable default-member tests passed, including 50 motion and 40
  simulator tests;
- formatting, warnings-denied all-target Clippy, warnings-denied rustdoc, and
  diff checks passed;
- strict target Clippy passed for classic ESP32 TinyBee and ESP32-S3 T-Deck
  Pro;
- both optimized target images linked successfully;
- the TinyBee image has 1,068,100 bytes text, 12,224 bytes data, and 249,920
  bytes BSS, with SHA-256
  `fd97e1cb1e6e61a131dd012a63c19e6bcf39f697cc24e0e8bc57f91c4d37eab7`;
  and
- the T-Deck Pro image has 1,002,533 bytes text, 12,976 bytes data, and 525,392
  bytes BSS, with SHA-256
  `fced1674beacddedf5ca371c223da51d8d08a91b39b003a887250ce96cf06845`.

The sizes are linked static sections, not runtime stack/heap watermarks, DMA
bandwidth, WCET, or physical timing evidence. Relative to the preceding
scheduled-direct checkpoint, making the direct owner reachable adds 14,976
bytes of TinyBee text and 13,460 bytes of T-Deck Pro text; data/BSS are
unchanged on both targets. This is compile/link reachability, not a claim that
either board can stream motion.

No dependency changed, so the existing MIT/Apache-compatible license inventory
is unchanged. A scan of the changed Rust found no GPL/AGPL/LGPL/SSPL identifier
or copied FluidNC, Klipper, Synthetos/g2core, or SimpleFOC implementation
reference. The configured CI license policy remains required.

## Closed claims and next boundary

This checkpoint closes permanent target-actor execution-kind selection and the
direct final-tail lifecycle. It does not implement the TinyBee PCM-short DMA
adapter, a T-Deck Pro motion backend, static-safe-to-stream transfer, qualified
physical latch observation, measured hardware lead, deadline/WCET margin under
Wi-Fi/service load, or arm authority. Both target resource adapters still
reject motion streaming and every board remains non-armable for this path.

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, driven,
or used by these checks. No WLAN association changed, no USB/serial transaction
occurred, and no analyzer, GPIO, motor, motor-power, or process-power action was
taken. The SLogic16U3 was not used. Physical work remains deferred.
