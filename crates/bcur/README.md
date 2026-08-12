# bcur

A Rust implementation of [Uniform Resources](https://github.com/BlockchainCommons/Research/blob/master/papers/bcr-2020-005-ur.md) (URs).

URs encode binary payloads as URI-friendly strings optimized for QR codes and unreliable channels, using [bytewords](https://github.com/BlockchainCommons/Research/blob/master/papers/bcr-2020-012-bytewords.md) and fountain codes for multi-part transfer.

## Features

| Feature | Default | Description |
|---------|---------|-------------|
| `std` | yes | Host / std builds |
| `dcbor` | no | Typed dCBOR layer (implies `std`) |
| `bytemoji` | no | Bytemoji helpers |

`no_std` + `alloc` is supported with `--no-default-features`.

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

## License

MIT OR Apache-2.0
