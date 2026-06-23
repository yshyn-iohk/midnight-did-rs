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

//! Pure-data domain types for the Midnight DID Method.
//!
//! This crate is the Rust port of `@midnight-ntwrk/midnight-did-domain` and
//! intentionally avoids any dependency on the on-chain runtime, ledger crates
//! or the generated contract surface so it can be reused in client SDKs,
//! resolvers, and CLI tools without dragging in the Midnight halo2 stack.
//!
//! Module layout mirrors the TypeScript source files in
//! `@midnight-ntwrk/midnight-did-domain/src/`:
//!
//! - [`crypto_codecs`] — base64url helpers and JWK coordinate decoding.
//! - [`did_document`] — W3C DID Core 1.0 data model + cross-consistency validation.
//! - [`did_resolver`] — DID resolver trait + W3C resolution result types.
//! - [`did_registrar`] — DID registrar trait + option types.
//! - [`uri`] — RFC 3986 URI normalization.
//! - [`ledger_utils`] — helpers that shape ledger reads/writes for the
//!   on-chain contract.
//!
//! The intent is byte-for-byte parity with the TypeScript implementation so
//! Rust- and TS-side resolvers agree on identifiers, state hashes, and frame
//! payloads.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms, clippy::all)]

/// Crate version reported by the build.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod crypto_codecs;
pub mod did_document;
pub mod did_registrar;
pub mod did_resolver;
pub mod ids;
pub mod ledger_utils;
pub mod uri;

// Re-exports mirroring the TS `index.ts` so downstream callers can `use
// midnight_did_domain::*` to get the most common items.
pub use crypto_codecs::{
    decode_base64url, decode_base64url_bytes, decode_base64url_bytes_32, decode_field_element, encode_base64url,
    encode_field_element,
};
pub use did_document::{
    CurveType, DidDocument, DidDocumentMetadata, DidKeyId, DidResolutionErrorCode, DidResolutionResult, DidString,
    DidUrl, KeyType, KnownDidMediaType, KnownDidResolutionErrorCode, PublicKeyJwk, RelativeUrl, Service,
    ServiceEndpoint, ValidationError, ValidationIssue, VerificationMethod, VerificationMethodRelation,
    VerificationMethodType, create_did_document, create_service, create_verification_method, parse_did,
    parse_did_document, parse_did_key_id, parse_did_url, parse_service, parse_verification_method,
};
pub use did_registrar::DidRegistrar;
pub use did_resolver::MidnightDidResolver;
pub use ledger_utils::{
    BoundIdField, assert_absolute_uri, normalize_bound_fragment_id, normalize_fragment_id, service_endpoint_to_ledger,
    service_type_to_ledger,
};
pub use uri::{normalize_service_endpoint_value, normalize_uri_string};
