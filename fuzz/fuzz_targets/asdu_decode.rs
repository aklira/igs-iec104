// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Fuzz target of the ASDU: header, information objects and profile rules.
//!
//! Any input must be accepted or rejected without a panic. An ASDU that
//! decodes must encode back to an ASDU that decodes to the same value, and the
//! profile check must answer without a panic.

#![no_main]

use igs_iec104_codec::asdu::{validate_profile, Asdu, Direction};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(asdu) = Asdu::decode(data) {
        let _ = validate_profile(&asdu, Direction::Monitor);
        let _ = validate_profile(&asdu, Direction::Control);
        let encoded = asdu.to_vec().expect("a decoded ASDU encodes");
        assert_eq!(Asdu::decode(&encoded), Ok(asdu));
    }
});
