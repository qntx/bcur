# bcur-cli

Command-line encoder/decoder for [Uniform Resources](https://github.com/BlockchainCommons/Research/blob/master/papers/bcr-2020-005-ur.md). Wraps the `bcur` transport crate. Renders single-part and fountain URs as terminal QR codes for air-gapped transfer.

Not a wallet. Not a camera scanner. Not a PNG/GIF studio (`bc-mur` / `mur` covers that).

## Install

```bash
cargo install --path crates/bcur-cli
# or, once published:
# cargo install bcur-cli
```

Binary name: **`bcur`**.

## Usage

```bash
# single-part UR on stdout
printf 'abc' | bcur encode --type test

# published golden (CBOR array [1,2,3])
printf '83010203' | bcur encode --type test --hex
# ur:test/lsadaoaxjygonesw

# terminal QR (Ctrl-C or q to stop animation)
bcur encode --qr --type bytes payload.bin

# Encoder path + QR loop (still single-part if the payload fits one fragment)
bcur encode --animate --qr payload.bin
# force fountain frames: shrink the per-URI budget
bcur encode --animate --qr --max-chars 80 payload.bin

# recover bytes from captured UR lines
bcur decode --out recovered.bin < parts.txt

# render already-encoded UR lines
bcur qr parts.txt
```

`--type` is only the UR type token. Payload bytes are sent unchanged — there is no automatic CBOR `bstr` wrap.

## Defaults

| Flag | Default | Role |
|------|---------|------|
| `--type` | `bytes` | Type token |
| `--max-chars` | `400` text / terminal-derived with `--qr` | Single vs fountain; fragment sizing |
| `--interval-ms` | `200` | Animated QR frame time |
| QR ECC | Quartile (Q) | Screen scan robustness |

`--max-fragment` overrides the auto fragment size. Text fountain output emits `max(3K, 20)` parts unless `--count` is set.

## License

MIT OR Apache-2.0
