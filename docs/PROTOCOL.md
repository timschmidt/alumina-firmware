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
