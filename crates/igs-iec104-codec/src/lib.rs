// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! APCI framing and ASDU encode/decode for IEC 60870-5-104.
//!
//! This crate performs no I/O. See IMPLEMENTATION.md section 2.

#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects
    )
)]

/// Constants generated from the profile data files; see the banner inside.
pub mod generated;

/// Primitive formats: bit fields, integers, real numbers and time tags.
pub mod formats;

/// Information elements of the 104 profile, built on `formats`.
pub mod elements;

/// ASDU data unit identifier and information object address.
pub mod header;

/// Typed errors of the codec.
pub mod error;

/// ASDU body: information objects by type identification, and the profile rules.
pub mod asdu;
