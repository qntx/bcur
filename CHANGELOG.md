# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
- `DecoderLimits::default` numbers remain experimental until 1.0
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

- `DecoderLimits::default` numeric values are **experimental** until 1.0
- Feature `dcbor` is a stub for the planned 0.2 typed layer
- Feature `bytemoji` is reserved (not implemented in 0.1)

## [0.0.1] — 2026-07-22

Workspace scaffold and experimental development.

[0.2.0]: https://github.com/qntx/bcur/releases/tag/v0.2.0
[0.1.0]: https://github.com/qntx/bcur/releases/tag/v0.1.0
[0.0.1]: https://github.com/qntx/bcur/releases/tag/v0.0.1
