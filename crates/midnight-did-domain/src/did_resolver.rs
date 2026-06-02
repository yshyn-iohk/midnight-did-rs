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

//! DID resolver trait + supporting types.
//!
//! Port of `did-resolver.ts`. The TS module is intentionally minimal — a
//! single `MidnightDIDResolver` interface with one `resolve` method. The
//! Rust port keeps the same shape and additionally re-exports the W3C
//! resolution-result types (which live in [`crate::did_document`]) for
//! callers that need them.

use std::error::Error as StdError;
use std::future::Future;
use std::pin::Pin;

use crate::did_document::{DidDocument, DidString};

pub use crate::did_document::{DidDocumentMetadata, DidResolutionResult};

/// Boxed, type-erased dynamic future used by `MidnightDidResolver` so it can
/// be implemented for executors of any flavour. Mirrors the TS `Promise<T>`
/// return type semantically.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// DID resolver trait. Implementations resolve a DID (validated bare
/// [`DidString`]) into a fully-built [`DidDocument`].
pub trait MidnightDidResolver: Send + Sync {
    /// Concrete error returned by [`Self::resolve`].
    type Error: StdError + Send + Sync + 'static;

    /// Resolve `did` and return the DID Document representation.
    fn resolve<'a>(&'a self, did: DidString) -> BoxFuture<'a, Result<DidDocument, Self::Error>>;
}
