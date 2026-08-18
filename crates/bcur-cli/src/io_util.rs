//! Stdin/file/stdout helpers.

use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

use crate::error::{Error, Result};

/// Reads FILE, `-`, or stdin as raw bytes. `--hex` treats the text as hex.
pub(crate) fn read_bytes(path: Option<&Path>, hex_input: bool) -> Result<Vec<u8>> {
    let raw = read_raw(path)?;
    if hex_input {
        let text = std::str::from_utf8(&raw).map_err(|_| Error::msg("hex input is not UTF-8"))?;
        let compact: String = text.split_whitespace().collect();
        Ok(hex::decode(compact)?)
    } else {
        Ok(raw)
    }
}

/// Reads FILE, `-`, or stdin as UTF-8 text.
pub(crate) fn read_text(path: Option<&Path>) -> Result<String> {
    let raw = read_raw(path)?;
    String::from_utf8(raw).map_err(|_| Error::msg("input is not UTF-8"))
}

/// Writes bytes to FILE, `-`, or stdout. `--hex` emits lowercase hex + newline.
pub(crate) fn write_bytes(path: Option<&Path>, data: &[u8], hex_output: bool) -> Result<()> {
    let out = if hex_output {
        let mut s = hex::encode(data);
        s.push('\n');
        s.into_bytes()
    } else {
        data.to_vec()
    };
    match path {
        Some(p) if p.as_os_str() != "-" => fs::write(p, out)?,
        _ => {
            let mut stdout = io::stdout().lock();
            stdout.write_all(&out)?;
            stdout.flush()?;
        }
    }
    Ok(())
}

fn read_raw(path: Option<&Path>) -> Result<Vec<u8>> {
    match path {
        Some(p) if p.as_os_str() != "-" => Ok(fs::read(p)?),
        _ => {
            let mut buf = Vec::new();
            io::stdin().lock().read_to_end(&mut buf)?;
            Ok(buf)
        }
    }
}
