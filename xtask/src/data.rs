// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

type Fail = Box<dyn std::error::Error>;

#[derive(serde::Deserialize)]
pub struct Profile {
    #[serde(rename = "asdu_header")]
    pub header: Header,
    #[serde(rename = "type_ids")]
    pub type_ids: BTreeMap<u8, TypeInfo>,
}

#[derive(serde::Deserialize)]
pub struct Header {
    pub byte_order: String,
    pub cause_of_transmission_octets: u8,
    pub common_address_octets: u8,
    pub information_object_address_octets: u8,
    pub max_apdu_length: u16,
}

#[derive(serde::Deserialize)]
pub struct TypeInfo {
    pub mnemonic: String,
    pub name: String,
    pub defined_in: String,
    pub in_104_profile: bool,
    pub sq_allowed: Vec<u8>,
    pub information_elements: Vec<Element>,
    pub object_size_bits: Option<u16>,
    pub time_tag: String,
    pub cot_104: Vec<u8>,
    pub cot_not_permitted: Vec<u8>,
}

#[derive(serde::Deserialize)]
pub struct Element {
    #[allow(dead_code)]
    pub size_bits: Option<u16>,
}

#[derive(serde::Deserialize)]
pub struct Formats {
    pub formats: BTreeMap<String, Format>,
}

#[derive(serde::Deserialize)]
pub struct Format {
    pub size_bits: Option<u16>,
}

pub fn validate(profile: &Profile, formats: &Formats) -> Result<(), Fail> {
    if profile.header.byte_order != "lsb_first" {
        return Err("unexpected ASDU byte order in profile_104.yaml".into());
    }
    if profile.type_ids.len() != 67 {
        return Err(format!(
            "expected 67 type IDs in profile_104.yaml, found {}",
            profile.type_ids.len()
        )
        .into());
    }
    for (id, info) in &profile.type_ids {
        if !matches!(info.time_tag.as_str(), "none" | "CP24Time2a" | "CP56Time2a") {
            return Err(format!("unknown time tag {:?} on type {id}", info.time_tag).into());
        }
        if info.sq_allowed.iter().any(|v| *v != 0u8 && *v != 1u8) {
            return Err(format!("unexpected sq_allowed {:?} on type {id}", info.sq_allowed).into());
        }
        if info.mnemonic.len() < 4 {
            return Err(format!("unexpected mnemonic {:?} on type {id}", info.mnemonic).into());
        }
        if let Some(total) = info.object_size_bits {
            let mut sum: u32 = info
                .information_elements
                .iter()
                .map(|e| u32::from(e.size_bits.unwrap_or(0)))
                .sum();
            if info.time_tag != "none" {
                sum += u32::from(
                    formats
                        .formats
                        .get(info.time_tag.as_str())
                        .and_then(|f| f.size_bits)
                        .unwrap_or(0),
                );
            }
            if u32::from(total) != sum {
                return Err(format!(
                    "object_size_bits of {} ({total}) differs from \
                     elements+time ({sum})",
                    info.mnemonic
                )
                .into());
            }
        }
    }
    Ok(())
}
