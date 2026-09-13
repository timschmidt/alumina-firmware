//! Bounded, safe-Rust Brotli decoder used to bootstrap the browser interface.
//!
//! This crate is a build tool, not an ESP32 firmware dependency. Its tiny WASM
//! artifact lets a plain-HTTP LAN browser consume the explicit RFC 7932
//! resources embedded by Alumina even when the browser declines `br` HTTP
//! content coding on an insecure origin.

use std::io::{self, Cursor};

use brotli_decompressor::BrotliDecompress;
use wasm_bindgen::prelude::*;

const MAXIMUM_COMPRESSED_BYTES: usize = 4 * 1_024 * 1_024;
// The browser bundle crossed 8 MiB as the authenticated provisioning and
// graphical-control surfaces grew. Keep a fixed power-of-two ceiling with
// ample evidence-visible headroom instead of deriving an allocation bound
// from an untrusted manifest value.
const MAXIMUM_DECOMPRESSED_BYTES: usize = 16 * 1_024 * 1_024;

/// Decode exactly one bounded RFC 7932 stream.
///
/// The caller supplies the manifest length. Output that ends early or tries to
/// exceed that exact length is rejected before any bytes are returned to JS.
#[wasm_bindgen(js_name = decompressExact)]
pub fn decompress_exact(
    compressed: &[u8],
    expected_decompressed_bytes: usize,
) -> Result<Vec<u8>, JsValue> {
    decode_bounded(compressed, expected_decompressed_bytes).map_err(JsValue::from_str)
}

fn decode_bounded(
    compressed: &[u8],
    expected_decompressed_bytes: usize,
) -> Result<Vec<u8>, &'static str> {
    if compressed.len() > MAXIMUM_COMPRESSED_BYTES {
        return Err("compressed Brotli resource exceeds bootstrap limit");
    }
    if expected_decompressed_bytes > MAXIMUM_DECOMPRESSED_BYTES {
        return Err("decoded Brotli resource exceeds bootstrap limit");
    }

    let mut input = Cursor::new(compressed);
    let mut output = ExactWriter::new(expected_decompressed_bytes);
    BrotliDecompress(&mut input, &mut output)
        .map_err(|_| "invalid or overlong Brotli resource")?;
    output.finish()
}

struct ExactWriter {
    expected_bytes: usize,
    bytes: Vec<u8>,
}

impl ExactWriter {
    fn new(expected_bytes: usize) -> Self {
        Self {
            expected_bytes,
            bytes: Vec::with_capacity(expected_bytes),
        }
    }

    fn finish(self) -> Result<Vec<u8>, &'static str> {
        if self.bytes.len() != self.expected_bytes {
            return Err("decoded Brotli resource has the wrong length");
        }
        Ok(self.bytes)
    }
}

impl io::Write for ExactWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(next_length) = self.bytes.len().checked_add(bytes.len()) else {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "length overflow"));
        };
        if next_length > self.expected_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "decoded output exceeds the manifest length",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn compressed(bytes: &[u8]) -> Vec<u8> {
        let mut output = Vec::new();
        {
            let mut writer = brotli::CompressorWriter::new(&mut output, 4_096, 11, 24);
            writer.write_all(bytes).unwrap();
        }
        output
    }

    #[test]
    fn exact_stream_round_trips() {
        let source = b"alumina exact browser bootstrap".repeat(1_024);
        let encoded = compressed(&source);
        assert_eq!(decode_bounded(&encoded, source.len()).unwrap(), source);
    }

    #[test]
    fn early_and_overlong_output_fail_closed() {
        let source = b"machine resolution".repeat(128);
        let encoded = compressed(&source);
        assert_eq!(
            decode_bounded(&encoded, source.len() + 1),
            Err("decoded Brotli resource has the wrong length")
        );
        assert_eq!(
            decode_bounded(&encoded, source.len() - 1),
            Err("invalid or overlong Brotli resource")
        );
    }

    #[test]
    fn invalid_stream_fails_closed() {
        assert_eq!(
            decode_bounded(b"not a Brotli stream", 8),
            Err("invalid or overlong Brotli resource")
        );
    }

    #[test]
    fn declared_output_beyond_the_fixed_ceiling_fails_before_decode() {
        assert_eq!(
            decode_bounded(b"", MAXIMUM_DECOMPRESSED_BYTES + 1),
            Err("decoded Brotli resource exceeds bootstrap limit")
        );
    }
}
