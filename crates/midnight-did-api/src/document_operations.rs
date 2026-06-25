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

//! Top-level DID Document operations: `alsoKnownAs` and deactivation.
//!
//! Rust port of `packages/api/src/document-operations.ts`.

use midnight_did_domain::ledger_utils::assert_absolute_uri;
use midnight_did_runtime::{Backend, Contract};

use crate::{
    contract::{FinalizedTxData, SetMutation},
    error::{ApiError, ContractError},
};

/// `addAlsoKnownAs(didContract, aliasUri)` — adds an `alsoKnownAs` entry on
/// the DID document.
pub async fn add_also_known_as<B: Backend>(
    contract: &Contract<B>,
    alias_uri: &str,
) -> Result<FinalizedTxData, ApiError> {
    let alias = assert_absolute_uri(alias_uri, Some("aliasUri"))?;
    contract
        .set_also_known_as(alias, SetMutation::Insert)
        .await
        .map_err(|e| ApiError::Contract(ContractError::Failed(e.to_string())))
}

/// `removeAlsoKnownAs(didContract, aliasUri)`.
pub async fn remove_also_known_as<B: Backend>(
    contract: &Contract<B>,
    alias_uri: &str,
) -> Result<FinalizedTxData, ApiError> {
    let alias = assert_absolute_uri(alias_uri, Some("aliasUri"))?;
    contract
        .set_also_known_as(alias, SetMutation::Remove)
        .await
        .map_err(|e| ApiError::Contract(ContractError::Failed(e.to_string())))
}

/// `deactivate(didContract)` — mark the DID inactive + deactivated on
/// the ledger.
pub async fn deactivate<B: Backend>(contract: &Contract<B>) -> Result<FinalizedTxData, ApiError> {
    contract
        .deactivate()
        .await
        .map_err(|e| ApiError::Contract(ContractError::Failed(e.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;
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
    async fn adds_an_alias() {
        let contract = contract();
        add_also_known_as(&contract, "https://example.com").await.unwrap();
        let calls = contract.backend.recorded_calls();
        assert!(matches!(
            &calls[..],
            [DidContractCall::SetAlsoKnownAs { alias_uri, mutation: SetMutation::Insert }] if alias_uri == "https://example.com"
        ));
    }

    #[tokio::test]
    async fn rejects_non_absolute_alias() {
        let contract = contract();
        let err = add_also_known_as(&contract, "not-a-uri").await.unwrap_err();
        assert!(matches!(err, ApiError::LedgerUtils(_)));
    }

    #[tokio::test]
    async fn deactivates() {
        let contract = contract();
        deactivate(&contract).await.unwrap();
        let calls = contract.backend.recorded_calls();
        assert!(matches!(&calls[..], [DidContractCall::Deactivate]));
    }
}
