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

//! UniFFI proof-of-concept exposing the Midnight DID async API to Swift,
//! Kotlin, and Python via the [uniffi-rs] toolchain.
//!
//! This crate is a *skeleton* — its sole purpose is to demonstrate that the
//! `midnight-did-api` async surface can be flattened cleanly across the FFI
//! boundary without an explosion of intermediate types. The FFI shape is:
//!
//! - 4 async functions ([`api::create_did`], [`api::rotate_controller_key`],
//!   [`api::resolve_did`], [`api::deactivate`]).
//! - One opaque handle object ([`handle::DidServiceHandle`]) wrapping the
//!   in-memory mock contract.
//! - One flat error enum ([`error::FlatError`]).
//! - All inputs and outputs are uniffi-compatible primitives: `String`
//!   (JSON or hex), `Arc<T>` for the handle, and `Result<T, FlatError>` for
//!   fallible calls.
//!
//! See `README.md` for instructions on running `uniffi-bindgen` and
//! generating Swift / Kotlin source.
//!
//! [uniffi-rs]: https://github.com/mozilla/uniffi-rs

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms, clippy::all)]

pub mod api;
pub mod error;
pub mod handle;

pub use api::{create_did, deactivate, resolve_did, rotate_controller_key};
pub use error::FlatError;
pub use handle::DidServiceHandle;

// Generate the uniffi scaffolding (foreign-language type tables, async
// runtime registration, FFI entry-points). Must be invoked exactly once
// at the crate root.
uniffi::setup_scaffolding!();
