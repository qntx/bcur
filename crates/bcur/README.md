# bcur

A Rust implementation of [Uniform Resources](https://github.com/BlockchainCommons/Research/blob/master/papers/bcr-2020-005-ur.md) (URs).

URs encode binary payloads as URI-friendly strings for QR codes and unreliable channels, using [bytewords](https://github.com/BlockchainCommons/Research/blob/master/papers/bcr-2020-012-bytewords.md) and fountain codes for multi-part transfer.

## Status (0.3)

Transport: bytewords + fountain + multi-part UR + `DecoderLimits`.  
Typed dCBOR: `feature = "dcbor"` (`Ur`, traits, multipart wrappers; implies `std`).

## Layering

**L0–L3 (always built).** A UR type token is a validated label (`[a-z0-9-]+` after ASCII lowercasing). The body is raw bytes plus bytewords CRC. `ur::encode` / `ur::Encoder` do **not** parse or require CBOR. `UrType::bytes()` and `Encoder::bytes` exist so tests and generic hosts can move untyped payloads. This is an intentional split, not an accident, and it matches ur-rs.

**BCR-2020-005** says a UR *message* MUST be dCBOR and that type `bytes` MUST NOT be used except for testing. That MUST is enforced on **L4** (`feature = "dcbor"`): `typed::Ur::from_ur_string` and `MultipartDecoder::message` reject non-dCBOR (`Error::Cbor`). L4 also uses the first registered `dcbor` tag **name** as the type token and strips the tag from the UR body (005 "top-level UR is untagged").

**This crate will not** grow a Blockchain Commons type registry, Envelope, or PSBT module to "satisfy 005." Application types belong in a consumer crate that implements `UrEncodable` / `UrDecodable`.

## Features

| Feature | Default | Description |
| --------- | --------- | ------------- |
| `std` | yes | Host / std builds |
| `dcbor` | no | Typed `Ur` / traits / multipart wrappers (implies `std`) |

`no_std` + `alloc`: `--no-default-features`.

## Quick start

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

Typed (`feature = "dcbor"`):

```rust
use bcur::Ur;

let ur = Ur::new("test", vec![1, 2, 3]).unwrap();
assert_eq!(ur.string(), "ur:test/lsadaoaxjygonesw");
```

## License

MIT OR Apache-2.0
