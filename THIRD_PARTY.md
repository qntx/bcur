# Third-party notices

## ur-rs (MIT)

Golden test vectors were taken from or checked against
[ur-rs](https://github.com/dspicher/ur-rs) 0.5.x (MIT License):

- multipart UR strings
- fountain partition hex
- crypto-request CBOR fixture
- `choose_fragments` tables
- RNG sequences

bcur reimplements the algorithms; it does not vendor ur-rs source modules.
When a test table is substantially identical to an ur-rs test, this notice
applies.

## Published specification examples

These strings are public examples from BCR papers and bc-ur documentation,
not source code:

- `ur:test/lsadaoaxjygonesw`
- bytewords `able acid also lava zoom…`

They are used as interop goldens.

## Not copied

This repository does **not** contain source from:

- [URKit](https://github.com/BlockchainCommons/URKit) (BSD-2-Clause-Patent)
- [bc-ur C++](https://github.com/BlockchainCommons/bc-ur) (BSD-2-Clause-Patent)
- [bc-ur Rust](https://github.com/BlockchainCommons/bc-ur-rust) (BSD-2-Clause-Patent)

Reference implementations are oracles for published strings only.
