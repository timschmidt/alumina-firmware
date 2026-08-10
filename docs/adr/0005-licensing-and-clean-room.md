# ADR 0005: Dual licensing and clean-room control algorithms

Status: accepted, 2026-08-10.

New work uses `MIT OR Apache-2.0`; imported T-Deck code retains its component
license and provenance. Dependencies under either MIT or Apache-2.0 may be
accepted after ordinary technical/security review.

Motion-planning behavior inspired by Synthetos/g2 and FOC behavior inspired by
SimpleFOC are implemented independently from functional requirements, published
mathematics, datasheets, and independently structured tests. No implementation
source is copied. `THIRD_PARTY.toml` records both copied sources and non-copied
behavioral references.
