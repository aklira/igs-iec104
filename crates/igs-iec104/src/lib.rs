// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Async IEC 60870-5-104 client and server with redundancy and TLS.
//!
//! The [`transport`] module runs one APCI session of `igs-iec104-link` over a byte
//! stream (task T1). The [`client`] keeps a connection up and offers the procedures
//! of §7 (task T2); the [`procedures`] build their ASDUs.

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

pub mod client;
pub mod error;
pub mod procedures;
pub mod transport;

pub use client::{Client, ClientConfig, ClientError, Event, ReconnectPolicy};
pub use error::TransportError;
pub use procedures::ProcedureError;
pub use transport::{connect, run, run_shared, Command, Delivery, Transport, DEFAULT_PORT};
