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

//! Integration tests for `controller_operations::rotate_controller_key`.
//!
//! Rust port of `packages/api/src/test/controller-operations.test.ts`.
//!
//! Behavioural coverage retained from the TS source:
//!   - happy path: pending slot is written before the rotation circuit, the
//!     circuit is invoked with the derived public key, the active slot is
//!     written after the circuit succeeds, and the pending slot is cleared.
//!   - storage offline before the circuit -> error surfaces, circuit not
//!     invoked.
//!   - circuit failure after pending write -> pending slot is best-effort
//!     cleared, caller sees the contract error.
//!   - circuit success + active promotion failure -> caller sees
//!     `ControllerRotationOrphaned`; the pending slot is NOT cleared.
//!
//! v0.4.0: the mock contract is `Contract<RecordingBackend>`; the failing
//! contract is `Contract<FailingBackend>` where `FailingBackend` is a
//! purpose-built `Backend` impl whose `submit_tx` always errors.

use std::sync::Mutex;

use async_trait::async_trait;
use midnight_did_api::{
    controller_operations::rotate_controller_key,
    error::{ApiError, ContractError},
    private_state::{
        DidPrivateState, InMemoryPrivateStateStore, PrivateStateError, PrivateStateSlot, PrivateStateStore,
        restore_private_state,
    },
};
use midnight_did_method::midnight_did::{MidnightNetwork, parse_contract_address};
use midnight_did_runtime::{
    Backend, BackendError, BuiltTx, Contract, DidContractCall, DidLedgerSnapshot, FinalizedTxData, RecordingBackend,
};

const ADDR: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn recording_contract() -> Contract<RecordingBackend> {
    Contract::new(
        RecordingBackend::new(),
        parse_contract_address(ADDR).unwrap(),
        MidnightNetwork::Undeployed,
    )
}

// ---------------------------------------------------------------------------
// TS: "rotates to a locally derived controller public key and stores the
// new secret"
// ---------------------------------------------------------------------------
#[tokio::test]
async fn rotates_and_promotes_private_state() {
    let contract = recording_contract();
    let store = InMemoryPrivateStateStore::new();
    let new_sk = [4u8; 32];
    let new_pk = [9u8; 32];

    rotate_controller_key(&contract, &store, new_sk, new_pk)
        .await
        .expect("rotate ok");

    let calls = contract.backend.recorded_calls();
    assert!(
        matches!(calls.first(), Some(DidContractCall::RotateControllerKey { new_public_key: pk }) if *pk == new_pk),
        "expected RotateControllerKey({new_pk:?}), got {calls:?}"
    );

    let active = restore_private_state(&store, PrivateStateSlot::Active).await.unwrap();
    assert_eq!(active.expect("active set").secret_key, new_sk);
    let pending = restore_private_state(&store, PrivateStateSlot::Pending).await.unwrap();
    assert!(pending.is_none(), "pending should be cleared after success");
}

// ---------------------------------------------------------------------------
// TS: "rejects before submitting a transaction if pending state cannot be
// saved"
// ---------------------------------------------------------------------------
#[tokio::test]
async fn surfaces_storage_failure_before_invoking_circuit() {
    let contract = recording_contract();
    let store = FailingStore::new(FailMode::FailOnEverySet);

    let err = rotate_controller_key(&contract, &store, [1u8; 32], [2u8; 32])
        .await
        .unwrap_err();
    assert!(matches!(err, ApiError::InvalidArgument(_)), "{err}");

    let calls = contract.backend.recorded_calls();
    assert!(
        !calls.iter().any(|c| matches!(c, DidContractCall::RotateControllerKey { .. })),
        "circuit must not run when pending write fails first: {calls:?}"
    );
}

// ---------------------------------------------------------------------------
// TS: "clears pending state when the transaction fails before finalization"
// ---------------------------------------------------------------------------
#[tokio::test]
async fn clears_pending_state_when_circuit_fails() {
    let contract = Contract::new(
        FailingBackend::new(),
        parse_contract_address(ADDR).unwrap(),
        MidnightNetwork::Undeployed,
    );
    let store = InMemoryPrivateStateStore::new();

    let err = rotate_controller_key(&contract, &store, [2u8; 32], [3u8; 32])
        .await
        .unwrap_err();
    // Circuit failure surfaces as ApiError::Contract.
    assert!(matches!(err, ApiError::Contract(_)), "{err}");

    let pending = restore_private_state(&store, PrivateStateSlot::Pending).await.unwrap();
    assert!(pending.is_none(), "pending should be cleared after circuit failure");
}

// ---------------------------------------------------------------------------
// TS: "keeps pending state when active promotion fails after finalization"
// ---------------------------------------------------------------------------
#[tokio::test]
async fn keeps_pending_when_active_promotion_fails() {
    let contract = recording_contract();
    let store = FailingStore::new(FailMode::FailOnNthSet { fail_on: 2 });

    let err = rotate_controller_key(&contract, &store, [3u8; 32], [4u8; 32])
        .await
        .unwrap_err();
    assert!(
        matches!(err, ApiError::Controller(midnight_did_api::error::ControllerError::RotationOrphaned(_))),
        "expected ControllerRotationOrphaned, got {err}"
    );

    // The pending slot must still hold the new secret key.
    let pending = store
        .raw_get(PrivateStateSlot::Pending)
        .expect("pending should not have been cleared");
    assert_eq!(pending.secret_key, [3u8; 32]);
    // The circuit ran exactly once.
    let circuit_calls: Vec<_> = contract
        .backend
        .recorded_calls()
        .iter()
        .filter(|c| matches!(c, DidContractCall::RotateControllerKey { .. }))
        .cloned()
        .collect();
    assert_eq!(circuit_calls.len(), 1, "circuit should run once: {:?}", circuit_calls);
}

// silence dead_code for the unused ContractError import — kept so callers
// can assert on `ApiError::Contract(ContractError::Failed(_))` semantics if
// they extend this file.
#[allow(dead_code)]
fn _force_contract_error_usage() -> ContractError {
    ContractError::Failed("placeholder".into())
}

// ===========================================================================
// Local test doubles.
// ===========================================================================

/// Strategy for the failing store's `set` method.
#[derive(Debug, Clone, Copy)]
enum FailMode {
    /// Every `set` call fails.
    FailOnEverySet,
    /// Fail on the Nth `set` call (1-indexed). Earlier calls succeed.
    FailOnNthSet { fail_on: usize },
}

/// A `PrivateStateStore` that can be configured to fail `set` calls in
/// specific orderings, mirroring the vitest `mockRejectedValueOnce` /
/// `mockResolvedValueOnce` patterns the TS test uses.
struct FailingStore {
    inner: Mutex<FailingInner>,
}

struct FailingInner {
    mode: FailMode,
    set_calls: usize,
    slots: std::collections::HashMap<PrivateStateSlot, DidPrivateState>,
}

impl FailingStore {
    fn new(mode: FailMode) -> Self {
        Self {
            inner: Mutex::new(FailingInner {
                mode,
                set_calls: 0,
                slots: Default::default(),
            }),
        }
    }

    fn raw_get(&self, slot: PrivateStateSlot) -> Option<DidPrivateState> {
        self.inner.lock().unwrap().slots.get(&slot).cloned()
    }
}

#[async_trait]
impl PrivateStateStore for FailingStore {
    fn set_contract_address(&self, _address: &str) {}

    async fn get(&self, slot: PrivateStateSlot) -> Result<Option<DidPrivateState>, PrivateStateError> {
        Ok(self.inner.lock().unwrap().slots.get(&slot).cloned())
    }

    async fn set(&self, slot: PrivateStateSlot, state: DidPrivateState) -> Result<(), PrivateStateError> {
        let mut inner = self.inner.lock().unwrap();
        inner.set_calls += 1;
        let should_fail = match inner.mode {
            FailMode::FailOnEverySet => true,
            FailMode::FailOnNthSet { fail_on } => inner.set_calls == fail_on,
        };
        if should_fail {
            return Err(PrivateStateError::Backend("simulated failure".into()));
        }
        inner.slots.insert(slot, state);
        Ok(())
    }

    async fn remove(&self, slot: PrivateStateSlot) -> Result<(), PrivateStateError> {
        self.inner.lock().unwrap().slots.remove(&slot);
        Ok(())
    }
}

/// A `Backend` whose `submit_tx` always returns an error. Replaces the
/// pre-v0.4.0 `FailingContract` impl of the deleted `DidContract` trait.
///
/// Custom backends use the `RawChargedState` + `RawDb` re-exports from
/// the runtime crate so api-level callers do not need a direct
/// `compact-runtime` dep just to satisfy the `Backend::read_state`
/// signature.
struct FailingBackend;

impl FailingBackend {
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Backend for FailingBackend {
    async fn submit_tx(&self, _tx: BuiltTx) -> Result<FinalizedTxData, BackendError> {
        Err(BackendError::Other("transaction rejected".into()))
    }

    async fn read_state(
        &self,
    ) -> Result<midnight_did_runtime::backend::RawChargedState<midnight_did_runtime::backend::RawDb>, BackendError> {
        unimplemented!("not used by controller tests")
    }

    async fn read_snapshot(&self) -> Result<DidLedgerSnapshot, BackendError> {
        Ok(DidLedgerSnapshot::default())
    }
}
