// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Async IEC 60870-5-104 client and server with redundancy and TLS.
//!
//! The [`transport`] module runs one APCI session of `igs-iec104-link` over a byte
//! stream (task T1). The [`client`] keeps a connection up and offers the procedures
//! of §7 (task T2); the [`procedures`] build their ASDUs. The [`server`] is the controlled
//! station (task S2), which answers them from its [`process_image`] (task S1); the
//! [`clock`] gives the CP56Time2a of this machine.

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
pub mod clock;
pub mod error;
pub mod procedures;
pub mod process_image;
pub mod server;
pub mod transport;

pub use client::{Client, ClientConfig, ClientError, Event, ReconnectPolicy};
pub use error::TransportError;
pub use procedures::ProcedureError;
pub use transport::{connect, run, run_shared, Command, Delivery, Transport, DEFAULT_PORT};
