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

//! CRUD flow orchestrator.
//!
//! Each step mutates the mock contract through the high-level API, then
//! installs the next ledger snapshot on the mock so subsequent `resolve`
//! calls reflect the change. The post-step ledger is computed in pure
//! Rust here — it does not come from the runtime crate (which doesn't
//! build today; see crate-level docs).
//!
//! All steps return a [`StepOutput`] that the caller (main.rs or
//! capture-fixtures) renders to JSON.

use anyhow::{Context, Result};
use serde_json::{Value, json};

use midnight_did_api::{
    contract::DidLedgerSnapshot,
    did_operations::{create_did, rotate_did_controller_key},
    document_operations::{add_also_known_as, deactivate},
    ledger_mappers::service_to_ledger,
    private_state::InMemoryPrivateStateStore,
    resolution::resolve,
    service_operations::add_service,
    verification_method_operations::{add_verification_method, add_verification_method_relation},
};
use midnight_did_domain::did_document::VerificationMethodRelation;
use midnight_did_method::midnight_did::parse_contract_address;
use midnight_did_runtime::{Contract, RecordingBackend};

use crate::fixtures::{
    self, ALSO_KNOWN_AS_URI, CONTRACT_ADDRESS, CREATED_MS, INITIAL_CONTROLLER_PK_HEX, INITIAL_SECRET_KEY, NETWORK,
    ROTATED_CONTROLLER_PK_BYTES, ROTATED_SECRET_KEY, STEP_ADVANCE_MS, VM_FRAGMENT,
};

/// Single step's serialized output.
#[derive(Debug, Clone)]
pub struct StepOutput {
    /// Stable kebab-case step identifier (also used as the filename stem in
    /// `capture-fixtures`).
    pub name: &'static str,
    /// Human-readable step name printed in the `== Step N: <name> ==` header.
    pub display_name: &'static str,
    /// The DID-Document-shaped JSON value rendered to stdout / disk.
    pub document: Value,
}

/// Selector for `run --step <name>` and the underlying step enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Create,
    SetVm,
    SetService,
    SetAka,
    Rotate,
    Resolve,
    Deactivate,
}

impl Step {
    /// All steps in execution order.
    pub const ALL: [Step; 7] = [
        Step::Create,
        Step::SetVm,
        Step::SetService,
        Step::SetAka,
        Step::Rotate,
        Step::Resolve,
        Step::Deactivate,
    ];

    /// Parse the kebab-case CLI form.
    pub fn from_cli(value: &str) -> Option<Step> {
        Some(match value {
            "create" => Step::Create,
            "set-vm" => Step::SetVm,
            "set-service" => Step::SetService,
            "set-aka" | "set-also-known-as" => Step::SetAka,
            "rotate" => Step::Rotate,
            "resolve" => Step::Resolve,
            "deactivate" => Step::Deactivate,
            _ => return None,
        })
    }

    /// Stable kebab-case identifier.
    pub fn name(self) -> &'static str {
        match self {
            Step::Create => "create",
            Step::SetVm => "set-vm",
            Step::SetService => "set-service",
            Step::SetAka => "set-aka",
            Step::Rotate => "rotate",
            Step::Resolve => "resolve",
            Step::Deactivate => "deactivate",
        }
    }

    /// Human-readable display name.
    pub fn display_name(self) -> &'static str {
        match self {
            Step::Create => "create",
            Step::SetVm => "set-vm-insert",
            Step::SetService => "set-service-insert",
            Step::SetAka => "set-alsoKnownAs-insert",
            Step::Rotate => "rotate",
            Step::Resolve => "resolve",
            Step::Deactivate => "deactivate",
        }
    }
}

/// One-stop driver that owns the mock contract + private-state store and
/// applies pre-computed ledger snapshots between mutations.
pub struct FlowDriver {
    contract: Contract<RecordingBackend>,
    store: InMemoryPrivateStateStore,
    operation_count: u64,
    version: u64,
    updated_ms: u64,
}

impl FlowDriver {
    /// Build a driver seeded with the initial ledger snapshot.
    pub fn new() -> Self {
        Self {
            contract: Contract::new(
                RecordingBackend::with_snapshot(initial_ledger()),
                parse_contract_address(CONTRACT_ADDRESS).expect("valid CONTRACT_ADDRESS fixture"),
                NETWORK,
            ),
            store: InMemoryPrivateStateStore::new(),
            operation_count: 0,
            version: 1,
            updated_ms: CREATED_MS,
        }
    }

    /// Advance the bookkeeping counters that mirror a successful mutation.
    fn advance(&mut self) {
        self.operation_count += 1;
        self.version += 1;
        self.updated_ms += STEP_ADVANCE_MS;
    }

    /// Apply a transform to the current ledger snapshot and reinstall it.
    async fn mutate_ledger<F>(&self, mutate: F) -> DidLedgerSnapshot
    where
        F: FnOnce(&mut DidLedgerSnapshot),
    {
        // Read current state via the backend (also records a synthetic
        // ReadLedger call).
        let mut snapshot = self
            .contract
            .read_snapshot()
            .await
            .expect("mock backend read_snapshot never fails");
        mutate(&mut snapshot);
        self.contract.backend.set_snapshot(snapshot.clone());
        snapshot
    }

    /// Run a single step and return its serialized DID Document.
    pub async fn run_step(&mut self, step: Step) -> Result<StepOutput> {
        match step {
            Step::Create => self.step_create().await,
            Step::SetVm => self.step_set_vm().await,
            Step::SetService => self.step_set_service().await,
            Step::SetAka => self.step_set_aka().await,
            Step::Rotate => self.step_rotate().await,
            Step::Resolve => self.step_resolve().await,
            Step::Deactivate => self.step_deactivate().await,
        }
    }

    /// Resolve the contract and serialize the (document, metadata) pair.
    async fn resolved_value(&self) -> Result<Value> {
        let resolved = resolve(&self.contract)
            .await
            .context("resolve failed")?
            .context("contract returned no live state")?;
        Ok(json!({
            "didDocument": serde_json::to_value(&resolved.did_document)?,
            "didDocumentMetadata": serde_json::to_value(&resolved.did_document_metadata)?,
        }))
    }

    async fn step_create(&mut self) -> Result<StepOutput> {
        create_did(&self.contract, &self.store, INITIAL_SECRET_KEY)
            .await
            .context("create_did failed")?;
        // The mock contract already starts at the initial ledger; capture it.
        let document = self.resolved_value().await?;
        Ok(StepOutput {
            name: Step::Create.name(),
            display_name: Step::Create.display_name(),
            document,
        })
    }

    async fn step_set_vm(&mut self) -> Result<StepOutput> {
        let vm = fixtures::sample_verification_method();
        add_verification_method(&self.contract, &vm)
            .await
            .context("add_verification_method failed")?;
        self.advance();
        let ledger_vm = fixtures::sample_ledger_verification_method();
        let updated_ms = self.updated_ms;
        let version = self.version;
        let op_count = self.operation_count;
        self.mutate_ledger(|state| {
            state.verification_methods.insert(VM_FRAGMENT.to_string(), ledger_vm);
            state.version = version;
            state.operation_count = op_count;
            state.updated_ms = updated_ms;
        })
        .await;

        // Scope the VM to authentication + assertionMethod relations.
        add_verification_method_relation(&self.contract, VerificationMethodRelation::Authentication, VM_FRAGMENT)
            .await
            .context("add_verification_method_relation(authentication) failed")?;
        self.advance();
        let updated_ms = self.updated_ms;
        let version = self.version;
        let op_count = self.operation_count;
        self.mutate_ledger(|state| {
            state.authentication_relation.push(VM_FRAGMENT.to_string());
            state.version = version;
            state.operation_count = op_count;
            state.updated_ms = updated_ms;
        })
        .await;

        add_verification_method_relation(&self.contract, VerificationMethodRelation::AssertionMethod, VM_FRAGMENT)
            .await
            .context("add_verification_method_relation(assertionMethod) failed")?;
        self.advance();
        let updated_ms = self.updated_ms;
        let version = self.version;
        let op_count = self.operation_count;
        self.mutate_ledger(|state| {
            state.assertion_method_relation.push(VM_FRAGMENT.to_string());
            state.version = version;
            state.operation_count = op_count;
            state.updated_ms = updated_ms;
        })
        .await;

        let document = self.resolved_value().await?;
        Ok(StepOutput {
            name: Step::SetVm.name(),
            display_name: Step::SetVm.display_name(),
            document,
        })
    }

    async fn step_set_service(&mut self) -> Result<StepOutput> {
        let svc = fixtures::sample_service();
        add_service(&self.contract, &svc).await.context("add_service failed")?;
        self.advance();
        let updated_ms = self.updated_ms;
        let version = self.version;
        let op_count = self.operation_count;
        let ledger_service = service_to_ledger(&self.contract, &svc).context("service_to_ledger failed")?;
        self.mutate_ledger(|state| {
            state.services.insert(ledger_service.id.clone(), ledger_service);
            state.version = version;
            state.operation_count = op_count;
            state.updated_ms = updated_ms;
        })
        .await;

        let document = self.resolved_value().await?;
        Ok(StepOutput {
            name: Step::SetService.name(),
            display_name: Step::SetService.display_name(),
            document,
        })
    }

    async fn step_set_aka(&mut self) -> Result<StepOutput> {
        add_also_known_as(&self.contract, ALSO_KNOWN_AS_URI)
            .await
            .context("add_also_known_as failed")?;
        self.advance();
        let updated_ms = self.updated_ms;
        let version = self.version;
        let op_count = self.operation_count;
        self.mutate_ledger(|state| {
            state.also_known_as.push(ALSO_KNOWN_AS_URI.to_string());
            state.version = version;
            state.operation_count = op_count;
            state.updated_ms = updated_ms;
        })
        .await;

        let document = self.resolved_value().await?;
        Ok(StepOutput {
            name: Step::SetAka.name(),
            display_name: Step::SetAka.display_name(),
            document,
        })
    }

    async fn step_rotate(&mut self) -> Result<StepOutput> {
        rotate_did_controller_key(
            &self.contract,
            &self.store,
            ROTATED_SECRET_KEY,
            ROTATED_CONTROLLER_PK_BYTES,
        )
        .await
        .context("rotate_controller_key failed")?;
        self.advance();
        let new_pk_hex = hex::encode(ROTATED_CONTROLLER_PK_BYTES);
        let updated_ms = self.updated_ms;
        let version = self.version;
        let op_count = self.operation_count;
        self.mutate_ledger(|state| {
            state.controller_public_key_hex = new_pk_hex;
            state.version = version;
            state.operation_count = op_count;
            state.updated_ms = updated_ms;
        })
        .await;

        let document = self.resolved_value().await?;
        Ok(StepOutput {
            name: Step::Rotate.name(),
            display_name: Step::Rotate.display_name(),
            document,
        })
    }

    async fn step_resolve(&mut self) -> Result<StepOutput> {
        let document = self.resolved_value().await?;
        Ok(StepOutput {
            name: Step::Resolve.name(),
            display_name: Step::Resolve.display_name(),
            document,
        })
    }

    async fn step_deactivate(&mut self) -> Result<StepOutput> {
        deactivate(&self.contract).await.context("deactivate failed")?;
        self.advance();
        let updated_ms = self.updated_ms;
        let version = self.version;
        let op_count = self.operation_count;
        self.mutate_ledger(|state| {
            state.active = false;
            state.deactivated = true;
            state.version = version;
            state.operation_count = op_count;
            state.updated_ms = updated_ms;
        })
        .await;

        let document = self.resolved_value().await?;
        Ok(StepOutput {
            name: Step::Deactivate.name(),
            display_name: Step::Deactivate.display_name(),
            document,
        })
    }
}

impl Default for FlowDriver {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the seed-state ledger snapshot used by step 1 ("create"). The
/// values mirror the in-circuit defaults: active, version 1, controller
/// public key set to the fixture-derived hex.
fn initial_ledger() -> DidLedgerSnapshot {
    DidLedgerSnapshot {
        id_hex: CONTRACT_ADDRESS.to_string(),
        active: true,
        deactivated: false,
        controller_public_key_hex: INITIAL_CONTROLLER_PK_HEX.to_string(),
        version: 1,
        operation_count: 0,
        contract_version: 1,
        created_ms: CREATED_MS,
        updated_ms: CREATED_MS,
        ..Default::default()
    }
}
