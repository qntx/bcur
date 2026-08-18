# Transport contracts

Normative host-visible behavior of `bcur` L0–L3 (and the `dcbor` typed wrappers
that forward to them). This file matches the code it landed with. It does not
describe planned 0.3 work that is not in tree.

## `K == 1`: UR vs fountain

BCR-2024-001’s “error on a second `nextPart`” applies to the **fountain**
encoder. The UR string encoder is a different type.

### `ur::Encoder` (`K == 1`)

- `is_single_part()` is true when fountain `K == 1`.
- `next_part()` emits `ur:<type>/<minimal-bytewords>` of the **message bytes**.
  It does **not** emit `ur:<type>/1-1/…`. It does **not** call
  `fountain::Encoder::next_part`.
- Every later `next_part()` returns the **same** string. CLI `--animate` stays
  correct; status `seq=` stays at `1`.
- `current_index()` is `0` before the first emit, then `1`. It does not track
  an inner fountain `seqNum`. Interop with URKit/bc-ur is the **emitted UR
  string**, not `current_index`.
- `complete()` is `false` before the first emit and `true` after.
- `MultipartEncoder::complete()` forwards to `ur::Encoder::complete()`.

### `fountain::Encoder` (`K == 1`)

- First `next_part()` returns `Ok` (simple part, `seqNum == 1`).
- Second `next_part()` returns `Error::SinglePartExhausted`.
- URKit debug-asserts; this crate returns `Result` and never asserts.

### Decode of foreign `1-1`

Our encoder no longer emits `1-1`. `ur::Decoder::receive` still accepts a
fountain URI `ur:<type>/1-1/<body>` built from a one-fragment `Part`. That is
interop with older or foreign senders, not a compatibility path in *our*
encoder. `receive_fountain` does not special-case `K == 1`.

## `seqNum` does not wrap

Fountain `seqNum` is a 1-based `u32`. Increment is
`fountain::next_sequence(current)`:

- `current == u32::MAX` → `Error::ResourceLimit(ResourceKind::Sequence)`
- otherwise `current + 1`

There is no wrap. URKit and bc-ur C++ wrap (`&+= 1` / `+= 1`). A wrapped `0`
is `Error::InvalidSequence` on this decoder. Fail-closed is the contract.

The wrap case is not looped at runtime in tests (`u32::MAX` steps). The
predicate is unit-tested via `next_sequence(u32::MAX)`.

## Type pin

`ur::Decoder::receive`:

- The first **successfully ingested** part pins `ur_type()`.
- Failed parse, bytewords, indices, part CBOR, or fountain ingest does **not**
  pin. Garbage such as `ur:beta/1-2/zzzz` leaves `ur_type()` as `None`.
- A later part with a different type is `Error::UnexpectedType`. That is **not**
  poison; the session stays usable for the pinned type.
- `with_expected_type` rejects a mismatch before ingest (also
  `UnexpectedType`, not poison).
- Mixing a completed single-part payload with a fountain part (or the reverse)
  is `Error::InconsistentPart`.

## Poison

Crate-private (not re-exported):

```text
enum Poison {
    Limit(ResourceKind),
    DecoderState,
}
```

Stored on both `fountain::Decoder` and `ur::Decoder`. Public `is_poisoned()`
is `bool`.

| Trigger | Stored | Later `receive` / `message` |
|---------|--------|------------------------------|
| `Error::ResourceLimit(kind)` | `Poison::Limit(kind)` | same `ResourceLimit(kind)` |
| `Error::DecoderState` | `Poison::DecoderState` | `DecoderState` |

Other errors (`UnexpectedType`, `InvalidIndices`, `InvalidWord`,
`SinglePartExhausted`, …) do **not** poison.

`ur::Decoder::is_poisoned()` is true if either the UR session or the inner
fountain decoder is poisoned.

## `DecoderLimits`

Default integers are **experimental until 1.0**. They are a desktop fail-closed
ceiling, not a QR-version table. Embedded and tight hosts must call
`Decoder::with_limits`.

| Field | Default | Role |
|-------|---------|------|
| `max_message_length` | `1_048_576` (1 MiB) | Original payload cap |
| `max_fragment_count` | `2_000` | `K` / `sequence_count` |
| `max_fragment_data_length` | `8_192` | `part.data.len()` and Part CBOR bstr |
| `max_buffer_parts` | `4_000` | Mixed-part XOR map |
| `max_received_parts` | `8_000` | Unique index-set set |
| `max_uri_len` | `8_192` | `ur::Decoder::receive` ASCII length |

Both `max_message_length` and `K ×` fragment size apply; the tighter one wins.
`Part::from_cbor` / `from_cbor_with_max` also apply `max_fragment_count` /
`max_fragment_data_length` before the fountain decoder sees the part.

CLI decode uses `DecoderLimits::default()`.

## 1.0 freeze

1.0 is **API + `DecoderLimits::default` freeze**, not a Blockchain Commons
product clone.

Freeze at 1.0 (not before):

- Public types and `Error` variants (including `ResourceKind`).
- `DecoderLimits::default` integers above. Change them only in a 0.x breaking
  release if fuzz/CVE forces a tighten.
- MSRV (currently 1.86).
- Feature set: `std` (default), `dcbor` (implies `std`). No empty features.

Soak before tagging 1.0: `bcur` 0.3.0 on crates.io and `fuzz-nightly` clean
for **14 days** with no decoder correctness issues. Do not start a 1.0 PR
during 0.3.

Out of crate scope through 1.0: Gordian Envelope, BCR-2020-006 registry, PSBT,
SSKR, Bytemoji, camera, PNG/GIF, `firstSeqNum`, `minFragmentLen`.
