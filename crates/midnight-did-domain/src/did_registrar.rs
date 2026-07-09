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

//! DID registrar trait + option types.
//!
//! Port of `did-registrar.ts`. The TS interface is generic over both the DID
//! type `D` and the patch-operation type `Op`. The Rust port carries the
//! same two generics; we also expose convenience structs (`CreateOptions`,
//! `UpdateOptions`, `DeactivateOptions`) that mirror the W3C DID Registration
//! draft so registrar implementations have a stable extension point.

use std::error::Error as StdError;

use serde::{Deserialize, Serialize};

use crate::did_document::DidDocument;
use crate::did_resolver::BoxFuture;

/// Result of a successful [`DidRegistrar::create`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateResult<D> {
    /// Newly-minted DID identifier.
    pub did: D,
    /// Initial DID Document representation.
    pub document: DidDocument,
}

/// Options accepted by [`DidRegistrar::create`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CreateOptions<Op> {
    /// Optional list of patch operations applied to the seed document.
    pub patches: Vec<Op>,
}

/// Options accepted by [`DidRegistrar::update`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UpdateOptions<Op> {
    /// Patch operations to apply.
    pub patches: Vec<Op>,
}

/// Options accepted by [`DidRegistrar::deactivate`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeactivateOptions {
    /// Optional human-readable reason recorded by the registrar.
    pub reason: Option<String>,
}

/// DID registrar trait. Implementations create, update, and deactivate DIDs.
pub trait DidRegistrar: Send + Sync {
    /// Concrete DID identifier type (e.g. `MidnightDidString` from the
    /// `midnight-did-method` crate).
    type Did: Send + Sync + 'static;
    /// Concrete patch-operation type.
    type Op: Send + Sync + 'static;
    /// Concrete error returned by registrar operations.
    type Error: StdError + Send + Sync + 'static;

    /// Create a new DID. Returns the freshly-minted identifier and its
    /// initial DID Document.
    fn create<'a>(&'a self, patches: Vec<Self::Op>) -> BoxFuture<'a, Result<CreateResult<Self::Did>, Self::Error>>;

    /// Apply `patches` to the DID document for `did` and return the updated
    /// representation.
    fn update<'a>(&'a self, did: Self::Did, patches: Vec<Self::Op>) -> BoxFuture<'a, Result<DidDocument, Self::Error>>;

    /// Deactivate `did` and return the final DID document representation.
    fn deactivate<'a>(&'a self, did: Self::Did) -> BoxFuture<'a, Result<DidDocument, Self::Error>>;
}
