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

//! Integration tests for the fallible composite-type constructors
//! introduced in R1 step 4a: `VerificationMethod::new`,
//! `Service::new`, `PublicKeyJwk::new`. Each runs the same validation
//! the legacy `.validate()` method ran, but lifts that check into the
//! construction path — once a value of these types exists, its
//! invariants are proven.
//!
//! These tests cover BOTH the happy path AND the explicit reject path
//! for each constructor — invalid inputs must error, not silently
//! construct.

use std::collections::BTreeMap;

use midnight_did_domain::did_document::{
    CurveType, KeyType, NewPublicKeyJwk, NewService, NewVerificationMethod, PublicKeyJwk, Service, ServiceEndpoint,
    ServiceType, VerificationMethod, VerificationMethodType,
};
use serde_json::json;

// ---- PublicKeyJwk --------------------------------------------------

fn valid_ed25519_jwk() -> NewPublicKeyJwk {
    NewPublicKeyJwk {
        kty: KeyType::OKP,
        crv: CurveType::Ed25519,
        x: "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo".to_string(),
        y: None,
        extensions: BTreeMap::new(),
    }
}

#[test]
fn public_key_jwk_new_accepts_valid_ed25519() {
    let jwk = PublicKeyJwk::new(valid_ed25519_jwk()).expect("valid OKP/Ed25519 JWK");
    assert_eq!(jwk.kty(), KeyType::OKP);
    assert_eq!(jwk.crv(), CurveType::Ed25519);
}

#[test]
fn public_key_jwk_new_rejects_okp_with_y_coord() {
    let bad = NewPublicKeyJwk {
        kty: KeyType::OKP,
        crv: CurveType::Ed25519,
        x: "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo".to_string(),
        y: Some("not-allowed".to_string()),
        extensions: BTreeMap::new(),
    };
    let err = PublicKeyJwk::new(bad).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("OKP keys must not include a y coordinate"), "got: {msg}");
}

#[test]
fn public_key_jwk_new_rejects_okp_with_wrong_curve() {
    let bad = NewPublicKeyJwk {
        kty: KeyType::OKP,
        crv: CurveType::P256, // P-256 is EC-only
        x: "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo".to_string(),
        y: None,
        extensions: BTreeMap::new(),
    };
    let err = PublicKeyJwk::new(bad).unwrap_err();
    assert!(format!("{err}").contains("OKP keys must use"));
}

#[test]
fn public_key_jwk_new_rejects_private_key_material() {
    let mut ext = BTreeMap::new();
    ext.insert("d".to_string(), json!("private-key-here"));
    let bad = NewPublicKeyJwk {
        kty: KeyType::OKP,
        crv: CurveType::Ed25519,
        x: "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo".to_string(),
        y: None,
        extensions: ext,
    };
    let err = PublicKeyJwk::new(bad).unwrap_err();
    assert!(format!("{err}").contains("publicKeyJwk must not include private key material"));
}

#[test]
fn public_key_jwk_new_rejects_ec_without_y_coord() {
    let bad = NewPublicKeyJwk {
        kty: KeyType::EC,
        crv: CurveType::P256,
        x: "uK6_5ZE2zfYsq_iVN6STWPyzMQpFOTECpfOzORzxAhc".to_string(),
        y: None,
        extensions: BTreeMap::new(),
    };
    let err = PublicKeyJwk::new(bad).unwrap_err();
    assert!(format!("{err}").contains("Non-OKP keys must include a y coordinate"));
}

#[test]
fn public_key_jwk_deserialize_validates() {
    // OKP with a y coordinate must be rejected during JSON
    // deserialisation — not silently accepted then later validated.
    let bad_json = serde_json::json!({
        "kty": "OKP",
        "crv": "Ed25519",
        "x": "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo",
        "y": "not-allowed"
    });
    let result: Result<PublicKeyJwk, _> = serde_json::from_value(bad_json);
    assert!(result.is_err(), "Deserialize should reject invalid JWK");
}

#[test]
fn public_key_jwk_deserialize_accepts_valid() {
    let good_json = serde_json::json!({
        "kty": "OKP",
        "crv": "Ed25519",
        "x": "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo"
    });
    let jwk: PublicKeyJwk = serde_json::from_value(good_json).expect("valid");
    assert_eq!(jwk.kty(), KeyType::OKP);
}

// ---- VerificationMethod -------------------------------------------

#[test]
fn verification_method_new_accepts_valid_input() {
    let vm = VerificationMethod::new(NewVerificationMethod {
        id: "did:midnight:testnet:abc#key-1".to_string(),
        type_: VerificationMethodType::JsonWebKey,
        controller: "did:midnight:testnet:abc".to_string(),
        public_key_jwk: PublicKeyJwk::new(valid_ed25519_jwk()).unwrap(),
    })
    .expect("valid");
    assert_eq!(vm.id().as_str(), "did:midnight:testnet:abc#key-1");
}

#[test]
fn verification_method_new_rejects_id_without_fragment() {
    let err = VerificationMethod::new(NewVerificationMethod {
        id: "did:midnight:testnet:abc".to_string(), // no #fragment
        type_: VerificationMethodType::JsonWebKey,
        controller: "did:midnight:testnet:abc".to_string(),
        public_key_jwk: PublicKeyJwk::new(valid_ed25519_jwk()).unwrap(),
    })
    .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("DID Key ID") || msg.contains("fragment"));
}

#[test]
fn verification_method_new_rejects_bad_controller_did() {
    let err = VerificationMethod::new(NewVerificationMethod {
        id: "did:midnight:testnet:abc#key-1".to_string(),
        type_: VerificationMethodType::JsonWebKey,
        controller: "not-a-did".to_string(),
        public_key_jwk: PublicKeyJwk::new(valid_ed25519_jwk()).unwrap(),
    })
    .unwrap_err();
    assert!(format!("{err}").contains("DID"));
}

// ---- Service -------------------------------------------------------

#[test]
fn service_new_accepts_valid_did_url_id() {
    let svc = Service::new(NewService {
        id: "did:midnight:testnet:abc#linked-domain".to_string(),
        type_: ServiceType::One("LinkedDomains".to_string()),
        service_endpoint: ServiceEndpoint::Uri("https://example.com".to_string()),
    })
    .expect("valid");
    assert_eq!(svc.id(), "did:midnight:testnet:abc#linked-domain");
}

#[test]
fn service_new_accepts_relative_reference_id() {
    let svc = Service::new(NewService {
        id: "#linked-domain".to_string(),
        type_: ServiceType::One("LinkedDomains".to_string()),
        service_endpoint: ServiceEndpoint::Uri("https://example.com".to_string()),
    })
    .expect("relative refs valid per W3C DID Core");
    assert_eq!(svc.id(), "#linked-domain");
}

#[test]
fn service_new_rejects_empty_type() {
    let err = Service::new(NewService {
        id: "#x".to_string(),
        type_: ServiceType::One("".to_string()),
        service_endpoint: ServiceEndpoint::Uri("https://example.com".to_string()),
    })
    .unwrap_err();
    assert!(format!("{err}").contains("service type"));
}

#[test]
fn service_new_rejects_empty_type_array() {
    let err = Service::new(NewService {
        id: "#x".to_string(),
        type_: ServiceType::Many(vec![]),
        service_endpoint: ServiceEndpoint::Uri("https://example.com".to_string()),
    })
    .unwrap_err();
    assert!(format!("{err}").contains("non-empty array"));
}

#[test]
fn service_new_rejects_invalid_id_format() {
    let err = Service::new(NewService {
        id: "ftp://not-a-did-url".to_string(),
        type_: ServiceType::One("LinkedDomains".to_string()),
        service_endpoint: ServiceEndpoint::Uri("https://example.com".to_string()),
    })
    .unwrap_err();
    assert!(format!("{err}").contains("DID URL") || format!("{err}").contains("relative reference"));
}

// R1 step 4c: the legacy `create_verification_method` /
// `create_service` factories have been retired. Their behaviour is
// covered by the `VerificationMethod::new` / `Service::new` happy-path
// tests above; this section formerly held redundant pinning tests for
// the deprecated free-function path.
