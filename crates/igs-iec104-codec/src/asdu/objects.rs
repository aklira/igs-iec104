// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The information objects of an ASDU in its two addressing modes (101 7.2.2).

use super::wire::{add, skip, skip_mut, Wire};
use crate::error::{DecodeError, EncodeError};
use crate::header::InformationObjectAddress;

/// Largest number of objects, or of element sets, in a variable structure
/// qualifier (seven bits).
pub const MAX_OBJECTS: usize = 127;

/// One information object: its address and its value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InformationObject<T> {
    /// The information object address (101 7.2.5).
    pub address: InformationObjectAddress,
    /// The value: its elements, and its time tag when the type has one.
    pub value: T,
}

/// The information objects of an ASDU.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Objects<T> {
    /// SQ = 0: each object carries its own address.
    Individual(Vec<InformationObject<T>>),
    /// SQ = 1: one address, then consecutive values from that address.
    Sequence {
        /// Address of the first value.
        address: InformationObjectAddress,
        /// The values, at consecutive addresses.
        values: Vec<T>,
    },
}

impl<T: Wire> Objects<T> {
    /// Number of objects (or of values) carried.
    pub fn len(&self) -> usize {
        match self {
            Self::Individual(objects) => objects.len(),
            Self::Sequence { values, .. } => values.len(),
        }
    }

    /// True when no object is carried.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// True for the SQ = 1 form with at least one value. An empty sequence is
    /// sent as an empty individual list: no address is sent for zero objects.
    pub fn is_sequence(&self) -> bool {
        matches!(self, Self::Sequence { values, .. } if !values.is_empty())
    }

    /// Writes the objects into the front of `dst`. Returns the octets written.
    pub fn encode(&self, dst: &mut [u8]) -> Result<usize, EncodeError> {
        if self.len() > MAX_OBJECTS {
            return Err(EncodeError::TooManyObjects { count: self.len() });
        }
        let mut used = 0;
        match self {
            Self::Individual(objects) => {
                for object in objects {
                    object.address.encode(skip_mut(dst, used)?)?;
                    used = add(used, InformationObjectAddress::SIZE);
                    used = add(used, object.value.encode_prefix(skip_mut(dst, used)?)?);
                }
            }
            Self::Sequence { address, values } => {
                if !values.is_empty() {
                    address.encode(skip_mut(dst, used)?)?;
                    used = add(used, InformationObjectAddress::SIZE);
                    for value in values {
                        used = add(used, value.encode_prefix(skip_mut(dst, used)?)?);
                    }
                }
            }
        }
        Ok(used)
    }
}

/// Decodes the objects of an ASDU body. `number` and `sequence` are the fields
/// of the variable structure qualifier; `body` is everything after the header.
///
/// The number of objects is checked against the octets left before any value
/// is read, when the value has a fixed size, so that a bogus count cannot make
/// the decoder read past the end.
pub fn decode<T: Wire>(body: &[u8], number: u8, sequence: bool) -> Result<Objects<T>, DecodeError> {
    let count = usize::from(number);
    if count == 0 {
        ensure_exhausted(body)?;
        return Ok(Objects::Individual(Vec::new()));
    }
    check_length::<T>(body, count, sequence)?;

    let mut rest = body;
    if sequence {
        let address = InformationObjectAddress::decode(rest)?;
        rest = skip(rest, InformationObjectAddress::SIZE)?;
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            let (value, used) = T::decode_prefix(rest)?;
            values.push(value);
            rest = skip(rest, used)?;
        }
        ensure_exhausted(rest)?;
        Ok(Objects::Sequence { address, values })
    } else {
        let mut objects = Vec::with_capacity(count);
        for _ in 0..count {
            let address = InformationObjectAddress::decode(rest)?;
            rest = skip(rest, InformationObjectAddress::SIZE)?;
            let (value, used) = T::decode_prefix(rest)?;
            objects.push(InformationObject { address, value });
            rest = skip(rest, used)?;
        }
        ensure_exhausted(rest)?;
        Ok(Objects::Individual(objects))
    }
}

/// Fails when the body is shorter than `count` objects of a fixed size need.
fn check_length<T: Wire>(body: &[u8], count: usize, sequence: bool) -> Result<(), DecodeError> {
    let Some(size) = T::SIZE else {
        return Ok(());
    };
    let needed = if sequence {
        count
            .checked_mul(size)
            .map(|octets| add(octets, InformationObjectAddress::SIZE))
    } else {
        count.checked_mul(add(InformationObjectAddress::SIZE, size))
    }
    .unwrap_or(usize::MAX);
    if needed > body.len() {
        return Err(DecodeError::Truncated {
            needed,
            available: body.len(),
        });
    }
    Ok(())
}

fn ensure_exhausted(rest: &[u8]) -> Result<(), DecodeError> {
    if rest.is_empty() {
        Ok(())
    } else {
        Err(DecodeError::TrailingOctets { extra: rest.len() })
    }
}
