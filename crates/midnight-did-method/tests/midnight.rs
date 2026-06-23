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

//! Port of `@midnight-ntwrk/midnight-did-domain`'s
//! `src/test/midnight.test.ts`. Only the pure-data behaviours are ported
//! here — none of the original cases touch the chain.

use midnight_did_method::hex_ext::HashOutputExt;
use midnight_did_method::midnight_did::{
    MidnightDidError, MidnightNetwork, create_midnight_did_string, parse_contract_address, parse_midnight_did,
    parse_midnight_did_string,
};

const SAMPLE: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

#[test]
fn parses_contract_addresses_and_builds_did_strings() {
    let address = parse_contract_address(SAMPLE).unwrap();
    // v0.2.0: ContractAddress is now the upstream
    // `compact_runtime::ContractAddress(pub HashOutput)` — its hex
    // rendering goes through HashOutputExt::to_hex.
    assert_eq!(address.to_hex(), SAMPLE);
    let did = create_midnight_did_string(&address.to_hex(), MidnightNetwork::DevNet);
    assert_eq!(did.0, format!("did:midnight:devnet:{SAMPLE}"));
}

#[test]
fn rejects_invalid_contract_address_strings() {
    assert!(parse_contract_address("zz").is_err());
    assert!(parse_contract_address(&"c".repeat(63)).is_err());
}

#[test]
fn parses_and_validates_midnight_did_strings() {
    let did = format!("did:midnight:testnet:{SAMPLE}");
    let parsed = parse_midnight_did_string(&did).unwrap();
    assert_eq!(parsed.0, did);
    let (network, id) = parse_midnight_did(&parsed).unwrap();
    assert_eq!(network, MidnightNetwork::Testnet);
    assert_eq!(id.to_hex(), SAMPLE);
}

#[test]
fn rejects_dids_with_invalid_network_or_address() {
    assert!(matches!(
        parse_midnight_did_string(&format!("did:midnight:unknown:{SAMPLE}")),
        Err(MidnightDidError::UnknownNetwork)
    ));
    assert!(matches!(
        parse_midnight_did_string(&format!("did:midnight:devnet:{}", "c".repeat(63))),
        Err(MidnightDidError::BadMethodSpecificId)
    ));
    assert!(matches!(
        parse_midnight_did_string(&format!("did:midnight:offchain:{}", "C".repeat(64))),
        Err(MidnightDidError::OffchainNotLowercase)
    ));
    assert!(matches!(
        parse_midnight_did_string(&format!("did:midnight:offchain:{}:not+base64url", SAMPLE)),
        Err(MidnightDidError::BadOffchainStateEncoding)
    ));
}

#[test]
fn maps_undeployed_network() {
    let did = format!("did:midnight:undeployed:{SAMPLE}");
    let (network, _) = parse_midnight_did(&parse_midnight_did_string(&did).unwrap()).unwrap();
    assert_eq!(network, MidnightNetwork::Undeployed);
}

#[test]
fn maps_preview_and_preprod() {
    let preview = format!("did:midnight:preview:{SAMPLE}");
    let preprod = format!("did:midnight:preprod:{SAMPLE}");
    assert_eq!(
        parse_midnight_did(&parse_midnight_did_string(&preview).unwrap())
            .unwrap()
            .0,
        MidnightNetwork::Preview
    );
    assert_eq!(
        parse_midnight_did(&parse_midnight_did_string(&preprod).unwrap())
            .unwrap()
            .0,
        MidnightNetwork::Preprod
    );
}

#[test]
fn maps_offchain_network() {
    let did = format!("did:midnight:offchain:{SAMPLE}");
    assert_eq!(
        parse_midnight_did(&parse_midnight_did_string(&did).unwrap()).unwrap().0,
        MidnightNetwork::Offchain
    );
}

#[test]
fn accepts_long_form_offchain_did_with_encoded_state() {
    let did = format!("did:midnight:offchain:{SAMPLE}:AQIDBA");
    let parsed = parse_midnight_did(&parse_midnight_did_string(&did).unwrap()).unwrap();
    assert_eq!(parsed.0, MidnightNetwork::Offchain);
    assert_eq!(parsed.1.to_hex(), SAMPLE);
}
