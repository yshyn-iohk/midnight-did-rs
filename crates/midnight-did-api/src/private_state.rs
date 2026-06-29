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

//! DID controller private-state lifecycle.
//!
//! Rust port of `packages/api/src/private-state.ts` and the storage trait
//! that backs it (`private-state-storage.ts` is reduced here to a generic
//! [`PrivateStateStore`] trait + an in-memory impl — the level-db wiring is
//! out of scope for now).
//!
//! Mirrors the public surface of the TS file:
//!
//! - `bindPrivateStateProvider` — [`bind_private_state_provider`].
//! - `restorePrivateState` — [`restore_private_state`].
//! - `requirePrivateState` — [`require_private_state`].
//! - `savePrivateState` — [`save_private_state`].
//! - `initPrivateState` — [`init_private_state`].
//! - `savePendingControllerPrivateState` — [`save_pending_controller_private_state`].
//! - `clearPendingControllerPrivateState` — [`clear_pending_controller_private_state`].
//! - `recoverPendingControllerPrivateState` — [`recover_pending_controller_private_state`].
//! - `isRestorableDIDPrivateState` — [`is_restorable_did_private_state`].

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::error::ApiError;

/// The two well-known private-state slot ids the contract uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrivateStateSlot {
    /// Active DID controller private state. Read by every controller-bound
    /// operation.
    Active,
    /// Pending controller private state — written by
    /// [`crate::controller_operations::rotate_controller_key`] before the
    /// rotation circuit is invoked and promoted to [`Self::Active`] after
    /// the transaction finalises.
    Pending,
}

impl PrivateStateSlot {
    /// Stable string id mirroring the TS constants
    /// `MidnightDIDPrivateStateId` and
    /// `MidnightDIDPendingControllerPrivateStateId`.
    pub fn as_str(self) -> &'static str {
        match self {
            PrivateStateSlot::Active => "midnightDIDPrivateState",
            PrivateStateSlot::Pending => "midnightDIDPendingControllerPrivateState",
        }
    }
}

/// DID controller private state. Mirrors `DIDPrivateState` from the TS
/// contract package: a 32-byte secret key from which the controller public
/// key is derived in-circuit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DidPrivateState {
    /// 32-byte controller secret key.
    pub secret_key: [u8; 32],
}

impl DidPrivateState {
    /// Build a private state from a 32-byte slice. Returns
    /// [`ApiError::Controller(crate::error::ControllerError::InvalidSecretKey)`] for any other length.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ApiError> {
        if bytes.len() != 32 {
            return Err(ApiError::Controller(crate::error::ControllerError::InvalidSecretKey));
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(bytes);
        Ok(Self { secret_key: out })
    }
}

/// Return `true` iff `state` is a well-formed restorable DID private state.
/// Mirrors `isRestorableDIDPrivateState`.
pub fn is_restorable_did_private_state(state: Option<&DidPrivateState>) -> bool {
    state.is_some()
}

/// Storage trait abstracting the private-state provider.
///
/// The TS code couples directly to a `LevelPrivateStateProvider`. Here we
/// keep the trait small and let downstream code wire a real store. An
/// in-memory implementation [`InMemoryPrivateStateStore`] is included for
/// tests.
#[async_trait]
pub trait PrivateStateStore: Send + Sync {
    /// Bind the store to a contract address. Calling [`Self::get`] /
    /// [`Self::set`] / [`Self::remove`] before binding may return
    /// [`PrivateStateError::ContractAddressNotSet`].
    fn set_contract_address(&self, address: &str);

    /// Fetch a private state from `slot`. Returns `Ok(None)` if no value is
    /// stored.
    async fn get(&self, slot: PrivateStateSlot) -> Result<Option<DidPrivateState>, PrivateStateError>;

    /// Write `state` into `slot`, overwriting any existing value.
    async fn set(&self, slot: PrivateStateSlot, state: DidPrivateState) -> Result<(), PrivateStateError>;

    /// Remove the value stored at `slot`. Removing an empty slot is not an
    /// error.
    async fn remove(&self, slot: PrivateStateSlot) -> Result<(), PrivateStateError>;
}

/// Errors surfaced by [`PrivateStateStore`] implementations.
#[derive(Debug, thiserror::Error)]
pub enum PrivateStateError {
    /// The store was not yet bound to a contract address. Mirrors the TS
    /// "Contract address not set" check.
    #[error("Contract address not set")]
    ContractAddressNotSet,
    /// Catch-all for storage backend errors.
    #[error("private state store error: {0}")]
    Backend(String),
}

/// In-memory [`PrivateStateStore`] used by unit tests.
///
/// Note: tracking `contract_address` mimics the TS provider semantics so we
/// can faithfully exercise the "address-not-set" code path; the in-memory
/// store still serves reads/writes if the address is unset (TS providers
/// surface that as a runtime error — replicated below).
///
/// # Panics
///
/// Every method takes `self.inner.lock().unwrap()`. `Mutex::lock` only
/// fails when the mutex is poisoned (a previous holder panicked while
/// holding it); that is a programming error, not a fallible runtime
/// condition, so the panic is intentional.
#[derive(Debug, Default)]
pub struct InMemoryPrivateStateStore {
    inner: Mutex<InMemoryInner>,
}

#[derive(Debug, Default)]
struct InMemoryInner {
    contract_address: Option<String>,
    require_address: bool,
    slots: HashMap<PrivateStateSlot, DidPrivateState>,
}

impl InMemoryPrivateStateStore {
    /// Build an empty store. The store does not require an address by
    /// default; tests that want the "address-not-set" failure mode should
    /// call [`Self::strict`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Make the store strict — operations before [`Self::set_contract_address`]
    /// return [`PrivateStateError::ContractAddressNotSet`].
    pub fn strict() -> Self {
        let store = Self::default();
        store.inner.lock().unwrap().require_address = true;
        store
    }

    fn check_address(&self) -> Result<(), PrivateStateError> {
        let inner = self.inner.lock().unwrap();
        if inner.require_address && inner.contract_address.is_none() {
            Err(PrivateStateError::ContractAddressNotSet)
        } else {
            Ok(())
        }
    }
}

#[async_trait]
impl PrivateStateStore for InMemoryPrivateStateStore {
    fn set_contract_address(&self, address: &str) {
        self.inner.lock().unwrap().contract_address = Some(address.to_owned());
    }

    async fn get(&self, slot: PrivateStateSlot) -> Result<Option<DidPrivateState>, PrivateStateError> {
        self.check_address()?;
        Ok(self.inner.lock().unwrap().slots.get(&slot).cloned())
    }

    async fn set(&self, slot: PrivateStateSlot, state: DidPrivateState) -> Result<(), PrivateStateError> {
        self.check_address()?;
        self.inner.lock().unwrap().slots.insert(slot, state);
        Ok(())
    }

    async fn remove(&self, slot: PrivateStateSlot) -> Result<(), PrivateStateError> {
        self.check_address()?;
        self.inner.lock().unwrap().slots.remove(&slot);
        Ok(())
    }
}

/// `bindPrivateStateProvider` — set the active contract address on the store.
pub fn bind_private_state_provider<S: PrivateStateStore + ?Sized>(store: &S, contract_address: &str) {
    store.set_contract_address(contract_address);
}

fn is_contract_address_unset(err: &PrivateStateError) -> bool {
    matches!(err, PrivateStateError::ContractAddressNotSet)
}

/// `restorePrivateState` — read the value at `slot`. Returns `Ok(None)` if
/// the slot is empty or if the store is not yet bound to a contract address.
pub async fn restore_private_state<S: PrivateStateStore + ?Sized>(
    store: &S,
    slot: PrivateStateSlot,
) -> Result<Option<DidPrivateState>, ApiError> {
    match store.get(slot).await {
        Ok(value) => Ok(value),
        Err(err) if is_contract_address_unset(&err) => Ok(None),
        Err(err) => Err(ApiError::InvalidArgument(err.to_string())),
    }
}

/// `requirePrivateState` — like [`restore_private_state`] but errors if the
/// slot is empty.
pub async fn require_private_state<S: PrivateStateStore + ?Sized>(
    store: &S,
    slot: PrivateStateSlot,
) -> Result<DidPrivateState, ApiError> {
    match restore_private_state(store, slot).await? {
        Some(state) => Ok(state),
        None => Err(ApiError::MissingPrivateState),
    }
}

/// `savePrivateState` — write a private state to a slot.
pub async fn save_private_state<S: PrivateStateStore + ?Sized>(
    store: &S,
    state: DidPrivateState,
    slot: PrivateStateSlot,
) -> Result<(), ApiError> {
    store
        .set(slot, state)
        .await
        .map_err(|err| ApiError::InvalidArgument(err.to_string()))
}

/// `initPrivateState` — if an active private state exists, return it;
/// otherwise create a new random one and persist it.
///
/// The TS source uses Web Crypto `randomBytes(32)` directly; here the caller
/// must pass a fresh secret key (typically from `rand::rngs::OsRng`). This
/// keeps `midnight-did-api` free of an RNG dependency and makes the function
/// testable.
pub async fn init_private_state<S: PrivateStateStore + ?Sized>(
    store: &S,
    new_secret_key: [u8; 32],
) -> Result<DidPrivateState, ApiError> {
    if let Some(existing) = restore_private_state(store, PrivateStateSlot::Active).await? {
        return Ok(existing);
    }
    let state = DidPrivateState {
        secret_key: new_secret_key,
    };
    match store.set(PrivateStateSlot::Active, state.clone()).await {
        Ok(()) => Ok(state),
        Err(err) if is_contract_address_unset(&err) => Ok(state),
        Err(err) => Err(ApiError::InvalidArgument(err.to_string())),
    }
}

/// `savePendingControllerPrivateState` — write to the pending slot.
pub async fn save_pending_controller_private_state<S: PrivateStateStore + ?Sized>(
    store: &S,
    state: DidPrivateState,
) -> Result<(), ApiError> {
    save_private_state(store, state, PrivateStateSlot::Pending).await
}

/// `clearPendingControllerPrivateState` — remove any value at the pending
/// slot.
pub async fn clear_pending_controller_private_state<S: PrivateStateStore + ?Sized>(store: &S) -> Result<(), ApiError> {
    store
        .remove(PrivateStateSlot::Pending)
        .await
        .map_err(|err| ApiError::InvalidArgument(err.to_string()))
}

/// `RecoverPendingControllerPrivateStateOptions` — explicit confirmation
/// that the caller has verified the rotation transaction finalized.
#[derive(Debug, Clone, Copy)]
pub struct RecoverPendingControllerPrivateStateOptions {
    /// Must be `true`. The struct exists solely as a "yes, I confirmed it"
    /// marker (just like the TS port).
    pub rotation_finalized: bool,
}

/// `recoverPendingControllerPrivateState` — promote the pending value to
/// active. Fails if `rotation_finalized` is not `true`.
pub async fn recover_pending_controller_private_state<S: PrivateStateStore + ?Sized>(
    store: &S,
    options: RecoverPendingControllerPrivateStateOptions,
) -> Result<DidPrivateState, ApiError> {
    if !options.rotation_finalized {
        return Err(ApiError::invalid_argument(
            "Pending controller private state can only be recovered after confirming the key-rotation transaction finalized",
        ));
    }
    let pending = require_private_state(store, PrivateStateSlot::Pending).await?;
    save_private_state(store, pending.clone(), PrivateStateSlot::Active).await?;
    clear_pending_controller_private_state(store).await?;
    Ok(pending)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn save_and_restore_round_trip() {
        let store = InMemoryPrivateStateStore::new();
        let state = DidPrivateState { secret_key: [7u8; 32] };
        save_private_state(&store, state.clone(), PrivateStateSlot::Active)
            .await
            .unwrap();
        let restored = restore_private_state(&store, PrivateStateSlot::Active).await.unwrap();
        assert_eq!(restored, Some(state));
    }

    #[tokio::test]
    async fn require_errors_when_missing() {
        let store = InMemoryPrivateStateStore::new();
        let err = require_private_state(&store, PrivateStateSlot::Active)
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::MissingPrivateState));
    }

    #[tokio::test]
    async fn recover_requires_finalization_marker() {
        let store = InMemoryPrivateStateStore::new();
        save_pending_controller_private_state(&store, DidPrivateState { secret_key: [3u8; 32] })
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
        assert!(matches!(err, ApiError::InvalidArgument(_)));
    }

    #[tokio::test]
    async fn recover_promotes_pending_to_active() {
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

    #[tokio::test]
    async fn strict_store_errors_before_binding() {
        let store = InMemoryPrivateStateStore::strict();
        let err = store.get(PrivateStateSlot::Active).await.unwrap_err();
        assert!(matches!(err, PrivateStateError::ContractAddressNotSet));
        store.set_contract_address("0xabc");
        let value = store.get(PrivateStateSlot::Active).await.unwrap();
        assert!(value.is_none());
    }

    #[tokio::test]
    async fn restore_swallows_contract_address_not_set() {
        let store = InMemoryPrivateStateStore::strict();
        let value = restore_private_state(&store, PrivateStateSlot::Active).await.unwrap();
        assert_eq!(value, None);
    }
}
