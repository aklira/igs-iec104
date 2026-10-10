// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The ASDUs of the controlling station's procedures (IEC 60870-5-104 §7.5 to §7.10).
//!
//! Each builder returns the ASDU of one request. The causes are those of the
//! service tables of §7: activation (6) for every request, except a read, which
//! is a request (5). A clock synchronization is never spontaneous (cause 3):
//! it is an activation (decision D-004).

use std::fmt;

use igs_iec104_codec::asdu::{Asdu, Body, InformationObject, Objects, Timed};
use igs_iec104_codec::elements::{Dco, Nva, Qcc, Qoi, Qos, Rco, Sco, ShortFloat, Sva, Tsc};
use igs_iec104_codec::formats::Cp56Time2a;
use igs_iec104_codec::header::{
    cause, CauseOfTransmission, CommonAddress, InformationObjectAddress,
};

/// A request that cannot be built. The constants of the procedures are valid, so
/// this only reports a bug in a caller's constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcedureError {
    /// The cause of transmission is out of range.
    Cause(u8),
    /// The information object address is out of range.
    Address,
}

impl fmt::Display for ProcedureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cause(code) => write!(f, "cause of transmission {code} is out of range"),
            Self::Address => write!(f, "information object address is out of range"),
        }
    }
}

impl std::error::Error for ProcedureError {}

fn zero() -> Result<InformationObjectAddress, ProcedureError> {
    InformationObjectAddress::new(0).ok_or(ProcedureError::Address)
}

fn one<T>(address: InformationObjectAddress, value: T) -> Objects<T> {
    Objects::Individual(vec![InformationObject { address, value }])
}

fn asdu(code: u8, common: CommonAddress, body: Body) -> Result<Asdu, ProcedureError> {
    let cot = CauseOfTransmission::new(code).ok_or(ProcedureError::Cause(code))?;
    Ok(Asdu {
        cot,
        common_address: common,
        body,
    })
}

/// General interrogation (§7.5): C_IC_NA_1, activation, of `group` (QOI 20 for the
/// whole station).
pub fn interrogation(common: CommonAddress, group: Qoi) -> Result<Asdu, ProcedureError> {
    asdu(
        cause::ACTIVATION,
        common,
        Body::C_IC_NA_1(one(zero()?, group)),
    )
}

/// Counter interrogation (§7.8): C_CI_NA_1, activation.
pub fn counter_interrogation(common: CommonAddress, request: Qcc) -> Result<Asdu, ProcedureError> {
    asdu(
        cause::ACTIVATION,
        common,
        Body::C_CI_NA_1(one(zero()?, request)),
    )
}

/// Read (§7.2): C_RD_NA_1, request, of the object at `address`.
pub fn read(
    common: CommonAddress,
    address: InformationObjectAddress,
) -> Result<Asdu, ProcedureError> {
    asdu(cause::REQUEST, common, Body::C_RD_NA_1(one(address, ())))
}

/// Clock synchronization (§7.6): C_CS_NA_1, activation, with the time of the
/// controlling station. Never spontaneous (D-004).
pub fn clock_sync(common: CommonAddress, time: Cp56Time2a) -> Result<Asdu, ProcedureError> {
    asdu(
        cause::ACTIVATION,
        common,
        Body::C_CS_NA_1(one(zero()?, time)),
    )
}

/// Test command (§7.10, §8.8): C_TS_TA_1, activation. The station answers with the
/// same pattern and the same time tag.
pub fn test_command(
    common: CommonAddress,
    pattern: u16,
    time: Cp56Time2a,
) -> Result<Asdu, ProcedureError> {
    let value = Timed {
        value: Tsc::new(pattern),
        time,
    };
    asdu(
        cause::ACTIVATION,
        common,
        Body::C_TS_TA_1(one(zero()?, value)),
    )
}

/// Single command (§7.7): C_SC_NA_1, or C_SC_TA_1 with a time tag.
pub fn single_command(
    common: CommonAddress,
    address: InformationObjectAddress,
    command: Sco,
    time: Option<Cp56Time2a>,
) -> Result<Asdu, ProcedureError> {
    let body = match time {
        Some(time) => Body::C_SC_TA_1(one(
            address,
            Timed {
                value: command,
                time,
            },
        )),
        None => Body::C_SC_NA_1(one(address, command)),
    };
    asdu(cause::ACTIVATION, common, body)
}

/// Double command (§7.7): C_DC_NA_1, or C_DC_TA_1 with a time tag.
pub fn double_command(
    common: CommonAddress,
    address: InformationObjectAddress,
    command: Dco,
    time: Option<Cp56Time2a>,
) -> Result<Asdu, ProcedureError> {
    let body = match time {
        Some(time) => Body::C_DC_TA_1(one(
            address,
            Timed {
                value: command,
                time,
            },
        )),
        None => Body::C_DC_NA_1(one(address, command)),
    };
    asdu(cause::ACTIVATION, common, body)
}

/// Regulating step command (§7.7): C_RC_NA_1, or C_RC_TA_1 with a time tag.
pub fn regulating_step(
    common: CommonAddress,
    address: InformationObjectAddress,
    command: Rco,
    time: Option<Cp56Time2a>,
) -> Result<Asdu, ProcedureError> {
    let body = match time {
        Some(time) => Body::C_RC_TA_1(one(
            address,
            Timed {
                value: command,
                time,
            },
        )),
        None => Body::C_RC_NA_1(one(address, command)),
    };
    asdu(cause::ACTIVATION, common, body)
}

/// Set-point command, normalized value (§7.7): C_SE_NA_1, or C_SE_TA_1 with a time tag.
pub fn set_point_normalized(
    common: CommonAddress,
    address: InformationObjectAddress,
    value: Nva,
    qualifier: Qos,
    time: Option<Cp56Time2a>,
) -> Result<Asdu, ProcedureError> {
    let body = match time {
        Some(time) => Body::C_SE_TA_1(one(
            address,
            Timed {
                value: (value, qualifier),
                time,
            },
        )),
        None => Body::C_SE_NA_1(one(address, (value, qualifier))),
    };
    asdu(cause::ACTIVATION, common, body)
}

/// Set-point command, scaled value (§7.7): C_SE_NB_1, or C_SE_TB_1 with a time tag.
pub fn set_point_scaled(
    common: CommonAddress,
    address: InformationObjectAddress,
    value: Sva,
    qualifier: Qos,
    time: Option<Cp56Time2a>,
) -> Result<Asdu, ProcedureError> {
    let body = match time {
        Some(time) => Body::C_SE_TB_1(one(
            address,
            Timed {
                value: (value, qualifier),
                time,
            },
        )),
        None => Body::C_SE_NB_1(one(address, (value, qualifier))),
    };
    asdu(cause::ACTIVATION, common, body)
}

/// Set-point command, short floating point (§7.7): C_SE_NC_1, or C_SE_TC_1 with a time tag.
pub fn set_point_float(
    common: CommonAddress,
    address: InformationObjectAddress,
    value: ShortFloat,
    qualifier: Qos,
    time: Option<Cp56Time2a>,
) -> Result<Asdu, ProcedureError> {
    let body = match time {
        Some(time) => Body::C_SE_TC_1(one(
            address,
            Timed {
                value: (value, qualifier),
                time,
            },
        )),
        None => Body::C_SE_NC_1(one(address, (value, qualifier))),
    };
    asdu(cause::ACTIVATION, common, body)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use igs_iec104_codec::elements::{DoubleCommandState, Qoc, RegulatingStep};

    fn common() -> CommonAddress {
        CommonAddress::new(1)
    }

    fn address(value: u32) -> InformationObjectAddress {
        InformationObjectAddress::new(value).expect("in range")
    }

    fn time() -> Cp56Time2a {
        Cp56Time2a::new(1000, 30, 12, 1, 1, 1, 26).expect("in range")
    }

    #[test]
    fn clock_sync_is_an_activation_and_never_spontaneous() {
        let asdu = clock_sync(common(), time()).expect("builds");
        assert_eq!(asdu.cot.cause(), cause::ACTIVATION);
        assert!(matches!(asdu.body, Body::C_CS_NA_1(_)));
        assert_ne!(asdu.cot.cause(), cause::SPONTANEOUS);
    }

    #[test]
    fn read_is_a_request_of_one_object() {
        let asdu = read(common(), address(672)).expect("builds");
        assert_eq!(asdu.cot.cause(), cause::REQUEST);
        assert!(matches!(asdu.body, Body::C_RD_NA_1(_)));
    }

    #[test]
    fn interrogation_is_an_activation_at_the_zero_address() {
        let asdu = interrogation(common(), Qoi::STATION).expect("builds");
        assert_eq!(asdu.cot.cause(), cause::ACTIVATION);
        match asdu.body {
            Body::C_IC_NA_1(Objects::Individual(objects)) => {
                assert_eq!(objects.len(), 1);
            }
            other => panic!("unexpected body {other:?}"),
        }
    }

    #[test]
    fn a_command_with_a_time_tag_uses_the_time_tagged_type() {
        let sco = Sco {
            on: true,
            qoc: Qoc::new(0, true).expect("in range"),
        };
        let plain = single_command(common(), address(1), sco, None).expect("builds");
        let tagged = single_command(common(), address(1), sco, Some(time())).expect("builds");
        assert!(matches!(plain.body, Body::C_SC_NA_1(_)));
        assert!(matches!(tagged.body, Body::C_SC_TA_1(_)));
    }

    #[test]
    fn every_command_is_an_activation() {
        let dco = Dco {
            state: DoubleCommandState::On,
            qoc: Qoc::new(0, false).expect("in range"),
        };
        let rco = Rco {
            step: RegulatingStep::Higher,
            qoc: Qoc::new(0, false).expect("in range"),
        };
        let qos = Qos::new(0, false).expect("in range");
        let built = [
            double_command(common(), address(1), dco, None),
            regulating_step(common(), address(1), rco, Some(time())),
            set_point_float(common(), address(1), ShortFloat::from_f32(1.5), qos, None),
        ];
        for asdu in built {
            assert_eq!(asdu.expect("builds").cot.cause(), cause::ACTIVATION);
        }
    }
}
