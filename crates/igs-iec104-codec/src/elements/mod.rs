// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Information elements of the 104 profile (IEC 60870-5-101 7.2.6, selected
//! by IEC 60870-5-104 section 8).
//!
//! Each element is a type with a `SIZE` (octets on the wire), a `decode` that
//! reads the first `SIZE` octets of a slice and an `encode` that writes them.
//! Bit positions stay internal: the public API exposes named fields.
//!
//! Types with plain flags have public fields, since every combination is
//! valid. Types with a range limit keep their fields private and check the
//! range in `new`, and `decode` goes through `new`, so an out-of-range value
//! cannot be built or read.
//!
//! Encoders clear the reserved bits of their octets. Decoders ignore them.
//!
//! Elements that only appear in type IDs outside the 104 profile (for
//! example FBP) are not implemented.

use crate::formats::{get_bits, set_bits};

/// Defines a whole-octet unsigned element (for example a qualifier) with
/// `SIZE`, `new`, `value`, `decode` and `encode`.
macro_rules! unsigned_element {
    ($(#[$doc:meta])* $name:ident, $ty:ty, size: $size:literal, bits: $bits:literal) => {
        $(#[$doc])*
        #[derive(Copy, Clone, Debug, PartialEq, Eq)]
        pub struct $name($ty);

        impl $name {
            /// Octets on the wire.
            pub const SIZE: usize = $size;

            /// Wraps a value; every value of the type is in range.
            pub const fn new(value: $ty) -> Self {
                Self(value)
            }

            /// The value.
            pub const fn value(self) -> $ty {
                self.0
            }

            /// Decodes the first octets of `src`.
            pub fn decode(src: &[u8]) -> Option<Self> {
                let raw = $crate::formats::get_bits(src, 0, $bits)?;
                Some(Self(<$ty>::try_from(raw).ok()?))
            }

            /// Writes the octets into the front of `dst`.
            pub fn encode(self, dst: &mut [u8]) -> Option<()> {
                $crate::elements::begin(dst, Self::SIZE)?;
                $crate::formats::set_bits(dst, 0, $bits, u64::from(self.0))
            }
        }
    };
}

mod command;
mod file;
mod measured;
mod quality;
mod status;
mod system;
mod time;

#[cfg(test)]
mod tests;

pub use command::{Dco, DoubleCommandState, Qoc, Qos, Rco, RegulatingStep, Sco};
pub use file::{Afq, Chs, Frq, Lof, Los, Lsq, Nof, Nos, Scq, Segment, Sof, Srq};
pub use measured::Vti;
pub use quality::{Diq, DoublePoint, Qdp, Qds, QualityFlags, Sep, Siq};
pub use status::{Bcr, Bsi, Oci, Scd, Spe};
pub use system::{Coi, CounterFreeze, Qcc, Qoi, Qpa, Qpm, Qrp, Tsc};
pub use time::RangeTime;

/// Two octet binary time, elapsed time (101 7.2.6.20).
pub use crate::formats::Cp16Time2a;
/// Normalized value (101 7.2.6.6). Same type as the `formats` fixed-point number.
pub use crate::formats::F16 as Nva;
/// Scaled value (101 7.2.6.7). Same type as the `formats` signed integer.
pub use crate::formats::I16 as Sva;
/// Short floating point number, IEEE STD 754 (101 7.2.6.8). Same type as the
/// `formats` real number.
pub use crate::formats::R32 as ShortFloat;

/// Reads the single-bit flag at zero-based index `bit`.
pub(crate) fn flag(src: &[u8], bit: usize) -> Option<bool> {
    Some(get_bits(src, bit, 1)? == 1)
}

/// Reads `width` bits starting at zero-based index `start`.
pub(crate) fn field(src: &[u8], start: usize, width: usize) -> Option<u64> {
    get_bits(src, start, width)
}

/// Reads `width` bits starting at zero-based index `start` into a `u8`.
pub(crate) fn field_u8(src: &[u8], start: usize, width: usize) -> Option<u8> {
    u8::try_from(field(src, start, width)?).ok()
}

/// Writes a single bit at zero-based index `bit`.
pub(crate) fn put_flag(dst: &mut [u8], bit: usize, on: bool) -> Option<()> {
    set_bits(dst, bit, 1, u64::from(on))
}

/// Writes `width` bits starting at zero-based index `start`.
pub(crate) fn put(dst: &mut [u8], start: usize, width: usize, value: u64) -> Option<()> {
    set_bits(dst, start, width, value)
}

/// Clears the first `size` octets of `dst` so that unwritten (reserved) bits
/// are zero. Fails without writing when `dst` is too short.
pub(crate) fn begin(dst: &mut [u8], size: usize) -> Option<()> {
    dst.get_mut(..size)?.fill(0);
    Some(())
}
