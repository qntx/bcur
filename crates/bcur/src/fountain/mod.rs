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
        let message_length =
            u32::try_from(message.len()).map_err(|_| Error::ResourceLimit("message_length"))?;
        let frag_len = fragment_length(message.len(), max_fragment_length);
        let fragments = partition(message.to_vec(), frag_len);
        let sequence_count =
            u32::try_from(fragments.len()).map_err(|_| Error::ResourceLimit("fragment_count"))?;
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

    /// Whether a previous `ResourceLimit` poisoned this decoder.
    #[must_use]
    pub const fn is_poisoned(&self) -> bool {
        self.poisoned.is_some()
    }

    const fn poison(&mut self, reason: &'static str) -> Error {
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
            self.enqueue_simple(part)?;
        } else {
            self.process_complex(part)?;
        }
        // Always drain the reduction queue after ingest so that a complex part
        // reduced to a simple fragment still cascades into the XOR buffer.
        self.process_queue()?;
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
            return Err(self.poison("buffer_parts"));
        }
        self.buffer.insert(indexes, part);
        Ok(())
    }

    /// Max fragment data length configured for this decoder.
    #[must_use]
    pub const fn max_fragment_data_length(&self) -> usize {
        self.limits.max_fragment_data_length
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
pub(crate) fn choose_fragments(
    sequence: usize,
    fragment_count: usize,
    checksum: u32,
) -> Vec<usize> {
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
            vec![1],
            vec![0, 2, 4, 5, 8, 10],
            vec![5],
            vec![2],
            vec![2],
            vec![0, 1, 3, 4, 5, 7, 9, 10],
            vec![0, 1, 2, 3, 5, 6, 8, 9, 10],
            vec![0, 2, 4, 5, 7, 8, 9, 10],
            vec![3, 5],
            vec![4],
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            vec![0, 1, 3, 4, 5, 6, 7, 9, 10],
            vec![6],
            vec![5, 6],
            vec![7],
        ];
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
        let expected_fragments = [
            "916ec65cf77cadf55cd7f9cda1a1030026ddd42e905b77adc36e4f2d3ccba44f7f04f2de44f42d84c374a0e149136f25b01852545961d55f7f7a8cde6d0e2ec43f3b2dcb644a2209e8c9e34af5c4747984a5e873c9cf5f965e25ee29039f",
            "df8ca74f1c769fc07eb7ebaec46e0695aea6cbd60b3ec4bbff1b9ffe8a9e7240129377b9d3711ed38d412fbb4442256f1e6f595e0fc57fed451fb0a0101fb76b1fb1e1b88cfdfdaa946294a47de8fff173f021c0e6f65b05c0a494e50791",
            "270a0050a73ae69b6725505a2ec8a5791457c9876dd34aadd192a53aa0dc66b556c0c215c7ceb8248b717c22951e65305b56a3706e3e86eb01c803bbf915d80edcd64d4d41977fa6f78dc07eecd072aae5bc8a852397e06034dba6a0b570",
            "797c3a89b16673c94838d884923b8186ee2db5c98407cab15e13678d072b43e406ad49477c2e45e85e52ca82a94f6df7bbbe7afbed3a3a830029f29090f25217e48d1f42993a640a67916aa7480177354cc7440215ae41e4d02eae9a1912",
            "33a6d4922a792c1b7244aa879fefdb4628dc8b0923568869a983b8c661ffab9b2ed2c149e38d41fba090b94155adbed32f8b18142ff0d7de4eeef2b04adf26f2456b46775c6c20b37602df7da179e2332feba8329bbb8d727a138b4ba7a5",
            "03215eda2ef1e953d89383a382c11d3f2cad37a4ee59a91236a3e56dcf89f6ac81dd4159989c317bd649d9cbc617f73fe10033bd288c60977481a09b343d3f676070e67da757b86de27bfca74392bac2996f7822a7d8f71a489ec6180390",
            "089ea80a8fcd6526413ec6c9a339115f111d78ef21d456660aa85f790910ffa2dc58d6a5b93705caef1091474938bd312427021ad1eeafbd19e0d916ddb111fabd8dcab5ad6a6ec3a9c6973809580cb2c164e26686b5b98cfb017a337968",
            "c7daaa14ae5152a067277b1b3902677d979f8e39cc2aafb3bc06fcf69160a853e6869dcc09a11b5009f91e6b89e5b927ab1527a735660faa6012b420dd926d940d742be6a64fb01cdc0cff9faa323f02ba41436871a0eab851e7f5782d10",
            "fbefde2a7e9ae9dc1e5c2c48f74f6c824ce9ef3c89f68800d44587bedc4ab417cfb3e7447d90e1e417e6e05d30e87239d3a5d1d45993d4461e60a0192831640aa32dedde185a371ded2ae15f8a93dba8809482ce49225daadfbb0fec629e",
            "23880789bdf9ed73be57fa84d555134630e8d0f7df48349f29869a477c13ccca9cd555ac42ad7f568416c3d61959d0ed568b2b81c7771e9088ad7fd55fd4386bafbf5a528c30f107139249357368ffa980de2c76ddd9ce4191376be0e6b5",
            "170010067e2e75ebe2d2904aeb1f89d5dc98cd4a6f2faaa8be6d03354c990fd895a97feb54668473e9d942bb99e196d897e8f1b01625cf48a7b78d249bb4985c065aa8cd1402ed2ba1b6f908f63dcd84b66425df00000000000000000000",
        ];
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
            Err(Error::ResourceLimit("fragment_count"))
        ));
        assert!(decoder.is_poisoned());
        // Fail-closed: subsequent receives keep failing.
        assert!(matches!(
            decoder.receive(encoder.next_part().unwrap()),
            Err(Error::ResourceLimit("fragment_count"))
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
            Err(Error::ResourceLimit("message_length"))
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
