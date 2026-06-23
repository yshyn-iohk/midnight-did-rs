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

//! Full-hex round-trip helpers for the upstream 32-byte hash types
//! we reuse.
//!
//! Upstream `Display for HashOutput` is intentionally truncated to
//! the first 10 hex characters (a log-friendly preview, not a
//! round-trippable serialisation — see
//! `third_party/midnight-ledger/base-crypto/src/hash.rs:86`). The
//! Midnight DID document wire format encodes hashes as the **full
//! 64-character lowercase hex string**; the [`HashOutputExt`] trait
//! below provides that round-trip without introducing yet another
//! wrapper newtype around `[u8; 32]`.
//!
//! ```
//! use midnight_base_crypto::hash::HashOutput;
//! use midnight_did_method::hex_ext::HashOutputExt;
//!
//! let s = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
//! let h = HashOutput::from_hex(s).unwrap();
//! assert_eq!(h.to_hex(), s);
//! ```

use compact_runtime::ContractAddress;
use midnight_base_crypto::hash::HashOutput;

/// Errors returned by [`HashOutputExt::from_hex`].
///
/// `WrongLength` carries the actual length we saw (in characters,
/// not bytes) so callers can render a precise diagnostic without
/// re-counting.
#[derive(Debug, thiserror::Error)]
pub enum ParseHexError {
    /// The input did not have exactly 64 hex characters.
    #[error("expected 64 hex characters, got {0}")]
    WrongLength(usize),
    /// The input had the right length but contained non-hex
    /// characters or an invalid hex digit pairing.
    #[error("invalid hex: {0}")]
    InvalidHex(#[from] hex::FromHexError),
}

/// Extension trait for round-tripping 32-byte hash-shaped types
/// through their canonical 64-character lowercase hex string form.
///
/// Implemented for the upstream Midnight ledger primitives we
/// re-use directly:
///
/// - [`HashOutput`] — the generic 32-byte hash output from
///   `midnight-base-crypto`. Backs `OffchainStateHash` and any
///   identifier-shaped digest in the DID method.
/// - [`ContractAddress`] — `ContractAddress(pub HashOutput)`, the
///   on-chain identity slot. Delegates to the inner `HashOutput`.
///
/// The trait is intentionally narrow: it does **not** cover
/// alternative encodings (bech32, base58, multibase). If the wire
/// format ever needs those, a sibling `BechExt`/`MultibaseExt`
/// trait keeps the surface focused.
pub trait HashOutputExt: Sized {
    /// Parse a 64-character lowercase or mixed-case hex string into
    /// the 32-byte target. Returns [`ParseHexError::WrongLength`]
    /// when the input is the wrong length (the most common caller
    /// mistake) and [`ParseHexError::InvalidHex`] for any other
    /// hex-decoding failure.
    fn from_hex(s: &str) -> Result<Self, ParseHexError>;

    /// Render to the canonical 64-character lowercase hex string
    /// form. Always 64 chars, never truncated.
    fn to_hex(&self) -> String;
}

impl HashOutputExt for HashOutput {
    fn from_hex(s: &str) -> Result<Self, ParseHexError> {
        if s.len() != 64 {
            return Err(ParseHexError::WrongLength(s.len()));
        }
        let mut buf = [0u8; 32];
        hex::decode_to_slice(s, &mut buf)?;
        Ok(HashOutput(buf))
    }

    fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl HashOutputExt for ContractAddress {
    fn from_hex(s: &str) -> Result<Self, ParseHexError> {
        HashOutput::from_hex(s).map(Self)
    }

    fn to_hex(&self) -> String {
        // ContractAddress(pub HashOutput) — delegate to the inner
        // HashOutput's hex rendering for byte-for-byte parity with
        // anywhere else in the codebase that hex-encodes a 32-byte
        // hash.
        self.0.to_hex()
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests covering the smallest building blocks of the trait;
    //! richer behavioural coverage lives in `tests/hex_ext.rs`
    //! (integration test against the upstream-typed surface).
    use super::*;

    #[test]
    fn from_hex_empty_string_is_wrong_length() {
        let err = HashOutput::from_hex("").unwrap_err();
        assert!(matches!(err, ParseHexError::WrongLength(0)));
    }

    #[test]
    fn to_hex_of_zeroed_hash_is_64_zeros() {
        let h = HashOutput([0u8; 32]);
        assert_eq!(h.to_hex(), "0".repeat(64));
    }
}
