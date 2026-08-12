#![allow(
    unused_crate_dependencies,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::missing_assert_message,
    reason = "demo example prints progress and uses expect for brevity"
)]

//! Print fountain progress while emitting multi-part UR strings.

use bcur::{Decoder, Encoder};

fn main() {
    let data = b"Progress demo payload - multi-part UR scan simulation.".repeat(4);
    let mut encoder = Encoder::bytes(&data, 16).expect("encoder");
    let mut decoder = Decoder::default();

    while !decoder.complete() {
        let part = encoder.next_part().expect("part");
        decoder.receive(&part).expect("receive");
        match decoder.resolved_fragment_count() {
            Some(resolved) => {
                println!(
                    "seq={} resolved={resolved}/{} poisoned={}",
                    encoder.current_index(),
                    decoder.fragment_count(),
                    decoder.is_poisoned()
                );
            }
            None => println!("waiting for first part"),
        }
    }

    let msg = decoder.message().expect("message").expect("complete");
    println!("done: {} bytes recovered", msg.len());
    assert_eq!(msg, data);
}
