// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The values of the TLS profile of IEC 62351-3: the cipher suites, groups and signature
//! algorithms that the standard marks in its conformance tables (clause 10). The IANA values are
//! data, used as the standard prints them.

/// The TLS version that an item of the profile belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Version {
    /// TLS 1.2 (clause 7).
    Tls12,
    /// TLS 1.3 (clause 8).
    Tls13,
}

/// How the profile treats an item (the notation of clause 10.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    /// Mandatory: the item shall be implemented (m).
    Mandatory,
    /// Optional: the item may be implemented (o).
    Optional,
    /// Conditional: implemented only under a condition, and disabled by default (c).
    Conditional,
}

/// A cipher suite of the profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CipherSuite {
    /// The IANA value, as the profile prints it (table 1 of clause 7.2, table 2 of clause 8.2).
    pub iana: u16,
    /// The IANA name.
    pub name: &'static str,
    /// The TLS version of the suite.
    pub version: Version,
    /// The support of the suite in the conformance tables.
    pub support: Support,
    /// True when the suite gives integrity only, with NULL encryption (clause 6.3).
    pub null_encryption: bool,
}

/// The cipher suites of TLS 1.2 (clause 7.2, table 9 of clause 10.5.1). Suites that the tables do
/// not list are not part of the profile.
pub const TLS12_SUITES: &[CipherSuite] = &[
    CipherSuite {
        iana: 0x003B,
        name: "TLS_RSA_WITH_NULL_SHA256",
        version: Version::Tls12,
        support: Support::Conditional,
        null_encryption: true,
    },
    CipherSuite {
        iana: 0x003C,
        name: "TLS_RSA_WITH_AES_128_CBC_SHA256",
        version: Version::Tls12,
        support: Support::Mandatory,
        null_encryption: false,
    },
    // The values of the two DHE suites are as printed in table 1. They do not seem to match the
    // names; see QUESTIONS.md, Q-019.
    CipherSuite {
        iana: 0xC09E,
        name: "TLS_DHE_RSA_WITH_AES_128_GCM_SHA256",
        version: Version::Tls12,
        support: Support::Mandatory,
        null_encryption: false,
    },
    CipherSuite {
        iana: 0x00A1,
        name: "TLS_DHE_RSA_WITH_AES_256_GCM_SHA384",
        version: Version::Tls12,
        support: Support::Optional,
        null_encryption: false,
    },
    CipherSuite {
        iana: 0xC02F,
        name: "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
        version: Version::Tls12,
        support: Support::Mandatory,
        null_encryption: false,
    },
    CipherSuite {
        iana: 0xC030,
        name: "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384",
        version: Version::Tls12,
        support: Support::Optional,
        null_encryption: false,
    },
    CipherSuite {
        iana: 0xC02B,
        name: "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256",
        version: Version::Tls12,
        support: Support::Mandatory,
        null_encryption: false,
    },
    CipherSuite {
        iana: 0xC02C,
        name: "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384",
        version: Version::Tls12,
        support: Support::Optional,
        null_encryption: false,
    },
];

/// The cipher suites of TLS 1.3 (clause 8.2, table 13 of clause 10.6.1).
pub const TLS13_SUITES: &[CipherSuite] = &[
    CipherSuite {
        iana: 0x1301,
        name: "TLS_AES_128_GCM_SHA256",
        version: Version::Tls13,
        support: Support::Mandatory,
        null_encryption: false,
    },
    CipherSuite {
        iana: 0x1302,
        name: "TLS_AES_256_GCM_SHA384",
        version: Version::Tls13,
        support: Support::Mandatory,
        null_encryption: false,
    },
    CipherSuite {
        iana: 0x1303,
        name: "TLS_CHACHA20_POLY1305_SHA256",
        version: Version::Tls13,
        support: Support::Optional,
        null_encryption: false,
    },
    CipherSuite {
        iana: 0x1304,
        name: "TLS_AES_128_CCM_SHA256",
        version: Version::Tls13,
        support: Support::Mandatory,
        null_encryption: false,
    },
    CipherSuite {
        iana: 0x1305,
        name: "TLS_AES_128_CCM_8_SHA256",
        version: Version::Tls13,
        support: Support::Optional,
        null_encryption: false,
    },
    CipherSuite {
        iana: 0xC0B4,
        name: "TLS_SHA256_SHA256",
        version: Version::Tls13,
        support: Support::Conditional,
        null_encryption: true,
    },
    CipherSuite {
        iana: 0xC0B5,
        name: "TLS_SHA384_SHA384",
        version: Version::Tls13,
        support: Support::Conditional,
        null_encryption: true,
    },
];

/// A named group for (EC)DHE key exchange (table 4 of clause 8.3.3, table 16 of clause 10.6.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Group {
    /// The IANA value of the named group.
    pub iana: u16,
    /// The name of the group.
    pub name: &'static str,
    /// The support of the group in the conformance tables.
    pub support: Support,
}

/// The groups of TLS 1.3 that the profile lists.
pub const GROUPS: &[Group] = &[
    Group {
        iana: 23,
        name: "secp256r1",
        support: Support::Mandatory,
    },
    Group {
        iana: 24,
        name: "secp384r1",
        support: Support::Optional,
    },
    Group {
        iana: 31,
        name: "brainpoolP256r1",
        support: Support::Optional,
    },
    Group {
        iana: 32,
        name: "brainpoolP384r1",
        support: Support::Optional,
    },
    Group {
        iana: 33,
        name: "brainpoolP512r1",
        support: Support::Optional,
    },
    Group {
        iana: 256,
        name: "ffdhe2048",
        support: Support::Mandatory,
    },
    Group {
        iana: 257,
        name: "ffdhe3072",
        support: Support::Optional,
    },
    Group {
        iana: 258,
        name: "ffdhe4096",
        support: Support::Optional,
    },
];

/// A signature algorithm that the profile requires (tables 5 and 6 of clause 8.3.4, tables 17
/// and 18 of clause 10.6.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignatureAlgorithm {
    /// The IANA value of the signature scheme.
    pub iana: u16,
    /// The name of the scheme.
    pub name: &'static str,
    /// True when the scheme is required for certificates (`signature_algorithms_cert`).
    pub for_certificates: bool,
}

/// The mandatory signature algorithms of TLS 1.3 (clause 8.3.4).
pub const MANDATORY_SIGNATURE_ALGORITHMS: &[SignatureAlgorithm] = &[
    SignatureAlgorithm {
        iana: 0x0804,
        name: "rsa_pss_rsae_sha256",
        for_certificates: true,
    },
    SignatureAlgorithm {
        iana: 0x0809,
        name: "rsa_pss_pss_sha256",
        for_certificates: true,
    },
    SignatureAlgorithm {
        iana: 0x0403,
        name: "ecdsa_secp256r1_sha256",
        for_certificates: true,
    },
    SignatureAlgorithm {
        iana: 0x0401,
        name: "rsa_pkcs1_sha256",
        for_certificates: true,
    },
];

/// True when the profile allows the cipher suite with this IANA value: it is mandatory or
/// optional in the tables, and it does not give NULL encryption (clause 6.3).
pub fn allows(iana: u16) -> bool {
    TLS12_SUITES
        .iter()
        .chain(TLS13_SUITES)
        .any(|suite| suite.iana == iana && suite.support != Support::Conditional)
}

/// True when the profile allows the named group with this IANA value.
pub fn allows_group(iana: u16) -> bool {
    GROUPS
        .iter()
        .any(|group| group.iana == iana && group.support != Support::Conditional)
}

/// True when the cipher suite with this IANA name is disallowed outright (clause 7.3): the NULL
/// suite, any suite with MD5 as a component, and any suite with DES for encryption.
/// 3DES (for example TLS_RSA_WITH_3DES_EDE_CBC_SHA) is not matched: QUESTIONS.md, Q-023 asks
/// whether clause 7.3 covers it.
pub fn is_disallowed(name: &str) -> bool {
    name == "TLS_NULL_WITH_NULL_NULL" || name.split('_').any(|part| part == "MD5" || part == "DES")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mandatory_suites_of_both_versions_are_allowed() {
        for suite in TLS12_SUITES.iter().chain(TLS13_SUITES) {
            if suite.support == Support::Mandatory {
                assert!(allows(suite.iana), "{} must be allowed", suite.name);
            }
        }
    }

    #[test]
    fn null_encryption_and_unlisted_suites_are_not_allowed() {
        assert!(!allows(0x003B), "TLS_RSA_WITH_NULL_SHA256 is conditional");
        assert!(!allows(0xC0B4), "TLS_SHA256_SHA256 is conditional");
        assert!(
            !allows(0x0000),
            "TLS_NULL_WITH_NULL_NULL is not in the tables"
        );
    }

    #[test]
    fn the_disallowed_names_of_clause_7_3_are_recognised() {
        assert!(is_disallowed("TLS_NULL_WITH_NULL_NULL"));
        assert!(is_disallowed("TLS_RSA_WITH_NULL_MD5"));
        assert!(is_disallowed("TLS_RSA_WITH_DES_CBC_SHA"));
        assert!(!is_disallowed("TLS_RSA_WITH_AES_128_CBC_SHA256"));
        assert!(!is_disallowed("TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256"));
    }

    #[test]
    fn the_profile_lists_the_groups_of_table_4() {
        assert_eq!(GROUPS.len(), 8);
        assert!(allows_group(23) && allows_group(24));
        assert!(!allows_group(0), "value 0 is not a group of table 4");
    }

    #[test]
    fn the_profile_has_four_signature_algorithms() {
        assert_eq!(MANDATORY_SIGNATURE_ALGORITHMS.len(), 4);
    }
}
