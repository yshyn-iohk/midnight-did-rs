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

//! Async FFI surface — 4 functions, all returning JSON strings.
//!
//! Every function takes an `Arc<DidServiceHandle>` and primitive String
//! arguments. Returning JSON keeps the FFI shape generic-free and pushes the
//! richly-typed DidDocument across the boundary as a single `String` —
//! foreign-language callers can decode with their own JSON library
//! (`JSONDecoder` on Swift, `kotlinx.serialization` on Kotlin, `json.loads`
//! on Python) and we don't pay the cost of mirroring every domain struct in
//! the uniffi schema.
//!
//! Each function is `async` and tagged with
//! `#[uniffi::export(async_runtime = "tokio")]` — uniffi 0.29 maps that
//! onto Swift `async/await`, Kotlin `suspendFun`, and Python `asyncio`.

use std::sync::Arc;

use midnight_did_api::{
    did_operations::{deactivate_did as api_deactivate, resolve_did as api_resolve},
    private_state::InMemoryPrivateStateStore,
};
use serde::Serialize;

use crate::{
    error::{FlatError, decode_hex_32},
    handle::DidServiceHandle,
};

/// `create_did(handle, seed_hex, controller_public_key_hex) -> JSON`.
///
/// Records the controller key into the mock contract's private-state store
/// and returns a JSON envelope describing the seeded DID. In production this
/// would deploy the contract via a `compact-runtime` provider stack — for
/// the skeleton it simply registers the controller key and returns the DID
/// string + seed echo.
#[uniffi::export(async_runtime = "tokio")]
pub async fn create_did(
    handle: Arc<DidServiceHandle>,
    seed_hex: String,
    controller_public_key_hex: String,
) -> Result<String, FlatError> {
    let _seed = decode_hex_32(&seed_hex, "seed_hex")?;
    let _pk = decode_hex_32(&controller_public_key_hex, "controller_public_key_hex")?;

    let contract = handle.contract.lock().await;
    let store = InMemoryPrivateStateStore::new();
    let _state = midnight_did_api::did_operations::create_did(&*contract, &store, _seed)
        .await
        .map_err(FlatError::from)?;

    let did_subject = midnight_did_api::subject::get_did_subject(&*contract).map_err(FlatError::from)?;
    Ok(serde_json::to_string(&CreateDidResponse {
        did: did_subject,
        controller_public_key_hex,
    })?)
}

/// `rotate_controller_key(handle, did, new_pk_hex) -> JSON`.
///
/// Drives the `rotateControllerKey` circuit on the mock contract. The
/// `did_subject` argument is currently informational (the handle knows which
/// contract to talk to), but it pins the FFI shape so a future real impl can
/// resolve to the right contract instance.
#[uniffi::export(async_runtime = "tokio")]
pub async fn rotate_controller_key(
    handle: Arc<DidServiceHandle>,
    did_subject: String,
    new_controller_public_key_hex: String,
) -> Result<String, FlatError> {
    let new_pk = decode_hex_32(&new_controller_public_key_hex, "new_controller_public_key_hex")?;

    let contract = handle.contract.lock().await;
    let store = InMemoryPrivateStateStore::new();
    let result = midnight_did_api::controller_operations::rotate_controller_key(
        &*contract, &store, [0u8; 32], // skeleton: real impl derives this from the new pk
        new_pk,
    )
    .await
    .map_err(FlatError::from)?;

    Ok(serde_json::to_string(&RotateResponse {
        did: did_subject,
        tx_hash: result.tx_hash,
        block_height: result.block_height,
    })?)
}

/// `resolve_did(handle, did) -> JSON` returns the DID Document JSON or a
/// [`FlatError::NotFound`] when the contract has no live state.
#[uniffi::export(async_runtime = "tokio")]
pub async fn resolve_did(handle: Arc<DidServiceHandle>, did_subject: String) -> Result<String, FlatError> {
    let contract = handle.contract.lock().await;
    let resolved = api_resolve(&*contract).await.map_err(FlatError::from)?;
    match resolved {
        Some(r) => Ok(serde_json::to_string(&ResolveResponse {
            did_document: r.did_document,
            did_document_metadata: r.did_document_metadata,
        })?),
        None => Err(FlatError::not_found(format!(
            "no live DID state for subject {did_subject}"
        ))),
    }
}

/// `deactivate(handle, did) -> JSON` marks the DID as deactivated and
/// returns the finalised tx data.
#[uniffi::export(async_runtime = "tokio")]
pub async fn deactivate(handle: Arc<DidServiceHandle>, did_subject: String) -> Result<String, FlatError> {
    let contract = handle.contract.lock().await;
    let result = api_deactivate(&*contract).await.map_err(FlatError::from)?;
    Ok(serde_json::to_string(&RotateResponse {
        did: did_subject,
        tx_hash: result.tx_hash,
        block_height: result.block_height,
    })?)
}

// --------------------------------------------------------------------------
// JSON envelope shapes — owned in this crate so the wire format is stable
// without leaking domain-crate refactors across the FFI boundary.
// --------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct CreateDidResponse {
    did: String,
    controller_public_key_hex: String,
}

#[derive(Debug, Serialize)]
struct RotateResponse {
    did: String,
    tx_hash: String,
    block_height: u64,
}

#[derive(Debug, Serialize)]
struct ResolveResponse {
    did_document: midnight_did_domain::did_document::DidDocument,
    did_document_metadata: midnight_did_domain::did_document::DidDocumentMetadata,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: &str = "0101010101010101010101010101010101010101010101010101010101010101";
    const PK: &str = "0202020202020202020202020202020202020202020202020202020202020202";

    #[tokio::test]
    async fn create_did_returns_json_with_did_subject() {
        let handle = DidServiceHandle::new();
        let json = create_did(handle, SEED.into(), PK.into()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v["did"].as_str().unwrap().starts_with("did:midnight:testnet:"));
        assert_eq!(v["controller_public_key_hex"], PK);
    }

    #[tokio::test]
    async fn create_did_rejects_bad_hex() {
        let handle = DidServiceHandle::new();
        let err = create_did(handle, "not-hex".into(), PK.into()).await.unwrap_err();
        assert!(matches!(err, FlatError::InvalidInput { .. }));
    }

    #[tokio::test]
    async fn create_did_rejects_short_hex() {
        let handle = DidServiceHandle::new();
        let err = create_did(handle, "ab".into(), PK.into()).await.unwrap_err();
        assert!(matches!(err, FlatError::InvalidInput { .. }));
    }

    #[tokio::test]
    async fn rotate_controller_key_round_trip() {
        let handle = DidServiceHandle::new();
        let json = rotate_controller_key(handle, "did:midnight:testnet:x".into(), PK.into())
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["did"], "did:midnight:testnet:x");
        assert!(v.get("tx_hash").is_some());
    }

    #[tokio::test]
    async fn resolve_did_returns_document_json() {
        let handle = DidServiceHandle::new();
        let json = resolve_did(handle, "did:midnight:testnet:x".into()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(
            v["did_document"]["id"]
                .as_str()
                .unwrap()
                .starts_with("did:midnight:testnet:")
        );
    }

    #[tokio::test]
    async fn deactivate_round_trip() {
        let handle = DidServiceHandle::new();
        let json = deactivate(handle, "did:midnight:testnet:x".into()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["did"], "did:midnight:testnet:x");
    }
}
