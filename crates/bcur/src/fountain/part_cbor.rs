//! Fixed-schema CBOR codec for fountain [`Part`](super::Part) (5-element array).
//!
//! Wire layout: `array(5) [ sequence:u32, sequence_count:u32, message_length:u32,
//! checksum:u32, data:bstr ]` using shortest-form major-type-0 integers.

use alloc::vec::Vec;

use super::Part;
use crate::{Error, Result};

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

/// Decodes a part from CBOR with a maximum `data` length.
pub(crate) fn decode_part(bytes: &[u8], max_data_len: usize) -> Result<Part> {
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
        out.push(v as u8);
    } else if v <= 0xff {
        out.push(0x18);
        out.push(v as u8);
    } else if v <= 0xffff {
        out.push(0x19);
        out.extend_from_slice(&(v as u16).to_be_bytes());
    } else {
        out.push(0x1a);
        out.extend_from_slice(&v.to_be_bytes());
    }
}

fn encode_bstr(out: &mut Vec<u8>, data: &[u8]) {
    let len = data.len();
    if len <= 23 {
        out.push(0x40 | (len as u8));
    } else if len <= 0xff {
        out.push(0x58);
        out.push(len as u8);
    } else if len <= 0xffff {
        out.push(0x59);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(0x5a);
        out.extend_from_slice(&(len as u32).to_be_bytes());
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
        24 => Ok(u32::from(next_byte(bytes, i)?)),
        25 => {
            let b0 = next_byte(bytes, i)?;
            let b1 = next_byte(bytes, i)?;
            Ok(u32::from(u16::from_be_bytes([b0, b1])))
        }
        26 => {
            let b0 = next_byte(bytes, i)?;
            let b1 = next_byte(bytes, i)?;
            let b2 = next_byte(bytes, i)?;
            let b3 = next_byte(bytes, i)?;
            Ok(u32::from_be_bytes([b0, b1, b2, b3]))
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
        24 => usize::from(next_byte(bytes, i)?),
        25 => {
            let b0 = next_byte(bytes, i)?;
            let b1 = next_byte(bytes, i)?;
            usize::from(u16::from_be_bytes([b0, b1]))
        }
        26 => {
            let b0 = next_byte(bytes, i)?;
            let b1 = next_byte(bytes, i)?;
            let b2 = next_byte(bytes, i)?;
            let b3 = next_byte(bytes, i)?;
            usize::try_from(u32::from_be_bytes([b0, b1, b2, b3])).map_err(|_| Error::InvalidPartCbor)?
        }
        _ => return Err(Error::InvalidPartCbor),
    };
    if len > max_data_len {
        return Err(Error::ResourceLimit("fragment_data"));
    }
    let end = i
        .checked_add(len)
        .ok_or(Error::InvalidPartCbor)?;
    let slice = bytes.get(*i..end).ok_or(Error::InvalidPartCbor)?;
    *i = end;
    Ok(slice.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_golden() {
        let part = Part::from_fields(1, 9, 256, 23_570_951, hex::decode("916ec65cf77cadf55cd7f9cda1a1030026ddd42e905b77adc36e4f2d3c").unwrap());
        let cbor = encode_part(&part);
        assert_eq!(
            hex::encode(&cbor),
            "8501091901001a0167aa07581d916ec65cf77cadf55cd7f9cda1a1030026ddd42e905b77adc36e4f2d3c"
        );
        let decoded = decode_part(&cbor, 8192).unwrap();
        assert_eq!(decoded, part);
    }
}
