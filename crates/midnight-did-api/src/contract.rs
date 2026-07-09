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

//! Contract-surface re-exports.
//!
//! ## v0.4.0 — Path 2 contract abstraction reform
//!
//! See `doc/adr/0008-contract-abstraction-reform.md`.
//!
//! The pre-v0.4.0 `DidContract` trait + `mock::RecordingContract` were
//! deleted in R2-2.4. Downstream code now uses
//! [`midnight_did_runtime::Contract<B>`] directly, with
//! [`midnight_did_runtime::RecordingBackend`] for tests and
//! [`midnight_did_runtime::ResolverBackend`] /
//! [`midnight_did_runtime::LiveBackend`] for the resolve / production
//! consumers.
//!
//! This module exists purely to re-export the ledger-shape value types
//! (`MapMutation`, `SetMutation`, [`LedgerVerificationMethod`],
//! [`LedgerSchnorrJubjubVerificationMethod`], [`LedgerService`],
//! [`LedgerPublicKeyJwk`], [`JubjubPointHex`],
//! [`SchnorrJubjubSignature`], [`SchnorrJubjubDigest`],
//! [`LedgerVerificationMethodRelation`], [`DidLedgerSnapshot`]) and
//! [`FinalizedTxData`] from `midnight-did-runtime` so api-layer consumers
//! do not need an explicit `midnight-did-runtime` dep to consume them.

pub use midnight_did_runtime::{
    DidLedgerSnapshot, FinalizedTxData, JubjubPointHex, LedgerPublicKeyJwk, LedgerSchnorrJubjubVerificationMethod,
    LedgerService, LedgerVerificationMethod, LedgerVerificationMethodRelation, MapMutation, NewJubjubPointHex,
    SchnorrJubjubDigest, SchnorrJubjubSignature, SetMutation, ValidationError,
};
