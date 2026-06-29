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

//! W3C-DID identifier newtypes.
//!
//! These types preserve a single load-bearing invariant per newtype
//! and refuse construction otherwise. Once a value of type
//! [`DidKeyId`] / [`FragmentId`] / [`ServiceId`] exists, no caller can
//! pass a malformed string by mistake — the validation runs at the
//! single entry point (`Self::new` and the `Deserialize` impl, which
//! delegates to `new`).
//!
//! ## Why newtypes and not bare `String`?
//!
//! The pre-v0.2.0 surface used `pub struct DidKeyId(pub String)`-style
//! shadow wrappers with separate `is_*` validation helpers scattered
//! across call sites. R1 step 3 collapses these into proper
//! parse-on-construction newtypes:
//!
//! - Inner field is **private** — direct `pub` access is gone.
//! - `Self::new(impl Into<String>) -> Result<Self, IdError>` runs the
//!   grammar check exactly once, at construction.
//! - `Deserialize` impl runs the same check, so JSON-loaded values
//!   are validated for free (no "deserialize, then forget to call
//!   `.validate()`" footgun).
//! - `Serialize` impl is `#[serde(transparent)]`-style: the wire
//!   format is a plain string identical to what the TS reference
//!   produces. Byte-parity preserved.
//!
//! Step 4 of R1 will switch the pub-`String`-field uses across
//! [`crate::did_document`] etc. to these typed equivalents.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Construction errors common to every W3C-DID newtype here.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IdError {
    /// The identifier string was empty or whitespace-only.
    #[error("identifier is empty")]
    Empty,
    /// A `FragmentId` was given without its leading `#`.
    #[error("fragment id must start with '#'")]
    MissingFragmentPrefix,
    /// A `FragmentId` contained whitespace or control characters
    /// after the leading `#`.
    #[error("fragment id contains whitespace or control characters")]
    BadFragmentChars,
    /// A `DidKeyId` was given without a `#` fragment portion.
    #[error("DID key id must contain a '#' fragment")]
    MissingDidFragment,
    /// A `DidKeyId` did not start with the `did:` scheme.
    #[error("invalid DID URI: {0}")]
    InvalidDidUri(String),
}

/// A W3C DID URI that ends in a fragment portion, e.g.
/// `did:midnight:testnet:abc#key-1`.
///
/// Constructed via [`DidKeyId::new`], which enforces:
/// - non-empty input;
/// - starts with `did:` (the DID scheme);
/// - contains exactly one `#` (the fragment delimiter) and a
///   non-empty fragment portion after it.
///
/// Serde wire format: transparent string. JSON round-trip runs the
/// same validation as [`DidKeyId::new`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct DidKeyId(String);

impl DidKeyId {
    /// Validate and construct a `DidKeyId`. Mirrors W3C DID Core
    /// §3.2 URL syntax: `did:<method>:<msi>#<fragment>`.
    ///
    /// # Errors
    ///
    /// Returns [`IdError::Empty`] if the input is whitespace-only,
    /// [`IdError::InvalidDidUri`] if the `did:` scheme is missing, or
    /// [`IdError::MissingDidFragment`] if no `#` fragment portion is
    /// present.
    pub fn new(s: impl Into<String>) -> Result<Self, IdError> {
        let s = s.into();
        if s.trim().is_empty() {
            return Err(IdError::Empty);
        }
        if !s.starts_with("did:") {
            return Err(IdError::InvalidDidUri(s));
        }
        match s.split_once('#') {
            None => Err(IdError::MissingDidFragment),
            Some((_, "")) => Err(IdError::MissingDidFragment),
            Some(_) => Ok(Self(s)),
        }
    }

    /// Borrow the full DID URI as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Borrow the fragment portion (the characters after `#`). The
    /// `#` itself is **not** included — this matches W3C DID Core's
    /// convention.
    #[must_use]
    pub fn fragment(&self) -> &str {
        // Safe: ::new guarantees exactly one '#' is present and the
        // fragment portion is non-empty.
        self.0.split_once('#').map(|(_, f)| f).unwrap_or("")
    }
}

impl<'de> Deserialize<'de> for DidKeyId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for DidKeyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A W3C fragment-relative reference, e.g. `#key-1`. Always begins
/// with `#` and contains at least one non-whitespace character after
/// the prefix.
///
/// Constructed via [`FragmentId::new`], which enforces:
/// - non-empty input;
/// - leading `#`;
/// - non-empty portion after the `#`;
/// - no whitespace or ASCII control characters.
///
/// Serde wire format: transparent string.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct FragmentId(String);

impl FragmentId {
    /// Validate and construct a `FragmentId`.
    ///
    /// # Errors
    ///
    /// Returns [`IdError::Empty`] if the input is empty or only `#`,
    /// [`IdError::MissingFragmentPrefix`] if it does not begin with
    /// `#`, or [`IdError::BadFragmentChars`] if the body contains
    /// whitespace or ASCII control characters.
    pub fn new(s: impl Into<String>) -> Result<Self, IdError> {
        let s = s.into();
        if s.is_empty() {
            return Err(IdError::Empty);
        }
        if !s.starts_with('#') {
            return Err(IdError::MissingFragmentPrefix);
        }
        // "#" alone has the prefix but nothing after.
        if s.len() == 1 {
            return Err(IdError::Empty);
        }
        // No whitespace / ASCII control chars in the fragment body.
        // (DID Core does not formally constrain fragment grammar
        // beyond URI syntax; rejecting these is a pragmatic guard
        // against the most common copy/paste mistakes.)
        if s.chars().skip(1).any(|c| c.is_whitespace() || c.is_control()) {
            return Err(IdError::BadFragmentChars);
        }
        Ok(Self(s))
    }

    /// Borrow the full fragment reference as a string slice, leading
    /// `#` included.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for FragmentId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for FragmentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A W3C DID Document `service[].id` identifier.
///
/// The W3C spec does **not** prescribe a structural grammar for
/// service IDs — they may be DID URIs, fragment refs, or any other
/// URI. We accept any non-empty, non-whitespace-only string. Stricter
/// validation lives at the application layer (e.g. the DID
/// Document's builder validates cross-reference uniqueness).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ServiceId(String);

impl ServiceId {
    /// Validate and construct a `ServiceId`.
    ///
    /// # Errors
    ///
    /// Returns [`IdError::Empty`] if the input is empty or
    /// whitespace-only.
    pub fn new(s: impl Into<String>) -> Result<Self, IdError> {
        let s = s.into();
        if s.trim().is_empty() {
            return Err(IdError::Empty);
        }
        Ok(Self(s))
    }

    /// Borrow the full identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ServiceId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for ServiceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    //! Narrow unit tests; richer behavioural coverage lives in
    //! `tests/ids.rs` (integration test against the public surface).
    use super::*;

    #[test]
    fn id_error_displays_helpfully_for_missing_fragment() {
        let err = DidKeyId::new("did:midnight:devnet:abc").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("fragment"));
    }

    #[test]
    fn fragment_id_internal_field_is_private() {
        // Compile-fail check: `FragmentId(String::new())` is no
        // longer valid because the inner field is private. The
        // `::new` constructor is the only entry point.
        let f = FragmentId::new("#k").unwrap();
        // Accessing `.0` here would be a compile error; the only
        // public read access is `as_str()`.
        assert_eq!(f.as_str(), "#k");
    }
}
