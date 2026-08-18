//! `bcur encode` — bytes to UR strings or terminal QR.

use std::path::PathBuf;

use bcur::{Encoder, UrType, encode};
use clap::Args;

use crate::error::{Error, Result};
use crate::io_util::read_bytes;
use crate::qr::{animate_encoder, show_static, tty_max_chars};

/// Encode a payload as Uniform Resource strings or a terminal QR.
#[derive(Debug, Args)]
pub(crate) struct EncodeArgs {
    /// UR type token (`[a-z0-9-]+`). Not a schema: payload bytes are unchanged.
    #[arg(long = "type", default_value = "bytes")]
    ur_type: String,
    /// Fountain fragment payload size in bytes. Default: fit `--max-chars`.
    #[arg(long)]
    max_fragment: Option<usize>,
    /// Max characters per UR. Default: 400 (text) or terminal-derived (with `--qr`).
    #[arg(long)]
    max_chars: Option<usize>,
    /// Treat input as hex (whitespace ignored).
    #[arg(long)]
    hex: bool,
    /// Render a terminal QR (animates fountain parts until quit).
    #[arg(long)]
    qr: bool,
    /// Use the Encoder path and animate QR. Still single-part if the payload
    /// fits one fragment; lower `--max-chars` to force fountain.
    #[arg(long)]
    animate: bool,
    /// Fountain parts to print in text mode. Default: `max(3K, 20)`.
    #[arg(long)]
    count: Option<u32>,
    /// Frame interval for animated QR, in milliseconds.
    #[arg(long, default_value_t = 200)]
    interval_ms: u64,
    /// Input file. Omit or `-` reads stdin.
    input: Option<PathBuf>,
}

pub(crate) fn run(args: &EncodeArgs) -> Result<()> {
    let max_chars = resolve_max_chars(args)?;
    let data = read_bytes(args.input.as_deref(), args.hex)?;
    let ur_type = UrType::new(&args.ur_type)?;
    let single = encode(&data, &ur_type);
    let use_single = !args.animate && single.len() <= max_chars;

    if use_single {
        if args.qr {
            show_static(&single)?;
        } else {
            println!("{single}");
        }
        return Ok(());
    }

    if data.is_empty() {
        return Err(Error::msg(
            "empty payload cannot be fountain-encoded; omit --animate",
        ));
    }

    let fragment_len = match args.max_fragment {
        Some(n) => n,
        None => fit_fragment_len(&data, &ur_type, max_chars)?,
    };
    if fragment_len == 0 {
        return Err(Error::msg("--max-fragment must be greater than zero"));
    }
    let mut encoder = Encoder::new(&data, fragment_len, &ur_type)?;

    if args.qr {
        let first = encoder.next_part()?;
        if first.len() > max_chars {
            return Err(Error::msg(format!(
                "first part is {} chars (limit {max_chars}); lower --max-fragment or raise --max-chars",
                first.len()
            )));
        }
        animate_encoder(&mut encoder, Some(first), args.interval_ms)?;
    } else {
        let k = encoder.fragment_count();
        let n = args.count.unwrap_or_else(|| k.saturating_mul(3).max(20));
        for _ in 0..n {
            println!("{}", encoder.next_part()?);
        }
    }
    Ok(())
}

fn resolve_max_chars(args: &EncodeArgs) -> Result<usize> {
    if let Some(n) = args.max_chars {
        if n == 0 {
            return Err(Error::msg("--max-chars must be greater than zero"));
        }
        return Ok(n);
    }
    if args.qr {
        Ok(tty_max_chars().unwrap_or(80))
    } else {
        Ok(400)
    }
}

/// Slack for later sequence numbers / longer CBOR integers vs the first part.
const PART_LEN_SLACK: usize = 12;

/// Largest fragment length whose first part URI plus slack is `<= max_chars`.
fn fit_fragment_len(data: &[u8], ur_type: &UrType, max_chars: usize) -> Result<usize> {
    let mut lo = 1_usize;
    let mut hi = data.len();
    let mut best = None;
    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        match measure_part(data, ur_type, mid) {
            Ok(len) if len.saturating_add(PART_LEN_SLACK) <= max_chars => {
                best = Some(mid);
                lo = mid.saturating_add(1);
            }
            Ok(_) | Err(Error::Bcur(_)) => {
                hi = mid.saturating_sub(1);
            }
            Err(e) => return Err(e),
        }
    }
    best.ok_or_else(|| {
        Error::msg(format!(
            "cannot fit a fountain part into {max_chars} chars; raise --max-chars"
        ))
    })
}

fn measure_part(data: &[u8], ur_type: &UrType, fragment_len: usize) -> Result<usize> {
    let mut encoder = Encoder::new(data, fragment_len, ur_type)?;
    Ok(encoder.next_part()?.len())
}
