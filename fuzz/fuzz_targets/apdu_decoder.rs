// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Fuzz target of the APCI: frame cutting and frame decoding.
//!
//! Any input must be accepted or rejected without a panic. A frame that
//! decodes must encode back to a frame that decodes to the same value.

#![no_main]

use igs_iec104_codec::apci::{Apdu, FrameDecoder};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = Apdu::decode(data);

    let mut decoder = FrameDecoder::new();
    let mut input = data;
    while let Ok(Some(frame)) = decoder.next_frame(&mut input) {
        if let Ok(apdu) = Apdu::decode(&frame) {
            let encoded = apdu.to_vec().expect("a decoded APDU encodes");
            assert_eq!(Apdu::decode(&encoded), Ok(apdu));
        }
    }
});
