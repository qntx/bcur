# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.0] — 2026-08-19

In-tree production freeze of the 0.3 transport + typed dCBOR API.
`bcur` 1.0.0, `bcur-cli` 1.0.0. No compatibility aliases.

### Changed

- `DecoderLimits::default` six integers are the production Default
  (`1_048_576`, `2_000`, `8_192`, `4_000`, `8_000`, `8_192`). Hosts that
  need another budget still use `Decoder::with_limits`.
- Workspace / CLI versions `1.0.0`; `bcur-cli` pins `bcur = "1"`.

## [0.3.0] — 2026-08-18

Production hardening of 0.2. Breaking 0.x API. Not a 1.0 freeze.
`bcur` 0.3.0, `bcur-cli` 0.2.0. No compatibility aliases.

### Added

- `ResourceKind`, `CborError`, `CborErrorKind`. `Error::ResourceLimit` takes
  `ResourceKind`. `Error::Cbor` wraps `CborError` (`kind()` / `detail()`).
- `Error::SinglePartExhausted` — fountain `Encoder` `K == 1` second `next_part`
- Public sealed `bcur::ur::IntoUrType` (`UrType`, `&UrType`, `&str`, `String`)
- `DecoderLimits::worst_case_heap_bytes`: saturating cap-product ceiling of
  the public caps, excluding allocator/BTree overhead. 64-bit `Default` is
  `225_824_768` (≈ 215 MiB). Default integers are unchanged (not frozen
  in 0.3).
- `ur::Encoder::complete`; `MultipartEncoder::{complete, is_single_part}`;
  `MultipartDecoder::{fragment_count, resolved_fragment_count, is_poisoned,
  ur_type, with_expected_type}`
- `fuzz/` cargo-fuzz targets (`decode_ur`, `fountain_part`, `bytewords`,
  `encode_roundtrip`) and seed corpora
- `crates/bcur/tests/vectors/` goldens; weekly published-string drift job
- `THIRD_PARTY.md` restored

### Changed

- Deleted `TryFrom<&UrType> for UrType`
- `fountain::Decoder::fragment_count` is `u32` (was `usize`);
  `resolved_fragment_count` is `Option<u32>` (was `Option<usize>`).
  The `unwrap_or(u32::MAX)` fallback is gone.
- UR `Encoder` `K == 1` re-emits `ur:<type>/<body>` and does not call fountain.
  `current_index` saturates at 1.
- Lockfile: `bitcoin_hashes` 1.2.0, `minicbor` 2.3.0, `thiserror` 2.0.20,
  and latest compatible transitives (`cc` 1.4.3, `wasm-bindgen` 0.2.127,
  `zerocopy` 0.8.56, `futures-*` 0.3.34). `Cargo.toml` ranges were already
  at current majors; no incompatible upgrades exist.

### Migration

```text
Error::ResourceLimit("uri_len")           → Error::ResourceLimit(ResourceKind::UriLen)
Error::Cbor(s)                            → Error::Cbor(c) ; c.kind() / c.detail()
TryFrom<&UrType> for UrType               → deleted; pass UrType / &UrType / &str / String into Ur::new
                                            (sealed bcur::ur::IntoUrType; do not impl)
ur::Decoder::fragment_count unwrap_or MAX → gone; type is still u32
fountain::Decoder::fragment_count         → u32 (was usize)
resolved_fragment_count                   → Option<u32> (was Option<usize>)
fountain Encoder K==1 second next_part    → Error::SinglePartExhausted
ur::Encoder::current_index when K == 1    → saturates at 1; fountain sequence is not advanced
                                            (interop vs URKit is the emitted UR string, not seqNum)
CLI encode                                → --type is required
bcur-cli Cargo.toml bcur version          → "0.3"
```

### CLI (`bcur-cli` 0.2.0)

- `--type` is required on encode (no default `bytes`)
- Path-dep SemVer pin `bcur = { path = "../bcur", version = "0.3" }`

### Notes

- L0–L3 vs L4 dCBOR contract is in crate rustdoc
- Default limit integers were not frozen in this release

## [0.2.0] — 2026-08-18

Optional typed dCBOR layer (`feature = "dcbor"`, implies `std`).

### Added

- `typed::Ur` value object (`new`, `from_ur_string`, `string`, QR helpers)
- `UrEncodable` / `UrDecodable` / `UrCodable` with blanket impls on
  `dcbor` tagged types (first tag **name** is the UR type; body is untagged
  dCBOR; unnamed tags return `Error::InvalidType`, never panic)
- Owned `MultipartEncoder` / `MultipartDecoder` wrappers (`DecoderLimits`,
  type stickiness, full-URI case fold)
- `ur::Decoder::ur_type` — type pinned by the first received part
- `TryFrom<String>` and `TryFrom<&UrType>` for `UrType`

### Changed

- `dcbor` dependency enables `multithreaded` so `Ur` is `Send + Sync`
- Crate version `0.2.0`
- `Part::from_cbor` / `from_cbor_with_max` cap `sequence_count` (default K = 2000)

### CLI (`bcur-cli` 0.1.0)

- Binary `bcur`: encode/decode UR text and terminal QR (raw payload, no CBOR wrap)

### Notes

- Transport (`no_std` + `alloc`) is unchanged
- Default limit integers were not frozen in this release
- Removed empty reserved `bytemoji` feature (implement when the tables land)
- MSRV 1.86; criterion 0.8
- `ur::encode` returns `String` (type is already validated; `Result` was dead)
- `Part::from_cbor` rejects zero sequence / empty metadata (no panic on `indexes()`)
- Tag workflows publish `bcur` / `bcur-cli` (removed kobe residue)
- `Encoder` emits single-part UR when `K == 1`; `Decoder::receive` accepts single-part
- `decode_message` for complete single-part payloads
- Type stickiness pins only after a part is successfully ingested
- Fountain rejects `fragment_len * K` padding that cannot come from partition
- `DecoderState` poisons the session; `fragment_count` is `u32` on UR decoder
- Removed public `parse_normalized` (use `parse`)
- Typed wrappers: `complete` / `fragment_count` (dropped `is_complete` / `parts_count`)

## [0.1.0] — 2026-08-12

First **transport** release (L0–L3): bytewords, fountain codes, multi-part UR,
resource limits, and interop goldens. No typed dCBOR API promise in this series.

### Added

- Owned bytewords encode/decode (standard / URI / minimal styles) with CRC-32
- Fountain encoder/decoder with Xoshiro256** + Walker alias degree selection
- Fixed-schema Part CBOR (shortest-form encode/decode)
- Multi-part UR `Encoder` / `Decoder` with type stickiness and `DecoderLimits`
- Fail-closed poison on multi-part `ResourceLimit` (URI length, fragment budgets)
- Progress helpers: `resolved_fragment_count`, `fragment_count`, `is_poisoned`
- `no_std` + `alloc` support (`--no-default-features`)
- ur-rs / bc-ur interop vectors (unit + integration tests)
- Criterion benches (`bytewords`, `fountain`, `ur_roundtrip`)
- Example: `multipart_progress`
- Quality gates: `just quality` (fmt, clippy `-D warnings`, test, no_std, deny)

### Notes

- Default limit integers were not frozen in this release
- Feature `dcbor` is a stub for the planned 0.2 typed layer
- Feature `bytemoji` is reserved (not implemented in 0.1)

## [0.0.1] — 2026-07-22

Workspace scaffold and experimental development.

[1.0.0]: https://github.com/qntx/bcur/releases/tag/v1.0.0
[0.3.0]: https://github.com/qntx/bcur/releases/tag/v0.3.0
[0.2.0]: https://github.com/qntx/bcur/releases/tag/v0.2.0
[0.1.0]: https://github.com/qntx/bcur/releases/tag/v0.1.0
[0.0.1]: https://github.com/qntx/bcur/releases/tag/v0.0.1
