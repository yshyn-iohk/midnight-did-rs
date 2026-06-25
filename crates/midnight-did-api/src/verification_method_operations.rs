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

//! Verification-method operations + relation membership helpers.
//!
//! Rust port of `packages/api/src/verification-method-operations.ts` and
//! `packages/api/src/verification-method-relations.ts`. The TS code spans
//! both files; we merge them here because the operations and the relation
//! helpers are tightly coupled (relation purge runs as part of remove).

use midnight_did_domain::{
    did_document::{VerificationMethod, VerificationMethodRelation},
    ledger_utils::BoundIdField,
};
use midnight_did_runtime::{Backend, Contract};

use crate::{
    contract::{
        DidLedgerSnapshot, FinalizedTxData, MapMutation, SchnorrJubjubDigest, SchnorrJubjubSignature, SetMutation,
    },
    error::{ApiError, ContractError},
    ledger_mappers::{
        SchnorrJubjubVerificationMethod, ledger_verification_method_relation_for,
        schnorr_jubjub_verification_method_to_ledger, verification_method_to_ledger,
    },
    subject::normalize_bound_fragment_id_for,
};

/// Canonical ordering of verification-method relations used by purge logic
/// (`VerificationMethodRelations` in the TS source).
pub const VERIFICATION_METHOD_RELATIONS: [VerificationMethodRelation; 5] = [
    VerificationMethodRelation::Authentication,
    VerificationMethodRelation::AssertionMethod,
    VerificationMethodRelation::KeyAgreement,
    VerificationMethodRelation::CapabilityInvocation,
    VerificationMethodRelation::CapabilityDelegation,
];

/// Membership entry mirroring `VerificationMethodRelationMembership` in TS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationMethodRelationMembership {
    /// Relation tag.
    pub relation: VerificationMethodRelation,
    /// Whether `normalized_method_id` is currently in the relation.
    pub member: bool,
}

/// Compute membership of `normalized_method_id` across every relation in
/// [`VERIFICATION_METHOD_RELATIONS`].
pub fn verification_method_relation_memberships(
    state: &DidLedgerSnapshot,
    normalized_method_id: &str,
) -> Vec<VerificationMethodRelationMembership> {
    VERIFICATION_METHOD_RELATIONS
        .iter()
        .copied()
        .map(|relation| {
            let ledger_relation = ledger_verification_method_relation_for(relation);
            let member = state.relation_contains(ledger_relation, normalized_method_id);
            VerificationMethodRelationMembership { relation, member }
        })
        .collect()
}

/// `assertVerificationMethodRelationAbsent` — error if the relation already
/// contains the method.
pub fn assert_verification_method_relation_absent(
    state: &DidLedgerSnapshot,
    relation: VerificationMethodRelation,
    normalized_method_id: &str,
) -> Result<(), ApiError> {
    let ledger_relation = ledger_verification_method_relation_for(relation);
    if state.relation_contains(ledger_relation, normalized_method_id) {
        Err(ApiError::Verification(crate::error::VerificationError::RelationAlreadyContains {
            relation: format!("{relation:?}"),
            method_id: normalized_method_id.to_owned(),
        }))
    } else {
        Ok(())
    }
}

/// `assertVerificationMethodRelationPresent` — error if the relation does
/// not contain the method.
pub fn assert_verification_method_relation_present(
    state: &DidLedgerSnapshot,
    relation: VerificationMethodRelation,
    normalized_method_id: &str,
) -> Result<(), ApiError> {
    let ledger_relation = ledger_verification_method_relation_for(relation);
    if state.relation_contains(ledger_relation, normalized_method_id) {
        Ok(())
    } else {
        Err(ApiError::Verification(crate::error::VerificationError::RelationMissing {
            relation: format!("{relation:?}"),
            method_id: normalized_method_id.to_owned(),
        }))
    }
}

/// Wrap a [`midnight_did_runtime::BackendError`] from a contract-call into
/// the api-level [`ApiError::Contract(ContractError::Failed)`] shape so
/// callers keep the same error category they had pre-R2-2.
fn map_backend_err(err: midnight_did_runtime::BackendError) -> ApiError {
    ApiError::Contract(ContractError::Failed(err.to_string()))
}

/// Remove `normalized_method_id` from every relation it currently belongs to.
/// Mirrors `removePresentVerificationMethodRelations`.
pub async fn remove_present_verification_method_relations<B: Backend>(
    contract: &Contract<B>,
    memberships: &[VerificationMethodRelationMembership],
    normalized_method_id: &str,
) -> Result<(), ApiError> {
    for entry in memberships.iter() {
        if !entry.member {
            continue;
        }
        let ledger_relation = ledger_verification_method_relation_for(entry.relation);
        contract
            .set_verification_method_relation(ledger_relation, normalized_method_id.to_owned(), SetMutation::Remove)
            .await
            .map_err(map_backend_err)?;
    }
    Ok(())
}

/// `purgeVerificationMethodFromAllRelations` — read fresh ledger state and
/// remove any relation memberships that reference `normalized_method_id`.
pub async fn purge_verification_method_from_all_relations<B: Backend>(
    contract: &Contract<B>,
    normalized_method_id: &str,
) -> Result<(), ApiError> {
    let state = contract.read_snapshot().await.map_err(map_backend_err)?;
    let memberships = verification_method_relation_memberships(&state, normalized_method_id);
    remove_present_verification_method_relations(contract, &memberships, normalized_method_id).await
}

/// `addVerificationMethod`.
pub async fn add_verification_method<B: Backend>(
    contract: &Contract<B>,
    verification_method: &VerificationMethod,
) -> Result<FinalizedTxData, ApiError> {
    let ledger = verification_method_to_ledger(contract, verification_method)?;
    contract
        .set_verification_method(ledger, MapMutation::Insert)
        .await
        .map_err(map_backend_err)
}

/// `updateVerificationMethod`.
pub async fn update_verification_method<B: Backend>(
    contract: &Contract<B>,
    verification_method: &VerificationMethod,
) -> Result<FinalizedTxData, ApiError> {
    let ledger = verification_method_to_ledger(contract, verification_method)?;
    contract
        .set_verification_method(ledger, MapMutation::Update)
        .await
        .map_err(map_backend_err)
}

/// `removeVerificationMethod` — purges relation memberships then removes
/// the method.
pub async fn remove_verification_method<B: Backend>(
    contract: &Contract<B>,
    method_id: &str,
) -> Result<FinalizedTxData, ApiError> {
    let normalized = normalize_bound_fragment_id_for(contract, method_id, BoundIdField::MethodId)?;
    purge_verification_method_from_all_relations(contract, &normalized).await?;
    contract
        .remove_verification_method(normalized)
        .await
        .map_err(map_backend_err)
}

/// `addSchnorrJubjubVerificationMethod`.
pub async fn add_schnorr_jubjub_verification_method<B: Backend>(
    contract: &Contract<B>,
    verification_method: &SchnorrJubjubVerificationMethod,
) -> Result<FinalizedTxData, ApiError> {
    let ledger = schnorr_jubjub_verification_method_to_ledger(contract, verification_method)?;
    contract
        .set_schnorr_jubjub_verification_method(ledger, MapMutation::Insert)
        .await
        .map_err(map_backend_err)
}

/// `updateSchnorrJubjubVerificationMethod`.
pub async fn update_schnorr_jubjub_verification_method<B: Backend>(
    contract: &Contract<B>,
    verification_method: &SchnorrJubjubVerificationMethod,
) -> Result<FinalizedTxData, ApiError> {
    let ledger = schnorr_jubjub_verification_method_to_ledger(contract, verification_method)?;
    contract
        .set_schnorr_jubjub_verification_method(ledger, MapMutation::Update)
        .await
        .map_err(map_backend_err)
}

/// `removeSchnorrJubjubVerificationMethod` — purges relation memberships
/// then removes the method.
pub async fn remove_schnorr_jubjub_verification_method<B: Backend>(
    contract: &Contract<B>,
    method_id: &str,
) -> Result<FinalizedTxData, ApiError> {
    let normalized = normalize_bound_fragment_id_for(contract, method_id, BoundIdField::MethodId)?;
    purge_verification_method_from_all_relations(contract, &normalized).await?;
    contract
        .remove_schnorr_jubjub_verification_method(normalized)
        .await
        .map_err(map_backend_err)
}

/// `verifySchnorrJubjubDigestSignature`.
pub async fn verify_schnorr_jubjub_digest_signature<B: Backend>(
    contract: &Contract<B>,
    method_id: &str,
    digest: SchnorrJubjubDigest,
    signature: SchnorrJubjubSignature,
) -> Result<FinalizedTxData, ApiError> {
    let normalized = normalize_bound_fragment_id_for(contract, method_id, BoundIdField::MethodId)?;
    contract
        .verify_schnorr_jubjub_digest_signature(normalized, digest, signature)
        .await
        .map_err(map_backend_err)
}

/// `addVerificationMethodRelation`.
pub async fn add_verification_method_relation<B: Backend>(
    contract: &Contract<B>,
    relation: VerificationMethodRelation,
    method_id: &str,
) -> Result<FinalizedTxData, ApiError> {
    let normalized = normalize_bound_fragment_id_for(contract, method_id, BoundIdField::MethodId)?;
    let state = contract.read_snapshot().await.map_err(map_backend_err)?;
    assert_verification_method_relation_absent(&state, relation, &normalized)?;
    let ledger_relation = ledger_verification_method_relation_for(relation);
    contract
        .set_verification_method_relation(ledger_relation, normalized, SetMutation::Insert)
        .await
        .map_err(map_backend_err)
}

/// `removeVerificationMethodRelation`.
pub async fn remove_verification_method_relation<B: Backend>(
    contract: &Contract<B>,
    relation: VerificationMethodRelation,
    method_id: &str,
) -> Result<FinalizedTxData, ApiError> {
    let normalized = normalize_bound_fragment_id_for(contract, method_id, BoundIdField::MethodId)?;
    let state = contract.read_snapshot().await.map_err(map_backend_err)?;
    assert_verification_method_relation_present(&state, relation, &normalized)?;
    let ledger_relation = ledger_verification_method_relation_for(relation);
    contract
        .set_verification_method_relation(ledger_relation, normalized, SetMutation::Remove)
        .await
        .map_err(map_backend_err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{DidLedgerSnapshot, LedgerVerificationMethodRelation};
    use midnight_did_domain::{
        crypto_codecs::encode_base64url,
        did_document::{
            CurveType, KeyType, NewPublicKeyJwk, NewVerificationMethod, PublicKeyJwk, VerificationMethodType,
        },
    };
    use midnight_did_method::midnight_did::{MidnightNetwork, parse_contract_address};
    use midnight_did_runtime::{DidContractCall, RecordingBackend};
    use std::collections::BTreeMap;

    const ADDR: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    fn did_subject() -> String {
        format!("did:midnight:testnet:{ADDR}")
    }

    fn test_contract() -> Contract<RecordingBackend> {
        Contract::new(
            RecordingBackend::new(),
            parse_contract_address(ADDR).unwrap(),
            MidnightNetwork::Testnet,
        )
    }

    fn test_contract_with(snapshot: DidLedgerSnapshot) -> Contract<RecordingBackend> {
        Contract::new(
            RecordingBackend::with_snapshot(snapshot),
            parse_contract_address(ADDR).unwrap(),
            MidnightNetwork::Testnet,
        )
    }

    fn p256_vm(id: &str) -> VerificationMethod {
        let coord = encode_base64url(&[0u8; 32]);
        let jwk = PublicKeyJwk::new(NewPublicKeyJwk {
            kty: KeyType::EC,
            crv: CurveType::P256,
            x: coord.clone(),
            y: Some(coord),
            extensions: BTreeMap::new(),
        })
        .expect("valid P-256 JWK fixture");
        VerificationMethod::new(NewVerificationMethod {
            id: format!("{}#{}", did_subject(), id),
            type_: VerificationMethodType::JsonWebKey,
            controller: did_subject(),
            public_key_jwk: jwk,
        })
        .expect("valid VM fixture")
    }

    #[tokio::test]
    async fn add_verification_method_records_insert() {
        let contract = test_contract();
        let vm = p256_vm("key-1");
        add_verification_method(&contract, &vm).await.unwrap();
        let calls = contract.backend.recorded_calls();
        assert!(matches!(
            &calls[..],
            [DidContractCall::SetVerificationMethod { method, mutation: MapMutation::Insert }]
                if method.id == "#key-1"
        ));
    }

    #[tokio::test]
    async fn update_verification_method_records_update() {
        let contract = test_contract();
        update_verification_method(&contract, &p256_vm("key-1")).await.unwrap();
        let calls = contract.backend.recorded_calls();
        assert!(matches!(
            &calls[..],
            [DidContractCall::SetVerificationMethod { mutation: MapMutation::Update, .. }]
        ));
    }

    #[tokio::test]
    async fn remove_verification_method_purges_relations() {
        let mut ledger = DidLedgerSnapshot::default();
        ledger.authentication_relation.push("#key-1".into());
        ledger.assertion_method_relation.push("#key-1".into());
        let contract = test_contract_with(ledger);

        remove_verification_method(&contract, "key-1").await.unwrap();
        let calls = contract.backend.recorded_calls();
        // read_snapshot + 2 remove-relation + remove vm.
        assert_eq!(calls.len(), 4);
        assert!(matches!(calls[0], DidContractCall::ReadLedger));
        assert!(matches!(
            &calls[1],
            DidContractCall::SetVerificationMethodRelation {
                relation: LedgerVerificationMethodRelation::Authentication,
                method_id,
                mutation: SetMutation::Remove,
            } if method_id == "#key-1"
        ));
        assert!(matches!(
            &calls[2],
            DidContractCall::SetVerificationMethodRelation {
                relation: LedgerVerificationMethodRelation::AssertionMethod,
                method_id,
                mutation: SetMutation::Remove,
            } if method_id == "#key-1"
        ));
        assert!(matches!(&calls[3], DidContractCall::RemoveVerificationMethod { method_id } if method_id == "#key-1"));
    }

    #[tokio::test]
    async fn add_relation_rejects_already_present() {
        let mut ledger = DidLedgerSnapshot::default();
        ledger.authentication_relation.push("#key-1".into());
        let contract = test_contract_with(ledger);
        let err = add_verification_method_relation(&contract, VerificationMethodRelation::Authentication, "key-1")
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::Verification(crate::error::VerificationError::RelationAlreadyContains { .. })));
    }

    #[tokio::test]
    async fn remove_relation_rejects_when_missing() {
        let contract = test_contract();
        let err = remove_verification_method_relation(&contract, VerificationMethodRelation::KeyAgreement, "key-1")
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::Verification(crate::error::VerificationError::RelationMissing { .. })));
    }
}
