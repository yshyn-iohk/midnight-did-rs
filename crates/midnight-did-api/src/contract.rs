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

//! Contract-surface re-exports + legacy trait shim.
//!
//! ## v0.4.0 — Path 2 contract abstraction reform
//!
//! See `doc/adr/0008-contract-abstraction-reform.md`.
//!
//! The ledger-shape value types (`MapMutation`, `SetMutation`,
//! [`LedgerVerificationMethod`], [`LedgerSchnorrJubjubVerificationMethod`],
//! [`LedgerService`], [`LedgerPublicKeyJwk`], [`JubjubPointHex`],
//! [`SchnorrJubjubSignature`], [`SchnorrJubjubDigest`],
//! [`LedgerVerificationMethodRelation`], [`DidLedgerSnapshot`]) and
//! [`FinalizedTxData`] are owned by `midnight_did_runtime` so the
//! [`midnight_did_runtime::Contract`] wrapper can carry them as
//! [`midnight_did_runtime::DidContractCall`] payloads. This module
//! re-exports them so api consumers do not need to add an explicit
//! `midnight-did-runtime` dep to their `Cargo.toml`.
//!
//! [`DidContract`] + [`mock::RecordingContract`] are retained in this commit
//! purely to keep the api crate's surface stable across the R2-2 migration
//! commits; both are deleted in R2-2.4 once the operation builders + tests
//! have moved to `Contract<B: Backend>`.

pub use midnight_did_runtime::{
    DidLedgerSnapshot, FinalizedTxData, JubjubPointHex, LedgerPublicKeyJwk, LedgerSchnorrJubjubVerificationMethod,
    LedgerService, LedgerVerificationMethod, LedgerVerificationMethodRelation, MapMutation, SchnorrJubjubDigest,
    SchnorrJubjubSignature, SetMutation,
};

use async_trait::async_trait;

use crate::error::ContractError;

/// Legacy `DidContract` trait — retained pending R2-2.4 deletion.
///
/// New code should accept `&midnight_did_runtime::Contract<B>` directly.
/// This trait + [`mock::RecordingContract`] only exist to keep the
/// intermediate R2-2 commits buildable; both are removed in R2-2.4.
#[async_trait]
pub trait DidContract: Send + Sync {
    /// Contract address (`0x…`).
    fn contract_address(&self) -> String;

    /// Network the contract is deployed on.
    fn network(&self) -> midnight_did_method::midnight_did::MidnightNetwork;

    /// Read the public ledger state.
    async fn read_ledger(&self) -> Result<DidLedgerSnapshot, ContractError>;

    /// `rotateControllerKey(new_pk)`.
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

/// Legacy recording mock — retained pending R2-2.4 deletion.
///
/// The operation-builder migration in R2-2.2/.3 moves to
/// `midnight_did_runtime::RecordingBackend` +
/// `midnight_did_runtime::Contract<RecordingBackend>`. This shim only
/// keeps the trait + its mock impl alive for the intermediate commits.
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
