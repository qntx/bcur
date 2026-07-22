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
use crate::rng::Xoshiro256;
use crate::{Error, Result};

/// Hard limits for adversarial multi-part streams.
///
/// Default numeric values are experimental before 1.0; hosts that need fixed
/// budgets should use [`Decoder::with_limits`].
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

/// Fountain encoder.
#[derive(Debug)]
pub struct Encoder {
    parts: Vec<Vec<u8>>,
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
        if message.len() > u32::MAX as usize {
            return Err(Error::ResourceLimit("message_length"));
        }
        let fragment_length = fragment_length(message.len(), max_fragment_length);
        let fragments = partition(message.to_vec(), fragment_length);
        if fragments.len() > u32::MAX as usize {
            return Err(Error::ResourceLimit("fragment_count"));
        }
        Ok(Self {
            parts: fragments,
            message_length: u32::try_from(message.len()).map_err(|_| Error::ResourceLimit("message_length"))?,
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
    pub fn fragment_count(&self) -> u32 {
        u32::try_from(self.parts.len()).unwrap_or(u32::MAX)
    }

    /// Whether all original segments have been emitted at least once.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.current_sequence as usize >= self.parts.len()
    }

    /// Emits the next fountain part.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ResourceLimit`] if the sequence would exceed `u32::MAX`.
    pub fn next_part(&mut self) -> Result<Part> {
        if self.current_sequence == u32::MAX {
            return Err(Error::ResourceLimit("sequence"));
        }
        self.current_sequence += 1;
        let indexes = choose_fragments(
            self.current_sequence as usize,
            self.parts.len(),
            self.checksum,
        );
        let mut mixed = alloc::vec![0_u8; self.parts[0].len()];
        for item in indexes {
            xor(&mut mixed, &self.parts[item]);
        }
        Ok(Part {
            sequence: self.current_sequence,
            sequence_count: self.fragment_count(),
            message_length: self.message_length,
            checksum: self.checksum,
            data: mixed,
        })
    }
}

/// Fountain decoder with resource limits and fail-closed poison.
#[derive(Debug)]
pub struct Decoder {
    decoded: BTreeMap<usize, Part>,
    received: BTreeSet<Vec<usize>>,
    buffer: BTreeMap<Vec<usize>, Part>,
    queue: Vec<(usize, Part)>,
    sequence_count: usize,
    message_length: usize,
    checksum: u32,
    fragment_length: usize,
    limits: DecoderLimits,
    poisoned: Option<&'static str>,
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
    pub fn with_limits(limits: DecoderLimits) -> Self {
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

    /// Whether a previous `ResourceLimit` poisoned this decoder.
    #[must_use]
    pub const fn is_poisoned(&self) -> bool {
        self.poisoned.is_some()
    }

    fn poison(&mut self, reason: &'static str) -> Error {
        self.poisoned = Some(reason);
        Error::ResourceLimit(reason)
    }

    /// Receives a fountain part.
    ///
    /// # Errors
    ///
    /// Returns an error if the part is invalid, inconsistent, or exceeds limits.
    pub fn receive(&mut self, part: Part) -> Result<bool> {
        if let Some(reason) = self.poisoned {
            return Err(Error::ResourceLimit(reason));
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
            return Err(self.poison("fragment_data"));
        }

        if self.received.is_empty() {
            let sc = part.sequence_count as usize;
            let ml = part.message_length as usize;
            if sc > self.limits.max_fragment_count {
                return Err(self.poison("fragment_count"));
            }
            if ml > self.limits.max_message_length {
                return Err(self.poison("message_length"));
            }
            let frag_len = part.data.len();
            let product = frag_len
                .checked_mul(sc)
                .ok_or_else(|| self.poison("message_length"))?;
            if product < ml {
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
            return Err(self.poison("received_parts"));
        }
        self.received.insert(indexes);
        if part.is_simple() {
            self.process_simple(part)?;
        } else {
            self.process_complex(part)?;
        }
        Ok(true)
    }

    fn process_simple(&mut self, part: Part) -> Result<()> {
        let index = *part.indexes().first().ok_or(Error::DecoderState)?;
        self.decoded.insert(index, part.clone());
        self.queue.push((index, part));
        self.process_queue()
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
                let mut part = self.buffer.remove(&indexes).ok_or(Error::DecoderState)?;
                let mut new_indexes = indexes;
                let to_remove = new_indexes
                    .iter()
                    .position(|&x| x == index)
                    .ok_or(Error::DecoderState)?;
                new_indexes.remove(to_remove);
                xor(&mut part.data, &simple.data);
                if new_indexes.len() == 1 {
                    let idx = *new_indexes.first().ok_or(Error::DecoderState)?;
                    self.decoded.insert(idx, part.clone());
                    self.queue.push((idx, part));
                } else {
                    if self.buffer.len() >= self.limits.max_buffer_parts {
                        return Err(self.poison("buffer_parts"));
                    }
                    self.buffer.insert(new_indexes, part);
                }
            }
        }
        Ok(())
    }

    fn process_complex(&mut self, mut part: Part) -> Result<()> {
        let mut indexes = part.indexes();
        let to_remove: Vec<usize> = indexes
            .iter()
            .copied()
            .filter(|idx| self.decoded.contains_key(idx))
            .collect();
        if indexes.len() == to_remove.len() {
            return Ok(());
        }
        for remove in to_remove {
            let idx_to_remove = indexes
                .iter()
                .position(|&x| x == remove)
                .ok_or(Error::DecoderState)?;
            indexes.remove(idx_to_remove);
            xor(
                &mut part.data,
                &self.decoded.get(&remove).ok_or(Error::DecoderState)?.data,
            );
        }
        if indexes.len() == 1 {
            let idx = *indexes.first().ok_or(Error::DecoderState)?;
            self.decoded.insert(idx, part.clone());
            self.queue.push((idx, part));
        } else {
            if self.buffer.len() >= self.limits.max_buffer_parts {
                return Err(self.poison("buffer_parts"));
            }
            self.buffer.insert(indexes, part);
        }
        Ok(())
    }

    /// Whether all source fragments have been recovered.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.message_length != 0 && self.decoded.len() == self.sequence_count
    }

    /// Number of resolved source fragments, or `None` before any part.
    #[must_use]
    pub fn resolved_fragment_count(&self) -> Option<usize> {
        if self.message_length == 0 {
            None
        } else {
            Some(self.decoded.len())
        }
    }

    /// Total fragment count `K`, or `0` before any part.
    #[must_use]
    pub const fn fragment_count(&self) -> usize {
        self.sequence_count
    }

    /// Whether `part` is consistent with previously received metadata.
    #[must_use]
    pub fn validate(&self, part: &Part) -> bool {
        if self.received.is_empty() {
            return false;
        }
        part.sequence_count as usize == self.sequence_count
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
        if let Some(reason) = self.poisoned {
            return Err(Error::ResourceLimit(reason));
        }
        if !self.complete() {
            return Ok(None);
        }
        let mut combined = Vec::with_capacity(self.fragment_length * self.sequence_count);
        for idx in 0..self.sequence_count {
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
    pub(crate) fn from_fields(
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
        Self::from_cbor_with_max(bytes, DecoderLimits::default().max_fragment_data_length)
    }

    /// Decodes a part from CBOR with an explicit max data length.
    ///
    /// # Errors
    ///
    /// Returns CBOR or resource-limit errors.
    pub fn from_cbor_with_max(bytes: &[u8], max_data_len: usize) -> Result<Self> {
        part_cbor::decode_part(bytes, max_data_len)
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
pub(crate) fn choose_fragments(sequence: usize, fragment_count: usize, checksum: u32) -> Vec<usize> {
    if sequence <= fragment_count {
        return alloc::vec![sequence - 1];
    }
    #[allow(clippy::cast_possible_truncation)]
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

fn xor(v1: &mut [u8], v2: &[u8]) {
    debug_assert_eq!(v1.len(), v2.len());
    for (x1, &x2) in v1.iter_mut().zip(v2.iter()) {
        *x1 ^= x2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::test_utils::make_message;

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
        let message = make_message("Wolf", 1024);
        let checksum = crc32::checksum(&message);
        let fl = fragment_length(message.len(), 100);
        let fragments = partition(message, fl);
        let expected = [
            vec![0],
            vec![1],
            vec![2],
            vec![3],
            vec![4],
            vec![5],
            vec![6],
            vec![7],
            vec![8],
            vec![9],
            vec![10],
            vec![9],
            vec![2, 5, 6, 8, 9, 10],
            vec![8],
            vec![1, 5],
        ];
        for (i, e) in expected.iter().enumerate() {
            let mut indexes = choose_fragments(i + 1, fragments.len(), checksum);
            indexes.sort_unstable();
            assert_eq!(&indexes, e);
        }
    }
}
