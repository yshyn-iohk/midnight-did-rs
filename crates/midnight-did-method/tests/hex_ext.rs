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

//! Integration tests for `midnight_did_method::hex_ext`.
//!
//! Validates the `HashOutputExt` extension trait that bridges the
//! truncated `Display` impl on upstream `HashOutput` (10-char preview
//! for logs) with the full 64-character hex round-trip the Midnight
//! DID document wire format uses.

use compact_runtime::ContractAddress;
use midnight_base_crypto::hash::HashOutput;
use midnight_did_method::hex_ext::{HashOutputExt, ParseHexError};

const ZERO_HEX: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";
const ARBITRARY_HEX: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[test]
fn hash_output_round_trips_through_hex() {
    let parsed = HashOutput::from_hex(ARBITRARY_HEX).expect("parse");
    let re_emitted = parsed.to_hex();
    assert_eq!(re_emitted, ARBITRARY_HEX);
}

#[test]
fn hash_output_zero_round_trips() {
    let parsed = HashOutput::from_hex(ZERO_HEX).expect("parse zero");
    assert_eq!(parsed.0, [0u8; 32]);
    assert_eq!(parsed.to_hex(), ZERO_HEX);
}

#[test]
fn hash_output_to_hex_is_full_64_chars() {
    // Upstream Display truncates to 10 chars for logs; to_hex must
    // emit the full 64.
    let parsed = HashOutput::from_hex(ARBITRARY_HEX).expect("parse");
    assert_eq!(parsed.to_hex().len(), 64);
}

#[test]
fn hash_output_from_hex_rejects_short_string() {
    let err =
        HashOutput::from_hex("abcd").expect_err("short hex should be rejected");
    match err {
        ParseHexError::WrongLength(n) => assert_eq!(n, 4),
        other => panic!("expected WrongLength(4), got {other:?}"),
    }
}

#[test]
fn hash_output_from_hex_rejects_long_string() {
    let long = "a".repeat(65);
    let err = HashOutput::from_hex(&long).expect_err("long hex rejected");
    match err {
        ParseHexError::WrongLength(n) => assert_eq!(n, 65),
        other => panic!("expected WrongLength(65), got {other:?}"),
    }
}

#[test]
fn hash_output_from_hex_rejects_non_hex_chars() {
    let bad =
        "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
    assert_eq!(bad.len(), 64);
    let err = HashOutput::from_hex(bad).expect_err("non-hex rejected");
    assert!(
        matches!(err, ParseHexError::InvalidHex(_)),
        "expected InvalidHex, got {err:?}",
    );
}

#[test]
fn contract_address_round_trips_through_hex() {
    let parsed = ContractAddress::from_hex(ARBITRARY_HEX).expect("parse");
    let re_emitted = parsed.to_hex();
    assert_eq!(re_emitted, ARBITRARY_HEX);
}

#[test]
fn contract_address_inner_hash_output_matches() {
    // ContractAddress(pub HashOutput) — from_hex(s) should produce
    // a ContractAddress whose inner HashOutput's bytes match the
    // hex-decoded input.
    let addr = ContractAddress::from_hex(ARBITRARY_HEX).expect("parse");
    let expected_bytes = HashOutput::from_hex(ARBITRARY_HEX).expect("parse").0;
    assert_eq!(addr.0.0, expected_bytes);
}

#[test]
fn contract_address_from_hex_rejects_wrong_length() {
    let err = ContractAddress::from_hex("deadbeef")
        .expect_err("short hex rejected");
    assert!(matches!(err, ParseHexError::WrongLength(8)));
}

#[test]
fn parse_hex_error_displays_helpfully() {
    let wrong_length = ParseHexError::WrongLength(7);
    let msg = format!("{wrong_length}");
    assert!(msg.contains("64"));
    assert!(msg.contains("7"));
}

#[test]
fn round_trip_property_for_random_byte_arrays() {
    // Property-style check: from_hex(to_hex(h)) == h for arbitrary
    // 32-byte arrays. Loop over a deterministic spread of bit
    // patterns rather than introducing a `rand`/`proptest` dep.
    for byte in (0u8..=255u8).step_by(7) {
        let bytes = [byte; 32];
        let hash = HashOutput(bytes);
        let s = hash.to_hex();
        let back = HashOutput::from_hex(&s).expect("round-trip");
        assert_eq!(back.0, bytes, "round-trip failed for byte {byte}");
    }
}
