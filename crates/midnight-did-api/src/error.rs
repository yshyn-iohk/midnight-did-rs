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

//! Domain-grouped error types for the Midnight DID API layer.
//!
//! R1 step 6 (v0.2.0): the previously flat `ApiError(13 variants)` is
//! split by failure category into focused enums:
//!
//! - [`VerificationError`] — verification-method add/remove/update
//!   failures (duplicate relations, missing relations).
//! - [`ControllerError`] — controller-key rotation failures
//!   (orphaned rotation, invalid secret length, subject mismatch).
//! - [`ContractError`] — on-chain contract call failures (existed
//!   pre-v0.2 as a nested enum; kept).
//!
//! [`ApiError`] is the umbrella: every public operation can return
//! it, and every domain enum lifts into it via `#[from]`. Callers
//! that only care about one domain can pattern-match the narrow
//! type directly without handling unrelated variants.

use thiserror::Error;

use midnight_did_domain::{crypto_codecs::CodecError, did_document::ValidationError, ledger_utils::LedgerUtilsError};
use midnight_did_method::midnight_did::MidnightDidError;

/// Error category returned by a [`crate::contract::DidContract`] implementation.
///
/// API-layer code wraps these as [`ApiError::Contract`].
#[derive(Debug, Error)]
pub enum ContractError {
    /// Underlying provider or network error message.
    #[error("contract call failed: {0}")]
    Failed(String),

    /// The contract is not deployed at the expected address.
    #[error("contract not deployed")]
    NotDeployed,

    /// State could not be read from the ledger.
    #[error("ledger state unavailable")]
    StateUnavailable,
}

/// Verification-method-domain errors. Captures the small set of
/// operation-specific failures that can occur during VM add / remove
/// / update / relation-management.
#[derive(Debug, Error)]
pub enum VerificationError {
    /// A relation already contains a verification method that callers
    /// attempted to add.
    #[error("relation {relation} already contains verification method {method_id}")]
    RelationAlreadyContains {
        /// Relation name (e.g. `"Authentication"`).
        relation: String,
        /// Normalized fragment id of the verification method.
        method_id: String,
    },

    /// A relation does not contain a verification method callers
    /// attempted to remove.
    #[error("relation {relation} does not contain verification method {method_id}")]
    RelationMissing {
        /// Relation name.
        relation: String,
        /// Normalized fragment id of the verification method.
        method_id: String,
    },
}

/// Controller-key-domain errors. Captures rotation, secret-key, and
/// controller-subject coherence failures.
#[derive(Debug, Error)]
pub enum ControllerError {
    /// A controller key rotation succeeded on-chain but the pending
    /// private state could not be promoted to active. The caller may
    /// invoke
    /// [`crate::private_state::recover_pending_controller_private_state`]
    /// once the transaction is confirmed to clean up.
    #[error("controller rotation finalized but pending state promotion failed: {0}")]
    RotationOrphaned(String),

    /// The new secret-key argument to a rotation was not exactly 32
    /// bytes.
    #[error("DID controller secret key must be 32 bytes")]
    InvalidSecretKey,

    /// `verificationMethod.controller` is not equal to the resolved
    /// DID subject.
    #[error("verificationMethod.controller must equal DID subject ({expected})")]
    SubjectMismatch {
        /// Expected DID string (i.e. `did:midnight:<network>:<address>`).
        expected: String,
    },
}

/// Top-level umbrella error returned by every public API operation
/// that touches more than one domain. Each domain-grouped error
/// (`VerificationError`, `ControllerError`, `ContractError`) lifts
/// into this via `#[from]`, so operation code can use the `?`
/// operator across domains without manual matching.
#[derive(Debug, Error)]
pub enum ApiError {
    // ---- Domain-grouped lifts (R1 step 6) ---------------------------
    /// Verification-method-domain failure (relation add/remove,
    /// duplicate methods, ...).
    #[error(transparent)]
    Verification(#[from] VerificationError),

    /// Controller-key-domain failure (rotation orphaned, bad secret
    /// length, controller/subject mismatch, ...).
    #[error(transparent)]
    Controller(#[from] ControllerError),

    /// On-chain contract call failure.
    #[error(transparent)]
    Contract(#[from] ContractError),

    // ---- Crate-spanning transparents -------------------------------
    /// A domain-level validation rule failed.
    #[error(transparent)]
    Validation(#[from] ValidationError),

    /// A codec (base64url, hex, JSON, ...) failed.
    #[error(transparent)]
    Codec(#[from] CodecError),

    /// A ledger-utils helper rejected an input.
    #[error(transparent)]
    LedgerUtils(#[from] LedgerUtilsError),

    /// A Midnight-DID parser rejected an input.
    #[error(transparent)]
    MidnightDid(#[from] MidnightDidError),

    // ---- Cross-domain leftovers ------------------------------------
    /// A private-state read returned `None` when a value was required.
    #[error(
        "DID controller private state is missing or malformed; import the controller secret before using this contract"
    )]
    MissingPrivateState,

    /// An argument value violated a documented precondition.
    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    /// Encoding of a value (JWK coordinate, JSON, ...) failed.
    #[error("encoding error: {0}")]
    Encoding(String),

    /// A ledger byte mapping failed (e.g. JWK coordinate decode).
    #[error("ledger mapping error: {0}")]
    Mapping(String),
}

impl ApiError {
    /// Build an `InvalidArgument` variant from any displayable value.
    pub fn invalid_argument(msg: impl Into<String>) -> Self {
        ApiError::InvalidArgument(msg.into())
    }

    /// Build a `Mapping` variant.
    pub fn mapping(msg: impl Into<String>) -> Self {
        ApiError::Mapping(msg.into())
    }
}
