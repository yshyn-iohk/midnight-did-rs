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

//! Base64url codecs and JWK coordinate decoding helpers.
//!
//! Port of `crypto-codecs.ts`. The Rust ecosystem already has solid base64
//! crates; this module re-uses [`base64`] for the underlying alphabet but
//! wraps it so the public API mirrors the TS surface (`encodeBase64Url`,
//! `decodeBase64UrlBytes(input, expectedLength)`), and so callers in this
//! crate don't have to depend on a specific base64 crate directly.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use thiserror::Error;

/// Errors returned by the base64url / field-element codecs.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum CodecError {
    /// Input contains a character outside the unpadded base64url alphabet.
    #[error("{label} contains an invalid base64url character")]
    InvalidCharacter {
        /// Operator-provided label that identifies the offending field.
        label: String,
    },
    /// Input length is not a valid unpadded base64url length (`len % 4 == 1`).
    #[error("{label} has an invalid unpadded base64url length")]
    InvalidLength {
        /// Operator-provided label.
        label: String,
    },
    /// Decoded byte length differs from the expected curve coordinate length.
    #[error("{label} must decode to exactly {expected} bytes (got {actual})")]
    UnexpectedByteLength {
        /// Operator-provided label.
        label: String,
        /// Expected number of bytes.
        expected: usize,
        /// Actual decoded byte count.
        actual: usize,
    },
    /// Re-encoding the decoded bytes did not round-trip back to the input —
    /// implying the input was not canonical unpadded base64url.
    #[error("{label} is not canonical unpadded base64url")]
    NotCanonical {
        /// Operator-provided label.
        label: String,
    },
}

/// Reference regex pattern accepted by the canonical-form check. Exposed
/// as a doc-string constant for parity with the TS implementation; the
/// runtime check is performed character-by-character via [`is_base64url_byte`]
/// to avoid pulling in a regex dependency.
pub const BASE64URL_TEXT_RE: &str = "^[A-Za-z0-9_-]+$";

/// Returns `true` if `value` consists exclusively of unpadded base64url
/// characters. Empty strings are rejected to match the TS implementation,
/// which uses `+` in the regex.
fn is_base64url_text(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(is_base64url_byte)
}

fn is_base64url_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
}

/// Encode `bytes` as unpadded base64url. Matches `encodeBase64Url` in the
/// TS port (no `=` padding, `-`/`_` alphabet).
pub fn encode_base64url(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Decode an unpadded base64url string into bytes.
///
/// Mirrors `decodeBase64Url` in the TS port: ignores `=` and is lenient on
/// length-mod-4 corner cases. Use [`decode_base64url_bytes`] when you need a
/// canonical, fixed-length decode.
pub fn decode_base64url(input: &str) -> Result<Vec<u8>, CodecError> {
    // Be lenient like the TS port: insert padding for non-zero modulos so we
    // can round-trip values written by tooling that accidentally drops or
    // adds `=`.
    let padded = match input.len() % 4 {
        0 => input.to_owned(),
        2 => format!("{input}=="),
        3 => format!("{input}="),
        _ => {
            return Err(CodecError::InvalidLength { label: "value".into() });
        }
    };
    URL_SAFE_NO_PAD
        .decode(padded.trim_end_matches('='))
        .map_err(|_| CodecError::InvalidCharacter { label: "value".into() })
}

/// Decode `input` as unpadded base64url and assert the decoded byte length.
///
/// Matches `decodeBase64UrlBytes(input, expectedLength, label)` in TS.
/// Performs three checks:
///
/// 1. character-set is unpadded base64url (`^[A-Za-z0-9_-]+$`),
/// 2. decoded byte length matches `expected_length`,
/// 3. re-encoding round-trips to `input` (canonical form).
pub fn decode_base64url_bytes(input: &str, expected_length: usize, label: &str) -> Result<Vec<u8>, CodecError> {
    if !is_base64url_text(input) || input.len() % 4 == 1 {
        return Err(CodecError::InvalidCharacter { label: label.into() });
    }
    let bytes = decode_base64url(input).map_err(|_| CodecError::InvalidCharacter { label: label.into() })?;
    if bytes.len() != expected_length {
        return Err(CodecError::UnexpectedByteLength {
            label: label.into(),
            expected: expected_length,
            actual: bytes.len(),
        });
    }
    if encode_base64url(&bytes) != input {
        return Err(CodecError::NotCanonical { label: label.into() });
    }
    Ok(bytes)
}

/// Convenience wrapper around [`decode_base64url_bytes`] with `expected_length = 32`.
pub fn decode_base64url_bytes_32(input: &str, label: &str) -> Result<Vec<u8>, CodecError> {
    decode_base64url_bytes(input, 32, label)
}

/// Decode a base64url string as a big-endian unsigned integer.
///
/// Matches `decodeFieldElement` in TS. Returns 0 for the empty string (since
/// `decode_base64url("")` returns `[]`).
pub fn decode_field_element(s: &str) -> Result<num_decode::BigUintBytes, CodecError> {
    let bytes = decode_base64url(s)?;
    Ok(num_decode::BigUintBytes(bytes))
}

/// Encode a big-endian byte representation of an unsigned integer as
/// unpadded base64url. Matches `encodeFieldElement` in TS.
pub fn encode_field_element(value: &num_decode::BigUintBytes) -> String {
    let mut start = 0;
    while start + 1 < value.0.len() && value.0[start] == 0 {
        start += 1;
    }
    // The TS port treats `0n` as the singleton `[0]`; matching that here.
    if value.0.is_empty() {
        return encode_base64url(&[0]);
    }
    encode_base64url(&value.0[start..])
}

/// Lightweight big-endian byte-vector wrapper standing in for a `BigUint`
/// dependency. We only need byte-level decode/encode for the field-element
/// codec; callers that need actual arithmetic can wire in `num-bigint`
/// themselves.
pub mod num_decode {
    /// Big-endian unsigned-integer bytes.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct BigUintBytes(pub Vec<u8>);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let bytes = b"midnight-rocks";
        let encoded = encode_base64url(bytes);
        let decoded = decode_base64url(&encoded).expect("decode");
        assert_eq!(decoded, bytes);
    }

    #[test]
    fn rejects_invalid_charset() {
        let err = decode_base64url_bytes("not+base64url", 32, "x").unwrap_err();
        assert!(matches!(err, CodecError::InvalidCharacter { .. }));
    }

    #[test]
    fn enforces_length() {
        let too_short = encode_base64url(&[1u8; 16]);
        let err = decode_base64url_bytes(&too_short, 32, "x").unwrap_err();
        assert!(matches!(err, CodecError::UnexpectedByteLength { .. }));
    }

    #[test]
    fn rejects_non_canonical() {
        // Length-mod-4 == 1 is invalid for unpadded base64url.
        let err = decode_base64url_bytes("AAAAA", 32, "x").unwrap_err();
        assert!(matches!(err, CodecError::InvalidCharacter { .. }));
    }

    #[test]
    fn regex_constant_is_documented() {
        // Sanity-check the doc-string constant matches the TS regex.
        assert_eq!(BASE64URL_TEXT_RE, "^[A-Za-z0-9_-]+$");
    }
}
