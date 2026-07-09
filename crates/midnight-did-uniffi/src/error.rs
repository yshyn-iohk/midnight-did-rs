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

//! Flattened FFI error type.
//!
//! The api-layer [`midnight_did_api::error::ApiError`] uses
//! `#[from]`-driven nested error enums (`ContractError`, `ValidationError`,
//! …) that uniffi cannot encode across the FFI boundary. We flatten the
//! taxonomy down to a single enum with one variant per failure category and
//! a single `message` field — uniffi-friendly, no generics, no nested error
//! payloads.

use midnight_did_api::error::ApiError;
use thiserror::Error;

/// FFI-flat error returned by every exported async function.
///
/// Each variant maps onto a category of failures from the underlying
/// [`midnight_did_api`] stack. The original error message is preserved in
/// the `message` field so foreign-language callers can surface it to users.
#[derive(Debug, Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum FlatError {
    /// An input argument failed a pre-flight check (hex decode, JSON parse,
    /// length mismatch, …).
    #[error("invalid input: {message}")]
    InvalidInput {
        /// Human-readable description of what was wrong.
        message: String,
    },

    /// The contract call itself failed (network error, provider rejection,
    /// not-deployed, …).
    #[error("contract error: {message}")]
    Contract {
        /// Human-readable description of the contract failure.
        message: String,
    },

    /// A domain-level validation rule was violated.
    #[error("validation error: {message}")]
    Validation {
        /// Human-readable description of the validation failure.
        message: String,
    },

    /// JSON (de)serialisation failed.
    #[error("serde error: {message}")]
    Serde {
        /// Human-readable description of the (de)serialisation failure.
        message: String,
    },

    /// A required resource was not found (e.g. resolve returned `None`).
    #[error("not found: {message}")]
    NotFound {
        /// Human-readable description of what was missing.
        message: String,
    },
}

impl FlatError {
    /// Convenience constructor for [`FlatError::InvalidInput`].
    pub fn invalid_input(msg: impl Into<String>) -> Self {
        FlatError::InvalidInput { message: msg.into() }
    }

    /// Convenience constructor for [`FlatError::Contract`].
    pub fn contract(msg: impl Into<String>) -> Self {
        FlatError::Contract { message: msg.into() }
    }

    /// Convenience constructor for [`FlatError::Validation`].
    pub fn validation(msg: impl Into<String>) -> Self {
        FlatError::Validation { message: msg.into() }
    }

    /// Convenience constructor for [`FlatError::Serde`].
    pub fn serde(msg: impl Into<String>) -> Self {
        FlatError::Serde { message: msg.into() }
    }

    /// Convenience constructor for [`FlatError::NotFound`].
    pub fn not_found(msg: impl Into<String>) -> Self {
        FlatError::NotFound { message: msg.into() }
    }
}

impl From<ApiError> for FlatError {
    fn from(err: ApiError) -> Self {
        use midnight_did_api::error::{ControllerError, VerificationError};
        match err {
            // R1 step 6: domain-grouped error lifts.
            ApiError::Verification(VerificationError::RelationAlreadyContains { relation, method_id }) => {
                FlatError::validation(format!(
                    "relation {relation} already contains verification method {method_id}"
                ))
            }
            ApiError::Verification(VerificationError::RelationMissing { relation, method_id }) => {
                FlatError::validation(format!(
                    "relation {relation} does not contain verification method {method_id}"
                ))
            }
            ApiError::Controller(ControllerError::RotationOrphaned(msg)) => {
                FlatError::contract(format!("controller rotation orphaned: {msg}"))
            }
            ApiError::Controller(ControllerError::InvalidSecretKey) => {
                FlatError::invalid_input("DID controller secret key must be 32 bytes".to_string())
            }
            ApiError::Controller(ControllerError::SubjectMismatch { expected }) => FlatError::validation(format!(
                "verificationMethod.controller must equal DID subject ({expected})"
            )),
            ApiError::Contract(e) => FlatError::contract(e.to_string()),

            // Crate-spanning transparents.
            ApiError::Validation(e) => FlatError::validation(e.to_string()),
            ApiError::Codec(e) => FlatError::invalid_input(e.to_string()),
            ApiError::LedgerUtils(e) => FlatError::validation(e.to_string()),
            ApiError::MidnightDid(e) => FlatError::validation(e.to_string()),

            // Cross-domain leftovers.
            ApiError::MissingPrivateState => {
                FlatError::invalid_input("DID controller private state is missing or malformed".to_string())
            }
            ApiError::InvalidArgument(msg) => FlatError::invalid_input(msg),
            ApiError::Encoding(msg) => FlatError::serde(msg),
            ApiError::Mapping(msg) => FlatError::serde(msg),
        }
    }
}

impl From<serde_json::Error> for FlatError {
    fn from(err: serde_json::Error) -> Self {
        FlatError::serde(err.to_string())
    }
}

impl From<hex::FromHexError> for FlatError {
    fn from(err: hex::FromHexError) -> Self {
        FlatError::invalid_input(format!("hex decode failed: {err}"))
    }
}

/// Decode a 32-byte hex argument or return a [`FlatError::InvalidInput`] with
/// the supplied parameter name.
pub(crate) fn decode_hex_32(input: &str, param: &str) -> Result<[u8; 32], FlatError> {
    let bytes = hex::decode(input).map_err(FlatError::from)?;
    if bytes.len() != 32 {
        return Err(FlatError::invalid_input(format!(
            "{param} must be exactly 32 bytes (got {})",
            bytes.len()
        )));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}
