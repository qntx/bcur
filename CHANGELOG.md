# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.1.0]: https://github.com/qntx/bcur/releases/tag/v0.1.0
[0.0.1]: https://github.com/qntx/bcur/releases/tag/v0.0.1
