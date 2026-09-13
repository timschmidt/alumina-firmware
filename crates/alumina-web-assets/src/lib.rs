#![no_std]
//! Exact, immutable browser-interface assets embedded in Alumina firmware.
//!
//! The bundle records both source-content and on-wire identities. Firmware
//! serves only these exact paths and never decompresses, rewrites, or guesses
//! media. Large sources are explicit RFC 7932 resources rather than HTTP
//! content codings, so plain-HTTP LAN browsers verify and decode them through
//! the retained bootstrap without a duplicate gzip representation.

#[cfg(test)]
extern crate std;

use alumina_protocol::Digest;

/// Canonical manifest format retained by this source tree.
pub const WEB_BUNDLE_FORMAT: &str = "alumina-web-bundle-v2";

/// Base `alumina-interface` commit for the retained working-tree capture.
///
/// The manifest's source lengths and hashes identify the exact built bytes.
pub const INTERFACE_COMMIT: &str = "0a111d2e5243db0e356e9befa1ac28c77785753a";

/// Browser policy required by the retained Trunk/egui application. The exact
/// bootstrap imports the verified source through a temporary blob URL; network
/// access remains explicit so the UI can coordinate LAN/VPN devices and fetch
/// user-approved resources.
pub const INTERFACE_CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self' blob: 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; connect-src 'self' http: https: ws: wss:; img-src 'self' data: blob:; worker-src 'self' blob:; font-src 'self' data: https:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'";

/// Canonical human-readable bundle manifest served for audit and update
/// matching. Its SHA-256 is [`WEB_BUNDLE_DIGEST`].
pub const WEB_BUNDLE_MANIFEST: &[u8] = include_bytes!("../assets/bundle.toml");

/// Storage form of one immutable resource.
///
/// This is deliberately distinct from HTTP `Content-Encoding`: every route is
/// transmitted byte-for-byte with identity HTTP coding. The `.br` routes are
/// explicit application resources decoded only after bootstrap verification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoredRepresentation {
    /// Bytes are the source representation.
    Identity,
    /// Bytes are an RFC 7932 Brotli stream encoded at quality 11/window 23.
    BrotliRfc7932Q11W23,
}

impl StoredRepresentation {
    /// Canonical manifest and HTTP diagnostic label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::BrotliRfc7932Q11W23 => "brotli-rfc7932-q11-w23",
        }
    }
}

/// One immutable route and its source/on-wire identities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmbeddedWebAsset {
    path: &'static str,
    source_media_type: &'static str,
    media_type: &'static str,
    stored_representation: StoredRepresentation,
    source_bytes: u32,
    wire_bytes: u32,
    source_digest: Digest,
    wire_digest: Digest,
    bytes: &'static [u8],
}

impl EmbeddedWebAsset {
    /// Exact request path.
    pub const fn path(self) -> &'static str {
        self.path
    }

    /// Exact response `Content-Type` field.
    pub const fn media_type(self) -> &'static str {
        self.media_type
    }

    /// Media type of the decoded source representation.
    pub const fn source_media_type(self) -> &'static str {
        self.source_media_type
    }

    /// Exact immutable storage representation.
    pub const fn stored_representation(self) -> StoredRepresentation {
        self.stored_representation
    }

    /// Complete decoded source length.
    pub const fn source_bytes(self) -> u32 {
        self.source_bytes
    }

    /// Complete exact on-wire representation length.
    pub const fn wire_bytes(self) -> u32 {
        self.wire_bytes
    }

    /// SHA-256 over the decoded source bytes.
    pub const fn source_digest(self) -> Digest {
        self.source_digest
    }

    /// SHA-256 over the exact on-wire bytes.
    pub const fn wire_digest(self) -> Digest {
        self.wire_digest
    }

    /// Exact immutable on-wire representation.
    pub const fn bytes(self) -> &'static [u8] {
        self.bytes
    }
}

include!("generated.rs");

/// Resolve one exact public path. Prefixes, aliases, implicit decompression,
/// and representation fallbacks are deliberately absent.
pub fn web_asset(path: &str) -> Option<EmbeddedWebAsset> {
    WEB_ASSETS.iter().copied().find(|asset| asset.path == path)
}

const fn digest(value: &str) -> Digest {
    let bytes = value.as_bytes();
    assert!(
        bytes.len() == 64,
        "SHA-256 text must contain 64 lowercase hex bytes"
    );
    let mut output = [0_u8; 32];
    let mut index = 0;
    while index < output.len() {
        output[index] = (hex(bytes[index * 2]) << 4) | hex(bytes[index * 2 + 1]);
        index += 1;
    }
    Digest(output)
}

const fn hex(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        _ => panic!("SHA-256 text is not lowercase hexadecimal"),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::string::String;
    use std::vec::Vec;

    use super::*;
    use alumina_storage::sha256;
    use brotli_decompressor::BrotliDecompress;

    #[test]
    fn every_retained_wire_identity_and_manifest_are_exact() {
        assert_eq!(WEB_ASSETS.len(), 8);
        assert_eq!(sha256(WEB_BUNDLE_MANIFEST).digest.0, WEB_BUNDLE_DIGEST.0);
        for asset in WEB_ASSETS {
            assert_eq!(
                u32::try_from(asset.bytes().len()).unwrap(),
                asset.wire_bytes()
            );
            assert_eq!(sha256(asset.bytes()).digest.0, asset.wire_digest().0);
            if asset.stored_representation() == StoredRepresentation::Identity {
                assert_eq!(
                    asset.source_bytes(),
                    u32::try_from(asset.bytes.len()).unwrap()
                );
                assert_eq!(asset.source_digest(), asset.wire_digest());
            }
        }
    }

    #[test]
    fn exact_routes_and_explicit_brotli_resources_fail_closed() {
        assert_eq!(web_asset("/").unwrap().path(), "/");
        assert!(web_asset("/alumina-interface.js").is_none());
        assert!(web_asset("/alumina-interface_bg.wasm").is_none());
        assert!(web_asset("/alumina-interface.js.br/extra").is_none());
        assert!(matches!(
            web_asset("/alumina-interface.js.br"),
            Some(asset)
                if asset.stored_representation()
                    == StoredRepresentation::BrotliRfc7932Q11W23
        ));
        assert_eq!(
            web_asset("/alumina-interface_bg.wasm.br")
                .unwrap()
                .source_media_type(),
            "application/wasm"
        );
    }

    #[test]
    fn every_brotli_resource_decodes_to_its_admitted_source_identity() {
        for asset in WEB_ASSETS.iter().copied().filter(|asset| {
            asset.stored_representation() == StoredRepresentation::BrotliRfc7932Q11W23
        }) {
            let mut input = Cursor::new(asset.bytes());
            let mut source = Vec::with_capacity(asset.source_bytes() as usize);
            BrotliDecompress(&mut input, &mut source).unwrap();
            assert_eq!(source.len(), asset.source_bytes() as usize);
            assert_eq!(sha256(&source).digest.0, asset.source_digest().0);
        }
    }

    #[test]
    fn bootstrap_pins_the_generated_asset_authority() {
        let bootstrap = core::str::from_utf8(BOOTSTRAP).unwrap();
        for asset in WEB_ASSETS.iter().copied().filter(|asset| {
            matches!(
                asset.path(),
                "/alumina-brotli-decoder_bg.wasm"
                    | "/alumina-interface.js.br"
                    | "/alumina-interface_bg.wasm.br"
            )
        }) {
            let source_digest = digest_text(asset.source_digest());
            let wire_digest = digest_text(asset.wire_digest());
            assert!(bootstrap.contains(asset.path()), "missing {}", asset.path());
            assert!(
                bootstrap.contains(&std::format!("sourceBytes: {}", asset.source_bytes())),
                "stale source length for {}",
                asset.path()
            );
            assert!(
                bootstrap.contains(&std::format!("wireBytes: {}", asset.wire_bytes())),
                "stale wire length for {}",
                asset.path()
            );
            assert!(
                bootstrap.contains(&std::format!("sourceSha256: '{source_digest}'")),
                "stale source digest for {}",
                asset.path()
            );
            assert!(
                bootstrap.contains(&std::format!("wireSha256: '{wire_digest}'")),
                "stale wire digest for {}",
                asset.path()
            );
        }
    }

    fn digest_text(digest: Digest) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut output = String::with_capacity(64);
        for byte in digest.0 {
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        output
    }
}
