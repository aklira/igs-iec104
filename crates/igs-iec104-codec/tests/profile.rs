// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Checks over the generated profile constants (task F3 done criteria).

use igs_iec104_codec::generated::profile::TypeId;

#[test]
fn catalogue_size() {
    assert_eq!(TypeId::COUNT, 67, "catalogue holds the whole 104 catalogue");
    assert_eq!(TypeId::ALL.len(), TypeId::COUNT);
}

#[test]
fn profile_membership() {
    let in_profile = TypeId::ALL.iter().filter(|t| t.in_profile()).count();
    assert_eq!(
        in_profile, 54,
        "the application profile selects 54 of the 67"
    );
}

#[test]
fn code_round_trips() {
    for ty in TypeId::ALL {
        let code = ty.as_u8();
        assert_eq!(
            TypeId::from_u8(code),
            Some(ty),
            "code {code} must map back to {ty:?}"
        );
    }
    assert_eq!(TypeId::from_u8(0), None);
    assert_eq!(TypeId::from_u8(255), Option::<TypeId>::None);
}

#[test]
fn clock_sync_cause_three_rejected() {
    // PROVENANCE.md D-004: spontaneous (cause 3) is not allowed for the clock
    // synchronisation command in the 104 profile.
    let clock_sync = match TypeId::from_u8(103) {
        Some(ty) => ty,
        None => panic!("103 must belong to the catalogue"),
    };
    assert!(
        !clock_sync.cot_allowed().contains(&3),
        "cause 3 must not be permitted for the clock sync command"
    );
    assert!(
        clock_sync.cot_not_permitted().contains(&3),
        "cause 3 must be recorded as not permitted"
    );
}
