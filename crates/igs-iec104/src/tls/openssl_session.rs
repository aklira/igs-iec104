// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The only unsafe code of igs-iec104: the calls to OpenSSL that renegotiate a TLS 1.2 session
//! (clause 7.4.5), that update the keys of a TLS 1.3 session (clause 8.4), and the message callback
//! that counts the KeyUpdate messages. Q-028 approved this module for the renegotiation; the key
//! update and its message callback are the same kind of call, and the maintainer approved them.
//!
//! The `openssl` crate has no wrapper for these calls, and the raw session pointer is not public.
//! This module declares the OpenSSL functions it needs, and wraps each one in a safe function.
//! Nothing else in the crate contains unsafe code (`#![deny(unsafe_code)]` in the crate root).

use std::ffi::{c_int, c_long, c_void};
use std::sync::atomic::{AtomicU32, Ordering};

use openssl::ssl::SslRef;

extern "C" {
    /// `int SSL_renegotiate(SSL *s)`: asks for a new handshake on the session. The handshake runs
    /// during the next operation on the session.
    fn SSL_renegotiate(ssl: *mut c_void) -> c_int;
    /// `int SSL_renegotiate_pending(const SSL *s)`: 1 from the request of a renegotiation until its
    /// handshake has finished, 0 otherwise.
    fn SSL_renegotiate_pending(ssl: *mut c_void) -> c_int;
    /// `long SSL_ctrl(SSL *ssl, int cmd, long larg, void *parg)`: the generic control call.
    fn SSL_ctrl(ssl: *mut c_void, cmd: c_int, larg: c_long, parg: *mut c_void) -> c_long;
    /// `int SSL_key_update(SSL *s, int updatetype)`: queues a KeyUpdate message. The message goes out
    /// with the next handshake call or write of the session.
    fn SSL_key_update(ssl: *mut c_void, updatetype: c_int) -> c_int;
    /// `void SSL_set_msg_callback(SSL *ssl, void (*cb)(...))`: sets the message callback of the session.
    fn SSL_set_msg_callback(ssl: *mut c_void, callback: MessageCallback);
}

/// The message callback of OpenSSL: the direction, the protocol version, the record type, the bytes of
/// the message, their length, the session and the argument that was set with it.
type MessageCallback =
    unsafe extern "C" fn(c_int, c_int, c_int, *const c_void, usize, *mut c_void, *mut c_void);

/// The control code that reports the number of renegotiations: `SSL_CTRL_GET_TOTAL_RENEGOTIATIONS`,
/// as the installed `ssl.h` defines it.
const SSL_CTRL_GET_TOTAL_RENEGOTIATIONS: c_int = 12;
/// The control code that sets the argument of the message callback: `SSL_CTRL_SET_MSG_CALLBACK_ARG`.
const SSL_CTRL_SET_MSG_CALLBACK_ARG: c_int = 16;
/// `SSL_KEY_UPDATE_REQUESTED`: the KeyUpdate asks the peer to update its keys too (clause 8.4).
const SSL_KEY_UPDATE_REQUESTED: c_int = 1;
/// The record type of the handshake messages, `SSL3_RT_HANDSHAKE`.
const SSL3_RT_HANDSHAKE: c_int = 22;
/// The handshake message type of KeyUpdate (RFC 8446, section 4).
const KEY_UPDATE_MESSAGE: u8 = 24;
/// The `request_update` field of a KeyUpdate: `update_not_requested` (the answer) and
/// `update_requested` (the request), as RFC 8446 defines them.
const UPDATE_NOT_REQUESTED: u8 = 0;
const UPDATE_REQUESTED: u8 = 1;

/// The KeyUpdate messages of one session, counted by the message callback.
#[derive(Debug, Default)]
pub struct KeyUpdates {
    /// The KeyUpdate messages that this side wrote, with `update_requested`.
    written: AtomicU32,
    /// The KeyUpdate messages that this side received, with `update_not_requested`: the answers to
    /// its own requests.
    answered: AtomicU32,
    /// The KeyUpdate messages that this side received, with `update_requested`: the peer's requests.
    requested_by_peer: AtomicU32,
}

impl KeyUpdates {
    /// The KeyUpdate messages written with `update_requested`.
    pub fn written(&self) -> u32 {
        self.written.load(Ordering::SeqCst)
    }

    /// The answers received to the KeyUpdate messages that this side wrote.
    pub fn answered(&self) -> u32 {
        self.answered.load(Ordering::SeqCst)
    }

    /// The KeyUpdate messages that the peer wrote with `update_requested`.
    pub fn requested_by_peer(&self) -> u32 {
        self.requested_by_peer.load(Ordering::SeqCst)
    }
}

/// The OpenSSL pointer of a session. The `openssl` crate builds its wrappers on `foreign-types`,
/// which makes a reference to an OpenSSL object be the object's address; this takes that address.
/// The reference stays valid for the call, so the pointer does too.
fn pointer(ssl: &SslRef) -> *mut c_void {
    (ssl as *const SslRef).cast_mut().cast()
}

/// The message callback of a session. It counts the KeyUpdate messages of the handshake layer, in
/// the direction they were written or received, and by their `request_update` field.
///
/// # Safety
///
/// OpenSSL calls it with the bytes of a message, valid for `len` octets for the duration of the call,
/// and with the argument that `observe_key_updates` set: the `KeyUpdates` of a stream that is alive
/// for as long as its session is (see `observe_key_updates`).
unsafe extern "C" fn count_key_updates(
    write_p: c_int,
    _version: c_int,
    content_type: c_int,
    buf: *const c_void,
    len: usize,
    _ssl: *mut c_void,
    arg: *mut c_void,
) {
    if content_type != SSL3_RT_HANDSHAKE || buf.is_null() || arg.is_null() {
        return;
    }
    // SAFETY: `buf` holds `len` octets for the duration of the call (see the safety notes above).
    let message = unsafe { std::slice::from_raw_parts(buf.cast::<u8>(), len) };
    if message.first() != Some(&KEY_UPDATE_MESSAGE) {
        return;
    }
    // SAFETY: `arg` is the `KeyUpdates` of the stream that installed this callback, alive as the
    // session is (see the safety notes above). It is only read through shared references.
    let counters = unsafe { &*arg.cast::<KeyUpdates>() };
    match (write_p, message.get(4)) {
        (1, Some(&UPDATE_REQUESTED)) => {
            counters.written.fetch_add(1, Ordering::SeqCst);
        }
        (0, Some(&UPDATE_NOT_REQUESTED)) => {
            counters.answered.fetch_add(1, Ordering::SeqCst);
        }
        (0, Some(&UPDATE_REQUESTED)) => {
            counters.requested_by_peer.fetch_add(1, Ordering::SeqCst);
        }
        _ => {}
    }
}

/// Installs the message callback on the session, which counts into `counters`.
///
/// The counters must live as long as the session: the stream keeps them in an `Arc` and declares
/// the session field first, so that the session is freed before the counters.
pub fn observe_key_updates(ssl: &SslRef, counters: &KeyUpdates) {
    let counters: *const KeyUpdates = counters;
    // SAFETY: the session is the one of the reference, alive for the calls. The argument is the
    // `KeyUpdates` the caller keeps alive for the session (see above).
    unsafe {
        SSL_set_msg_callback(pointer(ssl), count_key_updates);
        SSL_ctrl(
            pointer(ssl),
            SSL_CTRL_SET_MSG_CALLBACK_ARG,
            0,
            counters.cast_mut().cast(),
        );
    }
}

/// True while a renegotiation is requested and its handshake has not finished: a request is answered
/// when this turns false.
pub fn pending(ssl: &SslRef) -> bool {
    // SAFETY: the pointer is the session of the reference, valid for the call (see `pointer`).
    (unsafe { SSL_renegotiate_pending(pointer(ssl)) }) != 0
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

/// Asks the peer to update its keys too (clause 8.4): queues a KeyUpdate with `update_requested`.
/// The message goes out with the next handshake call or write of the session. Returns false when
/// OpenSSL refuses (TLS 1.2 sessions have no KeyUpdate).
pub fn request_key_update(ssl: &SslRef) -> bool {
    // SAFETY: as in `request`: the session is alive for the call, and the caller runs no other
    // operation on it at the same time.
    (unsafe { SSL_key_update(pointer(ssl), SSL_KEY_UPDATE_REQUESTED) }) == 1
}

/// The number of renegotiations that have started on the session, on either side.
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
