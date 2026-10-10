// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The answers of a controlled station to the requests of a controlling station (IEC
//! 60870-5-104 §7.2, §7.5 to §7.10), judged one ASDU at a time, without I/O.
//!
//! An answer has the type identification of its request, the common address and the
//! information object address of its request, and the cause that confirms the request:
//! the activation confirmation (7) for a select or a command, the activation termination
//! (10) for an execution or an interrogation, the request (5) for a read. A negative
//! confirmation (the P/N bit set, 101 7.2.3) refuses the request, and so does a cause of
//! 44 to 47, which says why the station could not place the request (§8.9).
//!
//! [`judge`] tells whether an ASDU is the answer to a [`Request`]. A caller that waits for
//! an answer judges each ASDU it receives until one of them is the answer, as the client
//! binary does. The pattern of a client that serves other tasks is in the documentation of
//! [`crate::client::Client`].

use igs_iec104_codec::asdu::Asdu;
use igs_iec104_codec::generated::profile::TypeId;
use igs_iec104_codec::header::{CommonAddress, InformationObjectAddress};

/// A request whose answer is awaited: what the answer must carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request<'a> {
    /// The type identifications that answer the request. `None` accepts any type: a read
    /// is answered with the type of the point it reads.
    pub types: Option<&'a [TypeId]>,
    /// The common address the request was sent to.
    pub common_address: CommonAddress,
    /// The information object address of the request: the address of the command or of
    /// the point read, and address 0 for the procedures that have none.
    pub address: InformationObjectAddress,
    /// The cause that confirms the request: the activation confirmation (7), the activation
    /// termination (10), or the request (5) for a read.
    pub cause: u8,
}

/// What the station answered to a request, as judged by [`judge`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// The request is confirmed: the expected cause, with the P/N bit clear.
    Confirmed,
    /// The request is refused, by a negative confirmation or by a cause of 44 to 47 (§8.9).
    Refused {
        /// The cause of transmission of the answer.
        cause: u8,
        /// True for a negative confirmation (the P/N bit is set); false for a cause of 44 to 47.
        negative: bool,
    },
}

/// Judges `asdu` as the answer to `request`. Returns `None` when the ASDU is not an answer
/// to it: another type identification, another common address or information object
/// address, or another cause.
///
/// A negative confirmation refuses the request whatever its cause. So does a positive
/// answer of a cause of 44 to 47, which the station sends when it cannot place the request.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use igs_iec104::answer::{judge, Answer, Request};
/// use igs_iec104_codec::asdu::{Asdu, Body, InformationObject, Objects};
/// use igs_iec104_codec::elements::Qoi;
/// use igs_iec104_codec::generated::profile::TypeId;
/// use igs_iec104_codec::header::{cause, CauseOfTransmission, CommonAddress, InformationObjectAddress};
///
/// // The station of common address 1 confirms a general interrogation.
/// let address = InformationObjectAddress::new(0).ok_or("the address is out of range")?;
/// let confirmation = Asdu {
///     cot: CauseOfTransmission::new(cause::ACTIVATION_CONFIRMATION)
///         .ok_or("the cause is out of range")?,
///     common_address: CommonAddress::new(1),
///     body: Body::C_IC_NA_1(Objects::Individual(vec![InformationObject {
///         address,
///         value: Qoi::STATION,
///     }])),
/// };
/// let request = Request {
///     types: Some(&[TypeId::C_IC_NA_1]),
///     common_address: CommonAddress::new(1),
///     address,
///     cause: cause::ACTIVATION_CONFIRMATION,
/// };
/// assert_eq!(judge(&confirmation, &request), Some(Answer::Confirmed));
/// # Ok(())
/// # }
/// ```
pub fn judge(asdu: &Asdu, request: &Request<'_>) -> Option<Answer> {
    if asdu.common_address != request.common_address
        || asdu.body.first_address() != Some(request.address)
        || !request
            .types
            .is_none_or(|types| types.contains(&asdu.type_id()))
    {
        return None;
    }
    let cause = asdu.cot.cause();
    if asdu.cot.negative {
        Some(Answer::Refused {
            cause,
            negative: true,
        })
    } else if cause == request.cause {
        Some(Answer::Confirmed)
    } else if (44..=47).contains(&cause) {
        Some(Answer::Refused {
            cause,
            negative: false,
        })
    } else {
        None
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use igs_iec104_codec::asdu::{Body, InformationObject, Objects};
    use igs_iec104_codec::elements::{Qoc, Qoi, Sco};
    use igs_iec104_codec::header::{cause, CauseOfTransmission};

    const STATION: CommonAddress = CommonAddress::new(45);

    fn at(value: u32) -> InformationObjectAddress {
        InformationObjectAddress::new(value).expect("an address")
    }

    /// An interrogation answer of the station, at address 0, with the given cause and P/N bit.
    fn interrogation(cause: u8, negative: bool) -> Asdu {
        let mut cot = CauseOfTransmission::new(cause).expect("a cause");
        cot.negative = negative;
        Asdu {
            cot,
            common_address: STATION,
            body: Body::C_IC_NA_1(Objects::Individual(vec![InformationObject {
                address: at(0),
                value: Qoi::STATION,
            }])),
        }
    }

    /// A request for an interrogation of the station, confirmed by an activation confirmation.
    fn interrogation_request() -> Request<'static> {
        Request {
            types: Some(&[TypeId::C_IC_NA_1]),
            common_address: STATION,
            address: at(0),
            cause: cause::ACTIVATION_CONFIRMATION,
        }
    }

    #[test]
    fn a_positive_confirmation_of_the_expected_cause_is_confirmed() {
        let answer = interrogation(cause::ACTIVATION_CONFIRMATION, false);
        assert_eq!(
            judge(&answer, &interrogation_request()),
            Some(Answer::Confirmed)
        );
    }

    #[test]
    fn a_negative_confirmation_refuses_with_its_cause() {
        let answer = interrogation(cause::ACTIVATION_CONFIRMATION, true);
        assert_eq!(
            judge(&answer, &interrogation_request()),
            Some(Answer::Refused {
                cause: cause::ACTIVATION_CONFIRMATION,
                negative: true,
            })
        );
    }

    #[test]
    fn a_negative_confirmation_refuses_whatever_its_cause() {
        let answer = interrogation(cause::ACTIVATION_TERMINATION, true);
        assert_eq!(
            judge(&answer, &interrogation_request()),
            Some(Answer::Refused {
                cause: cause::ACTIVATION_TERMINATION,
                negative: true,
            })
        );
    }

    #[test]
    fn a_cause_of_44_to_47_refuses_the_request() {
        let answer = interrogation(cause::UNKNOWN_INFO_OBJECT_ADDRESS, false);
        assert_eq!(
            judge(&answer, &interrogation_request()),
            Some(Answer::Refused {
                cause: cause::UNKNOWN_INFO_OBJECT_ADDRESS,
                negative: false,
            })
        );
    }

    #[test]
    fn another_cause_is_not_an_answer() {
        let termination = interrogation(cause::ACTIVATION_TERMINATION, false);
        assert_eq!(judge(&termination, &interrogation_request()), None);
        let spontaneous = interrogation(cause::SPONTANEOUS, false);
        assert_eq!(judge(&spontaneous, &interrogation_request()), None);
    }

    #[test]
    fn another_type_is_not_an_answer() {
        let command = Asdu {
            cot: CauseOfTransmission::new(cause::ACTIVATION_CONFIRMATION).expect("a cause"),
            common_address: STATION,
            body: Body::C_SC_NA_1(Objects::Individual(vec![InformationObject {
                address: at(0),
                value: Sco {
                    on: true,
                    qoc: Qoc::new(0, true).expect("a qualifier"),
                },
            }])),
        };
        assert_eq!(judge(&command, &interrogation_request()), None);
    }

    #[test]
    fn any_type_answers_a_request_that_accepts_any() {
        let request = Request {
            types: None,
            cause: cause::REQUEST,
            ..interrogation_request()
        };
        let answer = interrogation(cause::REQUEST, false);
        assert_eq!(judge(&answer, &request), Some(Answer::Confirmed));
    }

    #[test]
    fn another_common_address_is_not_an_answer() {
        let mut answer = interrogation(cause::ACTIVATION_CONFIRMATION, false);
        answer.common_address = CommonAddress::new(46);
        assert_eq!(judge(&answer, &interrogation_request()), None);
    }

    #[test]
    fn another_address_is_not_an_answer() {
        let request = Request {
            address: at(5),
            ..interrogation_request()
        };
        let answer = interrogation(cause::ACTIVATION_CONFIRMATION, false);
        assert_eq!(judge(&answer, &request), None);
    }

    #[test]
    fn an_answer_without_objects_is_not_an_answer() {
        let mut answer = interrogation(cause::ACTIVATION_CONFIRMATION, false);
        answer.body = Body::C_IC_NA_1(Objects::Individual(Vec::new()));
        assert_eq!(judge(&answer, &interrogation_request()), None);
    }
}
