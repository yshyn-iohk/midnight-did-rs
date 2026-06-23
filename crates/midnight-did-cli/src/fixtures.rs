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

//! Deterministic inputs that drive the CLI demo flow.
//!
//! Every value is hard-coded so two runs of `midnight-did-cli run` produce
//! byte-identical output. The same fixtures are also written to disk by
//! `capture-fixtures`, which lets a TS/Java/Swift reference implementation
//! assert structural equality against the Rust output.

use std::collections::BTreeMap;

use midnight_did_api::contract::{LedgerPublicKeyJwk, LedgerVerificationMethod};
use midnight_did_domain::{
    crypto_codecs::encode_base64url,
    did_document::{
        CurveType, KeyType, NewPublicKeyJwk, NewService, NewVerificationMethod, PublicKeyJwk, Service, ServiceEndpoint,
        ServiceType, VerificationMethod, VerificationMethodType,
    },
};
use midnight_did_method::midnight_did::MidnightNetwork;

/// 64-hex-char (32-byte) contract address used as the DID subject suffix.
pub const CONTRACT_ADDRESS: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// Network selector for the deterministic flow.
pub const NETWORK: MidnightNetwork = MidnightNetwork::Undeployed;

/// Initial controller secret key (32 bytes, all `0x42`).
pub const INITIAL_SECRET_KEY: [u8; 32] = [0x42u8; 32];

/// Controller public key derived deterministically from `INITIAL_SECRET_KEY`.
/// Real wallets compute this via `pad(32, "did:controller:pk") ‖ sk →
/// persistentHash`; the demo uses a fixed stub so the resulting ledger
/// snapshot is reproducible without depending on the runtime crate.
pub const INITIAL_CONTROLLER_PK_HEX: &str = "0707070707070707070707070707070707070707070707070707070707070707";

/// Secret key after rotation.
pub const ROTATED_SECRET_KEY: [u8; 32] = [0x55u8; 32];

/// Public key after rotation (deterministic stub).
pub const ROTATED_CONTROLLER_PK_BYTES: [u8; 32] = [0xAAu8; 32];

/// Fragment id of the JWK verification method inserted by step 2.
pub const VM_FRAGMENT: &str = "#key-1";

/// Service id inserted by step 3.
pub const SERVICE_ID: &str = "service-1";

/// `LinkedDomains` endpoint URL.
pub const SERVICE_ENDPOINT_URL: &str = "https://example.com";

/// `alsoKnownAs` alias inserted by step 4.
pub const ALSO_KNOWN_AS_URI: &str = "did:web:example.com";

/// Fixed creation timestamp in milliseconds (2026-06-03T00:00:00Z).
pub const CREATED_MS: u64 = 1_780_444_800_000;

/// Fixed timestamp advance per mutation step (10 minutes).
pub const STEP_ADVANCE_MS: u64 = 10 * 60 * 1000;

/// Build the canonical DID subject string for this fixture set.
pub fn did_subject() -> String {
    format!("did:midnight:{}:{}", NETWORK.as_wire_str(), CONTRACT_ADDRESS)
}

/// Sample P-256 JWK used by the JsonWebKey verification method. The
/// coordinates are fixed (32 zero bytes for `x`, 32 `0x11` bytes for `y`)
/// so the cross-language fixtures can be compared field-by-field.
pub fn sample_jwk() -> PublicKeyJwk {
    PublicKeyJwk::new(NewPublicKeyJwk {
        kty: KeyType::EC,
        crv: CurveType::P256,
        x: encode_base64url(&[0u8; 32]),
        y: Some(encode_base64url(&[0x11u8; 32])),
        extensions: BTreeMap::new(),
    })
    .expect("sample_jwk is valid")
}

/// Build the `VerificationMethod` value used by the JWK insert step.
pub fn sample_verification_method() -> VerificationMethod {
    let subject = did_subject();
    VerificationMethod::new(NewVerificationMethod {
        id: format!("{subject}{VM_FRAGMENT}"),
        type_: VerificationMethodType::JsonWebKey,
        controller: subject,
        public_key_jwk: sample_jwk(),
    })
    .expect("sample_verification_method is valid")
}

/// Ledger-shaped view of [`sample_verification_method`].
pub fn sample_ledger_verification_method() -> LedgerVerificationMethod {
    let jwk = sample_jwk();
    LedgerVerificationMethod {
        id: VM_FRAGMENT.to_string(),
        typ: VerificationMethodType::JsonWebKey,
        public_key_jwk: LedgerPublicKeyJwk {
            kty: jwk.kty,
            crv: jwk.crv,
            x: jwk.x,
            y: jwk.y.unwrap_or_default(),
        },
    }
}

/// Build the demo `LinkedDomains` service entry.
pub fn sample_service() -> Service {
    Service::new(NewService {
        id: SERVICE_ID.to_string(),
        type_: ServiceType::One("LinkedDomains".into()),
        service_endpoint: ServiceEndpoint::Uri(SERVICE_ENDPOINT_URL.into()),
    })
    .expect("sample_service is valid")
}
