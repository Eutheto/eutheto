#![no_main]

use eutheto_protocol::FrameClass;
use eutheto_protocol::frame::{decode_worker_frame, inspect_checked_in_frame};
use libfuzzer_sys::fuzz_target;
use prost::bytes::Bytes;

fuzz_target!(|data: &[u8]| {
    for class in [FrameClass::Handshake, FrameClass::WorkerEvent] {
        if let Ok(frame) = inspect_checked_in_frame(data, class) {
            let _ = decode_worker_frame(Bytes::copy_from_slice(frame.payload), class);
        }
    }
});
