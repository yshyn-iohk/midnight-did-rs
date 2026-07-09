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

//! Integration tests for the runtime ↔ domain network-id mapping.
//!
//! Rust port of:
//! - `packages/api/src/test/runtime-to-domain.test.ts`
//! - `packages/api/src/test/domain-to-runtime.test.ts`
//!
//! The TS source uses two `class`-like static wrappers (`RuntimeToDomain`,
//! `DomainToRuntime`) plus exported constant maps; the Rust port exposes the
//! same shape via [`RuntimeToDomain::network_map`] /
//! [`DomainToRuntime::network_map`] plus free functions.

use midnight_did_api::network_mapping::{
    DomainToRuntime, RuntimeNetworkId, RuntimeToDomain, domain_to_runtime, runtime_to_domain,
};
use midnight_did_method::midnight_did::MidnightNetwork;

// ---------------------------------------------------------------------------
// TS: "maps all NetworkId values to MidnightNetwork"
// ---------------------------------------------------------------------------
#[test]
fn runtime_to_domain_maps_every_network_id() {
    assert_eq!(
        RuntimeToDomain::network_map(RuntimeNetworkId::Undeployed),
        MidnightNetwork::Undeployed
    );
    assert_eq!(
        RuntimeToDomain::network_map(RuntimeNetworkId::DevNet),
        MidnightNetwork::DevNet
    );
    assert_eq!(
        RuntimeToDomain::network_map(RuntimeNetworkId::Testnet),
        MidnightNetwork::Testnet
    );
    assert_eq!(
        RuntimeToDomain::network_map(RuntimeNetworkId::Mainnet),
        MidnightNetwork::Mainnet
    );
    assert_eq!(
        RuntimeToDomain::network_map(RuntimeNetworkId::Preview),
        MidnightNetwork::Preview
    );
    assert_eq!(
        RuntimeToDomain::network_map(RuntimeNetworkId::Preprod),
        MidnightNetwork::Preprod
    );
}

// ---------------------------------------------------------------------------
// TS: "is inverse of DomainToRuntime.NetworkMap for all defined values"
// ---------------------------------------------------------------------------
#[test]
fn runtime_to_domain_is_inverse_of_domain_to_runtime() {
    for id in RuntimeNetworkId::ALL {
        let domain = runtime_to_domain(id);
        assert_eq!(
            domain_to_runtime(domain),
            Some(id),
            "round-trip failed for {id:?} (domain = {domain:?})"
        );
    }
}

// ---------------------------------------------------------------------------
// TS: "expect(DomainToRuntime.NetworkMap).not.toHaveProperty(MidnightNetwork.Offchain)"
// ---------------------------------------------------------------------------
#[test]
fn offchain_has_no_runtime_id() {
    assert_eq!(DomainToRuntime::network_map(MidnightNetwork::Offchain), None);
    assert_eq!(domain_to_runtime(MidnightNetwork::Offchain), None);
}

// ---------------------------------------------------------------------------
// TS: "maps all MidnightNetwork values to NetworkId"
// ---------------------------------------------------------------------------
#[test]
fn domain_to_runtime_maps_every_defined_midnight_network() {
    assert_eq!(
        DomainToRuntime::network_map(MidnightNetwork::Undeployed),
        Some(RuntimeNetworkId::Undeployed)
    );
    assert_eq!(
        DomainToRuntime::network_map(MidnightNetwork::DevNet),
        Some(RuntimeNetworkId::DevNet)
    );
    assert_eq!(
        DomainToRuntime::network_map(MidnightNetwork::Testnet),
        Some(RuntimeNetworkId::Testnet)
    );
    assert_eq!(
        DomainToRuntime::network_map(MidnightNetwork::Mainnet),
        Some(RuntimeNetworkId::Mainnet)
    );
    assert_eq!(
        DomainToRuntime::network_map(MidnightNetwork::Preview),
        Some(RuntimeNetworkId::Preview)
    );
    assert_eq!(
        DomainToRuntime::network_map(MidnightNetwork::Preprod),
        Some(RuntimeNetworkId::Preprod)
    );
}

// ---------------------------------------------------------------------------
// Extra: the wire-format strings round-trip with the static map exactly. The
// TS source pins these to NetworkId string values used by
// `@midnight-ntwrk/midnight-js-network-id`; if the wire form drifts, this
// test catches it before downstream callers do.
// ---------------------------------------------------------------------------
#[test]
fn runtime_network_id_wire_format_strings_match_ts() {
    let pairs = [
        (RuntimeNetworkId::Undeployed, "undeployed"),
        (RuntimeNetworkId::DevNet, "devnet"),
        (RuntimeNetworkId::Testnet, "testnet"),
        (RuntimeNetworkId::Mainnet, "mainnet"),
        (RuntimeNetworkId::Preview, "preview"),
        (RuntimeNetworkId::Preprod, "preprod"),
    ];
    for (id, wire) in pairs {
        assert_eq!(id.as_str(), wire);
        assert_eq!(RuntimeNetworkId::parse(wire), Some(id));
    }
    assert_eq!(RuntimeNetworkId::parse("offchain"), None);
}
