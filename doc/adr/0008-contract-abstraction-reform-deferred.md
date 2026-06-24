<!--
This file is part of midnightntwrk/midnight-did-rs.
Copyright (C) 2026 Midnight Foundation
SPDX-License-Identifier: Apache-2.0
-->

# ADR 0008 — R2 contract-abstraction reform: R2-1 shipped, R2-2/R2-3 deferred

**Status:** Accepted — partial
**Date:** 2026-06-24
**Supersedes (partially):** [ADR 0002](./0002-trait-erasure-for-contract.md)

## Context

R2 (per `doc/specs/2026-06-24-r2-contract-abstraction-design.md`)
splits the `&dyn DidContract` trait-erased shape into three pieces:

- **R2-1:** introduce a `Backend` trait (2 methods) + `LiveBackend` /
  `RecordingBackend` / `ResolverBackend` impls in
  `midnight-did-runtime`.
- **R2-2:** migrate operation builders + 56 integration tests to
  `Contract<B: Backend>` and the deterministic `RecordingBackend`.
- **R2-3:** delete `DidContract` + `RecordingContract`; tag v0.4.0.

R2-1 shipped in commit
[`5649d6a`](https://github.com/midnightntwrk/midnight-did-rs/commit/5649d6a):
`Backend` async trait (`submit_tx`, `read_state`), `BackendError`
(Network / Decode / ReadOnly / Other), `LiveBackend` stub,
`RecordingBackend` with `Mutex<Vec<BuiltTx>>` + state snapshot,
`ResolverBackend` read-only. Three inline unit tests, all green.

R2-2 hit three coupled structural blockers and is deferred until the
wallet+proof+indexer follow-up lands.

## R2-2 blockers

### Gap 1 — `Contract<B>` cannot delegate to `generated::Contract<PS, W>` without a wallet/proof bridge

The 12 circuit methods on `generated::Contract<PS, W>` (in
`crates/midnight-did-runtime/src/contract/generated.rs` lines
1711–2845) take a `CircuitContext<PS>` and return
`CircuitResults<PS, ()>` — the in-circuit witness/state update. There
is no path from `CircuitResults` → `BuiltTx`; that conversion is the
wallet + proof-server + indexer work the R2 design spec explicitly
defers (`LiveBackend::submit_tx` is `todo!()` in R2-1's stub).

The R2-2 implementation template ("build typed input → call inner
method → `backend.submit_tx(...)`") is non-executable in the current
shape — there is no plumbing between an in-circuit result and an
on-chain transaction.

### Gap 2 — No concrete `DidWitnesses` type exists

`generated.rs:779` declares `trait Witnesses<PS>` with three methods
(`local_secret_key`, `current_timestamp`, `get_schnorr_reduction`).
No concrete impl ships in the runtime crate. R2-1 did not introduce a
`DidWitnesses` over `DidPrivateState`, so the `Contract<B>`
constructor signature R2-2 assumed (`Contract::<RecordingBackend>::new(
backend, addr, network, DidWitnesses::default())`) refers to a type
that doesn't exist yet.

### Gap 3 — No `Ledger → DidLedgerSnapshot` adapter

The api's `DidLedgerSnapshot` carries 18 fields (id_hex,
controller_public_key_hex, version, also_known_as, `BTreeMap<String,
LedgerVerificationMethod>`, …). The existing `RecordingContract` mock
just stores/returns a hand-built snapshot; there is no production-shape
`Ledger::<DefaultDB>::new(state).into()` adapter. `Ledger` accessors
in `generated.rs` return `Result<…, CompactError>` over
`ChargedState` — wiring the mapper is multi-day work in itself.

### Test-assertion-shape concern

The 44 `RecordedCall::X(payload)` matches across the 7 test files
extract full payloads (`LedgerVerificationMethod`, `(id, digest,
signature)`, ordered call sequences). The two R2-2 paths considered:

- **(a) Extend `RecordingBackend` to expose decoded `RecordedCall`-style
  events.** Requires `Contract<B>` to serialise a tagged enum into
  `BuiltTx.bytes` (because `Backend::submit_tx` only sees `BuiltTx`),
  and `RecordingBackend` to decode them. That's a parallel surface to
  the trait we're deleting — effectively re-implementing
  `RecordedCall` as a `BuiltTx` payload.
- **(b) Collapse to "a tx was submitted"** — guts ~40 of the 56 tests
  and loses the call-payload coverage `did_api_end_to_end.rs` depends
  on.

Neither path is a clean follow-on commit; both warrant their own ADR.

## Decision

**R2-1 stands as this cycle's R2 delivery.** R2-2 and R2-3 are deferred
until at least Gap 1 is resolved — i.e. when the wallet+proof+indexer
work lands (`LiveBackend::submit_tx` becomes implementable). At that
point R2-2 can land naturally: `BuiltTx` gets a real shape,
`RecordingBackend` records meaningful payloads, and the 56 tests
migrate against actual transaction bytes rather than synthetic
`RecordedCall` enum variants.

## Consequences

- The `DidContract` trait + `mock::RecordingContract` in
  `crates/midnight-did-api/src/contract.rs:250–440` remain the
  production seam for operation builders and integration tests until
  Gap 1 is closed.
- ADR 0002's trait-erasure rationale stays in effect for now. It is
  **partially superseded** by R2-1 (the Backend trait + Recording /
  Resolver impls land for future use) and **fully superseded** by R2-2
  once the wallet bridge lands.
- v0.3.0 release (the R1-4b/4c milestone) is **not blocked** by R2.
  The R1 work ships against the existing trait-erased shape.
- A small documentation refresh in
  `crates/midnight-did-runtime/src/backend.rs` should note that
  `Contract<B>` does NOT yet exist as a callable wrapper — `Backend`
  is currently a forward-compatible scaffold, not an active code path.

## Re-entry checklist (R2 follow-up, post-wallet-bridge)

Picking R2-2 back up requires (in order):

1. Land a concrete `DidWitnesses` impl in `midnight-did-runtime` over
   `DidPrivateState`. Should be a small commit once `DidPrivateState`
   shape stabilises.
2. Land the `Ledger::<DefaultDB>::new(state).into() →
   DidLedgerSnapshot` adapter, with a byte-parity test against the
   existing hand-built snapshots in
   `crates/midnight-did-api/tests/fixtures/*.json`.
3. Define `BuiltTx`'s real shape (likely a `LedgerTransaction` from
   `midnight-onchain-runtime` or wherever the wallet SDK lands).
   Wire `LiveBackend::submit_tx` against it. Until that's done,
   R2-2 is non-executable.
4. Add a `Contract<B: Backend>` wrapper struct in
   `crates/midnight-did-runtime/src/contract/mod.rs` whose 12 inherent
   methods compose `CircuitContext` setup → inner-circuit invocation
   → `BuiltTx` synthesis → `backend.submit_tx(...)`.
5. Execute R2-2 (operation builder + 56-test migration) as originally
   spec'd. Worker prompt + structural notes preserved at
   `/private/tmp/claude-501/.../tasks/a0a235c6ec29da25b.output`
   for context (or rewrite from the R2 design spec).
6. Execute R2-3 (delete trait + mock + ADR 0008 supersession +
   v0.4.0 bump).

## References

- R2 design spec: `doc/specs/2026-06-24-r2-contract-abstraction-design.md`
- R2-1 commit: [`5649d6a`](https://github.com/midnightntwrk/midnight-did-rs/commit/5649d6a)
- ADR 0002 (partially superseded): [`./0002-trait-erasure-for-contract.md`](./0002-trait-erasure-for-contract.md)
- Walker-gap closure log
  (`compact/docs/superpowers/notes/2026-06-24-walker-gap-status.md`)
  — Module-1 closure means `did.compact` now compiles end-to-end,
  removing the original gate on R2.
