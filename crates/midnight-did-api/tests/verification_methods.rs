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

//! Integration tests for verification-method CRUD + relation purge logic.
//!
//! Rust port of `packages/api/src/test/verification-method-operations.test.ts`
//! and the related parts of `verification-method-relations.test.ts`. The TS
//! test focuses on:
//!
//! - SchnorrJubjub verifyDigestSignature passes a normalized fragment id to
//!   the contract circuit (`#key-1`, not the absolute `did:midnight:…#key-1`).
//! - Add / update / remove drive the right `MapMutation` tag.
//! - Remove purges the verification method from every relation it belongs
//!   to before invoking `removeVerificationMethod`.
//! - addRelation / removeRelation reject duplicate-insert and
//!   missing-remove early.

use std::collections::BTreeMap;

use midnight_did_api::contract::{
    DidLedgerSnapshot, JubjubPointHex, LedgerVerificationMethodRelation, MapMutation, NewJubjubPointHex,
    SchnorrJubjubDigest, SchnorrJubjubSignature, SetMutation,
};
use midnight_did_api::error::ApiError;
use midnight_did_api::ledger_mappers::{NewSchnorrJubjubVerificationMethod, SchnorrJubjubVerificationMethod};
use midnight_did_api::verification_method_operations::{
    VERIFICATION_METHOD_RELATIONS, add_schnorr_jubjub_verification_method, add_verification_method,
    add_verification_method_relation, remove_schnorr_jubjub_verification_method, remove_verification_method,
    remove_verification_method_relation, update_verification_method, verification_method_relation_memberships,
    verify_schnorr_jubjub_digest_signature,
};
use midnight_did_domain::crypto_codecs::encode_base64url;
use midnight_did_domain::did_document::{
    CurveType, KeyType, NewPublicKeyJwk, NewVerificationMethod, PublicKeyJwk, VerificationMethod,
    VerificationMethodRelation, VerificationMethodType,
};
use midnight_did_method::midnight_did::{MidnightNetwork, parse_contract_address};
use midnight_did_runtime::{Contract, DidContractCall, RecordingBackend};

const ADDR: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn contract() -> Contract<RecordingBackend> {
    Contract::new(
        RecordingBackend::new(),
        parse_contract_address(ADDR).unwrap(),
        MidnightNetwork::Undeployed,
    )
}

fn contract_with_ledger(ledger: DidLedgerSnapshot) -> Contract<RecordingBackend> {
    Contract::new(
        RecordingBackend::with_snapshot(ledger),
        parse_contract_address(ADDR).unwrap(),
        MidnightNetwork::Undeployed,
    )
}

fn did_subject() -> String {
    format!("did:midnight:undeployed:{ADDR}")
}

fn ed25519_vm(id: &str) -> VerificationMethod {
    let x = encode_base64url(&[0u8; 32]);
    VerificationMethod::new(NewVerificationMethod {
        id: format!("{}#{id}", did_subject()),
        type_: VerificationMethodType::JsonWebKey,
        controller: did_subject(),
        public_key_jwk: PublicKeyJwk::new(NewPublicKeyJwk {
            kty: KeyType::OKP,
            crv: CurveType::Ed25519,
            x,
            y: None,
            extensions: BTreeMap::new(),
        })
        .unwrap(),
    })
    .unwrap()
}

// ---------------------------------------------------------------------------
// TS: "verifies SchnorrJubjub signatures against a normalized ledger method id"
// ---------------------------------------------------------------------------
#[tokio::test]
async fn verifies_schnorr_jubjub_signature_with_normalized_method_id() {
    let c = contract();
    let absolute_method_id = format!("{}#key-1", did_subject());
    let digest = SchnorrJubjubDigest::new(["01".repeat(32), "02".repeat(32), "03".repeat(32), "04".repeat(32)])
        .expect("valid digest fixture");
    let signature = SchnorrJubjubSignature::new("ab".repeat(96)).expect("valid signature fixture");

    verify_schnorr_jubjub_digest_signature(&c, &absolute_method_id, digest.clone(), signature.clone())
        .await
        .expect("verify ok");

    let calls = c.backend.recorded_calls();
    let recorded = calls
        .iter()
        .find_map(|call| match call {
            DidContractCall::VerifySchnorrJubjubDigestSignature {
                method_id: id,
                digest: d,
                signature: s,
            } => Some((id, d, s)),
            _ => None,
        })
        .expect("recorded verify call");
    // TS asserts the trailing fragment is passed, not the absolute DID URL.
    assert_eq!(recorded.0, "#key-1");
    assert_eq!(recorded.1, &digest);
    assert_eq!(recorded.2, &signature);
}

// ---------------------------------------------------------------------------
// Round out the VM CRUD operations: add / update / remove.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn add_verification_method_records_insert_with_normalized_id() {
    let c = contract();
    add_verification_method(&c, &ed25519_vm("key-add"))
        .await
        .expect("add ok");
    let calls = c.backend.recorded_calls();
    match &calls[..] {
        [
            DidContractCall::SetVerificationMethod {
                method: ledger,
                mutation: MapMutation::Insert,
            },
        ] => {
            assert_eq!(ledger.id, "#key-add");
            assert_eq!(ledger.typ, VerificationMethodType::JsonWebKey);
            assert_eq!(ledger.public_key_jwk.kty, KeyType::OKP);
            assert_eq!(ledger.public_key_jwk.crv, CurveType::Ed25519);
            assert_eq!(ledger.public_key_jwk.y, ""); // ledger y sentinel
        }
        other => panic!("unexpected recorded calls: {other:?}"),
    }
}

#[tokio::test]
async fn update_verification_method_records_update() {
    let c = contract();
    update_verification_method(&c, &ed25519_vm("key-update"))
        .await
        .expect("update ok");
    let calls = c.backend.recorded_calls();
    match &calls[..] {
        [
            DidContractCall::SetVerificationMethod {
                method: ledger,
                mutation: MapMutation::Update,
            },
        ] => {
            assert_eq!(ledger.id, "#key-update");
        }
        other => panic!("unexpected recorded calls: {other:?}"),
    }
}

#[tokio::test]
async fn remove_verification_method_purges_then_removes() {
    // Seed: method belongs to two relations.
    let mut ledger = DidLedgerSnapshot::default();
    ledger.authentication_relation.push("#key-rm".into());
    ledger.key_agreement_relation.push("#key-rm".into());
    let c = contract_with_ledger(ledger);

    remove_verification_method(&c, "key-rm").await.expect("remove ok");

    let calls = c.backend.recorded_calls();
    // Expect: read_ledger, two remove-relation calls (only on the ones present),
    // then the final remove-vm.
    assert!(matches!(calls[0], DidContractCall::ReadLedger), "{:?}", calls);

    let relation_removes: Vec<_> = calls
        .iter()
        .filter_map(|c| match c {
            DidContractCall::SetVerificationMethodRelation {
                relation: rel,
                method_id: id,
                mutation: SetMutation::Remove,
            } => Some((*rel, id.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(relation_removes.len(), 2);
    assert!(
        relation_removes
            .iter()
            .any(|(r, id)| matches!(r, LedgerVerificationMethodRelation::Authentication) && id == "#key-rm")
    );
    assert!(
        relation_removes
            .iter()
            .any(|(r, id)| matches!(r, LedgerVerificationMethodRelation::KeyAgreement) && id == "#key-rm")
    );

    assert!(
        matches!(calls.last(), Some(DidContractCall::RemoveVerificationMethod { method_id: id }) if id == "#key-rm"),
        "{:?}",
        calls
    );
}

#[tokio::test]
async fn remove_verification_method_skips_relations_it_does_not_belong_to() {
    // Method only in authentication; assertion-method should not be touched.
    let mut ledger = DidLedgerSnapshot::default();
    ledger.authentication_relation.push("#key-solo".into());
    let c = contract_with_ledger(ledger);
    remove_verification_method(&c, "key-solo").await.expect("remove ok");
    let calls = c.backend.recorded_calls();
    let removes: Vec<_> = calls
        .iter()
        .filter(|c| {
            matches!(
                c,
                DidContractCall::SetVerificationMethodRelation {
                    relation: _,
                    method_id: _,
                    mutation: _
                }
            )
        })
        .collect();
    assert_eq!(removes.len(), 1, "expected one relation purge, got: {removes:?}");
}

// ---------------------------------------------------------------------------
// SchnorrJubjub VM CRUD path.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn add_schnorr_jubjub_verification_method_records_insert() {
    let c = contract();
    let vm = SchnorrJubjubVerificationMethod::new(NewSchnorrJubjubVerificationMethod {
        id: "#key-sj".to_string(),
        public_key: JubjubPointHex::new(NewJubjubPointHex {
            x: "00".repeat(32),
            y: "01".repeat(32),
        })
        .expect("valid Jubjub point fixture"),
    })
    .expect("valid SchnorrJubjub VM fixture");
    add_schnorr_jubjub_verification_method(&c, &vm).await.expect("add ok");
    let calls = c.backend.recorded_calls();
    assert!(
        matches!(
            &calls[..],
            [DidContractCall::SetSchnorrJubjubVerificationMethod { method: ledger, mutation: MapMutation::Insert }]
                if ledger.id == "#key-sj"
        ),
        "{:?}",
        calls
    );
}

#[tokio::test]
async fn remove_schnorr_jubjub_verification_method_purges_then_removes() {
    let mut ledger = DidLedgerSnapshot::default();
    ledger.authentication_relation.push("#key-sj".into());
    let c = contract_with_ledger(ledger);
    remove_schnorr_jubjub_verification_method(&c, "key-sj")
        .await
        .expect("remove ok");
    let calls = c.backend.recorded_calls();
    assert!(matches!(calls[0], DidContractCall::ReadLedger));
    assert!(matches!(
        calls.last(),
        Some(DidContractCall::RemoveSchnorrJubjubVerificationMethod { method_id: id }) if id == "#key-sj"
    ));
}

// ---------------------------------------------------------------------------
// TS verification-method-relations.test.ts: relation membership + guards.
// ---------------------------------------------------------------------------

#[test]
fn verification_method_relations_constant_skips_undefined() {
    // Mirrors TS: VerificationMethodRelations excludes Undefined.
    let expected = [
        VerificationMethodRelation::Authentication,
        VerificationMethodRelation::AssertionMethod,
        VerificationMethodRelation::KeyAgreement,
        VerificationMethodRelation::CapabilityInvocation,
        VerificationMethodRelation::CapabilityDelegation,
    ];
    assert_eq!(VERIFICATION_METHOD_RELATIONS, expected);
    assert!(!VERIFICATION_METHOD_RELATIONS.contains(&VerificationMethodRelation::Undefined));
}

#[test]
fn relation_memberships_reports_each_supported_relation() {
    let mut ledger = DidLedgerSnapshot::default();
    ledger.authentication_relation.push("#key-1".into());
    ledger.key_agreement_relation.push("#key-1".into());
    ledger.capability_delegation_relation.push("#key-2".into());

    let m = verification_method_relation_memberships(&ledger, "#key-1");
    assert_eq!(m.len(), 5);

    let pairs: Vec<_> = m.iter().map(|e| (e.relation, e.member)).collect();
    assert_eq!(
        pairs,
        vec![
            (VerificationMethodRelation::Authentication, true),
            (VerificationMethodRelation::AssertionMethod, false),
            (VerificationMethodRelation::KeyAgreement, true),
            (VerificationMethodRelation::CapabilityInvocation, false),
            (VerificationMethodRelation::CapabilityDelegation, false),
        ]
    );
}

#[tokio::test]
async fn add_verification_method_relation_rejects_already_present() {
    let mut ledger = DidLedgerSnapshot::default();
    ledger.authentication_relation.push("#key-1".into());
    let c = contract_with_ledger(ledger);
    let err = add_verification_method_relation(&c, VerificationMethodRelation::Authentication, "key-1")
        .await
        .unwrap_err();
    match &err {
        ApiError::Verification(midnight_did_api::error::VerificationError::RelationAlreadyContains {
            relation,
            method_id,
        }) => {
            assert_eq!(relation, "Authentication");
            assert_eq!(method_id, "#key-1");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn remove_verification_method_relation_rejects_when_missing() {
    let c = contract();
    let err = remove_verification_method_relation(&c, VerificationMethodRelation::KeyAgreement, "key-1")
        .await
        .unwrap_err();
    match &err {
        ApiError::Verification(midnight_did_api::error::VerificationError::RelationMissing { relation, method_id }) => {
            assert_eq!(relation, "KeyAgreement");
            assert_eq!(method_id, "#key-1");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}
