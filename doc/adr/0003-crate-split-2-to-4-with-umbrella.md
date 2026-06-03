<!--
This file is part of midnightntwrk/midnight-did-rs.
Copyright (C) 2026 Midnight Foundation
SPDX-License-Identifier: Apache-2.0
-->

# ADR 0003 — Crate split path: 2 → 4 + umbrella

**Status:** Implemented — 5-crate shape (4 stack + umbrella) landed on
2026-06-04.
**Date:** 2026-06-03 (Accepted); 2026-06-04 (Implemented).

## Context

The TS reference monorepo splits the DID method across four packages:
`@midnight-ntwrk/midnight-did-contract` (the `.compact` source + the
zkir + thin TS bindings), `midnight-did-domain` (pure-data W3C DID
Core types + offchain encoder), `midnight-did` aka the "did" package
(Midnight method profile + `LedgerToDomain` mapper + resolver), and
`midnight-did-api` (operation builders + private state + wallet glue).

The Rust port's first iteration shipped only two of those crates:
[`midnight-did-domain`](../../crates/midnight-did-domain/) (3,100 LOC)
and [`midnight-did-api`](../../crates/midnight-did-api/) (3,112 LOC).
The Midnight method profile and the `LedgerToDomain` mapper currently
live inside `midnight-did-api` (see
[`midnight_did_api::resolution`](../../crates/midnight-did-api/src/resolution.rs)
and [`midnight_did_api::network_mapping`](../../crates/midnight-did-api/src/network_mapping.rs)).
The codegen target is named `midnight-did` matching the TS package
name.

This shape has a known cost. The resolver use case (read-only:
`did:midnight:...` → DID Document) only needs the method profile +
ledger-to-domain mapper + a public-data provider. Today it has to
pull in the entire api crate, including operation builders, private
state, and (when wired) the wallet provider. For the web/wasm
consumer that cost is even higher — the wallet path has no business
in a browser bundle.

Two triggers are likely to flip the cost from acceptable to painful:

1. **`midnight-did-api` adds a wallet/provider dependency.** Once the
   production `DidContract` impl lands, the api crate (or a sibling
   module) will need a `MidnightProvider` trait or concrete client.
   That trait drags `compact-runtime` + Midnight provider stack into
   any consumer.
2. **First crates.io publication.** Each published crate is a public
   contract; consumer ergonomics + the dep cone become permanent
   commitments.

There is also a naming concern: the codegen target is named
`midnight-did` to match the TS package, but that name is the obvious
choice for an **umbrella** re-export crate that the mobile wallet
would depend on directly. Two artefacts cannot share a name on
crates.io.

## Decision

Move to the following target shape, triggered by the first of either
(a) the api crate gaining a wallet/provider dep or (b) the first
crates.io publication.

```
midnight-did-domain     Pure W3C DID Core + crypto codecs + MOD1 offchain encoder.
midnight-did-method     Midnight method profile + LedgerToDomain + network mapping.
midnight-did-api        DidContract trait + operation builders + resolution + private state.
midnight-did-runtime    Codegen target (rename of current `midnight-did`).
midnight-did            Umbrella re-export crate — `pub use midnight_did_{domain,method,api,runtime}::*;`.
```

- `midnight-did-method` is split out of the current api crate. The
  `LedgerToDomain` mapper, the Midnight method profile constants
  (allowed curves, required contexts), and the network mapping
  (`network_mapping.rs`) move there.
- `midnight-did-api` keeps the operation builders, the `DidContract`
  trait, the private-state lifecycle, and the high-level CRUD
  aggregations. Its only intra-workspace deps become
  `midnight-did-domain` and `midnight-did-method`.
- `midnight-did` (the current codegen-target crate) is renamed
  `midnight-did-runtime`. Renaming on first publication is cheap; the
  current name is reserved for the umbrella.
- `midnight-did` becomes the umbrella crate. Default features
  re-export the full stack; `--no-default-features` lets resolver-only
  consumers strip out the wallet path.

The split does not happen yet. The 2-crate shape is fine today
because no wallet code lives in `midnight-did-api` and no publication
is imminent. Pre-emptive splitting would burn time without delivering
consumer value.

## Alternatives considered

**Stay at 2 crates indefinitely.** Domain + a single "everything else"
crate. Rejected. Forces the resolver consumer to pull in the wallet
path, blocks the wasm/web case, and concentrates a wallet-coupled
change in the same crate as the W3C-spec-mapping code.

**Split now (before publication or wallet dep).** Pre-emptively
restructure into 4 crates. Rejected on timing grounds. The split is
mechanical work; doing it before a concrete trigger means churning
import paths across 90+ test files for no current consumer benefit.
Defer until the trigger fires.

**4 crates without an umbrella.** Ship `midnight-did-domain`,
`midnight-did-method`, `midnight-did-api`, `midnight-did-runtime` and
let monolithic consumers list all four. Rejected. The mobile wallet
ergonomics suffer (four version pins to keep in sync, four sets of
release notes to read), and a future feature-flag-driven
restructuring breaks every Cargo.toml in the ecosystem.

**5 crates including a separate `midnight-did-contract` for the
`.compact` source.** Rejected for now. The `.compact` source is
tooling input, not a runtime artefact; bundling it inside
`midnight-did-runtime/contract/` matches what the codegen does today
and avoids a publish-time circular cargo-vs-source-of-truth problem.

**Rename `midnight-did` to `midnight-did-monolith` and avoid the
umbrella name collision.** Rejected on ergonomics — the canonical
short name should belong to the canonical consumer entry point. The
umbrella is the canonical entry point.

## Consequences

**Positive:**
- Resolver consumer pulls in `midnight-did-domain` +
  `midnight-did-method` and skips the wallet path entirely.
- Web/wasm consumer can take the same two crates plus a thin
  wasm-bindgen wrapper without touching the runtime crate or its
  halo2 deps.
- UniFFI binding crate (future) consumes the umbrella, gets the full
  stack, and exposes it to Swift/Kotlin.
- Each layer's release cadence is independent. `midnight-did-domain`
  can publish first (it has no upstream halo2 dep), unblocking
  ecosystem consumers ahead of the runtime crate's first publish.

**Negative:**
- Splitting `midnight-did-method` out of the api crate requires
  migrating ~600 LOC and updating imports across the 56 integration
  tests. One-time cost.
- Renaming `midnight-did` → `midnight-did-runtime` is a breaking
  change for any consumer that pinned the pre-split crate name.
  Mitigation: the runtime crate has never been published; rename
  before first publication.
- Five crates means five Cargo.tomls to keep version-synced. Tooling
  (`cargo release` or `release-plz`) makes this routine, but it is
  noise compared to two crates.

**Locked in (once the split happens):** Public module paths in the
new method crate. Any consumer that uses
`midnight_did_api::resolution::LedgerToDomain` today will need to
migrate to `midnight_did_method::LedgerToDomain` post-split. The
umbrella crate masks this for consumers that depend on the umbrella.

**Preserved flexibility:** Either of the two triggers (wallet dep or
publication) can fire independently; we re-evaluate at the next
trigger.

## References

- [`crates/midnight-did-api/src/resolution.rs`](../../crates/midnight-did-api/src/resolution.rs)
  — current home of the `LedgerToDomain` mapper.
- [`crates/midnight-did-api/src/network_mapping.rs`](../../crates/midnight-did-api/src/network_mapping.rs)
  — current home of the runtime ↔ domain network map.
- [`Cargo.toml`](../../Cargo.toml) — workspace manifest, current
  3-member shape.
- Obsidian: `Research — TS port plan.md` § "Recommended Rust crate
  structure" — original 4-crate proposal.
- Related: [ADR 0001 — Async-only API](./0001-async-only-api.md),
  [ADR 0002 — Trait erasure for contract calls](./0002-trait-erasure-for-contract.md).

## Implementation note (2026-06-04)

The split landed in two commits on `cycle-1-bootstrap`. Final layout:

- `crates/midnight-did-domain/` — pure W3C DID Core (no Midnight-method types).
- `crates/midnight-did-method/` — `did:midnight:*` parsing
  (`midnight_did.rs`), runtime ↔ domain network map (`network_mapping.rs`),
  and MOD1 offchain frame codec (`offchain.rs`).
- `crates/midnight-did-api/` — `DidContract` async trait, operation
  builders, resolution, mocks, private state, error. The Ledger wire
  types (`LedgerVerificationMethod`, `JubjubPointHex`, etc.) and the
  trait-coupled helpers (`subject.rs`, `ledger_mappers.rs`) stayed in
  api because they depend on the trait itself. A future ADR may pull
  those down to method once the trait is split into a method-profile
  half and an operation-layer half; the current split is conservative
  and preserves 100 % test-count parity (144 → 144).
- `crates/midnight-did-runtime/` — renamed from `crates/midnight-did/`.
  Still blocked on upstream halo2 `ParamsKZG` API skew; tracked under
  DID-P2-2.
- `crates/midnight-did/` — new umbrella crate. Re-exports the four
  sibling crates and a small set of convenience flat exports. The
  optional `runtime` feature pulls `midnight-did-runtime` in.

Deviation from the target shape: `offchain.rs` moved to method instead
of staying in domain because it embeds `did:midnight:` strings — it is
method-specific. The spec target assumed a clean domain/method split
but `offchain` straddles both; honouring the dependency direction
(method depends on domain, never the reverse) decided the move.
