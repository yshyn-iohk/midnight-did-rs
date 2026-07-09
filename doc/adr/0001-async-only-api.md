<!--
This file is part of midnightntwrk/midnight-did-rs.
Copyright (C) 2026 Midnight Foundation
SPDX-License-Identifier: Apache-2.0
-->

# ADR 0001 — Async-only API

**Status:** Accepted
**Date:** 2026-06-03

## Context

The Midnight DID method has three I/O-bound surfaces that matter to
consumers:

1. **Contract calls** — submitting an operation (`rotate_controller_key`,
   `set_verification_method`, etc.) involves building a transaction,
   sending it to a proof server, waiting for the prover, then
   broadcasting and waiting for inclusion. Latencies range from
   hundreds of milliseconds to tens of seconds.
2. **Ledger reads** — the resolver path queries a public-data provider
   (indexer or node RPC) for the current `ContractState` of a DID's
   on-chain contract. Network-bound.
3. **Private-state persistence** — wallet impls back the
   `PrivateStateStore` with a filesystem or keychain handle. The mobile
   wallet must not block the UI thread on either of those.

The downstream consumers cover the full async spectrum: a Dioxus
mobile app on a single-threaded `tokio-current-thread` runtime; a CLI
on multi-threaded tokio; a wasm web app on `wasm-bindgen-futures`; and
UniFFI bindings exposing Rust async to Swift/Kotlin via `uniffi`'s
async support. Sync-only is not a viable shape — it would block the UI
on mobile and panic in wasm (no thread to block).

The TS reference is async-first (every `*-operations.ts` function
returns `Promise<...>`). Matching that shape simplifies the port and
preserves the consumer mental model.

## Decision

`midnight-did-api` exposes an **async-only** public surface. Every
method on the [`DidContract`](../../crates/midnight-did-api/src/contract.rs)
trait is `async`. Every operation builder (`controller_operations.rs`,
`verification_method_operations.rs`, `service_operations.rs`,
`document_operations.rs`, `did_operations.rs`, `resolution.rs`,
`private_state.rs`) is `async fn` (or `async fn`-equivalent via
`async-trait`). There is no parallel sync variant in the trait
surface, no `*_blocking()` twin, no `block_on(...)` convenience inside
the crate.

The traits in `midnight-did-domain` (`DidResolver`, `DidRegistrar`) use
`Pin<Box<dyn Future<Output = ...> + Send + '_>>` directly to stay
runtime-agnostic. The `DidContract` trait uses `#[async_trait]` for
ergonomics; future-Rust may let us drop the `Box` allocation, but the
public shape stays async.

If a sync-only consumer materialises, the answer is a separate
`midnight-did-blocking` facade crate that wraps every operation with
`tokio::runtime::Handle::block_on` (or equivalent). That facade is
explicitly not in scope until a real use case appears.

## Alternatives considered

**Dual sync + async surfaces.** Mirror every async fn with a sync
twin (`rotate_controller_key_blocking`). Rejected. Doubles the API
surface, doubles the test matrix, leaks runtime choice into the
crate, and inevitably one of the twins falls behind. The cost is
borne by every reader of the docs.

**`maybe-async` macro.** Use the `maybe-async` crate to flip
async/sync via cargo features. Rejected. The feature gates infect
every public signature; downstream tooling (rustdoc, rust-analyzer,
clippy) sees a moving target depending on enabled features. The
maintenance burden lands on us, not the consumer.

**Sync-only.** Expose a synchronous API and require consumers to wrap
calls in their own runtime. Rejected outright: blocks the UI on
mobile, panics in wasm, regresses against the TS reference shape, and
makes the UniFFI binding shape worse (UniFFI async callbacks
preserve cancellation semantics; sync ones do not).

**Runtime-agnostic with manual `Pin<Box<dyn Future>>` everywhere.**
The domain crate's resolver/registrar traits already use that shape.
Extending it to the api crate's trait was rejected on ergonomic
grounds: 12+ methods on `DidContract` with hand-rolled Pin/Box would
hurt readability and provide no concrete win over `#[async_trait]`.

## Consequences

**Positive:**
- Every public surface is a single shape. No twin to keep in sync.
- Mobile wallet, CLI, wasm, and UniFFI all work out of the box.
- The TS port stays straightforward — `Promise<T>` → `impl Future<Output = T>`.
- Tests that need to drive the api crate use `tokio-test::block_on`
  or `#[tokio::test]` — both are zero-friction.

**Negative:**
- Consumers must bring their own async runtime. The CLI example will
  bundle a tokio dependency.
- `async-trait` adds a per-call heap allocation (one `Box<dyn Future>`
  per trait method invocation). Acceptable: the operations are
  network-bound, the allocation is noise. Future Rust may let us drop
  the box via stabilised return-position-impl-trait-in-trait.
- A future sync-only consumer pays the `block_on` cost in a separate
  crate, not in our public API.

**Locked in:** The shape of the trait. Changing it from async to sync
in a future major version would break every consumer.

**Preserved flexibility:** Choice of concrete runtime (tokio,
async-std, smol, wasm-bindgen-futures); choice of single-threaded vs
multi-threaded; runtime-agnostic traits in `midnight-did-domain` can
still be implemented on top of any runtime.

## References

- [`crates/midnight-did-api/src/contract.rs`](../../crates/midnight-did-api/src/contract.rs)
  — `DidContract` async trait.
- [`crates/midnight-did-domain/src/did_resolver.rs`](../../crates/midnight-did-domain/src/did_resolver.rs)
  — runtime-agnostic resolver trait using `Pin<Box<dyn Future>>`.
- [`crates/midnight-did-domain/src/did_registrar.rs`](../../crates/midnight-did-domain/src/did_registrar.rs)
  — runtime-agnostic registrar trait.
- TS reference: `@midnight-ntwrk/midnight-did-api`, every
  `*-operations.ts` returns `Promise<...>`.
- Related: [ADR 0002 — Trait erasure for contract calls](./0002-trait-erasure-for-contract.md).
