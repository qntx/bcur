//! Fountain (Luby-transform style) encoder and decoder for multi-part URs.
//!
//! ```
//! let data = b"Ten chars!";
//! let mut encoder = bcur::fountain::Encoder::new(data, 4).unwrap();
//! let mut decoder = bcur::fountain::Decoder::default();
//! while !decoder.complete() {
//!     let part = encoder.next_part().unwrap();
//!     decoder.receive(part).unwrap();
//! }
//! assert_eq!(decoder.message().unwrap().as_deref(), Some(data.as_slice()));
//! ```

mod part_cbor;

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};

use crate::crc32;
use crate::error::Poison;
use crate::rng::Xoshiro256;
use crate::{Error, ResourceKind, Result};

/// Hard limits for adversarial multi-part streams.
///
/// Default integers are **experimental until 1.0**. They stay at the 0.2
/// values unless fuzz or a CVE forces a tighten before freeze. Hosts that
/// need a fixed or embedded budget must use [`Decoder::with_limits`].
///
/// These caps are a desktop fail-closed ceiling, not a QR-version table.
///
/// | Field | Default | Role |
/// |-------|---------|------|
/// | `max_message_length` | `1_048_576` (1 MiB) | Original payload cap |
/// | `max_fragment_count` | `2_000` | `K` / `sequence_count` |
/// | `max_fragment_data_length` | `8_192` | `part.data.len()` and `Part` CBOR bstr |
/// | `max_buffer_parts` | `4_000` | Mixed-part XOR map |
/// | `max_received_parts` | `8_000` | Unique index-set set |
/// | `max_uri_len` | `8_192` | `ur::Decoder::receive` ASCII length |
///
/// CLI payloads are uppercase UR and fit QR alphanumeric mode. ISO/IEC 18004
/// Table 7, version 40, alphanumeric Q capacity is 2420 characters.
/// `max_uri_len` = 8192 is a string-API `DoS` bound, above any single QR
/// (including alphanumeric L 4296).
///
/// At `--max-chars 400`, a part body is ~180 decoded bytes, so `K = 2000`
/// admits ~360 KiB, below `max_message_length`. Both caps apply; the tighter
/// one wins. [`Part::from_cbor`] / [`Part::from_cbor_with_max`] also apply
/// `max_fragment_count` / `max_fragment_data_length` before the fountain
/// decoder sees the part.
///
/// [`Self::worst_case_heap_bytes`] is a **cap-product ceiling excluding
/// allocator/BTree overhead**, not a conservative RSS figure. Independent
/// caps overestimate reachable payload heap (`K=2000` × 8 KiB is
/// [`Error::InconsistentPart`] against `max_message_length=1 MiB`).
/// `BTree`/allocator costs underestimate process RSS for a given cap tuple.
///
/// ```text
/// decoded  = min(max_message_length + max_fragment_data_length,
///                max_fragment_count * max_fragment_data_length)
/// buffer   = max_buffer_parts * (max_fragment_data_length
///            + max_fragment_count * size_of::<usize>())
/// received = max_received_parts * max_fragment_count * size_of::<usize>()
/// total    = decoded + buffer + received
/// ```
///
/// All multiplies and adds saturate at [`usize::MAX`].
///
/// On 64-bit (`size_of::<usize>() == 8`) [`Default`] is **`225_824_768`
/// (≈ 215 MiB)**. On 32-bit the same integers yield **`129_824_768`
/// (≈ 124 MiB)**. An embedded target still must call
/// [`Decoder::with_limits`] — 124 MiB is not an embedded budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecoderLimits {
    /// Max original message length in bytes.
    pub max_message_length: usize,
    /// Max fragment count `K` (`sequence_count`).
    pub max_fragment_count: usize,
    /// Max `part.data.len()` on every part.
    pub max_fragment_data_length: usize,
    /// Max entries in the complex-part XOR buffer.
    pub max_buffer_parts: usize,
    /// Max unique index-sets recorded in `received`.
    pub max_received_parts: usize,
    /// Max UR string length accepted by `ur::Decoder::receive` (ASCII bytes).
    pub max_uri_len: usize,
}

impl Default for DecoderLimits {
    fn default() -> Self {
        Self {
            max_message_length: 1_048_576,
            max_fragment_count: 2_000,
            max_fragment_data_length: 8_192,
            max_buffer_parts: 4_000,
            max_received_parts: 8_000,
            max_uri_len: 8_192,
        }
    }
}

impl DecoderLimits {
    /// Cap-product ceiling of the public caps, excluding allocator/`BTree`
    /// overhead. Saturates at [`usize::MAX`].
    ///
    /// 64-bit [`Default`] is `225_824_768` (≈ 215 MiB). 32-bit [`Default`] is
    /// `129_824_768` (≈ 124 MiB). This is not process RSS. `max_uri_len` is a
    /// receive-length bound and is not included in the product.
    #[must_use]
    pub const fn worst_case_heap_bytes(&self) -> usize {
        let usz = size_of::<usize>();
        let decoded_a = self
            .max_message_length
            .saturating_add(self.max_fragment_data_length);
        let decoded_b = self
            .max_fragment_count
            .saturating_mul(self.max_fragment_data_length);
        let decoded = if decoded_a < decoded_b {
            decoded_a
        } else {
            decoded_b
        };
        let per_buffer = self
            .max_fragment_data_length
            .saturating_add(self.max_fragment_count.saturating_mul(usz));
        let buffer = self.max_buffer_parts.saturating_mul(per_buffer);
        let received = self
            .max_received_parts
            .saturating_mul(self.max_fragment_count)
            .saturating_mul(usz);
        decoded.saturating_add(buffer).saturating_add(received)
    }
}

/// Fountain encoder.
#[derive(Debug)]
pub struct Encoder {
    parts: Vec<Vec<u8>>,
    sequence_count: u32,
    message_length: u32,
    checksum: u32,
    current_sequence: u32,
}

impl Encoder {
    /// Constructs an encoder for `message` with a maximum fragment length.
    ///
    /// # Errors
    ///
    /// Returns an error if the message is empty, `max_fragment_length` is zero,
    /// or sizes cannot fit `u32` wire fields.
    pub fn new(message: &[u8], max_fragment_length: usize) -> Result<Self> {
        if message.is_empty() {
            return Err(Error::EmptyMessage);
        }
        if max_fragment_length == 0 {
            return Err(Error::InvalidFragmentLen);
        }
        let message_length = u32::try_from(message.len())
            .map_err(|_| Error::ResourceLimit(ResourceKind::MessageLength))?;
        let frag_len = fragment_length(message.len(), max_fragment_length);
        let fragments = partition(message.to_vec(), frag_len);
        let sequence_count = u32::try_from(fragments.len())
            .map_err(|_| Error::ResourceLimit(ResourceKind::FragmentCount))?;
        Ok(Self {
            parts: fragments,
            sequence_count,
            message_length,
            checksum: crc32::checksum(message),
            current_sequence: 0,
        })
    }

    /// Number of parts already emitted.
    #[must_use]
    pub const fn current_sequence(&self) -> u32 {
        self.current_sequence
    }

    /// Number of source fragments `K`.
    #[must_use]
    pub const fn fragment_count(&self) -> u32 {
        self.sequence_count
    }

    /// Whether all original segments have been emitted at least once.
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.current_sequence >= self.sequence_count
    }

    /// Emits the next fountain part.
    ///
    /// # Errors
    ///
    /// [`Error::SinglePartExhausted`] if `K == 1` and a part was already
    /// emitted. [`Error::ResourceLimit`] ([`ResourceKind::Sequence`]) if the
    /// sequence would exceed `u32::MAX`.
    pub fn next_part(&mut self) -> Result<Part> {
        if self.sequence_count == 1 && self.current_sequence >= 1 {
            return Err(Error::SinglePartExhausted);
        }
        self.current_sequence = next_sequence(self.current_sequence)?;
        let indexes = choose_fragments(
            self.current_sequence as usize,
            self.parts.len(),
            self.checksum,
        );
        let first = self.parts.first().ok_or(Error::DecoderState)?;
        let mut mixed = alloc::vec![0_u8; first.len()];
        for item in indexes {
            let fragment = self.parts.get(item).ok_or(Error::DecoderState)?;
            xor(&mut mixed, fragment)?;
        }
        Ok(Part {
            sequence: self.current_sequence,
            sequence_count: self.sequence_count,
            message_length: self.message_length,
            checksum: self.checksum,
            data: mixed,
        })
    }
}

/// Next 1-based fountain `seqNum`. Does not wrap.
pub(crate) const fn next_sequence(current: u32) -> Result<u32> {
    if current == u32::MAX {
        return Err(Error::ResourceLimit(ResourceKind::Sequence));
    }
    Ok(current + 1)
}

/// Fountain decoder with resource limits and fail-closed poison.
#[derive(Debug)]
pub struct Decoder {
    decoded: BTreeMap<usize, Part>,
    received: BTreeSet<Vec<usize>>,
    buffer: BTreeMap<Vec<usize>, Part>,
    queue: Vec<(usize, Part)>,
    sequence_count: u32,
    message_length: usize,
    checksum: u32,
    fragment_length: usize,
    limits: DecoderLimits,
    poisoned: Option<Poison>,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder {
    /// Creates a decoder with default limits.
    #[must_use]
    pub fn new() -> Self {
        Self::with_limits(DecoderLimits::default())
    }

    /// Creates a decoder with custom limits.
    #[must_use]
    pub const fn with_limits(limits: DecoderLimits) -> Self {
        Self {
            decoded: BTreeMap::new(),
            received: BTreeSet::new(),
            buffer: BTreeMap::new(),
            queue: Vec::new(),
            sequence_count: 0,
            message_length: 0,
            checksum: 0,
            fragment_length: 0,
            limits,
            poisoned: None,
        }
    }

    /// Whether a previous [`Error::ResourceLimit`] or [`Error::DecoderState`]
    /// poisoned this decoder.
    #[must_use]
    pub const fn is_poisoned(&self) -> bool {
        self.poisoned.is_some()
    }

    const fn poison(&mut self, kind: ResourceKind) -> Error {
        let poison = Poison::Limit(kind);
        self.poisoned = Some(poison);
        poison.to_error()
    }

    fn escalate(&mut self, err: Error) -> Error {
        match err {
            Error::ResourceLimit(kind) => self.poison(kind),
            Error::DecoderState => {
                self.poisoned = Some(Poison::DecoderState);
                Poison::DecoderState.to_error()
            }
            other => other,
        }
    }

    /// Receives a fountain part.
    ///
    /// # Errors
    ///
    /// Returns an error if the part is invalid, inconsistent, or exceeds limits.
    pub fn receive(&mut self, part: Part) -> Result<bool> {
        if let Some(poison) = self.poisoned {
            return Err(poison.to_error());
        }
        if self.complete() {
            return Ok(false);
        }

        if part.sequence_count == 0 || part.data.is_empty() || part.message_length == 0 {
            return Err(Error::EmptyPart);
        }
        if part.sequence == 0 {
            return Err(Error::InvalidSequence);
        }
        if part.data.len() > self.limits.max_fragment_data_length {
            return Err(self.poison(ResourceKind::FragmentData));
        }

        if self.received.is_empty() {
            let sc = part.sequence_count;
            let sc_usz = sc as usize;
            let ml = part.message_length as usize;
            if sc_usz > self.limits.max_fragment_count {
                return Err(self.poison(ResourceKind::FragmentCount));
            }
            if ml > self.limits.max_message_length {
                return Err(self.poison(ResourceKind::MessageLength));
            }
            let frag_len = part.data.len();
            let product = frag_len
                .checked_mul(sc_usz)
                .ok_or_else(|| self.poison(ResourceKind::MessageLength))?;
            // Partition pads with at most `frag_len - 1` bytes.
            if product < ml || product - ml >= frag_len {
                return Err(Error::InconsistentPart);
            }
            self.sequence_count = sc;
            self.message_length = ml;
            self.checksum = part.checksum;
            self.fragment_length = frag_len;
        } else if !self.validate(&part) {
            return Err(Error::InconsistentPart);
        }

        let indexes = part.indexes();
        if self.received.contains(&indexes) {
            return Ok(false);
        }
        if self.received.len() >= self.limits.max_received_parts {
            return Err(self.poison(ResourceKind::ReceivedParts));
        }
        self.received.insert(indexes);
        if part.is_simple() {
            self.enqueue_simple(part).map_err(|e| self.escalate(e))?;
        } else {
            self.process_complex(part).map_err(|e| self.escalate(e))?;
        }
        // Always drain the reduction queue after ingest so that a complex part
        // reduced to a simple fragment still cascades into the XOR buffer.
        self.process_queue().map_err(|e| self.escalate(e))?;
        Ok(true)
    }

    fn enqueue_simple(&mut self, part: Part) -> Result<()> {
        let index = *part.indexes().first().ok_or(Error::DecoderState)?;
        if self.decoded.contains_key(&index) {
            return Ok(());
        }
        self.decoded.insert(index, part.clone());
        self.queue.push((index, part));
        Ok(())
    }

    fn process_queue(&mut self) -> Result<()> {
        while let Some((index, simple)) = self.queue.pop() {
            let to_process: Vec<Vec<usize>> = self
                .buffer
                .keys()
                .filter(|&idxs| idxs.contains(&index))
                .cloned()
                .collect();
            for indexes in to_process {
                self.reduce_buffered_part(indexes, index, &simple)?;
            }
        }
        Ok(())
    }

    fn reduce_buffered_part(
        &mut self,
        indexes: Vec<usize>,
        known_index: usize,
        simple: &Part,
    ) -> Result<()> {
        let mut part = self.buffer.remove(&indexes).ok_or(Error::DecoderState)?;
        let mut new_indexes = indexes;
        let to_remove = new_indexes
            .iter()
            .position(|&x| x == known_index)
            .ok_or(Error::DecoderState)?;
        new_indexes.remove(to_remove);
        xor(&mut part.data, &simple.data)?;
        self.insert_reduced(new_indexes, part)
    }

    fn process_complex(&mut self, mut part: Part) -> Result<()> {
        let mut indexes = part.indexes();
        let known: Vec<usize> = indexes
            .iter()
            .copied()
            .filter(|idx| self.decoded.contains_key(idx))
            .collect();
        if indexes.len() == known.len() {
            return Ok(());
        }
        for remove in known {
            let pos = indexes
                .iter()
                .position(|&x| x == remove)
                .ok_or(Error::DecoderState)?;
            indexes.remove(pos);
            xor(
                &mut part.data,
                &self.decoded.get(&remove).ok_or(Error::DecoderState)?.data,
            )?;
        }
        self.insert_reduced(indexes, part)
    }

    fn insert_reduced(&mut self, indexes: Vec<usize>, part: Part) -> Result<()> {
        if indexes.len() == 1 {
            let idx = *indexes.first().ok_or(Error::DecoderState)?;
            if self.decoded.contains_key(&idx) {
                return Ok(());
            }
            self.decoded.insert(idx, part.clone());
            self.queue.push((idx, part));
            return Ok(());
        }
        // Replacing an existing index-set does not grow the map; only count new keys.
        if !self.buffer.contains_key(&indexes) && self.buffer.len() >= self.limits.max_buffer_parts
        {
            return Err(self.poison(ResourceKind::BufferParts));
        }
        self.buffer.insert(indexes, part);
        Ok(())
    }

    /// Max fragment data length configured for this decoder.
    #[must_use]
    pub const fn max_fragment_data_length(&self) -> usize {
        self.limits.max_fragment_data_length
    }

    /// Max source fragment count `K` configured for this decoder.
    #[must_use]
    pub const fn max_fragment_count(&self) -> usize {
        self.limits.max_fragment_count
    }

    /// Max original message length configured for this decoder.
    #[must_use]
    pub const fn max_message_length(&self) -> usize {
        self.limits.max_message_length
    }

    /// Whether all source fragments have been recovered.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.message_length != 0 && self.decoded.len() == self.sequence_count as usize
    }

    /// Number of resolved source fragments, or `None` before any part.
    #[must_use]
    pub fn resolved_fragment_count(&self) -> Option<u32> {
        if self.message_length == 0 {
            None
        } else {
            debug_assert!(
                u32::try_from(self.decoded.len()).is_ok(),
                "decoded fragment count exceeds u32 (impossible under DecoderLimits)"
            );
            u32::try_from(self.decoded.len()).ok()
        }
    }

    /// Total fragment count `K`, or `0` before any part.
    #[must_use]
    pub const fn fragment_count(&self) -> u32 {
        self.sequence_count
    }

    /// Whether `part` is consistent with previously received metadata.
    #[must_use]
    pub fn validate(&self, part: &Part) -> bool {
        if self.received.is_empty() {
            return false;
        }
        part.sequence_count == self.sequence_count
            && part.message_length as usize == self.message_length
            && part.checksum == self.checksum
            && part.data.len() == self.fragment_length
    }

    /// Returns the decoded message if complete.
    ///
    /// # Errors
    ///
    /// Returns padding or checksum errors if the joined payload is invalid.
    pub fn message(&self) -> Result<Option<Vec<u8>>> {
        if let Some(poison) = self.poisoned {
            return Err(poison.to_error());
        }
        if !self.complete() {
            return Ok(None);
        }
        let k = self.sequence_count as usize;
        let mut combined = Vec::with_capacity(self.fragment_length * k);
        for idx in 0..k {
            let part = self.decoded.get(&idx).ok_or(Error::DecoderState)?;
            combined.extend_from_slice(&part.data);
        }
        if !combined
            .get(self.message_length..)
            .ok_or(Error::DecoderState)?
            .iter()
            .all(|&x| x == 0)
        {
            return Err(Error::InvalidPadding);
        }
        let message = combined
            .get(..self.message_length)
            .ok_or(Error::DecoderState)?
            .to_vec();
        if crc32::checksum(&message) != self.checksum {
            return Err(Error::InvalidMessageChecksum);
        }
        Ok(Some(message))
    }
}

/// A fountain part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    sequence: u32,
    sequence_count: u32,
    message_length: u32,
    checksum: u32,
    data: Vec<u8>,
}

impl Part {
    pub(crate) const fn from_fields(
        sequence: u32,
        sequence_count: u32,
        message_length: u32,
        checksum: u32,
        data: Vec<u8>,
    ) -> Self {
        Self {
            sequence,
            sequence_count,
            message_length,
            checksum,
            data,
        }
    }

    /// Sequence number (1-based).
    #[must_use]
    pub const fn sequence(&self) -> u32 {
        self.sequence
    }

    /// Total number of source fragments.
    #[must_use]
    pub const fn sequence_count(&self) -> u32 {
        self.sequence_count
    }

    /// Original message length in bytes.
    #[must_use]
    pub const fn message_length(&self) -> u32 {
        self.message_length
    }

    /// CRC-32 of the original message.
    #[must_use]
    pub const fn checksum(&self) -> u32 {
        self.checksum
    }

    /// Part payload bytes (possibly XOR-mixed).
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Indexes of source fragments mixed into this part.
    #[must_use]
    pub fn indexes(&self) -> Vec<usize> {
        choose_fragments(
            self.sequence as usize,
            self.sequence_count as usize,
            self.checksum,
        )
    }

    /// Whether this part is a single source fragment.
    #[must_use]
    pub fn is_simple(&self) -> bool {
        self.indexes().len() == 1
    }

    /// Encodes this part as fixed-schema CBOR.
    #[must_use]
    pub fn to_cbor(&self) -> Vec<u8> {
        part_cbor::encode_part(self)
    }

    /// Decodes a part from CBOR using the default max fragment data length.
    ///
    /// # Errors
    ///
    /// Returns CBOR or resource-limit errors.
    pub fn from_cbor(bytes: &[u8]) -> Result<Self> {
        let limits = DecoderLimits::default();
        Self::from_cbor_with_max(
            bytes,
            limits.max_fragment_data_length,
            limits.max_fragment_count,
        )
    }

    /// Decodes a part from CBOR with explicit data and fragment-count caps.
    ///
    /// # Errors
    ///
    /// Returns CBOR or resource-limit errors.
    pub fn from_cbor_with_max(
        bytes: &[u8],
        max_data_len: usize,
        max_fragment_count: usize,
    ) -> Result<Self> {
        part_cbor::decode_part(bytes, max_data_len, max_fragment_count)
    }

    /// Sequence id string `"{seq}-{count}"`.
    #[must_use]
    pub fn sequence_id(&self) -> String {
        alloc::format!("{}-{}", self.sequence, self.sequence_count)
    }
}

const fn div_ceil(a: usize, b: usize) -> usize {
    let d = a / b;
    let r = a % b;
    if r > 0 { d + 1 } else { d }
}

#[must_use]
pub(crate) const fn fragment_length(data_length: usize, max_fragment_length: usize) -> usize {
    let fragment_count = div_ceil(data_length, max_fragment_length);
    div_ceil(data_length, fragment_count)
}

#[must_use]
pub(crate) fn partition(mut data: Vec<u8>, fragment_length: usize) -> Vec<Vec<u8>> {
    let pad = (fragment_length - (data.len() % fragment_length)) % fragment_length;
    data.extend(core::iter::repeat_n(0, pad));
    data.chunks(fragment_length).map(<[u8]>::to_vec).collect()
}

#[must_use]
pub(crate) fn choose_fragments(
    sequence: usize,
    fragment_count: usize,
    checksum: u32,
) -> Vec<usize> {
    if sequence == 0 || fragment_count == 0 {
        return Vec::new();
    }
    if sequence <= fragment_count {
        return alloc::vec![sequence - 1];
    }
    // Sequence is already validated as a wire `u32` at the encoder/decoder edge.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "UR wire sequence is u32; callers only pass values that fit"
    )]
    let sequence_u32 = sequence as u32;
    let mut seed = [0u8; 8];
    seed[0..4].copy_from_slice(&sequence_u32.to_be_bytes());
    seed[4..8].copy_from_slice(&checksum.to_be_bytes());
    let mut xoshiro = Xoshiro256::from(seed.as_slice());
    let degree = xoshiro.choose_degree(fragment_count);
    let indexes = (0..fragment_count).collect();
    let mut shuffled = xoshiro.shuffled(indexes);
    shuffled.truncate(degree as usize);
    shuffled
}

fn xor(v1: &mut [u8], v2: &[u8]) -> Result<()> {
    if v1.len() != v2.len() {
        return Err(Error::DecoderState);
    }
    for (x1, x2) in v1.iter_mut().zip(v2.iter()) {
        *x1 ^= x2;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::test_utils::make_message;

    fn testdata_lines(raw: &str) -> Vec<&str> {
        raw.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect()
    }

    #[test]
    fn decoder_limits_default_budget_is_locked() {
        let limits = DecoderLimits::default();
        assert_eq!(limits.max_message_length, 1_048_576);
        assert_eq!(limits.max_fragment_count, 2_000);
        assert_eq!(limits.max_fragment_data_length, 8_192);
        assert_eq!(limits.max_buffer_parts, 4_000);
        assert_eq!(limits.max_received_parts, 8_000);
        assert_eq!(limits.max_uri_len, 8_192);

        let usz = size_of::<usize>();
        let decoded_a = limits
            .max_message_length
            .saturating_add(limits.max_fragment_data_length);
        let decoded_b = limits
            .max_fragment_count
            .saturating_mul(limits.max_fragment_data_length);
        let decoded = decoded_a.min(decoded_b);
        let per_buffer = limits
            .max_fragment_data_length
            .saturating_add(limits.max_fragment_count.saturating_mul(usz));
        let buffer = limits.max_buffer_parts.saturating_mul(per_buffer);
        let received = limits
            .max_received_parts
            .saturating_mul(limits.max_fragment_count)
            .saturating_mul(usz);
        let expected = decoded.saturating_add(buffer).saturating_add(received);
        assert_eq!(limits.worst_case_heap_bytes(), expected);

        #[cfg(target_pointer_width = "64")]
        assert_eq!(limits.worst_case_heap_bytes(), 225_824_768);

        let overflow = DecoderLimits {
            max_message_length: usize::MAX,
            max_fragment_count: usize::MAX,
            max_fragment_data_length: usize::MAX,
            max_buffer_parts: usize::MAX,
            max_received_parts: usize::MAX,
            max_uri_len: usize::MAX,
        };
        assert_eq!(overflow.worst_case_heap_bytes(), usize::MAX);
    }

    #[test]
    fn oversized_padding_is_inconsistent() {
        let mut decoder = Decoder::default();
        let part = Part::from_fields(1, 10, 1, 0, vec![0_u8; 100]);
        assert!(matches!(
            decoder.receive(part),
            Err(Error::InconsistentPart)
        ));
    }

    #[test]
    fn test_fragment_length() {
        assert_eq!(fragment_length(12345, 1955), 1764);
        assert_eq!(fragment_length(10, 4), 4);
        assert_eq!(fragment_length(10, 6), 5);
    }

    #[test]
    fn test_fountain_roundtrip() {
        let message = make_message("Wolf", 256);
        let mut encoder = Encoder::new(&message, 30).unwrap();
        let mut decoder = Decoder::default();
        while !decoder.complete() {
            let part = encoder.next_part().unwrap();
            decoder.receive(part).unwrap();
        }
        assert_eq!(decoder.message().unwrap(), Some(message));
    }

    #[test]
    fn test_fountain_encoder_parts() {
        let message = make_message("Wolf", 256);
        let mut encoder = Encoder::new(&message, 30).unwrap();
        let expected_first = "916ec65cf77cadf55cd7f9cda1a1030026ddd42e905b77adc36e4f2d3c";
        let part = encoder.next_part().unwrap();
        assert_eq!(hex::encode(part.data()), expected_first);
        assert_eq!(part.sequence(), 1);
        assert_eq!(part.sequence_count(), 9);
        assert_eq!(part.message_length(), 256);
    }

    #[test]
    fn test_cbor_golden() {
        let message = make_message("Wolf", 256);
        let mut encoder = Encoder::new(&message, 30).unwrap();
        let part = encoder.next_part().unwrap();
        assert_eq!(
            hex::encode(part.to_cbor()),
            "8501091901001a0167aa07581d916ec65cf77cadf55cd7f9cda1a1030026ddd42e905b77adc36e4f2d3c"
        );
    }

    #[test]
    fn test_empty_encoder() {
        assert!(matches!(Encoder::new(&[], 1), Err(Error::EmptyMessage)));
    }

    #[test]
    fn test_skip_fragments() {
        let message = make_message("Wolf", 32767);
        let mut encoder = Encoder::new(&message, 1000).unwrap();
        let mut decoder = Decoder::default();
        let mut skip = false;
        while !decoder.complete() {
            let part = encoder.next_part().unwrap();
            if !skip {
                decoder.receive(part).unwrap();
            }
            skip = !skip;
        }
        assert_eq!(decoder.message().unwrap(), Some(message));
    }

    #[test]
    fn test_choose_fragments() {
        // Extended table matches ur-rs 0.5 `test_choose_fragments` (seq 1..=30).
        let message = make_message("Wolf", 1024);
        let checksum = crc32::checksum(&message);
        let fl = fragment_length(message.len(), 100);
        let fragments = partition(message, fl);
        let expected: Vec<Vec<usize>> = testdata_lines(include_str!(
            "../../tests/vectors/ur_rs_choose_fragments.txt"
        ))
        .into_iter()
        .map(|line| {
            line.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.parse::<usize>().expect("choose_fragments index"))
                .collect()
        })
        .collect();
        for (i, e) in expected.iter().enumerate() {
            let mut indexes = choose_fragments(i + 1, fragments.len(), checksum);
            indexes.sort_unstable();
            assert_eq!(&indexes, e);
        }
    }

    #[test]
    fn test_partition_and_join() {
        // ur-rs 0.5 `test_partition_and_join` fragment hex table.
        let message = make_message("Wolf", 1024);
        let fl = fragment_length(message.len(), 100);
        let fragments = partition(message.clone(), fl);
        let expected_fragments =
            testdata_lines(include_str!("../../tests/vectors/wolf256_fragments.hex"));
        assert_eq!(fragments.len(), expected_fragments.len());
        for (fragment, expected) in fragments.iter().zip(expected_fragments.iter()) {
            assert_eq!(hex::encode(fragment), *expected);
        }
        let mut rejoined: Vec<u8> = fragments.into_iter().flatten().collect();
        rejoined.truncate(message.len());
        assert_eq!(rejoined, message);
    }

    /// Complex part reduced to a simple fragment must cascade into the buffer.
    #[test]
    fn test_complex_to_simple_cascades_into_buffer() {
        let (target, known, reducer) =
            cascade_fixture().expect("need mixed parts that exercise buffer cascade");

        let mut decoder = Decoder::default();
        decoder.receive(target).unwrap();
        decoder.receive(known).unwrap();
        decoder.receive(reducer).unwrap();

        // known simple + reducer-derived simple + cascade of buffered pair
        assert!(
            decoder.resolved_fragment_count().unwrap_or(0) >= 3,
            "cascade failed: resolved={:?} buffer should yield the third fragment",
            decoder.resolved_fragment_count()
        );
    }

    /// Finds `(buffered degree-2, known simple, reducer mixed)` for cascade tests.
    fn cascade_fixture() -> Option<(Part, Part, Part)> {
        let message = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ012345".to_vec();
        let mut encoder = Encoder::new(&message, 8).ok()?;
        let k = encoder.fragment_count() as usize;
        let mut parts = Vec::new();
        for _ in 0..k.saturating_mul(30) {
            parts.push(encoder.next_part().ok()?);
        }

        let mut simple_by_index: BTreeMap<usize, Part> = BTreeMap::new();
        for part in &parts {
            if let Some(idx) = part
                .indexes()
                .into_iter()
                .next()
                .filter(|_| part.is_simple())
            {
                simple_by_index.entry(idx).or_insert_with(|| part.clone());
            }
        }

        let target = parts.iter().find(|p| p.indexes().len() == 2)?.clone();
        let mut ends = target.indexes();
        ends.sort_unstable();
        let end_lo = *ends.first()?;
        let end_hi = *ends.get(1)?;

        parts.iter().find_map(|part| {
            if part.is_simple() {
                return None;
            }
            let idxs = part.indexes();
            if idxs.len() != 2 {
                return None;
            }
            let left = *idxs.first()?;
            let right = *idxs.get(1)?;
            candidate_reducer(end_lo, end_hi, left, right, part, &target, &simple_by_index).or_else(
                || candidate_reducer(end_lo, end_hi, right, left, part, &target, &simple_by_index),
            )
        })
    }

    fn candidate_reducer(
        end_lo: usize,
        end_hi: usize,
        recovered: usize,
        other: usize,
        reducer: &Part,
        target: &Part,
        simple_by_index: &BTreeMap<usize, Part>,
    ) -> Option<(Part, Part, Part)> {
        let recovers_endpoint = recovered == end_lo || recovered == end_hi;
        let other_outside_pair = other != end_lo && other != end_hi;
        if !(recovers_endpoint && other_outside_pair) {
            return None;
        }
        let known = simple_by_index.get(&other)?.clone();
        Some((target.clone(), known, reducer.clone()))
    }

    #[test]
    fn test_inconsistent_part_rejected() {
        let message = make_message("Wolf", 64);
        let mut encoder_a = Encoder::new(&message, 16).unwrap();
        let mut encoder_b = Encoder::new(&make_message("Other", 64), 16).unwrap();
        let mut decoder = Decoder::default();
        decoder.receive(encoder_a.next_part().unwrap()).unwrap();
        assert!(matches!(
            decoder.receive(encoder_b.next_part().unwrap()),
            Err(Error::InconsistentPart)
        ));
    }

    #[test]
    fn test_duplicate_part_ignored() {
        let message = make_message("Wolf", 64);
        let mut encoder = Encoder::new(&message, 16).unwrap();
        let part = encoder.next_part().unwrap();
        let mut decoder = Decoder::default();
        assert!(decoder.receive(part.clone()).unwrap());
        assert!(!decoder.receive(part).unwrap());
    }

    #[test]
    fn test_resource_limit_fragment_count_poisons() {
        let limits = DecoderLimits {
            max_fragment_count: 1,
            ..DecoderLimits::default()
        };
        let mut decoder = Decoder::with_limits(limits);
        let message = make_message("Wolf", 64);
        let mut encoder = Encoder::new(&message, 8).unwrap();
        assert!(encoder.fragment_count() > 1);
        assert!(matches!(
            decoder.receive(encoder.next_part().unwrap()),
            Err(Error::ResourceLimit(ResourceKind::FragmentCount))
        ));
        assert!(decoder.is_poisoned());
        // Fail-closed: subsequent receives keep failing.
        assert!(matches!(
            decoder.receive(encoder.next_part().unwrap()),
            Err(Error::ResourceLimit(ResourceKind::FragmentCount))
        ));
    }

    #[test]
    fn test_empty_part_and_invalid_sequence() {
        let mut decoder = Decoder::default();
        let empty = Part::from_fields(1, 1, 1, 0, Vec::new());
        assert!(matches!(decoder.receive(empty), Err(Error::EmptyPart)));
        let zero_seq = Part::from_fields(0, 1, 1, 0, alloc::vec![0]);
        assert!(matches!(
            decoder.receive(zero_seq),
            Err(Error::InvalidSequence)
        ));
    }

    #[test]
    fn test_invalid_max_fragment_len() {
        assert!(matches!(
            Encoder::new(b"x", 0),
            Err(Error::InvalidFragmentLen)
        ));
    }

    #[test]
    fn test_k1_second_next_part_is_exhausted() {
        let mut encoder = Encoder::new(b"hello", 64).unwrap();
        assert_eq!(encoder.fragment_count(), 1);
        assert!(encoder.next_part().is_ok());
        assert!(matches!(
            encoder.next_part(),
            Err(Error::SinglePartExhausted)
        ));
    }

    #[test]
    fn test_next_sequence_max_is_resource_limit() {
        // Wrap at u32::MAX is not exercised at runtime (too expensive).
        // The increment predicate is next_sequence.
        assert!(matches!(
            next_sequence(u32::MAX),
            Err(Error::ResourceLimit(ResourceKind::Sequence))
        ));
    }

    #[test]
    fn test_buffer_duplicate_at_capacity_does_not_poison() {
        let limits = DecoderLimits {
            max_buffer_parts: 1,
            ..DecoderLimits::default()
        };
        let message = make_message("Wolf", 64);
        let mut encoder = Encoder::new(&message, 8).unwrap();
        let k = encoder.fragment_count();
        let first_mixed = (0..k.saturating_mul(20))
            .map(|_| encoder.next_part().unwrap())
            .find(|p| !p.is_simple())
            .expect("need at least one mixed part");

        let mut decoder = Decoder::with_limits(limits);
        decoder.receive(first_mixed.clone()).unwrap();
        // Same index-set: ignored without growing the buffer or poisoning.
        assert!(!decoder.receive(first_mixed).unwrap());
        assert!(!decoder.is_poisoned());
    }

    #[test]
    fn test_resource_limit_message_length_poisons() {
        let limits = DecoderLimits {
            max_message_length: 16,
            ..DecoderLimits::default()
        };
        let mut decoder = Decoder::with_limits(limits);
        let message = make_message("Wolf", 64);
        let mut encoder = Encoder::new(&message, 8).unwrap();
        assert!(matches!(
            decoder.receive(encoder.next_part().unwrap()),
            Err(Error::ResourceLimit(ResourceKind::MessageLength))
        ));
        assert!(decoder.is_poisoned());
    }

    #[test]
    fn test_checksum_mismatch_is_inconsistent() {
        let message = make_message("Wolf", 32);
        let mut encoder = Encoder::new(&message, 8).unwrap();
        let part = encoder.next_part().unwrap();
        let bad = Part::from_fields(
            part.sequence(),
            part.sequence_count(),
            part.message_length(),
            part.checksum().wrapping_add(1),
            part.data().to_vec(),
        );
        let mut decoder = Decoder::default();
        decoder.receive(bad).unwrap();
        let mut encoder2 = Encoder::new(&message, 8).unwrap();
        let _ = encoder2.next_part().unwrap();
        assert!(matches!(
            decoder.receive(encoder2.next_part().unwrap()),
            Err(Error::InconsistentPart)
        ));
    }

    #[test]
    fn test_complete_message_crc_ok() {
        let message = make_message("Wolf", 10);
        let mut encoder = Encoder::new(&message, 4).unwrap();
        let mut decoder = Decoder::default();
        while !decoder.complete() {
            decoder.receive(encoder.next_part().unwrap()).unwrap();
        }
        assert_eq!(
            decoder.message().unwrap().as_deref(),
            Some(message.as_slice())
        );
    }
}
