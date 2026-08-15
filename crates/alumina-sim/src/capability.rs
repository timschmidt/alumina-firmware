//! Truthful capability package for the host-only TinyBee simulator.
//!
//! The simulated package reuses the physical TinyBee's descriptive topology,
//! but has a distinct immutable board identity and advertises only the
//! evidence paths actually composed by the host simulator. Constructing it
//! opens no device, GPIO, serial port, or network interface.

use alumina_board::{
    BoardDescriptor, BoardPackage, DigitalCaptureConfigureFlags, DigitalCaptureDescriptor,
    DigitalCaptureResourceDescriptor, DigitalCaptureSourceKind, DigitalCaptureTriggerSet,
    ResourceId, SupportLevel,
};
use alumina_protocol::Digest;

/// Stable board identity for the host-only TinyBee topology model.
pub const BOARD_ID: &str = "sim-mks-tinybee-v1";

/// Canonical channels implemented by the deterministic simulated acquisition.
pub static DIGITAL_CAPTURE_RESOURCES: &[DigitalCaptureResourceDescriptor] = &[
    DigitalCaptureResourceDescriptor {
        resource: ResourceId::Gpio(22),
        source: DigitalCaptureSourceKind::Simulated,
        support: SupportLevel::Compiles,
    },
    DigitalCaptureResourceDescriptor {
        resource: ResourceId::Gpio(32),
        source: DigitalCaptureSourceKind::Simulated,
        support: SupportLevel::Compiles,
    },
    DigitalCaptureResourceDescriptor {
        resource: ResourceId::Gpio(33),
        source: DigitalCaptureSourceKind::Simulated,
        support: SupportLevel::Compiles,
    },
    DigitalCaptureResourceDescriptor {
        resource: ResourceId::Gpio(35),
        source: DigitalCaptureSourceKind::Simulated,
        support: SupportLevel::Compiles,
    },
];

/// Fixed-memory acquisition limits implemented by the host simulator.
pub const DIGITAL_CAPTURE: DigitalCaptureDescriptor<'static> = DigitalCaptureDescriptor {
    schema_version: 1,
    support: Some(SupportLevel::Compiles),
    configure_flags: DigitalCaptureConfigureFlags(DigitalCaptureConfigureFlags::EDGE_TIMESTAMPS),
    trigger_kinds: DigitalCaptureTriggerSet(DigitalCaptureTriggerSet::IMMEDIATE),
    maximum_channels: 4,
    maximum_transitions: 64,
    configure_bytes: 208,
    record_bytes: 2_048,
    maximum_chunk_bytes: 168,
    maximum_pretrigger_micros: 0,
    maximum_duration_micros: 2_000_000,
    arm_horizon_micros: 30_000_000,
    resources: DIGITAL_CAPTURE_RESOURCES,
};

/// SHA-256 of the canonical simulator-specific `ALMCAP04` V4 document.
pub const CAPABILITY_DIGEST: Digest = Digest([
    0x4e, 0xa9, 0xbb, 0xf0, 0xb4, 0x4c, 0x86, 0x64, 0x80, 0x8b, 0x4e, 0x13, 0xb2, 0x02, 0x94, 0xa0,
    0x00, 0x63, 0x71, 0xcf, 0xe1, 0xd8, 0x43, 0x47, 0x8a, 0x19, 0x7b, 0x37, 0xb6, 0xbe, 0x6c, 0xc7,
]);

/// Builds the immutable simulator package without borrowing physical authority.
///
/// This is a value-returning function because the physical package is exported
/// as an immutable static. The returned value contains only `'static` tables and
/// is suitable for allocation-free capability encoding and service admission.
pub fn package() -> BoardPackage<'static> {
    let physical = board_mks_tinybee::PACKAGE;
    BoardPackage {
        board: BoardDescriptor {
            id: BOARD_ID,
            revision: "host-only deterministic model of TinyBee V1.x topology",
            capability_digest: CAPABILITY_DIGEST,
            ..physical.board
        },
        digital_capture: DIGITAL_CAPTURE,
        armable: false,
        ..physical
    }
}

#[cfg(test)]
mod tests {
    use alumina_capability::{calculate_identity, verify_declared_identity};

    use super::*;

    #[test]
    fn simulator_identity_and_capture_authority_are_explicit() {
        let package = package();
        assert_eq!(package.validate(), Ok(()));
        assert_eq!(package.board.id, BOARD_ID);
        assert_ne!(package.board.id, board_mks_tinybee::PACKAGE.board.id);
        assert!(!board_mks_tinybee::PACKAGE.digital_capture.is_implemented());
        assert!(package.digital_capture.is_implemented());
        assert_eq!(package.digital_capture.resources, DIGITAL_CAPTURE_RESOURCES);

        let calculated = calculate_identity(&package).unwrap();
        assert_eq!(calculated.byte_len, 3_655);
        assert_eq!(CAPABILITY_DIGEST.0, calculated.digest.0);
        assert_eq!(verify_declared_identity(&package), Ok(calculated));
    }
}
