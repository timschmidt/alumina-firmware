# Distributed jobs and SD storage

## Scope

Alumina coordinates dual-core ESP32 MCUs over Wi-Fi only. USB, serial, CAN/TWAI,
and other transports may exist as ordinary machine peripherals, but they are not
part of the first distributed-control protocol. Multi-MCU jobs are compiled in
the authoritative browser/WASM UI, partitioned before upload, cached by every
participant, and started from synchronized device clocks.

The design does not pretend that Wi-Fi is an atomic safety bus. Local firmware
and physical interlocks remain responsible for safe behavior if any packet,
browser, access point, or peer disappears.

## Network lifecycle

1. A new or reset device boots as a password-protected local AP and serves its
   embedded, exactly matching interface bundle.
2. The interface scans visible WLANs through the device API and stores selected
   STA credentials transactionally. AP+STA may remain active only when the chip,
   memory, and radio qualification permit it.
3. A multi-MCU machine places every participant on the same trusted
   infrastructure WLAN or VPN-reachable LAN. The UI connects directly to each
   device with its own credential and explicit allowed Alumina origin.
4. Identity includes device ID, board/revision, boot ID, firmware/protocol exact
   version, capability digest, current network identity, and machine membership.
   Version mismatch is a hard update request, not a negotiated compatibility
   mode.

## Clock model

Each MCU exposes a free-running, stable hardware cycle counter and its declared
frequency. A heartbeat exchange records:

- UI monotonic send and receive times;
- MCU receive and transmit cycle samples captured near packet handling;
- boot ID, sequence, counter width, clock source, and current real-time state;
- queue horizon, missed deadlines, and oscillator/clock-quality flags.

The UI unwraps counters per boot and fits a robust affine model
`device_cycles = rate * ui_monotonic + offset`. It rejects high-delay/asymmetric
samples, tracks rate drift, and maintains a prediction uncertainty envelope.
Mapping is continuously measured, never inferred solely from nominal CPU MHz or
wall-clock time. Browser workers own acquisition and fitting so rendering stalls
do not corrupt the model, but background throttling is still treated as clock
quality loss.

A command or job start is schedulable only if its lead time exceeds measured
network/validation margin and its required synchronization tolerance exceeds the
current mapped-clock uncertainty for every participant.

## Immutable job format

A global job manifest contains:

- job/schema/compiler version and global content digest;
- exact source/CAM, interface build, policy, and global machine digests;
- units, coordinate epochs, synchronization markers, duration, and participants;
- required capability/configuration/board digests for every MCU;
- one per-MCU partition digest, byte length, resource set, local time span,
  initial/final state, and error/safety envelope; and
- global rules for partial failure, hold/cancel, completion, and audit evidence.

Each partition contains only work for that MCU: integer/fixed-point trajectory
segments, local resource events, synchronization points, bounded conditions, and
expected terminal state. It does not contain CAD, G-code, arbitrary graph files,
or another MCU's executable work.

## SD cache service

Core 0 owns an explicit raw SD cache region and exposes authenticated APIs to:

- query capacity/health and list manifests/blobs;
- create a resumable upload session;
- write fixed-size content-addressed chunks with per-chunk checksums;
- atomically publish a manifest only after full digest and schema validation;
- read/export a cached package for audit; and
- delete unreferenced jobs while disarmed and idle.

Protocol V1 uses SHA-256 content identities and a canonical `ACMF` V1 manifest
hash stream. The manifest commits the object kind, complete object digest and
length, fixed chunk size/count, and every ordered `(index, length, digest)`
entry. Chunks arrive sequentially; a retry resumes at the first index not present
in the durable checkpoint. The last chunk alone may be shorter. This sacrifices
out-of-order upload in exchange for bounded MCU RAM, a constant-size coordinator,
and a journal that can be reconstructed by a linear SD scan.

The V1 cache is not FAT or another general filesystem. Two fixed SHA-256 anchors
alternate generations over an append-only, hash-chained sequence of begin,
chunk, abort, and publication records. Each record writes padded data, crosses a
device synchronization barrier, writes a bound commit sector, crosses another
barrier, then replaces the older anchor and synchronizes again. Mount chooses a
structurally valid generation and replays its exact committed tail. Formatting
is an explicit destructive provisioning operation and never occurs implicitly
at boot. Upload, compaction, repair, and other media mutation are forbidden while
armed. Raw source files may be cached by the browser elsewhere; firmware storage
accepts only machine-job packages and explicitly typed opaque user blobs that
cannot be executed. A separate non-authoritative exchange filesystem may be
added later if it remains useful and passes dependency/license review.

The portable transaction model separates verification from durability. Chunk
identity and aggregate object/manifest hashes are checked before a publication
record is admitted; only a synchronized replacement anchor advances externally
visible progress. A reset at any boundary therefore leaves the old committed
tail or the complete new tail, never a partially visible runnable object.

Core 1 never reads SD. Before and during a run, core 0 verifies blocks and fills
fixed internal-SRAM buffers through a credit-based boundary. Core 1 validates
sequence, digest chain, time range, and configuration identity again when taking
ownership. Low-water telemetry requests prefetch; an unrecoverable starvation
condition performs the job's certified constrained stop or faults before an
unsafe underrun. SD and Wi-Fi load are qualified together on TinyBee.

The implemented storage cursor opens only an exact typed object plus manifest,
replays the full committed log without allocation, and emits at most 1,024
verified bytes per call. It permits later append-only commits but rejects media
identity, region, generation, tail, record-chain, or aggregate divergence. This
is deliberately below the execution boundary: storage chunks may split any
machine-IR field and must be incrementally decoded on core 0 into separately
owned, fixed execution envelopes before a credit is transferred to core 1.

The firmware now implements that pre-commit boundary behind the authenticated
`POST /api/v1/control` route. `JobPrepare` opens an exact published partition and
installs matching service/core-1 actors; the service actor reads at most one
verified chunk per pass, and the real-time actor independently validates and
retains one owned block. `JobStatus` reports both domains, queue credits/depth,
and exact tick/digest progress; `JobCancel` invalidates the preparation and
drains queued ownership. The admitted first block is intentionally left
outstanding. No acknowledgement, epoch conversion, scheduler, or output path is
present yet. The two first board packages also retain zero capability identities,
so target preparation remains fail-closed until canonical capabilities and the
active configuration can both be verified.

## Deterministic prepare/commit start

The UI orchestrates a bounded two-phase procedure:

1. **Upload:** every MCU stores and verifies its partition and global manifest.
2. **Prepare:** the UI sends the global job/epoch, local partition digest, desired
   start window, and local configuration digest. Each MCU preflights storage,
   buffers, resources, safety inputs, and clock quality, then returns a
   nonce-bound prepared token and latest permissible commit time.
3. **Choose time:** after all participants are prepared, the UI chooses one
   future UI-time epoch with adequate guard margin and maps it to each MCU's
   integer start cycle. Quantization error is added to the sync certificate.
4. **Commit:** each MCU receives the complete participant set, global digest,
   prepared tokens, its local start cycle, and a finite arm lease. It acknowledges
   that the start compare is installed but remains abortable until a declared
   guard boundary.
5. **Confirm or abort:** the UI confirms every acknowledgement before that
   boundary. Any missing participant causes repeated authenticated/idempotent aborts to
   all participants; an unconfirmed MCU self-aborts when its lease expires.
6. **Execute:** after the boundary, each committed MCU starts from its local
   hardware clock without another network packet. Telemetry later reconciles the
   observed start edges and sync error.

This produces deterministic scheduled starts within a measured tolerance, not a
mathematically atomic distributed transaction. The initial qualification uses
harmless GPIO pulses and capture equipment. A machine whose safety depends on
all MCUs stopping simultaneously needs a hardwired, appropriately rated safety
chain; Wi-Fi stop/cancel is supplementary.

## Operation after network loss

A manifest declares either `network_attended` or `cached_autonomous` policy.

- An attended job holds or safely stops when its communication lease expires.
- A cached autonomous job may finish without the browser only when all resource
  actions and maximum energization durations are locally bounded, storage is
  complete, safety inputs are local/hardwired, and no cross-MCU live feedback is
  required.
- UI hold/cancel remains idempotent and best effort. Physical E-stop, limit,
  driver fault, overcurrent, and other local safety inputs bypass Wi-Fi queues.
- A reboot changes `boot_id`, invalidates the prepared epoch, and never resumes a
  hazardous job implicitly.

Initial cached multi-MCU support excludes live cross-MCU feedback loops and
runtime repartitioning. Those require a separate control/stability and failure
analysis rather than a larger message type.

## Acceptance evidence

- Repeated simulated and HIL starts across at least two MCUs meet a declared
  edge-to-edge synchronization bound under normal and saturated Wi-Fi load.
- Delay spikes, drift, browser throttling, AP restart, lost/duplicated/reordered
  prepare/commit/abort messages, MCU reboot, and one full/corrupt SD card either
  prevent start or produce the documented safe local behavior.
- Power loss at every upload/publish transition leaves either the prior valid
  cache or an unreferenced recoverable partial upload, never a runnable corrupt
  job.
- Long jobs demonstrate bounded internal SRAM, correct block handoff, no direct
  SD access from core 1, and safe response to storage starvation.
- Job records correlate global time, each clock fit/uncertainty, scheduled and
  observed start cycles, firmware/config/partition digests, and fault history.
