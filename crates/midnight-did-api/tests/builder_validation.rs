// This file is part of Compact.
// Copyright (C) 2026 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Regression tests for the architecture-audit Rec #1 follow-up — confirm
//! that the at-risk ledger-shape types (`JubjubPointHex`,
//! `SchnorrJubjubSignature`, `SchnorrJubjubDigest`,
//! `SchnorrJubjubVerificationMethod`) reject malformed inputs at
//! construction time, so no malformed data can land in `BuiltTx::bytes` via
//! the api crate's operation builders.
//!
//! See `docs/superpowers/notes/2026-06-26-builder-validation-audit.md` for
//! the full audit verdict.

use midnight_did_api::{
    contract::{JubjubPointHex, NewJubjubPointHex, SchnorrJubjubDigest, SchnorrJubjubSignature, ValidationError},
    error::ApiError,
    ledger_mappers::{NewSchnorrJubjubVerificationMethod, SchnorrJubjubVerificationMethod},
};

// ---------------------------------------------------------------------------
// JubjubPointHex
// ---------------------------------------------------------------------------

#[test]
fn jubjub_point_hex_accepts_32_byte_hex() {
    let pt = JubjubPointHex::new(NewJubjubPointHex {
        x: "00".repeat(32),
        y: "ff".repeat(32),
    })
    .expect("32-byte hex pair is valid");
    assert_eq!(pt.x(), &"00".repeat(32));
    assert_eq!(pt.y(), &"ff".repeat(32));
}

#[test]
fn jubjub_point_hex_rejects_empty_x() {
    let err = JubjubPointHex::new(NewJubjubPointHex {
        x: String::new(),
        y: "00".repeat(32),
    })
    .unwrap_err();
    assert!(matches!(err, ValidationError::Empty { field: "JubjubPointHex.x" }));
}

#[test]
fn jubjub_point_hex_rejects_empty_y() {
    let err = JubjubPointHex::new(NewJubjubPointHex {
        x: "00".repeat(32),
        y: String::new(),
    })
    .unwrap_err();
    assert!(matches!(err, ValidationError::Empty { field: "JubjubPointHex.y" }));
}

#[test]
fn jubjub_point_hex_rejects_odd_length_x() {
    let err = JubjubPointHex::new(NewJubjubPointHex {
        x: "0".repeat(63),
        y: "00".repeat(32),
    })
    .unwrap_err();
    assert!(matches!(err, ValidationError::NotHex { field: "JubjubPointHex.x", .. }));
}

#[test]
fn jubjub_point_hex_rejects_non_hex_character() {
    let err = JubjubPointHex::new(NewJubjubPointHex {
        x: format!("g{}", "0".repeat(63)),
        y: "00".repeat(32),
    })
    .unwrap_err();
    assert!(matches!(err, ValidationError::NotHex { field: "JubjubPointHex.x", .. }));
}

#[test]
fn jubjub_point_hex_rejects_short_coordinate() {
    // The legacy test fixture used `x: "01", y: "02"` which decodes to a
    // single byte. The new gate requires the full 32 bytes; this confirms
    // that path is now closed.
    let err = JubjubPointHex::new(NewJubjubPointHex {
        x: "01".into(),
        y: "02".into(),
    })
    .unwrap_err();
    assert!(matches!(
        err,
        ValidationError::WrongByteLength {
            field: "JubjubPointHex.x",
            expected_bytes: 32,
            actual_bytes: 1,
        }
    ));
}

#[test]
fn jubjub_point_hex_rejects_oversize_coordinate() {
    let err = JubjubPointHex::new(NewJubjubPointHex {
        x: "00".repeat(33),
        y: "00".repeat(32),
    })
    .unwrap_err();
    assert!(matches!(
        err,
        ValidationError::WrongByteLength {
            field: "JubjubPointHex.x",
            expected_bytes: 32,
            actual_bytes: 33,
        }
    ));
}

// ---------------------------------------------------------------------------
// SchnorrJubjubSignature
// ---------------------------------------------------------------------------

#[test]
fn schnorr_jubjub_signature_accepts_96_byte_hex() {
    let sig = SchnorrJubjubSignature::new("ab".repeat(96)).expect("96-byte hex is valid");
    assert_eq!(sig.bytes_hex(), &"ab".repeat(96));
}

#[test]
fn schnorr_jubjub_signature_rejects_empty() {
    let err = SchnorrJubjubSignature::new(String::new()).unwrap_err();
    assert!(matches!(
        err,
        ValidationError::Empty { field: "SchnorrJubjubSignature.bytes_hex" }
    ));
}

#[test]
fn schnorr_jubjub_signature_rejects_odd_length() {
    let err = SchnorrJubjubSignature::new("abc".into()).unwrap_err();
    assert!(matches!(
        err,
        ValidationError::NotHex { field: "SchnorrJubjubSignature.bytes_hex", .. }
    ));
}

#[test]
fn schnorr_jubjub_signature_rejects_non_hex() {
    let err = SchnorrJubjubSignature::new("zz".repeat(96)).unwrap_err();
    assert!(matches!(
        err,
        ValidationError::NotHex { field: "SchnorrJubjubSignature.bytes_hex", .. }
    ));
}

#[test]
fn schnorr_jubjub_signature_rejects_short_payload() {
    // Legacy fixture used `"deadbeef"` (4 bytes); the on-chain
    // Schnorr-Jubjub signature is 96 bytes.
    let err = SchnorrJubjubSignature::new("deadbeef".into()).unwrap_err();
    assert!(matches!(
        err,
        ValidationError::WrongByteLength {
            field: "SchnorrJubjubSignature.bytes_hex",
            expected_bytes: 96,
            actual_bytes: 4,
        }
    ));
}

// ---------------------------------------------------------------------------
// SchnorrJubjubDigest
// ---------------------------------------------------------------------------

#[test]
fn schnorr_jubjub_digest_accepts_four_32_byte_limbs() {
    let digest = SchnorrJubjubDigest::new([
        "01".repeat(32),
        "02".repeat(32),
        "03".repeat(32),
        "04".repeat(32),
    ])
    .expect("four 32-byte limbs are valid");
    let limbs = digest.limbs();
    assert_eq!(limbs[0], "01".repeat(32));
    assert_eq!(limbs[3], "04".repeat(32));
}

#[test]
fn schnorr_jubjub_digest_rejects_short_limb() {
    // Legacy fixture used `["1", "2", "3", "4"]` (each 1 hex char).
    let err = SchnorrJubjubDigest::new([
        "1".into(),
        "02".repeat(32),
        "03".repeat(32),
        "04".repeat(32),
    ])
    .unwrap_err();
    assert!(matches!(err, ValidationError::NotHex { field: "SchnorrJubjubDigest[0]", .. }));
}

#[test]
fn schnorr_jubjub_digest_rejects_empty_limb() {
    let err = SchnorrJubjubDigest::new([
        "01".repeat(32),
        String::new(),
        "03".repeat(32),
        "04".repeat(32),
    ])
    .unwrap_err();
    assert!(matches!(err, ValidationError::Empty { field: "SchnorrJubjubDigest[1]" }));
}

#[test]
fn schnorr_jubjub_digest_rejects_non_hex_limb() {
    let err = SchnorrJubjubDigest::new([
        "01".repeat(32),
        "02".repeat(32),
        format!("g{}", "0".repeat(63)),
        "04".repeat(32),
    ])
    .unwrap_err();
    assert!(matches!(err, ValidationError::NotHex { field: "SchnorrJubjubDigest[2]", .. }));
}

#[test]
fn schnorr_jubjub_digest_rejects_wrong_length_limb() {
    let err = SchnorrJubjubDigest::new([
        "01".repeat(32),
        "02".repeat(32),
        "03".repeat(32),
        "04".repeat(33),
    ])
    .unwrap_err();
    assert!(matches!(
        err,
        ValidationError::WrongByteLength {
            field: "SchnorrJubjubDigest[3]",
            expected_bytes: 32,
            actual_bytes: 33,
        }
    ));
}

// ---------------------------------------------------------------------------
// SchnorrJubjubVerificationMethod (api-layer wrapper)
// ---------------------------------------------------------------------------

#[test]
fn schnorr_jubjub_verification_method_accepts_valid_input() {
    let public_key = JubjubPointHex::new(NewJubjubPointHex {
        x: "11".repeat(32),
        y: "22".repeat(32),
    })
    .expect("valid public key");
    let vm = SchnorrJubjubVerificationMethod::new(NewSchnorrJubjubVerificationMethod {
        id: "#key-1".into(),
        public_key,
    })
    .expect("valid SchnorrJubjub VM");
    assert_eq!(vm.id(), "#key-1");
    assert_eq!(vm.public_key().x(), &"11".repeat(32));
}

#[test]
fn schnorr_jubjub_verification_method_rejects_empty_id() {
    let public_key = JubjubPointHex::new(NewJubjubPointHex {
        x: "00".repeat(32),
        y: "00".repeat(32),
    })
    .expect("valid public key");
    let err = SchnorrJubjubVerificationMethod::new(NewSchnorrJubjubVerificationMethod {
        id: String::new(),
        public_key,
    })
    .unwrap_err();
    assert!(matches!(err, ApiError::InvalidArgument(_)));
}
