# Native Alumina protocol V1

This is a greenfield exact-version protocol. There are no legacy routes,
negotiated compatibility modes, G-code commands, or generic numeric-pin calls.
A version mismatch produces an update flow or rejection. Rust memory layout is
never sent directly; every multibyte integer is encoded little-endian.

## Universal frame header

Every fixed binary WebSocket/intercore-derived native frame starts with exactly
56 bytes:

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 4 | ASCII `ALUM` |
| 4 | 2 | exact protocol version (`1`) |
| 6 | 1 | frame family |
| 7 | 1 | reserved flags (`0`) |
| 8 | 4 | payload byte length |
| 12 | 4 | stream sequence |
| 16 | 8 | device cycle/deadline |
| 24 | 32 | configuration-context SHA-256 digest, or zero only where allowed |

Decoding rejects an unknown family, nonzero flag, wrong magic/version, nonexact
header length, and payload length above the endpoint's fixed budget before a
payload decoder runs. Golden-byte tests cover the complete header.

For executable jobs, commands, and samples, the context is the exact active
configuration. Configuration lifecycle requests bind the current active digest
for `Get`/`Validate` and the exact selected candidate or active digest for
`Commit`/`Rollback`; the operation body independently repeats the selected
identity. Capability and unconfigured discovery frames require zero.

## Operation prefix

Every nonempty operation payload starts with exactly 16 bytes:

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 2 | exact operation number |
| 2 | 1 | request (`0`), response (`1`), or event (`2`) |
| 3 | 1 | reserved flags (`0`) |
| 4 | 2 | response status; requests/events require `Ok` |
| 6 | 2 | reserved (`0`) |
| 8 | 4 | nonzero request/response correlation; events require zero |
| 12 | 4 | following operation-body bytes |

The prefix and body must consume the outer payload exactly. An operation is
bound to one frame family. `TelemetryEvent`, `FaultEvent`, and `WaveformChunk`
are device events; other V1 operations are correlated request/response pairs.

Authenticated binary requests use the single greenfield transport endpoint
`POST /api/v1/control`. Its body is exactly one complete native frame and its
response is exactly one correlated native frame. The HTTP HMAC transcript binds
the `/api/v1/control` path, so a proof cannot be replayed against another route.
`GET /api/v1/storage` remains a small authenticated human-readable cache status;
`POST /api/v1/storage` is method-not-allowed and is not a compatibility alias.

## Families and assigned operations

| Family/range | V1 operations |
| --- | --- |
| identity `0x01xx` | get identity/boot facts |
| capabilities `0x02xx` | get canonical capabilities |
| clock `0x03xx` | timestamped heartbeat |
| configuration `0x04xx` | get, validate, commit, rollback |
| job `0x05xx` | inspect, prepare, commit, confirm, abort, hold, resume, cancel, status |
| command `0x06xx` | scheduled batch, diagnostic lease/release |
| telemetry `0x07xx` | subscribe, unsubscribe, event |
| network `0x08xx` | status, scan, join, leave, recover protected AP |
| storage `0x09xx` | status, list, begin, put chunk, finalize, read, delete, scrub, explicit provision |
| health `0x0axx` | bounded health snapshot |
| fault `0x0bxx` | fault event, reset request, physical/policy confirmation |
| waveform `0x0cxx` | configure, arm, chunk, stop |
| update `0x0dxx` | inspect, begin, put chunk, finalize, commit, rollback |

The Rust enum assigns all 50 values explicitly and rejects every unassigned
number. Operation-specific bodies are added only with fixed budgets and golden
browser/native/firmware fixtures.

## Clock heartbeat

`ClockHeartbeat` (`0x0301`) is an authenticated request in the zero-configuration
`ClockSample` family. Its 32-byte `ALMCLKQ1` body contains version/reserved bytes,
a nonzero browser probe ID at offset 16, and the browser worker's monotonic
nanosecond send timestamp at offset 24.

The fixed 128-byte `ALMCLKR1` response is:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 0 | 8 | magic `ALMCLKR1` |
| 8 | 2 | exact version (`1`) |
| 10 | 2 | monotonic/shared/deadline/job/safety flags |
| 12 | 1 | counter width, exactly 64 bits |
| 13 | 1 | clock source, currently Embassy monotonic (`1`) |
| 14 | 2 | reserved zero |
| 16 | 8 | echoed nonzero probe ID |
| 24 | 8 | echoed browser-worker send nanoseconds |
| 32 | 16 | nonzero boot ID, identical to the HTTP boot nonce |
| 48 | 8 | core-0 request-dequeue receive cycle |
| 56 | 8 | core-0 response-construction transmit cycle |
| 64 | 8 | Embassy counter frequency in hertz |
| 72 | 8 | minimum local start lead cycles |
| 80 | 8 | maximum local start horizon cycles |
| 88 | 8 | currently reported job/queue horizon cycles |
| 96 | 8 | cumulative maximum core-1 loop lateness |
| 104 | 8 | cumulative core-1 deadline misses this boot |
| 112 | 4 | free core-0→core-1 command slots |
| 116 | 4 | queued machine-work blocks |
| 120 | 8 | reserved zero |

Core 1 publishes a separate 40-byte `ALMCRTR1` report every 100 scheduler
samples: version/reserved at offsets 8/10, cumulative samples at 16, cumulative
misses at 24, and maximum lateness at 32. Core 0 accepts only fresh, increasing,
non-regressing reports. A heartbeat is schedule-authoritative only while that
report has zero misses and the separately observed safety state is fresh and
non-faulted. The firmware retains only the most recently returned healthy probe
for 500 ms; `JobCommit` must cite it exactly. Core 1 independently checks its
own deadline health again when applying commit and confirm.

The portable estimator maintains exact causal offset/rate intervals with a
declared parts-per-million rate envelope. It neither assumes nor estimates
symmetric Wi-Fi delay. Stale, high-round-trip, high-processing, reordered,
changed-boot/frequency, inconsistent, insufficient, or overly uncertain models
cannot yield a start cycle.

## Board capabilities

`CapabilitiesGet` reads the immutable canonical `ALMCAP01` board document in
authenticated ranges of at most 240 bytes. Public identity reports its SHA-256
and total length; every range repeats both, and the browser verifies the complete
reassembly before decoding or caching by digest. Request/response layouts, enum
assignments, and the complete serialization order are normative in
[`CAPABILITIES.md`](CAPABILITIES.md). The document includes typed resources,
aliases, buses/devices, memory/clock/electrical constraints, safe images,
licensed visual metadata, HIL requirements, qualification, and armability. Its
own declared digest is the sole excluded field, avoiding circular identity.

## Storage bodies and content identity

Storage V1 fixes canonical identities to SHA-256 and admits no per-connection
algorithm negotiation.

Storage object kind `6` is an inert canonical `MachineConfiguration`. The
browser uploads it through the ordinary content-addressed transaction, then
`ConfigurationValidate` selects its exact object/manifest identities. Resource
bindings, reduced rational facts and uncertainty, validation rules, external
selection bodies, and intercore transfer/report layouts are normative in
[`CONFIGURATION.md`](CONFIGURATION.md). Validation never aliases commit and
cannot change the active configuration.

Configuration V1 uses fixed bodies:

| Operation | Request body | Response body |
| --- | ---: | ---: |
| `ConfigurationGet` | 0 | 264-byte `ALMCST01` coordinator status |
| `ConfigurationValidate` | 96-byte `ALMCFQ01` publication | 264-byte status |
| `ConfigurationCommit` | 64-byte `ALMCFS01` selection | 264-byte status |
| `ConfigurationRollback` | 64-byte `ALMCFS01` selection | 264-byte status |

The response shape is fixed even for lifecycle errors. Commit is asynchronous:
the client polls status while core 0 validates, durably prepares, observes exact
core-1 activation, commits, and separately authorizes the identity for jobs.
Rollback aborts an uncommitted candidate or clears the exact committed active
selection. Neither response receipt nor Wi-Fi connectivity is a real-time
activation condition.

The current binary bodies are:

| Operation body | Fixed bytes | Variable bytes |
| --- | ---: | --- |
| storage status response | 112 | none |
| begin/resume upload plan | 96 | none |
| put-chunk prefix | 52 | exact declared chunk bytes |
| upload progress | 32 | none |
| finalize request | 8 | none |
| destructive cache provision | 112 | none |

An upload plan commits a nonzero upload ID, typed object, complete content
digest/length, canonical manifest digest, fixed chunk size, and exact derived
count. Chunks are sequential and independently hashed; the final chunk alone may
be short. The canonical `ACMF` V1 manifest hash commits its schema, object facts,
layout, and each ordered chunk descriptor. Mutation is rejected during every
armed/energized state and while deterministic execution owns storage service.

The implemented internal readback API identifies a publication by the exact
typed stored object and canonical manifest. It linearly revalidates the committed
record chain and returns sequential, independently hashed chunks into a fixed
1,024-byte caller buffer; the final chunk additionally requires aggregate object
and manifest verification. `StorageRead` remains unexposed by the HTTP service
at this checkpoint, and raw block addresses are not part of the native API.

The provision body begins with `ALMPRV01`, requires an explicit destructive
format flag, and binds the exact observed device block count, expected locator
generation/current media ID, new raw region, fresh media ID, and recovery intent
under a canonical SHA-256 confirmation. A stale request cannot silently reformat
a later generation. The recovery flag is required only when Alumina locator
bytes are recognizable but no valid prior generation can be trusted.

## Machine execution blocks

A V1 per-MCU machine partition is a nonempty concatenation of exact 512-byte
blocks. The storage object's byte length determines the block count and its
SHA-256 identity commits the complete concatenation, so the stream does not
embed a circular copy of its own object digest. Storage upload chunks may split
these bytes anywhere.

Each block has this canonical layout:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 0 | 8 | ASCII `ALMBLK01` |
| 8 | 2 | exact machine-IR version (`1`) |
| 10 | 1 | execution kind (`1` motion) |
| 11 | 1 | axis count (`1..=8`) |
| 12 | 4 | contiguous block sequence, beginning at zero |
| 16 | 4 | nonzero motion-segment count |
| 20 | 4 | exact initialized payload bytes |
| 24 | 8 | inclusive partition-relative stream tick |
| 32 | 8 | exclusive partition-relative stream tick |
| 40 | 16 | nonzero prepared stream ID |
| 56 | 32 | board-capability digest |
| 88 | 32 | active-configuration digest |
| 120 | 32 | previous block digest; zero only at sequence zero |
| 152 | 8 | reserved zero |
| 160 | 320 | records followed by zero padding |
| 480 | 32 | SHA-256 over bytes `0..480` |

One V1 motion record is `duration_ticks: u64`, zero flags `u32`, reserved zero
`u32`, then one signed little-endian `i64` lattice displacement per axis. The
payload length must equal `segment_count * (16 + 8 * axis_count)`. Thus one block
holds exactly eight 3-axis records or four 8-axis records at maximum capacity.
Durations must be nonzero and sum exactly to the block interval. Stream ticks
are not absolute device-counter values: deterministic commit supplies a future
local `DeviceCycle` epoch, and firmware uses checked addition when scheduling.
This keeps one cached partition independent of its eventual synchronized start.

Core 0 verifies the storage object, assembles blocks, checks this structure and
the prepared machine limits, then moves the complete owned value through a
fixed-credit channel. Core 1 hashes and validates the same bytes independently
before extending its admitted horizon. Unknown kinds, flags, versions, padding,
identity changes, skipped/duplicate/wrapped sequences, time gaps, digest-chain
changes, limit violations, and cumulative position overflow fail closed.

## Cached-job preparation bodies

`JobPrepare` has one exact 248-byte, self-hashed descriptor. The descriptor is
the UI compiler's claim about one already published per-MCU partition; it does
not contain a start epoch or permission to energize outputs.

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 0 | 8 | ASCII `ALMJOBD1` |
| 8 | 2 | exact descriptor version (`1`) |
| 10 | 6 | flags/reserved, all zero |
| 16 | 8 | nonzero boot-local prepare ID |
| 24 | 1 | fixed `MachineJobPartition` object kind |
| 25 | 1 | fixed SHA-256 object algorithm |
| 26 | 1 | fixed SHA-256 manifest algorithm |
| 27 | 1 | exact compile-time executor axis count |
| 28 | 4 | nonzero execution-block count |
| 32 | 8 | partition byte length, exactly `count * 512` |
| 40 | 8 | first relative stream tick, zero in V1 |
| 48 | 8 | nonzero maximum block ticks |
| 56 | 8 | nonzero maximum segment ticks |
| 64 | 8 | nonzero maximum lattice steps per segment |
| 72 | 16 | nonzero prepared stream ID |
| 88 | 32 | partition object SHA-256 digest |
| 120 | 32 | canonical publication-manifest SHA-256 digest |
| 152 | 32 | exact board-capability digest |
| 184 | 32 | exact active-configuration digest |
| 216 | 32 | SHA-256 over bytes `0..216` |

Decoding re-encodes the value and rejects every alternate representation. Core 0
also requires the outer frame configuration identity to equal the descriptor,
opens the exact typed publication, and compares the capability identity with the
selected board. Core 1 receives the complete descriptor in an independent fixed
command, repeats those identity checks, and constructs a separate stream
validator. TinyBee and T-Deck Pro now publish independently recomputed nonzero
capability identities. That alone does not enable preparation: firmware also
requires an exact nonzero active-configuration identity and an armable board.
Neither first board currently satisfies those later gates, so target
`JobPrepare` still returns `Unsupported` before storage is opened.

The 272-byte intercore command begins with `ALJC`, version `1`, a one-byte action,
and one reserved zero byte. Action `1` carries the 16-byte authentication boot ID
at `8..24` and the complete descriptor at `24..272`. Action `2` contains only the
nonzero prepare ID at `8..16`. Actions `3`, `4`, and `5` carry commit, confirm,
and abort bodies beginning at byte 8. Every unused byte is zero. `JobCancel`
uses the same bare eight-byte prepare ID as its native body. This 16-byte command
growth raises the reviewed runtime boundary storage from 12,480 to 12,608 bytes;
the 64 KiB internal-memory budget still covers the boundary and core-1 stack.

`JobCommit` is an exact 240-byte `ALMJCOM1` body:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 0 | 8 | magic `ALMJCOM1` |
| 8 | 2 | exact version (`1`) |
| 10 | 1 | attended (`1`) or cached-autonomous (`2`) policy |
| 11 | 5 | reserved zero |
| 16 | 8 | nonzero prepare ID |
| 24 | 16 | exact current boot ID |
| 40 | 32 | global job digest |
| 72 | 32 | complete participant-set digest |
| 104 | 32 | boot/descriptor-bound prepared token |
| 136 | 32 | local partition digest |
| 168 | 8 | local integer start cycle |
| 176 | 8 | confirmation deadline cycle |
| 184 | 8 | abort guard cycle |
| 192 | 8 | finite execution-lease expiry cycle |
| 200 | 8 | exact fresh heartbeat probe ID |
| 208 | 8 | certified local clock uncertainty cycles |
| 216 | 8 | nonzero required synchronization tolerance cycles |
| 224 | 16 | nonzero UI-selected commit ID |

The canonical order is `now < confirm deadline < abort guard < start < lease
expiry`; uncertainty may not exceed the required tolerance. The commit identity
used by later actions is SHA-256 over all 240 bytes. `JobConfirm` and `JobAbort`
each use an 88-byte `ALMJREF1` body: version/action/reserved at `8..16`, prepare
ID at 16, boot ID at 24, commit ID at 40, and complete commit digest at 56.
Confirm is deliberately a distinct `0x0509` operation; merely delivering commit
never grants start authority.

Core 1 owns `Prepared → Installed → Confirmed → Running → Complete/Faulted` plus
safe `Aborted` and unconfirmed `Expired` terminals. Installation requires the
exact active configuration, boot token, cached first block, local safety state,
fresh deadline health, lead/horizon/lease bounds, and policy. Confirmation must
arrive before both earlier guards and requires the realtime safety machine to
already be `Armed`. Missing confirmation self-expires at the confirmation
deadline; abort remains possible until the later abort guard. Start is emitted
at most once and a start later than its synchronization tolerance faults.

The 64-byte `ALMJSCH1` schedule report uses a strict union. Its header holds
version, state, fault, and commit/policy/start flags in bytes `8..16`. In
`Prepared`, bytes `16..48` are the prepared token and `48..64` are zero. After
commit, bytes `16..48` are start/confirmation/abort/lease cycles and `48..64`
is the commit ID. A committed report never carries a prepared token.

`JobStatus` has an empty request body and a fixed 304-byte response:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 0 | 8 | ASCII `ALMJST01` |
| 8 | 2 | exact status version (`1`) |
| 10 | 1 | bits 0/1/2: service, realtime, and schedule reports present |
| 11 | 5 | reserved zero |
| 16 | 96 | `ALMJSV01` core-0 report, or all zero |
| 112 | 128 | `ALMJRT01` core-1 report, or all zero |
| 240 | 64 | `ALMJSCH1` core-1 schedule report, or all zero |

The service report carries state, axis width, validated/sent/total block counts,
verified storage-chunk count, current ring credits/depth, and a terminal
`(StreamTick, block digest)` only when prefetch is complete. The realtime report
carries independently admitted/completed/total counts, ring depth, whether the
executor owns a block, and admitted/completed tick-and-digest facts. Absent
optional fields are zero-filled. Embedded stream reports must name the same nonzero
prepare ID and block count; if both are complete, their independently derived
terminal tick and block digest must also agree. Core 0 admits schedule reports
only when their prepared token or every committed field matches its exact local
descriptor/commit.

The current target images route prepare/commit/confirm/abort and independently
enforce these contracts on both cores, but neither board package is armable and
there is no interlock-qualified `Arm` transition or motor executor yet. Thus
target `JobPrepare` remains closed; if a future package were incorrectly made
armable without installing an executor, an emitted start is converted
immediately to a latched execution/safety fault rather than driving an output.
Hold, resume, lease renewal, observed-edge reconciliation, and cached-autonomous
authorization remain later operations.

## Real-time motion report

The portable executor defines one canonical 128-byte `ALMMOT01` report for the
future motion telemetry path. It is currently exercised by host and simulator
tests but is not yet published by the non-armable target images.

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 0 | 8 | ASCII `ALMMOT01` |
| 8 | 2 | exact report version (`1`) |
| 10 | 1 | executor state: idle `0`, ready `1`, segment `2`, complete `3`, faulted `4` |
| 11 | 1 | axis count (`1..=8`) |
| 12 | 2 | bit 0: next deadline present; all other bits zero |
| 14 | 2 | reserved zero |
| 16 | 8 | absolute job epoch in local device cycles |
| 24 | 8 | next contiguous stream-relative tick |
| 32 | 8 | exact next local deadline, or zero when absent |
| 40 | 4 | completed segment count |
| 44 | 4 | maximum accepted software lateness in cycles |
| 48 | 8 | deadline-miss count |
| 56 | 2 | logically enabled axis mask |
| 58 | 2 | currently high step mask |
| 60 | 2 | axes with known direction |
| 62 | 2 | known positive directions |
| 64 | 64 | eight signed `i64` lattice positions |

Only the segment state carries a next deadline. Bits above the declared axis
width and positions above it are zero; high steps must be enabled, and positive
direction bits must also be direction-known. Decoding re-encodes and compares
the complete value, rejecting alternate or reserved representations. A missed
edge is never returned to the backend: the executor faults, removes all future
deadlines, retains current output masks for the immediate fail-safe transaction,
and increments the miss count.

The 112-byte storage-status body is canonical little-endian:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 0 | 1 | backend availability |
| 1 | 1 | upload mutation currently admitted |
| 2 | 1 | degraded cache anchor |
| 3 | 1 | upload progress present |
| 4 | 1 | degraded provisioning locator |
| 5 | 1 | destructive provision currently admitted |
| 6 | 1 | coarse provisioning/media fault |
| 7 | 1 | bit 0 region present; bit 1 media ID present |
| 8 | 8 | physical device blocks |
| 16 | 8 | selected region start, zero-filled when absent |
| 24 | 8 | selected region blocks |
| 32 | 8 | free append blocks |
| 40 | 8 | last cache-record sequence |
| 48 | 8 | trusted locator generation |
| 56 | 4 | published object count |
| 60 | 4 | reserved zero |
| 64 | 16 | logical media ID, zero-filled when absent |
| 80 | 32 | upload progress, zero-filled when absent |

The destructive provision request uses offsets 0/8/10/12 for eight-byte magic,
version, flags, and reserved zero; 16/24 for expected device blocks/generation;
32 for the expected 16-byte current ID (zero only at generation zero); 48/56
for new region start/count; 64 for the fresh 16-byte ID; and 80 for SHA-256 over
the preceding 80 bytes. Flag bit zero confirms destructive format and bit one
confirms recovery of recognizable but untrusted locator bytes; no other flag is
assigned.

## Encoding and security boundary

These codecs establish framing, canonical bytes, bounds, and state-independent
semantic rejection. Authentication, origin policy, rate limiting, credentials,
HTTP/WebSocket routing, signed updates, and operation-specific authorization are
separate mandatory M3 layers; their absence is not treated as an open network
service. JSON remains limited to bounded human-facing discovery/configuration.

## HTTP authentication transcript V1

`GET /api/v1/auth` returns a fresh public 16-byte boot nonce as 32 lowercase hex
characters. It is never reused intentionally and invalidates all request
counters on reboot. Authenticated routes require exactly one canonical decimal
`X-Alumina-Counter` and one 64-lowercase-hex
`X-Alumina-Authorization` header. Counter zero, a leading zero, overflow,
duplicate security header, any `Transfer-Encoding`, or an ambiguous/nonexact
`Content-Length` is rejected. Native commands use exactly
`application/vnd.alumina.frame`.

The request tag is HMAC-SHA-256 keyed by the current device API secret over this
exact byte sequence:

| Field | Encoding |
| --- | --- |
| domain | ASCII `ALUMINA-HTTP-AUTH-V1` followed by one NUL |
| boot nonce | 16 raw bytes |
| request counter | `u64` little-endian |
| method | GET `1`, POST `2`, PUT `3`, DELETE `4` |
| path length and path | `u16` little-endian, then exact UTF-8 path bytes |
| body length | `u32` little-endian |
| body identity | raw SHA-256 of the exact HTTP body |

Responses to authenticated requests echo the canonical decimal counter and put
their tag in `X-Alumina-Response-Authorization`. That HMAC transcript is ASCII
`ALUMINA-HTTP-RESPONSE-V1` plus NUL, boot nonce, counter `u64` LE, HTTP status
`u16` LE, media byte (JSON `1`, native frame `2`), body length `u32` LE, and raw
SHA-256 body identity. Request and response golden vectors are tested in
`alumina-net` and independently reproducible with ordinary HMAC/SHA-256 tools.

The firmware accepts each valid counter once in a 64-counter out-of-order window
and applies a global 32-request burst/50-valid-request-per-second token bucket.
Invalid HMACs do not consume counters or tokens; a valid rate-limited request
does consume its counter. Authentication state and hashing execute only on the
cooperative core-0 executor and do not hold a cross-core critical section.

HMAC provides request/response authenticity and integrity, not confidentiality.
The API secret is never sent over HTTP. The initial AP relies on WPA2 link
protection and the local-LAN/VPN deployment boundary; production additionally
requires a unique transactional device-stored secret. The development/build
passphrase can drive bench authentication but can never satisfy the production
arming credential gate.
