<!--
This file is part of midnightntwrk/midnight-did-rs.
Copyright (C) 2026 Midnight Foundation
SPDX-License-Identifier: Apache-2.0
-->

# R2 — Contract abstraction reform (design spec)

**Status**: Design draft, 2026-06-24. Implementation gated on the
midnight-did-runtime crate's regen unblocking (compact codegen-rust
walker gap A12+ — see [ADR 0006](../adr/0006-runtime-crate-halo2-block.md)
and the codegen-rust branch's `Walker gap` task series).
**Targets**: midnight-did-rs v0.4.0 (post-R1-4b/4c v0.3.0).
**Partial supersession (proposed)**: [ADR 0002](../adr/0002-trait-erasure-for-contract.md),
[ADR 0004](../adr/0004-private-state-as-trait.md).

## TL;DR

Collapse the 12-method async `DidContract` trait into a concrete
`Contract` struct that owns the generated circuit-call surface, and
push trait-based pluggability down to a 2-method `Backend` trait
(`submit_tx`, `read_state`) at the I/O boundary. The shape mirrors
how the upstream Midnight Rust crates (compact-runtime, midnight-base
crypto) are factored — one concrete type per logical artefact,
generics over the I/O substrate.

## Context

R1 (v0.2.0) cleaned the type-safety surface. R2 closes the design
debt that the wave-3 audit flagged as the highest-impact remaining
inversion:

- **`DidContract` is a 12-method async trait** with one method per
  exported `did.compact` circuit. Every method has the same shape:
  build a typed input, hand it to the contract, get back a
  `FinalizedTxData`. The trait surface duplicates the circuit list
  three times (trait definition, `RecordingContract` mock, eventual
  production impl).
- **`PrivateState` is similarly trait-shaped** (see ADR 0004) when
  the only state shape the production code uses is the one the
  compact compiler generates. The trait was the only way to keep the
  api crate building while the runtime crate was halo2-blocked
  (ADR 0006).
- The two trait inversions trade compile-time-once design overhead
  for the ability to ship the api crate independently of the
  runtime crate. With ADR 0006's halo2 block clearing and the
  codegen-rust walker gap chain (A1-A11 closed) making did.compact
  regen tractable, that constraint is dissolving.

Reaffirmed from the audit: **the api layer shouldn't enumerate the
contract surface a second time**. Every new exported circuit added
to `did.compact` should auto-flow into the api's published surface
via the regenerated `Contract` struct, not via a hand-edit to
`DidContract`.

## Decision (proposed)

Replace `DidContract` with two pieces:

### 1. Concrete `Contract<B: Backend>` struct in the runtime crate

```rust
// crates/midnight-did-runtime/src/lib.rs
pub struct Contract<B: Backend> {
    pub backend: B,
    pub address: ContractAddress,
    pub network: MidnightNetwork,
    pub witnesses: DidWitnesses,
    // ... whatever else the compact-generated Contract<PS, W> needs
}

impl<B: Backend> Contract<B> {
    // One inherent async fn per exported circuit — generated, not hand-written.
    pub async fn rotate_controller_key(&self, new_pk: [u8; 32])
        -> Result<FinalizedTxData, ContractError> { /* delegate to backend */ }

    pub async fn set_verification_method(&self, m: LedgerVerificationMethod, mu: MapMutation)
        -> Result<FinalizedTxData, ContractError> { /* delegate */ }

    // ... 10 more, all generated.
}
```

These methods are not part of any trait. They form a stable, concrete
API surface that consumers depend on directly. The list grows when
`did.compact` grows; the regenerated `Contract` is the single source
of truth.

### 2. Minimal `Backend` trait at the I/O boundary

```rust
// crates/midnight-did-runtime/src/backend.rs
#[async_trait]
pub trait Backend: Send + Sync {
    /// Submit a built transaction (proven + signed by the upstream
    /// proof / wallet stack) and return its finalization data.
    async fn submit_tx(&self, tx: BuiltTx) -> Result<FinalizedTxData, BackendError>;

    /// Read the current contract state from the indexer / public data
    /// provider. Returns the raw ChargedState that Ledger::<DefaultDB>::new()
    /// can decode.
    async fn read_state(&self) -> Result<ChargedState<DefaultDB>, BackendError>;
}
```

That's the entire trait. Two methods. Substrate-agnostic.

Three impls ship in v0.4.0:

- `LiveBackend` — wraps the wallet SDK + proof server + indexer. The
  only impl that talks to a real Midnight node. Lives in the runtime
  crate.
- `RecordingBackend` — in-memory; records each submitted `BuiltTx`
  as a tagged enum; serves a configurable `ChargedState`. Replaces
  the current `RecordingContract`. Tests assert on the recorded
  builds plus the final snapshot.
- `ResolverBackend` — read-only; `submit_tx` returns
  `Err(BackendError::ReadOnly)`. For consumers that only need the
  resolve path (currently still serviced by the trait's `unreachable!`
  pattern, per ADR 0002's resolver-consumer note).

### 3. PrivateState becomes a concrete struct (per-network)

`DidPrivateState` is the compact-generated witness backing-store. No
trait. R1-4b/4c's private-fields direction is already heading this
way for the in-domain types; this extends it to the witness side.

Per-network variants (testnet/devnet/mainnet) become `#[cfg]`-gated
fields on the concrete struct, not separate trait impls.

## Why this shape

**Inversion is paid for by indirection.** The 12-method trait was
worth it when the api crate had to ship without the runtime crate
building. Once the runtime crate builds, the trait is paying a cost
(duplication, drift risk, indirect calls) for a benefit no consumer
is taking advantage of.

**Backend is where pluggability is actually needed.** Tests want a
recording variant. Resolver wants a read-only variant. Production
wants the live wallet/proof/indexer stack. All three differ only in
how `submit_tx` + `read_state` resolve; everything above them (the
12 circuit methods) is identical concrete code.

**One file to edit when did.compact grows.** Adding an exported
circuit to `did.compact` regenerates the compact-runtime crate's
`Contract`. R2 makes the api crate's published surface auto-extend;
no hand-edit to the trait, no `RecordedCall` enum to update, no
unreachable-arm migration in the resolver consumer.

**Pulls api closer to upstream-Midnight conventions.** Compact-runtime,
midnight-base-crypto, midnight-coin-structure all expose concrete
structs with the generics over substrate (DB backend, network) — not
trait-erased facades. The R1-2 reuse of `ContractAddress` /
`HashOutput` already aligned the type primitives; R2 aligns the
abstraction shape too.

## What this is NOT

- **Not a re-design of the operation builders.** `did_operations`,
  `controller_operations`, `verification_method_operations`,
  `subject` continue to take a generic parameter — but it's
  `Contract<B>` (where they previously took `&dyn DidContract`).
  Their internal flow doesn't change.
- **Not a removal of `DidLedgerSnapshot`.** That stays as the
  flattened view returned by `read_ledger()` (now an inherent
  method on `Contract<B>`). The mapping logic from raw `Ledger` to
  `DidLedgerSnapshot` is unchanged.
- **Not a deprecation of `ContractError`.** The `Backend` trait gets
  its own narrow `BackendError` (network failures, proof failures,
  read-state decode errors); `ContractError` continues to model the
  high-level circuit-call failures that the inherent methods produce.
  R1-6's domain-error split stays intact.

## Migration path

Three commits in v0.4.0 (gated on runtime crate building):

1. **R2-1**: Land the `Backend` trait + `LiveBackend` /
   `RecordingBackend` / `ResolverBackend` impls in the runtime
   crate. The 12-method `DidContract` continues to exist alongside.
2. **R2-2**: Migrate the operation builders + every test to depend
   on `Contract<B>` instead of `&dyn DidContract`. Remove the
   `RecordingContract` mock; tests use `Contract<RecordingBackend>`.
3. **R2-3**: Delete `DidContract` and the `RecordedCall` enum.
   CHANGELOG + new ADR 0008. Pin v0.4.0 in `[workspace.package]`.

Existing v0.2/0.3 callers get one upgrade step:
- `&dyn DidContract` → `&Contract<impl Backend>` (or take the
  concrete `Contract<B>` by reference at the use site).
- `RecordingContract::new(...)` → `Contract::<RecordingBackend>::new(...)`.

Wire format unchanged. All 13 byte-parity fixtures still apply.

## Risks

- **Object safety**: `Contract<B>` is generic, not object-safe. Any
  current `Box<dyn DidContract>` storage at consumer call sites
  (none in this workspace; flagged for downstream-consumer audit
  before v0.4.0 publication) must move to enum-dispatch or rely on
  the concrete type at the storage site. Mitigation: ship a
  `BackendEnum` umbrella enum (`Live` / `Recording` / `Resolver`) as
  the practical "I want one of three" type, since this is exactly
  the dispatch pattern the audit identified.
- **Runtime-crate dep on api crate types**: today the api crate
  depends on the (unbuilt) runtime crate's compact-generated types
  via re-exports under a `not(feature = "no-runtime")` cfg. R2 makes
  this explicit. Mitigation: domain-level types stay in
  `midnight-did-domain` (unchanged from R1); only the *contract
  surface* moves to the runtime crate.
- **Halo2 / proof server in CI**: the runtime crate's tests now
  depend on a proof-server fixture (or a mocked one). The recording
  backend doesn't touch halo2, so api-level tests stay fast.
  Mitigation: tag the live-network tests as `#[ignore]` by default;
  CI matrix opts them in.

## What stays the same (v0.4.0 invariants)

- W3C-DID document shape and resolver semantics: unchanged.
- Wire-format JSON: unchanged (13 byte-parity fixtures).
- CLI surface (`midnight-did-cli`): unchanged.
- UniFFI flat error enum: unchanged (still maps from `ContractError`
  + `BackendError` via the existing `From` impls).

## References

- [ADR 0002 — Trait erasure for contract](../adr/0002-trait-erasure-for-contract.md)
  (partial supersession proposed)
- [ADR 0004 — Private state as trait](../adr/0004-private-state-as-trait.md)
  (partial supersession proposed)
- [ADR 0006 — Runtime crate halo2 block](../adr/0006-runtime-crate-halo2-block.md)
  (gate)
- [Design spec — R1 type-safety sweep](2026-06-23-r1-type-safety-sweep-design.md)
  (precedes)
- Audit notes — `Rust DID port/Session 2026-06-23 wave 3 — audit + design.md`
  (Obsidian; off-repo)
