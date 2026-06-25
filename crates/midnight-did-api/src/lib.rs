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

//! Operation, resolution, and ledger-mapping layer for the Midnight DID
//! Method.
//!
//! This crate is the Rust port of the operation half of the TypeScript
//! `@midnight-ntwrk/midnight-did-api` package. It sits on top of
//! [`midnight_did_domain`] and drives the on-chain contract via the
//! [`midnight_did_runtime::Contract<B>`] wrapper.
//!
//! ## Module layout
//!
//! - [`contract`] — re-exports of the ledger-shape value types
//!   ([`DidLedgerSnapshot`](contract::DidLedgerSnapshot), mutation tags,
//!   [`LedgerVerificationMethod`](contract::LedgerVerificationMethod), …)
//!   from `midnight-did-runtime`.
//! - [`error`] — top-level [`error::ApiError`].
//! - [`subject`] — DID subject + bound-fragment-id helpers.
//! - [`ledger_mappers`] — domain → ledger conversion helpers.
//! - [`private_state`] — controller private-state lifecycle + storage trait.
//! - [`controller_operations`] — controller-key rotation.
//! - [`verification_method_operations`] — VM CRUD + relation purge logic.
//! - [`service_operations`] — service endpoint CRUD.
//! - [`document_operations`] — `alsoKnownAs` + deactivation.
//! - [`resolution`] — ledger snapshot → DID Document.
//! - [`did_operations`] — high-level CRUD aggregations.
//!
//! ## v0.4.0 contract abstraction
//!
//! The pre-v0.4.0 `DidContract` trait + `RecordingContract` mock were
//! replaced by [`midnight_did_runtime::Contract<B: Backend>`] (see
//! ADR 0008 — Path 2). The typed
//! [`midnight_did_runtime::DidContractCall`] envelope serialises into
//! [`midnight_did_runtime::BuiltTx::bytes`] and is decoded by the
//! recording backend for tests. The api-layer operation builders now
//! take `&Contract<B: Backend>` directly.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms, clippy::all)]

/// Crate version reported by the build.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod contract;
pub mod controller_operations;
pub mod did_operations;
pub mod document_operations;
pub mod error;
pub mod ledger_mappers;
pub mod private_state;
pub mod resolution;
pub mod service_operations;
pub mod subject;
pub mod verification_method_operations;

// Transitional re-exports — items that moved to `midnight-did-method` per
// ADR 0003 stay reachable via the api crate for downstream consumers that
// still depend on `midnight_did_api::network_mapping`, etc. Migrate to
// `midnight_did_method::*` when convenient.
pub use midnight_did_method::network_mapping;

// Re-exports — common API surface.
pub use contract::{
    DidLedgerSnapshot, FinalizedTxData, JubjubPointHex, LedgerPublicKeyJwk, LedgerSchnorrJubjubVerificationMethod,
    LedgerService, LedgerVerificationMethod, LedgerVerificationMethodRelation, MapMutation, SchnorrJubjubDigest,
    SchnorrJubjubSignature, SetMutation,
};
pub use error::{ApiError, ContractError};
pub use ledger_mappers::{
    SchnorrJubjubVerificationMethod, ledger_verification_method_relation_for, public_key_jwk_to_ledger,
    relation_set_from_state, schnorr_jubjub_verification_method_to_ledger, service_to_ledger,
    verification_method_to_ledger,
};
pub use midnight_did_method::network_mapping::{
    DomainToRuntime, RuntimeNetworkId, RuntimeToDomain, domain_to_runtime, runtime_to_domain,
};
pub use private_state::{
    DidPrivateState, InMemoryPrivateStateStore, PrivateStateError, PrivateStateSlot, PrivateStateStore,
    RecoverPendingControllerPrivateStateOptions, bind_private_state_provider, clear_pending_controller_private_state,
    init_private_state, is_restorable_did_private_state, recover_pending_controller_private_state,
    require_private_state, restore_private_state, save_pending_controller_private_state, save_private_state,
};
pub use resolution::{ResolvedMidnightDid, ledger_state_to_did_document, ledger_state_to_metadata, resolve};
pub use subject::{get_did_subject, get_did_subject_for, normalize_bound_fragment_id_for};
pub use verification_method_operations::{
    VERIFICATION_METHOD_RELATIONS, VerificationMethodRelationMembership, assert_verification_method_relation_absent,
    assert_verification_method_relation_present, purge_verification_method_from_all_relations,
    remove_present_verification_method_relations, verification_method_relation_memberships,
};
