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

use crate::{
    contract::{DidContract, FinalizedTxData, MapMutation},
    error::ApiError,
    ledger_mappers::service_to_ledger,
    subject::normalize_bound_fragment_id_for,
};

/// `addService`.
pub async fn add_service<C: DidContract + ?Sized>(
    did_contract: &C,
    service: &Service,
) -> Result<FinalizedTxData, ApiError> {
    let ledger = service_to_ledger(did_contract, service)?;
    Ok(did_contract.set_service(ledger, MapMutation::Insert).await?)
}

/// `updateService`.
pub async fn update_service<C: DidContract + ?Sized>(
    did_contract: &C,
    service: &Service,
) -> Result<FinalizedTxData, ApiError> {
    let ledger = service_to_ledger(did_contract, service)?;
    Ok(did_contract.set_service(ledger, MapMutation::Update).await?)
}

/// `removeService`.
pub async fn remove_service<C: DidContract + ?Sized>(
    did_contract: &C,
    service_id: &str,
) -> Result<FinalizedTxData, ApiError> {
    let normalized = normalize_bound_fragment_id_for(did_contract, service_id, BoundIdField::ShortServiceId)?;
    Ok(did_contract.remove_service(normalized).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::mock::{RecordedCall, RecordingContract};
    use midnight_did_domain::did_document::{NewService, ServiceEndpoint, ServiceType};
    use midnight_did_method::midnight_did::MidnightNetwork;

    const ADDR: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    #[tokio::test]
    async fn adds_a_service() {
        let contract = RecordingContract::new(ADDR, MidnightNetwork::Testnet);
        let svc = Service::new(NewService {
            id: "svc-1".into(),
            type_: ServiceType::One("LinkedDomains".into()),
            service_endpoint: ServiceEndpoint::Uri("https://example.com".into()),
        })
        .expect("valid service");
        add_service(&contract, &svc).await.unwrap();
        let calls = contract.calls();
        assert!(matches!(
            &calls[..],
            [RecordedCall::SetService(ledger, MapMutation::Insert)] if ledger.id == "#svc-1"
        ));
    }

    #[tokio::test]
    async fn removes_a_service() {
        let contract = RecordingContract::new(ADDR, MidnightNetwork::Testnet);
        remove_service(&contract, "svc-1").await.unwrap();
        let calls = contract.calls();
        assert!(matches!(&calls[..], [RecordedCall::RemoveService(id)] if id == "#svc-1"));
    }
}
