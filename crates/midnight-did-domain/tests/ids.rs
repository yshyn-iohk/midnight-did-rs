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

//! Integration tests for the W3C-DID newtypes: `DidKeyId`,
//! `FragmentId`, `ServiceId`. R1 step 3.
//!
//! Each newtype:
//! - is constructed via a fallible `new()` (rejects invalid grammar);
//! - has a `Deserialize` impl that delegates to `new()` (so JSON
//!   round-trip validates);
//! - has a transparent `Serialize` impl (wire format matches a plain
//!   string);
//! - rejects empties / whitespace.
//!
//! These tests pin the negative cases (must reject) explicitly — the
//! whole point of these newtypes is that invalid inputs cannot reach
//! call sites.

use midnight_did_domain::ids::{DidKeyId, FragmentId, IdError, ServiceId};

// ---- DidKeyId ------------------------------------------------------

#[test]
fn did_key_id_accepts_valid_did_with_fragment() {
    let id = DidKeyId::new("did:midnight:testnet:abc#key-1").expect("valid");
    assert_eq!(id.as_str(), "did:midnight:testnet:abc#key-1");
    assert_eq!(id.fragment(), "key-1");
}

#[test]
fn did_key_id_rejects_missing_fragment() {
    let err = DidKeyId::new("did:midnight:testnet:abc").unwrap_err();
    assert!(matches!(err, IdError::MissingDidFragment));
}

#[test]
fn did_key_id_rejects_empty_string() {
    let err = DidKeyId::new("").unwrap_err();
    assert!(matches!(err, IdError::Empty));
}

#[test]
fn did_key_id_rejects_non_did_prefix() {
    let err = DidKeyId::new("https://example.com#key-1").unwrap_err();
    assert!(matches!(err, IdError::InvalidDidUri(_)));
}

#[test]
fn did_key_id_serialize_round_trips_through_json() {
    let id = DidKeyId::new("did:midnight:testnet:abc#key-1").unwrap();
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "\"did:midnight:testnet:abc#key-1\"");
    let back: DidKeyId = serde_json::from_str(&json).unwrap();
    assert_eq!(back, id);
}

#[test]
fn did_key_id_deserialize_validates() {
    // The Deserialize impl must run the same validation as ::new.
    let bad_json = "\"not-a-did\"";
    let err = serde_json::from_str::<DidKeyId>(bad_json).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("DID") || msg.contains("did"));
}

#[test]
fn did_key_id_fragment_extracts_after_hash() {
    let id = DidKeyId::new("did:midnight:devnet:xyz#assertion-1").unwrap();
    assert_eq!(id.fragment(), "assertion-1");
}

// ---- FragmentId ----------------------------------------------------

#[test]
fn fragment_id_accepts_valid_hash_prefix() {
    let f = FragmentId::new("#key-1").expect("valid");
    assert_eq!(f.as_str(), "#key-1");
}

#[test]
fn fragment_id_rejects_missing_hash_prefix() {
    let err = FragmentId::new("key-1").unwrap_err();
    assert!(matches!(err, IdError::MissingFragmentPrefix));
}

#[test]
fn fragment_id_rejects_empty() {
    let err = FragmentId::new("").unwrap_err();
    assert!(matches!(err, IdError::Empty));
}

#[test]
fn fragment_id_rejects_just_hash() {
    // "#" alone has the prefix but nothing after — reject as empty
    // fragment.
    let err = FragmentId::new("#").unwrap_err();
    assert!(matches!(err, IdError::Empty));
}

#[test]
fn fragment_id_rejects_whitespace() {
    let err = FragmentId::new("# spaces").unwrap_err();
    assert!(matches!(err, IdError::BadFragmentChars));
}

#[test]
fn fragment_id_rejects_control_chars() {
    let err = FragmentId::new("#key\n1").unwrap_err();
    assert!(matches!(err, IdError::BadFragmentChars));
}

#[test]
fn fragment_id_serialize_is_transparent() {
    let f = FragmentId::new("#key-1").unwrap();
    let json = serde_json::to_string(&f).unwrap();
    assert_eq!(json, "\"#key-1\"");
}

#[test]
fn fragment_id_deserialize_validates_prefix() {
    let bad_json = "\"key-without-hash\"";
    let err = serde_json::from_str::<FragmentId>(bad_json).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("fragment"));
}

#[test]
fn fragment_id_round_trips_through_json() {
    let f = FragmentId::new("#assertion-1").unwrap();
    let json = serde_json::to_string(&f).unwrap();
    let back: FragmentId = serde_json::from_str(&json).unwrap();
    assert_eq!(back, f);
}

// ---- ServiceId -----------------------------------------------------

#[test]
fn service_id_accepts_arbitrary_uri() {
    // W3C DID core does NOT prescribe the structure of service IDs;
    // they're opaque to the resolver. We accept any non-empty,
    // non-whitespace-only string.
    let s = ServiceId::new("did:midnight:devnet:abc#linked-domain").unwrap();
    assert_eq!(s.as_str(), "did:midnight:devnet:abc#linked-domain");
}

#[test]
fn service_id_accepts_opaque_fragment() {
    let s = ServiceId::new("#my-service-1").unwrap();
    assert_eq!(s.as_str(), "#my-service-1");
}

#[test]
fn service_id_rejects_empty() {
    let err = ServiceId::new("").unwrap_err();
    assert!(matches!(err, IdError::Empty));
}

#[test]
fn service_id_rejects_whitespace_only() {
    let err = ServiceId::new("   ").unwrap_err();
    assert!(matches!(err, IdError::Empty));
}

#[test]
fn service_id_round_trips_through_json() {
    let s = ServiceId::new("#linked-domain").unwrap();
    let json = serde_json::to_string(&s).unwrap();
    let back: ServiceId = serde_json::from_str(&json).unwrap();
    assert_eq!(back, s);
}

// ---- Cross-newtype ergonomics --------------------------------------

#[test]
fn newtypes_implement_display() {
    let key = DidKeyId::new("did:midnight:devnet:abc#k").unwrap();
    let frag = FragmentId::new("#k").unwrap();
    let svc = ServiceId::new("#s").unwrap();
    assert_eq!(format!("{key}"), "did:midnight:devnet:abc#k");
    assert_eq!(format!("{frag}"), "#k");
    assert_eq!(format!("{svc}"), "#s");
}

#[test]
fn newtypes_implement_hash_and_eq() {
    use std::collections::HashSet;
    let a = FragmentId::new("#k").unwrap();
    let b = FragmentId::new("#k").unwrap();
    let mut set = HashSet::new();
    set.insert(a);
    assert!(set.contains(&b));
}
