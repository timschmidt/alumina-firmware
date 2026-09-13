# Embedded Alumina interface bundle

This crate owns the immutable browser files served by firmware and the
simulator. The retained application was built from the `alumina-interface`
working tree based on commit
`0a111d2e5243db0e356e9befa1ac28c77785753a`; `assets/bundle.toml` records the
exact source and on-wire identities and is the authority for this capture.

The application JS and WASM use maximum-quality standard Brotli with the
smallest window that attains the measured byte minimum: quality 11, 23-bit
window, and no large-window extension (`-q 11 -w 23 -n`).
They are explicit `.br` application resources, not HTTP `Content-Encoding`
variants. This avoids depending on Brotli advertisement from a plain-HTTP LAN
origin and avoids retaining a duplicate gzip representation.

| Resource | Source bytes | Wire bytes | Wire SHA-256 |
| --- | ---: | ---: | --- |
| `/` | 1,077 | 1,077 | `3ccd883461eb1c786c2418512f8cb6094a141a8459a2dd597fad00cccf5a3a2b` |
| `/alumina-bootstrap.js` | 16,235 | 16,235 | `2d3d556f85918dd4d4a93490b40a6d957c55bce5449274220a898171307fef18` |
| `/alumina-brotli-decoder_bg.wasm` | 196,134 | 196,134 | `8a656d9cb18dfce82838f06f6153005a6cfa0c22c3cf575ef5cf30932fb46cab` |
| `/alumina-interface.js.br` | 91,816 | 10,909 | `5d6515aa29962f29b17b25496c0911c3ba9b4ca619fe9fdbaef60c35f55b10c3` |
| `/alumina-interface_bg.wasm.br` | 8,421,767 | 2,820,967 | `9c6310d1993a7facbfd97d84be6185b20cb72af2d6997433e3bee40b1e20a415` |
| `/alumina-worker.js` | 678 | 678 | `2d23324fa4fe34169abcc34046dcbcadbd13a02b99958f465ab3878a709c9a75` |
| `/favicon.ico` | 196 | 196 | `311f2089f900726358e69d1c490236942ea17aa418f198764a5662c8eaeeb5af` |

The 2,755-byte manifest has SHA-256
`8588ad72e8a4b516d1299060f9ad470d9d1db397bf6a4c96a3fb99b18cef9e49`
and is the eighth public route. JS plus WASM occupy 2,831,876 wire bytes,
803,833 fewer than reproducible gzip level-9 representations of the same exact
sources (22.11%).

An exhaustive quality-11 sweep confirms that w23 is a byte-minimum standard
encoding for both retained source assets. JS reaches 10,909 bytes at automatic
window selection and windows 17 through 24. WASM reaches 2,820,967 bytes at
automatic selection and windows 23 through 24. Nonstandard large windows
25 through 30 add one byte to both streams, so q11/w23 retains the exact minimum
without leaving RFC 7932 or requiring a larger decoder window.

The integrity-pinned bootstrap verifies every header, length, wire SHA-256,
decoded length, and source SHA-256 before importing the application. It fetches
the decoder first and then the two Brotli payloads concurrently. The document
uses a data-URL favicon, so the constrained TinyBee path needs at most two
simultaneous HTTP admissions. Firmware transmits every retained byte unchanged
and never performs decompression.

Regenerate atomically from the admitted interface distribution with:

```console
bash tools/alumina-brotli-decoder/build-assets.sh
bash tools/alumina-brotli-decoder/verify-maximum-compression.sh
```

The generator is fail-closed over the interface commit, source lengths, and
source hashes. Crate tests independently decode both Brotli resources and
verify the finite route table and every retained identity.

The captured interface assets are MIT. This crate and its dedicated decoder
integration are `MIT OR Apache-2.0`; the embedded decoder incorporates the
permissive `BSD-3-Clause/MIT` `brotli-decompressor` crate with its notice
retained under `tools/alumina-brotli-decoder`. No GPL-family source or asset is
included.
