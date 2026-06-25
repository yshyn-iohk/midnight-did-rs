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

//! Controller-key rotation operations.
//!
//! Rust port of `packages/api/src/controller-operations.ts`.
//!
//! The TS source derives the new controller public key via
//! `deriveControllerPublicKey(newSecretKey)`, then drives the
//! `rotateControllerKey` circuit on the contract. To keep the API crate
//! independent of in-circuit derivation, callers pass the *derived* public
//! key as an explicit argument.

use midnight_did_runtime::{Backend, Contract};

use crate::{
    contract::FinalizedTxData,
    error::{ApiError, ContractError},
    private_state::{
        DidPrivateState, PrivateStateSlot, PrivateStateStore, clear_pending_controller_private_state,
        save_pending_controller_private_state, save_private_state,
    },
};

/// `rotateControllerKey(didContract, providers, newSecretKey)`.
///
/// Stashes the new private state into the pending slot, drives the
/// `rotateControllerKey` circuit, and on success promotes the pending value
/// to active.
///
/// If the circuit finalises but the active promotion fails, this returns
/// [`ApiError::Controller(ControllerError::RotationOrphaned)`] so the caller
/// knows to invoke
/// [`crate::private_state::recover_pending_controller_private_state`] once
/// the transaction is confirmed.
pub async fn rotate_controller_key<B, S>(
    contract: &Contract<B>,
    store: &S,
    new_secret_key: [u8; 32],
    new_controller_public_key: [u8; 32],
) -> Result<FinalizedTxData, ApiError>
where
    B: Backend,
    S: PrivateStateStore + ?Sized,
{
    let next_state = DidPrivateState {
        secret_key: new_secret_key,
    };

    save_pending_controller_private_state(store, next_state.clone()).await?;

    let result = match contract.rotate_controller_key(new_controller_public_key).await {
        Ok(result) => result,
        Err(err) => {
            // The TS source attempts to clear the pending slot — best-effort.
            let _ = clear_pending_controller_private_state(store).await;
            return Err(ApiError::Contract(ContractError::Failed(err.to_string())));
        }
    };

    // Try to promote the new state to active.
    if let Err(promote_err) = save_private_state(store, next_state, PrivateStateSlot::Active).await {
        return Err(ApiError::Controller(crate::error::ControllerError::RotationOrphaned(promote_err.to_string())));
    }
    // Best-effort cleanup of the pending slot.
    let _ = clear_pending_controller_private_state(store).await;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::private_state::{InMemoryPrivateStateStore, restore_private_state};
    use midnight_did_method::midnight_did::{MidnightNetwork, parse_contract_address};
    use midnight_did_runtime::{DidContractCall, RecordingBackend};

    const ADDR: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    fn contract() -> Contract<RecordingBackend> {
        Contract::new(
            RecordingBackend::new(),
            parse_contract_address(ADDR).unwrap(),
            MidnightNetwork::Testnet,
        )
    }

    #[tokio::test]
    async fn rotates_and_promotes_private_state() {
        let contract = contract();
        let store = InMemoryPrivateStateStore::new();

        let new_sk = [4u8; 32];
        let new_pk = [9u8; 32];
        rotate_controller_key(&contract, &store, new_sk, new_pk).await.unwrap();

        let calls = contract.backend.recorded_calls();
        assert!(matches!(
            calls.first(),
            Some(DidContractCall::RotateControllerKey { new_public_key }) if *new_public_key == new_pk
        ));

        let active = restore_private_state(&store, PrivateStateSlot::Active).await.unwrap();
        assert_eq!(active.unwrap().secret_key, new_sk);
        let pending = restore_private_state(&store, PrivateStateSlot::Pending).await.unwrap();
        assert!(pending.is_none());
    }
}
