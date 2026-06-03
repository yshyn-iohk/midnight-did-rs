<!--
This file is part of midnightntwrk/midnight-did-rs.
Copyright (C) 2026 Midnight Foundation
SPDX-License-Identifier: Apache-2.0
-->

# ADR 0004 — Private state as a trait

**Status:** Accepted
**Date:** 2026-06-03

## Context

`did.compact` declares one private-state-bound witness:
`localSecretKey() -> Bytes<32>` (plus `currentTimestamp` and
`getSchnorrReduction` which are not strictly private-state-bound).
The TS reference threads private state through a `LevelDB`-backed
`PrivateStateStore` provider configured at wallet boot:

```
configureProviders({ privateStateStore: levelDbStore, ... })
  → DIDContract.witnesses.localSecretKey(ctx) → ctx.privateState.secretKey
```

The same witness needs to be served by three concrete consumer
shapes, all with different storage models:

1. **Mobile wallet (Dioxus)** — secret-key persistence in the OS
   keychain (iOS Keychain / Android Keystore) via a platform-specific
   binding. The witness must read through whatever the wallet's
   private-state plumbing already provides; we should not invent a
   parallel store.
2. **CLI / sidecar** — secret key materialised in-memory from a hex
   seed at startup. Lifetime is the process lifetime.
3. **Tests** — `RecordingContract` runs without instantiating any
   wallet; the witness reads from an in-memory map populated by the
   test setup.

A fourth, more subtle, shape: controller-key rotation requires a
**pending** controller slot. During `rotateControllerKey`, the new
secret key is derived, stashed as "pending", the circuit is called
with the new public key, and only on successful finalisation is the
pending slot promoted to "current". This shape needs the same store
used for the witness — it cannot be decoupled.

A "just dump the secret key in a `static` somewhere" design works for
tests and breaks for everything else.

## Decision

[`midnight_did_api::private_state::PrivateStateStore`](../../crates/midnight-did-api/src/private_state.rs)
is a trait. The witness path reads `localSecretKey()` through this
trait; the controller-rotation path uses the same trait's pending
slot for the rotation lifecycle. The trait surface, in essence:

```rust
#[async_trait]
pub trait PrivateStateStore {
    async fn require_current(&self) -> Result<SecretKey, ApiError>;
    async fn save_current(&self, sk: SecretKey) -> Result<(), ApiError>;
    async fn stash_pending(&self, sk: SecretKey) -> Result<(), ApiError>;
    async fn require_pending(&self) -> Result<SecretKey, ApiError>;
    async fn promote_pending_to_current(&self) -> Result<(), ApiError>;
    async fn recover_pending(&self) -> Result<(), ApiError>;
    async fn clear(&self) -> Result<(), ApiError>;
}
```

The crate ships
[`InMemoryPrivateStateStore`](../../crates/midnight-did-api/src/private_state.rs)
as the test impl. The mobile wallet provides a keychain-backed impl
in the wallet crate. The CLI provides a file-backed (or in-memory
seeded) impl.

The witness binding (the bridge between the generated contract's
`witness localSecretKey` call and this trait) is owned by the runtime
crate (`midnight-did`) — once the runtime crate's witness shim is
hand-written, it accepts a `PrivateStateStore` handle and dispatches.

## Alternatives considered

**Hardcode private state in the runtime crate.** Pass a `SecretKey`
into the `Contract` constructor and have the witness call read from a
runtime-crate field. Rejected. Couples test isolation (every test
would need a custom `Contract` instance with a different baked-in
key), prevents the wallet from swapping in a keychain-backed store
without forking the runtime crate, and forces the controller-key
rotation flow (which needs pending + current slots) into the runtime
crate's surface.

**Pass private state inline to every circuit call.** Each operation
takes the secret key as an explicit parameter. Rejected. Verbose
(every method on `DidContract` gains a `SecretKey` arg), leaks the
witness mechanism to every caller, and forces the controller-rotation
flow's pending-slot mechanics into the public API. The TS reference
hides this — matching it keeps the port faithful and the user-facing
surface clean.

**`PrivateStateStore` as a struct with closure callbacks.** Replace
the trait with a struct that holds `Arc<dyn Fn(...) -> Future>`
closures for each operation. Rejected. The trait is more idiomatic
Rust, plays better with `async-trait`, and avoids the closure
type-elaboration cost.

**`thread_local!` global private state.** Rejected. Breaks
wasm-single-threaded, breaks mobile-multi-runtime, breaks tests that
need parallel isolation.

**Bundle pending-slot management into the controller-rotation
operation instead of the store.** Have the operation hold its own
`Mutex<Option<SecretKey>>` for the pending slot. Rejected. The
pending slot needs to survive a process restart (a mobile-wallet
crash mid-rotation must be recoverable); the store is the natural
home for that durability. The TS reference's `private-state.ts`
agrees.

## Consequences

**Positive:**
- Three concrete impls coexist (in-memory test store, file/keychain
  wallet impl, CLI seeded impl) with no friction.
- `RecordingContract`-driven tests use `InMemoryPrivateStateStore`
  and validate the full controller-rotation lifecycle (including
  pending-recovery) without any wallet code in scope.
- The mobile wallet integrator implements one trait and is done;
  there is no fork of the runtime crate.
- The witness mechanism stays implicit at the API surface — callers
  of `rotate_controller_key` do not see a `SecretKey` parameter, just
  like the TS reference.

**Negative:**
- The trait surface is 7 methods. Each impl has 7 methods to write,
  test, and document. Acceptable: the in-memory impl is ~80 LOC, and
  the keychain/file impls are mostly platform-binding glue.
- Trait-method async-trait box allocations on every call. Same noise
  as ADR 0001's accepted cost; private-state calls are not in a hot
  loop.
- The trait can drift from the actual storage requirements. We have
  one ported test suite (`private-state.test.ts` → 13 Rust tests)
  exercising every method, which catches drift.

**Locked in:** The trait method signatures. Consumers depend on the
exact method shape; expansion (adding a method) is non-breaking
because the trait has a default-impl path; renaming or removing a
method is breaking.

**Preserved flexibility:** Storage backend (in-memory, file, keychain,
encrypted-on-disk, hardware-key); concurrency model (mutex-per-store
or RWLock-per-store, mocked-or-real timestamps); cross-process
sharing (the store can be a thin async wrapper over an IPC client).

## References

- [`crates/midnight-did-api/src/private_state.rs`](../../crates/midnight-did-api/src/private_state.rs)
  — trait + `InMemoryPrivateStateStore` (~385 LOC).
- [`crates/midnight-did-api/src/controller_operations.rs`](../../crates/midnight-did-api/src/controller_operations.rs)
  — `rotate_controller_key_with_derivation` consumes the store for
  pending-slot mechanics.
- [`crates/midnight-did-api/tests/private_state.rs`](../../crates/midnight-did-api/tests/)
  — 13 Rust tests ported from TS `private-state.test.ts`.
- TS reference: `@midnight-ntwrk/midnight-did-api/src/private-state.ts`.
- Related: [ADR 0002 — Trait erasure for contract calls](./0002-trait-erasure-for-contract.md),
  [ADR 0001 — Async-only API](./0001-async-only-api.md).
