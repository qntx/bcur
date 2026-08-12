//! Print fountain progress while emitting multi-part UR strings.

use std::io::{self, Write};

use bcur::{Decoder, Encoder};

fn main() -> Result<(), bcur::Error> {
    let data = b"Progress demo payload - multi-part UR scan simulation.".repeat(4);
    let mut encoder = Encoder::bytes(&data, 16)?;
    let mut decoder = Decoder::default();
    let mut out = io::stdout();

    while !decoder.complete() {
        let part = encoder.next_part()?;
        decoder.receive(&part)?;
        if let Some(resolved) = decoder.resolved_fragment_count() {
            let line = format!(
                "seq={} resolved={resolved}/{} poisoned={}\n",
                encoder.current_index(),
                decoder.fragment_count(),
                decoder.is_poisoned()
            );
            out.write_all(line.as_bytes())
                .map_err(|_| bcur::Error::DecoderState)?;
        }
    }

    let msg = decoder.message()?.ok_or(bcur::Error::DecoderState)?;
    let done = format!("done: {} bytes recovered\n", msg.len());
    out.write_all(done.as_bytes())
        .map_err(|_| bcur::Error::DecoderState)?;
    if msg != data {
        return Err(bcur::Error::DecoderState);
    }
    Ok(())
}
