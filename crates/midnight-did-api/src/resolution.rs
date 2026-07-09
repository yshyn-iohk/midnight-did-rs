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

//! `resolve(didContract)` — read ledger state, map to a DID Document.
//!
//! Rust port of `packages/api/src/resolution.ts` and the `LedgerToDomain`
//! mapper in `packages/did/src/ledger-to-domain.ts`. Note: the canonical
//! `LedgerToDomain` is fairly large; this port covers the assembly half
//! (ledger snapshot → DID Document) directly against
//! [`crate::contract::DidLedgerSnapshot`]. JWK reconstruction from the
//! ledger is delegated to [`ledger_jwk_to_domain`].

use std::collections::BTreeMap;

use midnight_did_domain::crypto_codecs::encode_base64url;
use midnight_did_domain::did_document::{
    Controller, CurveType, DidDocument, DidDocumentMetadata, DidKeyId, DidString, DocumentContext, KeyType,
    NewPublicKeyJwk, NewService, NewVerificationMethod, PublicKeyJwk, Service, ServiceEndpoint,
    ServiceEndpointArrayEntry, ServiceType, VerificationMethod, VerificationMethodRelation, VerificationMethodType,
};
use midnight_did_method::hex_ext::HashOutputExt;
use midnight_did_method::midnight_did::{MidnightNetwork, create_midnight_did_string};
use midnight_did_runtime::{Backend, Contract};

use crate::contract::{
    DidLedgerSnapshot, LedgerPublicKeyJwk, LedgerSchnorrJubjubVerificationMethod, LedgerService,
    LedgerVerificationMethodRelation,
};
use crate::error::{ApiError, ContractError};

/// Outcome of [`resolve`]: a `(document, metadata)` pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMidnightDid {
    /// The reconstructed DID Document.
    pub did_document: DidDocument,
    /// Document metadata (created / updated / deactivated / versionId).
    pub did_document_metadata: DidDocumentMetadata,
}

/// `resolve(providers, didContract)` — load the ledger state and convert it
/// into a (DID Document, Metadata) pair.
///
/// Returns `Ok(None)` if the contract has no live state (mirrors the TS
/// `null` return).
pub async fn resolve<B: Backend>(contract: &Contract<B>) -> Result<Option<ResolvedMidnightDid>, ApiError> {
    let state = match contract.read_snapshot().await {
        Ok(state) => state,
        // R2-2 collapses the backend's failure modes into a single
        // `ContractError::Failed(_)` (the live `NotDeployed`/`StateUnavailable`
        // discrimination will return once `LiveBackend` lands and can
        // surface those signals through `BackendError`).
        Err(err) => return Err(ApiError::Contract(ContractError::Failed(err.to_string()))),
    };
    let network = contract.network();
    let address = contract.address.to_hex();
    let did_document = ledger_state_to_did_document(&state, network, &address)?;
    let did_document_metadata = ledger_state_to_metadata(&state);
    Ok(Some(ResolvedMidnightDid {
        did_document,
        did_document_metadata,
    }))
}

/// `LedgerToDomain.ledgerStateToDIDDocument` — assemble a [`DidDocument`]
/// from a ledger snapshot.
pub fn ledger_state_to_did_document(
    state: &DidLedgerSnapshot,
    network: MidnightNetwork,
    contract_address: &str,
) -> Result<DidDocument, ApiError> {
    let did = create_midnight_did_string(contract_address, network).0;
    let mut verification_method: Vec<VerificationMethod> = Vec::new();
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (id, method) in &state.verification_methods {
        let domain_jwk = ledger_jwk_to_domain(&method.public_key_jwk)?;
        let absolute_id = absolute_did_url_reference(&did, id);
        let normalized = verification_method_fragment_id(id);
        if !seen_ids.insert(normalized) {
            return Err(ApiError::mapping("Duplicate verification method id"));
        }
        verification_method.push(VerificationMethod::new(NewVerificationMethod {
            id: absolute_id,
            type_: ledger_verification_method_type_to_domain(method.typ)?,
            controller: did.clone(),
            public_key_jwk: domain_jwk,
        })?);
    }
    for (id, method) in &state.schnorr_jubjub_verification_methods {
        let domain_jwk = schnorr_jubjub_pk_to_jwk(method)?;
        let absolute_id = absolute_did_url_reference(&did, id);
        let normalized = verification_method_fragment_id(id);
        if !seen_ids.insert(normalized) {
            return Err(ApiError::mapping("Duplicate verification method id"));
        }
        verification_method.push(VerificationMethod::new(NewVerificationMethod {
            id: absolute_id,
            type_: VerificationMethodType::JsonWebKey,
            controller: did.clone(),
            public_key_jwk: domain_jwk,
        })?);
    }

    // Validate every relation entry refers to an existing verification method.
    let assert_targets = |name: &str, members: &[String]| -> Result<(), ApiError> {
        for member in members {
            let normalized = verification_method_fragment_id(member);
            if !seen_ids.contains(&normalized) {
                return Err(ApiError::mapping(format!(
                    "{name} references missing verification method '{}'",
                    absolute_did_url_reference(&did, member)
                )));
            }
        }
        Ok(())
    };
    assert_targets("authentication", &state.authentication_relation)?;
    assert_targets("assertionMethod", &state.assertion_method_relation)?;
    assert_targets("keyAgreement", &state.key_agreement_relation)?;
    assert_targets("capabilityInvocation", &state.capability_invocation_relation)?;
    assert_targets("capabilityDelegation", &state.capability_delegation_relation)?;

    let map_relation = |members: &[String]| -> Result<Option<Vec<DidKeyId>>, ApiError> {
        if members.is_empty() {
            Ok(None)
        } else {
            let parsed = members
                .iter()
                .map(|m| {
                    DidKeyId::parse(verification_method_fragment_id(m))
                        .map_err(|e| ApiError::mapping(format!("invalid DID key id in relation: {e}")))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Some(parsed))
        }
    };

    let services: Option<Vec<Service>> = if state.services.is_empty() {
        None
    } else {
        let mut out = Vec::new();
        for svc in state.services.values() {
            let parsed = parse_ledger_service(svc, &did)?;
            out.push(parsed);
        }
        Some(out)
    };
    let also_known_as = if state.also_known_as.is_empty() {
        None
    } else {
        Some(state.also_known_as.clone())
    };

    let verification_method_opt = if verification_method.is_empty() {
        None
    } else {
        Some(verification_method)
    };

    Ok(DidDocument {
        context: DocumentContext::Many(vec![
            "https://www.w3.org/ns/did/v1".into(),
            "https://w3c.github.io/vc-jws-2020/contexts/v1".into(),
        ]),
        id: DidString::parse(did.clone()).map_err(|e| ApiError::mapping(format!("invalid DID id: {e}")))?,
        controller: Some(Controller::One(
            DidString::parse(did.clone()).map_err(|e| ApiError::mapping(format!("invalid DID controller: {e}")))?,
        )),
        verification_method: verification_method_opt,
        authentication: map_relation(&state.authentication_relation)?,
        assertion_method: map_relation(&state.assertion_method_relation)?,
        key_agreement: map_relation(&state.key_agreement_relation)?,
        capability_invocation: map_relation(&state.capability_invocation_relation)?,
        capability_delegation: map_relation(&state.capability_delegation_relation)?,
        also_known_as,
        service: services,
        extra: BTreeMap::new(),
    })
}

/// `LedgerToDomain.ledgerStateToMetadata`.
pub fn ledger_state_to_metadata(state: &DidLedgerSnapshot) -> DidDocumentMetadata {
    let created = timestamp_to_iso(state.created_ms);
    let updated = timestamp_to_iso(state.updated_ms);
    let is_deactivated = state.deactivated || !state.active;

    DidDocumentMetadata {
        created,
        updated,
        deactivated: if is_deactivated { Some(true) } else { None },
        version_id: Some(state.version.to_string()),
        next_update: None,
        next_version_id: None,
        equivalent_id: None,
        canonical_id: None,
        extra: BTreeMap::new(),
    }
}

/// Translate a ledger `VerificationMethodType` discriminant to the domain
/// enum.
fn ledger_verification_method_type_to_domain(typ: VerificationMethodType) -> Result<VerificationMethodType, ApiError> {
    Ok(typ)
}

/// Map a ledger-wire JWK back to a domain JWK, applying the OKP-no-y rule.
pub fn ledger_jwk_to_domain(jwk: &LedgerPublicKeyJwk) -> Result<PublicKeyJwk, ApiError> {
    let y_value = if jwk.y.is_empty() { None } else { Some(jwk.y.clone()) };
    // OKP profiles must not include y; collapse empty-y to None then assert.
    if matches!(jwk.kty, KeyType::OKP) && y_value.is_some() {
        return Err(ApiError::mapping("OKP ledger publicKeyJwk.y must be empty"));
    }
    Ok(PublicKeyJwk::new(NewPublicKeyJwk {
        kty: jwk.kty,
        crv: jwk.crv,
        x: jwk.x.clone(),
        y: y_value,
        extensions: BTreeMap::new(),
    })?)
}

/// Reconstruct a Jubjub `PublicKeyJwk` from a Schnorr-Jubjub ledger entry.
pub fn schnorr_jubjub_pk_to_jwk(method: &LedgerSchnorrJubjubVerificationMethod) -> Result<PublicKeyJwk, ApiError> {
    let x = decode_jubjub_coordinate(method.public_key.x(), "publicKey.x")?;
    let y = decode_jubjub_coordinate(method.public_key.y(), "publicKey.y")?;
    Ok(PublicKeyJwk::new(NewPublicKeyJwk {
        kty: KeyType::EC,
        crv: CurveType::Jubjub,
        x: encode_base64url(&x),
        y: Some(encode_base64url(&y)),
        extensions: BTreeMap::new(),
    })?)
}

fn decode_jubjub_coordinate(hex_value: &str, label: &str) -> Result<[u8; 32], ApiError> {
    let mut bytes = hex::decode(hex_value).map_err(|err| ApiError::mapping(format!("{label}: {err}")))?;
    if bytes.len() > 32 {
        return Err(ApiError::mapping(format!(
            "{label} does not fit in 32 bytes (got {} bytes)",
            bytes.len()
        )));
    }
    // Pad to 32 bytes — coordinate is a field element so we accept short
    // hex inputs (right-padded to 32 little-endian bytes).
    bytes.resize(32, 0u8);
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// `LedgerToDomain.verificationMethodId` — normalize a ledger-stored id back
/// to fragment form. Already-normalized inputs pass through unchanged.
pub fn verification_method_fragment_id(id: &str) -> String {
    let raw = id.trim();
    if raw.starts_with("did:") {
        if raw.contains('#') {
            return raw.to_owned();
        }
        let last_colon = raw.rfind(':').unwrap_or(raw.len() - 1);
        return format!("{}#{}", raw, &raw[last_colon + 1..]);
    }
    let needs_prefix = !raw.starts_with('#') && !raw.starts_with('/') && !raw.starts_with('.') && !raw.starts_with('?');
    if needs_prefix {
        format!("#{raw}")
    } else {
        raw.to_owned()
    }
}

/// `LedgerToDomain.absoluteDidUrlReference` — produce an absolute DID URL
/// rooted at `did` if `id` is relative.
pub fn absolute_did_url_reference(did: &str, id: &str) -> String {
    let normalized = verification_method_fragment_id(id);
    if normalized.starts_with("did:") {
        normalized
    } else {
        format!("{did}{normalized}")
    }
}

/// Parse a ledger service entry, including its JSON-encoded `type` and
/// `serviceEndpoint`.
fn parse_ledger_service(svc: &LedgerService, did: &str) -> Result<Service, ApiError> {
    let id = absolute_did_url_reference(did, &svc.id);
    let type_ = parse_service_type(&svc.typ)?;
    let service_endpoint = parse_service_endpoint(&svc.service_endpoint)?;
    Ok(Service::new(NewService {
        id,
        type_,
        service_endpoint,
    })?)
}

fn parse_service_type(raw: &str) -> Result<ServiceType, ApiError> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(ApiError::mapping("Invalid service type: empty value"));
    }
    if value.starts_with('[') {
        let parsed: Vec<String> =
            serde_json::from_str(value).map_err(|_| ApiError::mapping("Invalid service type: malformed JSON array"))?;
        if parsed.is_empty() || parsed.iter().any(|s| s.trim().is_empty()) {
            return Err(ApiError::mapping(
                "Invalid service type: expected non-empty unique strings",
            ));
        }
        let trimmed: Vec<String> = parsed.iter().map(|s| s.trim().to_owned()).collect();
        let mut dedup = trimmed.clone();
        dedup.sort();
        dedup.dedup();
        if dedup.len() != trimmed.len() {
            return Err(ApiError::mapping(
                "Invalid service type: expected non-empty unique strings",
            ));
        }
        return Ok(ServiceType::Many(trimmed));
    }
    Ok(ServiceType::One(value.to_owned()))
}

fn parse_service_endpoint(raw: &str) -> Result<ServiceEndpoint, ApiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ApiError::mapping("Invalid serviceEndpoint: empty value"));
    }
    // Try parsing as JSON first.
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        match value {
            serde_json::Value::String(s) => return Ok(ServiceEndpoint::Uri(s)),
            serde_json::Value::Object(map) => return Ok(ServiceEndpoint::Object(map)),
            serde_json::Value::Array(arr) => {
                let mut entries = Vec::new();
                for entry in arr {
                    match entry {
                        serde_json::Value::String(s) => entries.push(ServiceEndpointArrayEntry::Uri(s)),
                        serde_json::Value::Object(map) => entries.push(ServiceEndpointArrayEntry::Object(map)),
                        _ => return Err(ApiError::mapping("Invalid serviceEndpoint array entry")),
                    }
                }
                return Ok(ServiceEndpoint::Array(entries));
            }
            _ => {}
        }
    }
    Ok(ServiceEndpoint::Uri(trimmed.to_owned()))
}

/// Convert a ledger millisecond timestamp into an ISO-8601 string. Mirrors
/// the TS `timestampToIsoString` helper, which trims sub-second precision.
fn timestamp_to_iso(ms: u64) -> Option<String> {
    if ms == 0 {
        return None;
    }
    let seconds = ms / 1000;
    let datetime = format_iso8601(seconds)?;
    Some(datetime)
}

/// Minimal ISO-8601 formatter (UTC seconds → `YYYY-MM-DDTHH:MM:SSZ`). Keeps
/// this crate free of a `chrono`/`time` dependency. Range: 1970-01-01 to
/// 9999-12-31.
fn format_iso8601(seconds: u64) -> Option<String> {
    const SECONDS_PER_DAY: u64 = 86_400;
    let total_days = seconds / SECONDS_PER_DAY;
    let secs_in_day = (seconds % SECONDS_PER_DAY) as u32;
    let hour = secs_in_day / 3600;
    let minute = (secs_in_day % 3600) / 60;
    let second = secs_in_day % 60;

    // Compute Y-M-D from total_days since 1970-01-01.
    let mut days = total_days as i64;
    let mut year: i64 = 1970;
    loop {
        let leap = is_leap(year);
        let year_days = if leap { 366 } else { 365 };
        if days >= year_days {
            days -= year_days;
            year += 1;
        } else {
            break;
        }
        if year > 9999 {
            return None;
        }
    }
    let month_days = [
        31,
        if is_leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 0usize;
    while month < month_days.len() && days >= month_days[month] {
        days -= month_days[month];
        month += 1;
    }
    let day = days + 1;
    Some(format!(
        "{year:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        month + 1,
        day,
        hour,
        minute,
        second
    ))
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Translate a ledger relation enum to its domain counterpart.
pub fn ledger_to_domain_relation(relation: LedgerVerificationMethodRelation) -> Option<VerificationMethodRelation> {
    use LedgerVerificationMethodRelation as L;
    Some(match relation {
        L::Undefined => VerificationMethodRelation::Undefined,
        L::Authentication => VerificationMethodRelation::Authentication,
        L::AssertionMethod => VerificationMethodRelation::AssertionMethod,
        L::KeyAgreement => VerificationMethodRelation::KeyAgreement,
        L::CapabilityInvocation => VerificationMethodRelation::CapabilityInvocation,
        L::CapabilityDelegation => VerificationMethodRelation::CapabilityDelegation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{LedgerPublicKeyJwk, LedgerService, LedgerVerificationMethod};

    const ADDR: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    fn b64() -> String {
        encode_base64url(&[0u8; 32])
    }

    fn p256_method(id: &str) -> LedgerVerificationMethod {
        LedgerVerificationMethod {
            id: id.into(),
            typ: VerificationMethodType::JsonWebKey,
            public_key_jwk: LedgerPublicKeyJwk {
                kty: KeyType::EC,
                crv: CurveType::P256,
                x: b64(),
                y: b64(),
            },
        }
    }

    #[test]
    fn empty_ledger_yields_minimal_document() {
        let state = DidLedgerSnapshot::default();
        let doc = ledger_state_to_did_document(&state, MidnightNetwork::Testnet, ADDR).unwrap();
        assert_eq!(doc.id.as_str(), format!("did:midnight:testnet:{ADDR}"));
        assert!(doc.verification_method.is_none());
        assert!(doc.authentication.is_none());
        assert!(doc.service.is_none());
    }

    #[test]
    fn maps_verification_methods_into_document() {
        let mut state = DidLedgerSnapshot::default();
        state
            .verification_methods
            .insert("#key-1".into(), p256_method("#key-1"));
        let doc = ledger_state_to_did_document(&state, MidnightNetwork::Testnet, ADDR).unwrap();
        let vms = doc.verification_method.unwrap();
        assert_eq!(vms.len(), 1);
        assert!(vms[0].id().as_str().ends_with("#key-1"));
    }

    #[test]
    fn rejects_dangling_relation() {
        let mut state = DidLedgerSnapshot::default();
        state.authentication_relation.push("#missing".into());
        let err = ledger_state_to_did_document(&state, MidnightNetwork::Testnet, ADDR).unwrap_err();
        assert!(matches!(err, ApiError::Mapping(_)));
    }

    #[test]
    fn parses_service_endpoint_uri() {
        let mut state = DidLedgerSnapshot::default();
        state.services.insert(
            "#svc-1".into(),
            LedgerService {
                id: "#svc-1".into(),
                typ: "LinkedDomains".into(),
                service_endpoint: "\"https://example.com\"".into(),
            },
        );
        let doc = ledger_state_to_did_document(&state, MidnightNetwork::Testnet, ADDR).unwrap();
        let svc = &doc.service.unwrap()[0];
        assert!(matches!(svc.service_endpoint(), ServiceEndpoint::Uri(u) if u == "https://example.com"));
    }

    #[test]
    fn metadata_marks_deactivated() {
        let state = DidLedgerSnapshot {
            active: false,
            deactivated: true,
            version: 7,
            ..DidLedgerSnapshot::default()
        };
        let md = ledger_state_to_metadata(&state);
        assert_eq!(md.deactivated, Some(true));
        assert_eq!(md.version_id.as_deref(), Some("7"));
    }

    #[test]
    fn iso_format_basic() {
        // 2024-01-01T00:00:00Z = 1704067200 seconds
        let s = format_iso8601(1_704_067_200).unwrap();
        assert_eq!(s, "2024-01-01T00:00:00Z");
    }
}
