# Fixed deployed graph IR V1

`alumina-graph-ir` is the portable, `no_std`, allocation-free admission boundary
for graph work that may eventually execute on firmware. The browser compiler
never sends an `ALGR` structural graph document to an MCU. It emits one exact
single-device `ALGRIR01` package containing only reviewed opcodes, integer
device-cycle schedules, fixed state/queue arenas, and the identities needed to
reject stale or substituted work.

This is a format, portable-admission, and fixed-opcode execution checkpoint. No
authenticated firmware API or board composition installs a package yet, and no
physical resource is claimed.

## Fixed package

Every package is exactly 4,096 bytes. Multi-byte integers are little-endian.
Bytes after initialized node/channel records and before the digest are zero.
The final 32 bytes are SHA-256 over bytes `0..4064`.

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 8 | magic `ALGRIR01` |
| 8 | 2 | exact version `1` |
| 10 | 2 | flags, zero in V1 |
| 12 | 4 | initialized byte length |
| 16 | 2 | node record count |
| 18 | 2 | channel record count |
| 20 | 2 | Service node count |
| 22 | 2 | Realtime node count |
| 24 | 2 | Service-to-Realtime bridge count |
| 26 | 2 | reserved zero |
| 28 | 4 | total node-state bytes |
| 32 | 4 | Service state-arena bytes |
| 36 | 4 | Realtime state-arena bytes |
| 40 | 4 | all channel-arena bytes |
| 44 | 4 | Service-to-Realtime channel bytes |
| 48 | 4 | Service schedule clock ID, or zero when absent |
| 52 | 4 | Realtime schedule clock ID, or zero when absent |
| 56 | 8 | Service release period in device cycles |
| 64 | 8 | Realtime release period in device cycles |
| 72 | 8 | summed Service node WCET cycles |
| 80 | 8 | summed Realtime node WCET cycles |
| 88 | 16 | exact target `DeviceId` |
| 104 | 32 | canonical source-graph digest |
| 136 | 32 | audited semantic/opcode-registry digest |
| 168 | 32 | target capability digest |
| 200 | 32 | active configuration digest |
| 232 | 8 | Service executor/queue reserve cycles |
| 240 | 8 | Realtime executor/queue reserve cycles |
| 248 | 8 | reserved zero |
| 256 | variable | 48-byte node records, then 32-byte channel records |
| initialized end | variable | required zero padding |
| 4064 | 32 | SHA-256 of the padded prefix |

A nonempty domain has one clock and period, at least one WCET cycle, and a
nonzero executor reserve. Checked `sum(WCET) + reserve <= period` is required.
An absent domain uses the all-zero schedule. The reserve is a static budget,
not measured WCET evidence; target execution remains closed until a real
executor and timing qualification establish it.

## Node records and opcodes

Each 48-byte node record retains graph node ID, domain, opcode, schedule clock,
contiguous domain-local state offset/size, period, WCET, and one canonical
64-bit immediate. Records are in deterministic topological order.

V1 intentionally admits only:

| Opcode | Domain | Input/output | State/immediate |
| --- | --- | --- | --- |
| `BooleanStreamConstant` | Service | no input, one Boolean Stream output | no state; immediate is exactly `0` or `1` |
| `BooleanLatest` | Realtime | one Boolean Stream input/output | exactly 5 retained bytes; immediate zero |
| `BooleanStreamSink` | Realtime | one Boolean Stream input, no output | no state; immediate zero; no modeled side effect |

`BooleanLatest` embodies the separately audited
`LatestAtOrBeforeSourceFirst` contract. It is not a general resampler opcode.
No GPIO, PWM, ADC, motion, FOC, safety, storage, network, or other resource
operation is in V1.

## Channel records

Each 32-byte channel record retains graph wire ID, topological source/target
indices, owning arena, full policy, capacity, item size, arena offset, and
storage size. Records are ordered by unique target. V1 uses only a timestamped
Boolean Stream item: a four-byte little-endian deployment-local Boolean tag
`1`, one canonical `0`/`1` value byte, an eight-byte source-schedule tick, and
an eight-byte monotonic sequence, or 21 bytes total. The deployment tag is not
an `ALGR` document-local type ID; the implementation digest binds that source
schema while firmware receives one fixed independently decodable runtime type.

The validator independently requires:

- source index less than target index;
- exactly one input channel for each consuming opcode;
- every producing opcode to have at least one consumer;
- contiguous offsets independently within Service, Realtime, and bridge arenas;
- `storage_bytes == capacity * item_bytes` with checked arithmetic;
- owner consistent with both endpoint domains;
- only Service→Realtime cross-core direction; and
- `Fault` on full for every queue consumed by Realtime.

Realtime→Service feedback, lossy realtime input, implicit shared state, events,
timeouts, synchronous slots, and multi-input nodes are not admitted. A future
Realtime→Service telemetry path must be a separately bounded, safety-reviewed
bridge rather than weakening this first command direction.

## Independent admission

`GraphIrPackage::encode` constructs bytes and then calls the same
`GraphIrPackage::decode` path used for untrusted input. Decode verifies exact
length, magic/version/flags, all identities, record counts/body length,
reserved bytes, padding, digest, opcode/domain/state rules, topology, schedule
counts and budgets, queue ownership, arena offsets/totals, and every header
aggregate. Validated node and channel iterators decode records without heap
allocation.

The representative package has three nodes, one 42-byte Service→Realtime queue,
one 21-byte Realtime queue, and five bytes of retained state. Its canonical
SHA-256 identity is:

```text
09ba7f443cb6acbd82c436943653fb55ce2d20992f763632f859e4f05fac5876
```

Tests reject every nonexact package length, ordinary digest tampering,
nonzero padding/reserved bytes, and semantic tampering followed by an attacker
recomputing a valid digest. A digest is content identity, never permission.

## Interface compiler boundary

The interface compiler now replays the structural graph, runs audited
type/channel/rate/cycle analysis, binds a reviewed implementation descriptor,
proves one target device, derives integer periods from that device's exact
cycle root, topologically orders nodes and target-owned channels, and proves
host-side arena policy. Its implementation digest binds the complete audited
semantic registry, fixed opcode descriptors, schedule clocks, WCETs, analysis
limits, and deployment limits. The emitted package is immediately decoded by
this crate. Firmware will still recheck every bounded invariant it can without
arbitrary-precision or graph-schema machinery.

## Portable split-core runtime

`alumina-runtime::graph::FixedGraphRuntime` owns one package plus caller-chosen
compile-time capacities for Service state, Realtime state, both local queue
arenas, and the one-way bridge. Admission copies and independently decodes the
exact 4 KiB package, requires its requested package digest, and matches device,
capability, configuration, and implementation identities before changing any
runtime state. It derives per-owner requirements again and rejects any package
that exceeds the concrete const-generic arrays. The installation report exposes
both selected payload bytes and `size_of::<Self>()`, so fixed queue cursors,
adjacency metadata, mutex, fault mailbox, and package storage are not hidden by
the 68-byte representative payload figure.

Start preparation is separately safety-gated. It fixes one device-cycle epoch
and executes Service release tick zero before issuing a Realtime owner, which
establishes the audited source-first initial value without inventing a default.
`split` then uniquely borrows disjoint Service and Realtime state/local queues.
Only the declared bridge arena and a first-cause fault mailbox remain shared;
the bridge uses the existing cross-core critical-section mechanism and copies
only canonical 21-byte items.

Each endpoint accepts only its exact next `DeviceCycle`, performs no allocation
or wait, and advances only after a complete release. The fixed executor runs
Boolean constants, consumes every due source item for latest-at-or-before,
retains the canonical five-byte Boolean, emits one target-tick item, and drains
the sink without a side effect. Queue full/corruption, missing initialization,
wrong-cycle release, arithmetic failure, invalid runtime shape, or absent safety
authority atomically latches the first fault and stops both domains. This is a
portable functional executor; the declared WCET/reserve is not target timing
evidence.

The first authenticated deployment lifecycle is now live. The browser publishes
the exact package as a typed immutable SD object, sends an identity-only install
request through the existing HMAC route, and reconciles status until both cores
independently admit the same content/package/implementation identities. Core 0
owns the verified publication reader and a complete validation buffer. Core 1
owns distinct staging and active 4 KiB arrays. Begin/Data/Finish/Activate/
Authorize/Clear/Abort commands fit the existing fixed inter-core payload, and a
combined canonical report exposes both actors without treating progress as
authority. Configuration, jobs, storage mutation, and graph lifecycle exclude
one another while fixed safety remains authoritative.

That lifecycle selects bytes but does not yet instantiate or split
`FixedGraphRuntime` inside the live Embassy tasks. Active selection is
boot-ephemeral; durable selector recovery, start/stop epochs, executor task
composition, measured deadline/WCET monitoring, resource opcodes,
capability-published arena limits, physical telemetry, and HIL timing remain
later work.

The reproduced compiler/runtime fixtures, target link results, artifact hashes,
and closed claims are recorded in
[`evidence/M9-FIXED-GRAPH-RUNTIME.md`](evidence/M9-FIXED-GRAPH-RUNTIME.md).
The authenticated SD/core lifecycle and its narrower open claims are recorded
in [`evidence/M9-AUTHENTICATED-GRAPH-DEPLOYMENT.md`](evidence/M9-AUTHENTICATED-GRAPH-DEPLOYMENT.md).
