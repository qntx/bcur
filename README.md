# bcur

A Rust implementation of [Uniform Resources](https://github.com/BlockchainCommons/Research/blob/master/papers/bcr-2020-005-ur.md) (URs).

URs encode binary data as URI-friendly strings for QR codes and unreliable channels, using [bytewords](https://github.com/BlockchainCommons/Research/blob/master/papers/bcr-2020-012-bytewords.md) and fountain codes.

## Status

**0.1.0 — transport layer.** Bytewords, fountain, multi-part UR, decoder resource limits, and interop goldens (ur-rs / bc-ur vectors). Wire-compatible with common BCR UR stacks for the transport surface.

Typed dCBOR (`feature = "dcbor"`) is planned for **0.2**. Bytemoji is reserved.

Design notes: [`docs/design/bcur-ur-design.md`](docs/design/bcur-ur-design.md)  
Changelog: [`CHANGELOG.md`](CHANGELOG.md)

## Usage

```toml
[dependencies]
bcur = "0.1"
```

```rust
use bcur::{Decoder, Encoder};

let data = b"Ten chars!".repeat(10);
let mut encoder = Encoder::bytes(&data, 5).unwrap();
let mut decoder = Decoder::default();
while !decoder.complete() {
    let part = encoder.next_part().unwrap();
    decoder.receive(&part).unwrap();
}
assert_eq!(decoder.message().unwrap().as_deref(), Some(data.as_slice()));
```

### Progress and limits

```rust
use bcur::{Decoder, DecoderLimits, Encoder};

let limits = DecoderLimits {
    max_uri_len: 4096,
    ..DecoderLimits::default()
};
let mut decoder = Decoder::with_limits(limits);
// after receive:
// decoder.resolved_fragment_count(), decoder.fragment_count(), decoder.is_poisoned()
```

Default limit numbers are experimental before 1.0; hosts with fixed budgets should set `DecoderLimits` explicitly.

## Features

| Feature | Default | Description |
|---------|---------|-------------|
| `std` | yes | Host builds |
| `dcbor` | no | Typed dCBOR layer (**stub** until 0.2) |
| `bytemoji` | no | Bytemoji helpers (**planned**) |

`no_std` + `alloc`:

```bash
cargo check -p bcur --target thumbv7m-none-eabi --no-default-features
```

## Development

```bash
just quality          # fmt-check, clippy -D warnings, test, no_std, cargo-deny
just bench-check      # compile criterion benches
cargo run -p bcur --example multipart_progress
```

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project shall be dual-licensed as above, without any additional terms or conditions.

---

<div align="center">

A **[QuantX](https://qntx.fun)** open-source project.

<a href="https://qntx.fun"><img alt="QuantX" width="369" src="https://raw.githubusercontent.com/qntx/.github/main/profile/qntx.svg" /></a>

Code is law. We write both.

</div>
