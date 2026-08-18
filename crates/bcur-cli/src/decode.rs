//! `bcur decode` — UR strings back to payload bytes.

use std::path::PathBuf;

use bcur::{Decoder, DecoderLimits, UrType};
use clap::Args;

use crate::error::{Error, Result};
use crate::io_util::{read_text, write_bytes};

/// Decode single-part or fountain UR lines to the original payload.
#[derive(Debug, Args)]
pub(crate) struct DecodeArgs {
    /// Require this UR type on every part.
    #[arg(long = "type")]
    ur_type: Option<String>,
    /// Write the payload as lowercase hex.
    #[arg(long)]
    hex: bool,
    /// Output file. Omit or `-` writes stdout.
    #[arg(short, long)]
    out: Option<PathBuf>,
    /// Input file of UR lines. Omit or `-` reads stdin.
    input: Option<PathBuf>,
}

pub(crate) fn run(args: &DecodeArgs) -> Result<()> {
    let text = read_text(args.input.as_deref())?;
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return Err(Error::msg("no UR lines in input"));
    }

    let mut decoder = Decoder::with_limits(DecoderLimits::default());
    if let Some(t) = args.ur_type.as_deref() {
        decoder = decoder.with_expected_type(UrType::new(t)?);
    }

    for (idx, line) in lines.iter().enumerate() {
        decoder.receive(line)?;
        if let Some(resolved) = decoder.resolved_fragment_count() {
            eprintln!(
                "part {} resolved={resolved}/{}",
                idx + 1,
                decoder.fragment_count()
            );
        }
        if decoder.complete() {
            let data = decoder
                .message()?
                .ok_or_else(|| Error::msg("decoder complete without message"))?;
            return write_bytes(args.out.as_deref(), &data, args.hex);
        }
    }
    Err(Error::msg(
        "stream ended before the fountain message completed",
    ))
}
