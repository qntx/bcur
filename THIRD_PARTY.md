# Third-party notices

## ur-rs (MIT)

Golden test vectors were taken from or checked against
[ur-rs](https://github.com/dspicher/ur-rs) 0.5.x (MIT License):

- multipart UR strings (`crates/bcur/tests/vectors/ur_rs_multipart_20.txt`)
- fountain partition hex (`crates/bcur/tests/vectors/wolf256_fragments.hex`)
- crypto-request CBOR fixture
- `choose_fragments` tables (`crates/bcur/tests/vectors/ur_rs_choose_fragments.txt`)
- RNG sequences (kept as small `u64` arrays in unit tests)

bcur reimplements the algorithms; it does not vendor ur-rs source modules.
When a test table is substantially identical to an ur-rs test, this notice
applies.

## Published specification examples

These strings are public examples from BCR papers and bc-ur documentation,
not source code. They live in
`crates/bcur/tests/vectors/published_single.txt`:

- `ur:test/lsadaoaxjygonesw`
- `ur:bytes/iehsjyhspmwfwfia`
- Wolf/50 `ur:bytes/hdey…`
- bytewords `able acid also lava zoom…`

They are used as interop goldens.

## Pinned-reference quoted literals

`crates/bcur/tests/vectors/published_from_refs.txt` is the sorted unique set
of quoted `ur:…` string literals extracted from pinned URKit / bc-ur **test
files** (not source). Weekly `.github/workflows/vectors-weekly.yml` diffs
that set. It is drift detection, not BCR-2020-005 compliance.

## Not copied

This repository does **not** contain source from:

- [URKit](https://github.com/BlockchainCommons/URKit) (BSD-2-Clause-Patent)
- [bc-ur C++](https://github.com/BlockchainCommons/bc-ur) (BSD-2-Clause-Patent)
- [bc-ur Rust](https://github.com/BlockchainCommons/bc-ur-rust) (BSD-2-Clause-Patent)

Reference implementations are oracles for published strings only.
