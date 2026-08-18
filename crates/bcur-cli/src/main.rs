//! `bcur` — encode and decode Uniform Resources, optionally as terminal QR codes.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "CLI writes UR/QR to stdout and progress to stderr"
)]

mod decode;
mod encode;
mod error;
mod io_util;
mod qr;

use std::path::PathBuf;
use std::process::ExitCode;

#[cfg(test)]
use assert_cmd as _;
use clap::{Parser, Subcommand};
#[cfg(test)]
use predicates as _;
#[cfg(test)]
use tempfile as _;

use crate::error::Result;

#[derive(Debug, Parser)]
#[command(
    name = "bcur",
    version,
    about = "Encode and decode Uniform Resources; optional terminal QR"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Encode bytes as UR strings or a terminal QR.
    Encode(encode::EncodeArgs),
    /// Decode UR lines back to the original payload.
    Decode(decode::DecodeArgs),
    /// Render already-encoded UR lines as a terminal QR.
    Qr(QrArgs),
}

#[derive(Debug, clap::Args)]
struct QrArgs {
    /// Frame interval for multi-line animation, in milliseconds.
    #[arg(long, default_value_t = 200)]
    interval_ms: u64,
    /// File of UR lines. Omit or `-` reads stdin.
    input: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bcur: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Encode(args) => encode::run(&args),
        Command::Decode(args) => decode::run(&args),
        Command::Qr(args) => {
            let text = io_util::read_text(args.input.as_deref())?;
            let parts: Vec<String> = text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect();
            qr::animate_parts(&parts, args.interval_ms)
        }
    }
}
