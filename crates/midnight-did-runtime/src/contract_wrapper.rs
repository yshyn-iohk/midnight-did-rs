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

//! `Contract<B>` — typed wrapper that mediates between the api-layer
//! operation builders and a [`Backend`] implementation.
//!
//! Path 2 (ADR 0008, v0.4.0): every mutating circuit call builds a
//! [`DidContractCall`], serialises it into [`BuiltTx::bytes`], and
//! forwards the envelope to [`Backend::submit_tx`]. The
//! `generated::Contract<PS, W>` shape stays untouched — it is the
//! eventual destination of the envelope once the wallet/proof bridge
//! lands, but is not on the v0.4.0 hot path.
//!
//! The `read_snapshot` method goes through [`Backend::read_snapshot`],
//! which returns a plain-data [`DidLedgerSnapshot`] without depending
//! on the codegen'd `Ledger` types.

use midnight_did_method::midnight_did::{ContractAddress, MidnightNetwork};

use crate::backend::{Backend, BackendError, BuiltTx, FinalizedTxData};
use crate::contract_call::{
    DidContractCall, DidLedgerSnapshot, LedgerSchnorrJubjubVerificationMethod, LedgerService,
    LedgerVerificationMethod, LedgerVerificationMethodRelation, MapMutation, SchnorrJubjubDigest,
    SchnorrJubjubSignature, SetMutation,
};
use midnight_did_method::hex_ext::HashOutputExt;

/// Concrete typed contract wrapper over a [`Backend`].
///
/// Replaces the `DidContract` trait that lived in `midnight-did-api`
/// pre-v0.4.0. Every method composes:
///
/// 1. Build the matching [`DidContractCall`] variant from the typed args.
/// 2. Encode it via [`DidContractCall::encode`].
/// 3. Forward the resulting [`BuiltTx`] to [`Backend::submit_tx`].
///
/// The `read_snapshot` method short-circuits the encode/decode step and
/// goes straight through [`Backend::read_snapshot`] for the read path —
/// reads do not produce a transaction.
///
/// `address` and `network` are stored by value so `subject` / DID-string
/// helpers in the api crate can read them without an `async` hop.
pub struct Contract<B: Backend> {
    /// I/O substrate. Owned so the wrapper has the lifetime profile of a
    /// regular value rather than a borrowed handle.
    pub backend: B,
    /// On-chain contract address (also rendered into the DID subject).
    pub address: ContractAddress,
    /// Network the contract is deployed on (drives the `did:midnight:net:…`
    /// rendering).
    pub network: MidnightNetwork,
}

impl<B: Backend> Contract<B> {
    /// Build a new [`Contract<B>`] from a backend, address, and network.
    pub fn new(backend: B, address: ContractAddress, network: MidnightNetwork) -> Self {
        Self {
            backend,
            address,
            network,
        }
    }

    /// Lowercase-hex 64-char rendering of the on-chain address.
    pub fn contract_address(&self) -> String {
        self.address.to_hex()
    }

    /// Network this contract is deployed on.
    pub fn network(&self) -> MidnightNetwork {
        self.network
    }

    /// Read the public ledger snapshot via the backend.
    ///
    /// Goes through [`Backend::read_snapshot`] rather than the submit path —
    /// reads do not produce a transaction.
    pub async fn read_snapshot(&self) -> Result<DidLedgerSnapshot, BackendError> {
        self.backend.read_snapshot().await
    }

    /// `rotateControllerKey(new_pk)` — new controller public key, 32 bytes.
    pub async fn rotate_controller_key(
        &self,
        new_controller_public_key: [u8; 32],
    ) -> Result<FinalizedTxData, BackendError> {
        self.submit(DidContractCall::RotateControllerKey {
            new_public_key: new_controller_public_key,
        })
        .await
    }

    /// `setVerificationMethod(method, mutation)`.
    pub async fn set_verification_method(
        &self,
        method: LedgerVerificationMethod,
        mutation: MapMutation,
    ) -> Result<FinalizedTxData, BackendError> {
        self.submit(DidContractCall::SetVerificationMethod { method, mutation }).await
    }

    /// `removeVerificationMethod(methodId)`.
    pub async fn remove_verification_method(
        &self,
        normalized_method_id: String,
    ) -> Result<FinalizedTxData, BackendError> {
        self.submit(DidContractCall::RemoveVerificationMethod {
            method_id: normalized_method_id,
        })
        .await
    }

    /// `setSchnorrJubjubVerificationMethod(method, mutation)`.
    pub async fn set_schnorr_jubjub_verification_method(
        &self,
        method: LedgerSchnorrJubjubVerificationMethod,
        mutation: MapMutation,
    ) -> Result<FinalizedTxData, BackendError> {
        self.submit(DidContractCall::SetSchnorrJubjubVerificationMethod { method, mutation })
            .await
    }

    /// `removeSchnorrJubjubVerificationMethod(methodId)`.
    pub async fn remove_schnorr_jubjub_verification_method(
        &self,
        normalized_method_id: String,
    ) -> Result<FinalizedTxData, BackendError> {
        self.submit(DidContractCall::RemoveSchnorrJubjubVerificationMethod {
            method_id: normalized_method_id,
        })
        .await
    }

    /// `verifySchnorrJubjubDigestSignature(methodId, digest, signature)`.
    pub async fn verify_schnorr_jubjub_digest_signature(
        &self,
        normalized_method_id: String,
        digest: SchnorrJubjubDigest,
        signature: SchnorrJubjubSignature,
    ) -> Result<FinalizedTxData, BackendError> {
        self.submit(DidContractCall::VerifySchnorrJubjubDigestSignature {
            method_id: normalized_method_id,
            digest,
            signature,
        })
        .await
    }

    /// `setVerificationMethodRelation(relation, methodId, mutation)`.
    pub async fn set_verification_method_relation(
        &self,
        relation: LedgerVerificationMethodRelation,
        normalized_method_id: String,
        mutation: SetMutation,
    ) -> Result<FinalizedTxData, BackendError> {
        self.submit(DidContractCall::SetVerificationMethodRelation {
            relation,
            method_id: normalized_method_id,
            mutation,
        })
        .await
    }

    /// `setService(service, mutation)`.
    pub async fn set_service(
        &self,
        service: LedgerService,
        mutation: MapMutation,
    ) -> Result<FinalizedTxData, BackendError> {
        self.submit(DidContractCall::SetService { service, mutation }).await
    }

    /// `removeService(serviceId)`.
    pub async fn remove_service(&self, normalized_service_id: String) -> Result<FinalizedTxData, BackendError> {
        self.submit(DidContractCall::RemoveService {
            service_id: normalized_service_id,
        })
        .await
    }

    /// `setAlsoKnownAs(aliasUri, mutation)`.
    pub async fn set_also_known_as(
        &self,
        alias_uri: String,
        mutation: SetMutation,
    ) -> Result<FinalizedTxData, BackendError> {
        self.submit(DidContractCall::SetAlsoKnownAs { alias_uri, mutation }).await
    }

    /// `deactivate()`.
    pub async fn deactivate(&self) -> Result<FinalizedTxData, BackendError> {
        self.submit(DidContractCall::Deactivate).await
    }

    /// Encode + submit helper shared by every mutating circuit.
    async fn submit(&self, call: DidContractCall) -> Result<FinalizedTxData, BackendError> {
        let tx = BuiltTx { bytes: call.encode() };
        self.backend.submit_tx(tx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::RecordingBackend;

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    fn addr() -> ContractAddress {
        midnight_did_method::midnight_did::parse_contract_address(
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        )
        .unwrap()
    }

    #[test]
    fn contract_records_typed_calls_through_recording_backend() {
        let rt = rt();
        let contract = Contract::new(RecordingBackend::new(), addr(), MidnightNetwork::Undeployed);
        rt.block_on(contract.deactivate()).unwrap();
        rt.block_on(contract.rotate_controller_key([9u8; 32])).unwrap();
        let recorded = contract.backend.recorded_calls();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0], DidContractCall::Deactivate);
        assert_eq!(
            recorded[1],
            DidContractCall::RotateControllerKey {
                new_public_key: [9u8; 32]
            }
        );
    }

    #[test]
    fn contract_address_and_network_accessors() {
        let contract = Contract::new(RecordingBackend::new(), addr(), MidnightNetwork::Testnet);
        assert_eq!(contract.network(), MidnightNetwork::Testnet);
        assert_eq!(
            contract.contract_address(),
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
        );
    }

    #[test]
    fn read_snapshot_returns_seeded_value() {
        let rt = rt();
        let mut snap = DidLedgerSnapshot::default();
        snap.version = 42;
        let backend = RecordingBackend::with_snapshot(snap.clone());
        let contract = Contract::new(backend, addr(), MidnightNetwork::Undeployed);
        let got = rt.block_on(contract.read_snapshot()).unwrap();
        assert_eq!(got, snap);
        // read_snapshot also records a synthetic ReadLedger call.
        let recorded = contract.backend.recorded_calls();
        assert_eq!(recorded, vec![DidContractCall::ReadLedger]);
    }
}
