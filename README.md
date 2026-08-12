# bcur

A Rust implementation of [Uniform Resources](https://github.com/BlockchainCommons/Research/blob/master/papers/bcr-2020-005-ur.md) (URs).

URs encode binary data as URI-friendly strings for QR codes and unreliable channels, using [bytewords](https://github.com/BlockchainCommons/Research/blob/master/papers/bcr-2020-012-bytewords.md) and fountain codes.

## Status

Transport layer (bytewords + fountain + multi-part UR) is implemented and wire-compatible with [ur-rs](https://github.com/dspicher/ur-rs) and [bc-ur](https://github.com/BlockchainCommons/bc-ur-rust) test vectors. Typed dCBOR layer is planned for 0.2.

Design notes: [`docs/design/bcur-ur-design.md`](docs/design/bcur-ur-design.md)

## Usage

```toml
[dependencies]
bcur = { path = "crates/bcur" } # or version from crates.io when published
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

## Features

| Feature | Default | Description |
| --------- | --------- | ------------- |
| `std` | yes | Host builds |
| `dcbor` | no | Typed dCBOR layer (placeholder until 0.2) |
| `bytemoji` | no | Bytemoji helpers (planned) |

`no_std` + `alloc`: `cargo check -p bcur --target thumbv7m-none-eabi --no-default-features`

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
