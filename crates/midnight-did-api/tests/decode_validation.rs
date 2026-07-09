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

//! Symmetric follow-up to the encode-side gate exercised in
//! [`builder_validation.rs`](builder_validation). Confirms that the
//! decode-side gate added in commit `b3fdb20` rejects malformed serde
//! envelopes for each privatised ledger-shape type, so a malformed
//! `BuiltTx::bytes` payload (e.g. fed into
//! [`midnight_did_runtime::RecordingBackend::submit_tx`] or a future
//! `LiveBackend` consuming an externally-produced tx) cannot land a
//! malformed inner value inside a typed
//! [`midnight_did_runtime::DidContractCall`] variant.
//!
//! The tests construct raw JSON payloads that match the legacy
//! pre-decode-gate public layout (because the validating `Repr` shims +
//! transparent newtype keep the wire format byte-identical for valid
//! inputs) and assert that `serde_json::from_str::<Type>(…)` returns
//! `Err`, mirroring the negative cases covered on the encoding side.
//!
//! Each type also has a positive round-trip case to confirm valid inputs
//! still encode → decode → encode byte-identical.

use midnight_did_api::contract::{JubjubPointHex, NewJubjubPointHex, SchnorrJubjubDigest, SchnorrJubjubSignature};

/// 64-char (= 32-byte) hex placeholder used as a valid coordinate / limb.
fn valid_coord() -> String {
    "00".repeat(32)
}

/// 192-char (= 96-byte) hex placeholder used as a valid signature payload.
fn valid_signature_hex() -> String {
    "ab".repeat(96)
}

// ---------------------------------------------------------------------------
// JubjubPointHex
// ---------------------------------------------------------------------------

#[test]
fn jubjub_point_hex_decode_rejects_short_x() {
    // `"01"` is the same legacy stub flagged by the encoding-side audit —
    // serde-decoding it must now fail too.
    let json = format!(r#"{{"x":"01","y":"{}"}}"#, valid_coord());
    let err = serde_json::from_str::<JubjubPointHex>(&json).expect_err("short x must fail decode");
    let msg = err.to_string();
    assert!(
        msg.contains("JubjubPointHex.x"),
        "error should name the offending field, got: {msg}"
    );
}

#[test]
fn jubjub_point_hex_decode_rejects_short_y() {
    let json = format!(r#"{{"x":"{}","y":"02"}}"#, valid_coord());
    let err = serde_json::from_str::<JubjubPointHex>(&json).expect_err("short y must fail decode");
    assert!(err.to_string().contains("JubjubPointHex.y"));
}

#[test]
fn jubjub_point_hex_decode_rejects_non_hex() {
    let bad = "z".repeat(64);
    let json = format!(r#"{{"x":"{bad}","y":"{}"}}"#, valid_coord());
    let err = serde_json::from_str::<JubjubPointHex>(&json).expect_err("non-hex x must fail decode");
    assert!(err.to_string().contains("hex"));
}

#[test]
fn jubjub_point_hex_decode_rejects_empty() {
    let json = format!(r#"{{"x":"","y":"{}"}}"#, valid_coord());
    let err = serde_json::from_str::<JubjubPointHex>(&json).expect_err("empty x must fail decode");
    assert!(err.to_string().contains("empty"));
}

#[test]
fn jubjub_point_hex_round_trip_byte_identical() {
    let pt = JubjubPointHex::new(NewJubjubPointHex {
        x: valid_coord(),
        y: "ff".repeat(32),
    })
    .expect("32-byte hex pair is valid");
    let encoded_once = serde_json::to_string(&pt).expect("serialise");
    let decoded: JubjubPointHex = serde_json::from_str(&encoded_once).expect("round-trip decode");
    let encoded_twice = serde_json::to_string(&decoded).expect("re-serialise");
    assert_eq!(encoded_once, encoded_twice, "wire format must be byte-identical");
    assert_eq!(pt, decoded);
}

// ---------------------------------------------------------------------------
// SchnorrJubjubSignature
// ---------------------------------------------------------------------------

#[test]
fn schnorr_jubjub_signature_decode_rejects_short_bytes_hex() {
    // `"deadbeef"` is the same legacy stub flagged by the encoding-side audit.
    let json = r#"{"bytes_hex":"deadbeef"}"#;
    let err = serde_json::from_str::<SchnorrJubjubSignature>(json).expect_err("short signature must fail decode");
    assert!(err.to_string().contains("SchnorrJubjubSignature.bytes_hex"));
}

#[test]
fn schnorr_jubjub_signature_decode_rejects_odd_length() {
    let json = r#"{"bytes_hex":"abc"}"#;
    let err = serde_json::from_str::<SchnorrJubjubSignature>(json).expect_err("odd-length must fail decode");
    assert!(err.to_string().contains("odd length"));
}

#[test]
fn schnorr_jubjub_signature_decode_rejects_empty() {
    let json = r#"{"bytes_hex":""}"#;
    let err = serde_json::from_str::<SchnorrJubjubSignature>(json).expect_err("empty must fail decode");
    assert!(err.to_string().contains("empty"));
}

#[test]
fn schnorr_jubjub_signature_round_trip_byte_identical() {
    let sig = SchnorrJubjubSignature::new(valid_signature_hex()).expect("valid 96-byte signature");
    let encoded_once = serde_json::to_string(&sig).expect("serialise");
    let decoded: SchnorrJubjubSignature = serde_json::from_str(&encoded_once).expect("round-trip decode");
    let encoded_twice = serde_json::to_string(&decoded).expect("re-serialise");
    assert_eq!(encoded_once, encoded_twice, "wire format must be byte-identical");
    assert_eq!(sig, decoded);
}

// ---------------------------------------------------------------------------
// SchnorrJubjubDigest
// ---------------------------------------------------------------------------

#[test]
fn schnorr_jubjub_digest_decode_rejects_short_limb() {
    // `"1"` short stub from the encoding-side regression cases — plus odd
    // length, so the validator rejects via `NotHex { reason: "odd length"
    // … }` rather than `WrongByteLength`.
    let coord = valid_coord();
    let json = format!(r#"["1","{coord}","{coord}","{coord}"]"#);
    let err = serde_json::from_str::<SchnorrJubjubDigest>(&json).expect_err("short limb must fail decode");
    assert!(err.to_string().contains("SchnorrJubjubDigest[0]"));
}

#[test]
fn schnorr_jubjub_digest_decode_rejects_non_hex_limb() {
    let coord = valid_coord();
    let bad = "z".repeat(64);
    let json = format!(r#"["{coord}","{bad}","{coord}","{coord}"]"#);
    let err = serde_json::from_str::<SchnorrJubjubDigest>(&json).expect_err("non-hex limb must fail decode");
    assert!(err.to_string().contains("SchnorrJubjubDigest[1]"));
}

#[test]
fn schnorr_jubjub_digest_decode_rejects_empty_limb() {
    let coord = valid_coord();
    let json = format!(r#"["{coord}","{coord}","","{coord}"]"#);
    let err = serde_json::from_str::<SchnorrJubjubDigest>(&json).expect_err("empty limb must fail decode");
    assert!(err.to_string().contains("SchnorrJubjubDigest[2]"));
}

#[test]
fn schnorr_jubjub_digest_round_trip_byte_identical() {
    let digest = SchnorrJubjubDigest::new(["00".repeat(32), "11".repeat(32), "22".repeat(32), "33".repeat(32)])
        .expect("valid 4 × 32-byte limbs");
    let encoded_once = serde_json::to_string(&digest).expect("serialise");
    let decoded: SchnorrJubjubDigest = serde_json::from_str(&encoded_once).expect("round-trip decode");
    let encoded_twice = serde_json::to_string(&decoded).expect("re-serialise");
    assert_eq!(encoded_once, encoded_twice, "wire format must be byte-identical");
    assert_eq!(digest, decoded);
    // `#[serde(transparent)]` is preserved — the JSON shape is still a flat
    // four-element array of hex strings, not a wrapping object.
    assert!(encoded_once.starts_with('['));
    assert!(encoded_once.ends_with(']'));
}

// ---------------------------------------------------------------------------
// End-to-end DidContractCall envelope
// ---------------------------------------------------------------------------
//
// Ensures the gate applies transitively through `DidContractCall::decode`,
// which is what `RecordingBackend::submit_tx` (and a future `LiveBackend`
// reading externally-produced envelopes) actually invokes.

#[test]
fn did_contract_call_decode_rejects_malformed_signature_in_verify_variant() {
    use midnight_did_runtime::DidContractCall;

    // Hand-crafted JSON matching the `VerifySchnorrJubjubDigestSignature`
    // variant shape but with a malformed `signature.bytes_hex` payload.
    let coord = valid_coord();
    let payload = format!(
        r##"{{
            "VerifySchnorrJubjubDigestSignature": {{
                "method_id": "#key-1",
                "digest": ["{coord}","{coord}","{coord}","{coord}"],
                "signature": {{"bytes_hex":"deadbeef"}}
            }}
        }}"##
    );
    let err =
        DidContractCall::decode(payload.as_bytes()).expect_err("malformed inner signature must fail envelope decode");
    assert!(
        err.to_string().contains("DidContractCall"),
        "decode error should surface as the envelope-level BackendError::Decode, got: {err}"
    );
}

#[test]
fn did_contract_call_decode_rejects_malformed_pubkey_in_set_schnorr_variant() {
    use midnight_did_runtime::DidContractCall;

    // `SetSchnorrJubjubVerificationMethod` payload with a short `x`
    // coordinate inside the nested `JubjubPointHex`.
    let coord = valid_coord();
    let payload = format!(
        r##"{{
            "SetSchnorrJubjubVerificationMethod": {{
                "method": {{
                    "id": "#key-1",
                    "public_key": {{"x":"01","y":"{coord}"}}
                }},
                "mutation": "Insert"
            }}
        }}"##
    );
    let err = DidContractCall::decode(payload.as_bytes())
        .expect_err("malformed inner JubjubPointHex must fail envelope decode");
    assert!(err.to_string().contains("DidContractCall"));
}
