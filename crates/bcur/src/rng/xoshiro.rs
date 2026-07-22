//! Xoshiro256** RNG with SHA-256 seeding matching URKit / ur-rs.

use alloc::vec::Vec;

use bitcoin_hashes::sha256;
use rand_xoshiro::Xoshiro256StarStar;
use rand_xoshiro::rand_core::{Rng, SeedableRng};

use super::Weighted;

/// Xoshiro256** wrapper with UR-compatible seeding and helpers.
pub(crate) struct Xoshiro256 {
    inner: Xoshiro256StarStar,
}

impl From<Xoshiro256StarStar> for Xoshiro256 {
    fn from(inner: Xoshiro256StarStar) -> Self {
        Self { inner }
    }
}

impl From<&[u8]> for Xoshiro256 {
    fn from(bytes: &[u8]) -> Self {
        let hash = sha256::Hash::hash(bytes);
        Self::from(hash.to_byte_array())
    }
}

impl From<&str> for Xoshiro256 {
    fn from(value: &str) -> Self {
        Self::from(value.as_bytes())
    }
}

impl From<[u8; 32]> for Xoshiro256 {
    fn from(value: [u8; 32]) -> Self {
        // Pack each 8-byte big-endian chunk into little-endian seed words
        // (matches ur-rs / URKit seed layout).
        let mut s = [0_u8; 32];
        for i in 0..4 {
            let mut v: u64 = 0;
            for n in 0..8 {
                v <<= 8;
                v |= u64::from(value[8 * i + n]);
            }
            let bytes = v.to_le_bytes();
            s[8 * i..8 * i + 8].copy_from_slice(&bytes);
        }
        Xoshiro256StarStar::from_seed(s).into()
    }
}

impl Xoshiro256 {
    pub(crate) fn next_u64(&mut self) -> u64 {
        self.inner.next_u64()
    }

    pub(crate) fn next_double(&mut self) -> f64 {
        unit_interval(self.next_u64())
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
    pub(crate) fn next_int(&mut self, low: u64, high: u64) -> u64 {
        (self.next_double() * ((high - low + 1) as f64)) as u64 + low
    }

    /// Remove-based shuffle (not Fisher–Yates) for UR interop.
    pub(crate) fn shuffled<T>(&mut self, mut items: Vec<T>) -> Vec<T> {
        let mut out = Vec::with_capacity(items.len());
        while !items.is_empty() {
            let index = self.next_int(0, (items.len() - 1) as u64) as usize;
            let item = items.remove(index);
            out.push(item);
        }
        out
    }

    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn choose_degree(&mut self, length: usize) -> u32 {
        let degree_weights: Vec<f64> = (1..=length).map(|x| 1.0 / x as f64).collect();
        let sampler = Weighted::new(degree_weights);
        sampler.next(self) + 1
    }
}

#[allow(clippy::cast_precision_loss)]
fn unit_interval(value: u64) -> f64 {
    const SCALE: f64 = 1.0 / ((1_u64 << 53) as f64);
    ((value >> 11) as f64) * SCALE
}

#[cfg(test)]
pub(crate) mod test_utils {
    use super::*;
    use crate::crc32;

    impl Xoshiro256 {
        #[allow(clippy::cast_possible_truncation)]
        fn next_byte(&mut self) -> u8 {
            self.next_int(0, 255) as u8
        }

        pub(crate) fn next_bytes(&mut self, n: usize) -> Vec<u8> {
            (0..n).map(|_| self.next_byte()).collect()
        }

        #[allow(dead_code)]
        pub(crate) fn from_crc(bytes: &[u8]) -> Self {
            Self::from(crc32::checksum(bytes).to_be_bytes().as_slice())
        }
    }

    pub(crate) fn make_message(seed: &str, size: usize) -> Vec<u8> {
        let mut xoshiro = Xoshiro256::from(seed);
        xoshiro.next_bytes(size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rng_1() {
        let mut rng = Xoshiro256::from("Wolf");
        let expected = [
            42, 81, 85, 8, 82, 84, 76, 73, 70, 88, 2, 74, 40, 48, 77, 54, 88, 7, 5, 88, 37, 25, 82,
            13, 69, 59, 30, 39, 11, 82, 19, 99, 45, 87, 30, 15, 32, 22, 89, 44, 92, 77, 29, 78, 4,
            92, 44, 68, 92, 69, 1, 42, 89, 50, 37, 84, 63, 34, 32, 3, 17, 62, 40, 98, 82, 89, 24,
            43, 85, 39, 15, 3, 99, 29, 20, 42, 27, 10, 85, 66, 50, 35, 69, 70, 70, 74, 30, 13, 72,
            54, 11, 5, 70, 55, 91, 52, 10, 43, 43, 52,
        ];
        for e in expected {
            assert_eq!(rng.next_u64() % 100, e);
        }
    }

    #[test]
    fn test_rng_3() {
        let mut rng = Xoshiro256::from("Wolf");
        let expected = [
            6, 5, 8, 4, 10, 5, 7, 10, 4, 9, 10, 9, 7, 7, 1, 1, 2, 9, 9, 2, 6, 4, 5, 7, 8, 5, 4, 2,
            3, 8, 7, 4, 5, 1, 10, 9, 3, 10, 2, 6, 8, 5, 7, 9, 3, 1, 5, 2, 7, 1, 4, 4, 4, 4, 9, 4,
            5, 5, 6, 9, 5, 1, 2, 8, 3, 3, 2, 8, 4, 3, 2, 1, 10, 8, 9, 3, 10, 8, 5, 5, 6, 7, 10, 5,
            8, 9, 4, 6, 4, 2, 10, 2, 1, 7, 9, 6, 7, 4, 2, 5,
        ];
        for e in expected {
            assert_eq!(rng.next_int(1, 10), e);
        }
    }

    #[test]
    fn test_shuffle() {
        let mut rng = Xoshiro256::from("Wolf");
        let values = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let expected = [
            vec![6, 4, 9, 3, 10, 5, 7, 8, 1, 2],
            vec![10, 8, 6, 5, 1, 2, 3, 9, 7, 4],
            vec![6, 4, 5, 8, 9, 3, 2, 1, 7, 10],
        ];
        for e in expected {
            assert_eq!(rng.shuffled(values.clone()), e);
        }
    }
}
