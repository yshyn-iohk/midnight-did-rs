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
//! Notes on porting:
//!
//! - The TS source accepts `Uint8Array` and rejects non-32-byte inputs at
//!   runtime; the Rust signature uses `[u8; 32]` so the length check is a
//!   compile-time guarantee. We retain the rest of the behavioural
//!   coverage:
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
//! The mock contract is the workspace `RecordingContract`; the failing
//! contract + failing store are small local doubles below.

use std::sync::Mutex;

use async_trait::async_trait;
use midnight_did_api::{
    contract::{
        DidContract, DidLedgerSnapshot, FinalizedTxData, LedgerSchnorrJubjubVerificationMethod, LedgerService,
        LedgerVerificationMethod, LedgerVerificationMethodRelation, MapMutation, SchnorrJubjubDigest,
        SchnorrJubjubSignature, SetMutation,
        mock::{RecordedCall, RecordingContract},
    },
    controller_operations::rotate_controller_key,
    error::{ApiError, ContractError},
    private_state::{
        DidPrivateState, InMemoryPrivateStateStore, PrivateStateError, PrivateStateSlot, PrivateStateStore,
        restore_private_state,
    },
};
use midnight_did_domain::midnight::MidnightNetwork;

const ADDR: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

// ---------------------------------------------------------------------------
// TS: "rotates to a locally derived controller public key and stores the
// new secret"
// ---------------------------------------------------------------------------
#[tokio::test]
async fn rotates_and_promotes_private_state() {
    let contract = RecordingContract::new(ADDR, MidnightNetwork::Undeployed);
    let store = InMemoryPrivateStateStore::new();
    let new_sk = [4u8; 32];
    let new_pk = [9u8; 32];

    rotate_controller_key(&contract, &store, new_sk, new_pk)
        .await
        .expect("rotate ok");

    let calls = contract.calls();
    assert!(
        matches!(calls.first(), Some(RecordedCall::RotateControllerKey(pk)) if *pk == new_pk),
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
    let contract = RecordingContract::new(ADDR, MidnightNetwork::Undeployed);
    let store = FailingStore::new(FailMode::FailOnEverySet);

    let err = rotate_controller_key(&contract, &store, [1u8; 32], [2u8; 32])
        .await
        .unwrap_err();
    assert!(matches!(err, ApiError::InvalidArgument(_)), "{err}");

    let calls = contract.calls();
    assert!(
        !calls.iter().any(|c| matches!(c, RecordedCall::RotateControllerKey(_))),
        "circuit must not run when pending write fails first: {calls:?}"
    );
}

// ---------------------------------------------------------------------------
// TS: "clears pending state when the transaction fails before finalization"
// ---------------------------------------------------------------------------
#[tokio::test]
async fn clears_pending_state_when_circuit_fails() {
    let contract = FailingContract::new();
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
    let contract = RecordingContract::new(ADDR, MidnightNetwork::Undeployed);
    let store = FailingStore::new(FailMode::FailOnNthSet { fail_on: 2 });

    let err = rotate_controller_key(&contract, &store, [3u8; 32], [4u8; 32])
        .await
        .unwrap_err();
    assert!(
        matches!(err, ApiError::ControllerRotationOrphaned(_)),
        "expected ControllerRotationOrphaned, got {err}"
    );

    // The pending slot must still hold the new secret key.
    let pending = store
        .raw_get(PrivateStateSlot::Pending)
        .expect("pending should not have been cleared");
    assert_eq!(pending.secret_key, [3u8; 32]);
    // The circuit ran exactly once.
    let circuit_calls: Vec<_> = contract
        .calls()
        .iter()
        .filter(|c| matches!(c, RecordedCall::RotateControllerKey(_)))
        .cloned()
        .collect();
    assert_eq!(circuit_calls.len(), 1, "circuit should run once: {:?}", circuit_calls);
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

/// A `DidContract` whose `rotate_controller_key` always fails. All other
/// methods are unused by the controller tests; their bodies panic so an
/// accidentally-extended test catches the missing implementation early.
struct FailingContract;

impl FailingContract {
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl DidContract for FailingContract {
    fn contract_address(&self) -> String {
        ADDR.to_owned()
    }

    fn network(&self) -> MidnightNetwork {
        MidnightNetwork::Undeployed
    }

    async fn read_ledger(&self) -> Result<DidLedgerSnapshot, ContractError> {
        Ok(DidLedgerSnapshot::default())
    }

    async fn rotate_controller_key(&self, _new_pk: [u8; 32]) -> Result<FinalizedTxData, ContractError> {
        Err(ContractError::Failed("transaction rejected".into()))
    }

    async fn set_verification_method(
        &self,
        _method: LedgerVerificationMethod,
        _mutation: MapMutation,
    ) -> Result<FinalizedTxData, ContractError> {
        unimplemented!("not used by controller tests")
    }

    async fn remove_verification_method(&self, _id: String) -> Result<FinalizedTxData, ContractError> {
        unimplemented!("not used by controller tests")
    }

    async fn set_schnorr_jubjub_verification_method(
        &self,
        _method: LedgerSchnorrJubjubVerificationMethod,
        _mutation: MapMutation,
    ) -> Result<FinalizedTxData, ContractError> {
        unimplemented!("not used by controller tests")
    }

    async fn remove_schnorr_jubjub_verification_method(&self, _id: String) -> Result<FinalizedTxData, ContractError> {
        unimplemented!("not used by controller tests")
    }

    async fn verify_schnorr_jubjub_digest_signature(
        &self,
        _id: String,
        _digest: SchnorrJubjubDigest,
        _signature: SchnorrJubjubSignature,
    ) -> Result<FinalizedTxData, ContractError> {
        unimplemented!("not used by controller tests")
    }

    async fn set_verification_method_relation(
        &self,
        _relation: LedgerVerificationMethodRelation,
        _id: String,
        _mutation: SetMutation,
    ) -> Result<FinalizedTxData, ContractError> {
        unimplemented!("not used by controller tests")
    }

    async fn set_service(
        &self,
        _service: LedgerService,
        _mutation: MapMutation,
    ) -> Result<FinalizedTxData, ContractError> {
        unimplemented!("not used by controller tests")
    }

    async fn remove_service(&self, _id: String) -> Result<FinalizedTxData, ContractError> {
        unimplemented!("not used by controller tests")
    }

    async fn set_also_known_as(
        &self,
        _alias: String,
        _mutation: SetMutation,
    ) -> Result<FinalizedTxData, ContractError> {
        unimplemented!("not used by controller tests")
    }

    async fn deactivate(&self) -> Result<FinalizedTxData, ContractError> {
        unimplemented!("not used by controller tests")
    }
}
