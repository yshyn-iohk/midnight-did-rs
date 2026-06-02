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

use crate::{
    contract::{DidContract, FinalizedTxData, SetMutation},
    error::ApiError,
};

/// `addAlsoKnownAs(didContract, aliasUri)` — adds an `alsoKnownAs` entry on
/// the DID document.
pub async fn add_also_known_as<C: DidContract + ?Sized>(
    did_contract: &C,
    alias_uri: &str,
) -> Result<FinalizedTxData, ApiError> {
    let alias = assert_absolute_uri(alias_uri, Some("aliasUri"))?;
    Ok(did_contract.set_also_known_as(alias, SetMutation::Insert).await?)
}

/// `removeAlsoKnownAs(didContract, aliasUri)`.
pub async fn remove_also_known_as<C: DidContract + ?Sized>(
    did_contract: &C,
    alias_uri: &str,
) -> Result<FinalizedTxData, ApiError> {
    let alias = assert_absolute_uri(alias_uri, Some("aliasUri"))?;
    Ok(did_contract.set_also_known_as(alias, SetMutation::Remove).await?)
}

/// `deactivate(didContract)` — mark the DID inactive + deactivated on
/// the ledger.
pub async fn deactivate<C: DidContract + ?Sized>(did_contract: &C) -> Result<FinalizedTxData, ApiError> {
    Ok(did_contract.deactivate().await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::mock::{RecordedCall, RecordingContract};
    use midnight_did_domain::midnight::MidnightNetwork;

    const ADDR: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    #[tokio::test]
    async fn adds_an_alias() {
        let contract = RecordingContract::new(ADDR, MidnightNetwork::Testnet);
        add_also_known_as(&contract, "https://example.com").await.unwrap();
        let calls = contract.calls();
        assert!(matches!(
            &calls[..],
            [RecordedCall::SetAlsoKnownAs(uri, SetMutation::Insert)] if uri == "https://example.com"
        ));
    }

    #[tokio::test]
    async fn rejects_non_absolute_alias() {
        let contract = RecordingContract::new(ADDR, MidnightNetwork::Testnet);
        let err = add_also_known_as(&contract, "not-a-uri").await.unwrap_err();
        assert!(matches!(err, ApiError::LedgerUtils(_)));
    }

    #[tokio::test]
    async fn deactivates() {
        let contract = RecordingContract::new(ADDR, MidnightNetwork::Testnet);
        deactivate(&contract).await.unwrap();
        let calls = contract.calls();
        assert!(matches!(&calls[..], [RecordedCall::Deactivate]));
    }
}
