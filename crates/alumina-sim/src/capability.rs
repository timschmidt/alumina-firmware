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
use alumina_service::capability::CapabilityVisualAsset;
#[cfg(test)]
use alumina_service::capability::{CapabilityVisualCatalogError, VerifiedCapabilityVisualAssets};

use crate::visual_fixture;

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
    0x21, 0x8c, 0xc7, 0x58, 0xf4, 0x30, 0xf8, 0x89, 0x7f, 0x8c, 0x7d, 0xbc, 0xda, 0x6c, 0x1a, 0xf2,
    0x07, 0x7f, 0xae, 0x5f, 0xc0, 0x06, 0x2c, 0xce, 0x2f, 0xe7, 0xcb, 0x49, 0xa0, 0x66, 0xaa, 0x79,
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
        visuals: visual_fixture::VISUALS,
        armable: false,
        ..physical
    }
}

/// Immutable simulation-only visual blobs matched by the package descriptors.
pub const fn visual_assets() -> &'static [CapabilityVisualAsset<'static>] {
    visual_fixture::ASSETS
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
        assert_eq!(package.visuals, visual_fixture::VISUALS);
        assert_eq!(visual_assets(), visual_fixture::ASSETS);
        assert!(VerifiedCapabilityVisualAssets::try_new(&package, visual_assets()).is_ok());

        let invalid = [CapabilityVisualAsset {
            digest: visual_fixture::ASSET_DIGEST,
            bytes: b"not the declared PNG",
        }];
        assert!(matches!(
            VerifiedCapabilityVisualAssets::try_new(&package, &invalid),
            Err(CapabilityVisualCatalogError::Digest { .. })
        ));

        let calculated = calculate_identity(&package).unwrap();
        assert_eq!(calculated.byte_len, 4_028);
        assert_eq!(CAPABILITY_DIGEST.0, calculated.digest.0);
        assert_eq!(verify_declared_identity(&package), Ok(calculated));
    }
}
