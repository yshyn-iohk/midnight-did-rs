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

//! Opaque service handle exposed to foreign-language callers.
//!
//! Foreign callers see a single `DidServiceHandle` class with a `new()`
//! constructor; behind the FFI boundary the handle holds an
//! [`Arc<Mutex<RecordingContract>>`][RecordingContract] so concurrent FFI
//! calls — particularly Swift's structured concurrency tasks and Kotlin's
//! coroutines — cannot race on the mock contract's in-memory state.

use std::sync::Arc;
use tokio::sync::Mutex;

use midnight_did_api::contract::mock::RecordingContract;
use midnight_did_method::midnight_did::MidnightNetwork;

/// Default contract address for the mock — same value used by the
/// integration tests in `midnight-did-api`. Real impls will replace the
/// inner [`RecordingContract`] with a `compact-runtime`-backed contract.
const DEFAULT_ADDRESS: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

/// FFI-opaque service handle.
///
/// Holds the underlying mock contract behind a [`tokio::sync::Mutex`] so the
/// async FFI methods can `.lock().await` without blocking the foreign-
/// language thread pool. Once the runtime crate builds, the inner
/// [`RecordingContract`] is swapped for an
/// `Arc<dyn DidContract + Send + Sync>` with no change to the FFI surface.
#[derive(uniffi::Object)]
pub struct DidServiceHandle {
    pub(crate) contract: Arc<Mutex<RecordingContract>>,
}

#[uniffi::export]
impl DidServiceHandle {
    /// Construct a new handle backed by the in-memory mock contract.
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            contract: Arc::new(Mutex::new(RecordingContract::new(
                DEFAULT_ADDRESS,
                MidnightNetwork::Testnet,
            ))),
        })
    }

    /// Return the contract address as a hex string.
    pub fn contract_address(&self) -> String {
        DEFAULT_ADDRESS.to_string()
    }

    /// Return the wire spelling of the contract's network (`"testnet"`,
    /// `"mainnet"`, ...).
    pub fn network(&self) -> String {
        MidnightNetwork::Testnet.as_wire_str().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_constructor_round_trip() {
        let handle = DidServiceHandle::new();
        assert_eq!(handle.contract_address(), DEFAULT_ADDRESS);
        assert_eq!(handle.network(), "testnet");
    }

    #[tokio::test]
    async fn handle_mutex_is_async_acquirable() {
        let handle = DidServiceHandle::new();
        let guard = handle.contract.lock().await;
        // Verify we can re-acquire after drop on the same task.
        drop(guard);
        let _again = handle.contract.lock().await;
    }
}
