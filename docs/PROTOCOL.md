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
| 24 | 32 | active-configuration SHA-256 digest, or zero only where allowed |

Decoding rejects an unknown family, nonzero flag, wrong magic/version, nonexact
header length, and payload length above the endpoint's fixed budget before a
payload decoder runs. Golden-byte tests cover the complete header.

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

## Families and assigned operations

| Family/range | V1 operations |
| --- | --- |
| identity `0x01xx` | get identity/boot facts |
| capabilities `0x02xx` | get canonical capabilities |
| clock `0x03xx` | timestamped heartbeat |
| configuration `0x04xx` | get, validate, commit, rollback |
| job `0x05xx` | inspect, prepare, commit, abort, hold, resume, cancel, status |
| command `0x06xx` | scheduled batch, diagnostic lease/release |
| telemetry `0x07xx` | subscribe, unsubscribe, event |
| network `0x08xx` | status, scan, join, leave, recover protected AP |
| storage `0x09xx` | status, list, begin, put chunk, finalize, read, delete, scrub |
| health `0x0axx` | bounded health snapshot |
| fault `0x0bxx` | fault event, reset request, physical/policy confirmation |
| waveform `0x0cxx` | configure, arm, chunk, stop |
| update `0x0dxx` | inspect, begin, put chunk, finalize, commit, rollback |

The Rust enum assigns all 48 values explicitly and rejects every unassigned
number. Operation-specific bodies are added only with fixed budgets and golden
browser/native/firmware fixtures.

## Storage bodies and content identity

Storage V1 fixes canonical identities to SHA-256 and admits no per-connection
algorithm negotiation. The current binary bodies are:

| Operation body | Fixed bytes | Variable bytes |
| --- | ---: | --- |
| begin/resume upload plan | 96 | none |
| put-chunk prefix | 52 | exact declared chunk bytes |
| upload progress | 32 | none |
| finalize request | 8 | none |

An upload plan commits a nonzero upload ID, typed object, complete content
digest/length, canonical manifest digest, fixed chunk size, and exact derived
count. Chunks are sequential and independently hashed; the final chunk alone may
be short. The canonical `ACMF` V1 manifest hash commits its schema, object facts,
layout, and each ordered chunk descriptor. Mutation is rejected during every
armed/energized state and while deterministic execution owns storage service.

## Encoding and security boundary

These codecs establish framing, canonical bytes, bounds, and state-independent
semantic rejection. Authentication, origin policy, rate limiting, credentials,
HTTP/WebSocket routing, signed updates, and operation-specific authorization are
separate mandatory M3 layers; their absence is not treated as an open network
service. JSON remains limited to bounded human-facing discovery/configuration.
