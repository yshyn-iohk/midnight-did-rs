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

//! W3C DID Core 1.0 data model and cross-consistency validation.
//!
//! Port of `did-document.ts`. Where the TypeScript implementation uses Zod
//! schemas, this crate exposes plain `serde`-friendly structs plus
//! `validate(&self) -> Result<(), Vec<ValidationIssue>>` methods. The
//! validation rules and message strings are kept close to the TS port so
//! that error output stays comparable across runtimes.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;

use crate::crypto_codecs::{CodecError, decode_base64url_bytes};
use crate::uri::normalize_uri_string;

// ---------------------------------------------------------------------------
// Validation primitives
// ---------------------------------------------------------------------------

/// A single validation problem with optional dot-joined path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    /// Human-readable description.
    pub message: String,
    /// Path of property accesses from the root of the validated value.
    pub path: Vec<String>,
}

impl ValidationIssue {
    /// Build a top-level issue with no path.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            path: Vec::new(),
        }
    }

    /// Build an issue rooted at `path`.
    pub fn at(message: impl Into<String>, path: Vec<String>) -> Self {
        Self {
            message: message.into(),
            path,
        }
    }
}

/// Aggregate validation error carrying every issue discovered during a single
/// `validate()` pass.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("validation failed: {summary}")]
pub struct ValidationError {
    /// Joined error summary (matches the TS error message shape).
    pub summary: String,
    /// Underlying issue list.
    pub issues: Vec<ValidationIssue>,
}

impl ValidationError {
    /// Build a [`ValidationError`] from a non-empty issue list.
    pub fn from_issues(issues: Vec<ValidationIssue>) -> Self {
        let summary = issues
            .iter()
            .map(|issue| {
                if issue.path.is_empty() {
                    issue.message.clone()
                } else {
                    format!("{} at {}", issue.message, issue.path.join("."))
                }
            })
            .collect::<Vec<_>>()
            .join("; ");
        Self { summary, issues }
    }
}

// ---------------------------------------------------------------------------
// DID string newtypes
// ---------------------------------------------------------------------------

/// `did:method:specific-id` URL string (may include path/query/fragment).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DidUrl(pub String);

/// Relative URL reference (no scheme, no leading `//`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RelativeUrl(pub String);

/// Bare DID string with no path/query/fragment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DidString(pub String);

/// DID Key ID — either a full DID URL `did:...#frag` or a relative `#frag` reference.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DidKeyId(pub String);

const URI_SCHEME_PREFIX_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
const URI_SCHEME_INNER_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+.-";

fn has_uri_scheme(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    if !URI_SCHEME_PREFIX_CHARS.contains(&bytes[0]) {
        return false;
    }
    for (idx, byte) in bytes.iter().enumerate().skip(1) {
        if *byte == b':' {
            return idx > 0;
        }
        if !URI_SCHEME_INNER_CHARS.contains(byte) {
            return false;
        }
    }
    false
}

fn is_relative_reference(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    if value.trim() != value {
        return false;
    }
    if has_uri_scheme(value) {
        return false;
    }
    if value.starts_with("//") {
        return false;
    }
    // Best-effort URL parse against a synthetic base — matches the TS check.
    url::Url::parse("https://example.org/")
        .and_then(|base| base.join(value))
        .is_ok()
}

fn is_key_fragment(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '%'))
}

fn extract_key_fragment(value: &str) -> &str {
    if let Some(rest) = value.strip_prefix('#') {
        return rest;
    }
    if value.starts_with("did:") {
        return match value.find('#') {
            Some(idx) => &value[idx + 1..],
            None => "",
        };
    }
    value
}

fn is_did_url(value: &str) -> bool {
    value.starts_with("did:") && value.len() >= 5 && value.split(':').count() >= 3
}

fn is_did_string(value: &str) -> bool {
    if !value.starts_with("did:") || value.len() < 5 {
        return false;
    }
    if value.split(':').count() < 3 {
        return false;
    }
    !value.chars().any(|c| matches!(c, '/' | '?' | '#'))
}

fn is_did_key_id(value: &str) -> bool {
    let union = is_did_url(value) || is_relative_reference(value);
    if !union {
        return false;
    }
    is_key_fragment(extract_key_fragment(value))
}

impl DidUrl {
    /// Validate a candidate DID URL.
    pub fn parse(value: impl Into<String>) -> Result<Self, ValidationError> {
        let s = value.into();
        if !is_did_url(&s) {
            return Err(ValidationError::from_issues(vec![ValidationIssue::new(
                "Invalid DID URL format",
            )]));
        }
        Ok(Self(s))
    }
}

impl RelativeUrl {
    /// Validate a candidate relative URL.
    pub fn parse(value: impl Into<String>) -> Result<Self, ValidationError> {
        let s = value.into();
        if !is_relative_reference(&s) {
            return Err(ValidationError::from_issues(vec![ValidationIssue::new(
                "Relative URL must be relative to the DID subject",
            )]));
        }
        Ok(Self(s))
    }
}

impl DidString {
    /// Validate a candidate bare DID string.
    pub fn parse(value: impl Into<String>) -> Result<Self, ValidationError> {
        let s = value.into();
        if !is_did_string(&s) {
            return Err(ValidationError::from_issues(vec![ValidationIssue::new(
                "Invalid DID format",
            )]));
        }
        Ok(Self(s))
    }
}

impl DidKeyId {
    /// Validate a candidate DID key id.
    pub fn parse(value: impl Into<String>) -> Result<Self, ValidationError> {
        let s = value.into();
        if !is_did_key_id(&s) {
            return Err(ValidationError::from_issues(vec![ValidationIssue::new(
                "Invalid DID Key ID format: invalid or missing fragment",
            )]));
        }
        Ok(Self(s))
    }
}

// ---------------------------------------------------------------------------
// Verification method enums
// ---------------------------------------------------------------------------

/// Verification method `type` keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VerificationMethodType {
    /// Sentinel for absent / unrecognised type. Matches the TS export.
    Undefined,
    /// `JsonWebKey` verification method as per the JOSE register.
    JsonWebKey,
}

/// JWK `kty` (key type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyType {
    /// Elliptic curve key.
    EC,
    /// RSA key.
    RSA,
    /// Symmetric octet key.
    #[allow(non_camel_case_types)]
    oct,
    /// CFRG octet key pair (Ed25519, X25519, …).
    OKP,
}

/// JWK `crv` (curve) — uses serde rename to keep the TS spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CurveType {
    /// Edwards curve used for EdDSA signatures.
    Ed25519,
    /// Montgomery curve used for ECDH.
    X25519,
    /// Midnight's Jubjub curve.
    Jubjub,
    /// NIST P-256 curve.
    #[serde(rename = "P-256")]
    P256,
    /// Bitcoin's secp256k1 curve.
    #[serde(rename = "secp256k1")]
    Secp256k1,
    /// BLS12-381 curve, group G1.
    BLS12381G1,
    /// BLS12-381 curve, group G2.
    BLS12381G2,
}

/// Verification relationship keyword (authentication, assertionMethod, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VerificationMethodRelation {
    /// Sentinel for absent / unrecognised relation.
    Undefined,
    /// `authentication` proves the controller's identity.
    Authentication,
    /// `assertionMethod` issues verifiable credentials.
    AssertionMethod,
    /// `keyAgreement` performs ECDH key exchange.
    KeyAgreement,
    /// `capabilityInvocation` invokes capabilities.
    CapabilityInvocation,
    /// `capabilityDelegation` delegates capabilities.
    CapabilityDelegation,
}

// ---------------------------------------------------------------------------
// JWK
// ---------------------------------------------------------------------------

/// JWK coordinate selector used by [`public_key_jwk_coordinate_byte_length`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicKeyJwkCoordinate {
    /// `x` coordinate (always present).
    X,
    /// `y` coordinate (present only for EC keys).
    Y,
}

/// Byte length of a JWK coordinate for a `(kty, crv)` profile.
///
/// Returns `None` for unsupported `(kty, crv, coordinate)` triples — same
/// semantics as the TS helper.
pub fn public_key_jwk_coordinate_byte_length(
    kty: KeyType,
    crv: CurveType,
    coordinate: PublicKeyJwkCoordinate,
) -> Option<usize> {
    use CurveType::*;
    use PublicKeyJwkCoordinate::*;
    match coordinate {
        X => match crv {
            Ed25519 | X25519 | Jubjub | P256 | Secp256k1 => Some(32),
            BLS12381G1 if kty == KeyType::OKP => Some(48),
            BLS12381G2 if kty == KeyType::OKP => Some(96),
            _ => None,
        },
        Y => match kty {
            KeyType::EC => Some(32),
            _ => None,
        },
    }
}

/// Public-key JWK (DID-Core: no private `d` material allowed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicKeyJwk {
    /// Key type.
    pub kty: KeyType,
    /// Curve.
    pub crv: CurveType,
    /// X coordinate (base64url).
    pub x: String,
    /// Optional Y coordinate (base64url).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<String>,
    /// Catch-all for additional public-only JWK members so we don't drop
    /// resolver-specific keywords on the floor.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, JsonValue>,
}

impl PublicKeyJwk {
    /// Run all JWK validation checks. Returns the structured list of issues.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let issues = self.collect_issues();
        if issues.is_empty() {
            Ok(())
        } else {
            Err(ValidationError::from_issues(issues))
        }
    }

    /// Variant of [`Self::validate`] that returns issues without bundling them
    /// into a [`ValidationError`]. Useful when a caller wants to collect
    /// issues from several values into one error.
    pub fn collect_issues(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        if self.extensions.contains_key("d") {
            issues.push(ValidationIssue::new(
                "publicKeyJwk must not include private key material",
            ));
        }
        // OKP-curve restrictions.
        let okp_curves = matches!(
            self.crv,
            CurveType::Ed25519 | CurveType::X25519 | CurveType::BLS12381G1 | CurveType::BLS12381G2
        );
        if matches!(self.kty, KeyType::OKP) && !okp_curves {
            issues.push(ValidationIssue::new(
                "OKP keys must use the Ed25519, X25519, BLS12381G1, or BLS12381G2 curve",
            ));
        }
        let ec_curves = matches!(self.crv, CurveType::Jubjub | CurveType::P256 | CurveType::Secp256k1);
        if matches!(self.kty, KeyType::EC) && !ec_curves {
            issues.push(ValidationIssue::new(
                "EC keys must use Jubjub, P-256, or secp256k1 curve",
            ));
        }
        match (self.kty, self.y.is_some()) {
            (KeyType::OKP, true) => issues.push(ValidationIssue::new("OKP keys must not include a y coordinate")),
            (KeyType::EC, false) | (KeyType::RSA, false) | (KeyType::oct, false) => {
                issues.push(ValidationIssue::new("Non-OKP keys must include a y coordinate"));
            }
            _ => {}
        }
        if let Some(expected_x) = public_key_jwk_coordinate_byte_length(self.kty, self.crv, PublicKeyJwkCoordinate::X) {
            if decode_base64url_bytes(&self.x, expected_x, "publicKeyJwk.x").is_err() {
                issues.push(ValidationIssue::new(
                    "publicKeyJwk.x must be canonical base64url for the supported curve length",
                ));
            }
        }
        if let Some(y) = &self.y {
            if let Some(expected_y) =
                public_key_jwk_coordinate_byte_length(self.kty, self.crv, PublicKeyJwkCoordinate::Y)
            {
                if decode_base64url_bytes(y, expected_y, "publicKeyJwk.y").is_err() {
                    issues.push(ValidationIssue::new(
                        "publicKeyJwk.y must be canonical base64url for the supported curve length",
                    ));
                }
            }
        }
        issues
    }

    /// Convenience helper: decode the `x` coordinate to bytes (canonical
    /// length-checked decode). Returns a [`CodecError`] if the value is not a
    /// canonical base64url string of the expected length.
    pub fn decode_x(&self) -> Result<Vec<u8>, CodecError> {
        let expected = public_key_jwk_coordinate_byte_length(self.kty, self.crv, PublicKeyJwkCoordinate::X)
            .unwrap_or(self.x.len());
        decode_base64url_bytes(&self.x, expected, "publicKeyJwk.x")
    }
}

// ---------------------------------------------------------------------------
// VerificationMethod and Service
// ---------------------------------------------------------------------------

/// Verification method entry. Matches the TS `VerificationMethod` shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationMethod {
    /// Key id (DID URL with fragment or relative `#fragment`).
    pub id: DidKeyId,
    /// Method type keyword.
    #[serde(rename = "type")]
    pub type_: VerificationMethodType,
    /// Controller DID.
    pub controller: DidString,
    /// Inline JWK with the public key material.
    #[serde(rename = "publicKeyJwk")]
    pub public_key_jwk: PublicKeyJwk,
}

impl VerificationMethod {
    /// Validate this method's id, controller, and embedded JWK.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Vec::new();
        if !is_did_key_id(&self.id.0) {
            issues.push(ValidationIssue::new(
                "Invalid DID Key ID format: invalid or missing fragment",
            ));
        }
        if !is_did_string(&self.controller.0) {
            issues.push(ValidationIssue::new("Invalid DID format"));
        }
        issues.extend(self.public_key_jwk.collect_issues());
        if issues.is_empty() {
            Ok(())
        } else {
            Err(ValidationError::from_issues(issues))
        }
    }
}

/// `serviceEndpoint` — a URI string, an object, or a heterogeneous array.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ServiceEndpoint {
    /// Single URI.
    Uri(String),
    /// Inline object (e.g. for routing-key services).
    Object(serde_json::Map<String, JsonValue>),
    /// Array of entries (each is a URI or an inline object).
    Array(Vec<ServiceEndpointArrayEntry>),
}

/// Entry inside a [`ServiceEndpoint::Array`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ServiceEndpointArrayEntry {
    /// URI string.
    Uri(String),
    /// Inline object.
    Object(serde_json::Map<String, JsonValue>),
}

/// `service.type` — either a single string or an array of strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ServiceType {
    /// Single type.
    One(String),
    /// Multiple types.
    Many(Vec<String>),
}

/// Service entry on a DID Document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Service {
    /// Either a DID URL or a relative reference.
    pub id: String,
    /// Service type keyword(s).
    #[serde(rename = "type")]
    pub type_: ServiceType,
    /// Endpoint(s) — URI, object, or array.
    #[serde(rename = "serviceEndpoint")]
    pub service_endpoint: ServiceEndpoint,
}

impl Service {
    /// Run id and serviceEndpoint structural checks. Endpoint normalization
    /// is handled separately via [`normalize_service_endpoint`].
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Vec::new();
        if !is_did_url(&self.id) && !is_relative_reference(&self.id) {
            issues.push(ValidationIssue::new(
                "Service id must be a DID URL or relative reference",
            ));
        }
        match &self.type_ {
            ServiceType::One(s) if s.is_empty() => issues.push(ValidationIssue::new("service type must not be empty")),
            ServiceType::Many(types) if types.is_empty() => {
                issues.push(ValidationIssue::new("service type must be a non-empty array"))
            }
            _ => {}
        }
        if issues.is_empty() {
            Ok(())
        } else {
            Err(ValidationError::from_issues(issues))
        }
    }
}

fn normalize_endpoint_value(value: JsonValue) -> JsonValue {
    match value {
        JsonValue::String(s) => JsonValue::String(normalize_uri_string(&s)),
        JsonValue::Array(items) => JsonValue::Array(items.into_iter().map(normalize_endpoint_value).collect()),
        JsonValue::Object(map) => {
            JsonValue::Object(map.into_iter().map(|(k, v)| (k, normalize_endpoint_value(v))).collect())
        }
        other => other,
    }
}

/// Normalize URIs nested anywhere inside a service endpoint value.
///
/// String endpoints are normalized directly; arrays and objects are walked
/// recursively. Matches `normalizeServiceEndpoint` in TS.
pub fn normalize_service_endpoint(endpoint: ServiceEndpoint) -> ServiceEndpoint {
    match endpoint {
        ServiceEndpoint::Uri(s) => ServiceEndpoint::Uri(normalize_uri_string(&s)),
        ServiceEndpoint::Object(map) => {
            let normalized = map
                .into_iter()
                .map(|(k, v)| (k, normalize_endpoint_value(v)))
                .collect::<serde_json::Map<_, _>>();
            ServiceEndpoint::Object(normalized)
        }
        ServiceEndpoint::Array(entries) => ServiceEndpoint::Array(
            entries
                .into_iter()
                .map(|entry| match entry {
                    ServiceEndpointArrayEntry::Uri(s) => ServiceEndpointArrayEntry::Uri(normalize_uri_string(&s)),
                    ServiceEndpointArrayEntry::Object(map) => {
                        let normalized = map
                            .into_iter()
                            .map(|(k, v)| (k, normalize_endpoint_value(v)))
                            .collect::<serde_json::Map<_, _>>();
                        ServiceEndpointArrayEntry::Object(normalized)
                    }
                })
                .collect(),
        ),
    }
}

// ---------------------------------------------------------------------------
// DID Document
// ---------------------------------------------------------------------------

/// `@context` — either a single string or an array of strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DocumentContext {
    /// Single context IRI.
    One(String),
    /// Multiple context IRIs.
    Many(Vec<String>),
}

/// `controller` — either a DID or an array of DIDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Controller {
    /// Single controller DID.
    One(DidString),
    /// Multiple controller DIDs.
    Many(Vec<DidString>),
}

/// W3C DID Document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DidDocument {
    /// JSON-LD context(s).
    #[serde(rename = "@context")]
    pub context: DocumentContext,
    /// DID Subject.
    pub id: DidString,
    /// Optional alternate identifiers.
    #[serde(rename = "alsoKnownAs", default, skip_serializing_if = "Option::is_none")]
    pub also_known_as: Option<Vec<String>>,
    /// Optional controller(s).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller: Option<Controller>,
    /// Verification methods.
    #[serde(rename = "verificationMethod", default, skip_serializing_if = "Option::is_none")]
    pub verification_method: Option<Vec<VerificationMethod>>,
    /// Authentication relation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authentication: Option<Vec<DidKeyId>>,
    /// Assertion-method relation.
    #[serde(rename = "assertionMethod", default, skip_serializing_if = "Option::is_none")]
    pub assertion_method: Option<Vec<DidKeyId>>,
    /// Key-agreement relation.
    #[serde(rename = "keyAgreement", default, skip_serializing_if = "Option::is_none")]
    pub key_agreement: Option<Vec<DidKeyId>>,
    /// Capability invocation relation.
    #[serde(rename = "capabilityInvocation", default, skip_serializing_if = "Option::is_none")]
    pub capability_invocation: Option<Vec<DidKeyId>>,
    /// Capability delegation relation.
    #[serde(rename = "capabilityDelegation", default, skip_serializing_if = "Option::is_none")]
    pub capability_delegation: Option<Vec<DidKeyId>>,
    /// Service endpoints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<Vec<Service>>,
    /// Unrecognised properties (DID-Core allows extension).
    #[serde(flatten)]
    pub extra: BTreeMap<String, JsonValue>,
}

impl DidDocument {
    /// Run W3C DID Core cross-consistency validation. Returns a structured
    /// list of issues; the message text matches the TS port.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Vec::new();
        let normalized = self.clone().with_normalized_service_endpoints();

        let empty: Vec<VerificationMethod> = Vec::new();
        let vms: &[VerificationMethod] = normalized.verification_method.as_deref().unwrap_or(&empty);
        let mut seen_vm_ids: HashMap<String, usize> = HashMap::new();
        let did = &normalized.id.0;
        let canonicalize = |value: &str| -> String {
            if value.starts_with("did:") {
                value.to_owned()
            } else if let Some(rest) = value.strip_prefix('#') {
                format!("{did}#{rest}")
            } else {
                format!("{did}#{value}")
            }
        };
        for (index, vm) in vms.iter().enumerate() {
            let canonical = canonicalize(&vm.id.0);
            if let std::collections::hash_map::Entry::Vacant(entry) =
                seen_vm_ids.entry(canonical)
            {
                entry.insert(index);
            } else {
                issues.push(ValidationIssue::at(
                    "verificationMethod ids must be unique",
                    vec!["verificationMethod".into(), index.to_string(), "id".into()],
                ));
            }
        }

        let check_relation = |name: &str, values: Option<&Vec<DidKeyId>>, issues: &mut Vec<ValidationIssue>| {
            if let Some(values) = values {
                let mut seen = HashSet::new();
                for (index, value) in values.iter().enumerate() {
                    let canonical = canonicalize(&value.0);
                    if !seen.insert(canonical.clone()) {
                        issues.push(ValidationIssue::at(
                            format!("{name} must not contain duplicate entries"),
                            vec![name.into(), index.to_string()],
                        ));
                        continue;
                    }
                    if !seen_vm_ids.contains_key(&canonical) {
                        issues.push(ValidationIssue::at(
                            format!("{name} references a verificationMethod id that does not exist"),
                            vec![name.into(), index.to_string()],
                        ));
                    }
                }
            }
        };
        check_relation("authentication", normalized.authentication.as_ref(), &mut issues);
        check_relation("assertionMethod", normalized.assertion_method.as_ref(), &mut issues);
        check_relation("keyAgreement", normalized.key_agreement.as_ref(), &mut issues);
        check_relation(
            "capabilityInvocation",
            normalized.capability_invocation.as_ref(),
            &mut issues,
        );
        check_relation(
            "capabilityDelegation",
            normalized.capability_delegation.as_ref(),
            &mut issues,
        );

        if let Some(services) = &normalized.service {
            let mut seen_ids = HashSet::new();
            for (index, service) in services.iter().enumerate() {
                if !seen_ids.insert(service.id.clone()) {
                    issues.push(ValidationIssue::at(
                        "service ids must be unique",
                        vec!["service".into(), index.to_string(), "id".into()],
                    ));
                }
                let endpoints: Vec<JsonValue> = match &service.service_endpoint {
                    ServiceEndpoint::Uri(s) => vec![JsonValue::String(s.clone())],
                    ServiceEndpoint::Object(obj) => vec![JsonValue::Object(obj.clone())],
                    ServiceEndpoint::Array(items) => items
                        .iter()
                        .map(|item| match item {
                            ServiceEndpointArrayEntry::Uri(s) => JsonValue::String(s.clone()),
                            ServiceEndpointArrayEntry::Object(obj) => JsonValue::Object(obj.clone()),
                        })
                        .collect(),
                };
                let mut seen_endpoints = HashSet::new();
                for (endpoint_index, endpoint) in endpoints.iter().enumerate() {
                    let key = endpoint.to_string();
                    if !seen_endpoints.insert(key) {
                        issues.push(ValidationIssue::at(
                            "serviceEndpoint values must be unique",
                            vec![
                                "service".into(),
                                index.to_string(),
                                "serviceEndpoint".into(),
                                endpoint_index.to_string(),
                            ],
                        ));
                    }
                }
            }
        }

        if issues.is_empty() {
            Ok(())
        } else {
            Err(ValidationError::from_issues(issues))
        }
    }

    /// Return a copy of this document with every service endpoint URI run
    /// through [`normalize_service_endpoint`].
    pub fn with_normalized_service_endpoints(mut self) -> Self {
        if let Some(services) = self.service.as_mut() {
            for service in services.iter_mut() {
                let endpoint = std::mem::replace(&mut service.service_endpoint, ServiceEndpoint::Uri(String::new()));
                service.service_endpoint = normalize_service_endpoint(endpoint);
            }
        }
        self
    }
}

// ---------------------------------------------------------------------------
// DID Document metadata + resolution
// ---------------------------------------------------------------------------

/// DID Document metadata block, as returned alongside resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DidDocumentMetadata {
    /// ISO 8601 datetime when the DID was created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// ISO 8601 datetime when the DID was last updated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
    /// Whether the DID is currently deactivated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deactivated: Option<bool>,
    /// Opaque version identifier.
    #[serde(rename = "versionId", default, skip_serializing_if = "Option::is_none")]
    pub version_id: Option<String>,
    /// Hint about when the next update is expected.
    #[serde(rename = "nextUpdate", default, skip_serializing_if = "Option::is_none")]
    pub next_update: Option<String>,
    /// Hint about the next version id.
    #[serde(rename = "nextVersionId", default, skip_serializing_if = "Option::is_none")]
    pub next_version_id: Option<String>,
    /// DIDs that point to the same subject.
    #[serde(rename = "equivalentId", default, skip_serializing_if = "Option::is_none")]
    pub equivalent_id: Option<Vec<String>>,
    /// Canonical form of this DID.
    #[serde(rename = "canonicalId", default, skip_serializing_if = "Option::is_none")]
    pub canonical_id: Option<String>,
    /// Extension keywords.
    #[serde(flatten)]
    pub extra: BTreeMap<String, JsonValue>,
}

/// Known DID document/resolution media types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KnownDidMediaType {
    /// `application/did+ld+json` — DID document JSON-LD.
    #[serde(rename = "application/did+ld+json")]
    DidLdJson,
    /// `application/did+json` — DID document plain JSON.
    #[serde(rename = "application/did+json")]
    DidJson,
    /// `application/ld+json` — Resolution-result JSON-LD envelope.
    #[serde(rename = "application/ld+json")]
    LdJson,
    /// `application/json` — Resolution-result plain JSON envelope.
    #[serde(rename = "application/json")]
    Json,
}

/// Known DID resolution error codes (registry-only subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KnownDidResolutionErrorCode {
    /// The supplied DID was syntactically invalid.
    InvalidDid,
    /// Resolver hit an unexpected internal error.
    InternalError,
    /// The supplied public key was invalid.
    InvalidPublicKey,
    /// The supplied public key length was invalid.
    InvalidPublicKeyLength,
    /// The supplied public key type was invalid.
    InvalidPublicKeyType,
    /// The DID method is not supported by this resolver.
    MethodNotSupported,
    /// Certificate is not allowed.
    NotAllowedCertificate,
    /// Global duplicate-key constraint violated.
    NotAllowedGlobalDuplicateKey,
    /// Key type is not allowed.
    NotAllowedKeyType,
    /// Locally-derived keys are not allowed.
    NotAllowedLocalDerivedKey,
    /// Local duplicate-key constraint violated.
    NotAllowedLocalDuplicateKey,
    /// DID method is not allowed.
    NotAllowedMethod,
    /// Verification-method type is not allowed.
    NotAllowedVerificationMethodType,
    /// DID was not found.
    NotFound,
    /// The requested representation is not supported.
    RepresentationNotSupported,
    /// Public-key type is not supported.
    UnsupportedPublicKeyType,
}

/// Generic DID resolution error keyword — accepts registered extension values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DidResolutionErrorCode(pub String);

impl DidResolutionErrorCode {
    /// Validate the keyword shape (`[A-Za-z][A-Za-z0-9]*`).
    pub fn validate(&self) -> Result<(), ValidationError> {
        let bytes = self.0.as_bytes();
        let valid =
            !bytes.is_empty() && bytes[0].is_ascii_alphabetic() && bytes.iter().all(|b| b.is_ascii_alphanumeric());
        if !valid {
            Err(ValidationError::from_issues(vec![ValidationIssue::new(
                "DID resolution error must match [A-Za-z][A-Za-z0-9]*",
            )]))
        } else {
            Ok(())
        }
    }
}

/// `didResolutionMetadata` envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DidResolutionMetadata {
    /// Content type the resolver returned.
    #[serde(rename = "contentType", default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<KnownDidMediaType>,
    /// Optional structured error keyword.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<DidResolutionErrorCode>,
    /// Extension keywords.
    #[serde(flatten)]
    pub extra: BTreeMap<String, JsonValue>,
}

/// W3C DID resolution result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DidResolutionResult {
    /// Optional JSON-LD context(s).
    #[serde(rename = "@context", default, skip_serializing_if = "Option::is_none")]
    pub context: Option<DocumentContext>,
    /// Resolved DID document.
    #[serde(rename = "didDocument", default, skip_serializing_if = "Option::is_none")]
    pub did_document: Option<DidDocument>,
    /// Metadata for the resolved DID document.
    #[serde(rename = "didDocumentMetadata")]
    pub did_document_metadata: DidDocumentMetadata,
    /// Metadata for the resolution operation itself.
    #[serde(rename = "didResolutionMetadata")]
    pub did_resolution_metadata: DidResolutionMetadata,
    /// Extension keywords.
    #[serde(flatten)]
    pub extra: BTreeMap<String, JsonValue>,
}

// ---------------------------------------------------------------------------
// Parsing + creation helpers
// ---------------------------------------------------------------------------

/// Validate a candidate JSON DID Document.
pub fn parse_did_document(value: JsonValue) -> Result<DidDocument, ValidationError> {
    let doc: DidDocument = serde_json::from_value(value).map_err(|e| {
        ValidationError::from_issues(vec![ValidationIssue::new(format!(
            "DID Document JSON shape is invalid: {e}"
        ))])
    })?;
    doc.validate()?;
    Ok(doc.with_normalized_service_endpoints())
}

/// Validate a candidate DID URL string.
pub fn parse_did_url(input: &str) -> Result<DidUrl, ValidationError> {
    DidUrl::parse(input)
}

/// Validate a candidate DID Key ID string.
pub fn parse_did_key_id(input: &str) -> Result<DidKeyId, ValidationError> {
    DidKeyId::parse(input)
}

/// Validate a candidate bare DID string.
pub fn parse_did(input: &str) -> Result<DidString, ValidationError> {
    DidString::parse(input)
}

/// Validate and return a verification method.
pub fn parse_verification_method(value: JsonValue) -> Result<VerificationMethod, ValidationError> {
    let vm: VerificationMethod = serde_json::from_value(value).map_err(|e| {
        ValidationError::from_issues(vec![ValidationIssue::new(format!(
            "VerificationMethod JSON shape is invalid: {e}"
        ))])
    })?;
    vm.validate()?;
    Ok(vm)
}

/// Validate and return a service entry (with normalized endpoints).
pub fn parse_service(value: JsonValue) -> Result<Service, ValidationError> {
    let mut svc: Service = serde_json::from_value(value).map_err(|e| {
        ValidationError::from_issues(vec![ValidationIssue::new(format!(
            "Service JSON shape is invalid: {e}"
        ))])
    })?;
    svc.validate()?;
    svc.service_endpoint = normalize_service_endpoint(svc.service_endpoint);
    Ok(svc)
}

/// Parameters accepted by [`create_verification_method`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateVerificationMethodParams {
    /// Key id.
    pub id: String,
    /// Method type keyword.
    pub type_: VerificationMethodType,
    /// Controller DID.
    pub controller: String,
    /// Embedded JWK.
    pub public_key_jwk: PublicKeyJwk,
}

/// Build a verification method, running the same validation as the TS helper.
pub fn create_verification_method(
    params: CreateVerificationMethodParams,
) -> Result<VerificationMethod, ValidationError> {
    let vm = VerificationMethod {
        id: DidKeyId::parse(params.id)?,
        type_: params.type_,
        controller: DidString::parse(params.controller)?,
        public_key_jwk: params.public_key_jwk,
    };
    vm.validate()?;
    Ok(vm)
}

/// Parameters accepted by [`create_service`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateServiceParams {
    /// Service id.
    pub id: String,
    /// Service type keyword(s).
    pub type_: ServiceType,
    /// Service endpoint(s).
    pub service_endpoint: ServiceEndpoint,
}

/// Build a service, validating and normalizing the endpoint.
pub fn create_service(params: CreateServiceParams) -> Result<Service, ValidationError> {
    let mut svc = Service {
        id: params.id,
        type_: params.type_,
        service_endpoint: params.service_endpoint,
    };
    svc.validate()?;
    svc.service_endpoint = normalize_service_endpoint(svc.service_endpoint);
    Ok(svc)
}

/// Parameters accepted by [`create_did_document`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CreateDidDocumentParams {
    /// Subject DID.
    pub id: String,
    /// Optional `@context`. Defaults to `https://www.w3.org/ns/did/v1`.
    pub context: Option<DocumentContext>,
    /// Optional alternate identifiers.
    pub also_known_as: Option<Vec<String>>,
    /// Optional controller(s).
    pub controller: Option<Controller>,
    /// Verification methods.
    pub verification_method: Option<Vec<VerificationMethod>>,
    /// Authentication relation.
    pub authentication: Option<Vec<DidKeyId>>,
    /// Assertion-method relation.
    pub assertion_method: Option<Vec<DidKeyId>>,
    /// Key-agreement relation.
    pub key_agreement: Option<Vec<DidKeyId>>,
    /// Capability invocation relation.
    pub capability_invocation: Option<Vec<DidKeyId>>,
    /// Capability delegation relation.
    pub capability_delegation: Option<Vec<DidKeyId>>,
    /// Service endpoints.
    pub service: Option<Vec<Service>>,
}

/// Build a fully validated DID Document. Defaults `@context` to the standard
/// DID-Core context when unset.
pub fn create_did_document(params: CreateDidDocumentParams) -> Result<DidDocument, ValidationError> {
    let doc = DidDocument {
        context: params
            .context
            .unwrap_or(DocumentContext::One("https://www.w3.org/ns/did/v1".into())),
        id: DidString::parse(params.id)?,
        also_known_as: params.also_known_as,
        controller: params.controller,
        verification_method: params.verification_method,
        authentication: params.authentication,
        assertion_method: params.assertion_method,
        key_agreement: params.key_agreement,
        capability_invocation: params.capability_invocation,
        capability_delegation: params.capability_delegation,
        service: params.service,
        extra: BTreeMap::new(),
    };
    doc.validate()?;
    Ok(doc.with_normalized_service_endpoints())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn did_string_validates_basic_shape() {
        assert!(DidString::parse("did:example:1234").is_ok());
        assert!(DidString::parse("did:example:1234#frag").is_err());
        assert!(DidString::parse("not-a-did").is_err());
    }

    #[test]
    fn did_key_id_accepts_fragments() {
        assert!(DidKeyId::parse("#key-1").is_ok());
        assert!(DidKeyId::parse("did:example:1#key-1").is_ok());
        assert!(DidKeyId::parse("did:example:1").is_err());
    }
}
