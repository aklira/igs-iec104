// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Primitive binary formats of the 104 codec: bit fields, integers, real
//! numbers and time tags. Data and bit layouts come from the transcription
//! of the 5-4 formats (5-4 clauses) and the little-endian octet order of
//! 104 section 9.5.

mod bits;
mod bs;
mod floats;
mod ints;
mod times;

pub use bits::{get_bits, set_bits};
pub use bs::Bs;
pub use floats::{F16, R32};
pub use ints::I16;
pub use times::{Cp16Time2a, Cp24Time2a, Cp56Time2a};
