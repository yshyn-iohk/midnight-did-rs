// This file is part of midnightntwrk/midnight-did-rs.
// Copyright (C) 2026 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Typed circuit-call envelope serialised into [`crate::backend::BuiltTx`].
//!
//! Path 2 strategy (ADR 0008, v0.4.0): rather than have [`Contract<B>`] route
//! into the codegen'd `generated::Contract<PS, W>` — which requires a
//! wallet/proof-server bridge that is not yet implemented — we encode each
//! invocation as a [`DidContractCall`] variant, serialise it into
//! [`BuiltTx::bytes`], and let [`crate::backend::Backend::submit_tx`] forward
//! the envelope. The recording mock decodes the envelope back into the typed
//! enum and exposes it via `recorded_calls()` for tests.
//!
//! Variant payloads are 1:1 with the legacy `RecordedCall` enum that used to
//! live in `midnight-did-api/src/contract.rs` so the api-layer test migration
//! is mechanical. The 12 mutating circuits plus a synthetic `ReadLedger`
//! variant (recorded by [`crate::backend::RecordingBackend::read_snapshot`])
//! preserve the existing call-sequence assertions verbatim.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use midnight_did_domain::did_document::{CurveType, KeyType, VerificationMethodType};

use crate::backend::BackendError;

// ─────────────────────────────────────────────────────────────────────
// Ledger-shape value types (moved from midnight-did-api per R2-2.1)
// ─────────────────────────────────────────────────────────────────────

/// Ledger map-mutation tag mirroring `DIDContract.MapMutation`.
///
/// Selects insert-vs-update semantics for `setVerificationMethod`,
/// `setSchnorrJubjubVerificationMethod`, and `setService` circuits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MapMutation {
    /// New entry must not exist; insert it.
    Insert,
    /// Entry must exist; replace its value.
    Update,
}

/// Ledger set-mutation tag mirroring `DIDContract.SetMutation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SetMutation {
    /// Add the element to the set.
    Insert,
    /// Remove the element from the set.
    Remove,
}

/// Verification-method relation tag matching the on-chain enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LedgerVerificationMethodRelation {
    /// Sentinel — never written to the ledger but defined by the enum.
    Undefined,
    /// Authentication relation.
    Authentication,
    /// Assertion-method relation.
    AssertionMethod,
    /// Key-agreement relation.
    KeyAgreement,
    /// Capability-invocation relation.
    CapabilityInvocation,
    /// Capability-delegation relation.
    CapabilityDelegation,
}

/// Ledger-shaped public-key JWK.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerPublicKeyJwk {
    /// Key type.
    pub kty: KeyType,
    /// Curve.
    pub crv: CurveType,
    /// X coordinate (base64url).
    pub x: String,
    /// Y coordinate (base64url) — empty for OKP profiles.
    pub y: String,
}

/// Ledger-shaped verification method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerVerificationMethod {
    /// Fragment-id form of the verification method id (e.g. `#key-1`).
    pub id: String,
    /// Verification method type discriminant.
    pub typ: VerificationMethodType,
    /// Ledger-shaped public-key JWK.
    pub public_key_jwk: LedgerPublicKeyJwk,
}

/// Pair-of-coordinate Jubjub point in hex form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JubjubPointHex {
    /// X coordinate as hex.
    pub x: String,
    /// Y coordinate as hex.
    pub y: String,
}

/// Ledger-shaped Schnorr-Jubjub verification method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerSchnorrJubjubVerificationMethod {
    /// Fragment-id form of the verification method id.
    pub id: String,
    /// Jubjub point coordinates as little-endian-base16 hex strings.
    pub public_key: JubjubPointHex,
}

/// Ledger-shaped service entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerService {
    /// Fragment-id form of the service id.
    pub id: String,
    /// Service `type` (single string or JSON-array form).
    pub typ: String,
    /// Service endpoint encoded as the canonical JSON string.
    pub service_endpoint: String,
}

/// Schnorr-Jubjub digest argument to
/// [`DidContractCall::VerifySchnorrJubjubDigestSignature`].
pub type SchnorrJubjubDigest = [String; 4];

/// Schnorr signature payload — kept opaque so the api layer can carry it
/// across without depending on the runtime's representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchnorrJubjubSignature {
    /// Opaque signature bytes (hex-encoded).
    pub bytes_hex: String,
}

/// Plain-data snapshot of the DID contract's `Ledger`.
///
/// API code consumes this view; the contract impl produces it by reading the
/// generated `Ledger` accessors. Where the on-chain ledger uses `Counter`,
/// `Set`, `Map`, we represent it here as primitives + std-collections so the
/// api layer can be compiled without taking the on-chain runtime types into
/// its public surface.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DidLedgerSnapshot {
    /// 32-byte contract identifier (hex).
    pub id_hex: String,
    /// Active flag.
    pub active: bool,
    /// Deactivated flag.
    pub deactivated: bool,
    /// Controller public key (32 bytes, hex).
    pub controller_public_key_hex: String,
    /// Monotonic version counter.
    pub version: u64,
    /// Monotonic operation counter.
    pub operation_count: u64,
    /// Contract semantic version cell.
    pub contract_version: u64,
    /// `created` timestamp in milliseconds since the Unix epoch.
    pub created_ms: u64,
    /// `updated` timestamp in milliseconds since the Unix epoch.
    pub updated_ms: u64,
    /// `alsoKnownAs` set members.
    pub also_known_as: Vec<String>,
    /// JWK verification methods keyed by fragment id.
    pub verification_methods: BTreeMap<String, LedgerVerificationMethod>,
    /// Schnorr-Jubjub verification methods keyed by fragment id.
    pub schnorr_jubjub_verification_methods: BTreeMap<String, LedgerSchnorrJubjubVerificationMethod>,
    /// Authentication relation members.
    pub authentication_relation: Vec<String>,
    /// Assertion-method relation members.
    pub assertion_method_relation: Vec<String>,
    /// Key-agreement relation members.
    pub key_agreement_relation: Vec<String>,
    /// Capability-invocation relation members.
    pub capability_invocation_relation: Vec<String>,
    /// Capability-delegation relation members.
    pub capability_delegation_relation: Vec<String>,
    /// Services keyed by fragment id.
    pub services: BTreeMap<String, LedgerService>,
}

impl DidLedgerSnapshot {
    /// Return the relation set for a given relation kind.
    pub fn relation_set(&self, relation: LedgerVerificationMethodRelation) -> Option<&[String]> {
        match relation {
            LedgerVerificationMethodRelation::Undefined => None,
            LedgerVerificationMethodRelation::Authentication => Some(&self.authentication_relation),
            LedgerVerificationMethodRelation::AssertionMethod => Some(&self.assertion_method_relation),
            LedgerVerificationMethodRelation::KeyAgreement => Some(&self.key_agreement_relation),
            LedgerVerificationMethodRelation::CapabilityInvocation => Some(&self.capability_invocation_relation),
            LedgerVerificationMethodRelation::CapabilityDelegation => Some(&self.capability_delegation_relation),
        }
    }

    /// Return `true` if `relation` contains `normalized_method_id`.
    pub fn relation_contains(&self, relation: LedgerVerificationMethodRelation, normalized_method_id: &str) -> bool {
        self.relation_set(relation)
            .map(|members| members.iter().any(|m| m == normalized_method_id))
            .unwrap_or(false)
    }
}

// ─────────────────────────────────────────────────────────────────────
// DidContractCall — typed envelope serialised into BuiltTx.bytes
// ─────────────────────────────────────────────────────────────────────

/// Typed envelope for one DID contract circuit invocation.
///
/// `Contract<B>` builds a [`DidContractCall`], serialises it via
/// [`Self::encode`] into [`crate::backend::BuiltTx::bytes`], and forwards
/// the result to [`crate::backend::Backend::submit_tx`]. The recording
/// backend reverses the encoding through [`Self::decode`] and exposes the
/// typed call list for test assertions.
///
/// Variant payloads mirror the 12 mutating circuits exported by `did.compact`
/// plus a synthetic [`Self::ReadLedger`] entry recorded by
/// [`crate::backend::RecordingBackend::read_snapshot`] so tests that count
/// on the read-ledger position in the call sequence keep working.
///
/// The wire format is JSON (via `serde_json`). The exact bytes are an
/// implementation detail — only the backend that produced them is expected
/// to decode them. Once the wallet/proof bridge lands `LiveBackend` will
/// either materialise these envelopes into real transactions or replace this
/// representation entirely; the trait-level surface stays identical.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DidContractCall {
    /// Synthetic — recorded by [`crate::backend::RecordingBackend::read_snapshot`]
    /// so test parity with the legacy `RecordedCall::ReadLedger` variant is
    /// preserved.
    ReadLedger,
    /// `rotateControllerKey(new_pk)`.
    RotateControllerKey {
        /// New controller public key (32 bytes).
        new_public_key: [u8; 32],
    },
    /// `setVerificationMethod(method, mutation)`.
    SetVerificationMethod {
        /// Ledger-shaped verification method.
        method: LedgerVerificationMethod,
        /// Insert vs update.
        mutation: MapMutation,
    },
    /// `removeVerificationMethod(methodId)`.
    RemoveVerificationMethod {
        /// Normalised fragment id.
        method_id: String,
    },
    /// `setSchnorrJubjubVerificationMethod(method, mutation)`.
    SetSchnorrJubjubVerificationMethod {
        /// Ledger-shaped Schnorr-Jubjub verification method.
        method: LedgerSchnorrJubjubVerificationMethod,
        /// Insert vs update.
        mutation: MapMutation,
    },
    /// `removeSchnorrJubjubVerificationMethod(methodId)`.
    RemoveSchnorrJubjubVerificationMethod {
        /// Normalised fragment id.
        method_id: String,
    },
    /// `verifySchnorrJubjubDigestSignature(methodId, digest, signature)`.
    VerifySchnorrJubjubDigestSignature {
        /// Normalised fragment id.
        method_id: String,
        /// 4-limb digest representation.
        digest: SchnorrJubjubDigest,
        /// Signature bytes (hex).
        signature: SchnorrJubjubSignature,
    },
    /// `setVerificationMethodRelation(relation, methodId, mutation)`.
    SetVerificationMethodRelation {
        /// Relation kind.
        relation: LedgerVerificationMethodRelation,
        /// Normalised method id.
        method_id: String,
        /// Add vs remove.
        mutation: SetMutation,
    },
    /// `setService(service, mutation)`.
    SetService {
        /// Ledger-shaped service entry.
        service: LedgerService,
        /// Insert vs update.
        mutation: MapMutation,
    },
    /// `removeService(serviceId)`.
    RemoveService {
        /// Normalised service id.
        service_id: String,
    },
    /// `setAlsoKnownAs(aliasUri, mutation)`.
    SetAlsoKnownAs {
        /// Alias URI.
        alias_uri: String,
        /// Add vs remove.
        mutation: SetMutation,
    },
    /// `deactivate()`.
    Deactivate,
}

impl DidContractCall {
    /// Serialise this envelope into raw bytes for
    /// [`crate::backend::BuiltTx::bytes`].
    ///
    /// The wire format is JSON. Any future ledger-byte-parity work that
    /// retires the JSON envelope can either swap this body for the
    /// real transaction encoder or wire `Contract<B>` past the envelope
    /// step entirely — `DidContractCall` is purely an in-process
    /// intermediate.
    pub fn encode(&self) -> Vec<u8> {
        // `unwrap` is justified: the variants only contain owned plain-data
        // types whose serde impls are total (no `Path`s, no streams, no
        // foreign trait objects).
        serde_json::to_vec(self).expect("DidContractCall serialises to JSON")
    }

    /// Reverse [`Self::encode`].
    pub fn decode(bytes: &[u8]) -> Result<Self, BackendError> {
        serde_json::from_slice(bytes).map_err(|err| BackendError::Decode(format!("DidContractCall: {err}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_vm() -> LedgerVerificationMethod {
        LedgerVerificationMethod {
            id: "#key-1".into(),
            typ: VerificationMethodType::JsonWebKey,
            public_key_jwk: LedgerPublicKeyJwk {
                kty: KeyType::EC,
                crv: CurveType::P256,
                x: "Zm9v".into(),
                y: "YmFy".into(),
            },
        }
    }

    #[test]
    fn rotate_controller_key_roundtrip() {
        let call = DidContractCall::RotateControllerKey {
            new_public_key: [7u8; 32],
        };
        let bytes = call.encode();
        let decoded = DidContractCall::decode(&bytes).unwrap();
        assert_eq!(call, decoded);
    }

    #[test]
    fn set_verification_method_roundtrip() {
        let call = DidContractCall::SetVerificationMethod {
            method: sample_vm(),
            mutation: MapMutation::Update,
        };
        let bytes = call.encode();
        let decoded = DidContractCall::decode(&bytes).unwrap();
        assert_eq!(call, decoded);
    }

    #[test]
    fn deactivate_roundtrip() {
        let call = DidContractCall::Deactivate;
        let bytes = call.encode();
        let decoded = DidContractCall::decode(&bytes).unwrap();
        assert_eq!(call, decoded);
    }

    #[test]
    fn decode_rejects_garbage() {
        let res = DidContractCall::decode(&[0xff, 0xfe]);
        assert!(matches!(res, Err(BackendError::Decode(_))));
    }

    #[test]
    fn relation_set_lookup() {
        let mut snap = DidLedgerSnapshot::default();
        snap.authentication_relation.push("#k1".into());
        assert!(snap.relation_contains(LedgerVerificationMethodRelation::Authentication, "#k1"));
        assert!(!snap.relation_contains(LedgerVerificationMethodRelation::AssertionMethod, "#k1"));
        assert!(snap.relation_set(LedgerVerificationMethodRelation::Undefined).is_none());
    }
}
