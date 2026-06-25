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

//! End-to-end CRUD flow + TS reference-fixture interop assertions.
//!
//! Rust port of the structural pieces of
//! `packages/api/src/test/did.api.test.ts`. The TS test stands up a live
//! Midnight node + wallet via testcontainers; we replace those parts with
//! the in-memory mock contract + private-state store. The remaining test
//! intent is preserved:
//!
//! - createDID seeds the controller secret and reads back an active flag.
//! - rotateControllerKey changes the controller public key and keeps
//!   subsequent operations authorized; the version counter advances.
//! - resolve returns a DID Document with the W3C DID Core + JWS-2020
//!   contexts.
//! - addVerificationMethod inserts a JWK method; resolve surfaces it.
//! - addService / removeService round-trip.
//! - addAlsoKnownAs adds an entry to the alsoKnownAs set.
//! - deactivate flips the metadata.
//!
//! Three JSON fixtures under `tests/fixtures/` capture the expected DID
//! Document for "initial", "after rotate", and "after set verification
//! method" states. The Rust test seeds an equivalent
//! [`DidLedgerSnapshot`] and asserts the resolver output matches the
//! fixture structurally (via `serde_json::Value` equality so key order is
//! irrelevant).

use std::collections::BTreeMap;
use std::path::PathBuf;

use midnight_did_api::{
    contract::{
        DidLedgerSnapshot, JubjubPointHex, LedgerPublicKeyJwk, LedgerSchnorrJubjubVerificationMethod, LedgerService,
        LedgerVerificationMethod,
    },
    controller_operations::rotate_controller_key,
    did_operations::create_did,
    document_operations::{add_also_known_as, deactivate, remove_also_known_as},
    ledger_mappers::SchnorrJubjubVerificationMethod,
    private_state::{InMemoryPrivateStateStore, PrivateStateSlot, restore_private_state},
    resolution::{ledger_state_to_did_document, ledger_state_to_metadata, resolve},
    service_operations::{add_service, remove_service},
    verification_method_operations::{
        add_schnorr_jubjub_verification_method, add_verification_method, add_verification_method_relation,
        remove_schnorr_jubjub_verification_method, remove_verification_method, update_verification_method,
    },
};
use midnight_did_domain::{
    crypto_codecs::encode_base64url,
    did_document::{
        CurveType, KeyType, NewPublicKeyJwk, NewService, NewVerificationMethod, PublicKeyJwk, Service, ServiceEndpoint,
        ServiceType, VerificationMethod, VerificationMethodRelation, VerificationMethodType,
    },
};
use midnight_did_method::midnight_did::{MidnightNetwork, parse_contract_address};
use midnight_did_runtime::{Contract, DidContractCall, RecordingBackend};

/// Build a `Contract<RecordingBackend>` seeded with `ledger` — replaces the
/// legacy `RecordingContract::with_ledger(ADDR, network, ledger)` ergonomic.
fn contract_with(network: MidnightNetwork, ledger: DidLedgerSnapshot) -> Contract<RecordingBackend> {
    Contract::new(
        RecordingBackend::with_snapshot(ledger),
        parse_contract_address(ADDR).unwrap(),
        network,
    )
}

const ADDR: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn did_subject() -> String {
    format!("did:midnight:undeployed:{ADDR}")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures")
}

fn load_fixture(name: &str) -> serde_json::Value {
    let path = fixtures_dir().join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|err| panic!("read {path:?}: {err}"));
    serde_json::from_slice(&bytes).unwrap_or_else(|err| panic!("parse {path:?}: {err}"))
}

fn dump<T: serde::Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).expect("serialize")
}

// ---------------------------------------------------------------------------
// In-memory contract sequencing helpers. The mock RecordingContract does not
// reseed its ledger snapshot on mutate calls, so each "stage" of the flow
// installs the next state explicitly.
// ---------------------------------------------------------------------------

fn initial_ledger() -> DidLedgerSnapshot {
    DidLedgerSnapshot {
        active: true,
        deactivated: false,
        controller_public_key_hex: "01".repeat(32),
        version: 1,
        operation_count: 0,
        contract_version: 1,
        created_ms: 1_700_000_000_000,
        updated_ms: 1_700_000_000_000,
        ..Default::default()
    }
}

fn rotated_ledger() -> DidLedgerSnapshot {
    let mut s = initial_ledger();
    s.controller_public_key_hex = "ff".repeat(32);
    s.version = 2;
    s.operation_count = 1;
    s.updated_ms = 1_700_001_000_000; // 2023-11-14T22:30:00Z
    s
}

/// Ledger after adding a single Ed25519 verification method with id `#key-1`.
fn vm_added_ledger() -> DidLedgerSnapshot {
    let mut s = rotated_ledger();
    s.version = 3;
    s.operation_count = 2;
    s.updated_ms = 1_700_001_900_000; // 2023-11-14T22:45:00Z
    s.verification_methods.insert(
        "#key-1".to_string(),
        LedgerVerificationMethod {
            id: "#key-1".into(),
            typ: VerificationMethodType::JsonWebKey,
            public_key_jwk: LedgerPublicKeyJwk {
                kty: KeyType::OKP,
                crv: CurveType::Ed25519,
                x: encode_base64url(&[0u8; 32]),
                // ledger y sentinel
                y: String::new(),
            },
        },
    );
    s
}

// ===========================================================================
// TS fixture parity tests
// ===========================================================================

/// Mirrors the TS "should publish the associated smart-contract … with an
/// empty state" + "should resolve the DID Document" cases combined: empty
/// ledger -> initial DID Document fixture.
#[tokio::test]
async fn initial_state_matches_ts_fixture() {
    let contract = contract_with(MidnightNetwork::Undeployed, initial_ledger());
    let resolved = resolve(&contract).await.unwrap().expect("resolves");

    let actual = serde_json::json!({
        "didDocument": dump(&resolved.did_document),
        "didDocumentMetadata": dump(&resolved.did_document_metadata),
    });
    let expected = load_fixture("initial-state.json");
    assert_eq!(
        actual,
        expected,
        "initial DID Document diverges from TS fixture: actual = {}",
        serde_json::to_string_pretty(&actual).unwrap()
    );
}

/// After rotateControllerKey the document keeps the same id + controller +
/// contexts; only metadata (`versionId`, `updated`) advances.
#[tokio::test]
async fn after_rotate_controller_key_matches_ts_fixture() {
    let contract = contract_with(MidnightNetwork::Undeployed, initial_ledger());
    let store = InMemoryPrivateStateStore::new();

    // Seed the controller secret + drive a rotate. The mock contract does not
    // update its ledger snapshot in response to the circuit call — the test
    // installs the post-rotate ledger by hand so the fixture parity assertion
    // is reproducible.
    create_did(&contract, &store, [1u8; 32]).await.unwrap();
    rotate_controller_key(&contract, &store, [2u8; 32], [0xffu8; 32])
        .await
        .unwrap();
    contract.backend.set_snapshot(rotated_ledger());

    let resolved = resolve(&contract).await.unwrap().expect("resolves");
    let actual = serde_json::json!({
        "didDocument": dump(&resolved.did_document),
        "didDocumentMetadata": dump(&resolved.did_document_metadata),
    });
    let expected = load_fixture("after-rotate-controller-key.json");
    assert_eq!(actual, expected);
}

/// After addVerificationMethod the document gains a `verificationMethod`
/// array with the inserted entry, controller / contexts unchanged.
#[tokio::test]
async fn after_set_verification_method_matches_ts_fixture() {
    let contract = contract_with(MidnightNetwork::Undeployed, initial_ledger());

    let coord = encode_base64url(&[0u8; 32]);
    let vm = VerificationMethod::new(NewVerificationMethod {
        id: format!("{}#key-1", did_subject()),
        type_: VerificationMethodType::JsonWebKey,
        controller: did_subject(),
        public_key_jwk: PublicKeyJwk::new(NewPublicKeyJwk {
            kty: KeyType::OKP,
            crv: CurveType::Ed25519,
            x: coord,
            y: None,
            extensions: BTreeMap::new(),
        })
        .unwrap(),
    })
    .unwrap();

    add_verification_method(&contract, &vm).await.expect("add ok");
    // Replace ledger to reflect the insertion (mock contract is recording-only).
    contract.backend.set_snapshot(vm_added_ledger());

    let resolved = resolve(&contract).await.unwrap().expect("resolves");
    let actual = serde_json::json!({
        "didDocument": dump(&resolved.did_document),
        "didDocumentMetadata": dump(&resolved.did_document_metadata),
    });
    let expected = load_fixture("after-set-verification-method.json");
    assert_eq!(actual, expected);
}

// ===========================================================================
// End-to-end CRUD spec coverage (no fixture; verifies behaviour).
// ===========================================================================

/// "should publish the associated smart-contract to the Midnight blockchain
/// with an empty state" — Rust counterpart asserts createDID seeds the
/// active slot and the resolver returns the empty initial document.
#[tokio::test]
async fn create_did_seeds_active_slot_and_resolves_empty_document() {
    let contract = contract_with(MidnightNetwork::Undeployed, initial_ledger());
    let store = InMemoryPrivateStateStore::new();
    let secret_key = [7u8; 32];

    create_did(&contract, &store, secret_key).await.unwrap();
    let active = restore_private_state(&store, PrivateStateSlot::Active).await.unwrap();
    assert_eq!(active.unwrap().secret_key, secret_key);

    let resolved = resolve(&contract).await.unwrap().expect("resolves");
    assert!(resolved.did_document.verification_method.is_none());
    assert!(resolved.did_document.service.is_none());
    assert!(resolved.did_document.also_known_as.is_none());
    assert_eq!(
        resolved.did_document.id.as_str(),
        did_subject(),
        "DID Document id must equal the DID subject"
    );
}

/// "should rotate the controller key and keep subsequent updates authorized"
#[tokio::test]
async fn rotate_controller_key_then_add_and_remove_aka() {
    let contract = contract_with(MidnightNetwork::Undeployed, initial_ledger());
    let store = InMemoryPrivateStateStore::new();

    create_did(&contract, &store, [1u8; 32]).await.unwrap();
    rotate_controller_key(&contract, &store, [2u8; 32], [0xffu8; 32])
        .await
        .unwrap();

    // Subsequent aka updates succeed.
    add_also_known_as(&contract, "did:example:rotated-controller")
        .await
        .unwrap();
    remove_also_known_as(&contract, "did:example:rotated-controller")
        .await
        .unwrap();

    // The pending slot was cleared; the active slot holds the new key.
    let active = restore_private_state(&store, PrivateStateSlot::Active).await.unwrap();
    assert_eq!(active.unwrap().secret_key, [2u8; 32]);
    let pending = restore_private_state(&store, PrivateStateSlot::Pending).await.unwrap();
    assert!(pending.is_none());
}

/// "should resolve the DID Document including a reference to the DID Core
/// 1.0 specification in the `@context` property"
#[tokio::test]
async fn resolved_document_carries_w3c_contexts() {
    let contract = contract_with(MidnightNetwork::Undeployed, initial_ledger());
    let resolved = resolve(&contract).await.unwrap().expect("resolves");
    let json = serde_json::to_value(&resolved.did_document).unwrap();
    let ctx = json.get("@context").expect("@context present");
    let ctx_array = ctx.as_array().expect("@context is array");
    assert_eq!(
        ctx_array.first().and_then(|v| v.as_str()),
        Some("https://www.w3.org/ns/did/v1")
    );
    assert_eq!(
        ctx_array.get(1).and_then(|v| v.as_str()),
        Some("https://w3c.github.io/vc-jws-2020/contexts/v1")
    );
}

/// "should resolve the DID Document with an `id` matching the format
/// `did:midnight:<network_id>:<contract_address>`"
#[tokio::test]
async fn resolved_document_id_uses_canonical_midnight_did_format() {
    let contract = contract_with(MidnightNetwork::Testnet, initial_ledger());
    let resolved = resolve(&contract).await.unwrap().expect("resolves");
    assert_eq!(resolved.did_document.id.as_str(), format!("did:midnight:testnet:{ADDR}"));
}

/// "should surface DID Document metadata with version and activation state"
#[tokio::test]
async fn resolved_document_metadata_has_version_and_timestamp() {
    let contract = contract_with(MidnightNetwork::Undeployed, initial_ledger());
    let resolved = resolve(&contract).await.unwrap().expect("resolves");
    let meta = resolved.did_document_metadata;
    assert_eq!(meta.version_id.as_deref(), Some("1"));
    assert!(meta.deactivated.is_none(), "active document has no deactivated flag");
    // RFC3339 / TS regex /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/.
    let created = meta.created.expect("created present");
    assert!(
        created.len() == 20 && created.ends_with('Z') && created.chars().nth(10) == Some('T'),
        "unexpected created shape: {created}"
    );
}

/// "should add the verification method with JsonWebKey public key"
#[tokio::test]
async fn add_verification_method_records_insert_and_resolves() {
    let contract = contract_with(MidnightNetwork::Undeployed, initial_ledger());

    let coord = encode_base64url(&[0u8; 32]);
    let vm = VerificationMethod::new(NewVerificationMethod {
        id: format!("{}#key-1", did_subject()),
        type_: VerificationMethodType::JsonWebKey,
        controller: did_subject(),
        public_key_jwk: PublicKeyJwk::new(NewPublicKeyJwk {
            kty: KeyType::OKP,
            crv: CurveType::Ed25519,
            x: coord.clone(),
            y: None,
            extensions: BTreeMap::new(),
        })
        .unwrap(),
    })
    .unwrap();
    add_verification_method(&contract, &vm).await.unwrap();

    let recorded = contract
        .backend.recorded_calls()
        .into_iter()
        .find_map(|c| match c {
            DidContractCall::SetVerificationMethod { method: ledger, mutation: _ } => Some(ledger),
            _ => None,
        })
        .expect("set-vm call recorded");
    assert_eq!(recorded.id, "#key-1");
    assert_eq!(recorded.public_key_jwk.x, coord);
    assert_eq!(recorded.public_key_jwk.y, ""); // ledger y sentinel
}

/// "should update the DID by adding a new service endpoint" +
/// "should update the DID by removing the service using its `id`"
#[tokio::test]
async fn add_then_remove_service_records_expected_calls() {
    let contract = contract_with(MidnightNetwork::Undeployed, initial_ledger());
    let svc = Service::new(NewService {
        id: "service-1".into(),
        type_: ServiceType::One("DIDCommV2".into()),
        service_endpoint: ServiceEndpoint::Array(vec![
            midnight_did_domain::did_document::ServiceEndpointArrayEntry::Uri("https://localhost/didcomm/v2".into()),
            midnight_did_domain::did_document::ServiceEndpointArrayEntry::Uri("wss://localhost/didcomm/v2".into()),
        ]),
    })
    .unwrap();
    add_service(&contract, &svc).await.unwrap();
    remove_service(&contract, "service-1").await.unwrap();

    let calls = contract.backend.recorded_calls();
    assert!(
        calls
            .iter()
            .any(|c| matches!(c, DidContractCall::SetService { service: s, mutation: _ } if s.id == "#service-1")),
        "{calls:?}"
    );
    assert!(
        calls
            .iter()
            .any(|c| matches!(c, DidContractCall::RemoveService { service_id: id } if id == "#service-1")),
        "{calls:?}"
    );
}

/// "should add alsoKnownAs alias" + reject branches.
#[tokio::test]
async fn add_also_known_as_records_set_call_and_rejects_invalid_uris() {
    let contract = contract_with(MidnightNetwork::Undeployed, initial_ledger());
    add_also_known_as(&contract, "did:example:aka-1").await.unwrap();
    let calls = contract.backend.recorded_calls();
    assert!(
        calls
            .iter()
            .any(|c| matches!(c, DidContractCall::SetAlsoKnownAs { alias_uri: uri, mutation: _ } if uri == "did:example:aka-1")),
        "{calls:?}"
    );

    // TS: "should reject invalid alsoKnownAs URI when adding".
    let err = add_also_known_as(&contract, "not-a-uri").await.unwrap_err();
    assert!(
        err.to_string().contains("valid absolute URI") || err.to_string().contains("uri"),
        "unexpected error: {err}"
    );
}

/// "should deactivate the DID"
#[tokio::test]
async fn deactivate_records_call_and_metadata_reflects_state() {
    let contract = contract_with(MidnightNetwork::Undeployed, initial_ledger());
    deactivate(&contract).await.unwrap();
    assert!(
        contract.backend.recorded_calls().iter().any(|c| matches!(c, DidContractCall::Deactivate)),
        "{:?}",
        contract.backend.recorded_calls()
    );

    // Now install a "deactivated" ledger and confirm metadata reflects it.
    let mut deactivated_ledger = initial_ledger();
    deactivated_ledger.deactivated = true;
    deactivated_ledger.active = false;
    contract.backend.set_snapshot(deactivated_ledger);

    let resolved = resolve(&contract).await.unwrap().expect("resolves");
    assert_eq!(resolved.did_document_metadata.deactivated, Some(true));
}

// ===========================================================================
// Pure-function tests against the same fixtures (no contract, no async).
// These pin the resolver mapper layer on its own, separate from the
// `resolve` orchestrator. Useful as a regression net if the resolver gains
// pre/post hooks.
// ===========================================================================

#[test]
fn ledger_state_to_did_document_initial_matches_fixture() {
    let s = initial_ledger();
    let doc = ledger_state_to_did_document(&s, MidnightNetwork::Undeployed, ADDR).unwrap();
    let meta = ledger_state_to_metadata(&s);
    let actual = serde_json::json!({
        "didDocument": dump(&doc),
        "didDocumentMetadata": dump(&meta),
    });
    let expected = load_fixture("initial-state.json");
    assert_eq!(actual, expected);
}

#[test]
fn ledger_state_to_did_document_with_vm_matches_fixture() {
    let s = vm_added_ledger();
    let doc = ledger_state_to_did_document(&s, MidnightNetwork::Undeployed, ADDR).unwrap();
    let meta = ledger_state_to_metadata(&s);
    let actual = serde_json::json!({
        "didDocument": dump(&doc),
        "didDocumentMetadata": dump(&meta),
    });
    let expected = load_fixture("after-set-verification-method.json");
    assert_eq!(actual, expected);
}

// ===========================================================================
// Extended TS fixture parity: one fixture per remaining contract mutation.
// Each fixture mirrors what `LedgerToDomain.ledgerStateToDIDDocument` +
// `ledgerStateToMetadata` would produce for a deterministic post-mutation
// ledger snapshot. The Rust tests drive the api layer through the recording
// mock contract, install the corresponding ledger snapshot, then assert the
// resolver output is structurally identical to the fixture.
//
// Authoritative TS reference:
//   packages/did/src/ledger-to-domain.ts  (LedgerToDomain.*)
//   packages/api/src/ledger-mappers.ts    (verification method / service / aka)
// ===========================================================================

/// Reusable Ed25519 verification method on `#key-1` with all-zero coords.
fn vm_key_1_ed25519_ledger() -> LedgerVerificationMethod {
    LedgerVerificationMethod {
        id: "#key-1".into(),
        typ: VerificationMethodType::JsonWebKey,
        public_key_jwk: LedgerPublicKeyJwk {
            kty: KeyType::OKP,
            crv: CurveType::Ed25519,
            x: encode_base64url(&[0u8; 32]),
            y: String::new(),
        },
    }
}

#[allow(dead_code)]
fn vm_key_1_ed25519_domain() -> VerificationMethod {
    VerificationMethod::new(NewVerificationMethod {
        id: format!("{}#key-1", did_subject()),
        type_: VerificationMethodType::JsonWebKey,
        controller: did_subject(),
        public_key_jwk: PublicKeyJwk::new(NewPublicKeyJwk {
            kty: KeyType::OKP,
            crv: CurveType::Ed25519,
            x: encode_base64url(&[0u8; 32]),
            y: None,
            extensions: BTreeMap::new(),
        })
        .unwrap(),
    })
    .unwrap()
}

// ---------------------------------------------------------------------------
// alsoKnownAs (insert / remove)
// ---------------------------------------------------------------------------

/// `setAlsoKnownAs(uri, Insert)` — mirrors the TS "should add alsoKnownAs"
/// case after the ledger reflects one inserted alias.
#[tokio::test]
async fn after_set_aka_insert_matches_ts_fixture() {
    let contract = contract_with(MidnightNetwork::Undeployed, initial_ledger());
    add_also_known_as(&contract, "did:example:aka-1").await.unwrap();

    let mut post = initial_ledger();
    post.also_known_as = vec!["did:example:aka-1".into()];
    post.version = 2;
    post.operation_count = 1;
    post.updated_ms = 1_700_002_800_000; // 2023-11-14T23:00:00Z
    contract.backend.set_snapshot(post);

    let resolved = resolve(&contract).await.unwrap().expect("resolves");
    let actual = serde_json::json!({
        "didDocument": dump(&resolved.did_document),
        "didDocumentMetadata": dump(&resolved.did_document_metadata),
    });
    let expected = load_fixture("after-set-aka-insert.json");
    assert_eq!(actual, expected);
}

/// `setAlsoKnownAs(uri, Remove)` — after the alias is gone, the
/// `alsoKnownAs` field collapses away (mirrors `LedgerToDomain` skipping
/// empty sets) and `versionId` advances to 3.
#[tokio::test]
async fn after_set_aka_remove_matches_ts_fixture() {
    let contract = contract_with(MidnightNetwork::Undeployed, {
        let mut s = initial_ledger();
        s.also_known_as = vec!["did:example:aka-1".into()];
        s.version = 2;
        s
    });
    remove_also_known_as(&contract, "did:example:aka-1").await.unwrap();

    let mut post = initial_ledger();
    post.version = 3;
    post.operation_count = 2;
    post.updated_ms = 1_700_003_700_000; // 2023-11-14T23:15:00Z
    contract.backend.set_snapshot(post);

    let resolved = resolve(&contract).await.unwrap().expect("resolves");
    let actual = serde_json::json!({
        "didDocument": dump(&resolved.did_document),
        "didDocumentMetadata": dump(&resolved.did_document_metadata),
    });
    let expected = load_fixture("after-set-aka-remove.json");
    assert_eq!(actual, expected);
}

// ---------------------------------------------------------------------------
// verificationMethod update / remove
// ---------------------------------------------------------------------------

/// `setVerificationMethod(vm, Update)` — replace the JWK `x` for the
/// already-existing `#key-1`. The fixture has the new base64url `x`.
#[tokio::test]
async fn after_set_vm_update_matches_ts_fixture() {
    let mut pre = initial_ledger();
    pre.verification_methods
        .insert("#key-1".into(), vm_key_1_ed25519_ledger());
    pre.version = 3;
    pre.operation_count = 2;
    let contract = contract_with(MidnightNetwork::Undeployed, pre);

    // Update: same id, new coord bytes (0x77…).
    let new_x = encode_base64url(&[0x77u8; 32]);
    let updated_vm = VerificationMethod::new(NewVerificationMethod {
        id: format!("{}#key-1", did_subject()),
        type_: VerificationMethodType::JsonWebKey,
        controller: did_subject(),
        public_key_jwk: PublicKeyJwk::new(NewPublicKeyJwk {
            kty: KeyType::OKP,
            crv: CurveType::Ed25519,
            x: new_x.clone(),
            y: None,
            extensions: BTreeMap::new(),
        })
        .unwrap(),
    })
    .unwrap();
    update_verification_method(&contract, &updated_vm).await.unwrap();

    let mut post = initial_ledger();
    post.verification_methods.insert(
        "#key-1".into(),
        LedgerVerificationMethod {
            id: "#key-1".into(),
            typ: VerificationMethodType::JsonWebKey,
            public_key_jwk: LedgerPublicKeyJwk {
                kty: KeyType::OKP,
                crv: CurveType::Ed25519,
                x: new_x,
                y: String::new(),
            },
        },
    );
    post.version = 4;
    post.operation_count = 3;
    post.updated_ms = 1_700_002_800_000;
    contract.backend.set_snapshot(post);

    let resolved = resolve(&contract).await.unwrap().expect("resolves");
    let actual = serde_json::json!({
        "didDocument": dump(&resolved.did_document),
        "didDocumentMetadata": dump(&resolved.did_document_metadata),
    });
    let expected = load_fixture("after-set-vm-update.json");
    assert_eq!(actual, expected);
}

/// `removeVerificationMethod(id)` — also purges any relations referencing
/// the method. After removal the document has no `verificationMethod`
/// array and no relation arrays.
#[tokio::test]
async fn after_remove_vm_matches_ts_fixture() {
    let mut pre = initial_ledger();
    pre.verification_methods
        .insert("#key-1".into(), vm_key_1_ed25519_ledger());
    pre.authentication_relation = vec!["#key-1".into()];
    pre.version = 4;
    pre.operation_count = 3;
    let contract = contract_with(MidnightNetwork::Undeployed, pre);

    remove_verification_method(&contract, "#key-1").await.unwrap();

    let mut post = initial_ledger();
    post.version = 5;
    post.operation_count = 5; // relation purge + remove vm
    post.updated_ms = 1_700_003_700_000;
    contract.backend.set_snapshot(post);

    let resolved = resolve(&contract).await.unwrap().expect("resolves");
    let actual = serde_json::json!({
        "didDocument": dump(&resolved.did_document),
        "didDocumentMetadata": dump(&resolved.did_document_metadata),
    });
    let expected = load_fixture("after-remove-vm.json");
    assert_eq!(actual, expected);
}

// ---------------------------------------------------------------------------
// Schnorr-Jubjub verification methods
// ---------------------------------------------------------------------------

/// `setSchnorrJubjubVerificationMethod(vm, Insert)` — fixture shows the EC
/// + Jubjub JWK reconstruction performed by
/// `LedgerToDomain.schnorrJubjubPkToJwk`. The hex coords `01` / `02` are
/// right-padded to 32 bytes then base64url-encoded.
#[tokio::test]
async fn after_set_schnorr_jubjub_vm_insert_matches_ts_fixture() {
    let contract = contract_with(MidnightNetwork::Undeployed, initial_ledger());

    let vm = SchnorrJubjubVerificationMethod {
        id: "#jub-1".into(),
        public_key: JubjubPointHex {
            x: "01".into(),
            y: "02".into(),
        },
    };
    add_schnorr_jubjub_verification_method(&contract, &vm).await.unwrap();

    let mut post = initial_ledger();
    post.schnorr_jubjub_verification_methods.insert(
        "#jub-1".into(),
        LedgerSchnorrJubjubVerificationMethod {
            id: "#jub-1".into(),
            public_key: JubjubPointHex {
                x: "01".into(),
                y: "02".into(),
            },
        },
    );
    post.version = 2;
    post.operation_count = 1;
    post.updated_ms = 1_700_002_800_000;
    contract.backend.set_snapshot(post);

    let resolved = resolve(&contract).await.unwrap().expect("resolves");
    let actual = serde_json::json!({
        "didDocument": dump(&resolved.did_document),
        "didDocumentMetadata": dump(&resolved.did_document_metadata),
    });
    let expected = load_fixture("after-set-schnorr-jubjub-vm-insert.json");
    assert_eq!(actual, expected);
}

/// `removeSchnorrJubjubVerificationMethod(id)`.
#[tokio::test]
async fn after_remove_schnorr_jubjub_vm_matches_ts_fixture() {
    let mut pre = initial_ledger();
    pre.schnorr_jubjub_verification_methods.insert(
        "#jub-1".into(),
        LedgerSchnorrJubjubVerificationMethod {
            id: "#jub-1".into(),
            public_key: JubjubPointHex {
                x: "01".into(),
                y: "02".into(),
            },
        },
    );
    pre.version = 2;
    pre.operation_count = 1;
    let contract = contract_with(MidnightNetwork::Undeployed, pre);

    remove_schnorr_jubjub_verification_method(&contract, "#jub-1")
        .await
        .unwrap();

    let mut post = initial_ledger();
    post.version = 3;
    post.operation_count = 2;
    post.updated_ms = 1_700_003_700_000;
    contract.backend.set_snapshot(post);

    let resolved = resolve(&contract).await.unwrap().expect("resolves");
    let actual = serde_json::json!({
        "didDocument": dump(&resolved.did_document),
        "didDocumentMetadata": dump(&resolved.did_document_metadata),
    });
    let expected = load_fixture("after-remove-schnorr-jubjub-vm.json");
    assert_eq!(actual, expected);
}

// ---------------------------------------------------------------------------
// verificationMethodRelation insert (authentication)
// ---------------------------------------------------------------------------

/// `setVerificationMethodRelation(Authentication, "#key-1", Insert)` —
/// fixture shows the relation array with the fragment-form key id.
#[tokio::test]
async fn after_set_vm_relation_insert_matches_ts_fixture() {
    let mut pre = initial_ledger();
    pre.verification_methods
        .insert("#key-1".into(), vm_key_1_ed25519_ledger());
    pre.version = 2;
    pre.operation_count = 1;
    let contract = contract_with(MidnightNetwork::Undeployed, pre);

    add_verification_method_relation(&contract, VerificationMethodRelation::Authentication, "#key-1")
        .await
        .unwrap();

    let mut post = initial_ledger();
    post.verification_methods
        .insert("#key-1".into(), vm_key_1_ed25519_ledger());
    post.authentication_relation = vec!["#key-1".into()];
    post.version = 3;
    post.operation_count = 2;
    post.updated_ms = 1_700_002_800_000;
    contract.backend.set_snapshot(post);

    let resolved = resolve(&contract).await.unwrap().expect("resolves");
    let actual = serde_json::json!({
        "didDocument": dump(&resolved.did_document),
        "didDocumentMetadata": dump(&resolved.did_document_metadata),
    });
    let expected = load_fixture("after-set-vm-relation-insert.json");
    assert_eq!(actual, expected);
}

// ---------------------------------------------------------------------------
// service insert / remove
// ---------------------------------------------------------------------------

/// `setService(svc, Insert)` — single URI endpoint, single-string type.
/// `parseServiceEndpoint` round-trips the JSON-encoded URI back to a bare
/// string, so the fixture's `serviceEndpoint` is just the URL.
#[tokio::test]
async fn after_set_service_insert_matches_ts_fixture() {
    let contract = contract_with(MidnightNetwork::Undeployed, initial_ledger());

    let svc = Service::new(NewService {
        id: "svc-1".into(),
        type_: ServiceType::One("DIDCommMessaging".into()),
        service_endpoint: ServiceEndpoint::Uri("https://example.com/didcomm".into()),
    })
    .unwrap();
    add_service(&contract, &svc).await.unwrap();

    let mut post = initial_ledger();
    post.services.insert(
        "#svc-1".into(),
        LedgerService {
            id: "#svc-1".into(),
            typ: "DIDCommMessaging".into(),
            service_endpoint: "\"https://example.com/didcomm\"".into(),
        },
    );
    post.version = 2;
    post.operation_count = 1;
    post.updated_ms = 1_700_002_800_000;
    contract.backend.set_snapshot(post);

    let resolved = resolve(&contract).await.unwrap().expect("resolves");
    let actual = serde_json::json!({
        "didDocument": dump(&resolved.did_document),
        "didDocumentMetadata": dump(&resolved.did_document_metadata),
    });
    let expected = load_fixture("after-set-service-insert.json");
    assert_eq!(actual, expected);
}

/// `removeService(id)` — after removal the document has no `service` array.
#[tokio::test]
async fn after_remove_service_matches_ts_fixture() {
    let mut pre = initial_ledger();
    pre.services.insert(
        "#svc-1".into(),
        LedgerService {
            id: "#svc-1".into(),
            typ: "DIDCommMessaging".into(),
            service_endpoint: "\"https://example.com/didcomm\"".into(),
        },
    );
    pre.version = 2;
    pre.operation_count = 1;
    let contract = contract_with(MidnightNetwork::Undeployed, pre);

    remove_service(&contract, "svc-1").await.unwrap();

    let mut post = initial_ledger();
    post.version = 3;
    post.operation_count = 2;
    post.updated_ms = 1_700_003_700_000;
    contract.backend.set_snapshot(post);

    let resolved = resolve(&contract).await.unwrap().expect("resolves");
    let actual = serde_json::json!({
        "didDocument": dump(&resolved.did_document),
        "didDocumentMetadata": dump(&resolved.did_document_metadata),
    });
    let expected = load_fixture("after-remove-service.json");
    assert_eq!(actual, expected);
}

// ---------------------------------------------------------------------------
// deactivate
// ---------------------------------------------------------------------------

/// `deactivate()` — `LedgerToDomain.ledgerStateToMetadata` flips
/// `deactivated: true` whenever `deactivated || !active` holds. Document
/// body itself is unchanged.
#[tokio::test]
async fn after_deactivate_matches_ts_fixture() {
    let contract = contract_with(MidnightNetwork::Undeployed, initial_ledger());
    deactivate(&contract).await.unwrap();

    let mut post = initial_ledger();
    post.deactivated = true;
    post.active = false;
    post.version = 2;
    post.operation_count = 1;
    post.updated_ms = 1_700_002_800_000;
    contract.backend.set_snapshot(post);

    let resolved = resolve(&contract).await.unwrap().expect("resolves");
    let actual = serde_json::json!({
        "didDocument": dump(&resolved.did_document),
        "didDocumentMetadata": dump(&resolved.did_document_metadata),
    });
    let expected = load_fixture("after-deactivate.json");
    assert_eq!(actual, expected);
}
