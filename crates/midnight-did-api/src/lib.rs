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
//! [`midnight_did_domain`] and abstracts the on-chain contract behind the
//! [`contract::DidContract`] trait so the API surface can be built and
//! unit-tested independently of `midnight-did` (the runtime crate).
//!
//! ## Module layout
//!
//! - [`contract`] — [`DidContract`](contract::DidContract) trait, ledger
//!   snapshot view ([`DidLedgerSnapshot`](contract::DidLedgerSnapshot)),
//!   mutation tags, and a recording mock implementation
//!   ([`contract::mock::RecordingContract`]).
//! - [`error`] — top-level [`error::ApiError`].
//! - [`network_mapping`] — runtime ↔ domain network-id mapping.
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
//! ## Why a trait, not the runtime crate?
//!
//! The codegen-rust toolchain currently has gaps that block the
//! `midnight-did` runtime crate from building end-to-end (halo2 ParamsKZG
//! API skew in the Nix-pinned `midnight-transient-crypto` snapshot). To keep
//! the API layer landing as testable Rust code, this crate depends on
//! [`midnight_did_domain`] only. The [`contract::DidContract`] trait
//! supplies a thin abstraction over the impure circuit surface; a real
//! implementation wraps the runtime when it builds. Until then the
//! [`contract::mock::RecordingContract`] is enough to drive every
//! operation-level test.

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
pub mod network_mapping;
pub mod private_state;
pub mod resolution;
pub mod service_operations;
pub mod subject;
pub mod verification_method_operations;

// Re-exports — common API surface.
pub use contract::{
    DidContract, DidLedgerSnapshot, FinalizedTxData, JubjubPointHex, LedgerPublicKeyJwk, LedgerSchnorrJubjubVerificationMethod,
    LedgerService, LedgerVerificationMethod, LedgerVerificationMethodRelation, MapMutation, SchnorrJubjubDigest,
    SchnorrJubjubSignature, SetMutation,
};
pub use error::{ApiError, ContractError};
pub use ledger_mappers::{
    SchnorrJubjubVerificationMethod, ledger_verification_method_relation_for, public_key_jwk_to_ledger,
    relation_set_from_state, schnorr_jubjub_verification_method_to_ledger, service_to_ledger,
    verification_method_to_ledger,
};
pub use network_mapping::{
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
    VERIFICATION_METHOD_RELATIONS, VerificationMethodRelationMembership,
    assert_verification_method_relation_absent, assert_verification_method_relation_present,
    purge_verification_method_from_all_relations, remove_present_verification_method_relations,
    verification_method_relation_memberships,
};
