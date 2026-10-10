// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The body of an ASDU: one variant per type identification of the 104
//! profile, each holding the information objects of that type.
//!
//! The variant list is the profile itself (`profile_104.yaml`): every
//! in-profile type ID, with the value its elements form. The value is a tuple
//! of elements in the order of the profile, wrapped in `Timed` when the type
//! carries a CP56Time2a time tag.

use super::objects::{self, Objects};
use super::wire::{FileSegment, Timed};
use crate::elements::{
    Afq, Bcr, Bsi, Chs, Coi, Dco, Diq, Frq, Lof, Lsq, Nof, Nos, Nva, Oci, Qcc, Qdp, Qds, Qoi, Qos,
    Qpa, Qpm, Qrp, RangeTime, Rco, Scd, Sco, Scq, Sep, ShortFloat, Siq, Sof, Spe, Srq, Sva, Tsc,
    Vti,
};
use crate::error::{DecodeError, EncodeError};
use crate::formats::{Cp16Time2a, Cp56Time2a};
use crate::generated::profile::TypeId;

/// Declares [`Body`] from the list of in-profile type IDs and their values.
macro_rules! asdu_bodies {
    ($($type_id:ident => $value:ty),* $(,)?) => {
        /// The information objects of an ASDU, by type identification.
        #[allow(non_camel_case_types)]
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub enum Body {
            $(
                #[doc = concat!("The objects of ", stringify!($type_id), ".")]
                $type_id(Objects<$value>),
            )*
        }

        impl Body {
            /// The type identification of the objects.
            pub fn type_id(&self) -> TypeId {
                match self {
                    $( Self::$type_id(_) => TypeId::$type_id, )*
                }
            }

            /// Number of objects (or of value sets) carried.
            pub fn len(&self) -> usize {
                match self {
                    $( Self::$type_id(objects) => objects.len(), )*
                }
            }

            /// True when the body carries no object.
            pub fn is_empty(&self) -> bool {
                self.len() == 0
            }

            /// True when the objects use the SQ = 1 form.
            pub fn is_sequence(&self) -> bool {
                match self {
                    $( Self::$type_id(objects) => objects.is_sequence(), )*
                }
            }

            /// Writes the body into the front of `dst`. Returns the octets written.
            pub(crate) fn encode(&self, dst: &mut [u8]) -> Result<usize, EncodeError> {
                match self {
                    $( Self::$type_id(objects) => objects.encode(dst), )*
                }
            }

            /// Decodes the body of an ASDU of type `type_id`. The caller checks
            /// that the type is in the profile.
            pub(crate) fn decode(
                type_id: TypeId,
                body: &[u8],
                number: u8,
                sequence: bool,
            ) -> Result<Self, DecodeError> {
                match type_id {
                    $(
                        TypeId::$type_id => Ok(Self::$type_id(objects::decode::<$value>(
                            body, number, sequence,
                        )?)),
                    )*
                    _ => Err(DecodeError::UnsupportedTypeId {
                        type_code: type_id.as_u8(),
                        raw: Vec::new(),
                    }),
                }
            }

            /// Strategy of the bodies of one type, for the property tests. The
            /// sequence form is only generated when `sequence` is true.
            #[cfg(test)]
            pub(crate) fn arbitrary(
                type_id: TypeId,
                sequence: bool,
            ) -> proptest::strategy::BoxedStrategy<Self> {
                use proptest::strategy::Strategy;
                match type_id {
                    $(
                        TypeId::$type_id => crate::proptests::objects::<$value>(sequence)
                            .prop_map(Self::$type_id)
                            .boxed(),
                    )*
                    _ => unreachable!("{type_id:?} is not in the profile"),
                }
            }
        }
    };
}

asdu_bodies! {
    M_SP_NA_1 => Siq,
    M_DP_NA_1 => Diq,
    M_ST_NA_1 => (Vti, Qds),
    M_BO_NA_1 => (Bsi, Qds),
    M_ME_NA_1 => (Nva, Qds),
    M_ME_NB_1 => (Sva, Qds),
    M_ME_NC_1 => (ShortFloat, Qds),
    M_IT_NA_1 => Bcr,
    M_PS_NA_1 => (Scd, Qds),
    M_ME_ND_1 => Nva,
    M_SP_TB_1 => Timed<Siq>,
    M_DP_TB_1 => Timed<Diq>,
    M_ST_TB_1 => Timed<(Vti, Qds)>,
    M_BO_TB_1 => Timed<(Bsi, Qds)>,
    M_ME_TD_1 => Timed<(Nva, Qds)>,
    M_ME_TE_1 => Timed<(Sva, Qds)>,
    M_ME_TF_1 => Timed<(ShortFloat, Qds)>,
    M_IT_TB_1 => Timed<Bcr>,
    M_EP_TD_1 => Timed<(Sep, Cp16Time2a)>,
    M_EP_TE_1 => Timed<(Spe, Qdp, Cp16Time2a)>,
    M_EP_TF_1 => Timed<(Oci, Qdp, Cp16Time2a)>,
    C_SC_NA_1 => Sco,
    C_DC_NA_1 => Dco,
    C_RC_NA_1 => Rco,
    C_SE_NA_1 => (Nva, Qos),
    C_SE_NB_1 => (Sva, Qos),
    C_SE_NC_1 => (ShortFloat, Qos),
    C_BO_NA_1 => Bsi,
    C_SC_TA_1 => Timed<Sco>,
    C_DC_TA_1 => Timed<Dco>,
    C_RC_TA_1 => Timed<Rco>,
    C_SE_TA_1 => Timed<(Nva, Qos)>,
    C_SE_TB_1 => Timed<(Sva, Qos)>,
    C_SE_TC_1 => Timed<(ShortFloat, Qos)>,
    C_BO_TA_1 => Timed<Bsi>,
    M_EI_NA_1 => Coi,
    C_IC_NA_1 => Qoi,
    C_CI_NA_1 => Qcc,
    C_RD_NA_1 => (),
    C_CS_NA_1 => Cp56Time2a,
    C_RP_NA_1 => Qrp,
    C_TS_TA_1 => Timed<Tsc>,
    P_ME_NA_1 => (Nva, Qpm),
    P_ME_NB_1 => (Sva, Qpm),
    P_ME_NC_1 => (ShortFloat, Qpm),
    P_AC_NA_1 => Qpa,
    F_FR_NA_1 => (Nof, Lof, Frq),
    F_SR_NA_1 => (Nof, Nos, Lof, Srq),
    F_SC_NA_1 => (Nof, Nos, Scq),
    F_LS_NA_1 => (Nof, Nos, Lsq, Chs),
    F_AF_NA_1 => (Nof, Nos, Afq),
    F_SG_NA_1 => FileSegment,
    F_DR_TA_1 => Timed<(Nof, Lof, Sof)>,
    F_SC_NB_1 => (Nof, RangeTime, RangeTime),
}
