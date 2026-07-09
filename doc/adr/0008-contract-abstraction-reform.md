<!--
This file is part of midnightntwrk/midnight-did-rs.
Copyright (C) 2026 Midnight Foundation
SPDX-License-Identifier: Apache-2.0
-->

# ADR 0008 — R2 contract-abstraction reform

**Status:** Accepted — implemented in v0.4.0
**Date:** 2026-06-25 (R2-2 + R2-3 landed)
**Supersedes (fully):** [ADR 0002](./0002-trait-erasure-for-contract.md)
**Supersedes (partially):** [ADR 0004](./0004-private-state-as-trait.md)

## Context

R2 (per [`doc/specs/2026-06-24-r2-contract-abstraction-design.md`](../specs/2026-06-24-r2-contract-abstraction-design.md))
splits the 12-method async `DidContract` trait into three pieces:

- **R2-1:** introduce a `Backend` trait + `LiveBackend` /
  `RecordingBackend` / `ResolverBackend` impls in
  `midnight-did-runtime`.
- **R2-2:** migrate the operation builders + 56 integration tests
  to `Contract<B: Backend>` and the deterministic
  `RecordingBackend`.
- **R2-3:** delete `DidContract` + `mock::RecordingContract` +
  `RecordedCall`; tag v0.4.0.

R2-1 shipped in [`5649d6a`](https://github.com/yshyn-iohk/midnight-did-rs/commit/5649d6a)
against v0.3.0. R2-2 + R2-3 land here against v0.4.0.

The earlier deferral of R2-2 (preserved as the original draft of
this ADR in git history) identified three coupled structural gaps
that blocked the spec's original "build typed input → call inner
circuit method → submit_tx" template:

- **Gap 1** — no path from `generated::Contract<PS, W>::<circuit>`'s
  `CircuitResults<PS, ()>` to a `BuiltTx` without the
  wallet+proof-server+indexer bridge (a multi-week external
  workstream).
- **Gap 2** — no concrete `DidWitnesses` impl over
  `DidPrivateState`.
- **Gap 3** — no `Ledger::<DefaultDB>::new(state).into() →
  DidLedgerSnapshot` adapter wired against the production `Ledger`
  accessors in `contract/generated.rs`.

## Decision — Path 2: `DidContractCall` enum over `BuiltTx::bytes`

`Contract<B>` does NOT delegate to `generated::Contract<PS, W>` in
v0.4.0. Instead each of the 12 inherent methods on `Contract<B>`:

1. Builds a typed variant of a `DidContractCall` enum from its
   arguments (one variant per exported `did.compact` circuit;
   payload fields mirror the v0.3.0 `RecordedCall::X` shape 1:1 so
   the 56-test migration is mechanical).
2. Encodes the variant via `bincode` into a
   `BuiltTx { bytes: Vec<u8> }`.
3. Hands the `BuiltTx` to `self.backend.submit_tx(tx)`.

`RecordingBackend::submit_tx` decodes the bytes back to a
`DidContractCall` and appends it to an internal
`Mutex<Vec<DidContractCall>>`. Tests assert via
`contract.backend.recorded_calls()` instead of the deleted
`RecordingContract::calls()`.

`Backend` grows a third method, `read_snapshot(&self) ->
DidLedgerSnapshot`, that sidesteps Gap 3: `RecordingBackend` and
`ResolverBackend` return a snapshot configured at construction
time; `LiveBackend::read_snapshot` is `todo!("wire the Ledger →
DidLedgerSnapshot mapper")` until the bridge lands.

`LiveBackend::submit_tx` stays `todo!()` (unchanged from R2-1) —
Gap 1 is still real, but the public Rust API surface is now what
it will be when the wallet bridge lands. Test coverage rides on
`RecordingBackend`; production callers will swap in `LiveBackend`
once `BuiltTx`'s real shape (likely
`midnight_onchain_runtime::Transaction`) materialises.

### Why Path 2 (not the spec's original template)

The spec's "Contract delegates to generated::Contract" template
required Gap 1 closed first. Path 2 keeps the contract-abstraction
shape Rust consumers see (`Contract<B>` + `Backend` + recorded
typed events) decoupled from the wallet-bridge timeline. The cost
is a 14-variant `DidContractCall` enum carried inside
`BuiltTx::bytes`; the benefit is that R2-2/R2-3 ship now,
unblocking the v0.4.0 release and the downstream consumer-facing
work (CLI, UniFFI, WASM) that was waiting for the trait-erased
shape to retire.

## Consequences

- **`DidContract` trait + `mock::RecordingContract` mock +
  `RecordedCall` enum are gone** from
  `crates/midnight-did-api/src/contract.rs`. All operation
  builders (`did_operations`, `controller_operations`,
  `verification_method_operations`, `service_operations`,
  `document_operations`) now take `&Contract<B: Backend>` directly.
- **56 integration tests migrated 1:1** —
  `RecordingContract::new(...)` →
  `Contract::new(RecordingBackend::with_snapshot(...), ...)`,
  `RecordedCall::X(payload)` →
  `DidContractCall::X { payload fields }`.
- **`Backend` is now a 3-method trait**: `submit_tx`, `read_state`
  (`ChargedState` snapshot for the future wallet-side use),
  `read_snapshot` (`DidLedgerSnapshot` for the api surface).
- **ADR 0002 is fully superseded.** Trait-erasure rationale no
  longer applies — the runtime exposes a concrete `Contract<B>`
  generic over `B`, not a `dyn Trait`.
- **`LiveBackend` remains a stub.** The R2 design's "production
  path" is non-executable until the wallet+proof-server+indexer
  bridge lands. v0.4.0's R2 closure is a Rust-API-shape reform,
  not a production-deployment milestone.

## Future work

Gated on the wallet+proof+indexer bridge:

1. Define `BuiltTx`'s real shape (likely
   `midnight_onchain_runtime::Transaction` once the SDK lands).
   Replace `bytes: Vec<u8>` with the typed transaction.
2. Implement `LiveBackend::submit_tx` against that shape —
   `DidContractCall` becomes an *internal* representation that
   `Contract<B>` translates into
   `generated::Contract<PS, W>::<circuit>` invocations + proof
   synthesis + on-chain submission.
3. Implement `LiveBackend::read_snapshot` by wiring the
   `Ledger::<DefaultDB>::new(state).into() → DidLedgerSnapshot`
   adapter. The signature is already in place; only the body is
   `todo!()`.
4. Implement `DidWitnesses` over `DidPrivateState`. R2-2 did not
   need it (no in-circuit invocation yet); the future
   `LiveBackend::submit_tx` will.

Once those four steps land, `DidContractCall` may either remain (as
a stable test-recording surface) or be deleted (if
`generated::Contract`'s inputs become directly recordable). Either
choice is purely internal to the runtime crate at that point.

### Builder + decode validation gate

The audit-flagged ledger-shape types (`JubjubPointHex`,
`SchnorrJubjubSignature`, `SchnorrJubjubDigest`) are now closed on
both sides of `BuiltTx::bytes`:

- Encoding side ([`59ed1f5`](https://github.com/yshyn-iohk/midnight-did-rs/commit/59ed1f5)) —
  fields privatised, validating `::new` constructors. Callers cannot
  struct-literal a malformed value into a `DidContractCall` variant.
- Decoding side ([`b3fdb20`](https://github.com/yshyn-iohk/midnight-did-rs/commit/b3fdb20)) —
  `#[serde(try_from = "Repr")]` shims + hand-rolled `Deserialize` on
  the `#[serde(transparent)]` digest. An incoming envelope decoded
  via `RecordingBackend::submit_tx` (or any future `LiveBackend`
  consuming externally-produced bytes) cannot land a malformed inner
  value either. Wire format stays byte-identical for valid inputs.

The api-layer `SchnorrJubjubVerificationMethod` wrapper in
`midnight-did-api/src/ledger_mappers.rs` does not derive `Serialize`
/ `Deserialize` and is never on the envelope wire — its `::new`
constructor is sufficient.

## References

- R2 design spec:
  [`doc/specs/2026-06-24-r2-contract-abstraction-design.md`](../specs/2026-06-24-r2-contract-abstraction-design.md)
- R2-1 (Backend trait scaffold):
  [`5649d6a`](https://github.com/yshyn-iohk/midnight-did-rs/commit/5649d6a)
- R2-2.1 (DidContractCall enum + Contract<B>):
  [`2a54efb`](https://github.com/yshyn-iohk/midnight-did-rs/commit/2a54efb)
- R2-2.2 (operation-builder migration):
  [`a394756`](https://github.com/yshyn-iohk/midnight-did-rs/commit/a394756)
- R2-2.3 (56-test migration):
  [`26552f9`](https://github.com/yshyn-iohk/midnight-did-rs/commit/26552f9)
- R2-3 (trait + mock deletion):
  [`3746610`](https://github.com/yshyn-iohk/midnight-did-rs/commit/3746610)
- ADR 0002 (fully superseded):
  [`./0002-trait-erasure-for-contract.md`](./0002-trait-erasure-for-contract.md)
- ADR 0004 (partially superseded — private state still threads
  through `DidWitnesses` once that materialises):
  [`./0004-private-state-as-trait.md`](./0004-private-state-as-trait.md)
