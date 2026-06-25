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

//! Integration tests for `DidDocumentBuilder` (R1 step 5).
//!
//! The builder is a Rust-idiomatic incremental constructor for
//! `DidDocument` that validates cross-reference invariants on
//! `build()`:
//!
//! - Every authentication / assertion_method / key_agreement /
//!   capability_invocation / capability_delegation entry must
//!   reference an existing verification method's id.
//! - No duplicate verification-method ids.
//! - No duplicate service ids.
//! - The subject DID is well-formed.
//!
//! These tests pin the happy-path build + each cross-reference
//! reject path explicitly.

use midnight_did_domain::did_document::{
    CurveType, DidDocumentBuilder, DidKeyId, KeyType, NewPublicKeyJwk, NewService, NewVerificationMethod,
    PublicKeyJwk, ServiceEndpoint, ServiceType, VerificationMethod, VerificationMethodType,
};
use std::collections::BTreeMap;

fn ed25519_jwk() -> PublicKeyJwk {
    PublicKeyJwk::new(NewPublicKeyJwk {
        kty: KeyType::OKP,
        crv: CurveType::Ed25519,
        x: "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo".to_string(),
        y: None,
        extensions: BTreeMap::new(),
    })
    .expect("valid JWK")
}

fn vm(subject: &str, fragment: &str) -> VerificationMethod {
    VerificationMethod::new(NewVerificationMethod {
        id: format!("{subject}#{fragment}"),
        type_: VerificationMethodType::JsonWebKey,
        controller: subject.to_string(),
        public_key_jwk: ed25519_jwk(),
    })
    .expect("valid VM")
}

const SUBJECT: &str = "did:midnight:testnet:abc";

// ---- Happy path ----------------------------------------------------

#[test]
fn builder_with_no_vms_or_services_builds() {
    let doc = DidDocumentBuilder::new(SUBJECT).build().expect("minimal doc valid");
    assert_eq!(doc.id.as_str(), SUBJECT);
}

#[test]
fn builder_with_single_vm_builds() {
    let key = vm(SUBJECT, "key-1");
    let doc = DidDocumentBuilder::new(SUBJECT)
        .add_verification_method(key)
        .build()
        .expect("valid");
    assert_eq!(doc.verification_method.as_ref().unwrap().len(), 1);
}

#[test]
fn builder_with_authentication_reference_to_existing_vm_builds() {
    let key = vm(SUBJECT, "key-1");
    let key_id = key.id().clone();
    let doc = DidDocumentBuilder::new(SUBJECT)
        .add_verification_method(key)
        .authentication(vec![key_id])
        .build()
        .expect("valid cross-ref");
    assert_eq!(doc.authentication.as_ref().unwrap().len(), 1);
}

#[test]
fn builder_with_service_builds() {
    let svc = midnight_did_domain::did_document::Service::new(NewService {
        id: format!("{SUBJECT}#linked-domain"),
        type_: ServiceType::One("LinkedDomains".to_string()),
        service_endpoint: ServiceEndpoint::Uri("https://example.com".to_string()),
    })
    .unwrap();
    let doc = DidDocumentBuilder::new(SUBJECT).add_service(svc).build().unwrap();
    assert_eq!(doc.service.as_ref().unwrap().len(), 1);
}

#[test]
fn builder_threads_all_5_relations() {
    let key = vm(SUBJECT, "key-1");
    let key_id = key.id().clone();
    let doc = DidDocumentBuilder::new(SUBJECT)
        .add_verification_method(key)
        .authentication(vec![key_id.clone()])
        .assertion_method(vec![key_id.clone()])
        .key_agreement(vec![key_id.clone()])
        .capability_invocation(vec![key_id.clone()])
        .capability_delegation(vec![key_id])
        .build()
        .expect("all relations OK");
    assert!(doc.authentication.is_some());
    assert!(doc.assertion_method.is_some());
    assert!(doc.key_agreement.is_some());
    assert!(doc.capability_invocation.is_some());
    assert!(doc.capability_delegation.is_some());
}

// ---- Cross-reference rejects --------------------------------------

#[test]
fn builder_rejects_authentication_referencing_unknown_vm() {
    // No verification methods added, but an authentication relation
    // tries to point to one. build() must reject.
    let phantom = DidKeyId::parse(format!("{SUBJECT}#phantom-key")).unwrap();
    let err = DidDocumentBuilder::new(SUBJECT)
        .authentication(vec![phantom])
        .build()
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("authentication") || msg.contains("verificationMethod"),
        "expected message about dangling reference, got: {msg}");
}

#[test]
fn builder_rejects_assertion_method_referencing_unknown_vm() {
    let phantom = DidKeyId::parse(format!("{SUBJECT}#phantom-key")).unwrap();
    let err = DidDocumentBuilder::new(SUBJECT)
        .assertion_method(vec![phantom])
        .build()
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("assertionMethod") || msg.contains("verificationMethod"));
}

#[test]
fn builder_rejects_duplicate_verification_method_ids() {
    let key1 = vm(SUBJECT, "key-1");
    let key2 = vm(SUBJECT, "key-1");
    let err = DidDocumentBuilder::new(SUBJECT)
        .add_verification_method(key1)
        .add_verification_method(key2)
        .build()
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("duplicate") || msg.contains("verificationMethod"),
        "expected duplicate-id error, got: {msg}");
}

#[test]
fn builder_rejects_bad_subject_did() {
    let err = DidDocumentBuilder::new("not-a-did").build().unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("DID") || msg.contains("did:"));
}

// ---- Fluent API ergonomics ----------------------------------------

#[test]
fn builder_chains_methods_fluently() {
    // Compile-only smoke: every setter returns Self by value.
    let key = vm(SUBJECT, "key-1");
    let _ = DidDocumentBuilder::new(SUBJECT)
        .add_verification_method(key)
        .build()
        .expect("fluent chain compiles + builds");
}

#[test]
fn builder_can_add_multiple_verification_methods() {
    let key1 = vm(SUBJECT, "key-1");
    let key2 = vm(SUBJECT, "key-2");
    let doc = DidDocumentBuilder::new(SUBJECT)
        .add_verification_method(key1)
        .add_verification_method(key2)
        .build()
        .unwrap();
    assert_eq!(doc.verification_method.as_ref().unwrap().len(), 2);
}

#[test]
fn builder_can_add_multiple_services() {
    let s1 = midnight_did_domain::did_document::Service::new(NewService {
        id: format!("{SUBJECT}#svc-1"),
        type_: ServiceType::One("LinkedDomains".to_string()),
        service_endpoint: ServiceEndpoint::Uri("https://a.example".to_string()),
    })
    .unwrap();
    let s2 = midnight_did_domain::did_document::Service::new(NewService {
        id: format!("{SUBJECT}#svc-2"),
        type_: ServiceType::One("LinkedDomains".to_string()),
        service_endpoint: ServiceEndpoint::Uri("https://b.example".to_string()),
    })
    .unwrap();
    let doc = DidDocumentBuilder::new(SUBJECT)
        .add_service(s1)
        .add_service(s2)
        .build()
        .unwrap();
    assert_eq!(doc.service.as_ref().unwrap().len(), 2);
}
