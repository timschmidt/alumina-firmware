# aluminafw

`aluminafw` is the greenfield Embassy firmware platform planned for Alumina
machines, instruments, controllers, and embedded operator interfaces. It combines
the async driver structure of `t-deck-async-drivers-rs` with the useful embedded
web-serving behavior demonstrated by `alumina-firmware`, while putting all
network and UI work on one ESP32 core and all real-time work on the other.

Implementation is underway from the researched delivery plan. The portable
crates now define exact protocol identities, integer machine-job validation,
board-resource ownership, a fail-closed safety state machine, and bounded
cross-core channels. TinyBee and T-Deck Pro have chip-specific composition roots
that consume the HAL peripheral singleton once, partition owned tokens, and run
one Embassy executor on each application core. Their exported packages now also
carry clocks, electrical constraints, interrupts, safe images, fitted-device
auxiliaries, licensed-photo overlays, and explicit HIL promotion gates. There are
no deployed clients and no compatibility requirement: the old Alumina firmware
and interface are functional references, not APIs to preserve.

The coordinated interface now extends its greenfield I0 baseline through exact
CAM, immutable per-MCU cache packaging, and canonical global-job construction.
Exact and measured source values, canonical
firmware values, and lossy display values remain separate Rust domains.
Hypergraphics renders certified Hypercurve paths and role-preserving regions;
a disjoint compiler certifies motion chords, exact lengths, machine-step
rounding, and timer rounding into the real `alumina-machine-ir` schema. It then
replays canonical blocks, packages real content-addressed storage objects, and
binds owned participant artifacts into the shared `alumina-job` manifest.
The machine-bound line/arc/certified-cubic path now derives exact dynamics and a
complete position-error budget from canonical Configuration V6, runs exact
forward/reverse node planning, and certifies jerk schedules through
Hyperpath/Hypersolve. Hyperpath refines each stop-separated positive component
by exact uniform halving until every touching span has a replayed monotonic
jerk transition or a caller-owned bound rejects it. Alumina permits a positive
join only between lossless exact source lines which Hyperpath independently
classifies G1. Curvature-bearing joins, true corners, reversals, and every
approximated cubic chord boundary remain exact stops. Phase selection consumes
those planned boundary feeds: zero/zero spans retain symmetric rest-to-rest
motion and eligible positive spans reuse Hyperpath's certified two-phase
transition. On an all-line route, Hyperpath now projects exact affine
velocity, acceleration, and jerk constraints across any dense axis count and
independently replays every span/axis row and selected bottleneck through
Hypersolve. The current Cartesian compiler derives exact unit-direction rows
and retains the report; a route containing any curve deliberately keeps the
older conservative direction-independent limits. The schedule lowers to the
current constant-velocity IR under a proved interpolation bound. Each exact
ideal interval is ceiled to the configured output quantum, and a bounded exact
rational search selects the smallest local factor whose complete stream passes
the unchanged production preflight. Factor one and the immediate predecessor
remain retained failures when headroom is required; structural failures never
enter retiming.
Before cache release it replays the allocation-free production stepper
electrical contract; the final partition then passes an event-level
`RealtimeJob`/`CachedStepperExecutor` simulation and canonical evidence replay.
Canonical `ALMEVD03` now commits independent exact source, metric,
source-approximation, planner-policy/certification, and complete lowering/timer
subtranscripts. Exact planner/lowering structures are rebuilt from live state;
changing caller policy changes evidence even when canonical machine bytes do
not. Firmware does not parse this browser audit transcript and core 1 continues
to consume only bounded independently admitted machine IR.
The next browser boundary now also selects one exact factor across a complete
same-grid MCU set before any immutable participant cache object exists. Every
candidate is replayed for every MCU; selected streams are checked against their
retained exact point carriers, independently partitioned, and committed by
compact `ALMSYN01` over a streamed `ALMSRT01` transcript. That evidence digest
becomes both the global synchronization identity and each participant's
timing/error evidence in `ALMJMF02`. Firmware still parses none of these
Hyper/browser audit formats.
Native and WASM tests, strict lint, sibling-source and license policy, and the
compressed production bundle pass. This remains a development checkpoint, not
a qualified compiler release. See the [interface baseline
evidence](docs/evidence/M5-INTERFACE-EXACT-BASELINE.md) and [exact-CAM compiler
evidence](docs/evidence/M5-EXACT-CAM-COMPILER.md), plus the [global-job packaging
evidence](docs/evidence/M7-GLOBAL-JOB-MANIFEST.md) and [exact scheduling
evidence](docs/evidence/M10-EXACT-SCHEDULE-PREFLIGHT.md), through the [exact
monotonic-jerk evidence](docs/evidence/M10-EXACT-MONOTONIC-JERK.md) and [exact
jerk-feasible G1 evidence](docs/evidence/M10-EXACT-JERK-FEASIBLE-G1.md), followed
by the [exact affine-axis projection
evidence](docs/evidence/M10-EXACT-AFFINE-AXIS-PROJECTION.md) and [exact
timer-lattice evidence](docs/evidence/M10-EXACT-TIMER-LATTICE-HEADROOM.md),
followed by the [canonical planner/lowering V3
evidence](docs/evidence/M10-CANONICAL-PLANNER-EVIDENCE-V3.md) and [shared-MCU
timer-retiming evidence](docs/evidence/M10-SHARED-MCU-TIMER-RETIMING.md).

The first I4 diagnostic boundary is portable and remains physically offline.
`alumina-diagnostics` defines allocation-free bounded canonical resource
overviews and triggered digital edge captures with complete device/boot/
capability/config/clock identity, provenance, quality, sample cycles, trigger,
pre/post windows, buffer capacity, decimation, and loss flags. A deterministic
TinyBee simulator emits four input values and a four-lane edge trace; the
browser independently reconciles both records to the complete board capability
and cross-links its ledger, selected resource, and exact-cycle cursor. It grants
no connection, measurement, diagnostic lease, command, or output authority.
See the [diagnostic contract](docs/DIAGNOSTICS.md) and [offline diagnostic
evidence](docs/evidence/M9-OFFLINE-DIAGNOSTIC-EXPLORER.md).

Authenticated V1 telemetry/capture transport now binds those records to exact
device/boot/capability/configuration context and SHA-256 request/record identity.
A fixed-memory core-0 owner implements idempotent lifecycle, latest-only loss
accounting, retained capture, 168-byte native ranges, and retry reconciliation;
the typed interface client passes both in-memory and real localhost HTTP/HMAC
flows. TinyBee and T-Deck Pro compile the dispatcher but honestly return
`Unsupported` until a physical provider is connected. See the [transport
contract](docs/DIAGNOSTIC-TRANSPORT.md) and [offline authenticated transport
evidence](docs/evidence/M9-AUTHENTICATED-DIAGNOSTIC-TRANSPORT.md).

The first deployed graphical-control path is also portable end to end. The
interface lowers one audited Boolean Service-to-Realtime graph into a fixed
4 KiB package; firmware independently decodes exact identities and arena
requirements, primes source tick zero, splits compile-time storage between core
owners, and executes only four whitelisted opcodes through bounded queues and a
first-cause fault latch. V2's first physical opcode reads only a board-published
fresh debounced safety-input semantic value; it grants no raw GPIO or output
authority. The package can now be uploaded as an immutable SD object, installed
through authenticated Wi-Fi, independently
rehash-validated on core 0 and core 1, selected, and exposed only after an exact
dual-core authorization handshake. Permanent core-local actors now accept an
authenticated future run epoch, preserve source-first tick-zero priming,
release from their pinned Embassy tasks, retain the first cross-core fault, and
reconcile exact stop. Active selection is now protected by a power-cut-tested
two-phase media journal and is independently revalidated on both cores after
configuration-first boot recovery. Production lowering derives exact split
arenas and opcode/resource palettes from the authenticated target capability
document, and both firmware cores independently enforce the selected board's
same static palette. No target timing, physical input HIL, or physical side
effect is claimed. The native/WASM application now also opens a bounded,
editable view of the shared exact PID/interlock fixture with audited semantic
layers, explicit state feedback, typed ports, exact parameters, and exact-cursor
plots. Canonical `ALGW` keeps presentation-only integer placement separate from
the embedded `ALGR`; its 11-entry fixed-schema palette supports monotonic node
creation, atomic node/incident-wire deletion, node moves, typed wire edits, and
bounded exact scalar parameter replacement. Bounded canonical snapshots now
provide undo/redo, origin-local browser storage preserves only the current
document, and native/browser `.algw` exchange requires full replay and audited
draft admission. All edits are transactional, and graph changes detach the old
graph-bound trace. It grants no deployment or output authority. See
the [fixed graph-IR boundary](docs/GRAPH-IR.md), [deployment
evidence](docs/evidence/M9-AUTHENTICATED-GRAPH-DEPLOYMENT.md), and [split-core
execution evidence](docs/evidence/M9-SPLIT-CORE-GRAPH-EXECUTION.md), plus the
[durable-selection evidence](docs/evidence/M9-DURABLE-GRAPH-SELECTION.md) and
[capability-bound input evidence](docs/evidence/M9-CAPABILITY-BOUND-GRAPH-INPUT.md),
and the [exact-control inspector
evidence](docs/evidence/M9-EXACT-CONTROL-INSPECTOR.md) plus the [canonical graph
workspace evidence](docs/evidence/M9-CANONICAL-GRAPH-WORKSPACE.md) and [graph
palette/parameter evidence](docs/evidence/M9-GRAPH-PALETTE-PARAMETERS.md), plus
the [graph history/persistence
evidence](docs/evidence/M9-GRAPH-WORKSPACE-HISTORY-PERSISTENCE.md).

The first M3 foundation adds an explicit little-endian native protocol, bounded
storage operation bodies, SHA-256 content-addressed sequential uploads, atomic
publication checkpoints, and a deterministic reboot/cache/prefetch simulator.
The durable coordinator remains encapsulated by a core-0-owned backend; core 1
has no media handle. See the
[protocol/storage simulation evidence](docs/evidence/M3-PROTOCOL-STORAGE-SIM.md).

The first live-network foundation now initializes `esp-radio` on core 0 before
the real-time core starts, runs a protected WPA2 device AP, a fixed four-lease
DHCP service, and a two-connection bounded HTTP bootstrap on that same service
executor. Only exact greenfield read-only routes are admitted. A repository
development password exists for bench discovery, is reported as non-production,
and can never make an image production-armable. See the
[Wi-Fi/web compile evidence](docs/evidence/M3-WIFI-WEB-FOUNDATION.md).

Authenticated storage admission is also wired end to end: the public auth route
returns a fresh 128-bit boot challenge; browser/WASM requests and device
responses carry exact HMAC-SHA-256 proofs bound to a nonzero counter, method,
path, calling browser origin, status/media, and SHA-256 body identity. The V2
browser boundary handles exact route-scoped CORS/private-network preflights,
echoes only the observed canonical origin (never `*`), and exposes response
proofs to the calling origin. Strict raw-header policy rejects duplicates,
transfer coding, noncanonical lengths, wrong media, origin substitution,
replays, and a global valid-request flood before the core-0 service owner
decodes a native frame. That admission milestone deliberately used an
unavailable backend, so it could not acknowledge volatile bytes as durable. See
the historical
[authenticated-service evidence](docs/evidence/M3-AUTHENTICATED-SERVICE.md).

The cache backend itself is now concrete: `alumina-storage` implements a bounded
asynchronous 512-byte block-device contract, an explicitly provisioned raw SD
region, alternating SHA-256 anchors, and a hash-chained append-only record log.
Begin, chunk, abort, and publication are acknowledged only after ordered sync
barriers; reboot replay reconstructs exact upload state without trusting a
filesystem. The authenticated service dispatch is generic over that backend,
and `alumina-sim` exposes snapshots, torn writes, power cuts, and corruption for
the same implementation. That milestone kept board firmware explicitly
`unavailable` until a physical SD-SPI adapter could exist. See the historical
[durable-cache evidence](docs/evidence/M3-DURABLE-CACHE-MEDIA.md).

The first physical transport is now composed on both boards. A clean-room,
allocation-free SD SPI driver enters identification at 400 kHz, negotiates a
modern SD V2 card, enables command/data CRC, derives capacity from CSD, and
provides bounded single-block read/write/sync at 10 MHz. TinyBee owns SPI2 on
GPIO 18/23/19 with CS 5; T-Deck Pro owns GPIO 36/33/47 with CS 48 and keeps the
shared EPD/LoRa devices inactive. Boot identifies a card on core 0 and reports
`detached`, or `faulted` on failure. See the historical
[SD SPI transport evidence](docs/evidence/M3-SD-SPI-TRANSPORT.md).

Cache placement is now explicit and persistent. Boot reads only fixed raw blocks
2046–2047; foreign bytes mean unprovisioned, recognizable damaged Alumina bytes
mean faulted, and a valid hashed locator selects one exact region whose anchor
identity and complete log must replay. `StorageProvision` binds destructive
formatting to the authenticated request's observed card size, old generation and
media ID, exact new interval, fresh ID, and recovery intent. A 1 MiB front guard
keeps the locators outside conventional primary partition metadata and their
fixed placement avoids GPT's card-tail backup sectors. Core 1 now establishes a
board-specific hazardous-output contract before Wi-Fi: T-Deck Pro retains GPIO2
as high impedance, while TinyBee first latches a complete 24-bit all-safe
shift-register image and retains GPIO2 plus its four digital safety-input routes
as high impedance. Core 0 admits
storage mutation only while 100 ms safety publications remain identity-matched
and no more than 500 ms old. This makes explicit provisioning reachable in the
compiled images without making a bench, armability, or implicit-format claim.
See the [safe-boot evidence](docs/evidence/M3-SAFE-BOOT-OBSERVATION.md) and
[cache-provisioning evidence](docs/evidence/M3-CACHE-PROVISIONING.md).

Published cache objects can now be reopened without a heap-backed directory.
The core-0 reader linearly revalidates the committed log, binds an opaque cursor
to one exact typed object and manifest, verifies every sequential chunk before
copying it into fixed caller memory, and withholds the last chunk until the
aggregate object, manifest, and publication record all agree. Lookup misses are
benign; media divergence or post-open corruption latches an integrity fault.
This is storage readback, not executable job admission; the later machine-block
and job-prefetch layers add those independent gates. See the
[published-object reader evidence](docs/evidence/M3-PUBLISHED-OBJECT-READER.md).

The next boundary is also concrete: `alumina-machine-ir` now defines canonical
512-byte motion blocks for up to eight axes, with exact relative-tick intervals,
integer lattice displacement, repeated stream/capability/configuration
identities, zero padding, SHA-256 block identity, and a previous-block digest
chain. An incremental assembler handles arbitrary storage chunk splits. Core 0
and core 1 use separate stateful validators, and a dedicated inline work ring
transfers non-cloneable owned blocks under exact credits. See the
[machine-block boundary evidence](docs/evidence/M3-MACHINE-BLOCK-BOUNDARY.md).

`alumina-job` now owns the portable prepare/prefetch/admission lifecycle. Core 0
opens one exact typed publication, reads at most one verified SD chunk per
bounded step, retains ownership across a full work ring, and never marks a
partition complete until every block has transferred. Core 1 independently
validates a strict two-block ownership window and advances completion only
through ordered exact acknowledgement tokens. Cached `StreamTick` values remain
distinct from absolute `DeviceCycle` values until a future deterministic commit
installs an epoch. The simulator drives these actors through a provisioned
cache with 700-byte chunks, forces backpressure, and proves identical terminal
stream facts. Firmware now compiles the crate for both boards; authenticated job
control now routes canonical prepare/cancel/status frames through core 0, drives
bounded verified prefetch, installs the independent core-1 validator, and
publishes correlated status. Core 1 retains one initial block, or two for a
multi-block job, without acknowledging or executing them. See the
[cross-block prefill evidence](docs/evidence/M6-CROSS-BLOCK-PREFILL.md).
`alumina-capability` now encodes every typed board
fact as an allocation-free canonical `ALMCAP02` document. Both first packages
compile its independently recomputed SHA-256, firmware verifies it before Wi-Fi,
identity advertises it, and authenticated `CapabilitiesGet` returns bounded
digest-stable ranges. Preparation still returns `Unsupported` on both current
images because neither board package is armable; configuration commit and
hardware qualification remain separate closed gates. A complete bounded
consumer decoder now exposes borrowed resource, alias, visual and hotspot views
without allocation; the interface uses it for a searchable TinyBee ledger that
keeps all descriptive facts separate from the four explicitly graph-readable
inputs and draws no physical overlay while the package has no licensed photo.
See the
[portable lifecycle evidence](docs/evidence/M3-JOB-PREFETCH-LIFECYCLE.md) and
[firmware wiring evidence](docs/evidence/M3-FIRMWARE-JOB-PREFETCH.md), plus the
[capability format](docs/CAPABILITIES.md) and
[canonical-capability evidence](docs/evidence/M3-CANONICAL-CAPABILITIES.md), plus
the [board-explorer evidence](docs/evidence/M9-BOARD-CAPABILITY-EXPLORER.md).

The same shared job crate now defines the canonical global multi-MCU manifest:
a fixed header plus strictly sorted, fixed participant records binding exact
source/compiler/policy/machine identities to every cached partition, resource
set, evidence envelope, timer span, and terminal lattice state. Allocation-free
decode recomputes the participant-set digest and proves local/global rational
durations equal without floats; those identities feed the existing deterministic
schedule commit directly. The interface now constructs that exact shared object
from owned, independently replayed local cache artifacts and can give every MCU
an independent resumable upload transaction without changing content identity.
For exact schedule-derived jobs, the interface no longer accepts caller-owned
duration or synchronization placeholders: it derives the common timer,
terminal tick, and `ALMSYN01` identity after jointly minimal shared retiming and
partition replay, then emits the manifest. V1 deliberately requires one exact
ideal event grid, local timer frequency, and output quantum; mixed grids fail
closed pending explicit synchronization-marker/idle semantics.
The headless browser client now authenticates the exact origin-bound native
route, reconciles retry-safe cache uploads, and orders every executable
partition before the shared manifest. It also acquires conservatively widened
boot-scoped clock samples from window or worker contexts and coordinates exact
prepare/install/confirm-or-abort transitions without confirming before all
installs. The browser now creates a dedicated control worker that owns
independent authenticated sessions, exact causal clock models, bounded history,
and retry-safe redacted diagnostic panels; its module lifecycle reaches a
rendered worker-ready state in Chromium. That production worker now exchanges
real authenticated browser HTTP/CORS heartbeat traffic with a deterministic
host MCU fixture, recovers from response loss, a finite outage, and reboot, and
refuses an excessive causal interval. Physical radio and timing qualification
remain closed. The same coordinator now consumes canonical first-output
observations, maps their exact device-cycle bounds back through the boot-scoped
affine clock envelopes, preserves whether their authority is a simulator,
peripheral latch, or software bracket, and displays conservative cross-device
spread and target error. Missing, foreign, overwide, erased, or replaced
evidence fails closed; the two-device simulator proves each known edge remains
inside its reconstructed interval. See the
[global-job packaging evidence](docs/evidence/M7-GLOBAL-JOB-MANIFEST.md) and
[browser cache-delivery evidence](docs/evidence/M7-BROWSER-CACHE-DELIVERY.md),
[browser clock/coordinator evidence](docs/evidence/M7-BROWSER-CLOCK-COORDINATOR.md)
and [authenticated browser/HTTP evidence](docs/evidence/M7-BROWSER-AUTH-HTTP-SIM.md),
and the
[observed-start replay evidence](docs/evidence/M7-OBSERVED-START-REPLAY.md).

`alumina-config` defines canonical content-addressed resource bindings and
reduced exact machine facts, validates them against the immutable board package,
and streams a real published SD object through independent validators on both
cores. Authenticated firmware operations now order durable prepare, unauthorized
core-1 activation, durable commit, and exact authorization; boot recovery
revalidates committed bytes and discards orphan prepares. Only the resulting
durably authorized digest reaches either job actor. See the
[configuration format](docs/CONFIGURATION.md),
[portable configuration evidence](docs/evidence/M4-CONFIGURATION-IR.md), and
[firmware lifecycle evidence](docs/evidence/M4-FIRMWARE-CONFIGURATION-LIFECYCLE.md).

The first portable M6 execution slice now binds cached ownership to an exact
integer step-event trace: whole blocks are preflighted without work proportional
to step count, retained until terminal tick/position correlation, and never
acknowledged after a fault. Configuration-derived safety inputs have stable
core-1 slots with polarity, pull, exact assert/release debounce, finite sampling
watchdogs, arming masks, and typed E-stop/interlock/limit/driver/probe reactions.
TinyBee now realizes GPIO 33/32/22/35 as a transactional nominal one-millisecond
core-1 polling bank; every fault first reapplies the full board-safe image and
invalidates admitted job ownership. The exact input masks and next watchdog
deadline cross to core 0 in the canonical safety snapshot. Core 1 also binds
the exact scheduled epoch and descriptor starting lattice position to cached
motion, and releases a block only after every complete output image has a
target-confirmed physical commit. TinyBee's blocking writer is an unqualified
compile/HIL staging path; T-Deck Pro has no motion backend. This is target wiring,
not measured GPIO, electrical, response-time, or motion-output
qualification, so both packages remain non-armable. See the
[stepper evidence](docs/evidence/M6-EXACT-STEPPER-CORE.md) and
[portable safety-input evidence](docs/evidence/M6-SAFETY-INPUT-CORE.md), plus the
[target safety-input evidence](docs/evidence/M6-TARGET-SAFETY-INPUTS.md) and
[target motion-commit evidence](docs/evidence/M6-TARGET-MOTION-COMMIT.md). The
greenfield machine boundary now also has canonical `ALMBLK03` direct
third-order finite-difference records and `ALMJOBD4` kind-bound preparation.
Signed Q31.32 Newton-forward state remains exact across records and blocks,
nearest-integer ties-to-even is the only step projection, and sparse electrical
admission is logarithmic in each record's update count. The allocation-free
core consumes every dense update, emits the same logical step/direction/enable
transactions as ordinary motion, and retains the cached block token until both
integer and Q31.32 terminal state agree. A deterministic immutable-partition
simulation covers that full portable path. The authoritative browser/WASM
compiler now lowers exact stop-to-stop affine Hyperpath schedules through
grid-retimed jerk recertification, interval-certified coefficient projection,
explicit propagated error, direct cache packaging, and replayable `ALMDFE01`
evidence. Firmware-owned pulse falls may cross contiguous direct records and
cached blocks without a false dwell. The direct recurrence now feeds a bounded
scheduled complete-image owner that composes same-cycle changes, retains an
unstageable continuation boundary, drains terminal falls, and passes a dense
PCM-short/bit-level latch simulation. The permanent core-1 actor now selects
the ordinary or direct owner exactly once from the validated job descriptor,
keeps direct nonfinal boundaries open for delayed lookahead, and explicitly
owns the final direct tail before scheduling normal disable. Cross-family
blocks cannot mutate the selected executor, and the final token cannot return
ahead of its physical output prefix. Curved/positive-feed lowering and the
TinyBee peripheral adapter remain open; both target adapters still reject
motion streaming, and no hardware qualification is claimed. See
the [direct finite-difference IR
evidence](docs/evidence/M10-DIRECT-FINITE-DIFFERENCE-IR.md) and [browser direct
lowering evidence](docs/evidence/M10-BROWSER-DIRECT-FINITE-DIFFERENCE.md), plus
the [scheduled direct PCM
evidence](docs/evidence/M10-SCHEDULED-DIRECT-PCM.md) and [target direct-dispatch
evidence](docs/evidence/M10-TARGET-DIRECT-DISPATCH.md).

Machine-block V3 also introduces a separate servo recurrence family: up to four
axes of Q31.32 absolute position plus Q2.30 normalized velocity and
quadrature-current feed-forward, all on one exact configuration-derived
position-loop cadence. Browser/WASM projection uses certified Hyperreal dyadic
intervals, exact ties-to-even quantization, discrete-extremum splitting, forced
encoded continuity, and explicit approximation evidence before packaging
content-addressed blocks and an `ALMJMF02` global job. Firmware independently
checks those blocks using limits rebuilt from complete FOC axis profiles. Its
allocation-free two-block runner retains half-open recurrence ownership,
prepares every simultaneous setpoint transactionally, and appends exactly one
terminal at-rest hold. The permanent core-1 selector now admits kind `3` only
through independently retained configuration-derived profiles, primes one exact
future setpoint batch, and preserves block tokens through the distributed
start, cancellation/fault, continuation, and terminal-hold barriers. A portable
FOC bank now prepares all axes, validates every modeled physical compare commit,
and installs the complete controller array only after the whole simultaneous
transaction succeeds. The virtual two-axis mailbox drives that bank through a
401-current-period cached-job replay; a late second-axis latch, missing
second-axis encoder sample, or ordered safe invalidation cannot advance the
first axis alone. Every target mailbox still returns unavailable, and all servo
implementation, qualification, commit-latency, and prime-lead gates remain
closed, so no peripheral, energization, or hardware claim follows. See the
[servo finite-difference contract](docs/SERVO-FINITE-DIFFERENCE.md),
[exact cached-servo evidence](docs/evidence/M10-EXACT-CACHED-SERVO-STREAM.md),
and [permanent servo-lifecycle evidence](docs/evidence/M10-PERMANENT-SERVO-LIFECYCLE.md).

The portable scheduled backend further separates future generation, immutable
timeline acceptance, and physical latch observation on an exact output lattice;
its motion-to-PCM-to-wire simulator retains ownership through output-free dwell
and terminal disable. See the
[scheduled-output evidence](docs/evidence/M6-SCHEDULED-OUTPUT-HORIZON.md).
Firmware now adopts that owner structurally: commit/reference wire version 2
requires a board-qualified `Priming → Primed` output-horizon acknowledgement
before local start, while schedule-report version 3 retains the first correlated
output edge and refuses completion without it. TinyBee's safe-prefilled
PCM-short HAL composition is compile-only; both board adapters still reject
streaming, so armability and all physical edge claims remain closed. See the
[prestart priming evidence](docs/evidence/M7-PRESTART-HARDWARE-PRIMING.md).
The next portable slice models exact circular-DMA slot release/refill ownership,
requires an accepted dense frame before extending the sealed horizon, and keeps
descriptor progress distinct from physical latch authority. Final disable is
owned before the final block can return, including the case where its earliest
legal cycle follows the last step image. A four-slot circular-ring simulation
exercises release, refill, serial reconstruction, and terminal disable without
claiming TinyBee phase or timing. See the
[circular-DMA evidence](docs/evidence/M6-CIRCULAR-DMA-HORIZON.md).
An isolated TinyBee safe-image HIL artifact now makes the first physical capture
repeatable without enabling Wi-Fi, storage, motion, process commands, or the
second core. Its repository command builds but never flashes, and its successful
refill result is explicitly not a waveform verdict. See the
[HIL procedure](docs/HIL.md) and
[harness evidence](docs/evidence/M6-TINYBEE-PCM-SHORT-SAFE-HARNESS.md).
A follow-on disconnected-load contract adds an analyzer-only phase/result
marker, exact SLogic16U3 probe/photo requirements, bounded VCD reconstruction,
and a digest-bound run-record replay without flashing or qualifying hardware.
See the
[capture-contract evidence](docs/evidence/M6-TINYBEE-SLOGIC-CAPTURE-CONTRACT.md).
The portable DMA owner now also records the complete static-safe-to-stream
lifecycle. An identical safe ring and successful HAL start permit safe-only
refill; motion images remain forbidden until an independent first safe latch
establishes the exact grid. Stop destroys all tags, software safe rewrite is
not confused with physical reclaim, and first cause survives recovery. The
bit-level simulator passes through that gate, while the HIL-only TinyBee image
deliberately stops at an unobserved safe rewrite. Production still uses the
static writer and rejects streaming. See the
[static/stream handoff evidence](docs/evidence/M10-STATIC-SAFE-STREAM-HANDOFF.md).
The capture contract now also hashes a plain RTT log and parses one stable
numeric `HIL_PCM_ATTEST_V2` record independently of human debug rendering. A
schema-v2 pass recomputes the dense horizon, requires ordered start/stop/rewrite
windows and the unfaulted software-only lifecycle state, and correlates its
marker code with the VCD decoder. Missing attestation remains valid only for
non-pass evidence such as a failed start. See the
[software-attestation evidence](docs/evidence/M10-PCM-SOFTWARE-ATTESTATION.md).
The live refill path now uses one portable fixed-memory batch transaction per
target availability sample. The caller supplies a maximum frame budget, the
ring shape limits every call to at most its compile-time frame count, and each
frame crosses preview, exact target acceptance, and model acceptance in order.
The result reports exact partial progress, retained credit, and sealed horizon;
an uncertain target write invalidates the owner before recovery. The TinyBee
HIL target passes its remaining 50,000-frame budget through this transaction,
so the exact endpoint no longer depends on a target-specific inner refill loop.
This transaction alone is not an interrupt actor, deadline/WCET result, or
physical qualification.
See the [bounded-refill evidence](docs/evidence/M10-BOUNDED-DMA-REFILL.md).
An allocation-free refill supervisor now turns that transaction into a
core-local scheduling contract. A fixed policy binds per-turn frames,
worst-case target-push cycles, and required completion lead to one exact frame
grid and ring shape. Every target push returns a bracketed device-cycle window;
late/reversed/overlong/rejected calls invalidate stream ownership. Successful
turns require immediate reservice while credit remains, otherwise they name an
absolute interrupt-or-fallback wake. A missing release at the fallback faults
before the modeled frame-start window is lost. Independent bit-level simulation
proves both a continuous release/wake/refill stream and delayed-wake rejection
before wire starvation. No TinyBee adapter selects this supervisor: interrupt
source, prefetch lead, push WCET, and physical phase remain unqualified. See the
[refill-supervisor evidence](docs/evidence/M10-REFILL-WAKE-SUPERVISOR.md).

The first M8 portable FOC slice is also implemented without creating a hardware
drive path. A new no-std crate carries exact Q2.30 points and outward intervals,
wide-intermediate Clarke/Park transforms, certified rotation inputs,
non-clipping min/max modulation, anti-windup dq-current PI control, immutable
parameter snapshots whose complete PI output rectangle is voltage-circle
bounded, digest-bound scheduled commands, and allocation-free
sensor/current/power-stage traits. A deliberately dimensionless simulator proves
deterministic controller replay, convergence, visible saturation, and fail-closed
parameter/vector rejection. Energizing PWM/ADC adapters, motor identification,
deadlines, shutdown measurement, and every energization claim remain open. See
the [portable FOC evidence](docs/evidence/M8-PORTABLE-FOC-FOUNDATION.md).

A separate portable M8 outer-loop slice now implements the first clean-room
cascaded servo contract. Configuration-defined position uses an exact signed
Q31.32 lattice with conservative observation intervals, while velocity and
current retain the existing Q2.30 domain. An integer device-cycle grid derives
current, velocity, and position boundaries from the immutable FOC dividers; no
fractional period is rounded. Scheduled position setpoints feed a proportional
position loop with bounded velocity feed-forward, and the held velocity target
feeds an anti-windup PI q-current loop. Digest, cycle, contiguous identity,
fresh-sample, overspeed, following-error, and current-circle substitutions latch
transactionally before controller state advances. A dimensionless ideal-current
mechanical fixture replays 12,000 current ticks identically and converges while
preserving the exact 240 position and 1,200 velocity updates. Configuration V6
stores the complete cascade and exact loop-grid policy and lowers both under the
full configuration digest. No firmware task, target adapter, deadline proof,
or energizing path selects it.
See the [portable cascaded-servo evidence](docs/evidence/M8-PORTABLE-CASCADED-SERVO.md).

The adjacent portable encoder checkpoint now turns truthful raw absolute counts
into those conservative servo observations without hiding multi-turn state or
measurement error. An explicit boot-local turn seed establishes the otherwise
unknowable wrap branch. Reduced rational scales define Q31.32 position per turn
and raw-count rate at normalized velocity one. Exact sample cadence, bounded
availability latency, and a configured trackable speed plus both samples' count
error derive an integer wrap window; profiles are rejected unless that window
is strictly narrower than half a turn. Accepted counts map outward to position
and two-sample velocity intervals. A required symmetric estimator-error term
keeps bounded acceleration/secant-to-endpoint, timestamp, and model uncertainty
from disappearing when that average becomes a newest-sample velocity enclosure;
separate ULP and admitted-speed gates then apply. All state changes are
transactional and latch first cause. An independent
1,600-sample simulator repeatedly crosses the wrap in both configured
directions and reproduces the same multi-turn truth byte for byte. Configuration
V6 stores and cross-checks the exact scale, clock, cadence, latency, wrap-speed,
precision, and acceleration-derived estimator policy, then lowers one
digest-bound estimator profile. No AS5600 target owner creates the required
stamps or seed, and no physical timestamp, speed, homing, or sensor-accuracy
fact is claimed. See the
[portable encoder-estimator evidence](docs/evidence/M8-PORTABLE-ENCODER-ESTIMATOR.md).

The MKS ESP32 FOC V1.0 now also has an independently authored typed board
package and linked `xtensa-esp32-none-elf` safe target. The revision schematic,
not example code, is authoritative: GPIO22/GPIO12 are unconnected, no
independent inverter enable or fitted cache medium is established, and the
capability record exposes neither. Core 1 owns all six phase inputs and drives
them to no-pull input mode synchronously before its first await. This matters:
each signal drives paired active-high/active-low EG2133 inputs, so either driven
level selects one MOSFET while high impedance is the documented both-off
candidate. Every FOC/motion operation still rejects. This is compile evidence
only, not a reset-state, shutdown, timing, or energization claim. See the
[MKS safe-target evidence](docs/evidence/M8-MKS-FOC-SAFE-TARGET.md).

Canonical machine configuration is now deliberately V6. It retains the V5
motion and FOC hardware authority and adds mandatory cascaded-servo,
encoder-scale, and encoder-policy records for every FOC axis. Exact gearing,
travel, calibration, velocity, acceleration, following-error, count, and timing
facts select the Q31.32/Q2.30 runtime lattices through checked rational
equalities and conservative uncertainty endpoints. Both cores validate the
same fixed bytes. Only a complete SHA-256-verified profile can lower into
digest-bound FOC controller, rotor, current, PWM compare, loop-grid, cascade,
and encoder objects; no V5 or older compatibility decoder remains.
Board-package qualification remains the authority, so configuration cannot
promote the MKS stages beyond `Described`;
the real target still rejects every FOC axis and exposes no energization path.
See the [canonical servo/encoder Configuration V6 evidence](docs/evidence/M8-CANONICAL-SERVO-ENCODER-CONFIGURATION-V6.md),
the historical [configuration V4 evidence](docs/evidence/M8-FOC-HARDWARE-CONFIGURATION-V4.md),
and its [V3 predecessor](docs/evidence/M8-FOC-CONFIGURATION-V3.md).

Portable rotor angles now use exact wrapping binary turns. Nearest-quadrant
reduction feeds outward Q2.30 sine/cosine series whose pi and coefficient
enclosures have independent integer certificates; runtime ULP policies reject
overwide components or unit-norm evidence. Absolute sensor counts, direction,
pole pairs, half-count quantization, electrical alignment error, and lattice
rounding are retained in one digest-bound rotor observation. This remains
portable mathematics: no scheduled AS5600 transaction, physical alignment,
PWM/ADC path, or energized target exists. See the
[exact-angle evidence](docs/evidence/M8-EXACT-ELECTRICAL-ANGLE.md).

A clean-room `alumina-as5600` crate now owns the read-only sensor wire boundary:
one two-byte RAW ANGLE transaction returns an exact count in `0..4096`, STATUS
flags remain explicit, and an optional three-transaction observation brackets
the angle without pretending the reads are simultaneous. It exposes no sensor
configuration or OTP/burn command. The MKS target seals both MCPWM units and
ADC1 in non-operational ownership states. Safe boot keeps both mode-selectable
encoder connectors dormant as inputs; an explicit, currently unscheduled
type-state transition can construct two independent 400 kHz AS5600 buses
without sending a transaction. Motion still rejects and the board remains
non-armable. See the
[AS5600/closed-ownership evidence](docs/evidence/M8-AS5600-CLOSED-OWNERSHIP.md).

The portable current boundary now likewise preserves measurement uncertainty.
Each raw ADC channel maps through an outward Q2.30 gain interval plus mandatory
additive error that cannot omit half-count quantization; rail codes, an
unbracketed zero, overwide intervals, and values outside the configured current
limit reject. A validated two-shunt snapshot names the measured phase pair,
widens for interchannel skew using an explicit current-slew bound, reconstructs
the third phase, and retains the independent-box zero-sequence residual. Every
sample also carries replayable PWM-period, duty-token, acquisition, channel,
conversion, and nearest-switching-edge timestamps. Device-cycle/PWM rate,
jitter, skew, latency, and edge guards must all agree before the sample can bind
to a FOC parameter snapshot. The synchronized producer remains portable
software: no target can create this stamp and no target current sample is
claimed. See the
[current-sampling evidence](docs/evidence/M8-CURRENT-SAMPLING-CONTRACT.md).

The MKS target now has a separate compile-only ADC1 commissioning transition.
It consumes the one ADC1 token and all four schematic current pins, programs an
explicit approximate attenuation for each route at the HAL-default 12-bit
resolution, and polls exactly channel 0 followed by channel 1. The returned raw
pair records request and conversion-completion observations only. It has no PWM
token, digest, sample-aperture time, edge witness, calibration lowering, or
`CurrentSense` implementation, so diagnostic reads cannot enter the torque
loop. The transition is not scheduled and has not run on hardware. See the
[classic ESP32 ADC1 evidence](docs/evidence/M8-CLASSIC-ESP32-ADC1-OWNER.md).

Exact center-aligned PWM lowering is now portable and replayable as well. A
digest-bound contract proves the device-cycle/PWM/counter-clock ratios, selects
the midpoint of each conservative duty interval on an integer compare lattice
with ties-to-even, retains the full interval-plus-quantization error, enforces
minimum active and inactive pulse widths, and derives both switching edges in
counter ticks. A bounded owner stages only complete three-phase images and
latches them on an exact timer-zero sequence. The MKS compile-only transition
can configure, immediately stop, and zero timer 0 in both MCPWM units while all
six phase GPIOs remain no-pull inputs. It attaches no operator or pin, exposes
no compare-write method, and implements no `PowerStage`. See the
[MCPWM compare evidence](docs/evidence/M8-EXACT-MCPWM-COMPARE.md).

The MKS capability document now identifies each independent 400 kHz encoder
connector as its own compile-supported AS5600 endpoint at address `0x36`, while
keeping the two unqualified power stages separate. A canonical FOC profile must
bind one endpoint, exactly two phase-selected ADC inputs, all three PWM phases,
and the qualified stage topology. Exact scalar authorities must agree with the
retained integer pole-pair, encoder-modulus, loop-rate, PWM-period, and dead-time
facts. Portable V5 tests replay every fixed record form and every two-chunk byte
boundary before lowering. The MKS adapter additionally checks the compiled
capability digest and exact motor routing before constructing a private stopped
MCPWM selection. These remain synthetic qualification fixtures; the physical
MKS target is non-armable, ADC1 commissioning is unqualified and unscheduled,
and both MCPWM units remain disconnected closed owners.

A configuration-derived hardware-loop simulator now joins those portable
boundaries without introducing a drive path. It starts with a neutral compare
image, derives each synchronized two-shunt observation from the active integer
compare edges, runs calibrated current reconstruction, exact-angle Park control,
interval SVPWM, and stages the next complete image at the sole expected
timer-zero. Clock-grid, sample, command, compare-precision, or boundary
disagreement terminally faults the virtual owner. An eight-period replay proves
identical complete results and deliberately demonstrates that a
1,000,000-ULP compare policy rejects an interval result accepted by the named
1,200,000-ULP test policy. This is virtual timing evidence, not an electrical,
WCET, ADC, PWM, or energization claim. See the
[configured FOC-loop evidence](docs/evidence/M8-CONFIGURED-FOC-HARDWARE-LOOP.md).

The portable complete-axis checkpoint now composes the Configuration V6
encoder estimator, nested position/velocity cascade, calibrated synchronized
current observation, certified electrical angle, dq PI, interval SVPWM, and
integer compare lowering behind one allocation-free owner. Activation first
returns a complete neutral image and becomes live only after the target reports
that exact image at the selected timer zero. Each live period is likewise a
two-phase transition: calculation advances only copied candidate state, and
the estimator, outer loops, current controller, counters, and active PWM image
advance together only after the exact future image is acknowledged. Wrong
identity, sample, schedule, prefix, or physical-commit evidence latches the
first cause without partial logical advance. A configuration-derived simulator
replays 401 complete periods twice and injects late-commit and missing-encoder
failures. This is portable software composition: no target constructs the
actor, no HAL writes or reads back a compare image, no shutdown transaction is
invoked, and no WCET or physical timing is claimed. See the
[portable complete-axis evidence](docs/evidence/M8-PORTABLE-SERVO-FOC-AXIS.md).

## Developer checks

The repository pins Rust 1.88. Run the portable checks from its root:

```console
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo xtask board list
cargo xtask board check mks-tinybee-v1
cargo xtask board check mks-tinybee-v1-4mb
cargo xtask board check t-deck-pro
cargo xtask board check mks-esp32-foc-v1
cargo xtask capabilities --board mks-tinybee --json
```

The portable commands intentionally operate on the workspace's default members.
The imported ESP32-S3 crates require the Espressif Xtensa toolchain and an
explicit target:

```console
cargo +esp check --target xtensa-esp32s3-none-elf --locked --lib \
  -p embedded-bus-async -p sx126x_async \
  -p t-deck-pro-battery-async -p t-deck-pro-epd-async \
  -p t-deck-pro-gps-async -p t-deck-pro-keyboard-async \
  -p t-deck-pro-lora-async -p t-deck-pro-touch-async
cargo +esp check --target xtensa-esp32s3-none-elf --locked --bins \
  -p i2c-tester -p patina
cargo +esp check --target xtensa-esp32s3-none-elf --locked --examples \
  -p t-deck-pro-epd-async -p t-deck-pro-lora-async
```

The board-aware commands select exactly one chip family and discover the espup
Xtensa linker bundle when it is not already on `PATH`:

```console
cargo xtask check --board mks-tinybee
cargo xtask check --board mks-tinybee-4mb
cargo xtask check --board t-deck-pro
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board mks-tinybee-4mb --profile release
cargo xtask build --board t-deck-pro --profile release
cargo xtask hil list
cargo xtask hil build mks-tinybee-pcm-short-safe
```

All current board packages remain intentionally non-armable. “Compiles” means the
typed package and complete release image build for the declared chip; it is not
bench, safe-state, peripheral-smoke, or timing qualification. See the
[dual-core compile evidence](docs/evidence/M2-DUAL-CORE-RUNTIME.md) and the
[expanded board-metadata evidence](docs/evidence/M2-BOARD-METADATA.md). The
HIL command only builds a separate safe-image capture artifact; it never
flashes. Running it still requires the disconnected-load checklist in
[HIL.md](docs/HIL.md). The network foundation can be built with
`ALUMINA_AP_PASSWORD` supplied outside the
repository, but build-provisioned credentials still do not satisfy the planned
unique device-stored production credential gate.

`mks-tinybee` selects the primary 8 MiB package observed on the connected V1.0
fixture. `mks-tinybee-4mb` is a separate opportunistic 4 MiB package with its
own immutable board ID and capability digest. Selection is compile-time only;
firmware never probes flash and substitutes packages at runtime. Successful
builds preserve board-qualified ELF names beside Cargo's conventional output
so building one variant cannot erase the only clearly named artifact for the
other. The 4 MiB profile is compile-supported but has no matching fixture and
remains subject to future web-bundle/update-slot flash budgets.

## Imported T-Deck support

The complete audited T-Deck snapshot is registered as workspace members under
`drivers/` and `examples/`: shared async I²C/SPI buses, SX126x, battery/charger,
e-paper, GPS, keyboard, LoRa, touch, the I²C tester, and the Patina integration
firmware. Datasheets and upstream root metadata are retained under
`imports/t-deck-async-drivers-rs/upstream-root/`. See the
[machine-readable import record](imports/t-deck-async-drivers-rs.toml) and
[M1 evidence](docs/evidence/M1-TDECK-IMPORT.md).

## Committed direction

- New code is dual-licensed `MIT OR Apache-2.0`; copied Apache-2.0 T-Deck drivers
  retain their original notices and provenance. Permissive MIT/Apache-compatible
  dependencies may be accepted after normal review; GPL-family implementation
  dependencies are excluded.
- Only dual-core ESP32 targets are in scope. MKS TinyBee and LILYGO T-Deck Pro
  are the first hardware targets; MKS ESP32 FOC V1.0 follows for servo control.
  The current T-LoRa Pager receives a late board stub before full support.
- Core 0 owns Wi-Fi, the web server, SD/cache service, T-Deck peripherals,
  telemetry presentation, and idle work. Core 1 owns safety, motion, FOC,
  deterministic I/O, and hardware-timed queues.
- Every board starts as its own Wi-Fi AP and serves the matching UI. The UI can
  scan and join infrastructure Wi-Fi; a shared WLAN is the only planned
  first-generation transport for coordinating multiple MCUs.
- The browser/WASM application is the authoritative CAD/CAM and machine-job
  compiler. CSGRS and the Hyper stack, especially Hypercurve, Hyperpath, and
  Hypersolve, preserve exactness until a named conversion to the configured
  motor/count/timer lattice.
- Firmware is deliberately thin: board drivers, safety, protocols, clock sync,
  SD-backed immutable job caches, bounded queues, real-time interpolation, and
  motor control. It does not parse source geometry or raw G-code.
- Each MCU in a synchronized machine caches and validates its own command-stream
  partition, then starts from an agreed future local cycle count derived from
  the UI's clock model. No CAN, USB, or serial multi-MCU transport is planned.
- `alumina-interface` may be substantially redesigned. It will use
  Hypergraphics rather than its hand renderer and grow into an exact CAM,
  LabVIEW-style graph, board-aware logic analyzer, oscilloscope, and debugger.

## Planning set

- [Delivery plan](docs/PLAN.md) — milestones, dependencies, gates, risks, and
  the first end-to-end workflow.
- [Decisions](docs/DECISIONS.md) — resolved product, licensing, board, safety,
  network, and compatibility policy.
- [Architecture](docs/ARCHITECTURE.md) — dual-core runtime, resources, protocol,
  safety, motion, FOC, and exact-to-integer execution boundary.
- [Native protocol](docs/PROTOCOL.md) — exact frame/message bytes, operation
  families, storage bodies, and hard version behavior.
- [Hyper integration](docs/HYPER-INTEGRATION.md) — precise roles for Hyperpath,
  Hypersolve, CSGRS, Hypergraphics, and optional Hyper crates.
- [Distributed jobs and storage](docs/DISTRIBUTED-JOBS.md) — Wi-Fi clock models,
  per-MCU SD caches, prepare/commit start, and failure semantics.
- [Repository audit](docs/REPOSITORY-AUDIT.md) — findings from all requested
  local repositories and the reusable T-Deck driver inventory.
- [Boards and hardware](docs/BOARD-MATRIX.md) — TinyBee, T-Deck Pro, MKS ESP32
  FOC V1.0, T-LoRa Pager, and the board-package/capability contract.
- [Peripheral and protocol coverage](docs/PERIPHERAL-COVERAGE.md) — generated
  resource coverage and staged support for wider ESP32 hardware.
- [Interface roadmap](docs/INTERFACE-ROADMAP.md) — authoritative WASM CAM,
  Hypergraphics, graph programming, annotated-board diagnostics, and plotting.
- [Fixed deployed graph IR](docs/GRAPH-IR.md) — canonical whitelisted opcodes,
  device-cycle schedules, const-generic arenas, independent admission, and
  authenticated split-core execution without a graph-document interpreter.
- [Verification strategy](docs/VERIFICATION.md) — simulation, exactness tests,
  HIL timing, synchronized-job, safety, and release evidence.
- [Research sources](docs/SOURCES.md) — local and upstream evidence captured on
  2026-08-10.

## First end-to-end workflow

The first demonstrator is an exact Hypercurve/Hyperpath 2D contour compiled by
the browser into a certified jerk-limited XYZ command stream, replayed first in
`alumina-sim`, uploaded as an immutable job to TinyBee SD, and executed as a
three-axis pen/air-cut trace. The UI overlays live resource state on an annotated
photo of the actual TinyBee and displays logic-analyzer-style step, direction,
limit, queue, and clock traces. A captured hardware trace must agree with the
simulator and the job's integer event record before powered cutting, laser,
heater, or plasma loads enter scope.
