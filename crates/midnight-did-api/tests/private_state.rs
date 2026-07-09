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

//! Integration tests for the DID controller private-state lifecycle.
//!
//! Rust port of `packages/api/src/test/private-state.test.ts`. The Rust port
//! exposes a `PrivateStateStore` trait rather than a `LevelPrivateStateProvider`
//! instance, so the TS "providers" shape is replaced by stores. The behavioural
//! coverage is preserved:
//!
//! - 32-byte secret-key restriction (compile-time via `[u8; 32]` here, plus a
//!   runtime check on the slice constructor).
//! - `init_private_state` returns the existing state without rewriting it.
//! - Round-trip restore / save / require.
//! - Contract-address-not-set is swallowed by `restore_private_state` and by
//!   the `set` half of `init_private_state`.
//! - Backend failures propagate through `restore` / `save`.
//! - `recover_pending_controller_private_state` promotes pending -> active
//!   only with the explicit `rotation_finalized: true` confirmation.

use std::sync::Mutex;

use async_trait::async_trait;
use midnight_did_api::error::ApiError;
use midnight_did_api::private_state::{
    DidPrivateState, InMemoryPrivateStateStore, PrivateStateError, PrivateStateSlot, PrivateStateStore,
    RecoverPendingControllerPrivateStateOptions, bind_private_state_provider, init_private_state,
    is_restorable_did_private_state, recover_pending_controller_private_state, require_private_state,
    restore_private_state, save_pending_controller_private_state, save_private_state,
};

// ---------------------------------------------------------------------------
// TS: "accepts only 32-byte Uint8Array secret keys as restorable state"
// ---------------------------------------------------------------------------
#[test]
fn from_bytes_requires_exactly_32_bytes() {
    assert!(DidPrivateState::from_bytes(&[0u8; 32]).is_ok());
    assert!(matches!(
        DidPrivateState::from_bytes(&[0u8; 31]).unwrap_err(),
        ApiError::Controller(midnight_did_api::error::ControllerError::InvalidSecretKey)
    ));
    assert!(matches!(
        DidPrivateState::from_bytes(&[0u8; 33]).unwrap_err(),
        ApiError::Controller(midnight_did_api::error::ControllerError::InvalidSecretKey)
    ));
    assert!(matches!(
        DidPrivateState::from_bytes(&[]).unwrap_err(),
        ApiError::Controller(midnight_did_api::error::ControllerError::InvalidSecretKey)
    ));
}

#[test]
fn is_restorable_did_private_state_handles_present_and_absent() {
    let state = DidPrivateState { secret_key: [0u8; 32] };
    assert!(is_restorable_did_private_state(Some(&state)));
    assert!(!is_restorable_did_private_state(None));
}

// ---------------------------------------------------------------------------
// TS: "returns provider state without deriving or saving a replacement"
// ---------------------------------------------------------------------------
#[tokio::test]
async fn init_private_state_returns_existing_without_overwriting() {
    let store = InMemoryPrivateStateStore::new();
    let stored = DidPrivateState { secret_key: [7u8; 32] };
    save_private_state(&store, stored.clone(), PrivateStateSlot::Active)
        .await
        .unwrap();

    // Call init with a *different* candidate; the existing state should win.
    let result = init_private_state(&store, [99u8; 32]).await.unwrap();
    assert_eq!(result, stored);

    // The active slot is unchanged.
    let restored = restore_private_state(&store, PrivateStateSlot::Active).await.unwrap();
    assert_eq!(restored, Some(stored));
}

// ---------------------------------------------------------------------------
// TS: "restores null for malformed state but requirePrivateState rejects it"
// In Rust, malformed inputs cannot reach the store (typed slot value); the
// equivalent assertion is: empty active slot -> restore returns None, require
// errors.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn require_errors_when_empty_restore_returns_none() {
    let store = InMemoryPrivateStateStore::new();
    assert_eq!(
        restore_private_state(&store, PrivateStateSlot::Active).await.unwrap(),
        None
    );
    let err = require_private_state(&store, PrivateStateSlot::Active)
        .await
        .unwrap_err();
    assert!(matches!(err, ApiError::MissingPrivateState), "{err}");
}

// ---------------------------------------------------------------------------
// TS: "generates and saves a replacement when stored state is missing or
// malformed"
// ---------------------------------------------------------------------------
#[tokio::test]
async fn init_private_state_seeds_a_new_key_when_empty() {
    let store = InMemoryPrivateStateStore::new();
    let candidate = [42u8; 32];
    let result = init_private_state(&store, candidate).await.unwrap();
    assert_eq!(result.secret_key, candidate);
    let saved = restore_private_state(&store, PrivateStateSlot::Active).await.unwrap();
    assert_eq!(saved.unwrap().secret_key, candidate);
}

// ---------------------------------------------------------------------------
// TS: "allows restore and save before a contract address is bound"
// + "restorePrivateState returns null when the provider has no contract
// address"
// ---------------------------------------------------------------------------
#[tokio::test]
async fn restore_swallows_contract_address_not_set() {
    let store = InMemoryPrivateStateStore::strict();
    // Reads succeed (returning None) even though no contract address is bound.
    let restored = restore_private_state(&store, PrivateStateSlot::Active).await.unwrap();
    assert_eq!(restored, None);
}

#[tokio::test]
async fn init_returns_state_even_if_address_unset_during_set() {
    let store = InMemoryPrivateStateStore::strict();
    let candidate = [11u8; 32];
    let result = init_private_state(&store, candidate).await.unwrap();
    assert_eq!(result.secret_key, candidate);
    // Once the address is set, the store should report the slot as empty
    // (the strict store rejected the write earlier).
    store.set_contract_address("contract-1");
    let saved = restore_private_state(&store, PrivateStateSlot::Active).await.unwrap();
    assert_eq!(saved, None);
}

// ---------------------------------------------------------------------------
// TS: "propagates private-state provider failures unrelated to contract
// binding"
// ---------------------------------------------------------------------------
#[tokio::test]
async fn restore_propagates_backend_failure() {
    let store = AlwaysFailingStore::new(FailKind::Get);
    let err = restore_private_state(&store, PrivateStateSlot::Active)
        .await
        .unwrap_err();
    assert!(matches!(err, ApiError::InvalidArgument(_)), "{err}");
}

#[tokio::test]
async fn init_propagates_set_backend_failure() {
    let store = AlwaysFailingStore::new(FailKind::Set);
    let err = init_private_state(&store, [3u8; 32]).await.unwrap_err();
    assert!(matches!(err, ApiError::InvalidArgument(_)), "{err}");
}

// ---------------------------------------------------------------------------
// TS: "binds the private-state provider to a contract address"
// ---------------------------------------------------------------------------
#[tokio::test]
async fn bind_private_state_provider_sets_address() {
    let store = InMemoryPrivateStateStore::strict();
    bind_private_state_provider(&store, "0xabc");
    // Now save/get should succeed (no address check).
    save_private_state(
        &store,
        DidPrivateState { secret_key: [1u8; 32] },
        PrivateStateSlot::Active,
    )
    .await
    .unwrap();
    let restored = restore_private_state(&store, PrivateStateSlot::Active).await.unwrap();
    assert_eq!(restored.unwrap().secret_key, [1u8; 32]);
}

// ---------------------------------------------------------------------------
// TS: "promotes pending controller private state for recovery"
// ---------------------------------------------------------------------------
#[tokio::test]
async fn recover_pending_promotes_to_active_then_clears_pending() {
    let store = InMemoryPrivateStateStore::new();
    let pending = DidPrivateState { secret_key: [9u8; 32] };
    save_pending_controller_private_state(&store, pending.clone())
        .await
        .unwrap();

    let recovered = recover_pending_controller_private_state(
        &store,
        RecoverPendingControllerPrivateStateOptions {
            rotation_finalized: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(recovered, pending);

    assert_eq!(
        restore_private_state(&store, PrivateStateSlot::Active).await.unwrap(),
        Some(pending)
    );
    assert_eq!(
        restore_private_state(&store, PrivateStateSlot::Pending).await.unwrap(),
        None
    );
}

// ---------------------------------------------------------------------------
// TS: "refuses to recover pending controller private state without
// finalization confirmation"
// ---------------------------------------------------------------------------
#[tokio::test]
async fn recover_pending_requires_finalization_marker() {
    let store = InMemoryPrivateStateStore::new();
    save_pending_controller_private_state(&store, DidPrivateState { secret_key: [9u8; 32] })
        .await
        .unwrap();
    let err = recover_pending_controller_private_state(
        &store,
        RecoverPendingControllerPrivateStateOptions {
            rotation_finalized: false,
        },
    )
    .await
    .unwrap_err();
    let msg = err.to_string();
    assert!(matches!(err, ApiError::InvalidArgument(_)), "{msg}");
    assert!(msg.contains("only be recovered after confirming"), "{msg}");

    // Pending must not have been cleared, active must still be empty.
    let pending = restore_private_state(&store, PrivateStateSlot::Pending).await.unwrap();
    assert!(pending.is_some());
    assert_eq!(
        restore_private_state(&store, PrivateStateSlot::Active).await.unwrap(),
        None
    );
}

// ---------------------------------------------------------------------------
// TS: "rejects pending controller recovery when no pending state exists"
// ---------------------------------------------------------------------------
#[tokio::test]
async fn recover_pending_errors_when_no_pending_state() {
    let store = InMemoryPrivateStateStore::new();
    let err = recover_pending_controller_private_state(
        &store,
        RecoverPendingControllerPrivateStateOptions {
            rotation_finalized: true,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, ApiError::MissingPrivateState), "{err}");
}

// ===========================================================================
// Local test doubles for failure injection.
// ===========================================================================

#[derive(Debug, Clone, Copy)]
enum FailKind {
    Get,
    Set,
}

struct AlwaysFailingStore {
    fail: FailKind,
    slots: Mutex<std::collections::HashMap<PrivateStateSlot, DidPrivateState>>,
}

impl AlwaysFailingStore {
    fn new(fail: FailKind) -> Self {
        Self {
            fail,
            slots: Default::default(),
        }
    }
}

#[async_trait]
impl PrivateStateStore for AlwaysFailingStore {
    fn set_contract_address(&self, _address: &str) {}

    async fn get(&self, slot: PrivateStateSlot) -> Result<Option<DidPrivateState>, PrivateStateError> {
        if matches!(self.fail, FailKind::Get) {
            return Err(PrivateStateError::Backend("storage offline".into()));
        }
        Ok(self.slots.lock().unwrap().get(&slot).cloned())
    }

    async fn set(&self, slot: PrivateStateSlot, state: DidPrivateState) -> Result<(), PrivateStateError> {
        if matches!(self.fail, FailKind::Set) {
            return Err(PrivateStateError::Backend("write failed".into()));
        }
        self.slots.lock().unwrap().insert(slot, state);
        Ok(())
    }

    async fn remove(&self, slot: PrivateStateSlot) -> Result<(), PrivateStateError> {
        self.slots.lock().unwrap().remove(&slot);
        Ok(())
    }
}
