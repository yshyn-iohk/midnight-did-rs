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
//!
//! The TypeScript port has a slim `did-operations.ts` (deprecated shim) plus
//! a richer set of per-domain operation files. This Rust port collapses the
//! intended public surface into a single set of re-exports + a couple of
//! convenience helpers so callers can drive the full CRUD without reaching
//! into individual modules.
//!
//! Re-exports of the underlying operation modules:
//!
//! - Controller rotation:
//!   [`rotate_controller_key`](crate::controller_operations::rotate_controller_key).
//! - Verification methods: [`crate::verification_method_operations`].
//! - Services: [`crate::service_operations`].
//! - Document-level: [`crate::document_operations`].
//! - Resolution: [`crate::resolution::resolve`].

use crate::{
    controller_operations::rotate_controller_key,
    document_operations::deactivate,
    error::ApiError,
    private_state::{DidPrivateState, PrivateStateSlot, PrivateStateStore, init_private_state, save_private_state},
    resolution::{ResolvedMidnightDid, resolve},
};

use crate::contract::{DidContract, FinalizedTxData};

pub use crate::controller_operations::rotate_controller_key as rotate_did_controller_key;
pub use crate::document_operations::{add_also_known_as, deactivate as deactivate_did, remove_also_known_as};
pub use crate::resolution::resolve as resolve_did;
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
/// The pre-v0.2.0 `Option<[u8; 32]>` shape silently fell back to
/// `[0u8; 32]` when `None` was passed — a real footgun that
/// production callers could trip without knowing. The library never
/// decides whether to generate or accept key material; that's the
/// caller's responsibility. The reference CLI provides a
/// `--generate-secret` flag that wraps `rand::thread_rng().r#gen()`
/// for ergonomic ad-hoc use.
///
/// The TS source uses this to (a) ensure the private state slot is
/// seeded and (b) capture the controller secret key into the store,
/// before the deploy / find-contract flow returns. In the Rust port
/// the contract is assumed already deployed (the runtime crate owns
/// deployment). This helper records the controller secret into the
/// active slot so subsequent controller-bound operations succeed.
pub async fn create_did<C, S>(
    did_contract: &C,
    store: &S,
    secret_key: [u8; 32],
) -> Result<DidPrivateState, ApiError>
where
    C: DidContract + ?Sized,
    S: PrivateStateStore + ?Sized,
{
    crate::private_state::bind_private_state_provider(store, &did_contract.contract_address());
    init_private_state(store, secret_key).await
}

/// Update path — orchestrates a partial document update. Each operation
/// argument is independently optional; missing fields are left untouched.
///
/// This is a convenience helper; downstream callers can also drive the
/// individual ops directly.
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
pub async fn apply_patch<C: DidContract + ?Sized>(
    did_contract: &C,
    patch: &DidDocumentPatch,
) -> Result<Vec<FinalizedTxData>, ApiError> {
    let mut results = Vec::new();
    for uri in &patch.also_known_as_added {
        results.push(add_also_known_as(did_contract, uri).await?);
    }
    for uri in &patch.also_known_as_removed {
        results.push(remove_also_known_as(did_contract, uri).await?);
    }
    for svc in &patch.services_added {
        results.push(add_service(did_contract, svc).await?);
    }
    for svc in &patch.services_updated {
        results.push(update_service(did_contract, svc).await?);
    }
    for svc_id in &patch.services_removed {
        results.push(remove_service(did_contract, svc_id).await?);
    }
    Ok(results)
}

/// Re-export-friendly alias to `resolve` to highlight the operation-level
/// intent at the call site.
pub async fn read_did_document<C: DidContract + ?Sized>(
    did_contract: &C,
) -> Result<Option<ResolvedMidnightDid>, ApiError> {
    resolve(did_contract).await
}

/// Convenience: rotate the controller key and persist the new secret as
/// active. Wraps [`rotate_controller_key`] with the canonical
/// `pad(32, "did:controller:pk") || sk` style derivation that the in-circuit
/// `controllerKey` witness uses. The actual `persistentHash` is delegated to
/// the supplied closure because this crate does not depend on the runtime
/// hashing primitive.
pub async fn rotate_controller_key_with_derivation<C, S, F>(
    did_contract: &C,
    store: &S,
    new_secret_key: [u8; 32],
    derive_public_key: F,
) -> Result<FinalizedTxData, ApiError>
where
    C: DidContract + ?Sized,
    S: PrivateStateStore + ?Sized,
    F: FnOnce([u8; 32]) -> [u8; 32],
{
    let new_pk = derive_public_key(new_secret_key);
    rotate_controller_key(did_contract, store, new_secret_key, new_pk).await
}

/// Convenience helper: deactivate the DID and save a sentinel zero private
/// state to the active slot so subsequent operations fail-closed. Mirrors
/// the post-deactivation policy in the TS contract-lifecycle tests.
pub async fn deactivate_and_clear<C, S>(did_contract: &C, store: &S) -> Result<FinalizedTxData, ApiError>
where
    C: DidContract + ?Sized,
    S: PrivateStateStore + ?Sized,
{
    let result = deactivate(did_contract).await?;
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
    use super::*;
    use crate::contract::mock::{RecordedCall, RecordingContract};
    use crate::private_state::{InMemoryPrivateStateStore, PrivateStateSlot, restore_private_state};
    use midnight_did_domain::did_document::{NewService, Service, ServiceEndpoint, ServiceType};
    use midnight_did_method::midnight_did::MidnightNetwork;

    const ADDR: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    #[tokio::test]
    async fn create_did_seeds_active_slot() {
        let contract = RecordingContract::new(ADDR, MidnightNetwork::Testnet);
        let store = InMemoryPrivateStateStore::new();
        let state = create_did(&contract, &store, [3u8; 32]).await.unwrap();
        assert_eq!(state.secret_key, [3u8; 32]);
        let restored = restore_private_state(&store, PrivateStateSlot::Active).await.unwrap();
        assert_eq!(restored.unwrap().secret_key, [3u8; 32]);
    }

    #[tokio::test]
    async fn apply_patch_runs_in_order() {
        let contract = RecordingContract::new(ADDR, MidnightNetwork::Testnet);
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
        let calls = contract.calls();
        assert_eq!(calls.len(), 2);
        assert!(matches!(calls[0], RecordedCall::SetAlsoKnownAs(_, _)));
        assert!(matches!(calls[1], RecordedCall::SetService(_, _)));
    }

    #[tokio::test]
    async fn rotate_with_derivation_passes_public_key() {
        let contract = RecordingContract::new(ADDR, MidnightNetwork::Testnet);
        let store = InMemoryPrivateStateStore::new();
        rotate_controller_key_with_derivation(&contract, &store, [1u8; 32], |sk| {
            let mut out = sk;
            out[0] = 42;
            out
        })
        .await
        .unwrap();
        let calls = contract.calls();
        match &calls[0] {
            RecordedCall::RotateControllerKey(pk) => {
                assert_eq!(pk[0], 42);
                assert_eq!(pk[1], 1);
            }
            _ => panic!("expected rotate call"),
        }
    }
}
