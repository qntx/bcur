# bcur

A Rust implementation of [Uniform Resources](https://github.com/BlockchainCommons/Research/blob/master/papers/bcr-2020-005-ur.md) (URs).

URs encode binary payloads as URI-friendly strings for QR codes and unreliable channels, using [bytewords](https://github.com/BlockchainCommons/Research/blob/master/papers/bcr-2020-012-bytewords.md) and fountain codes for multi-part transfer.

## Status (0.1)

Transport only: bytewords + fountain + multi-part UR + `DecoderLimits`.  
Typed dCBOR is a 0.2 feature stub; bytemoji is planned.

## Features

| Feature | Default | Description |
|---------|---------|-------------|
| `std` | yes | Host / std builds |
| `dcbor` | no | Typed dCBOR layer (stub until 0.2; implies `std`) |
| `bytemoji` | no | Bytemoji helpers (planned) |

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

## License

MIT OR Apache-2.0
