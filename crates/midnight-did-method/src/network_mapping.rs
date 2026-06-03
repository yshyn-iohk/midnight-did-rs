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

//! Runtime ↔ domain network-id mapping.
//!
//! Mirrors `packages/api/src/network-mapping.ts` and the
//! `RuntimeToDomain` / `DomainToRuntime` class wrappers
//! (`runtime-to-domain.ts`, `domain-to-runtime.ts`) from the TypeScript
//! source.

use crate::midnight_did::MidnightNetwork;

/// Midnight runtime network identifier (matches the `NetworkId` string keys
/// used by `@midnight-ntwrk/midnight-js-network-id`).
///
/// The values are kept in lowercase string form to match the TS canonical
/// representation; serialization in either direction goes through
/// [`RuntimeNetworkId::as_str`] / [`RuntimeNetworkId::from_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuntimeNetworkId {
    /// Local / not-deployed test environment.
    Undeployed,
    /// Internal development network.
    DevNet,
    /// Public testnet.
    Testnet,
    /// Production mainnet.
    Mainnet,
    /// Preview network.
    Preview,
    /// Preproduction network.
    Preprod,
}

impl RuntimeNetworkId {
    /// Wire-format string used by midnight-js-network-id.
    pub fn as_str(self) -> &'static str {
        match self {
            RuntimeNetworkId::Undeployed => "undeployed",
            RuntimeNetworkId::DevNet => "devnet",
            RuntimeNetworkId::Testnet => "testnet",
            RuntimeNetworkId::Mainnet => "mainnet",
            RuntimeNetworkId::Preview => "preview",
            RuntimeNetworkId::Preprod => "preprod",
        }
    }

    /// Parse a wire-format string. Returns `None` for unknown ids.
    ///
    /// Named `parse` rather than `from_str` to avoid colliding with the
    /// `std::str::FromStr` trait, which would require returning a `Result`
    /// rather than an `Option` and changing the public surface.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "undeployed" => Some(RuntimeNetworkId::Undeployed),
            "devnet" => Some(RuntimeNetworkId::DevNet),
            "testnet" => Some(RuntimeNetworkId::Testnet),
            "mainnet" => Some(RuntimeNetworkId::Mainnet),
            "preview" => Some(RuntimeNetworkId::Preview),
            "preprod" => Some(RuntimeNetworkId::Preprod),
            _ => None,
        }
    }

    /// All six runtime ids, in canonical declaration order.
    pub const ALL: [RuntimeNetworkId; 6] = [
        RuntimeNetworkId::Undeployed,
        RuntimeNetworkId::DevNet,
        RuntimeNetworkId::Testnet,
        RuntimeNetworkId::Mainnet,
        RuntimeNetworkId::Preview,
        RuntimeNetworkId::Preprod,
    ];
}

/// `RuntimeToDomain.NetworkMap` from the TS source.
///
/// Total over [`RuntimeNetworkId`] — every runtime id maps to exactly one
/// [`MidnightNetwork`].
pub fn runtime_to_domain(id: RuntimeNetworkId) -> MidnightNetwork {
    match id {
        RuntimeNetworkId::Undeployed => MidnightNetwork::Undeployed,
        RuntimeNetworkId::DevNet => MidnightNetwork::DevNet,
        RuntimeNetworkId::Testnet => MidnightNetwork::Testnet,
        RuntimeNetworkId::Mainnet => MidnightNetwork::Mainnet,
        RuntimeNetworkId::Preview => MidnightNetwork::Preview,
        RuntimeNetworkId::Preprod => MidnightNetwork::Preprod,
    }
}

/// `DomainToRuntime.NetworkMap` from the TS source.
///
/// Partial — [`MidnightNetwork::Offchain`] has no runtime equivalent.
pub fn domain_to_runtime(network: MidnightNetwork) -> Option<RuntimeNetworkId> {
    match network {
        MidnightNetwork::Undeployed => Some(RuntimeNetworkId::Undeployed),
        MidnightNetwork::DevNet => Some(RuntimeNetworkId::DevNet),
        MidnightNetwork::Testnet => Some(RuntimeNetworkId::Testnet),
        MidnightNetwork::Mainnet => Some(RuntimeNetworkId::Mainnet),
        MidnightNetwork::Preview => Some(RuntimeNetworkId::Preview),
        MidnightNetwork::Preprod => Some(RuntimeNetworkId::Preprod),
        MidnightNetwork::Offchain => None,
    }
}

/// Class-shaped wrapper matching the TS `RuntimeToDomain` static class.
///
/// Provided to make porting downstream call sites mechanical; idiomatic
/// Rust callers should prefer [`runtime_to_domain`] directly.
pub struct RuntimeToDomain;

impl RuntimeToDomain {
    /// Map a runtime id to its domain [`MidnightNetwork`].
    pub fn network_map(id: RuntimeNetworkId) -> MidnightNetwork {
        runtime_to_domain(id)
    }
}

/// Class-shaped wrapper matching the TS `DomainToRuntime` static class.
pub struct DomainToRuntime;

impl DomainToRuntime {
    /// Map a domain [`MidnightNetwork`] back to its runtime id.
    pub fn network_map(network: MidnightNetwork) -> Option<RuntimeNetworkId> {
        domain_to_runtime(network)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_to_domain_is_total_over_all_runtime_ids() {
        for id in RuntimeNetworkId::ALL {
            // Returning a value (rather than panicking) is what the test
            // proves; `domain_to_runtime` round-trips.
            let domain = runtime_to_domain(id);
            assert_eq!(domain_to_runtime(domain), Some(id), "{id:?} did not round-trip");
        }
    }

    #[test]
    fn offchain_has_no_runtime_id() {
        assert_eq!(domain_to_runtime(MidnightNetwork::Offchain), None);
    }

    #[test]
    fn parse_round_trip() {
        for id in RuntimeNetworkId::ALL {
            assert_eq!(RuntimeNetworkId::parse(id.as_str()), Some(id));
        }
        assert_eq!(RuntimeNetworkId::parse("nope"), None);
    }
}
