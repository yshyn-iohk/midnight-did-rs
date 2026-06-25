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

//! Service endpoint operations.
//!
//! Rust port of `packages/api/src/service-operations.ts`.

use midnight_did_domain::{did_document::Service, ledger_utils::BoundIdField};
use midnight_did_runtime::{Backend, Contract};

use crate::{
    contract::{FinalizedTxData, MapMutation},
    error::{ApiError, ContractError},
    ledger_mappers::service_to_ledger,
    subject::normalize_bound_fragment_id_for,
};

/// `addService`.
pub async fn add_service<B: Backend>(
    contract: &Contract<B>,
    service: &Service,
) -> Result<FinalizedTxData, ApiError> {
    let ledger = service_to_ledger(contract, service)?;
    contract
        .set_service(ledger, MapMutation::Insert)
        .await
        .map_err(|e| ApiError::Contract(ContractError::Failed(e.to_string())))
}

/// `updateService`.
pub async fn update_service<B: Backend>(
    contract: &Contract<B>,
    service: &Service,
) -> Result<FinalizedTxData, ApiError> {
    let ledger = service_to_ledger(contract, service)?;
    contract
        .set_service(ledger, MapMutation::Update)
        .await
        .map_err(|e| ApiError::Contract(ContractError::Failed(e.to_string())))
}

/// `removeService`.
pub async fn remove_service<B: Backend>(
    contract: &Contract<B>,
    service_id: &str,
) -> Result<FinalizedTxData, ApiError> {
    let normalized = normalize_bound_fragment_id_for(contract, service_id, BoundIdField::ShortServiceId)?;
    contract
        .remove_service(normalized)
        .await
        .map_err(|e| ApiError::Contract(ContractError::Failed(e.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use midnight_did_domain::did_document::{NewService, ServiceEndpoint, ServiceType};
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
    async fn adds_a_service() {
        let contract = contract();
        let svc = Service::new(NewService {
            id: "svc-1".into(),
            type_: ServiceType::One("LinkedDomains".into()),
            service_endpoint: ServiceEndpoint::Uri("https://example.com".into()),
        })
        .expect("valid service");
        add_service(&contract, &svc).await.unwrap();
        let calls = contract.backend.recorded_calls();
        assert!(matches!(
            &calls[..],
            [DidContractCall::SetService { service, mutation: MapMutation::Insert }] if service.id == "#svc-1"
        ));
    }

    #[tokio::test]
    async fn removes_a_service() {
        let contract = contract();
        remove_service(&contract, "svc-1").await.unwrap();
        let calls = contract.backend.recorded_calls();
        assert!(matches!(&calls[..], [DidContractCall::RemoveService { service_id }] if service_id == "#svc-1"));
    }
}
