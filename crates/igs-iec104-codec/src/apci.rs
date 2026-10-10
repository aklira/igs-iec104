// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! APCI and framing (IEC 60870-5-104 §5, figures 4 to 8).
//!
//! A frame is the start octet 68H, the length octet, the four control octets
//! and, for the I format only, an ASDU. The length counts the control field
//! and the ASDU, so it is 4 to 253. [`FrameDecoder`](crate::apci::FrameDecoder) cuts a byte
//! stream into frames; [`Apdu`](crate::apci::Apdu) reads one frame.
//!
//! Bit positions of the control field come from the figures 6 to 8 of §5.

use crate::asdu::Asdu;
use crate::error::{DecodeError, EncodeError};
use crate::generated::profile::MAX_APDU_LENGTH;

/// Start octet of every frame (104 §5).
pub const START: u8 = 0x68;

/// Octets of the control field (104 §5).
pub(crate) const CONTROL_FIELD_OCTETS: usize = 4;

/// Octets before the control field: the start octet and the length octet.
const HEADER_OCTETS: usize = 2;

/// Largest frame: the header and the largest APDU body.
pub const MAX_FRAME_OCTETS: usize = HEADER_OCTETS + MAX_APDU_LENGTH;

/// Largest sequence number: N(S) and N(R) are fifteen bits (figure 6).
pub const MAX_SEQUENCE_NUMBER: u16 = 0x7FFF;

/// A send or receive sequence number, from 0 to 32767.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SequenceNumber(u16);

impl SequenceNumber {
    /// The first number of a connection.
    pub const ZERO: Self = Self(0);

    /// Fails when `value` is above 32767 (fifteen bits).
    pub const fn new(value: u16) -> Option<Self> {
        if value > MAX_SEQUENCE_NUMBER {
            return None;
        }
        Some(Self(value))
    }

    /// The number, 0..=32767.
    pub const fn value(self) -> u16 {
        self.0
    }

    /// The next number, wrapping from 32767 to 0.
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1) & MAX_SEQUENCE_NUMBER)
    }

    /// Reads the number from the two octets of the control field that carry
    /// it: the low seven bits sit in bits 2 to 8 of the first octet.
    fn from_control(low: u8, high: u8) -> Self {
        Self((u16::from(low) >> 1) | (u16::from(high) << 7))
    }

    /// The first of the two control octets that carry the number.
    const fn low_octet(self) -> u8 {
        ((self.0 & 0x7F) << 1) as u8
    }

    /// The second of the two control octets that carry the number.
    const fn high_octet(self) -> u8 {
        (self.0 >> 7) as u8
    }
}

/// The unnumbered control functions of the U format (figure 8). Only one of
/// them is in a frame.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum UnnumberedFunction {
    /// STARTDT act: start the data transfer (bit 3).
    StartDtAct,
    /// STARTDT con: confirmation of STARTDT (bit 4).
    StartDtCon,
    /// STOPDT act: stop the data transfer (bit 5).
    StopDtAct,
    /// STOPDT con: confirmation of STOPDT (bit 6).
    StopDtCon,
    /// TESTFR act: test the connection (bit 7).
    TestFrAct,
    /// TESTFR con: confirmation of TESTFR (bit 8).
    TestFrCon,
}

impl UnnumberedFunction {
    /// The first control octet: bits 1 and 2 set, and the function bit.
    const fn control_octet(self) -> u8 {
        match self {
            Self::StartDtAct => 0x07,
            Self::StartDtCon => 0x0B,
            Self::StopDtAct => 0x13,
            Self::StopDtCon => 0x23,
            Self::TestFrAct => 0x43,
            Self::TestFrCon => 0x83,
        }
    }

    /// The function of a first control octet, or `None` when the octet has no
    /// function or more than one.
    fn from_control_octet(octet: u8) -> Option<Self> {
        match octet {
            0x07 => Some(Self::StartDtAct),
            0x0B => Some(Self::StartDtCon),
            0x13 => Some(Self::StopDtAct),
            0x23 => Some(Self::StopDtCon),
            0x43 => Some(Self::TestFrAct),
            0x83 => Some(Self::TestFrCon),
            _ => None,
        }
    }
}

/// One application protocol data unit: a frame of the I, S or U format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Apdu {
    /// I format: sequence numbers and an ASDU.
    Information {
        /// N(S): the number of this frame.
        send: SequenceNumber,
        /// N(R): the next frame expected from the peer.
        receive: SequenceNumber,
        /// The ASDU.
        asdu: Asdu,
    },
    /// S format: acknowledges the I frames up to `receive`.
    Supervisory {
        /// N(R): the next frame expected from the peer.
        receive: SequenceNumber,
    },
    /// U format: one control function.
    Unnumbered(UnnumberedFunction),
}

impl Apdu {
    /// Reads one frame, which holds exactly its octets: start, length, control
    /// field and, for the I format, the ASDU.
    pub fn decode(frame: &[u8]) -> Result<Self, DecodeError> {
        let [start, length, c1, c2, c3, c4, body @ ..] = frame else {
            return Err(DecodeError::Truncated {
                needed: HEADER_OCTETS + CONTROL_FIELD_OCTETS,
                available: frame.len(),
            });
        };
        if *start != START {
            return Err(DecodeError::FrameStart { found: *start });
        }
        let length_octets = usize::from(*length);
        if !(CONTROL_FIELD_OCTETS..=MAX_APDU_LENGTH).contains(&length_octets) {
            return Err(DecodeError::FrameLength { length: *length });
        }
        let expected = HEADER_OCTETS.saturating_add(length_octets);
        if frame.len() < expected {
            return Err(DecodeError::Truncated {
                needed: expected,
                available: frame.len(),
            });
        }
        if frame.len() > expected {
            return Err(DecodeError::TrailingOctets {
                extra: frame.len().saturating_sub(expected),
            });
        }
        let control = [*c1, *c2, *c3, *c4];
        let invalid = DecodeError::InvalidControlField { control };

        if *c1 & 1 == 0 {
            // I format: the bit 1 of the receive number octet is fixed to 0.
            if *c3 & 1 != 0 {
                return Err(invalid);
            }
            Ok(Self::Information {
                send: SequenceNumber::from_control(*c1, *c2),
                receive: SequenceNumber::from_control(*c3, *c4),
                asdu: Asdu::decode(body)?,
            })
        } else if *c1 & 2 == 0 {
            // S format: no other bit set, and no ASDU.
            if *c1 != 0x01 || *c2 != 0 || *c3 & 1 != 0 || !body.is_empty() {
                return Err(invalid);
            }
            Ok(Self::Supervisory {
                receive: SequenceNumber::from_control(*c3, *c4),
            })
        } else {
            // U format: one function, and no other bit or ASDU.
            if *c2 != 0 || *c3 != 0 || *c4 != 0 || !body.is_empty() {
                return Err(invalid);
            }
            let function = UnnumberedFunction::from_control_octet(*c1).ok_or(invalid)?;
            Ok(Self::Unnumbered(function))
        }
    }

    /// Writes the frame. Fails when the ASDU does not encode.
    pub fn to_vec(&self) -> Result<Vec<u8>, EncodeError> {
        let (control, body) = match self {
            Self::Information {
                send,
                receive,
                asdu,
            } => (
                [
                    send.low_octet(),
                    send.high_octet(),
                    receive.low_octet(),
                    receive.high_octet(),
                ],
                asdu.to_vec()?,
            ),
            Self::Supervisory { receive } => (
                [0x01, 0x00, receive.low_octet(), receive.high_octet()],
                Vec::new(),
            ),
            Self::Unnumbered(function) => ([function.control_octet(), 0, 0, 0], Vec::new()),
        };
        let length = CONTROL_FIELD_OCTETS.saturating_add(body.len());
        let length_octet = u8::try_from(length).map_err(|_| EncodeError::ValueOutOfRange {
            field: "APDU length",
        })?;
        let mut frame = Vec::with_capacity(HEADER_OCTETS.saturating_add(length));
        frame.push(START);
        frame.push(length_octet);
        frame.extend_from_slice(&control);
        frame.extend_from_slice(&body);
        Ok(frame)
    }
}

/// Cuts a byte stream into frames.
///
/// Feed it the octets as they arrive, whatever their chunks are. It takes
/// only the octets of the frame in progress from the input, so it never holds
/// more than one frame ([`MAX_FRAME_OCTETS`] octets). A framing error is
/// sticky: the frame boundaries are lost, so the connection has to be closed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrameDecoder {
    frame: Vec<u8>,
    failure: Option<DecodeError>,
}

impl FrameDecoder {
    /// A decoder with no octets buffered.
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes octets from the front of `input` until one frame is complete.
    /// Returns the frame, or `None` when `input` ran out first (it is then
    /// empty). Call it again while `input` still holds octets.
    pub fn next_frame(&mut self, input: &mut &[u8]) -> Result<Option<Vec<u8>>, DecodeError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        match self.advance(input) {
            Ok(frame) => Ok(frame),
            Err(error) => {
                self.failure = Some(error.clone());
                self.frame.clear();
                Err(error)
            }
        }
    }

    fn advance(&mut self, input: &mut &[u8]) -> Result<Option<Vec<u8>>, DecodeError> {
        // The start and length octets are checked as soon as they arrive.
        while self.frame.len() < HEADER_OCTETS {
            let Some((&octet, rest)) = input.split_first() else {
                return Ok(None);
            };
            *input = rest;
            match self.frame.len() {
                0 if octet != START => return Err(DecodeError::FrameStart { found: octet }),
                1 if !(CONTROL_FIELD_OCTETS..=MAX_APDU_LENGTH).contains(&usize::from(octet)) => {
                    return Err(DecodeError::FrameLength { length: octet });
                }
                _ => {}
            }
            self.frame.push(octet);
        }

        let Some(total) = self.total() else {
            return Ok(None);
        };
        let take = total.saturating_sub(self.frame.len()).min(input.len());
        let Some((head, rest)) = input.split_at_checked(take) else {
            return Ok(None);
        };
        self.frame.extend_from_slice(head);
        *input = rest;
        if self.frame.len() == total {
            return Ok(Some(std::mem::take(&mut self.frame)));
        }
        Ok(None)
    }

    /// The octets of the frame in progress, once the length octet is known.
    fn total(&self) -> Option<usize> {
        let length = usize::from(*self.frame.get(1)?);
        Some(HEADER_OCTETS.saturating_add(length))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::cause;

    /// C_IC_NA_1, activation, common address 1: the station interrogation.
    const INTERROGATION: &[u8] = &[
        0x64, 0x01, 0x06, 0x00, 0x01, 0x00, // header
        0x00, 0x00, 0x00, // information object address 0
        0x14, // qualifier of interrogation: station
    ];

    fn interrogation() -> Asdu {
        Asdu::decode(INTERROGATION).expect("sample decodes")
    }

    /// Frames of the three formats, with their hand-written octets, in this order:
    /// I(1, 0), S(5), the six U functions, I(32767, 32767).
    fn sample_frames() -> Vec<Vec<u8>> {
        vec![
            // I(N(S) = 1, N(R) = 0): control 02 00 00 00, then the ASDU.
            [&[START, 0x0E, 0x02, 0x00, 0x00, 0x00][..], INTERROGATION].concat(),
            // S(N(R) = 5): control 01 00 0A 00.
            vec![START, 0x04, 0x01, 0x00, 0x0A, 0x00],
            // STARTDT act, STARTDT con, STOPDT act, STOPDT con, TESTFR act, TESTFR con.
            vec![START, 0x04, 0x07, 0x00, 0x00, 0x00],
            vec![START, 0x04, 0x0B, 0x00, 0x00, 0x00],
            vec![START, 0x04, 0x13, 0x00, 0x00, 0x00],
            vec![START, 0x04, 0x23, 0x00, 0x00, 0x00],
            vec![START, 0x04, 0x43, 0x00, 0x00, 0x00],
            vec![START, 0x04, 0x83, 0x00, 0x00, 0x00],
            // I(N(S) = 32767, N(R) = 32767): control FE FF FE FF, then the ASDU.
            [&[START, 0x0E, 0xFE, 0xFF, 0xFE, 0xFF][..], INTERROGATION].concat(),
        ]
    }

    /// Feeds the chunks to a fresh decoder and collects every frame.
    fn feed(chunks: &[&[u8]]) -> Result<Vec<Vec<u8>>, DecodeError> {
        let mut decoder = FrameDecoder::new();
        let mut frames = Vec::new();
        for chunk in chunks {
            let mut input: &[u8] = chunk;
            while let Some(frame) = decoder.next_frame(&mut input)? {
                frames.push(frame);
            }
        }
        Ok(frames)
    }

    #[test]
    fn sequence_numbers_are_fifteen_bits_and_wrap() {
        assert_eq!(
            SequenceNumber::new(32767).map(SequenceNumber::value),
            Some(32767)
        );
        assert_eq!(SequenceNumber::new(32768), None);
        assert_eq!(
            SequenceNumber::new(32767)
                .map(SequenceNumber::next)
                .map(SequenceNumber::value),
            Some(0)
        );
    }

    #[test]
    fn hand_written_frames_decode_to_their_formats() {
        let information = Apdu::decode(&sample_frames()[0]).expect("I frame");
        assert_eq!(
            information,
            Apdu::Information {
                send: SequenceNumber::new(1).expect("in range"),
                receive: SequenceNumber::new(0).expect("in range"),
                asdu: interrogation(),
            }
        );
        assert_eq!(
            Apdu::decode(&[START, 0x04, 0x01, 0x00, 0x0A, 0x00]),
            Ok(Apdu::Supervisory {
                receive: SequenceNumber::new(5).expect("in range"),
            })
        );
        let functions = [
            (0x07, UnnumberedFunction::StartDtAct),
            (0x0B, UnnumberedFunction::StartDtCon),
            (0x13, UnnumberedFunction::StopDtAct),
            (0x23, UnnumberedFunction::StopDtCon),
            (0x43, UnnumberedFunction::TestFrAct),
            (0x83, UnnumberedFunction::TestFrCon),
        ];
        for (octet, function) in functions {
            assert_eq!(
                Apdu::decode(&[START, 0x04, octet, 0x00, 0x00, 0x00]),
                Ok(Apdu::Unnumbered(function)),
                "control octet {octet:#04X}"
            );
        }
    }

    #[test]
    fn encoding_matches_the_hand_written_frames() {
        let frames = sample_frames();
        let information = Apdu::Information {
            send: SequenceNumber::new(1).expect("in range"),
            receive: SequenceNumber::new(0).expect("in range"),
            asdu: interrogation(),
        };
        assert_eq!(information.to_vec(), Ok(frames[0].clone()));
        let maximal = Apdu::Information {
            send: SequenceNumber::new(32767).expect("in range"),
            receive: SequenceNumber::new(32767).expect("in range"),
            asdu: interrogation(),
        };
        assert_eq!(maximal.to_vec(), Ok(frames[8].clone()));
        assert_eq!(
            Apdu::Supervisory {
                receive: SequenceNumber::new(5).expect("in range"),
            }
            .to_vec(),
            Ok(frames[1].clone())
        );
        assert_eq!(
            Apdu::Unnumbered(UnnumberedFunction::TestFrCon).to_vec(),
            Ok(vec![START, 0x04, 0x83, 0, 0, 0])
        );
    }

    #[test]
    fn every_format_round_trips() {
        for frame in sample_frames() {
            let apdu = Apdu::decode(&frame).expect("decodes");
            assert_eq!(apdu.to_vec(), Ok(frame));
        }
    }

    #[test]
    fn a_frame_split_at_every_byte_boundary_gives_the_same_frames() {
        let frames = sample_frames();
        let stream: Vec<u8> = frames.concat();
        for split in 0..=stream.len() {
            let (head, tail) = stream.split_at(split);
            assert_eq!(feed(&[head, tail]), Ok(frames.clone()), "split at {split}");
        }
    }

    #[test]
    fn a_frame_fed_one_octet_at_a_time_gives_the_same_frames() {
        let frames = sample_frames();
        let stream: Vec<u8> = frames.concat();
        let chunks: Vec<&[u8]> = stream.chunks(1).collect();
        assert_eq!(feed(&chunks), Ok(frames));
    }

    #[test]
    fn concatenated_frames_in_one_chunk_come_out_in_order() {
        let frames = sample_frames();
        let stream: Vec<u8> = frames.concat();
        assert_eq!(feed(&[&stream]), Ok(frames));
    }

    #[test]
    fn the_decoder_takes_only_the_octets_of_one_frame() {
        let frames = sample_frames();
        let mut decoder = FrameDecoder::new();
        let stream: Vec<u8> = [frames[1].clone(), frames[2].clone()].concat();
        let mut input: &[u8] = &stream;
        assert_eq!(decoder.next_frame(&mut input), Ok(Some(frames[1].clone())));
        assert_eq!(input, frames[2].as_slice());
    }

    #[test]
    fn a_maximal_frame_is_held_until_its_last_octet() {
        // Length 253: the largest frame is 255 octets, and nothing beyond it is taken.
        let mut frame = vec![START, 0xFD];
        frame.extend(std::iter::repeat_n(0x00, 253));
        let trailing = [START, 0x04, 0x07, 0x00, 0x00, 0x00];
        let mut decoder = FrameDecoder::new();
        let stream = [frame.clone(), trailing.to_vec()].concat();
        let mut input: &[u8] = &stream[..254];
        assert_eq!(decoder.next_frame(&mut input), Ok(None));
        assert!(input.is_empty(), "every octet so far is held");
        let mut input: &[u8] = &stream[254..];
        assert_eq!(decoder.next_frame(&mut input), Ok(Some(frame)));
        assert_eq!(input, &trailing[..]);
        assert_eq!(MAX_FRAME_OCTETS, 255);
    }

    #[test]
    fn a_bad_start_octet_is_a_sticky_framing_error() {
        let mut decoder = FrameDecoder::new();
        let mut input: &[u8] = &[0x69, 0x04, 0x07, 0x00, 0x00, 0x00];
        assert_eq!(
            decoder.next_frame(&mut input),
            Err(DecodeError::FrameStart { found: 0x69 })
        );
        let mut input: &[u8] = &[START, 0x04, 0x07, 0x00, 0x00, 0x00];
        assert_eq!(
            decoder.next_frame(&mut input),
            Err(DecodeError::FrameStart { found: 0x69 }),
            "the error stays until the connection is reset"
        );
    }

    #[test]
    fn lengths_outside_4_to_253_are_framing_errors() {
        for length in [0x00, 0x03, 0xFE, 0xFF] {
            let mut decoder = FrameDecoder::new();
            let mut input: &[u8] = &[START, length, 0x00, 0x00, 0x00, 0x00];
            assert_eq!(
                decoder.next_frame(&mut input),
                Err(DecodeError::FrameLength { length }),
                "length {length}"
            );
        }
        // The smallest length, 4, is a frame of the control field only.
        assert_eq!(
            feed(&[&[START, 0x04, 0x01, 0x00, 0x00, 0x00][..]]).map(|f| f.len()),
            Ok(1)
        );
    }

    #[test]
    fn a_length_error_is_reported_before_the_rest_of_the_frame() {
        let mut decoder = FrameDecoder::new();
        let mut input: &[u8] = &[START, 0x03];
        assert_eq!(
            decoder.next_frame(&mut input),
            Err(DecodeError::FrameLength { length: 3 })
        );
    }

    #[test]
    fn apdu_decoding_rejects_bad_frames() {
        assert_eq!(
            Apdu::decode(&[0x69, 0x04, 0x07, 0x00, 0x00, 0x00]),
            Err(DecodeError::FrameStart { found: 0x69 })
        );
        assert_eq!(
            Apdu::decode(&[START, 0x04, 0x07]),
            Err(DecodeError::Truncated {
                needed: 6,
                available: 3
            })
        );
        assert_eq!(
            Apdu::decode(&[START, 0x05, 0x07, 0x00, 0x00, 0x00]),
            Err(DecodeError::Truncated {
                needed: 7,
                available: 6
            })
        );
        assert_eq!(
            Apdu::decode(&[START, 0x04, 0x07, 0x00, 0x00, 0x00, 0x00]),
            Err(DecodeError::TrailingOctets { extra: 1 })
        );
    }

    #[test]
    fn invalid_control_fields_are_rejected() {
        // I format with bit 1 of the receive number octet set.
        assert_eq!(
            Apdu::decode(&[&[START, 0x0E, 0x02, 0x00, 0x01, 0x00][..], INTERROGATION].concat()),
            Err(DecodeError::InvalidControlField {
                control: [0x02, 0x00, 0x01, 0x00]
            })
        );
        // S format with a bit above bit 2 set, or with a non-zero second octet.
        assert_eq!(
            Apdu::decode(&[START, 0x04, 0x05, 0x00, 0x00, 0x00]),
            Err(DecodeError::InvalidControlField {
                control: [0x05, 0x00, 0x00, 0x00]
            })
        );
        assert_eq!(
            Apdu::decode(&[START, 0x04, 0x01, 0x01, 0x00, 0x00]),
            Err(DecodeError::InvalidControlField {
                control: [0x01, 0x01, 0x00, 0x00]
            })
        );
        // U format: two functions at once, no function, or a non-zero octet.
        assert_eq!(
            Apdu::decode(&[START, 0x04, 0x0F, 0x00, 0x00, 0x00]),
            Err(DecodeError::InvalidControlField {
                control: [0x0F, 0x00, 0x00, 0x00]
            })
        );
        assert_eq!(
            Apdu::decode(&[START, 0x04, 0x03, 0x00, 0x00, 0x00]),
            Err(DecodeError::InvalidControlField {
                control: [0x03, 0x00, 0x00, 0x00]
            })
        );
        assert_eq!(
            Apdu::decode(&[START, 0x04, 0x07, 0x01, 0x00, 0x00]),
            Err(DecodeError::InvalidControlField {
                control: [0x07, 0x01, 0x00, 0x00]
            })
        );
    }

    #[test]
    fn an_unsupported_asdu_is_reported_and_the_stream_goes_on() {
        // M_SP_TA_1 (type 2) inside an I frame: the frame is cut, decoding it fails.
        let unsupported = [
            vec![START, 0x0E, 0x00, 0x00, 0x00, 0x00],
            vec![
                0x02,
                0x01,
                cause::SPONTANEOUS,
                0x00,
                0x01,
                0x00,
                0x01,
                0x00,
                0x00,
                0x01,
            ],
        ]
        .concat();
        let stream = [unsupported.clone(), sample_frames()[1].clone()].concat();
        let frames = feed(&[&stream]).expect("framing is fine");
        assert_eq!(frames.len(), 2);
        assert!(matches!(
            Apdu::decode(&frames[0]),
            Err(DecodeError::UnsupportedTypeId { type_code: 2, .. })
        ));
        assert_eq!(
            Apdu::decode(&frames[1]),
            Ok(Apdu::Supervisory {
                receive: SequenceNumber::new(5).expect("in range")
            })
        );
    }
}
