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

//! DID-subject string helpers.
//!
//! Rust port of `packages/api/src/did-subject.ts`. The TS code looks up the
//! current `NetworkId` from `getNetworkId()` at runtime; in Rust the network
//! is carried by the [`midnight_did_runtime::Contract`] instance so callers
//! pass it explicitly. This keeps the helpers pure and testable.

use midnight_did_domain::ledger_utils::{BoundIdField, normalize_bound_fragment_id};
use midnight_did_method::hex_ext::HashOutputExt;
use midnight_did_method::midnight_did::{ContractAddress, MidnightNetwork, create_midnight_did_string};
use midnight_did_runtime::{Backend, Contract};

use crate::error::ApiError;

/// `getDidSubject(didContract)` — return the canonical
/// `did:midnight:<network>:<address>` for the contract.
pub fn get_did_subject<B: Backend>(contract: &Contract<B>) -> Result<String, ApiError> {
    Ok(create_midnight_did_string(&contract.address.to_hex(), contract.network).0)
}

/// `normalizeBoundFragmentId(didContract, value, field)` — the contract-aware
/// wrapper that validates the DID subject of `value` matches the contract's
/// DID subject before normalising to the leading-`#` form.
pub fn normalize_bound_fragment_id_for<B: Backend>(
    contract: &Contract<B>,
    value: &str,
    field: BoundIdField,
) -> Result<String, ApiError> {
    let subject = get_did_subject(contract)?;
    Ok(normalize_bound_fragment_id(value, field, &subject)?)
}

/// Pure variant that takes a pre-computed DID subject + network.
///
/// Useful in code paths that already have the subject in hand and want to
/// avoid the double-`parse_contract_address` call.
pub fn get_did_subject_for(address: &ContractAddress, network: MidnightNetwork) -> String {
    create_midnight_did_string(&address.to_hex(), network).0
}

#[cfg(test)]
mod tests {
    use midnight_did_method::midnight_did::parse_contract_address;
    use midnight_did_runtime::RecordingBackend;

    use super::*;

    const ADDR: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    fn contract() -> Contract<RecordingBackend> {
        Contract::new(
            RecordingBackend::new(),
            parse_contract_address(ADDR).unwrap(),
            MidnightNetwork::Testnet,
        )
    }

    #[test]
    fn computes_did_subject_for_testnet() {
        let contract = contract();
        let subject = get_did_subject(&contract).expect("subject");
        assert_eq!(subject, format!("did:midnight:testnet:{ADDR}"));
    }

    #[test]
    fn normalizes_bare_fragment_id() {
        let contract = contract();
        let id =
            normalize_bound_fragment_id_for(&contract, "key-1", BoundIdField::VerificationMethodId).expect("normalize");
        assert_eq!(id, "#key-1");
    }

    #[test]
    fn rejects_foreign_did_subject() {
        let contract = contract();
        let other = "1".repeat(64);
        let value = format!("did:midnight:testnet:{other}#key-1");
        let err = normalize_bound_fragment_id_for(&contract, &value, BoundIdField::VerificationMethodId).unwrap_err();
        assert!(matches!(err, ApiError::LedgerUtils(_)));
    }
}
