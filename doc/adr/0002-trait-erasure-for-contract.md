<!--
This file is part of midnightntwrk/midnight-did-rs.
Copyright (C) 2026 Midnight Foundation
SPDX-License-Identifier: Apache-2.0
-->

# ADR 0002 — Trait erasure for contract calls (`DidContract`)

**Status:** Accepted
**Date:** 2026-06-03

## Context

The Midnight DID method's on-chain contract is `did.compact` — a 375-LOC
Compact source compiled by `compactc --rust` into
`crates/midnight-did/src/contract/generated.rs` (1,320 LOC). The
generated module exposes a `Contract` struct with one async method
per exported circuit (`rotate_controller_key`, `set_verification_method`,
`set_service`, `set_also_known_as`, `set_verification_method_relation`,
`deactivate`, and six more), plus a `Ledger` snapshot view, plus
witness hooks.

Two facts collide. First, the api crate needs to land soon — it owns
the operation builders, ledger mappers, resolution, and private-state
lifecycle that downstream consumers will use, and there is no reason
to make those wait. Second, the runtime crate `midnight-did` does not
currently build: the Nix-pinned `midnight-transient-crypto` calls
halo2 APIs (`ParamsKZG::unsafe_setup`, `from_parts`, `read_custom`)
that do not exist on the linked halo2 version. The fix is a flake-input
refresh that is out-of-scope for this iteration and gated by upstream
release timing.

If the api crate depended on the runtime crate directly, the api
crate could not build, ship, or be unit-tested until the runtime
build was unblocked. That coupling would burn weeks of calendar time
on a problem orthogonal to the api itself.

There is also a longer-term concern: the real `Contract` struct
internally talks to a Midnight provider (wallet SDK, proof server,
indexer client). Coupling the api crate to that stack drags
wallet-sdk-* deps (or their Rust equivalents) into every consumer,
including the resolver path that does not need them.

## Decision

The api crate abstracts the on-chain contract behind an async trait,
[`contract::DidContract`](../../crates/midnight-did-api/src/contract.rs).
The trait surfaces one async method per exported circuit, plus a
`ledger_snapshot()` read method that returns a
[`DidLedgerSnapshot`](../../crates/midnight-did-api/src/contract.rs)
— a plain-data view of the ledger fields the operation layer cares
about. Implementations of `DidContract`:

- **Production impl** (future, in the runtime crate): wraps
  `midnight_did::contract::Contract` + a Midnight provider, translates
  the trait methods into transaction-building + proof-server +
  broadcast calls.
- **`RecordingContract` mock** (today, in
  [`midnight_did_api::contract::mock`](../../crates/midnight-did-api/src/contract.rs)):
  in-memory ledger that records every mutation as a tagged enum
  variant. Tests assert on the recorded mutation tags + the final
  ledger snapshot.

The api crate's `Cargo.toml` lists `midnight-did-domain` as its only
intra-workspace dep; the runtime crate is **not** a dependency.

## Alternatives considered

**Api depends on the generated `Contract` struct directly.** Reduces
indirection. Rejected. Couples the api crate's build to the runtime
crate's halo2 saga; couples api-level tests to the codegen-gap stubs
in `generated.rs`; drags `compact-runtime` + halo2 into every
downstream that wants the operation builders. Negates the entire
shipping-incrementally story.

**Api owns the Midnight provider directly.** The provider abstraction
(public-data, proof, wallet) lives in api; the runtime crate becomes
a thin codegen target. Rejected. Pushes wallet-sdk-equivalent deps
into the resolver consumer's cone. The wallet shape is also a moving
target — the upstream Rust wallet SDK is not yet published — and
locking the api crate's surface to it would force a breaking change
when it lands.

**Generate the trait from the `.compact` source.** Have the codegen
emit two artefacts: the concrete `Contract` impl and a trait that
the api crate consumes. Rejected for now (could be revisited later).
The codegen-rust toolchain is already on the critical path for
`generated.rs`; adding a trait-emission feature is a parallel
investment that delivers no incremental consumer value today. The
hand-written trait is small (one method per exported circuit, 12
methods total) and stable enough that drift cost is acceptable.

**Use object-safe `dyn DidContract` everywhere.** We do, through the
operation builders' generic parameter. The trait is object-safe
because every method is `async fn` (via `#[async_trait]`) and returns
owned data. This is the property `RecordingContract` relies on.

## Consequences

**Positive:**
- The api crate ships with 92 tests passing while the runtime crate
  is upstream-blocked. Independent timelines.
- `RecordingContract` is a 200-LOC in-memory mock that the entire
  operation layer's test suite (controller rotation, VM CRUD, service
  CRUD, deactivation, end-to-end CRUD) runs against in
  sub-second wall-clock time. No proof-server, no wallet, no halo2
  initialisation.
- The resolver consumer (read-only path) can implement `DidContract`
  with the mutation methods as `unreachable!()` and only the
  `ledger_snapshot()` method live, pulling in the public-data provider
  and nothing else.
- The future production impl in the runtime crate is the third
  implementation; the trait surface is already validated by the mock.

**Negative:**
- The api crate cannot exercise a real proof. End-to-end byte-parity
  tests against actual `ContractState` bytes are deferred to the
  runtime crate. That gap is acceptable because the codegen-rust
  toolchain already ships an `tests-e2e-rust` pattern that fits the
  runtime crate cleanly.
- The trait drifts from the generated contract surface if `did.compact`
  changes. Mitigation: a `did.compact` change is rare and accompanied
  by a `generated.rs` regeneration; the trait update lands in the
  same commit.

**Locked in:** Operation builders depend on `dyn DidContract`, not
the concrete generated `Contract`. Swapping that out in a future
version is a breaking change for any direct consumer of the trait.

**Preserved flexibility:** Any number of `DidContract` impls can
coexist (production, mock, resolver-only, fault-injection); each is
its own crate or module.

## References

- [`crates/midnight-did-api/src/contract.rs`](../../crates/midnight-did-api/src/contract.rs)
  — `DidContract` trait + `RecordingContract` mock.
- [`crates/midnight-did-api/src/lib.rs`](../../crates/midnight-did-api/src/lib.rs)
  — module-level rationale block ("Why a trait, not the runtime crate?").
- [`crates/midnight-did-api/tests/`](../../crates/midnight-did-api/tests/)
  — 56 integration tests, all driving the api through `RecordingContract`.
- [`crates/midnight-did/src/contract/generated.rs`](../../crates/midnight-did/src/contract/generated.rs)
  — the concrete `Contract` struct the production impl will wrap.
- Related: [ADR 0001 — Async-only API](./0001-async-only-api.md),
  [ADR 0005 — Codegen-gap handling strategy](./0005-codegen-gap-handling.md).
