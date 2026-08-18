//! Fixed-schema CBOR codec for fountain [`Part`](super::Part) (5-element array).
//!
//! Wire layout: `array(5) [ sequence:u32, sequence_count:u32, message_length:u32,
//! checksum:u32, data:bstr ]` using shortest-form major-type-0 integers.

use alloc::vec::Vec;

use super::Part;
use crate::{Error, ResourceKind, Result};

/// Encodes a part to deterministic CBOR bytes.
#[must_use]
pub(crate) fn encode_part(part: &Part) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + part.data().len());
    out.push(0x85); // array of 5
    encode_u32(&mut out, part.sequence());
    encode_u32(&mut out, part.sequence_count());
    encode_u32(&mut out, part.message_length());
    encode_u32(&mut out, part.checksum());
    encode_bstr(&mut out, part.data());
    out
}

/// Decodes a part from CBOR with data-length and fragment-count caps.
pub(crate) fn decode_part(
    bytes: &[u8],
    max_data_len: usize,
    max_fragment_count: usize,
) -> Result<Part> {
    let mut i = 0;
    let head = next_byte(bytes, &mut i)?;
    if head != 0x85 {
        return Err(Error::InvalidPartCbor);
    }
    let sequence = decode_u32(bytes, &mut i)?;
    let sequence_count = decode_u32(bytes, &mut i)?;
    let message_length = decode_u32(bytes, &mut i)?;
    let checksum = decode_u32(bytes, &mut i)?;
    let data = decode_bstr(bytes, &mut i, max_data_len)?;
    if i != bytes.len() {
        return Err(Error::InvalidPartCbor);
    }
    if sequence == 0 {
        return Err(Error::InvalidSequence);
    }
    if sequence_count == 0 || message_length == 0 || data.is_empty() {
        return Err(Error::EmptyPart);
    }
    let count = usize::try_from(sequence_count).unwrap_or(usize::MAX);
    if count > max_fragment_count {
        return Err(Error::ResourceLimit(ResourceKind::FragmentCount));
    }
    Ok(Part::from_fields(
        sequence,
        sequence_count,
        message_length,
        checksum,
        data,
    ))
}

fn encode_u32(out: &mut Vec<u8>, v: u32) {
    if v <= 23 {
        #[allow(clippy::cast_possible_truncation, reason = "v is checked <= 23")]
        {
            out.push(v as u8);
        }
    } else if v <= 0xff {
        out.push(0x18);
        #[allow(clippy::cast_possible_truncation, reason = "v is checked <= 0xff")]
        {
            out.push(v as u8);
        }
    } else if v <= 0xffff {
        out.push(0x19);
        #[allow(clippy::cast_possible_truncation, reason = "v is checked <= 0xffff")]
        {
            out.extend_from_slice(&(v as u16).to_be_bytes());
        }
    } else {
        out.push(0x1a);
        out.extend_from_slice(&v.to_be_bytes());
    }
}

fn encode_bstr(out: &mut Vec<u8>, data: &[u8]) {
    let len = data.len();
    if len <= 23 {
        #[allow(clippy::cast_possible_truncation, reason = "len is checked <= 23")]
        {
            out.push(0x40 | (len as u8));
        }
    } else if len <= 0xff {
        out.push(0x58);
        #[allow(clippy::cast_possible_truncation, reason = "len is checked <= 0xff")]
        {
            out.push(len as u8);
        }
    } else if len <= 0xffff {
        out.push(0x59);
        #[allow(clippy::cast_possible_truncation, reason = "len is checked <= 0xffff")]
        {
            out.extend_from_slice(&(len as u16).to_be_bytes());
        }
    } else {
        out.push(0x5a);
        #[allow(
            clippy::cast_possible_truncation,
            reason = "bstr lengths used here fit u32 (fragment data is capped)"
        )]
        {
            out.extend_from_slice(&(len as u32).to_be_bytes());
        }
    }
    out.extend_from_slice(data);
}

fn next_byte(bytes: &[u8], i: &mut usize) -> Result<u8> {
    let b = *bytes.get(*i).ok_or(Error::InvalidPartCbor)?;
    *i += 1;
    Ok(b)
}

fn decode_u32(bytes: &[u8], i: &mut usize) -> Result<u32> {
    let head = next_byte(bytes, i)?;
    let major = head >> 5;
    let ai = head & 0x1f;
    if major != 0 {
        return Err(Error::InvalidPartCbor);
    }
    match ai {
        n @ 0..=23 => Ok(u32::from(n)),
        24 => {
            let v = u32::from(next_byte(bytes, i)?);
            // Shortest-form: values 0..=23 must use the compact ai encoding.
            if v <= 23 {
                return Err(Error::InvalidPartCbor);
            }
            Ok(v)
        }
        25 => {
            let b0 = next_byte(bytes, i)?;
            let b1 = next_byte(bytes, i)?;
            let v = u32::from(u16::from_be_bytes([b0, b1]));
            if v <= 0xff {
                return Err(Error::InvalidPartCbor);
            }
            Ok(v)
        }
        26 => {
            let b0 = next_byte(bytes, i)?;
            let b1 = next_byte(bytes, i)?;
            let b2 = next_byte(bytes, i)?;
            let b3 = next_byte(bytes, i)?;
            let v = u32::from_be_bytes([b0, b1, b2, b3]);
            if v <= 0xffff {
                return Err(Error::InvalidPartCbor);
            }
            Ok(v)
        }
        // Reject u64 and indefinite forms for shortest-form / schema strictness.
        _ => Err(Error::InvalidPartCbor),
    }
}

fn decode_bstr(bytes: &[u8], i: &mut usize, max_data_len: usize) -> Result<Vec<u8>> {
    let head = next_byte(bytes, i)?;
    let major = head >> 5;
    let ai = head & 0x1f;
    if major != 2 {
        return Err(Error::InvalidPartCbor);
    }
    let len = match ai {
        n @ 0..=23 => usize::from(n),
        24 => {
            let len = usize::from(next_byte(bytes, i)?);
            if len <= 23 {
                return Err(Error::InvalidPartCbor);
            }
            len
        }
        25 => {
            let b0 = next_byte(bytes, i)?;
            let b1 = next_byte(bytes, i)?;
            let len = usize::from(u16::from_be_bytes([b0, b1]));
            if len <= 0xff {
                return Err(Error::InvalidPartCbor);
            }
            len
        }
        26 => {
            let b0 = next_byte(bytes, i)?;
            let b1 = next_byte(bytes, i)?;
            let b2 = next_byte(bytes, i)?;
            let b3 = next_byte(bytes, i)?;
            let len = usize::try_from(u32::from_be_bytes([b0, b1, b2, b3]))
                .map_err(|_| Error::InvalidPartCbor)?;
            if len <= 0xffff {
                return Err(Error::InvalidPartCbor);
            }
            len
        }
        _ => return Err(Error::InvalidPartCbor),
    };
    if len > max_data_len {
        return Err(Error::ResourceLimit(ResourceKind::FragmentData));
    }
    let end = i.checked_add(len).ok_or(Error::InvalidPartCbor)?;
    let slice = bytes.get(*i..end).ok_or(Error::InvalidPartCbor)?;
    *i = end;
    Ok(slice.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_golden() {
        let part = Part::from_fields(
            1,
            9,
            256,
            23_570_951,
            hex::decode("916ec65cf77cadf55cd7f9cda1a1030026ddd42e905b77adc36e4f2d3c").unwrap(),
        );
        let cbor = encode_part(&part);
        assert_eq!(
            hex::encode(&cbor),
            "8501091901001a0167aa07581d916ec65cf77cadf55cd7f9cda1a1030026ddd42e905b77adc36e4f2d3c"
        );
        let decoded = decode_part(&cbor, 8192, 2000).unwrap();
        assert_eq!(decoded, part);
    }

    #[test]
    fn rejects_non_shortest_integer() {
        // array(5) with sequence encoded as 0x18 0x01 (non-shortest for 1)
        let cbor = hex::decode(
            "851801091901001a0167aa07581d916ec65cf77cadf55cd7f9cda1a1030026ddd42e905b77adc36e4f2d3c",
        )
        .unwrap();
        assert!(matches!(
            decode_part(&cbor, 8192, 2000),
            Err(Error::InvalidPartCbor)
        ));
    }

    #[test]
    fn rejects_zero_sequence_fields() {
        // [0, 1, 1, 0, h'00']
        let zero_seq = hex::decode("85000101004100").unwrap();
        assert!(matches!(
            decode_part(&zero_seq, 8192, 2000),
            Err(Error::InvalidSequence)
        ));
        // [1, 0, 1, 0, h'00']
        let zero_count = hex::decode("85010001004100").unwrap();
        assert!(matches!(
            decode_part(&zero_count, 8192, 2000),
            Err(Error::EmptyPart)
        ));
    }

    #[test]
    fn rejects_trailing_bytes() {
        let part = Part::from_fields(1, 1, 1, 0, alloc::vec![0xab]);
        let mut cbor = encode_part(&part);
        cbor.push(0x00);
        assert!(matches!(
            decode_part(&cbor, 8192, 2000),
            Err(Error::InvalidPartCbor)
        ));
    }

    #[test]
    fn rejects_oversized_data() {
        let part = Part::from_fields(1, 1, 1, 0, alloc::vec![0; 32]);
        let cbor = encode_part(&part);
        assert!(matches!(
            decode_part(&cbor, 16, 2000),
            Err(Error::ResourceLimit(ResourceKind::FragmentData))
        ));
    }

    #[test]
    fn rejects_oversize_fragment_count() {
        let part = Part::from_fields(1, 9, 9, 0, alloc::vec![0xab]);
        let cbor = encode_part(&part);
        assert!(matches!(
            decode_part(&cbor, 8192, 8),
            Err(Error::ResourceLimit(ResourceKind::FragmentCount))
        ));
    }
}
