//! Small, unmistakably synthetic raster used to exercise the visual pipeline.

use alumina_board::{BoardVisualDescriptor, HotspotDescriptor, NormalizedPoint, ResourceId};
use alumina_protocol::Digest;
use alumina_service::capability::CapabilityVisualAsset;

const TOP_LEFT: &[NormalizedPoint] = &[
    NormalizedPoint { x: 0, y: 0 },
    NormalizedPoint { x: 5_000, y: 0 },
    NormalizedPoint { x: 5_000, y: 5_000 },
    NormalizedPoint { x: 0, y: 5_000 },
];
const TOP_RIGHT: &[NormalizedPoint] = &[
    NormalizedPoint { x: 5_000, y: 0 },
    NormalizedPoint { x: 10_000, y: 0 },
    NormalizedPoint {
        x: 10_000,
        y: 5_000,
    },
    NormalizedPoint { x: 5_000, y: 5_000 },
];
const BOTTOM_LEFT: &[NormalizedPoint] = &[
    NormalizedPoint { x: 0, y: 5_000 },
    NormalizedPoint { x: 5_000, y: 5_000 },
    NormalizedPoint {
        x: 5_000,
        y: 10_000,
    },
    NormalizedPoint { x: 0, y: 10_000 },
];
const BOTTOM_RIGHT: &[NormalizedPoint] = &[
    NormalizedPoint { x: 5_000, y: 5_000 },
    NormalizedPoint {
        x: 10_000,
        y: 5_000,
    },
    NormalizedPoint {
        x: 10_000,
        y: 10_000,
    },
    NormalizedPoint {
        x: 5_000,
        y: 10_000,
    },
];

const HOTSPOTS: &[HotspotDescriptor<'static>] = &[
    HotspotDescriptor {
        id: "sim.gpio22",
        resource: ResourceId::Gpio(22),
        polygon: TOP_LEFT,
    },
    HotspotDescriptor {
        id: "sim.gpio32",
        resource: ResourceId::Gpio(32),
        polygon: TOP_RIGHT,
    },
    HotspotDescriptor {
        id: "sim.gpio33",
        resource: ResourceId::Gpio(33),
        polygon: BOTTOM_LEFT,
    },
    HotspotDescriptor {
        id: "sim.gpio35",
        resource: ResourceId::Gpio(35),
        polygon: BOTTOM_RIGHT,
    },
];

pub(crate) const ASSET_DIGEST: Digest = Digest([
    0x79, 0xfc, 0xf4, 0x64, 0xe3, 0x43, 0xff, 0xc8, 0xf3, 0x3c, 0xde, 0xc9, 0x6a, 0x8e, 0x2f, 0xd8,
    0x7d, 0xa8, 0x86, 0xc5, 0x7d, 0xd4, 0xbe, 0x08, 0x08, 0x98, 0xa5, 0xfc, 0x59, 0xb5, 0x24, 0x12,
]);

pub(crate) const VISUALS: &[BoardVisualDescriptor<'static>] = &[BoardVisualDescriptor {
    id: "simulated-topology",
    asset_path: "crates/alumina-sim/assets/simulated-topology.png",
    media_type: "image/png",
    pixel_width: 40,
    pixel_height: 20,
    asset_digest: ASSET_DIGEST,
    license: "CC0-1.0",
    attribution: "Alumina deterministic four-quadrant simulator fixture; not a PCB photograph",
    hotspots: HOTSPOTS,
}];

const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x28, 0x00, 0x00, 0x00, 0x14, 0x08, 0x02, 0x00, 0x00, 0x00, 0x70, 0x24, 0xe8,
    0xec, 0x00, 0x00, 0x00, 0x34, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x78, 0x50, 0x15, 0x4f,
    0x36, 0x8a, 0xdb, 0x71, 0x84, 0x6c, 0xc4, 0x30, 0x6a, 0xf1, 0xa8, 0xc5, 0xa3, 0x16, 0x8f, 0x5a,
    0x4c, 0xb6, 0xc5, 0xaf, 0x8e, 0x64, 0x91, 0x8d, 0xba, 0xf6, 0x95, 0x93, 0x8d, 0x46, 0x2d, 0x1e,
    0xb5, 0x78, 0xd4, 0xe2, 0x51, 0x8b, 0xc9, 0x46, 0x00, 0x54, 0x1a, 0xcb, 0x1c, 0x10, 0x5a, 0x3b,
    0x42, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

pub(crate) const ASSETS: &[CapabilityVisualAsset<'static>] = &[CapabilityVisualAsset {
    digest: ASSET_DIGEST,
    bytes: PNG,
}];

#[cfg(test)]
mod tests {
    use alumina_storage::sha256;

    use super::*;

    #[test]
    fn synthetic_asset_has_its_declared_identity() {
        assert_eq!(sha256(PNG).digest, ASSET_DIGEST);
        assert_eq!(VISUALS[0].hotspots.len(), 4);
    }
}
