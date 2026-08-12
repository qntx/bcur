//! CRC-32 (ISO-HDLC) helper used by bytewords and fountain messages.

/// Returns the CRC-32 ISO-HDLC instance used by the UR stack.
#[must_use]
pub(crate) const fn crc32() -> crc::Crc<u32> {
    crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC)
}

/// Computes the CRC-32 checksum of `data`.
#[must_use]
#[allow(
    clippy::missing_const_for_fn,
    reason = "crc crate checksum is not const"
)]
pub(crate) fn checksum(data: &[u8]) -> u32 {
    crc32().checksum(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors() {
        assert_eq!(checksum(b"Hello, world!"), 0xebe6_c6e6);
        assert_eq!(checksum(b"Wolf"), 0x598c_84dc);
    }
}
