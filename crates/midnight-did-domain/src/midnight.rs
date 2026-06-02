// This file is part of Compact.
// Copyright (C) 2026 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Midnight-method-specific DID types.
//!
//! Port of `midnight.ts`. The shape is `did:midnight:<network>:<id>` for
//! the on-chain variants and `did:midnight:offchain:<state_hash>[:<state>]`
//! for the off-chain encoding. Validation rules match the TS source.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Networks the Midnight method recognises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MidnightNetwork {
    /// Local / not-deployed environment.
    Undeployed,
    /// Long-lived dev network.
    DevNet,
    /// Public testnet.
    Testnet,
    /// Production mainnet.
    Mainnet,
    /// Pre-mainnet preview environment.
    Preview,
    /// Pre-mainnet preprod environment.
    Preprod,
    /// Off-chain (state-hash) DID.
    Offchain,
}

impl MidnightNetwork {
    /// Lowercase wire spelling used inside a Midnight DID string.
    pub fn as_wire_str(self) -> &'static str {
        match self {
            MidnightNetwork::Undeployed => "undeployed",
            MidnightNetwork::DevNet => "devnet",
            MidnightNetwork::Testnet => "testnet",
            MidnightNetwork::Mainnet => "mainnet",
            MidnightNetwork::Preview => "preview",
            MidnightNetwork::Preprod => "preprod",
            MidnightNetwork::Offchain => "offchain",
        }
    }

    /// Parse the lowercase wire spelling.
    pub fn from_wire_str(value: &str) -> Option<Self> {
        Some(match value {
            "undeployed" => MidnightNetwork::Undeployed,
            "devnet" => MidnightNetwork::DevNet,
            "testnet" => MidnightNetwork::Testnet,
            "mainnet" => MidnightNetwork::Mainnet,
            "preview" => MidnightNetwork::Preview,
            "preprod" => MidnightNetwork::Preprod,
            "offchain" => MidnightNetwork::Offchain,
            _ => return None,
        })
    }
}

/// 32-byte contract address in hex (64 chars, mixed-case allowed).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContractAddress(pub String);

/// 32-byte offchain DID state hash in **lowercase** hex.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OffchainStateHashHex(pub String);

/// Subject id portion of a Midnight DID — either a contract address or an
/// off-chain state hash.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MidnightSubjectId {
    /// Contract address (`did:midnight:devnet:<addr>`).
    Contract(ContractAddress),
    /// Offchain state hash (`did:midnight:offchain:<hash>`).
    Offchain(OffchainStateHashHex),
}

impl MidnightSubjectId {
    /// Return the underlying hex string regardless of the variant.
    pub fn as_hex(&self) -> &str {
        match self {
            MidnightSubjectId::Contract(c) => &c.0,
            MidnightSubjectId::Offchain(h) => &h.0,
        }
    }
}

/// `did:midnight:...` DID string with method-specific validation applied.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MidnightDidString(pub String);

/// Errors returned while parsing Midnight DID strings.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MidnightDidError {
    /// Contract address was not 64 hex chars.
    #[error("Contract address must be 64 hex chars")]
    BadContractAddress,
    /// Offchain state hash was not lowercase 64-hex.
    #[error("Offchain state hash must use lowercase hex")]
    BadOffchainStateHash,
    /// String did not start with `did:midnight:`.
    #[error("Invalid Midnight DID format")]
    BadFormat,
    /// Network token did not match a known network.
    #[error("Unknown network in Midnight DID")]
    UnknownNetwork,
    /// Method-specific identifier was malformed.
    #[error("Invalid method-specific identifier in Midnight DID")]
    BadMethodSpecificId,
    /// Offchain identifier used mixed-case hex.
    #[error("Offchain Midnight DID identifiers must use lowercase hex")]
    OffchainNotLowercase,
    /// Offchain state segment was not unpadded base64url.
    #[error("Invalid offchain Midnight DID state encoding")]
    BadOffchainStateEncoding,
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_lowercase_hex64(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

fn is_base64url_segment(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') && s.len() % 4 != 1
}

/// Validate a 32-byte hex contract address (case-insensitive).
pub fn parse_contract_address(input: &str) -> Result<ContractAddress, MidnightDidError> {
    if !is_hex64(input) {
        return Err(MidnightDidError::BadContractAddress);
    }
    Ok(ContractAddress(input.to_owned()))
}

/// Validate a 32-byte lowercase-hex offchain state hash.
pub fn parse_offchain_state_hash(input: &str) -> Result<OffchainStateHashHex, MidnightDidError> {
    if !is_lowercase_hex64(input) {
        return Err(MidnightDidError::BadOffchainStateHash);
    }
    Ok(OffchainStateHashHex(input.to_owned()))
}

/// Build a `did:midnight:<network>:<id>` string. Mirrors
/// `createMidnightDIDString` in TS — does **not** validate the underlying
/// id (callers are expected to use the `ContractAddress` / `OffchainStateHashHex`
/// constructors first).
pub fn create_midnight_did_string(id: &str, network: MidnightNetwork) -> MidnightDidString {
    MidnightDidString(format!("did:midnight:{}:{id}", network.as_wire_str()))
}

/// Validate a candidate `did:midnight:...` string.
pub fn parse_midnight_did_string(input: &str) -> Result<MidnightDidString, MidnightDidError> {
    if !input.starts_with("did:midnight:") {
        return Err(MidnightDidError::BadFormat);
    }
    let parts: Vec<&str> = input.split(':').collect();
    let net = parts.get(2).copied().unwrap_or("");
    if net == "offchain" {
        if parts.len() != 4 && parts.len() != 5 {
            return Err(MidnightDidError::BadFormat);
        }
    } else if parts.len() != 4 {
        return Err(MidnightDidError::BadFormat);
    }
    let _network = MidnightNetwork::from_wire_str(net).ok_or(MidnightDidError::UnknownNetwork)?;
    let identifier = parts.get(3).copied().unwrap_or("");
    if !is_hex64(identifier) {
        return Err(MidnightDidError::BadMethodSpecificId);
    }
    if net == "offchain" && !is_lowercase_hex64(identifier) {
        return Err(MidnightDidError::OffchainNotLowercase);
    }
    if net == "offchain" {
        if let Some(state) = parts.get(4) {
            if !is_base64url_segment(state) {
                return Err(MidnightDidError::BadOffchainStateEncoding);
            }
        }
    }
    Ok(MidnightDidString(input.to_owned()))
}

/// Decompose a Midnight DID string into its `(network, id)` pair.
pub fn parse_midnight_did(did: &MidnightDidString) -> Result<(MidnightNetwork, MidnightSubjectId), MidnightDidError> {
    let parts: Vec<&str> = did.0.split(':').collect();
    let net = parts.get(2).copied().unwrap_or("");
    let id = parts.get(3).copied().unwrap_or("").to_owned();
    let network = MidnightNetwork::from_wire_str(net).ok_or(MidnightDidError::UnknownNetwork)?;
    let subject = if matches!(network, MidnightNetwork::Offchain) {
        MidnightSubjectId::Offchain(OffchainStateHashHex(id))
    } else {
        MidnightSubjectId::Contract(ContractAddress(id))
    };
    Ok((network, subject))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    #[test]
    fn build_and_decompose() {
        let address = parse_contract_address(SAMPLE).unwrap();
        let did = create_midnight_did_string(&address.0, MidnightNetwork::DevNet);
        assert_eq!(did.0, format!("did:midnight:devnet:{SAMPLE}"));
        let parsed = parse_midnight_did_string(&did.0).unwrap();
        let (net, id) = parse_midnight_did(&parsed).unwrap();
        assert_eq!(net, MidnightNetwork::DevNet);
        assert_eq!(id.as_hex(), SAMPLE);
    }

    #[test]
    fn rejects_unknown_network() {
        let err = parse_midnight_did_string(&format!("did:midnight:unknown:{SAMPLE}")).unwrap_err();
        assert!(matches!(err, MidnightDidError::UnknownNetwork));
    }

    #[test]
    fn rejects_uppercase_offchain() {
        let err = parse_midnight_did_string(&format!("did:midnight:offchain:{}", "C".repeat(64))).unwrap_err();
        assert!(matches!(err, MidnightDidError::OffchainNotLowercase));
    }

    #[test]
    fn rejects_bad_state_encoding() {
        let err = parse_midnight_did_string(&format!("did:midnight:offchain:{}:not+base64url", SAMPLE)).unwrap_err();
        assert!(matches!(err, MidnightDidError::BadOffchainStateEncoding));
    }

    #[test]
    fn maps_undeployed_preview_preprod() {
        for (wire, expected) in [
            ("undeployed", MidnightNetwork::Undeployed),
            ("preview", MidnightNetwork::Preview),
            ("preprod", MidnightNetwork::Preprod),
        ] {
            let did = format!("did:midnight:{wire}:{SAMPLE}");
            let parsed = parse_midnight_did_string(&did).unwrap();
            assert_eq!(parse_midnight_did(&parsed).unwrap().0, expected);
        }
    }

    #[test]
    fn long_form_offchain_parses() {
        let did = format!("did:midnight:offchain:{SAMPLE}:AQIDBA");
        let parsed = parse_midnight_did_string(&did).unwrap();
        let (net, id) = parse_midnight_did(&parsed).unwrap();
        assert_eq!(net, MidnightNetwork::Offchain);
        assert_eq!(id.as_hex(), SAMPLE);
    }
}
