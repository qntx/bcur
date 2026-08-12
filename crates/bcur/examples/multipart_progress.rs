//! Print fountain progress while emitting multi-part UR strings.

#![allow(
    unused_crate_dependencies,
    clippy::print_stdout,
    reason = "examples inherit package deps; progress output is the point of the demo"
)]

use bcur::{Decoder, Encoder};

fn main() -> Result<(), bcur::Error> {
    let data = b"Progress demo payload - multi-part UR scan simulation.".repeat(4);
    let mut encoder = Encoder::bytes(&data, 16)?;
    let mut decoder = Decoder::default();

    while !decoder.complete() {
        let part = encoder.next_part()?;
        decoder.receive(&part)?;
        if let Some(resolved) = decoder.resolved_fragment_count() {
            println!(
                "seq={} resolved={resolved}/{} poisoned={}",
                encoder.current_index(),
                decoder.fragment_count(),
                decoder.is_poisoned()
            );
        }
    }

    let msg = decoder.message()?.ok_or(bcur::Error::DecoderState)?;
    println!("done: {} bytes recovered", msg.len());
    if msg != data {
        return Err(bcur::Error::DecoderState);
    }
    Ok(())
}
