// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Async IEC 60870-5-104 client and server with redundancy and TLS.
//!
//! The [`transport`] module runs one APCI session of `igs-iec104-link` over a byte
//! stream (task T1). The client and server APIs are built on it.

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

pub mod error;
pub mod transport;

pub use error::TransportError;
pub use transport::{connect, run, Command, Delivery, Transport, DEFAULT_PORT};
