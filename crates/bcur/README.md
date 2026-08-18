# bcur

A Rust implementation of [Uniform Resources](https://github.com/BlockchainCommons/Research/blob/master/papers/bcr-2020-005-ur.md) (URs).

URs encode binary payloads as URI-friendly strings for QR codes and unreliable channels, using [bytewords](https://github.com/BlockchainCommons/Research/blob/master/papers/bcr-2020-012-bytewords.md) and fountain codes for multi-part transfer.

## Status (0.2)

Transport: bytewords + fountain + multi-part UR + `DecoderLimits`.  
Typed dCBOR: `feature = "dcbor"` (`Ur`, traits, multipart wrappers; implies `std`).

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
