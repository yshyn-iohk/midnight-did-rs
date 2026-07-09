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

//! Umbrella crate re-exporting the full Midnight DID Rust stack.
//!
//! Pull this crate in when you want every layer of the stack in one
//! dependency — typically a mobile wallet, an end-user CLI, or a Dioxus
//! frontend. Resolver-only and wasm consumers should prefer depending on
//! the smaller sibling crates directly:
//!
//! - [`domain`](mod@self::domain) ([`midnight_did_domain`]) — pure W3C DID
//!   Core types, MOD1 offchain encoder, crypto codecs. No runtime deps.
//! - [`method`](mod@self::method) ([`midnight_did_method`]) — Midnight
//!   method profile: `did:midnight:*` parsing, network mapping.
//! - [`api`](mod@self::api) ([`midnight_did_api`]) — async API:
//!   operation builders, resolution. Drives `Contract<B>` from the
//!   runtime crate.
//! - [`runtime`](mod@self::runtime) ([`midnight_did_runtime`]) — codegen
//!   target. Behind the `runtime` feature; currently blocked on upstream
//!   halo2 skew (see ADR 0003).
//!
//! ## When to depend on which crate
//!
//! - **Resolver / wasm**: `midnight-did-domain` + `midnight-did-method`.
//!   Skip the api and runtime crates entirely.
//! - **Write-side (CLI / tests / library)**: `midnight-did-api` (it
//!   transitively pulls in domain + method).
//! - **Mobile / Dioxus / monolithic app**: this umbrella crate. Opt into
//!   `features = ["runtime"]` to also pull the codegen target.
//!
//! See
//! [ADR 0003](https://github.com/yshyn-iohk/midnight-did-rs/blob/main/doc/adr/0003-crate-split-2-to-4-with-umbrella.md)
//! for the design rationale.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms, clippy::all)]

/// Async API: operation builders + resolution. Drives `Contract<B>` from
/// the runtime crate.
pub use midnight_did_api as api;
// Convenience flat re-exports of the most-used types.
pub use midnight_did_api::ApiError;
/// Pure-data DID Core types, validators, MOD1 offchain encoder.
pub use midnight_did_domain as domain;
pub use midnight_did_domain::DidDocument;
/// Midnight method profile: `did:midnight:*` parsing, ledger mappers,
/// network mapping.
pub use midnight_did_method as method;
pub use midnight_did_method::midnight_did::{MidnightDidString, MidnightNetwork, MidnightSubjectId};
/// Codegen target + concrete contract impls. Behind the `runtime` feature.
#[cfg(feature = "runtime")]
pub use midnight_did_runtime as runtime;
// `Contract<B>` is the v0.4.0 operation-driver shape. Re-export from the
// runtime crate so downstream consumers can pull it in via the umbrella
// without an explicit runtime dep. Behind the same `runtime` feature so
// resolver-only consumers keep their slim dep cone.
#[cfg(feature = "runtime")]
pub use midnight_did_runtime::{Contract, RecordingBackend};
