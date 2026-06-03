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

//! Unified error type for the Midnight DID API layer.
//!
//! All public operations return [`ApiError`]. Variants are organised by the
//! failure category so callers can pattern-match on the error category without
//! relying on message strings.

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

/// Top-level error returned by every public API operation.
#[derive(Debug, Error)]
pub enum ApiError {
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

    /// The on-chain contract call failed.
    #[error(transparent)]
    Contract(#[from] ContractError),

    /// A controller key rotation failed and the pending private state was
    /// orphaned. The caller may invoke
    /// [`crate::private_state::recover_pending_controller_private_state`] once
    /// the transaction is confirmed.
    #[error("controller rotation finalized but pending state promotion failed: {0}")]
    ControllerRotationOrphaned(String),

    /// A private-state read returned `None` when a value was required.
    #[error(
        "DID controller private state is missing or malformed; import the controller secret before using this contract"
    )]
    MissingPrivateState,

    /// The new secret-key argument to a rotation was not exactly 32 bytes.
    #[error("DID controller secret key must be 32 bytes")]
    InvalidSecretKey,

    /// A relation already contains a verification method that callers
    /// attempted to add.
    #[error("relation {relation} already contains verification method {method_id}")]
    RelationAlreadyContains {
        /// Relation name (e.g. `"Authentication"`).
        relation: String,
        /// Normalized fragment id of the verification method.
        method_id: String,
    },

    /// A relation does not contain a verification method callers attempted to
    /// remove.
    #[error("relation {relation} does not contain verification method {method_id}")]
    RelationMissing {
        /// Relation name.
        relation: String,
        /// Normalized fragment id of the verification method.
        method_id: String,
    },

    /// An argument value violated a documented precondition.
    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    /// Encoding of a value (JWK coordinate, JSON, ...) failed.
    #[error("encoding error: {0}")]
    Encoding(String),

    /// `verificationMethod.controller` is not equal to the resolved DID
    /// subject.
    #[error("verificationMethod.controller must equal DID subject ({expected})")]
    ControllerSubjectMismatch {
        /// Expected DID string (i.e. `did:midnight:<network>:<address>`).
        expected: String,
    },

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
