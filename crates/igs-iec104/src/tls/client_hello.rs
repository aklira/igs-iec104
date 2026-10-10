// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The fields of a ClientHello that the security events of clauses 6.2, 7.5.4 and 7.5.7 need. The
//! message comes from OpenSSL's message callback, with its 4-octet handshake header and the body of
//! RFC 5246 (section 7.4.1.2) and RFC 8446 (section 4.1.2). A message that does not parse gives no
//! fields, and no event: the handshake then decides.

use super::events::SecurityEvent;
use super::profile::TLS12_SUITES;

/// The handshake type of a ClientHello (RFC 5246, section 7.4).
pub const CLIENT_HELLO: u8 = 1;
/// The protocol version TLS 1.0, which is the record version of SSL 3.1 (RFC 2246, appendix A.4).
const TLS_1_0: u16 = 0x0301;
/// The protocol version TLS 1.2 (RFC 5246, appendix E).
const TLS_1_2: u16 = 0x0303;
/// The extension `signature_algorithms` (RFC 5246, section 7.4.1.4.1).
const EXTENSION_SIGNATURE_ALGORITHMS: u16 = 13;
/// The extension `encrypt_then_mac` (RFC 7366, section 2).
const EXTENSION_ENCRYPT_THEN_MAC: u16 = 22;
/// The extension `supported_versions` (RFC 8446, section 4.2.1).
const EXTENSION_SUPPORTED_VERSIONS: u16 = 43;
/// The extension `psk_key_exchange_modes` (RFC 8446, section 4.2.9).
const EXTENSION_PSK_KEY_EXCHANGE_MODES: u16 = 45;
/// The extension `early_data` (RFC 8446, section 4.2.10).
const EXTENSION_EARLY_DATA: u16 = 42;
/// The key exchange mode `psk_ke` (RFC 8446, section 4.2.9): a PSK without an ephemeral key.
const PSK_KE: u8 = 0;
/// The hash and the signature of {SHA-1, RSA} in the `signature_algorithms` extension (RFC 5246,
/// section 7.4.1.4.1): the hash `sha1` and the signature `rsa`.
const HASH_SHA1: u8 = 2;
const SIGNATURE_RSA: u8 = 1;

/// The fields of a ClientHello that the events need.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientHello {
    /// The highest version that the client offers: the highest of `supported_versions`, or else the
    /// legacy version of the message.
    pub highest_version: u16,
    /// The `signature_algorithms` extension is present (clause 7.5.4).
    pub signature_algorithms: bool,
    /// The `signature_algorithms` extension signals {SHA-1, RSA} (clause 7.5.4).
    pub sha1_rsa: bool,
    /// The `encrypt_then_mac` extension is present (clause 7.5.7).
    pub encrypt_then_mac: bool,
    /// The client offers a CBC cipher suite of the profile (clause 7.5.7).
    pub cbc: bool,
    /// The client offers `psk_ke` as its only key exchange mode (clause 8.3).
    pub psk_ke_only: bool,
    /// The client sends early data (clause 8.8.11).
    pub early_data: bool,
}

impl ClientHello {
    /// Reads a handshake message of the client, with its 4-octet header. None when it does not parse.
    pub fn parse(message: &[u8]) -> Option<Self> {
        let mut reader = Reader::new(message);
        if reader.u8()? != CLIENT_HELLO {
            return None;
        }
        let length = reader.u24()?;
        let mut body = Reader::new(reader.take(length)?);
        let legacy_version = body.u16()?;
        body.take(32)?;
        let session_length = body.u8()?;
        body.take(usize::from(session_length))?;
        let suites_length = body.u16()?;
        let suites = body.take(usize::from(suites_length))?;
        let compression_length = body.u8()?;
        body.take(usize::from(compression_length))?;
        let mut hello = Self {
            highest_version: legacy_version,
            signature_algorithms: false,
            sha1_rsa: false,
            encrypt_then_mac: false,
            cbc: offers_cbc(suites),
            psk_ke_only: false,
            early_data: false,
        };
        // The extensions are optional: a body that ends here has none (RFC 5246, section 7.4.1.2).
        if body.is_empty() {
            return Some(hello);
        }
        let extensions_length = body.u16()?;
        let mut extensions = Reader::new(body.take(usize::from(extensions_length))?);
        while !extensions.is_empty() {
            let kind = extensions.u16()?;
            let length = extensions.u16()?;
            let data = extensions.take(usize::from(length))?;
            match kind {
                EXTENSION_SUPPORTED_VERSIONS => hello.highest_version = highest_version(data)?,
                EXTENSION_SIGNATURE_ALGORITHMS => {
                    hello.signature_algorithms = true;
                    hello.sha1_rsa = signals_sha1_rsa(data)?;
                }
                EXTENSION_ENCRYPT_THEN_MAC => hello.encrypt_then_mac = true,
                EXTENSION_PSK_KEY_EXCHANGE_MODES => hello.psk_ke_only = only_psk_ke(data)?,
                EXTENSION_EARLY_DATA => hello.early_data = true,
                _ => {}
            }
        }
        Some(hello)
    }

    /// The events that the hello raises: the versions (clause 6.2), the signature algorithms (clause
    /// 7.5.4) and the early data (clause 8.8.11). The checks that end the session are `mac_then_encrypt`
    /// (clause 7.5.7) and `psk_ke_only` (clause 8.3).
    pub fn events(&self) -> Vec<SecurityEvent> {
        let mut events = Vec::new();
        if self.highest_version < TLS_1_0 {
            events.push(SecurityEvent::DisallowedVersion);
        } else if self.highest_version < TLS_1_2 {
            events.push(SecurityEvent::DeprecatedVersion);
        }
        if !self.signature_algorithms {
            events.push(SecurityEvent::NoSignatureAlgorithmExtension);
        }
        if self.sha1_rsa {
            events.push(SecurityEvent::DeprecatedSignatureAlgorithm);
        }
        if self.early_data {
            events.push(SecurityEvent::EarlyDataIgnored);
        }
        events
    }

    /// True when the client offers a CBC cipher suite without encrypt-then-MAC (clause 7.5.7): the session
    /// is then terminated.
    pub fn mac_then_encrypt(&self) -> bool {
        self.cbc && !self.encrypt_then_mac
    }
}

/// True when one of the suites is a CBC suite of the profile.
fn offers_cbc(suites: &[u8]) -> bool {
    suites.chunks_exact(2).any(|pair| {
        let code = u16::from_be_bytes(<[u8; 2]>::try_from(pair).unwrap_or([0, 0]));
        TLS12_SUITES
            .iter()
            .any(|suite| suite.name.contains("_CBC_") && suite.iana == code)
    })
}

/// The highest version in `supported_versions`, which is a list after one length octet.
fn highest_version(data: &[u8]) -> Option<u16> {
    let mut reader = Reader::new(data);
    let length = reader.u8()?;
    let mut versions = Reader::new(reader.take(usize::from(length))?);
    let mut highest = None;
    while !versions.is_empty() {
        let version = versions.u16()?;
        highest = Some(highest.map_or(version, |current: u16| current.max(version)));
    }
    highest
}

/// True when the `signature_algorithms` extension lists {SHA-1, RSA}. The list follows a length of two
/// octets, and each entry is a hash and a signature.
fn signals_sha1_rsa(data: &[u8]) -> Option<bool> {
    let mut reader = Reader::new(data);
    let length = reader.u16()?;
    let mut entries = Reader::new(reader.take(usize::from(length))?);
    let mut found = false;
    while !entries.is_empty() {
        let hash = entries.u8()?;
        let signature = entries.u8()?;
        found |= hash == HASH_SHA1 && signature == SIGNATURE_RSA;
    }
    Some(found)
}

/// True when the `psk_key_exchange_modes` extension lists `psk_ke` and nothing else. The list follows a
/// length of one octet, and each entry is one mode.
fn only_psk_ke(data: &[u8]) -> Option<bool> {
    let mut reader = Reader::new(data);
    let length = reader.u8()?;
    let modes = reader.take(usize::from(length))?;
    Some(!modes.is_empty() && modes.iter().all(|mode| *mode == PSK_KE))
}

/// A cursor over bytes that checks every read.
struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { rest: bytes }
    }

    fn is_empty(&self) -> bool {
        self.rest.is_empty()
    }

    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let head = self.rest.get(..count)?;
        self.rest = self.rest.get(count..)?;
        Some(head)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1)?.first().copied()
    }

    fn u16(&mut self) -> Option<u16> {
        let bytes = <[u8; 2]>::try_from(self.take(2)?).ok()?;
        Some(u16::from_be_bytes(bytes))
    }

    fn u24(&mut self) -> Option<usize> {
        let bytes = <[u8; 3]>::try_from(self.take(3)?).ok()?;
        let [high, middle, low] = bytes;
        usize::try_from(u32::from_be_bytes([0, high, middle, low])).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ClientHello with the legacy version, the cipher suites and the extensions given.
    fn hello(legacy: u16, suites: &[u16], extensions: &[(u16, Vec<u8>)]) -> Vec<u8> {
        let mut body = legacy.to_be_bytes().to_vec();
        body.extend([0u8; 32]);
        body.push(0);
        let suite_bytes: Vec<u8> = suites
            .iter()
            .flat_map(|suite| suite.to_be_bytes())
            .collect();
        body.extend(u16::try_from(suite_bytes.len()).unwrap().to_be_bytes());
        body.extend(suite_bytes);
        body.extend([1, 0]);
        if !extensions.is_empty() {
            let mut block = Vec::new();
            for (kind, data) in extensions {
                block.extend(kind.to_be_bytes());
                block.extend(u16::try_from(data.len()).unwrap().to_be_bytes());
                block.extend(data);
            }
            body.extend(u16::try_from(block.len()).unwrap().to_be_bytes());
            body.extend(block);
        }
        let length = body.len();
        let mut message = vec![CLIENT_HELLO];
        message.extend(&(length as u32).to_be_bytes()[1..]);
        message.extend(body);
        message
    }

    /// A `signature_algorithms` extension with the given (hash, signature) pairs.
    fn signatures(pairs: &[(u8, u8)]) -> Vec<u8> {
        let list: Vec<u8> = pairs.iter().flat_map(|(hash, sig)| [*hash, *sig]).collect();
        let mut data = u16::try_from(list.len()).unwrap().to_be_bytes().to_vec();
        data.extend(list);
        data
    }

    const GCM: u16 = 0xC02F;
    const CBC: u16 = 0x003C;

    #[test]
    fn a_tls_12_hello_with_its_extensions_raises_nothing() {
        let message = hello(
            0x0303,
            &[GCM],
            &[
                (EXTENSION_SIGNATURE_ALGORITHMS, signatures(&[(4, 1)])),
                (EXTENSION_ENCRYPT_THEN_MAC, Vec::new()),
            ],
        );
        let parsed = ClientHello::parse(&message).expect("parses");
        assert_eq!(parsed.highest_version, 0x0303);
        assert!(parsed.events().is_empty());
        assert!(!parsed.mac_then_encrypt());
    }

    #[test]
    fn the_legacy_version_decides_the_versions_without_supported_versions() {
        let tls_11 = ClientHello::parse(&hello(0x0302, &[GCM], &[])).expect("parses");
        assert_eq!(
            tls_11.events(),
            vec![
                SecurityEvent::DeprecatedVersion,
                SecurityEvent::NoSignatureAlgorithmExtension,
            ]
        );
        let ssl_3 = ClientHello::parse(&hello(0x0300, &[GCM], &[])).expect("parses");
        assert!(ssl_3.events().contains(&SecurityEvent::DisallowedVersion));
        assert!(!ssl_3.events().contains(&SecurityEvent::DeprecatedVersion));
    }

    #[test]
    fn supported_versions_gives_the_highest_version_offered() {
        let versions = vec![4, 0x03, 0x04, 0x03, 0x03];
        let message = hello(
            0x0301,
            &[GCM],
            &[
                (EXTENSION_SUPPORTED_VERSIONS, versions),
                (EXTENSION_SIGNATURE_ALGORITHMS, signatures(&[(4, 3)])),
            ],
        );
        let parsed = ClientHello::parse(&message).expect("parses");
        assert_eq!(parsed.highest_version, 0x0304);
        assert!(parsed.events().is_empty());
    }

    #[test]
    fn a_hello_without_signature_algorithms_raises_the_warning() {
        let message = hello(0x0303, &[GCM], &[]);
        let parsed = ClientHello::parse(&message).expect("parses");
        assert_eq!(
            parsed.events(),
            vec![SecurityEvent::NoSignatureAlgorithmExtension]
        );
    }

    #[test]
    fn sha_1_with_rsa_in_the_signature_algorithms_raises_the_warning() {
        let message = hello(
            0x0303,
            &[GCM],
            &[(
                EXTENSION_SIGNATURE_ALGORITHMS,
                signatures(&[(2, 1), (4, 3)]),
            )],
        );
        let parsed = ClientHello::parse(&message).expect("parses");
        assert_eq!(
            parsed.events(),
            vec![SecurityEvent::DeprecatedSignatureAlgorithm]
        );
    }

    #[test]
    fn a_cbc_suite_without_encrypt_then_mac_is_mac_then_encrypt() {
        let without = ClientHello::parse(&hello(0x0303, &[CBC], &[])).expect("parses");
        assert!(without.mac_then_encrypt());
        let with = ClientHello::parse(&hello(
            0x0303,
            &[CBC],
            &[(EXTENSION_ENCRYPT_THEN_MAC, Vec::new())],
        ))
        .expect("parses");
        assert!(!with.mac_then_encrypt());
        let gcm_only = ClientHello::parse(&hello(0x0303, &[GCM], &[])).expect("parses");
        assert!(!gcm_only.mac_then_encrypt());
    }

    #[test]
    fn psk_ke_only_and_early_data_are_read_from_their_extensions() {
        let psk_ke = hello(
            0x0303,
            &[GCM],
            &[(EXTENSION_PSK_KEY_EXCHANGE_MODES, vec![1, PSK_KE])],
        );
        let parsed = ClientHello::parse(&psk_ke).expect("parses");
        assert!(parsed.psk_ke_only);
        let dhe = hello(
            0x0303,
            &[GCM],
            &[(EXTENSION_PSK_KEY_EXCHANGE_MODES, vec![1, 1])],
        );
        assert!(!ClientHello::parse(&dhe).expect("parses").psk_ke_only);
        let early = hello(0x0303, &[GCM], &[(EXTENSION_EARLY_DATA, Vec::new())]);
        let parsed = ClientHello::parse(&early).expect("parses");
        assert_eq!(
            parsed.events(),
            vec![
                SecurityEvent::NoSignatureAlgorithmExtension,
                SecurityEvent::EarlyDataIgnored,
            ]
        );
    }

    #[test]
    fn a_message_that_does_not_parse_gives_nothing() {
        let message = hello(0x0303, &[GCM], &[]);
        assert_eq!(ClientHello::parse(&message[..message.len() - 1]), None);
        let mut other = message.clone();
        other[0] = 2;
        assert_eq!(ClientHello::parse(&other), None);
        assert_eq!(ClientHello::parse(&[]), None);
    }
}
