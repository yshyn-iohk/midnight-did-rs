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
//!
//! ## v0.2.0 type change — drop String shadow primitives
//!
//! [`ContractAddress`] used to be a local `pub struct
//! ContractAddress(pub String)` wrapping 64-char hex; same for the
//! off-chain state hash. These two types are now re-exported from the
//! upstream Midnight ledger libraries:
//!
//! - [`ContractAddress`] = [`compact_runtime::ContractAddress`] (which is
//!   `midnight_coin_structure::contract::ContractAddress(pub HashOutput)`)
//! - [`OffchainStateHashHex`] = [`midnight_base_crypto::hash::HashOutput`]
//!
//! The in-memory representation is therefore a `[u8; 32]` rather than a
//! 64-character `String`, inheriting all the upstream derives we need
//! (`FieldRepr` / `FromFieldRepr` / `BinaryHashRepr` / `Serializable` /
//! `Zeroize` / constant-time `eq`). Hex round-trips go through the
//! [`crate::hex_ext::HashOutputExt`] extension trait.
//!
//! The JSON wire shape of the W3C DID Document is unaffected — DID
//! identifiers are always embedded inside the `did:midnight:net:<hex>`
//! string form, never serialised as a bare `ContractAddress` JSON field.

pub use compact_runtime::ContractAddress;
pub use midnight_base_crypto::hash::HashOutput as OffchainStateHashHex;

use crate::hex_ext::HashOutputExt;
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

/// Subject id portion of a Midnight DID — either a contract address or an
/// off-chain state hash. Both variants now hold the upstream-typed 32-byte
/// representation; render via [`Self::to_hex`] when a 64-character hex
/// string is needed (e.g. when re-assembling a `did:midnight:` URI).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MidnightSubjectId {
    /// Contract address (`did:midnight:devnet:<addr>`). Backed by the
    /// upstream [`compact_runtime::ContractAddress`] which is in turn a
    /// `(pub HashOutput)` newtype — full ledger-trait stack derived
    /// upstream.
    Contract(ContractAddress),
    /// Offchain state hash (`did:midnight:offchain:<hash>`). Backed by
    /// the upstream [`midnight_base_crypto::hash::HashOutput`] — same
    /// 32-byte storage, same trait stack.
    Offchain(OffchainStateHashHex),
}

impl MidnightSubjectId {
    /// Render the underlying 32-byte identifier as a 64-character
    /// lowercase hex string, regardless of variant. Mirrors the
    /// pre-v0.2.0 `as_hex(&self) -> &str` shape, but allocates because
    /// the in-memory representation is now bytes (was a borrowable
    /// `String`).
    pub fn to_hex(&self) -> String {
        match self {
            MidnightSubjectId::Contract(c) => c.to_hex(),
            MidnightSubjectId::Offchain(h) => h.to_hex(),
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

/// Validate a 32-byte hex contract address (case-insensitive) and lift
/// it into the upstream [`ContractAddress`] type.
///
/// Returns [`MidnightDidError::BadContractAddress`] on any hex error —
/// wrong length, non-hex chars, or any failure in the underlying
/// [`HashOutputExt::from_hex`] conversion.
pub fn parse_contract_address(input: &str) -> Result<ContractAddress, MidnightDidError> {
    if !is_hex64(input) {
        return Err(MidnightDidError::BadContractAddress);
    }
    // Mixed-case allowed for contract addresses; lowercase before
    // handing to from_hex (the hex crate is case-insensitive but the
    // explicit normalisation here documents the intent and matches
    // the upstream Display rendering).
    let lower = input.to_ascii_lowercase();
    ContractAddress::from_hex(&lower).map_err(|_| MidnightDidError::BadContractAddress)
}

/// Validate a 32-byte **lowercase**-hex offchain state hash and lift
/// it into the upstream [`HashOutput`] type ([`OffchainStateHashHex`]).
///
/// Off-chain identifiers must use lowercase hex — mirrors the TS
/// `parseOffchainStateHash` invariant. Mixed-case is rejected with
/// [`MidnightDidError::BadOffchainStateHash`].
pub fn parse_offchain_state_hash(input: &str) -> Result<OffchainStateHashHex, MidnightDidError> {
    if !is_lowercase_hex64(input) {
        return Err(MidnightDidError::BadOffchainStateHash);
    }
    OffchainStateHashHex::from_hex(input).map_err(|_| MidnightDidError::BadOffchainStateHash)
}

/// Build a `did:midnight:<network>:<id>` string. Mirrors
/// `createMidnightDIDString` in TS — does **not** validate the
/// underlying id (callers are expected to use the parsers first or pass
/// a hex string that already round-trips through
/// [`HashOutputExt::to_hex`]).
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

/// Decompose a Midnight DID string into its `(network, subject_id)` pair.
///
/// Both subject variants now wrap upstream-typed `[u8; 32]` storage; the
/// hex sub-string from the DID URI is round-tripped through
/// [`HashOutputExt::from_hex`].
pub fn parse_midnight_did(did: &MidnightDidString) -> Result<(MidnightNetwork, MidnightSubjectId), MidnightDidError> {
    let parts: Vec<&str> = did.0.split(':').collect();
    let net = parts.get(2).copied().unwrap_or("");
    let id_str = parts.get(3).copied().unwrap_or("");
    let network = MidnightNetwork::from_wire_str(net).ok_or(MidnightDidError::UnknownNetwork)?;
    let subject = if matches!(network, MidnightNetwork::Offchain) {
        let hash = OffchainStateHashHex::from_hex(id_str).map_err(|_| MidnightDidError::BadOffchainStateHash)?;
        MidnightSubjectId::Offchain(hash)
    } else {
        let addr = ContractAddress::from_hex(&id_str.to_ascii_lowercase())
            .map_err(|_| MidnightDidError::BadContractAddress)?;
        MidnightSubjectId::Contract(addr)
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
        // Address now backed by upstream ContractAddress(pub HashOutput);
        // hex round-trip goes through HashOutputExt::to_hex.
        let did = create_midnight_did_string(&address.to_hex(), MidnightNetwork::DevNet);
        assert_eq!(did.0, format!("did:midnight:devnet:{SAMPLE}"));
        let parsed = parse_midnight_did_string(&did.0).unwrap();
        let (net, id) = parse_midnight_did(&parsed).unwrap();
        assert_eq!(net, MidnightNetwork::DevNet);
        assert_eq!(id.to_hex(), SAMPLE);
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
        assert_eq!(id.to_hex(), SAMPLE);
    }

    // ---- v0.2.0 type-change coverage ----------------------------------

    #[test]
    fn parse_contract_address_returns_upstream_type() {
        // Confirm the parser yields an upstream ContractAddress whose
        // inner HashOutput's bytes match the hex-decoded input. This
        // pins the new in-memory shape (was a 64-char String).
        let addr = parse_contract_address(SAMPLE).unwrap();
        let inner_hash: midnight_base_crypto::hash::HashOutput = addr.0;
        let bytes = inner_hash.0;
        // SAMPLE is "cccc..." 64 chars → 32 bytes all 0xcc.
        assert_eq!(bytes, [0xccu8; 32]);
    }

    #[test]
    fn parse_offchain_state_hash_returns_upstream_type() {
        let hash = parse_offchain_state_hash(SAMPLE).unwrap();
        assert_eq!(hash.0, [0xccu8; 32]);
    }

    #[test]
    fn parse_offchain_state_hash_rejects_mixed_case() {
        let mixed = "AAAAaaaaAAAAaaaaAAAAaaaaAAAAaaaaAAAAaaaaAAAAaaaaAAAAaaaaAAAAaaaa";
        assert_eq!(mixed.len(), 64);
        let err = parse_offchain_state_hash(mixed).unwrap_err();
        assert!(matches!(err, MidnightDidError::BadOffchainStateHash));
    }

    #[test]
    fn midnight_subject_id_renders_hex_for_contract_variant() {
        let addr = parse_contract_address(SAMPLE).unwrap();
        let subj = MidnightSubjectId::Contract(addr);
        assert_eq!(subj.to_hex(), SAMPLE);
    }

    #[test]
    fn midnight_subject_id_renders_hex_for_offchain_variant() {
        let hash = parse_offchain_state_hash(SAMPLE).unwrap();
        let subj = MidnightSubjectId::Offchain(hash);
        assert_eq!(subj.to_hex(), SAMPLE);
    }

    #[test]
    fn parse_contract_address_accepts_mixed_case() {
        let mixed = "Cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        assert_eq!(mixed.len(), 64);
        let addr = parse_contract_address(mixed).unwrap();
        // Should normalise to lowercase internally; round-trip via
        // upstream to_hex confirms.
        assert_eq!(addr.to_hex(), SAMPLE);
    }

    #[test]
    fn parse_contract_address_rejects_wrong_length() {
        let err = parse_contract_address("dead").unwrap_err();
        assert!(matches!(err, MidnightDidError::BadContractAddress));
    }
}
