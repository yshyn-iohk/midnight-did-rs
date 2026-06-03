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

//! Integration tests for the domain ↔ ledger mappers in
//! `midnight_did_api::ledger_mappers`.
//!
//! Rust port of `packages/api/src/test/ledger-mappers.test.ts`.
//!
//! Each test mirrors one TS `it(…)` case; the assertions preserve the TS
//! intent (key-profile sentinel rules and exception messaging) rather than
//! the exact error string.

use std::collections::BTreeMap;

use midnight_did_api::{
    contract::{DidLedgerSnapshot, LedgerVerificationMethodRelation, mock::RecordingContract},
    error::ApiError,
    ledger_mappers::{public_key_jwk_to_ledger, relation_set_from_state, verification_method_to_ledger},
};
use midnight_did_domain::{
    crypto_codecs::encode_base64url,
    did_document::{
        CurveType, DidKeyId, DidString, KeyType, PublicKeyJwk, VerificationMethod, VerificationMethodRelation,
        VerificationMethodType,
    },
};
use midnight_did_method::midnight_did::MidnightNetwork;

// Address constant chosen to match the TS test setup (32 bytes hex).
// The TS source uses `0123…ef` ad infinitum; in Rust the API surface
// validates the contract address shape via the domain parser, so we use a
// well-formed test address here. The address itself does not affect the
// ledger-mapper output beyond the fragment normalization step.
const ADDR: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn contract() -> RecordingContract {
    RecordingContract::new(ADDR, MidnightNetwork::Undeployed)
}

fn did_subject() -> String {
    format!("did:midnight:undeployed:{ADDR}")
}

/// 32-byte all-zero coords encoded as base64url (length 43, no padding).
fn zeros32_b64url() -> String {
    encode_base64url(&[0u8; 32])
}

/// 48-byte BLS12-381 G1 coordinate placeholder (length 64 base64url, no padding).
fn bls_g1_b64url() -> String {
    encode_base64url(&[6u8; 48])
}

fn vm_with_jwk(id: &str, jwk: PublicKeyJwk) -> VerificationMethod {
    VerificationMethod {
        id: DidKeyId(format!("{}#{id}", did_subject())),
        type_: VerificationMethodType::JsonWebKey,
        controller: DidString(did_subject()),
        public_key_jwk: jwk,
    }
}

// ---------------------------------------------------------------------------
// TS: "normalizes OKP keys to the ledger y sentinel"
// ---------------------------------------------------------------------------
#[test]
fn normalizes_okp_keys_to_ledger_y_sentinel() {
    let x = zeros32_b64url();
    let vm = vm_with_jwk(
        "key-ed25519",
        PublicKeyJwk {
            kty: KeyType::OKP,
            crv: CurveType::Ed25519,
            x: x.clone(),
            y: None,
            extensions: BTreeMap::new(),
        },
    );
    let ledger = verification_method_to_ledger(&contract(), &vm).expect("map ok");
    assert_eq!(ledger.public_key_jwk.kty, KeyType::OKP);
    assert_eq!(ledger.public_key_jwk.crv, CurveType::Ed25519);
    assert_eq!(ledger.public_key_jwk.x, x);
    // Y collapses to the empty-string sentinel.
    assert_eq!(ledger.public_key_jwk.y, "");
}

// ---------------------------------------------------------------------------
// TS: "normalizes BLS12-381 OKP keys to the ledger y sentinel"
// ---------------------------------------------------------------------------
#[test]
fn normalizes_bls12381_g1_okp_keys_to_ledger_y_sentinel() {
    let x = bls_g1_b64url();
    let vm = vm_with_jwk(
        "key-bls12381-g1",
        PublicKeyJwk {
            kty: KeyType::OKP,
            crv: CurveType::BLS12381G1,
            x: x.clone(),
            y: None,
            extensions: BTreeMap::new(),
        },
    );
    let ledger = verification_method_to_ledger(&contract(), &vm).expect("map ok");
    assert_eq!(ledger.public_key_jwk.kty, KeyType::OKP);
    assert_eq!(ledger.public_key_jwk.crv, CurveType::BLS12381G1);
    assert_eq!(ledger.public_key_jwk.x, x);
    assert_eq!(ledger.public_key_jwk.y, "");
}

// ---------------------------------------------------------------------------
// TS: "rejects opaque JWK shapes that would not resolve cleanly"
// One Rust test per rejection path.
// ---------------------------------------------------------------------------

#[test]
fn rejects_okp_keys_with_y_coordinate() {
    let x = zeros32_b64url();
    let vm = vm_with_jwk(
        "key-okp-with-y",
        PublicKeyJwk {
            kty: KeyType::OKP,
            crv: CurveType::Ed25519,
            x: x.clone(),
            y: Some(x),
            extensions: BTreeMap::new(),
        },
    );
    let err = verification_method_to_ledger(&contract(), &vm).unwrap_err();
    let msg = err.to_string();
    assert!(matches!(err, ApiError::InvalidArgument(_)), "{msg}");
    assert!(msg.contains("OKP keys must not include a y coordinate"), "{msg}");
}

#[test]
fn rejects_ec_keys_without_y_coordinate() {
    let x = zeros32_b64url();
    let vm = vm_with_jwk(
        "key-ec-without-y",
        PublicKeyJwk {
            kty: KeyType::EC,
            crv: CurveType::P256,
            x,
            y: None,
            extensions: BTreeMap::new(),
        },
    );
    let err = verification_method_to_ledger(&contract(), &vm).unwrap_err();
    let msg = err.to_string();
    assert!(matches!(err, ApiError::InvalidArgument(_)), "{msg}");
    assert!(msg.contains("EC keys must include a y coordinate"), "{msg}");
}

#[test]
fn rejects_bls_okp_keys_with_y_coordinate() {
    let x = bls_g1_b64url();
    let vm = vm_with_jwk(
        "key-bls-with-y",
        PublicKeyJwk {
            kty: KeyType::OKP,
            crv: CurveType::BLS12381G1,
            x: x.clone(),
            y: Some(zeros32_b64url()),
            extensions: BTreeMap::new(),
        },
    );
    let err = verification_method_to_ledger(&contract(), &vm).unwrap_err();
    let msg = err.to_string();
    assert!(matches!(err, ApiError::InvalidArgument(_)), "{msg}");
    assert!(msg.contains("OKP keys must not include a y coordinate"), "{msg}");
}

#[test]
fn rejects_jwk_with_private_key_material() {
    let x = zeros32_b64url();
    let mut extensions = BTreeMap::new();
    extensions.insert("d".into(), serde_json::Value::String(x.clone()));
    let vm = vm_with_jwk(
        "key-with-private-d",
        PublicKeyJwk {
            kty: KeyType::OKP,
            crv: CurveType::Ed25519,
            x,
            y: None,
            extensions,
        },
    );
    let err = verification_method_to_ledger(&contract(), &vm).unwrap_err();
    let msg = err.to_string();
    assert!(matches!(err, ApiError::InvalidArgument(_)), "{msg}");
    assert!(msg.contains("private key material"), "{msg}");
}

// ---------------------------------------------------------------------------
// Extra: ensure the standalone `public_key_jwk_to_ledger` also collapses y.
// ---------------------------------------------------------------------------
#[test]
fn public_key_jwk_to_ledger_okp_y_is_empty() {
    let jwk = PublicKeyJwk {
        kty: KeyType::OKP,
        crv: CurveType::Ed25519,
        x: zeros32_b64url(),
        y: None,
        extensions: BTreeMap::new(),
    };
    let ledger = public_key_jwk_to_ledger(&jwk).unwrap();
    assert_eq!(ledger.y, "");
}

// ---------------------------------------------------------------------------
// Extra: `relation_set_from_state` looks up the per-relation member slice.
// Mirrors the TS `relationSetFromState` helper exercised indirectly by the
// verification-method-relations spec.
// ---------------------------------------------------------------------------
#[test]
fn relation_set_from_state_returns_correct_slice() {
    let mut state = DidLedgerSnapshot::default();
    state.authentication_relation.push("#key-a".into());
    state.assertion_method_relation.push("#key-b".into());
    state.key_agreement_relation.push("#key-c".into());

    let auth = relation_set_from_state(&state, VerificationMethodRelation::Authentication).unwrap();
    assert_eq!(auth, &["#key-a".to_string()]);

    let assertion = relation_set_from_state(&state, VerificationMethodRelation::AssertionMethod).unwrap();
    assert_eq!(assertion, &["#key-b".to_string()]);

    let ka = relation_set_from_state(&state, VerificationMethodRelation::KeyAgreement).unwrap();
    assert_eq!(ka, &["#key-c".to_string()]);

    // Undefined is rejected (matches TS guard).
    let err = relation_set_from_state(&state, VerificationMethodRelation::Undefined).unwrap_err();
    assert!(matches!(err, ApiError::InvalidArgument(_)), "{err}");
}

// ---------------------------------------------------------------------------
// Extra: the ledger snapshot's `relation_set` accessor matches the enum
// discriminant used by the contract trait.
// ---------------------------------------------------------------------------
#[test]
fn ledger_snapshot_relation_set_uses_ledger_enum() {
    let mut state = DidLedgerSnapshot::default();
    state.capability_delegation_relation.push("#key-deleg".into());

    assert_eq!(
        state.relation_set(LedgerVerificationMethodRelation::CapabilityDelegation),
        Some(&["#key-deleg".to_string()][..])
    );
    assert_eq!(state.relation_set(LedgerVerificationMethodRelation::Undefined), None);
    assert!(state.relation_contains(LedgerVerificationMethodRelation::CapabilityDelegation, "#key-deleg"));
    assert!(!state.relation_contains(LedgerVerificationMethodRelation::Authentication, "#key-deleg"));
}
