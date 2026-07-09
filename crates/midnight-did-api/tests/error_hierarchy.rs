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

//! Integration tests for the R1 step 6 error hierarchy.
//!
//! The flat 13-variant `ApiError` is split into:
//!
//! - [`VerificationError`] (RelationAlreadyContains, RelationMissing)
//! - [`ControllerError`] (RotationOrphaned, InvalidSecretKey, SubjectMismatch)
//! - [`ContractError`] (existed pre-R1, untouched)
//!
//! These domain enums lift into the umbrella `ApiError` via
//! `#[from]`. These tests pin three properties:
//!
//! 1. **Construction**: domain enums can be built directly.
//! 2. **Lift via `?`**: each domain enum auto-lifts into ApiError.
//! 3. **Display passthrough**: ApiError's transparent Display
//!    renders the inner enum's message unchanged.

use midnight_did_api::error::{ApiError, ContractError, ControllerError, VerificationError};

// ---- Construction --------------------------------------------------

#[test]
fn verification_error_constructs_relation_already_contains() {
    let err = VerificationError::RelationAlreadyContains {
        relation: "Authentication".to_string(),
        method_id: "#key-1".to_string(),
    };
    let msg = format!("{err}");
    assert!(msg.contains("Authentication"));
    assert!(msg.contains("#key-1"));
}

#[test]
fn controller_error_constructs_invalid_secret_key() {
    let err = ControllerError::InvalidSecretKey;
    assert!(format!("{err}").contains("32 bytes"));
}

#[test]
fn controller_error_constructs_rotation_orphaned() {
    let err = ControllerError::RotationOrphaned("provider lost".to_string());
    assert!(format!("{err}").contains("provider lost"));
}

#[test]
fn controller_error_constructs_subject_mismatch() {
    let err = ControllerError::SubjectMismatch {
        expected: "did:midnight:testnet:abc".to_string(),
    };
    assert!(format!("{err}").contains("did:midnight:testnet:abc"));
}

#[test]
fn contract_error_constructs_state_unavailable() {
    let err = ContractError::StateUnavailable;
    assert!(format!("{err}").contains("ledger state unavailable"));
}

// ---- Lift via `?` (From impls) -------------------------------------

#[test]
fn verification_error_lifts_into_api_error() {
    let domain_err = VerificationError::RelationMissing {
        relation: "Auth".to_string(),
        method_id: "#k".to_string(),
    };
    let api_err: ApiError = domain_err.into();
    assert!(matches!(
        api_err,
        ApiError::Verification(VerificationError::RelationMissing { .. })
    ));
}

#[test]
fn controller_error_lifts_into_api_error() {
    let domain_err = ControllerError::InvalidSecretKey;
    let api_err: ApiError = domain_err.into();
    assert!(matches!(
        api_err,
        ApiError::Controller(ControllerError::InvalidSecretKey)
    ));
}

#[test]
fn contract_error_lifts_into_api_error() {
    let domain_err = ContractError::NotDeployed;
    let api_err: ApiError = domain_err.into();
    assert!(matches!(api_err, ApiError::Contract(ContractError::NotDeployed)));
}

// ---- `?` operator threads lifted errors through ---------------------

fn returns_verification_error() -> Result<(), VerificationError> {
    Err(VerificationError::RelationMissing {
        relation: "Auth".to_string(),
        method_id: "#k".to_string(),
    })
}

fn umbrella_caller() -> Result<(), ApiError> {
    returns_verification_error()?; // VerificationError → ApiError via #[from]
    Ok(())
}

#[test]
fn question_mark_operator_threads_domain_error_into_umbrella() {
    let result = umbrella_caller();
    assert!(matches!(
        result,
        Err(ApiError::Verification(VerificationError::RelationMissing { .. }))
    ));
}

// ---- Display passthrough -------------------------------------------

#[test]
fn api_error_display_passes_through_verification_message() {
    let api_err: ApiError = VerificationError::RelationAlreadyContains {
        relation: "Auth".to_string(),
        method_id: "#k".to_string(),
    }
    .into();
    let api_msg = format!("{api_err}");
    assert!(api_msg.contains("Auth"));
    assert!(api_msg.contains("#k"));
}

#[test]
fn api_error_display_passes_through_controller_message() {
    let api_err: ApiError = ControllerError::InvalidSecretKey.into();
    assert!(format!("{api_err}").contains("32 bytes"));
}

// ---- Narrow pattern-match doesn't need the umbrella -----------------

#[test]
fn callers_can_pattern_match_on_narrow_type() {
    // Simulates a caller that only handles VerificationError outcomes
    // — they don't need to consider the 9+ unrelated ApiError variants.
    fn handle_only_verification(e: &VerificationError) -> &str {
        match e {
            VerificationError::RelationAlreadyContains { .. } => "duplicate",
            VerificationError::RelationMissing { .. } => "missing",
        }
    }
    let err = VerificationError::RelationMissing {
        relation: "Auth".to_string(),
        method_id: "#k".to_string(),
    };
    assert_eq!(handle_only_verification(&err), "missing");
}
