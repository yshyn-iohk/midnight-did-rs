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

//! Ledger ↔ domain conversion helpers.
//!
//! Rust port of `packages/api/src/ledger-mappers.ts`. These functions adapt
//! the domain [`VerificationMethod`], [`Service`], and Schnorr-Jubjub
//! verification method into the ledger-shaped wire types the contract
//! consumes ([`crate::contract::LedgerVerificationMethod`],
//! [`crate::contract::LedgerService`], etc.).
//!
//! `relationSetFromState` is provided in
//! [`crate::contract::DidLedgerSnapshot::relation_set`] — the ledger
//! abstraction owns the relation lookup.

use midnight_did_runtime::{Backend, Contract};

use crate::{
    contract::{
        JubjubPointHex, LedgerPublicKeyJwk, LedgerSchnorrJubjubVerificationMethod, LedgerService,
        LedgerVerificationMethod, LedgerVerificationMethodRelation,
    },
    error::ApiError,
    subject::normalize_bound_fragment_id_for,
};
use midnight_did_domain::{
    crypto_codecs::decode_base64url_bytes,
    did_document::{
        CurveType, KeyType, PublicKeyJwk, PublicKeyJwkCoordinate, Service, VerificationMethod, VerificationMethodType,
        public_key_jwk_coordinate_byte_length,
    },
    ledger_utils::{BoundIdField, service_endpoint_to_ledger, service_type_to_ledger},
};

/// Schnorr-Jubjub verification method as seen by the API layer. Mirrors
/// `SchnorrJubjubVerificationMethod` from `packages/api/src/types.ts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchnorrJubjubVerificationMethod {
    /// Fragment id of the verification method (e.g. `#key-1`).
    pub id: String,
    /// Jubjub public key, hex-encoded x/y coordinates.
    pub public_key: JubjubPointHex,
}

/// Map a domain [`PublicKeyJwk`] into the ledger-wire shape, validating both
/// coordinate length and the Midnight key-profile rules.
pub fn public_key_jwk_to_ledger(jwk: &PublicKeyJwk) -> Result<LedgerPublicKeyJwk, ApiError> {
    if jwk.extensions().contains_key("d") {
        return Err(ApiError::invalid_argument(
            "publicKeyJwk must not include private key material",
        ));
    }
    let x_len =
        public_key_jwk_coordinate_byte_length(jwk.kty(), jwk.crv(), PublicKeyJwkCoordinate::X).ok_or_else(|| {
            ApiError::invalid_argument(format!(
                "Unsupported publicKeyJwk.x profile {:?}/{:?}",
                jwk.kty(),
                jwk.crv()
            ))
        })?;
    decode_base64url_bytes(jwk.x(), x_len, "publicKeyJwk.x")?;

    let y_value = match jwk.y() {
        Some(y) => {
            let y_len = public_key_jwk_coordinate_byte_length(jwk.kty(), jwk.crv(), PublicKeyJwkCoordinate::Y)
                .ok_or_else(|| {
                    ApiError::invalid_argument(format!(
                        "Unsupported publicKeyJwk.y profile {:?}/{:?}",
                        jwk.kty(),
                        jwk.crv()
                    ))
                })?;
            decode_base64url_bytes(y, y_len, "publicKeyJwk.y")?;
            y.to_owned()
        }
        None => String::new(),
    };

    Ok(LedgerPublicKeyJwk {
        kty: jwk.kty(),
        crv: jwk.crv(),
        x: jwk.x().to_owned(),
        y: y_value,
    })
}

/// Enforce the Midnight method's key-profile restrictions documented in
/// `assertMidnightKeyProfile` in TS:
///
/// - OKP keys may only use Ed25519, X25519, BLS12381G1, BLS12381G2 and must
///   not carry a y coordinate.
/// - EC keys may only use P256 or Secp256k1; Jubjub keys must use the
///   dedicated Schnorr-Jubjub flow. EC keys require a y coordinate.
/// - All other (kty, crv) combinations are rejected.
pub fn assert_midnight_key_profile(jwk: &PublicKeyJwk) -> Result<(), ApiError> {
    match jwk.kty() {
        KeyType::OKP => {
            let allowed = matches!(
                jwk.crv(),
                CurveType::Ed25519 | CurveType::X25519 | CurveType::BLS12381G1 | CurveType::BLS12381G2
            );
            if !allowed {
                return Err(ApiError::invalid_argument(
                    "OKP keys must use Ed25519, X25519, BLS12381G1, or BLS12381G2",
                ));
            }
            if jwk.y().is_some() {
                return Err(ApiError::invalid_argument("OKP keys must not include a y coordinate"));
            }
            Ok(())
        }
        KeyType::EC => {
            if matches!(jwk.crv(), CurveType::Jubjub) {
                return Err(ApiError::invalid_argument(
                    "Jubjub keys must use addSchnorrJubjubVerificationMethod",
                ));
            }
            if !matches!(jwk.crv(), CurveType::P256 | CurveType::Secp256k1) {
                return Err(ApiError::invalid_argument(
                    "EC keys must use P-256 or secp256k1; use SchnorrJubjub methods for Jubjub",
                ));
            }
            if jwk.y().is_none() {
                return Err(ApiError::invalid_argument("EC keys must include a y coordinate"));
            }
            Ok(())
        }
        KeyType::RSA | KeyType::oct => Err(ApiError::invalid_argument(
            "Only OKP (Ed25519/X25519/BLS12381G1/BLS12381G2) and EC (P-256/secp256k1) keys are supported",
        )),
    }
}

/// Map a domain [`VerificationMethod`] into the ledger-wire shape, enforcing
/// the Midnight-method profile constraints and validating that the
/// `controller` field equals the DID subject of `did_contract`.
pub fn verification_method_to_ledger<B: Backend>(
    contract: &Contract<B>,
    method: &VerificationMethod,
) -> Result<LedgerVerificationMethod, ApiError> {
    if !matches!(method.type_(), VerificationMethodType::JsonWebKey) {
        return Err(ApiError::invalid_argument("verificationMethod.type must be JsonWebKey"));
    }
    assert_midnight_key_profile(method.public_key_jwk())?;
    let subject = crate::subject::get_did_subject(contract)?;
    if method.controller().as_str() != subject {
        return Err(ApiError::Controller(crate::error::ControllerError::SubjectMismatch { expected: subject }));
    }
    let id = normalize_bound_fragment_id_for(contract, method.id().as_str(), BoundIdField::VerificationMethodId)?;
    Ok(LedgerVerificationMethod {
        id,
        typ: method.type_(),
        public_key_jwk: public_key_jwk_to_ledger(method.public_key_jwk())?,
    })
}

/// Map a Schnorr-Jubjub [`SchnorrJubjubVerificationMethod`] into the
/// ledger-wire shape.
pub fn schnorr_jubjub_verification_method_to_ledger<B: Backend>(
    contract: &Contract<B>,
    method: &SchnorrJubjubVerificationMethod,
) -> Result<LedgerSchnorrJubjubVerificationMethod, ApiError> {
    let id = normalize_bound_fragment_id_for(
        contract,
        &method.id,
        BoundIdField::SchnorrJubjubVerificationMethodId,
    )?;
    Ok(LedgerSchnorrJubjubVerificationMethod {
        id,
        public_key: method.public_key.clone(),
    })
}

/// Map a domain [`Service`] into the ledger-wire shape, including JSON
/// canonicalisation of the service endpoint.
pub fn service_to_ledger<B: Backend>(
    contract: &Contract<B>,
    service: &Service,
) -> Result<LedgerService, ApiError> {
    let endpoint = service_endpoint_to_ledger(service.service_endpoint().clone());
    let typ = service_type_to_ledger(service.type_())?;
    let id = normalize_bound_fragment_id_for(contract, service.id(), BoundIdField::ServiceId)?;
    Ok(LedgerService {
        id,
        typ,
        service_endpoint: endpoint,
    })
}

/// Map a domain
/// [`VerificationMethodRelation`](midnight_did_domain::did_document::VerificationMethodRelation)
/// into the on-chain [`LedgerVerificationMethodRelation`] enum.
pub fn ledger_verification_method_relation_for(
    relation: midnight_did_domain::did_document::VerificationMethodRelation,
) -> LedgerVerificationMethodRelation {
    use midnight_did_domain::did_document::VerificationMethodRelation as R;
    match relation {
        R::Undefined => LedgerVerificationMethodRelation::Undefined,
        R::Authentication => LedgerVerificationMethodRelation::Authentication,
        R::AssertionMethod => LedgerVerificationMethodRelation::AssertionMethod,
        R::KeyAgreement => LedgerVerificationMethodRelation::KeyAgreement,
        R::CapabilityInvocation => LedgerVerificationMethodRelation::CapabilityInvocation,
        R::CapabilityDelegation => LedgerVerificationMethodRelation::CapabilityDelegation,
    }
}

/// The TS `relationSetFromState` helper. Returns the relevant relation
/// member slice for `relation`. Rejects `Undefined`.
pub fn relation_set_from_state(
    state: &crate::contract::DidLedgerSnapshot,
    relation: midnight_did_domain::did_document::VerificationMethodRelation,
) -> Result<&[String], ApiError> {
    use midnight_did_domain::did_document::VerificationMethodRelation as R;
    if matches!(relation, R::Undefined) {
        return Err(ApiError::invalid_argument("relation must be defined"));
    }
    let ledger_relation = ledger_verification_method_relation_for(relation);
    state
        .relation_set(ledger_relation)
        .ok_or_else(|| ApiError::invalid_argument("relation must be defined"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use midnight_did_domain::did_document::{
        NewPublicKeyJwk, NewVerificationMethod, ServiceEndpoint, ServiceType,
    };
    use midnight_did_method::midnight_did::{MidnightNetwork, parse_contract_address};
    use midnight_did_runtime::RecordingBackend;
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

    fn p256_jwk(x: &str, y: &str) -> PublicKeyJwk {
        PublicKeyJwk::new(NewPublicKeyJwk {
            kty: KeyType::EC,
            crv: CurveType::P256,
            x: x.into(),
            y: Some(y.into()),
            extensions: BTreeMap::new(),
        })
        .expect("valid P-256 JWK fixture")
    }

    // 32-byte all-zero coords encoded as base64url (length 43, no padding).
    fn zeros32_b64url() -> String {
        midnight_did_domain::crypto_codecs::encode_base64url(&[0u8; 32])
    }

    #[test]
    fn maps_p256_verification_method() {
        let coord = zeros32_b64url();
        let contract = test_contract();
        let vm = VerificationMethod::new(NewVerificationMethod {
            id: format!("{}#key-1", did_subject()),
            type_: VerificationMethodType::JsonWebKey,
            controller: did_subject(),
            public_key_jwk: p256_jwk(&coord, &coord),
        })
        .expect("valid VM fixture");
        let ledger = verification_method_to_ledger(&contract, &vm).expect("map ok");
        assert_eq!(ledger.id, "#key-1");
        assert_eq!(ledger.typ, VerificationMethodType::JsonWebKey);
        assert_eq!(ledger.public_key_jwk.x, coord);
        assert_eq!(ledger.public_key_jwk.y, coord);
    }

    #[test]
    fn rejects_jubjub_in_jwk_path() {
        let coord = zeros32_b64url();
        let contract = test_contract();
        // Construct a Jubjub JWK via ::new so structural fields pass; the
        // ledger-mapper rejects Jubjub downstream.
        let jubjub_jwk = PublicKeyJwk::new(NewPublicKeyJwk {
            kty: KeyType::EC,
            crv: CurveType::Jubjub,
            x: coord.clone(),
            y: Some(coord),
            extensions: BTreeMap::new(),
        })
        .expect("valid Jubjub EC JWK at structural layer");
        let vm = VerificationMethod::new(NewVerificationMethod {
            id: format!("{}#key-2", did_subject()),
            type_: VerificationMethodType::JsonWebKey,
            controller: did_subject(),
            public_key_jwk: jubjub_jwk,
        })
        .expect("valid VM fixture");
        let err = verification_method_to_ledger(&contract, &vm).unwrap_err();
        assert!(matches!(err, ApiError::InvalidArgument(_)));
    }

    #[test]
    fn rejects_controller_mismatch() {
        let coord = zeros32_b64url();
        let contract = test_contract();
        let other = "1".repeat(64);
        let vm = VerificationMethod::new(NewVerificationMethod {
            id: format!("did:midnight:testnet:{other}#key-1"),
            type_: VerificationMethodType::JsonWebKey,
            controller: format!("did:midnight:testnet:{other}"),
            public_key_jwk: p256_jwk(&coord, &coord),
        })
        .expect("structurally valid VM with mismatched controller");
        let err = verification_method_to_ledger(&contract, &vm).unwrap_err();
        assert!(matches!(err, ApiError::Controller(crate::error::ControllerError::SubjectMismatch { .. })));
    }

    #[test]
    fn maps_service() {
        let contract = test_contract();
        let svc = Service::new(midnight_did_domain::did_document::NewService {
            id: "svc-1".into(),
            type_: ServiceType::One("LinkedDomains".into()),
            service_endpoint: ServiceEndpoint::Uri("https://example.com".into()),
        })
        .expect("sample Service is valid");
        let ledger = service_to_ledger(&contract, &svc).expect("map ok");
        assert_eq!(ledger.id, "#svc-1");
        assert_eq!(ledger.typ, "LinkedDomains");
        assert!(ledger.service_endpoint.contains("example.com"));
    }
}
