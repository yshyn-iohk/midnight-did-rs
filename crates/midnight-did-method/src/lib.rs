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

//! Midnight method profile for the Rust DID port.
//!
//! This crate sits between [`midnight_did_domain`] (pure W3C DID Core types)
//! and [`midnight_did_api`](https://docs.rs/midnight-did-api) (the async
//! operation layer). It hosts the pieces that are **specific to the Midnight
//! method** but do not need the on-chain runtime or the operation-layer
//! abstractions:
//!
//! - [`midnight_did`] — `did:midnight:<network>:<id>` string types,
//!   parsing, and subject-id helpers (moved from
//!   `midnight_did_domain::midnight`).
//! - [`network_mapping`] — runtime ↔ domain network identifier mapping
//!   (moved from `midnight_did_api::network_mapping`).
//! - [`offchain`] — MOD1-tagged binary frame encoder/decoder for
//!   `did:midnight:offchain:*` DIDs (moved from
//!   `midnight_did_domain::offchain` — it is method-specific because
//!   it embeds `did:midnight:` strings).
//!
//! ## Why a separate crate?
//!
//! Resolver-only and wasm consumers want the method profile without the
//! wallet path that lives in the api crate. See
//! [ADR 0003](https://github.com/yshyn-iohk/midnight-did-rs/blob/main/doc/adr/0003-crate-split-2-to-4-with-umbrella.md)
//! for the rationale.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms, clippy::all)]

/// Crate version reported by the build.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod midnight_did;
pub mod network_mapping;
pub mod offchain;

// Re-exports — common method-profile surface.
pub use midnight_did::{
    ContractAddress, MidnightDidError, MidnightDidString, MidnightNetwork, MidnightSubjectId, OffchainStateHashHex,
    create_midnight_did_string, parse_contract_address, parse_midnight_did, parse_midnight_did_string,
    parse_offchain_state_hash,
};
pub use network_mapping::{DomainToRuntime, RuntimeNetworkId, RuntimeToDomain, domain_to_runtime, runtime_to_domain};
pub use offchain::{
    EncodedOffchainMidnightDidState, OFFCHAIN_STATE_ENCODING, OffchainMidnightDidState, OffchainService,
    OffchainStateHash, OffchainVerificationMethod, OffchainVerificationRelationships,
    ParsedLongFormOffchainMidnightDid, create_long_form_offchain_midnight_did_string,
    create_offchain_midnight_did_document_metadata, create_offchain_midnight_did_string,
    create_offchain_midnight_did_string_from_state, decode_offchain_midnight_did_state,
    encode_offchain_midnight_did_state, offchain_service_to_did_document_service, offchain_state_to_did_document,
    offchain_verification_method_to_did_document_method, parse_long_form_offchain_midnight_did_string,
};
