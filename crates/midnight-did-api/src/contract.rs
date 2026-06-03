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

//! Abstraction over the on-chain Midnight DID contract.
//!
//! The TypeScript API talks to `didContract.callTx.<circuit>` directly. In
//! Rust we hide the runtime crate behind the [`DidContract`] trait so the API
//! layer can be built and unit-tested independently of the halo2 stack that
//! `midnight-did` (the runtime crate) depends on. A real implementation that
//! drives the generated contract via `compact-runtime` belongs in a separate
//! crate (or feature) and can be added once the runtime side builds
//! end-to-end.
//!
//! The trait deliberately mirrors the *exported circuit* surface of the
//! `did.compact` contract (one method per impure circuit) plus a
//! [`DidContract::read_ledger`] accessor that returns a [`DidLedgerSnapshot`]
//! — a plain-data view that the API layer can consume without depending on
//! the runtime types.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::error::ContractError;
use midnight_did_domain::did_document::{CurveType, KeyType, VerificationMethodType};

/// Ledger map-mutation tag mirroring `DIDContract.MapMutation`.
///
/// Selects insert-vs-update semantics for `setVerificationMethod`,
/// `setSchnorrJubjubVerificationMethod`, and `setService` circuits. The wire
/// values do not matter to API callers — they are routed by the
/// [`DidContract`] impl.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MapMutation {
    /// New entry must not exist; insert it.
    Insert,
    /// Entry must exist; replace its value.
    Update,
}

/// Ledger set-mutation tag mirroring `DIDContract.SetMutation`.
///
/// Selects add-vs-remove semantics for `setVerificationMethodRelation` and
/// `setAlsoKnownAs` circuits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SetMutation {
    /// Add the element to the set.
    Insert,
    /// Remove the element from the set.
    Remove,
}

/// Verification-method relation tag matching the on-chain enum.
///
/// Mirrors `DIDContract.VerificationMethodRelation`. The order of the
/// variants matches the ledger encoding so future byte-parity work can keep
/// the discriminants aligned with the TS source.
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

/// Ledger-shaped public-key JWK. The contract stores the full set of JOSE
/// fields as raw `Opaque<"string">` cells; the API maps domain JWKs into
/// this form before invoking a circuit.
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

/// Ledger-shaped verification method (corresponding to
/// `DIDContract.VerificationMethod`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerVerificationMethod {
    /// Fragment-id form of the verification method id (e.g. `#key-1`).
    pub id: String,
    /// Verification method type discriminant.
    pub typ: VerificationMethodType,
    /// Ledger-shaped public-key JWK.
    pub public_key_jwk: LedgerPublicKeyJwk,
}

/// Ledger-shaped Schnorr-Jubjub verification method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerSchnorrJubjubVerificationMethod {
    /// Fragment-id form of the verification method id.
    pub id: String,
    /// Jubjub point coordinates as little-endian-base16 hex strings (so the
    /// API layer does not need to depend on a big-integer crate).
    ///
    /// A real implementation will encode these into `JubjubPoint` field
    /// elements when invoking the circuit.
    pub public_key: JubjubPointHex,
}

/// Pair-of-coordinate Jubjub point in hex form. Mirrors
/// `JubjubPoint { x: bigint, y: bigint }` from the runtime; using hex keeps
/// the API layer free of a bigint dependency. The contract impl performs the
/// decode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JubjubPointHex {
    /// X coordinate as hex.
    pub x: String,
    /// Y coordinate as hex.
    pub y: String,
}

/// Ledger-shaped service entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerService {
    /// Fragment-id form of the service id.
    pub id: String,
    /// Service `type` (single string or JSON-array form).
    pub typ: String,
    /// Service endpoint encoded as the canonical JSON string the contract
    /// stores.
    pub service_endpoint: String,
}

/// Schnorr-Jubjub digest argument to
/// [`DidContract::verify_schnorr_jubjub_digest_signature`].
pub type SchnorrJubjubDigest = [String; 4];

/// Schnorr signature payload — kept opaque so the API layer can carry it
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
/// API layer can be compiled without `midnight-onchain-runtime`.
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
    /// `alsoKnownAs` set members (lexicographic order, matching the TS
    /// `Set.values()` enumeration).
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

/// Result of a finalized transaction. The TS API returns a richer
/// `FinalizedTxData`; for the Rust port we currently expose the hash + block
/// height the operation committed at. Implementations may extend this struct
/// (or wrap it) without breaking API consumers since it is owned.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalizedTxData {
    /// Transaction hash (hex). Empty in mock implementations.
    pub tx_hash: String,
    /// Block height the transaction was included in. Zero in mocks.
    pub block_height: u64,
}

/// Abstraction over the on-chain Midnight DID contract.
///
/// In production a single impl wraps a `compact-runtime` + provider stack to
/// actually invoke the generated impure circuits. For unit-testing the API
/// layer we use a recording mock (see [`mock::RecordingContract`]).
///
/// Every method corresponds 1:1 to an exported impure circuit defined in
/// `did.compact`; method names use snake_case.
#[async_trait]
pub trait DidContract: Send + Sync {
    /// Contract address (`0x…`) — used by [`crate::did_subject`] to build the
    /// DID subject string.
    fn contract_address(&self) -> String;

    /// Network the contract is deployed on.
    fn network(&self) -> midnight_did_method::midnight_did::MidnightNetwork;

    /// Read the public ledger state.
    async fn read_ledger(&self) -> Result<DidLedgerSnapshot, ContractError>;

    /// `rotateControllerKey(new_pk)` — new controller public key, 32 bytes.
    async fn rotate_controller_key(
        &self,
        new_controller_public_key: [u8; 32],
    ) -> Result<FinalizedTxData, ContractError>;

    /// `setVerificationMethod(vm, mutation)`.
    async fn set_verification_method(
        &self,
        method: LedgerVerificationMethod,
        mutation: MapMutation,
    ) -> Result<FinalizedTxData, ContractError>;

    /// `removeVerificationMethod(methodId)`.
    async fn remove_verification_method(&self, normalized_method_id: String) -> Result<FinalizedTxData, ContractError>;

    /// `setSchnorrJubjubVerificationMethod(vm, mutation)`.
    async fn set_schnorr_jubjub_verification_method(
        &self,
        method: LedgerSchnorrJubjubVerificationMethod,
        mutation: MapMutation,
    ) -> Result<FinalizedTxData, ContractError>;

    /// `removeSchnorrJubjubVerificationMethod(methodId)`.
    async fn remove_schnorr_jubjub_verification_method(
        &self,
        normalized_method_id: String,
    ) -> Result<FinalizedTxData, ContractError>;

    /// `verifySchnorrJubjubDigestSignature(methodId, digest, signature)`.
    async fn verify_schnorr_jubjub_digest_signature(
        &self,
        normalized_method_id: String,
        digest: SchnorrJubjubDigest,
        signature: SchnorrJubjubSignature,
    ) -> Result<FinalizedTxData, ContractError>;

    /// `setVerificationMethodRelation(relation, methodId, mutation)`.
    async fn set_verification_method_relation(
        &self,
        relation: LedgerVerificationMethodRelation,
        normalized_method_id: String,
        mutation: SetMutation,
    ) -> Result<FinalizedTxData, ContractError>;

    /// `setService(service, mutation)`.
    async fn set_service(
        &self,
        service: LedgerService,
        mutation: MapMutation,
    ) -> Result<FinalizedTxData, ContractError>;

    /// `removeService(serviceId)`.
    async fn remove_service(&self, normalized_service_id: String) -> Result<FinalizedTxData, ContractError>;

    /// `setAlsoKnownAs(alias, mutation)`.
    async fn set_also_known_as(
        &self,
        alias_uri: String,
        mutation: SetMutation,
    ) -> Result<FinalizedTxData, ContractError>;

    /// `deactivate()`.
    async fn deactivate(&self) -> Result<FinalizedTxData, ContractError>;
}

/// Recording mock implementation used by API-layer tests.
///
/// The mock captures the sequence of circuit invocations as
/// [`RecordedCall`] entries; tests assert against the recorded sequence.
pub mod mock {
    use super::*;
    use std::sync::Mutex;

    /// Single recorded invocation. One variant per [`DidContract`] method.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum RecordedCall {
        /// `read_ledger()` invocation (no args).
        ReadLedger,
        /// `rotate_controller_key(new_pk)`.
        RotateControllerKey([u8; 32]),
        /// `set_verification_method(vm, mutation)`.
        SetVerificationMethod(LedgerVerificationMethod, MapMutation),
        /// `remove_verification_method(id)`.
        RemoveVerificationMethod(String),
        /// `set_schnorr_jubjub_verification_method(vm, mutation)`.
        SetSchnorrJubjubVerificationMethod(LedgerSchnorrJubjubVerificationMethod, MapMutation),
        /// `remove_schnorr_jubjub_verification_method(id)`.
        RemoveSchnorrJubjubVerificationMethod(String),
        /// `verify_schnorr_jubjub_digest_signature(id, digest, signature)`.
        VerifySchnorrJubjubDigestSignature(String, SchnorrJubjubDigest, SchnorrJubjubSignature),
        /// `set_verification_method_relation(relation, id, mutation)`.
        SetVerificationMethodRelation(LedgerVerificationMethodRelation, String, SetMutation),
        /// `set_service(service, mutation)`.
        SetService(LedgerService, MapMutation),
        /// `remove_service(id)`.
        RemoveService(String),
        /// `set_also_known_as(uri, mutation)`.
        SetAlsoKnownAs(String, SetMutation),
        /// `deactivate()`.
        Deactivate,
    }

    /// In-memory recording contract.
    ///
    /// Pass an initial [`DidLedgerSnapshot`] to seed the value returned by
    /// [`DidContract::read_ledger`]. Mutate operations are recorded but do
    /// **not** modify the stored snapshot; tests that need stateful
    /// behaviour can call [`RecordingContract::set_ledger`] between
    /// operations.
    #[derive(Debug)]
    pub struct RecordingContract {
        address: String,
        network: midnight_did_method::midnight_did::MidnightNetwork,
        ledger: Mutex<DidLedgerSnapshot>,
        calls: Mutex<Vec<RecordedCall>>,
    }

    impl RecordingContract {
        /// Build a new recording contract with empty initial ledger state.
        pub fn new(address: impl Into<String>, network: midnight_did_method::midnight_did::MidnightNetwork) -> Self {
            Self::with_ledger(address, network, DidLedgerSnapshot::default())
        }

        /// Build a recording contract seeded with a specific ledger snapshot.
        pub fn with_ledger(
            address: impl Into<String>,
            network: midnight_did_method::midnight_did::MidnightNetwork,
            ledger: DidLedgerSnapshot,
        ) -> Self {
            Self {
                address: address.into(),
                network,
                ledger: Mutex::new(ledger),
                calls: Mutex::new(Vec::new()),
            }
        }

        /// Replace the ledger snapshot returned by [`Self::read_ledger`].
        pub fn set_ledger(&self, ledger: DidLedgerSnapshot) {
            *self.ledger.lock().unwrap() = ledger;
        }

        /// Return a snapshot of all recorded calls (in invocation order).
        pub fn calls(&self) -> Vec<RecordedCall> {
            self.calls.lock().unwrap().clone()
        }

        fn record(&self, call: RecordedCall) {
            self.calls.lock().unwrap().push(call);
        }
    }

    #[async_trait]
    impl DidContract for RecordingContract {
        fn contract_address(&self) -> String {
            self.address.clone()
        }

        fn network(&self) -> midnight_did_method::midnight_did::MidnightNetwork {
            self.network
        }

        async fn read_ledger(&self) -> Result<DidLedgerSnapshot, ContractError> {
            self.record(RecordedCall::ReadLedger);
            Ok(self.ledger.lock().unwrap().clone())
        }

        async fn rotate_controller_key(&self, new_pk: [u8; 32]) -> Result<FinalizedTxData, ContractError> {
            self.record(RecordedCall::RotateControllerKey(new_pk));
            Ok(FinalizedTxData::default())
        }

        async fn set_verification_method(
            &self,
            method: LedgerVerificationMethod,
            mutation: MapMutation,
        ) -> Result<FinalizedTxData, ContractError> {
            self.record(RecordedCall::SetVerificationMethod(method, mutation));
            Ok(FinalizedTxData::default())
        }

        async fn remove_verification_method(&self, id: String) -> Result<FinalizedTxData, ContractError> {
            self.record(RecordedCall::RemoveVerificationMethod(id));
            Ok(FinalizedTxData::default())
        }

        async fn set_schnorr_jubjub_verification_method(
            &self,
            method: LedgerSchnorrJubjubVerificationMethod,
            mutation: MapMutation,
        ) -> Result<FinalizedTxData, ContractError> {
            self.record(RecordedCall::SetSchnorrJubjubVerificationMethod(method, mutation));
            Ok(FinalizedTxData::default())
        }

        async fn remove_schnorr_jubjub_verification_method(
            &self,
            id: String,
        ) -> Result<FinalizedTxData, ContractError> {
            self.record(RecordedCall::RemoveSchnorrJubjubVerificationMethod(id));
            Ok(FinalizedTxData::default())
        }

        async fn verify_schnorr_jubjub_digest_signature(
            &self,
            id: String,
            digest: SchnorrJubjubDigest,
            signature: SchnorrJubjubSignature,
        ) -> Result<FinalizedTxData, ContractError> {
            self.record(RecordedCall::VerifySchnorrJubjubDigestSignature(id, digest, signature));
            Ok(FinalizedTxData::default())
        }

        async fn set_verification_method_relation(
            &self,
            relation: LedgerVerificationMethodRelation,
            id: String,
            mutation: SetMutation,
        ) -> Result<FinalizedTxData, ContractError> {
            self.record(RecordedCall::SetVerificationMethodRelation(relation, id, mutation));
            Ok(FinalizedTxData::default())
        }

        async fn set_service(
            &self,
            service: LedgerService,
            mutation: MapMutation,
        ) -> Result<FinalizedTxData, ContractError> {
            self.record(RecordedCall::SetService(service, mutation));
            Ok(FinalizedTxData::default())
        }

        async fn remove_service(&self, id: String) -> Result<FinalizedTxData, ContractError> {
            self.record(RecordedCall::RemoveService(id));
            Ok(FinalizedTxData::default())
        }

        async fn set_also_known_as(
            &self,
            alias: String,
            mutation: SetMutation,
        ) -> Result<FinalizedTxData, ContractError> {
            self.record(RecordedCall::SetAlsoKnownAs(alias, mutation));
            Ok(FinalizedTxData::default())
        }

        async fn deactivate(&self) -> Result<FinalizedTxData, ContractError> {
            self.record(RecordedCall::Deactivate);
            Ok(FinalizedTxData::default())
        }
    }
}
