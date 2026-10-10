// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Sans-I/O APCI session state machine for IEC 60870-5-104.
//!
//! Takes events (bytes received, current instant, user requests) and returns
//! actions (frames to send, ASDUs to deliver, rejections, close). The caller asks
//! `next_deadline` when the next timer is due.
//! See IMPLEMENTATION.md section 2.

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

/// Session parameters: timeouts t0 to t3, window sizes k and w, and the role (task L1).
pub mod config;

pub use config::{ConfigError, LinkConfig, Parameters, Recommendation, Role};

/// The APCI session state machine: sequence numbers, window, acknowledgements and the
/// start and stop of the data transfer (task L2), sans I/O.
pub mod session;

pub use session::{Action, CloseReason, Event, Rejection, Request, Session, TransferState};
