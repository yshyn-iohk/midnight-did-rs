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

//! High-level DID CRUD aggregations.

use midnight_did_method::hex_ext::HashOutputExt;
use midnight_did_runtime::{Backend, Contract};

use crate::contract::FinalizedTxData;
use crate::controller_operations::rotate_controller_key;
pub use crate::controller_operations::rotate_controller_key as rotate_did_controller_key;
use crate::document_operations::deactivate;
pub use crate::document_operations::{add_also_known_as, deactivate as deactivate_did, remove_also_known_as};
use crate::error::ApiError;
use crate::private_state::{
    DidPrivateState, PrivateStateSlot, PrivateStateStore, init_private_state, save_private_state,
};
pub use crate::resolution::resolve as resolve_did;
use crate::resolution::{ResolvedMidnightDid, resolve};
pub use crate::service_operations::{add_service, remove_service, update_service};
pub use crate::verification_method_operations::{
    add_schnorr_jubjub_verification_method, add_verification_method, add_verification_method_relation,
    remove_schnorr_jubjub_verification_method, remove_verification_method, remove_verification_method_relation,
    update_schnorr_jubjub_verification_method, update_verification_method, verify_schnorr_jubjub_digest_signature,
};

/// `createDID(didContract, providers, secretKey)` — Rust port of the
/// high level "create" entry point.
///
/// R1 step 7 (v0.2.0): the `secret_key` parameter is now **required**.
pub async fn create_did<B, S>(
    contract: &Contract<B>,
    store: &S,
    secret_key: [u8; 32],
) -> Result<DidPrivateState, ApiError>
where
    B: Backend,
    S: PrivateStateStore + ?Sized,
{
    crate::private_state::bind_private_state_provider(store, &contract.address.to_hex());
    init_private_state(store, secret_key).await
}

/// Update path — orchestrates a partial document update.
#[derive(Debug, Default, Clone)]
pub struct DidDocumentPatch {
    /// `alsoKnownAs` URIs to add.
    pub also_known_as_added: Vec<String>,
    /// `alsoKnownAs` URIs to remove.
    pub also_known_as_removed: Vec<String>,
    /// Service entries to add.
    pub services_added: Vec<midnight_did_domain::did_document::Service>,
    /// Service entries to update.
    pub services_updated: Vec<midnight_did_domain::did_document::Service>,
    /// Service ids to remove.
    pub services_removed: Vec<String>,
}

/// Apply a patch to a deployed DID. Operations are submitted in the order
/// they appear in the [`DidDocumentPatch`] fields. Stops on the first
/// failure.
pub async fn apply_patch<B: Backend>(
    contract: &Contract<B>,
    patch: &DidDocumentPatch,
) -> Result<Vec<FinalizedTxData>, ApiError> {
    let mut results = Vec::new();
    for uri in &patch.also_known_as_added {
        results.push(add_also_known_as(contract, uri).await?);
    }
    for uri in &patch.also_known_as_removed {
        results.push(remove_also_known_as(contract, uri).await?);
    }
    for svc in &patch.services_added {
        results.push(add_service(contract, svc).await?);
    }
    for svc in &patch.services_updated {
        results.push(update_service(contract, svc).await?);
    }
    for svc_id in &patch.services_removed {
        results.push(remove_service(contract, svc_id).await?);
    }
    Ok(results)
}

/// Re-export-friendly alias to `resolve` to highlight the operation-level
/// intent at the call site.
pub async fn read_did_document<B: Backend>(contract: &Contract<B>) -> Result<Option<ResolvedMidnightDid>, ApiError> {
    resolve(contract).await
}

/// Convenience: rotate the controller key and persist the new secret as
/// active.
pub async fn rotate_controller_key_with_derivation<B, S, F>(
    contract: &Contract<B>,
    store: &S,
    new_secret_key: [u8; 32],
    derive_public_key: F,
) -> Result<FinalizedTxData, ApiError>
where
    B: Backend,
    S: PrivateStateStore + ?Sized,
    F: FnOnce([u8; 32]) -> [u8; 32],
{
    let new_pk = derive_public_key(new_secret_key);
    rotate_controller_key(contract, store, new_secret_key, new_pk).await
}

/// Convenience helper: deactivate the DID and save a sentinel zero private
/// state to the active slot so subsequent operations fail-closed.
pub async fn deactivate_and_clear<B, S>(contract: &Contract<B>, store: &S) -> Result<FinalizedTxData, ApiError>
where
    B: Backend,
    S: PrivateStateStore + ?Sized,
{
    let result = deactivate(contract).await?;
    save_private_state(
        store,
        DidPrivateState { secret_key: [0u8; 32] },
        PrivateStateSlot::Active,
    )
    .await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use midnight_did_domain::did_document::{NewService, Service, ServiceEndpoint, ServiceType};
    use midnight_did_method::midnight_did::{MidnightNetwork, parse_contract_address};
    use midnight_did_runtime::{DidContractCall, RecordingBackend};

    use super::*;
    use crate::private_state::{InMemoryPrivateStateStore, PrivateStateSlot, restore_private_state};

    const ADDR: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    fn test_contract() -> Contract<RecordingBackend> {
        Contract::new(
            RecordingBackend::new(),
            parse_contract_address(ADDR).unwrap(),
            MidnightNetwork::Testnet,
        )
    }

    #[tokio::test]
    async fn create_did_seeds_active_slot() {
        let contract = test_contract();
        let store = InMemoryPrivateStateStore::new();
        let state = create_did(&contract, &store, [3u8; 32]).await.unwrap();
        assert_eq!(state.secret_key, [3u8; 32]);
        let restored = restore_private_state(&store, PrivateStateSlot::Active).await.unwrap();
        assert_eq!(restored.unwrap().secret_key, [3u8; 32]);
    }

    #[tokio::test]
    async fn apply_patch_runs_in_order() {
        let contract = test_contract();
        let patch = DidDocumentPatch {
            also_known_as_added: vec!["https://example.com/a".into()],
            services_added: vec![
                Service::new(NewService {
                    id: "svc-1".into(),
                    type_: ServiceType::One("LinkedDomains".into()),
                    service_endpoint: ServiceEndpoint::Uri("https://example.com".into()),
                })
                .expect("valid service"),
            ],
            ..DidDocumentPatch::default()
        };
        apply_patch(&contract, &patch).await.unwrap();
        let calls = contract.backend.recorded_calls();
        assert_eq!(calls.len(), 2);
        assert!(matches!(calls[0], DidContractCall::SetAlsoKnownAs { .. }));
        assert!(matches!(calls[1], DidContractCall::SetService { .. }));
    }

    #[tokio::test]
    async fn rotate_with_derivation_passes_public_key() {
        let contract = test_contract();
        let store = InMemoryPrivateStateStore::new();
        rotate_controller_key_with_derivation(&contract, &store, [1u8; 32], |sk| {
            let mut out = sk;
            out[0] = 42;
            out
        })
        .await
        .unwrap();
        let calls = contract.backend.recorded_calls();
        match &calls[0] {
            DidContractCall::RotateControllerKey { new_public_key } => {
                assert_eq!(new_public_key[0], 42);
                assert_eq!(new_public_key[1], 1);
            }
            _ => panic!("expected rotate call"),
        }
    }
}
