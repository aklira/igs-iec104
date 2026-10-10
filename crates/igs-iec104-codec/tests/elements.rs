// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Element sizes against the generated profile constants (task C2 done
//! criteria). The size of an information object is read from
//! `TypeId::object_size_bits` and its time tag from `TypeId::time_tag`; the
//! element sizes come from the `SIZE` constants of the element types. The
//! element table below transcribes which elements each in-profile type
//! carries (profile_104.yaml, `information_elements`), without the time tag.

#![allow(clippy::expect_used, clippy::arithmetic_side_effects)]

use igs_iec104_codec::elements::{
    Afq, Bcr, Bsi, Chs, Coi, Cp16Time2a, Dco, Diq, Frq, Lof, Los, Lsq, Nof, Nos, Nva, Oci, Qcc,
    Qdp, Qds, Qoi, Qos, Qpa, Qpm, Qrp, RangeTime, Rco, Scd, Sco, Scq, Sep, ShortFloat, Siq, Sof,
    Spe, Srq, Sva, Tsc, Vti,
};
use igs_iec104_codec::formats::{Cp24Time2a, Cp56Time2a};
use igs_iec104_codec::generated::profile::{TimeTag, TypeId};

/// Octets of the elements of `id`, without the time tag. `None` for a type
/// that is not in the profile.
fn elements_of(id: TypeId) -> Option<&'static [usize]> {
    match id {
        TypeId::M_SP_NA_1 => Some(&[Siq::SIZE]),
        TypeId::M_DP_NA_1 => Some(&[Diq::SIZE]),
        TypeId::M_ST_NA_1 => Some(&[Vti::SIZE, Qds::SIZE]),
        TypeId::M_BO_NA_1 => Some(&[Bsi::SIZE, Qds::SIZE]),
        TypeId::M_ME_NA_1 => Some(&[Nva::SIZE, Qds::SIZE]),
        TypeId::M_ME_NB_1 => Some(&[Sva::SIZE, Qds::SIZE]),
        TypeId::M_ME_NC_1 => Some(&[ShortFloat::SIZE, Qds::SIZE]),
        TypeId::M_IT_NA_1 => Some(&[Bcr::SIZE]),
        TypeId::M_PS_NA_1 => Some(&[Scd::SIZE, Qds::SIZE]),
        TypeId::M_ME_ND_1 => Some(&[Nva::SIZE]),
        TypeId::M_SP_TB_1 => Some(&[Siq::SIZE]),
        TypeId::M_DP_TB_1 => Some(&[Diq::SIZE]),
        TypeId::M_ST_TB_1 => Some(&[Vti::SIZE, Qds::SIZE]),
        TypeId::M_BO_TB_1 => Some(&[Bsi::SIZE, Qds::SIZE]),
        TypeId::M_ME_TD_1 => Some(&[Nva::SIZE, Qds::SIZE]),
        TypeId::M_ME_TE_1 => Some(&[Sva::SIZE, Qds::SIZE]),
        TypeId::M_ME_TF_1 => Some(&[ShortFloat::SIZE, Qds::SIZE]),
        TypeId::M_IT_TB_1 => Some(&[Bcr::SIZE]),
        TypeId::M_EP_TD_1 => Some(&[Sep::SIZE, Cp16Time2a::SIZE]),
        TypeId::M_EP_TE_1 => Some(&[Spe::SIZE, Qdp::SIZE, Cp16Time2a::SIZE]),
        TypeId::M_EP_TF_1 => Some(&[Oci::SIZE, Qdp::SIZE, Cp16Time2a::SIZE]),
        TypeId::C_SC_NA_1 => Some(&[Sco::SIZE]),
        TypeId::C_DC_NA_1 => Some(&[Dco::SIZE]),
        TypeId::C_RC_NA_1 => Some(&[Rco::SIZE]),
        TypeId::C_SE_NA_1 => Some(&[Nva::SIZE, Qos::SIZE]),
        TypeId::C_SE_NB_1 => Some(&[Sva::SIZE, Qos::SIZE]),
        TypeId::C_SE_NC_1 => Some(&[ShortFloat::SIZE, Qos::SIZE]),
        TypeId::C_BO_NA_1 => Some(&[Bsi::SIZE]),
        TypeId::C_SC_TA_1 => Some(&[Sco::SIZE]),
        TypeId::C_DC_TA_1 => Some(&[Dco::SIZE]),
        TypeId::C_RC_TA_1 => Some(&[Rco::SIZE]),
        TypeId::C_SE_TA_1 => Some(&[Nva::SIZE, Qos::SIZE]),
        TypeId::C_SE_TB_1 => Some(&[Sva::SIZE, Qos::SIZE]),
        TypeId::C_SE_TC_1 => Some(&[ShortFloat::SIZE, Qos::SIZE]),
        TypeId::C_BO_TA_1 => Some(&[Bsi::SIZE]),
        TypeId::M_EI_NA_1 => Some(&[Coi::SIZE]),
        TypeId::C_IC_NA_1 => Some(&[Qoi::SIZE]),
        TypeId::C_CI_NA_1 => Some(&[Qcc::SIZE]),
        TypeId::C_RD_NA_1 => Some(&[]),
        TypeId::C_CS_NA_1 => Some(&[]),
        TypeId::C_RP_NA_1 => Some(&[Qrp::SIZE]),
        TypeId::C_TS_TA_1 => Some(&[Tsc::SIZE]),
        TypeId::P_ME_NA_1 => Some(&[Nva::SIZE, Qpm::SIZE]),
        TypeId::P_ME_NB_1 => Some(&[Sva::SIZE, Qpm::SIZE]),
        TypeId::P_ME_NC_1 => Some(&[ShortFloat::SIZE, Qpm::SIZE]),
        TypeId::P_AC_NA_1 => Some(&[Qpa::SIZE]),
        TypeId::F_FR_NA_1 => Some(&[Nof::SIZE, Lof::SIZE, Frq::SIZE]),
        TypeId::F_SR_NA_1 => Some(&[Nof::SIZE, Nos::SIZE, Lof::SIZE, Srq::SIZE]),
        TypeId::F_SC_NA_1 => Some(&[Nof::SIZE, Nos::SIZE, Scq::SIZE]),
        TypeId::F_LS_NA_1 => Some(&[Nof::SIZE, Nos::SIZE, Lsq::SIZE, Chs::SIZE]),
        TypeId::F_AF_NA_1 => Some(&[Nof::SIZE, Nos::SIZE, Afq::SIZE]),
        TypeId::F_SG_NA_1 => Some(&[Nof::SIZE, Nos::SIZE, Los::SIZE]),
        TypeId::F_DR_TA_1 => Some(&[Nof::SIZE, Lof::SIZE, Sof::SIZE]),
        TypeId::F_SC_NB_1 => Some(&[Nof::SIZE, RangeTime::SIZE, RangeTime::SIZE]),
        _ => None,
    }
}

#[test]
fn every_in_profile_type_has_an_element_table() {
    for id in TypeId::ALL.iter().copied().filter(|id| id.in_profile()) {
        assert!(
            elements_of(id).is_some(),
            "{} has no element table",
            id.mnemonic()
        );
    }
}

#[test]
fn element_sizes_add_up_to_the_generated_object_sizes() {
    for id in TypeId::ALL.iter().copied().filter(|id| id.in_profile()) {
        let elements: usize = elements_of(id).expect("checked above").iter().sum();
        let time = match id.time_tag() {
            TimeTag::None => 0,
            TimeTag::Cp24Time2a => Cp24Time2a::SIZE,
            TimeTag::Cp56Time2a => Cp56Time2a::SIZE,
        };
        match id.object_size_bits() {
            // F_SG_NA_1: the segment has a length given at run time.
            None => assert_eq!(id.mnemonic(), "F_SG_NA_1"),
            Some(bits) => assert_eq!(
                usize::from(bits),
                8 * (elements + time),
                "object size of {}",
                id.mnemonic()
            ),
        }
    }
}

#[test]
fn range_times_of_querylog_are_seven_octets() {
    // F_SC_NB_1: name of file (2 octets) and two CP56Time2a range times.
    let elements = elements_of(TypeId::F_SC_NB_1).expect("in profile");
    assert_eq!(elements, &[Nof::SIZE, RangeTime::SIZE, RangeTime::SIZE]);
    assert_eq!(
        usize::from(TypeId::F_SC_NB_1.object_size_bits().expect("fixed size")),
        8 * (2 + 7 + 7)
    );
}
