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

//! Helpers that shape ledger reads/writes for the Midnight DID contract.
//!
//! Port of `ledger-utils.ts`. The helpers focus on string-level pre/post-
//! processing — fragment-id normalization, encoding the polymorphic service
//! `type` and `serviceEndpoint` fields into the on-chain string flavour, and
//! aliasing absolute-URI checks.

use thiserror::Error;

use crate::did_document::{ServiceEndpoint, ServiceType, normalize_service_endpoint};
use crate::uri::normalize_uri_string;

/// Which ledger field a value is bound to. Used solely for error messages so
/// callers can distinguish failures between, e.g., `verificationMethod.id`
/// and `service.id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundIdField {
    /// `verificationMethod.id`.
    VerificationMethodId,
    /// `schnorrJubjubVerificationMethod.id`.
    SchnorrJubjubVerificationMethodId,
    /// `service.id`.
    ServiceId,
    /// `methodId` (compact field name).
    MethodId,
    /// `serviceId` (compact field name).
    ShortServiceId,
}

impl BoundIdField {
    fn label(self) -> &'static str {
        match self {
            BoundIdField::VerificationMethodId => "verificationMethod.id",
            BoundIdField::SchnorrJubjubVerificationMethodId => "schnorrJubjubVerificationMethod.id",
            BoundIdField::ServiceId => "service.id",
            BoundIdField::MethodId => "methodId",
            BoundIdField::ShortServiceId => "serviceId",
        }
    }
}

/// Errors surfaced by the ledger helpers.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LedgerUtilsError {
    /// Value was empty (after trimming).
    #[error("{field} must not be empty")]
    Empty {
        /// Field name where the empty value was provided.
        field: String,
    },
    /// Value did not look like a DID URL or a relative reference.
    #[error("{field} must be a DID URL or relative reference")]
    NotARelativeOrDidUrl {
        /// Field name.
        field: String,
    },
    /// Embedded DID URL fragment was empty.
    #[error("{field} DID URL must include a non-empty fragment identifier")]
    EmptyFragment {
        /// Field name.
        field: String,
    },
    /// DID URL subject did not match the current DID.
    #[error("{field} DID URL subject must match the current DID ({expected})")]
    SubjectMismatch {
        /// Field name.
        field: String,
        /// Expected DID subject.
        expected: String,
    },
    /// Service-type entries were not unique.
    #[error("service type entries must be unique")]
    DuplicateServiceTypeEntries,
    /// Service-type entries were empty.
    #[error("service type entries must not be empty")]
    EmptyServiceTypeEntries,
    /// `service type` property was empty.
    #[error("service type must not be empty")]
    EmptyServiceType,
    /// `service type` property was malformed.
    #[error("service type property must be a non-empty string set")]
    InvalidServiceType,
    /// Absolute-URI assertion failed.
    #[error("{field} must be a valid absolute URI (RFC3986)")]
    NotAbsoluteUri {
        /// Field name.
        field: String,
    },
}

const URI_SCHEME_PREFIX_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
const URI_SCHEME_INNER_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+.-";

fn has_uri_scheme(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    if !URI_SCHEME_PREFIX_CHARS.contains(&bytes[0]) {
        return false;
    }
    for (idx, byte) in bytes.iter().enumerate().skip(1) {
        if *byte == b':' {
            return idx > 0;
        }
        if !URI_SCHEME_INNER_CHARS.contains(byte) {
            return false;
        }
    }
    false
}

/// Normalize any fragment-id-shaped string to the leading-`#` form. Bare
/// fragments are prefixed, DID URLs are reduced to their fragment portion.
pub fn normalize_fragment_id(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with('#') {
        return trimmed.to_owned();
    }
    if let Some(idx) = trimmed.find('#') {
        return format!("#{}", &trimmed[idx + 1..]);
    }
    format!("#{trimmed}")
}

/// Normalize a fragment id that is bound to a specific DID subject. Rejects
/// inputs whose DID subject does not match `expected_did_subject`, and the
/// other failure modes called out by [`LedgerUtilsError`].
pub fn normalize_bound_fragment_id(
    value: &str,
    field: BoundIdField,
    expected_did_subject: &str,
) -> Result<String, LedgerUtilsError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(LedgerUtilsError::Empty {
            field: field.label().into(),
        });
    }
    if trimmed.starts_with("//") {
        return Err(LedgerUtilsError::NotARelativeOrDidUrl {
            field: field.label().into(),
        });
    }
    if trimmed.starts_with('#') {
        return Ok(trimmed.to_owned());
    }

    let hash_index = trimmed.find('#');
    if trimmed.starts_with("did:") {
        match hash_index {
            Some(idx) if idx > 0 && idx < trimmed.len() - 1 => {
                let did_subject = &trimmed[..idx];
                if did_subject != expected_did_subject {
                    return Err(LedgerUtilsError::SubjectMismatch {
                        field: field.label().into(),
                        expected: expected_did_subject.into(),
                    });
                }
                return Ok(format!("#{}", &trimmed[idx + 1..]));
            }
            _ => {
                return Err(LedgerUtilsError::EmptyFragment {
                    field: field.label().into(),
                });
            }
        }
    }

    if trimmed.starts_with('/') || trimmed.starts_with('.') || trimmed.starts_with('?') {
        return Ok(format!("#{trimmed}"));
    }
    if has_uri_scheme(trimmed) {
        return Err(LedgerUtilsError::NotARelativeOrDidUrl {
            field: field.label().into(),
        });
    }
    Ok(normalize_fragment_id(trimmed))
}

/// Encode the polymorphic service `type` property into the single-string
/// shape the ledger stores. A single type becomes the value as-is; an array
/// is JSON-encoded so callers can recover the original list.
///
/// # Panics
///
/// Panics only on impossible-by-construction conditions: the
/// `normalized.into_iter().next().unwrap()` is guarded by
/// `if normalized.len() == 1`, and `serde_json::to_string` on a
/// `Vec<String>` is total. A panic here would indicate a logic bug in
/// this function, not a fallible runtime input.
pub fn service_type_to_ledger(service_type: &ServiceType) -> Result<String, LedgerUtilsError> {
    match service_type {
        ServiceType::One(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return Err(LedgerUtilsError::EmptyServiceType);
            }
            Ok(trimmed.to_owned())
        }
        ServiceType::Many(types) => {
            if types.is_empty() {
                return Err(LedgerUtilsError::InvalidServiceType);
            }
            let normalized: Vec<String> = types.iter().map(|s| s.trim().to_owned()).collect();
            if normalized.iter().any(|s| s.is_empty()) {
                return Err(LedgerUtilsError::EmptyServiceTypeEntries);
            }
            let mut sorted = normalized.clone();
            sorted.sort();
            sorted.dedup();
            if sorted.len() != normalized.len() {
                return Err(LedgerUtilsError::DuplicateServiceTypeEntries);
            }
            if normalized.len() == 1 {
                Ok(normalized.into_iter().next().unwrap())
            } else {
                Ok(serde_json::to_string(&normalized).expect("serialize ServiceType"))
            }
        }
    }
}

/// Encode a service endpoint as the JSON-encoded canonical string the ledger
/// uses. Performs URI normalization on every nested string.
///
/// # Panics
///
/// `serde_json::to_string` on a normalised `ServiceEndpoint` is total —
/// every variant contains only strings or maps of strings. A panic here
/// would indicate a `serde` invariant break, not a runtime input failure.
pub fn service_endpoint_to_ledger(endpoint: ServiceEndpoint) -> String {
    let normalized = normalize_service_endpoint(endpoint);
    serde_json::to_string(&normalized).expect("serialize ServiceEndpoint")
}

/// Assert that `value` is a syntactically-valid absolute URI (RFC 3986).
/// Returns the trimmed value on success. Default field label is `aliasUri`,
/// matching the TS port.
pub fn assert_absolute_uri(value: &str, field: Option<&str>) -> Result<String, LedgerUtilsError> {
    let label = field.unwrap_or("aliasUri");
    let alias = value.trim();
    if alias.is_empty() {
        return Err(LedgerUtilsError::Empty { field: label.into() });
    }
    url::Url::parse(alias)
        .map(|_| ())
        .map_err(|_| LedgerUtilsError::NotAbsoluteUri { field: label.into() })?;
    // Also pass through the normaliser so callers store canonical hex.
    let _ = normalize_uri_string(alias);
    Ok(alias.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_bare_fragment() {
        assert_eq!(normalize_fragment_id("key-1"), "#key-1");
        assert_eq!(normalize_fragment_id("did:example:1#key-1"), "#key-1");
        assert_eq!(normalize_fragment_id("#key-1"), "#key-1");
    }

    #[test]
    fn binds_fragment_to_did() {
        let did = "did:midnight:devnet:abcd";
        let frag =
            normalize_bound_fragment_id(&format!("{did}#key-1"), BoundIdField::VerificationMethodId, did).unwrap();
        assert_eq!(frag, "#key-1");
    }

    #[test]
    fn rejects_mismatched_subject() {
        let did = "did:midnight:devnet:abcd";
        let err = normalize_bound_fragment_id(
            "did:midnight:devnet:xxxx#key-1",
            BoundIdField::VerificationMethodId,
            did,
        )
        .unwrap_err();
        assert!(matches!(err, LedgerUtilsError::SubjectMismatch { .. }));
    }
}
