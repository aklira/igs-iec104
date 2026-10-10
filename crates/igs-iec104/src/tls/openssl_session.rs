// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The only unsafe code of igs-iec104: the calls to OpenSSL that renegotiate a TLS 1.2 session
//! (clause 7.4.5), that update the keys of a TLS 1.3 session (clause 8.4), and the message callback
//! that observes the handshake. Q-028 approved this module for the renegotiation; the key update and
//! the observation of the messages are the same kind of call, and the maintainer approved them.
//!
//! The `openssl` crate has no wrapper for these calls, and the raw session pointer is not public.
//! This module declares the OpenSSL functions it needs, and wraps each one in a safe function.
//! Nothing else in the crate contains unsafe code (`#![deny(unsafe_code)]` in the crate root).

use std::ffi::{c_int, c_long, c_void};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use openssl::ssl::SslRef;

use super::client_hello::{ClientHello, CLIENT_HELLO};
use super::events::{SecurityEvent, SecurityEvents};

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

/// What the message callback of a session observes: the KeyUpdate messages (clause 8.4), and the
/// ClientHello of a peer that connects to a station (clauses 6.2, 7.5.4 and 7.5.7).
pub struct Observed {
    /// The KeyUpdate messages that this side wrote, with `update_requested`.
    written: AtomicU32,
    /// The KeyUpdate messages that this side received, with `update_not_requested`: the answers to
    /// its own requests.
    answered: AtomicU32,
    /// The KeyUpdate messages that this side received, with `update_requested`: the peer's requests.
    requested_by_peer: AtomicU32,
    /// Where the events of a ClientHello go. None for a client, which receives no ClientHello.
    events: Option<Arc<dyn SecurityEvents>>,
    /// A ClientHello offered a CBC suite without encrypt-then-MAC: the session is terminated.
    mac_then_encrypt: AtomicBool,
    /// A ClientHello offered `psk_ke` only: the session is terminated (clause 8.3).
    psk_ke_only: AtomicBool,
}

impl Observed {
    /// The observation of a session. `events` receive the events of the ClientHello of a peer, when
    /// this side is the station.
    pub fn new(events: Option<Arc<dyn SecurityEvents>>) -> Self {
        Self {
            written: AtomicU32::new(0),
            answered: AtomicU32::new(0),
            requested_by_peer: AtomicU32::new(0),
            events,
            mac_then_encrypt: AtomicBool::new(false),
            psk_ke_only: AtomicBool::new(false),
        }
    }

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

    /// True when a ClientHello offered a CBC suite without encrypt-then-MAC (clause 7.5.7).
    pub fn mac_then_encrypt(&self) -> bool {
        self.mac_then_encrypt.load(Ordering::SeqCst)
    }

    /// True when a ClientHello offered `psk_ke` only (clause 8.3).
    pub fn psk_ke_only(&self) -> bool {
        self.psk_ke_only.load(Ordering::SeqCst)
    }
}

/// The OpenSSL pointer of a session. The `openssl` crate builds its wrappers on `foreign-types`,
/// which makes a reference to an OpenSSL object be the object's address; this takes that address.
/// The reference stays valid for the call, so the pointer does too.
fn pointer(ssl: &SslRef) -> *mut c_void {
    (ssl as *const SslRef).cast_mut().cast()
}

/// The message callback of a session. It observes the handshake messages: the ClientHello of the peer,
/// and the KeyUpdate messages in the direction they were written or received.
///
/// # Safety
///
/// OpenSSL calls it with the bytes of a message, valid for `len` octets for the duration of the call,
/// and with the argument that `observe` set: the `Observed` of a stream that is alive for as long as
/// its session is (see `observe`). The events of the observation must not panic: the callback runs
/// inside OpenSSL.
unsafe extern "C" fn observe_messages(
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
    // SAFETY: `arg` is the `Observed` of the stream that installed this callback, alive as the session
    // is (see the safety notes above). It is only read through shared references.
    let observed = unsafe { &*arg.cast::<Observed>() };
    match message.first() {
        Some(&CLIENT_HELLO) if write_p == 0 => observe_client_hello(observed, message),
        Some(&KEY_UPDATE_MESSAGE) => match (write_p, message.get(4)) {
            (1, Some(&UPDATE_REQUESTED)) => {
                observed.written.fetch_add(1, Ordering::SeqCst);
            }
            (0, Some(&UPDATE_NOT_REQUESTED)) => {
                observed.answered.fetch_add(1, Ordering::SeqCst);
            }
            (0, Some(&UPDATE_REQUESTED)) => {
                observed.requested_by_peer.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        },
        _ => {}
    }
}

/// Reads the ClientHello of the peer: its events are raised as it arrives, and a CBC suite without
/// encrypt-then-MAC is recorded for the end of the handshake.
fn observe_client_hello(observed: &Observed, message: &[u8]) {
    let Some(hello) = ClientHello::parse(message) else {
        return;
    };
    let Some(events) = observed.events.as_ref() else {
        return;
    };
    for event in hello.events() {
        events.raise(event, "");
    }
    if hello.mac_then_encrypt() {
        observed.mac_then_encrypt.store(true, Ordering::SeqCst);
        events.raise(
            SecurityEvent::NoEncryptThenMac,
            "a CBC suite without encrypt-then-MAC",
        );
    }
    if hello.psk_ke_only {
        observed.psk_ke_only.store(true, Ordering::SeqCst);
        events.raise(
            SecurityEvent::NoEphemeralPskMode,
            "psk_ke as the only key exchange mode",
        );
    }
}

/// Installs the observation on the session. The observation must live as long as the session: the
/// stream keeps it in an `Arc`, and declares the session field first, so the session is freed first.
pub fn observe(ssl: &SslRef, observed: &Observed) {
    let observed: *const Observed = observed;
    // SAFETY: the session is the one of the reference, alive for the calls. The argument is the
    // `Observed` that the caller keeps alive for the session (see above).
    unsafe {
        SSL_set_msg_callback(pointer(ssl), observe_messages);
        SSL_ctrl(
            pointer(ssl),
            SSL_CTRL_SET_MSG_CALLBACK_ARG,
            0,
            observed.cast_mut().cast(),
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
