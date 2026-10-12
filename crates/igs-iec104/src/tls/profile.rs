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
    // The values of the two DHE suites are the registry values of their names, not the values printed
    // in table 1 (0xC09E and 0x00A1 name other suites); see QUESTIONS.md, Q-019 and PROVENANCE.md, D-022.
    CipherSuite {
        iana: 0x009E,
        name: "TLS_DHE_RSA_WITH_AES_128_GCM_SHA256",
        version: Version::Tls12,
        support: Support::Mandatory,
        null_encryption: false,
    },
    CipherSuite {
        iana: 0x009F,
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

/// The cipher suites disallowed by clause 7.3, with their IANA values: the suites named by
/// [`is_disallowed`], read from the IANA TLS cipher suite registry (tls-parameters-4.csv, retrieved
/// 2026-10-12). Suites with NULL encryption and no MD5 or DES component are conditional (table 1), not here.
pub const DISALLOWED_SUITES: &[(u16, &str)] = &[
    (0x0000, "TLS_NULL_WITH_NULL_NULL"),
    (0x0001, "TLS_RSA_WITH_NULL_MD5"),
    (0x0003, "TLS_RSA_EXPORT_WITH_RC4_40_MD5"),
    (0x0004, "TLS_RSA_WITH_RC4_128_MD5"),
    (0x0006, "TLS_RSA_EXPORT_WITH_RC2_CBC_40_MD5"),
    (0x0008, "TLS_RSA_EXPORT_WITH_DES40_CBC_SHA"),
    (0x0009, "TLS_RSA_WITH_DES_CBC_SHA"),
    (0x000a, "TLS_RSA_WITH_3DES_EDE_CBC_SHA"),
    (0x000b, "TLS_DH_DSS_EXPORT_WITH_DES40_CBC_SHA"),
    (0x000c, "TLS_DH_DSS_WITH_DES_CBC_SHA"),
    (0x000d, "TLS_DH_DSS_WITH_3DES_EDE_CBC_SHA"),
    (0x000e, "TLS_DH_RSA_EXPORT_WITH_DES40_CBC_SHA"),
    (0x000f, "TLS_DH_RSA_WITH_DES_CBC_SHA"),
    (0x0010, "TLS_DH_RSA_WITH_3DES_EDE_CBC_SHA"),
    (0x0011, "TLS_DHE_DSS_EXPORT_WITH_DES40_CBC_SHA"),
    (0x0012, "TLS_DHE_DSS_WITH_DES_CBC_SHA"),
    (0x0013, "TLS_DHE_DSS_WITH_3DES_EDE_CBC_SHA"),
    (0x0014, "TLS_DHE_RSA_EXPORT_WITH_DES40_CBC_SHA"),
    (0x0015, "TLS_DHE_RSA_WITH_DES_CBC_SHA"),
    (0x0016, "TLS_DHE_RSA_WITH_3DES_EDE_CBC_SHA"),
    (0x0017, "TLS_DH_anon_EXPORT_WITH_RC4_40_MD5"),
    (0x0018, "TLS_DH_anon_WITH_RC4_128_MD5"),
    (0x0019, "TLS_DH_anon_EXPORT_WITH_DES40_CBC_SHA"),
    (0x001a, "TLS_DH_anon_WITH_DES_CBC_SHA"),
    (0x001b, "TLS_DH_anon_WITH_3DES_EDE_CBC_SHA"),
    (0x001e, "TLS_KRB5_WITH_DES_CBC_SHA"),
    (0x001f, "TLS_KRB5_WITH_3DES_EDE_CBC_SHA"),
    (0x0022, "TLS_KRB5_WITH_DES_CBC_MD5"),
    (0x0023, "TLS_KRB5_WITH_3DES_EDE_CBC_MD5"),
    (0x0024, "TLS_KRB5_WITH_RC4_128_MD5"),
    (0x0025, "TLS_KRB5_WITH_IDEA_CBC_MD5"),
    (0x0026, "TLS_KRB5_EXPORT_WITH_DES_CBC_40_SHA"),
    (0x0029, "TLS_KRB5_EXPORT_WITH_DES_CBC_40_MD5"),
    (0x002a, "TLS_KRB5_EXPORT_WITH_RC2_CBC_40_MD5"),
    (0x002b, "TLS_KRB5_EXPORT_WITH_RC4_40_MD5"),
    (0x008b, "TLS_PSK_WITH_3DES_EDE_CBC_SHA"),
    (0x008f, "TLS_DHE_PSK_WITH_3DES_EDE_CBC_SHA"),
    (0x0093, "TLS_RSA_PSK_WITH_3DES_EDE_CBC_SHA"),
    (0xc003, "TLS_ECDH_ECDSA_WITH_3DES_EDE_CBC_SHA"),
    (0xc008, "TLS_ECDHE_ECDSA_WITH_3DES_EDE_CBC_SHA"),
    (0xc00d, "TLS_ECDH_RSA_WITH_3DES_EDE_CBC_SHA"),
    (0xc012, "TLS_ECDHE_RSA_WITH_3DES_EDE_CBC_SHA"),
    (0xc017, "TLS_ECDH_anon_WITH_3DES_EDE_CBC_SHA"),
    (0xc01a, "TLS_SRP_SHA_WITH_3DES_EDE_CBC_SHA"),
    (0xc01b, "TLS_SRP_SHA_RSA_WITH_3DES_EDE_CBC_SHA"),
    (0xc01c, "TLS_SRP_SHA_DSS_WITH_3DES_EDE_CBC_SHA"),
    (0xc034, "TLS_ECDHE_PSK_WITH_3DES_EDE_CBC_SHA"),
];

/// True when the cipher suite with this name is disallowed outright (clause 7.3): the NULL suite, any
/// suite with MD5 as a component, and any suite with DES (DES40 and 3DES included) for encryption. The
/// clause says "not limited to" its list, so 3DES and the export DES suites are refused too (D-026, D-030).
pub fn is_disallowed(name: &str) -> bool {
    name == "TLS_NULL_WITH_NULL_NULL"
        || name
            .split('_')
            .any(|part| part == "MD5" || part == "3DES" || part.starts_with("DES"))
}

/// True when the cipher suite with this IANA value is disallowed (clause 7.3), see [`DISALLOWED_SUITES`].
pub fn is_disallowed_value(iana: u16) -> bool {
    DISALLOWED_SUITES.iter().any(|(value, _)| *value == iana)
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
    fn the_dhe_gcm_suites_have_the_values_of_their_names() {
        // RFC 5288 values, from the IANA registry (PROVENANCE.md, D-022). The printed values 0xC09E and
        // 0x00A1 name the CCM and DH_RSA suites.
        let value = |name: &str| {
            TLS12_SUITES
                .iter()
                .find(|suite| suite.name == name)
                .map(|suite| suite.iana)
        };
        assert_eq!(value("TLS_DHE_RSA_WITH_AES_128_GCM_SHA256"), Some(0x009E));
        assert_eq!(value("TLS_DHE_RSA_WITH_AES_256_GCM_SHA384"), Some(0x009F));
        assert!(
            !allows(0xC09E),
            "0xC09E is TLS_DHE_RSA_WITH_AES_128_CCM, not in the profile"
        );
        assert!(
            !allows(0x00A1),
            "0x00A1 is TLS_DH_RSA_WITH_AES_256_GCM_SHA384, not in the profile"
        );
    }

    #[test]
    fn the_disallowed_table_follows_the_names() {
        for (value, name) in DISALLOWED_SUITES {
            assert!(
                is_disallowed(name),
                "{name} ({value:#06x}) must be disallowed"
            );
        }
        assert!(is_disallowed_value(0x0000), "TLS_NULL_WITH_NULL_NULL");
        assert!(is_disallowed_value(0x000A), "TLS_RSA_WITH_3DES_EDE_CBC_SHA");
        assert!(
            !is_disallowed_value(0x0002),
            "NULL encryption without MD5 or DES is conditional"
        );
        assert!(
            !is_disallowed_value(0xC02F),
            "the GCM suite of the profile is allowed"
        );
        assert!(
            is_disallowed("TLS_RSA_EXPORT_WITH_DES40_CBC_SHA"),
            "DES40 encrypts with DES"
        );
        assert!(!is_disallowed("TLS_RSA_WITH_NULL_SHA"));
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
        assert!(
            is_disallowed("TLS_RSA_WITH_3DES_EDE_CBC_SHA"),
            "3DES encrypts with DES (D-026)"
        );
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
