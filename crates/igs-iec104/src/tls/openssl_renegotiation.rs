// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The only unsafe code of igs-iec104: the calls to OpenSSL that renegotiate a TLS 1.2 session
//! (clause 7.4.5; Q-028, approved by the maintainer).
//!
//! The `openssl` crate has no wrapper for renegotiation, and the raw session pointer is not public.
//! This module declares the three OpenSSL functions it needs, and wraps each one in a safe function.
//! Nothing else in the crate contains unsafe code (`#![deny(unsafe_code)]` in the crate root).

use std::ffi::{c_int, c_long, c_void};

use openssl::ssl::SslRef;

extern "C" {
    /// `int SSL_renegotiate(SSL *s)`: asks for a new handshake on the session. The handshake runs
    /// during the next operation on the session.
    fn SSL_renegotiate(ssl: *mut c_void) -> c_int;
    /// `long SSL_ctrl(SSL *ssl, int cmd, long larg, void *parg)`: the generic control call.
    fn SSL_ctrl(ssl: *mut c_void, cmd: c_int, larg: c_long, parg: *mut c_void) -> c_long;
}

/// The control code that reports the number of renegotiations: `SSL_CTRL_GET_TOTAL_RENEGOTIATIONS`,
/// as the installed `ssl.h` defines it.
const SSL_CTRL_GET_TOTAL_RENEGOTIATIONS: c_int = 12;

/// The OpenSSL pointer of a session. The `openssl` crate builds its wrappers on `foreign-types`,
/// which makes a reference to an OpenSSL object be the object's address; this takes that address.
/// The reference stays valid for the call, so the pointer does too.
fn pointer(ssl: &SslRef) -> *mut c_void {
    (ssl as *const SslRef).cast_mut().cast()
}

/// Asks for a renegotiation of the session. The handshake runs inside the next read or write of the
/// stream: tokio-openssl installs the socket callbacks of OpenSSL only while it polls, so the
/// handshake must not be driven from here. Returns false when OpenSSL refuses (TLS 1.3 sessions are
/// not renegotiated).
pub fn request(ssl: &SslRef) -> bool {
    let pointer = pointer(ssl);
    // SAFETY: `pointer` is the session that `ssl` refers to, alive for the whole call. The caller
    // is the stream that owns the session, and it calls this only when no write is in progress, so
    // no other operation runs on the session at the same time.
    (unsafe { SSL_renegotiate(pointer) }) == 1
}

/// The number of renegotiations of the session so far, on either side.
pub fn total(ssl: &SslRef) -> i64 {
    // SAFETY: as in `request`. The control code only reads the counter; the arguments that it does
    // not use are null.
    let count = unsafe {
        SSL_ctrl(
            pointer(ssl),
            SSL_CTRL_GET_TOTAL_RENEGOTIATIONS,
            0,
            std::ptr::null_mut(),
        )
    };
    count as i64
}
