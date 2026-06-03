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

use crate::{
    contract::{
        DidContract, DidLedgerSnapshot, FinalizedTxData, MapMutation, SchnorrJubjubDigest, SchnorrJubjubSignature,
        SetMutation,
    },
    error::ApiError,
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
        Err(ApiError::RelationAlreadyContains {
            relation: format!("{relation:?}"),
            method_id: normalized_method_id.to_owned(),
        })
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
        Err(ApiError::RelationMissing {
            relation: format!("{relation:?}"),
            method_id: normalized_method_id.to_owned(),
        })
    }
}

/// Remove `normalized_method_id` from every relation it currently belongs to.
/// Mirrors `removePresentVerificationMethodRelations`.
pub async fn remove_present_verification_method_relations<C>(
    did_contract: &C,
    memberships: &[VerificationMethodRelationMembership],
    normalized_method_id: &str,
) -> Result<(), ApiError>
where
    C: DidContract + ?Sized,
{
    for entry in memberships.iter() {
        if !entry.member {
            continue;
        }
        let ledger_relation = ledger_verification_method_relation_for(entry.relation);
        did_contract
            .set_verification_method_relation(ledger_relation, normalized_method_id.to_owned(), SetMutation::Remove)
            .await?;
    }
    Ok(())
}

/// `purgeVerificationMethodFromAllRelations` — read fresh ledger state and
/// remove any relation memberships that reference `normalized_method_id`.
pub async fn purge_verification_method_from_all_relations<C>(
    did_contract: &C,
    normalized_method_id: &str,
) -> Result<(), ApiError>
where
    C: DidContract + ?Sized,
{
    let state = did_contract.read_ledger().await?;
    let memberships = verification_method_relation_memberships(&state, normalized_method_id);
    remove_present_verification_method_relations(did_contract, &memberships, normalized_method_id).await
}

/// `addVerificationMethod`.
pub async fn add_verification_method<C>(
    did_contract: &C,
    verification_method: &VerificationMethod,
) -> Result<FinalizedTxData, ApiError>
where
    C: DidContract + ?Sized,
{
    let ledger = verification_method_to_ledger(did_contract, verification_method)?;
    Ok(did_contract
        .set_verification_method(ledger, MapMutation::Insert)
        .await?)
}

/// `updateVerificationMethod`.
pub async fn update_verification_method<C>(
    did_contract: &C,
    verification_method: &VerificationMethod,
) -> Result<FinalizedTxData, ApiError>
where
    C: DidContract + ?Sized,
{
    let ledger = verification_method_to_ledger(did_contract, verification_method)?;
    Ok(did_contract
        .set_verification_method(ledger, MapMutation::Update)
        .await?)
}

/// `removeVerificationMethod` — purges relation memberships then removes
/// the method.
pub async fn remove_verification_method<C>(did_contract: &C, method_id: &str) -> Result<FinalizedTxData, ApiError>
where
    C: DidContract + ?Sized,
{
    let normalized = normalize_bound_fragment_id_for(did_contract, method_id, BoundIdField::MethodId)?;
    purge_verification_method_from_all_relations(did_contract, &normalized).await?;
    Ok(did_contract.remove_verification_method(normalized).await?)
}

/// `addSchnorrJubjubVerificationMethod`.
pub async fn add_schnorr_jubjub_verification_method<C>(
    did_contract: &C,
    verification_method: &SchnorrJubjubVerificationMethod,
) -> Result<FinalizedTxData, ApiError>
where
    C: DidContract + ?Sized,
{
    let ledger = schnorr_jubjub_verification_method_to_ledger(did_contract, verification_method)?;
    Ok(did_contract
        .set_schnorr_jubjub_verification_method(ledger, MapMutation::Insert)
        .await?)
}

/// `updateSchnorrJubjubVerificationMethod`.
pub async fn update_schnorr_jubjub_verification_method<C>(
    did_contract: &C,
    verification_method: &SchnorrJubjubVerificationMethod,
) -> Result<FinalizedTxData, ApiError>
where
    C: DidContract + ?Sized,
{
    let ledger = schnorr_jubjub_verification_method_to_ledger(did_contract, verification_method)?;
    Ok(did_contract
        .set_schnorr_jubjub_verification_method(ledger, MapMutation::Update)
        .await?)
}

/// `removeSchnorrJubjubVerificationMethod` — purges relation memberships
/// then removes the method.
pub async fn remove_schnorr_jubjub_verification_method<C>(
    did_contract: &C,
    method_id: &str,
) -> Result<FinalizedTxData, ApiError>
where
    C: DidContract + ?Sized,
{
    let normalized = normalize_bound_fragment_id_for(did_contract, method_id, BoundIdField::MethodId)?;
    purge_verification_method_from_all_relations(did_contract, &normalized).await?;
    Ok(did_contract
        .remove_schnorr_jubjub_verification_method(normalized)
        .await?)
}

/// `verifySchnorrJubjubDigestSignature`.
pub async fn verify_schnorr_jubjub_digest_signature<C>(
    did_contract: &C,
    method_id: &str,
    digest: SchnorrJubjubDigest,
    signature: SchnorrJubjubSignature,
) -> Result<FinalizedTxData, ApiError>
where
    C: DidContract + ?Sized,
{
    let normalized = normalize_bound_fragment_id_for(did_contract, method_id, BoundIdField::MethodId)?;
    Ok(did_contract
        .verify_schnorr_jubjub_digest_signature(normalized, digest, signature)
        .await?)
}

/// `addVerificationMethodRelation`.
pub async fn add_verification_method_relation<C>(
    did_contract: &C,
    relation: VerificationMethodRelation,
    method_id: &str,
) -> Result<FinalizedTxData, ApiError>
where
    C: DidContract + ?Sized,
{
    let normalized = normalize_bound_fragment_id_for(did_contract, method_id, BoundIdField::MethodId)?;
    let state = did_contract.read_ledger().await?;
    assert_verification_method_relation_absent(&state, relation, &normalized)?;
    let ledger_relation = ledger_verification_method_relation_for(relation);
    Ok(did_contract
        .set_verification_method_relation(ledger_relation, normalized, SetMutation::Insert)
        .await?)
}

/// `removeVerificationMethodRelation`.
pub async fn remove_verification_method_relation<C>(
    did_contract: &C,
    relation: VerificationMethodRelation,
    method_id: &str,
) -> Result<FinalizedTxData, ApiError>
where
    C: DidContract + ?Sized,
{
    let normalized = normalize_bound_fragment_id_for(did_contract, method_id, BoundIdField::MethodId)?;
    let state = did_contract.read_ledger().await?;
    assert_verification_method_relation_present(&state, relation, &normalized)?;
    let ledger_relation = ledger_verification_method_relation_for(relation);
    Ok(did_contract
        .set_verification_method_relation(ledger_relation, normalized, SetMutation::Remove)
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{
        DidLedgerSnapshot, LedgerVerificationMethodRelation,
        mock::{RecordedCall, RecordingContract},
    };
    use midnight_did_domain::{
        crypto_codecs::encode_base64url,
        did_document::{CurveType, DidKeyId, DidString, KeyType, PublicKeyJwk, VerificationMethodType},
        midnight::MidnightNetwork,
    };
    use std::collections::BTreeMap;

    const ADDR: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    fn did_subject() -> String {
        format!("did:midnight:testnet:{ADDR}")
    }

    fn p256_vm(id: &str) -> VerificationMethod {
        let coord = encode_base64url(&[0u8; 32]);
        VerificationMethod {
            id: DidKeyId(format!("{}#{}", did_subject(), id)),
            type_: VerificationMethodType::JsonWebKey,
            controller: DidString(did_subject()),
            public_key_jwk: PublicKeyJwk {
                kty: KeyType::EC,
                crv: CurveType::P256,
                x: coord.clone(),
                y: Some(coord),
                extensions: BTreeMap::new(),
            },
        }
    }

    #[tokio::test]
    async fn add_verification_method_records_insert() {
        let contract = RecordingContract::new(ADDR, MidnightNetwork::Testnet);
        let vm = p256_vm("key-1");
        add_verification_method(&contract, &vm).await.unwrap();
        let calls = contract.calls();
        assert!(matches!(
            &calls[..],
            [RecordedCall::SetVerificationMethod(ledger, MapMutation::Insert)]
                if ledger.id == "#key-1"
        ));
    }

    #[tokio::test]
    async fn update_verification_method_records_update() {
        let contract = RecordingContract::new(ADDR, MidnightNetwork::Testnet);
        update_verification_method(&contract, &p256_vm("key-1")).await.unwrap();
        let calls = contract.calls();
        assert!(matches!(
            &calls[..],
            [RecordedCall::SetVerificationMethod(_, MapMutation::Update)]
        ));
    }

    #[tokio::test]
    async fn remove_verification_method_purges_relations() {
        let mut ledger = DidLedgerSnapshot::default();
        ledger.authentication_relation.push("#key-1".into());
        ledger.assertion_method_relation.push("#key-1".into());
        let contract = RecordingContract::with_ledger(ADDR, MidnightNetwork::Testnet, ledger);

        remove_verification_method(&contract, "key-1").await.unwrap();
        let calls = contract.calls();
        // read_ledger + 2 remove-relation + remove vm.
        assert_eq!(calls.len(), 4);
        assert!(matches!(calls[0], RecordedCall::ReadLedger));
        assert!(matches!(
            &calls[1],
            RecordedCall::SetVerificationMethodRelation(LedgerVerificationMethodRelation::Authentication, id, SetMutation::Remove)
                if id == "#key-1"
        ));
        assert!(matches!(
            &calls[2],
            RecordedCall::SetVerificationMethodRelation(LedgerVerificationMethodRelation::AssertionMethod, id, SetMutation::Remove)
                if id == "#key-1"
        ));
        assert!(matches!(&calls[3], RecordedCall::RemoveVerificationMethod(id) if id == "#key-1"));
    }

    #[tokio::test]
    async fn add_relation_rejects_already_present() {
        let mut ledger = DidLedgerSnapshot::default();
        ledger.authentication_relation.push("#key-1".into());
        let contract = RecordingContract::with_ledger(ADDR, MidnightNetwork::Testnet, ledger);
        let err = add_verification_method_relation(&contract, VerificationMethodRelation::Authentication, "key-1")
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::RelationAlreadyContains { .. }));
    }

    #[tokio::test]
    async fn remove_relation_rejects_when_missing() {
        let contract = RecordingContract::new(ADDR, MidnightNetwork::Testnet);
        let err = remove_verification_method_relation(&contract, VerificationMethodRelation::KeyAgreement, "key-1")
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::RelationMissing { .. }));
    }
}
