// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The `Wire` trait: how one value of an information object is laid out on
//! the wire. Implemented by every element of `elements`, by tuples of values
//! (an information object with several elements), by values followed by a
//! CP56Time2a, and by the file segment whose length is given at run time.

use crate::elements::{
    Afq, Bcr, Bsi, Chs, Coi, Dco, Diq, Frq, Lof, Los, Lsq, Nof, Nos, Nva, Oci, Qcc, Qdp, Qds, Qoi,
    Qos, Qpa, Qpm, Qrp, RangeTime, Rco, Scd, Sco, Scq, Segment, Sep, ShortFloat, Siq, Sof, Spe,
    Srq, Sva, Tsc, Vti,
};
use crate::error::{DecodeError, EncodeError};
use crate::formats::{Cp16Time2a, Cp56Time2a};

/// A value of an information object, as laid out on the wire.
pub trait Wire: Sized {
    /// Octets of the value when it has a fixed size, `None` otherwise.
    const SIZE: Option<usize>;

    /// Decodes the value at the front of `src`. Returns it with the octets it used.
    fn decode_prefix(src: &[u8]) -> Result<(Self, usize), DecodeError>;

    /// Encodes the value at the front of `dst`. Returns the octets it wrote.
    fn encode_prefix(&self, dst: &mut [u8]) -> Result<usize, EncodeError>;
}

/// Adds two octet counts; an overflow can only come from absurd input and is
/// reported as a length no buffer can hold.
pub(crate) fn add(a: usize, b: usize) -> usize {
    a.saturating_add(b)
}

/// The sum of two fixed sizes, or `None` as soon as one of them is variable.
pub(crate) const fn add_sizes(a: Option<usize>, b: Option<usize>) -> Option<usize> {
    match (a, b) {
        (Some(a), Some(b)) => a.checked_add(b),
        _ => None,
    }
}

/// The source after the first `used` octets.
pub(crate) fn skip(src: &[u8], used: usize) -> Result<&[u8], DecodeError> {
    src.get(used..).ok_or(DecodeError::Truncated {
        needed: used,
        available: src.len(),
    })
}

/// The destination after the first `used` octets.
pub(crate) fn skip_mut(dst: &mut [u8], used: usize) -> Result<&mut [u8], EncodeError> {
    let available = dst.len();
    dst.get_mut(used..).ok_or(EncodeError::BufferTooSmall {
        needed: used,
        available,
    })
}

/// Implements `Wire` for a fixed-size element with `SIZE`, `decode` and `encode`.
/// The name is the element's name in IEC 60870-5-101 7.2.6, used in errors.
macro_rules! wire_element {
    ($($ty:ty => $name:literal),* $(,)?) => {$(
        impl Wire for $ty {
            const SIZE: Option<usize> = Some(<$ty>::SIZE);

            fn decode_prefix(src: &[u8]) -> Result<(Self, usize), DecodeError> {
                if src.len() < <$ty>::SIZE {
                    return Err(DecodeError::Truncated {
                        needed: <$ty>::SIZE,
                        available: src.len(),
                    });
                }
                let value = <$ty>::decode(src).ok_or(DecodeError::InvalidElement {
                    element: $name,
                })?;
                Ok((value, <$ty>::SIZE))
            }

            fn encode_prefix(&self, dst: &mut [u8]) -> Result<usize, EncodeError> {
                if dst.len() < <$ty>::SIZE {
                    return Err(EncodeError::BufferTooSmall {
                        needed: <$ty>::SIZE,
                        available: dst.len(),
                    });
                }
                (*self).encode(dst).ok_or(EncodeError::BufferTooSmall {
                    needed: <$ty>::SIZE,
                    available: dst.len(),
                })?;
                Ok(<$ty>::SIZE)
            }
        }
    )*};
}

wire_element! {
    Siq => "SIQ",
    Diq => "DIQ",
    Qds => "QDS",
    Vti => "VTI",
    Bsi => "BSI",
    Nva => "NVA",
    Sva => "SVA",
    ShortFloat => "IEEE STD 754",
    Bcr => "BCR",
    Scd => "SCD",
    Sep => "SEP",
    Spe => "SPE",
    Qdp => "QDP",
    Oci => "OCI",
    Sco => "SCO",
    Dco => "DCO",
    Rco => "RCO",
    Qos => "QOS",
    Coi => "COI",
    Qoi => "QOI",
    Qcc => "QCC",
    Qrp => "QRP",
    Tsc => "TSC",
    Qpm => "QPM",
    Qpa => "QPA",
    Nof => "Name of file",
    Nos => "Name of section",
    Lof => "Length of file",
    Los => "Length of segment",
    Chs => "Checksum",
    Lsq => "LSQ",
    Sof => "Status of file",
    Frq => "FRQ",
    Srq => "SRQ",
    Scq => "SCQ",
    Afq => "AFQ",
    RangeTime => "RangeStartTime",
    Cp16Time2a => "CP16Time2a",
    Cp56Time2a => "CP56Time2a",
}

/// An information object without elements: C_RD_NA_1 carries only its address.
impl Wire for () {
    const SIZE: Option<usize> = Some(0);

    fn decode_prefix(_src: &[u8]) -> Result<(Self, usize), DecodeError> {
        Ok(((), 0))
    }

    fn encode_prefix(&self, _dst: &mut [u8]) -> Result<usize, EncodeError> {
        Ok(0)
    }
}

/// Implements `Wire` for a tuple of values, laid out one after the other in
/// the order of the tuple. The `$idx` are the tuple indexes.
macro_rules! wire_tuple {
    ($($ty:ident $idx:tt),+) => {
        impl<$($ty: Wire),+> Wire for ($($ty,)+) {
            const SIZE: Option<usize> = sum_sizes!($($ty),+);

            fn decode_prefix(src: &[u8]) -> Result<(Self, usize), DecodeError> {
                let mut used = 0;
                let values = ($(
                    {
                        let (value, n) = <$ty as Wire>::decode_prefix(skip(src, used)?)?;
                        used = add(used, n);
                        value
                    },
                )+);
                Ok((values, used))
            }

            fn encode_prefix(&self, dst: &mut [u8]) -> Result<usize, EncodeError> {
                let mut used = 0;
                $(
                    used = add(used, self.$idx.encode_prefix(skip_mut(dst, used)?)?);
                )+
                Ok(used)
            }
        }
    };
}

/// The fixed size of a sequence of `Wire` types, summed.
macro_rules! sum_sizes {
    ($t:ident) => {
        <$t as Wire>::SIZE
    };
    ($t:ident, $($rest:ident),+) => {
        add_sizes(<$t as Wire>::SIZE, sum_sizes!($($rest),+))
    };
}

wire_tuple!(A 0, B 1);
wire_tuple!(A 0, B 1, C 2);
wire_tuple!(A 0, B 1, C 2, D 3);

/// A value followed by its CP56Time2a time tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Timed<T> {
    /// The value (its elements).
    pub value: T,
    /// The time tag, CP56Time2a (101 7.2.6.18).
    pub time: Cp56Time2a,
}

impl<T: Wire> Wire for Timed<T> {
    const SIZE: Option<usize> = add_sizes(T::SIZE, <Cp56Time2a as Wire>::SIZE);

    fn decode_prefix(src: &[u8]) -> Result<(Self, usize), DecodeError> {
        let (value, used) = T::decode_prefix(src)?;
        let (time, time_used) = <Cp56Time2a as Wire>::decode_prefix(skip(src, used)?)?;
        Ok((Self { value, time }, add(used, time_used)))
    }

    fn encode_prefix(&self, dst: &mut [u8]) -> Result<usize, EncodeError> {
        let used = self.value.encode_prefix(dst)?;
        let time_used = self.time.encode_prefix(skip_mut(dst, used)?)?;
        Ok(add(used, time_used))
    }
}

/// The file segment F_SG_NA_1 (101 7.3.6.6): name of file, name of section,
/// the length of the segment (LOS) and the segment octets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSegment {
    /// Name of file.
    pub name_of_file: Nof,
    /// Name of section.
    pub name_of_section: Nos,
    /// The segment octets; their count is the length of segment.
    pub segment: Segment,
}

impl Wire for FileSegment {
    const SIZE: Option<usize> = None;

    fn decode_prefix(src: &[u8]) -> Result<(Self, usize), DecodeError> {
        let (name_of_file, a) = <Nof as Wire>::decode_prefix(src)?;
        let (name_of_section, b) = <Nos as Wire>::decode_prefix(skip(src, a)?)?;
        let (length, c) = <Los as Wire>::decode_prefix(skip(src, add(a, b))?)?;
        let start = add(add(a, b), c);
        let segment = Segment::decode(skip(src, start)?, usize::from(length.value())).ok_or(
            DecodeError::Truncated {
                needed: add(start, usize::from(length.value())),
                available: src.len(),
            },
        )?;
        let used = add(start, usize::from(length.value()));
        Ok((
            Self {
                name_of_file,
                name_of_section,
                segment,
            },
            used,
        ))
    }

    fn encode_prefix(&self, dst: &mut [u8]) -> Result<usize, EncodeError> {
        let length =
            u8::try_from(self.segment.data().len()).map_err(|_| EncodeError::ValueOutOfRange {
                field: "Length of segment",
            })?;
        let a = self.name_of_file.encode_prefix(dst)?;
        let b = self.name_of_section.encode_prefix(skip_mut(dst, a)?)?;
        let c = <Los as Wire>::encode_prefix(&Los::new(length), skip_mut(dst, add(a, b))?)?;
        let start = add(add(a, b), c);
        let needed = self.segment.data().len();
        let total = add(start, needed);
        let available = dst.len();
        let window =
            skip_mut(dst, start)?
                .get_mut(..needed)
                .ok_or(EncodeError::BufferTooSmall {
                    needed: total,
                    available,
                })?;
        window.copy_from_slice(self.segment.data());
        Ok(total)
    }
}
