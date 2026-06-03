<!--
This file is part of midnightntwrk/midnight-did-rs.
Copyright (C) 2026 Midnight Foundation
SPDX-License-Identifier: Apache-2.0
-->

# midnight-did-rs — Architecture Overview

Status: living document. Last updated 2026-06-03.

`midnight-did-rs` is the Rust port of the Midnight DID Method reference
implementation (TypeScript: `@midnight-ntwrk/midnight-did`). This document
captures the current shape of the workspace, where it is heading, and the
design patterns that knit the layers together. ADRs at
[`doc/adr/`](./adr/) record the load-bearing decisions; this overview
cross-links them from each section.

---

## 1. Project goals

**Native Rust implementation of the Midnight DID Method.** The reference
implementation ships as a TypeScript monorepo (`packages/contract`,
`domain`, `did`, `api`). The Rust port preserves the same layering — DID
Core data model, Midnight method profile, contract operations, codegen
output — but rebuilt to compile on every target Rust reaches: native
mobile, native desktop, server, wasm, and FFI-via-UniFFI.

**Byte-parity interop with the TypeScript reference.** The DID method is a
multi-runtime contract: a long-form `did:midnight:offchain:...` produced
in TypeScript must resolve to the same DID Document when decoded in Rust,
and a `ContractState` mutated by Rust circuits must produce the same
bytes the TypeScript runtime would emit for the same operation sequence.
The codegen-rust toolchain (`compactc --rust`) is the mechanism for the
on-chain side; the offchain MOD1 frame encoder + the W3C DID Core data
model are hand-ported and validated against TS reference fixtures.

**Multiple downstream consumers, one source of truth.** Four concrete
shapes are in scope:

1. **Mobile wallet** (Dioxus on iOS/Android) — needs operation builders
   + private-state persistence, no proof-server in the same process,
   driven through a wallet provider trait.
2. **DID resolver** (CLI / sidecar / web service) — needs only the
   read path: `did:midnight:...` URL → ledger snapshot → DID Document.
   Should compile without the wallet SDK or the proof prover.
3. **Web app / wasm** — needs the resolver path + offchain decoder
   running in-browser. No path to the Compact halo2 prover stack from
   the browser; the prover is delegated to a server.
4. **Swift / Kotlin via UniFFI** — host-language bindings re-exporting
   the same operation surface as the mobile wallet.

The layering is designed so each shape pulls in exactly the deps it
needs and no more.

---

## 2. Current crate layout

Two functional crates today plus one that is upstream-blocked. Concrete
LOC counts as of 2026-06-03:

| Crate | LOC | Tests | Status |
| --- | ---: | ---: | --- |
| `midnight-did-domain` | 3,100 | 34 | green |
| `midnight-did-api`    | 3,112 (+ tests) | 92 | green |
| `midnight-did`        | 1,320 (gen) + 19 | — | **blocked** |

```
                 ┌───────────────────────────────┐
                 │     midnight-did-domain       │
                 │ Pure-data W3C DID Core types  │
                 │ + MOD1 offchain encoder       │
                 │ + Midnight method types       │
                 │ Zero midnight-* deps          │
                 └──────────────┬────────────────┘
                                │
                                ▼
                 ┌───────────────────────────────┐
                 │      midnight-did-api         │
                 │ DidContract async trait       │
                 │ + operation builders          │
                 │ + ledger ↔ domain mappers     │
                 │ + RecordingContract mock      │
                 │ + Midnight method profile     │
                 └──────────────┬────────────────┘
                                │
                                ▼  (will depend, once it builds)
                 ┌───────────────────────────────┐
                 │       midnight-did            │
                 │  ✗ BLOCKED                    │
                 │ Generated contract from       │
                 │   compactc --rust did.compact │
                 │ + 11 stubbed circuit bodies   │
                 │ Pending: halo2 ParamsKZG API  │
                 │ alignment in the third_party  │
                 │ midnight-ledger pin           │
                 └───────────────────────────────┘
```

**`midnight-did-domain`** is pure-data Rust plus serde, hex, and the
ported MOD1 frame encoder. It has **no** dependency on any
`midnight-*` ledger crate, `compact-runtime`, or the wallet SDK. This
buys it three properties: it compiles to wasm without ceremony, it
compiles in milliseconds, and it is unaffected by the upstream halo2
skew that blocks the runtime crate.

**`midnight-did-api`** layers operation builders, ledger ↔ domain
mappers, resolution, and private-state lifecycle on top of the domain
crate. The on-chain contract is abstracted behind the
[`DidContract`](../crates/midnight-did-api/src/contract.rs) async
trait — see [ADR 0002](./adr/0002-trait-erasure-for-contract.md). This
lets the api crate ship today (with a `RecordingContract` mock for
tests) even though `midnight-did` cannot yet build.

**`midnight-did`** is the codegen target. The patched `compactc --rust`
emits `crates/midnight-did/src/contract/generated.rs` (1,320 LOC).
Eleven of the 23 exported circuit bodies hit codegen gaps and ship as
stubs in a working-copy `did.compact`; see
[ADR 0005](./adr/0005-codegen-gap-handling.md). The crate itself
is gated by an unrelated upstream issue: the Nix-pinned
`midnight-transient-crypto` calls `ParamsKZG::unsafe_setup` /
`from_parts` / `read_custom`, which do not exist on the halo2 version
linked against this snapshot. Refreshing that pin is the unblock.

---

## 3. Target crate layout

The TS port plan (see `Research — TS port plan.md` in Obsidian)
recommended a 4-crate split. We pulled the trigger on 2 crates first
to ship the bedrock layers fast. The remaining split — extracting
`midnight-did-method` from `midnight-did-api` and adding an umbrella —
is captured in [ADR 0003](./adr/0003-crate-split-2-to-4-with-umbrella.md).

```
            ┌─────────────────────────────────────┐
            │       midnight-did-domain           │
            │   W3C DID Core data model           │
            │   + crypto codecs + offchain MOD1   │
            └────────────────┬────────────────────┘
                             │
            ┌────────────────┴────────────────────┐
            │                                     │
            ▼                                     ▼
  ┌──────────────────────┐         ┌───────────────────────────┐
  │ midnight-did-method  │         │   midnight-did-runtime    │
  │ Midnight DID profile │         │  Codegen target           │
  │ + LedgerToDomain     │         │  (renamed from midnight-  │
  │ + network mapping    │         │   did) — compactc --rust  │
  │ Resolver-friendly    │         │   output + hand-written   │
  │  (no wallet deps)    │         │   shims for codegen gaps  │
  └─────────┬────────────┘         └─────────────┬─────────────┘
            │                                    │
            │                                    │
            ▼                                    ▼
  ┌─────────────────────────────────────────────────────────────┐
  │                    midnight-did-api                         │
  │   DidContract trait + operation builders + resolution       │
  │   + private-state lifecycle + mock contract                 │
  └────────────────────────────┬────────────────────────────────┘
                               │
                               ▼
            ┌──────────────────────────────────┐
            │         midnight-did             │
            │  Umbrella re-export crate        │
            │  (monolithic consumer entrypoint)│
            └──────────────────────────────────┘
```

**`midnight-did-method`** carries the Midnight-specific profile (single
controller, allowed curves, required contexts) and the canonical
`LedgerToDomain` mapper. Splitting it from `midnight-did-api`
isolates the resolver use case from the wallet-coupled operation
builders.

**`midnight-did-runtime`** is the current `midnight-did` crate renamed
on publication. The change avoids the umbrella crate (which we want to
name `midnight-did` so monolithic consumers can write `midnight_did::*`)
colliding with the runtime crate. Discussion in
[ADR 0003](./adr/0003-crate-split-2-to-4-with-umbrella.md).

**`midnight-did`** as an umbrella crate keeps the wallet consumer's
ergonomics intact — a single `[dependencies] midnight-did = "0.1"`
brings in the standard stack with sensible feature flags. Resolver and
wasm consumers can still depend on just `midnight-did-method` (or
`midnight-did-domain`) and skip the rest of the tree.

When the split happens is driven by two triggers: (a) `midnight-did-api`
gains a wallet/provider dependency that is not wanted in the
resolver path, or (b) we approach the first crates.io publication, at
which point consumer ergonomics dictates the umbrella shape.

---

## 4. Use-case → dep-cone mapping

Today (2-crate world):

| Use case | Crates pulled in | Notes |
| --- | --- | --- |
| Mobile wallet (Dioxus) | `midnight-did-domain` + `midnight-did-api` + `midnight-did` | `midnight-did` is the on-chain bind; pending unblock. |
| DID resolver | `midnight-did-domain` + `midnight-did-api` | Resolver uses `LedgerToDomain` (in api today) + ledger-reader; no wallet path. |
| Web / wasm | `midnight-did-domain` + `midnight-did-api` | Both crates build cleanly against `wasm32-unknown-unknown` and are gated by a CI job that runs on every PR. The resolver path is wasm-ready; the wallet path still needs `midnight-did` which is not wasm-targetable (halo2 deps). |
| UniFFI binding | `midnight-did-domain` + `midnight-did-api` + `midnight-did` + new `midnight-did-uniffi` | UniFFI wrapper is its own crate; reuses the rest. |

After the 4-crate split:

| Use case | Crates pulled in |
| --- | --- |
| Mobile wallet (Dioxus) | `midnight-did` (umbrella) → pulls all four |
| DID resolver | `midnight-did-domain` + `midnight-did-method` |
| Web / wasm | `midnight-did-domain` + `midnight-did-method` (resolver path), plus a thin wasm-bindgen wrapper crate |
| UniFFI binding | `midnight-did-uniffi` → depends on `midnight-did` (umbrella) + the `uniffi` runtime |

The 4-crate split is **orthogonal** across the four downstream use
cases: each cone picks exactly the crates it needs and nothing else.
The UniFFI binding needs the 5th wrapper crate so the umbrella stays
free of the `uniffi` proc-macro deps.

---

## 5. Key design patterns

### 5.1 Trait erasure for contract calls

The api crate does not depend on the runtime crate. Instead, it
abstracts the on-chain contract behind
[`contract::DidContract`](../crates/midnight-did-api/src/contract.rs)
— an `#[async_trait]` with methods like `rotate_controller_key`,
`set_verification_method`, `set_service`, `deactivate`, plus a
read-side `ledger_snapshot()`. The runtime crate provides the real
impl (wrapping `midnight-did::contract::Contract` + the wallet
provider); tests use `RecordingContract`, an in-memory mock that
records the mutation tags applied so assertions can verify
operation sequencing without a halo2 prover.

Rationale: [ADR 0002](./adr/0002-trait-erasure-for-contract.md).

### 5.2 Pure-data crate is dep-free of `midnight-*`

`midnight-did-domain` deliberately has zero `midnight-*`,
`compact-runtime`, or wallet dependencies. The MOD1 offchain frame
encoder needs to call into the upstream Compact value serializer
(used by `persistentHash` to compute the state hash); rather than
adding the dep, the encoder accepts a
[`CompactValueCodec`](../crates/midnight-did-domain/src/offchain.rs)
trait that the runtime crate plugs into. Tests use a `Vec<u8>`-based
golden-vector codec.

Consequences: the domain crate compiles to wasm with no ceremony,
compiles in seconds, and is immune to the upstream halo2 churn that
periodically breaks the runtime build.

### 5.3 Async-only API surface

Every method on `DidContract`, every `*_operations.rs` function, and
the registrar/resolver traits in `midnight-did-domain` are `async`.
No sync-twin variants. Mobile-UI consumers run on a tokio-current-thread
or async-std runtime; the wasm consumer brings its own; UniFFI uses
the native async support added in `uniffi` 0.28+. A future
`midnight-did-blocking` facade can wrap the async surface for
sync-only consumers if one materialises.

Rationale: [ADR 0001](./adr/0001-async-only-api.md).

### 5.4 MOD1 frame encoder abstracted via `CompactValueCodec`

Offchain DID encoding produces a binary frame: a 4-byte MOD1 magic
header + length-prefixed chunks for each DID Document field, followed
by a 32-byte blake2s state hash computed by hashing the
Compact-value-serialized form. The domain crate owns the framing
logic; the value serializer is injected through `CompactValueCodec`.
This is the only place the domain crate would have needed
`compact-runtime` as a dep, and the abstraction lets it stay free.

### 5.5 Private state behind a `PrivateStateStore` trait

The witness `localSecretKey()` (called by the generated contract) reads
through
[`PrivateStateStore`](../crates/midnight-did-api/src/private_state.rs).
`InMemoryPrivateStateStore` covers tests; the mobile wallet swaps in a
file-backed (later keychain-backed) impl; the CLI uses a fresh
in-memory store seeded from a hex seed.

Rationale: [ADR 0004](./adr/0004-private-state-as-trait.md).

---

## 6. Testing strategy

**Per-module unit tests.** 34 in `midnight-did-domain`, 36 in
`midnight-did-api`. Each ports the equivalent TS unit test where one
exists; otherwise covers a single function's contract.

**Integration tests at the api layer.** 56 integration tests across
six files in `crates/midnight-did-api/tests/` use `RecordingContract`
to drive end-to-end CRUD flows without instantiating the real
contract. The tests cover the full TS-port matrix for P0 acceptance
(ledger mappers, runtime-domain roundtrip, VM CRUD, controller
rotation, private-state, end-to-end DID API).

**TS reference fixtures.** Three JSON fixtures captured from the TS
`@midnight-ntwrk/midnight-did-api` test suite at deterministic points
(initial DID Document, post-controller-rotate, post-VM-add). The Rust
`tests/did_api_end_to_end.rs` runs the same operation sequence and
asserts `serde_json::Value`-equality against the fixture. This proves
the domain types serialize identically across runtimes.

**Future: byte-parity tests for ContractState.** The on-chain
byte-parity story is blocked until `midnight-did` builds. The pattern
to drop in is the `tests-e2e-rust/tests/codegen_regression.rs` shape
from the codegen-rust toolchain: capture TS `ContractState.serialize()`
bytes at each of the nine fixture points enumerated in the port plan;
assert byte-equality after running the same sequence through the Rust
contract. Eight of these fixtures will validate single-circuit
mutations; the ninth (`did_multi_op`) validates the operation-counter
+ versioning invariants across a 9-step sequence.

**CI.** GitHub Actions workflow runs `cargo fmt --check`, `cargo
clippy -D warnings`, and `cargo test` on the two functional crates.
`midnight-did` stays out of CI until the upstream pin is refreshed.

**Wasm build gate.** A third CI job builds `midnight-did-domain` +
`midnight-did-api` against `wasm32-unknown-unknown` on every PR. This
turns the architecture-doc claim "both crates are wasm-clean" from a
promise into an enforced invariant: the moment a transitive dep
regresses wasm support (e.g. someone pulls in a crate that uses
`std::process` or filesystem APIs), the build fails. Pure
`wasm32-unknown-unknown` target only — no `wasm-bindgen` / `web-sys`
ceremony. JS interop is deferred to a future browser-side wrapper
crate so the core stays runtime-agnostic.

---

## 7. Open questions and roadmap

Five open questions captured in the
[2026-06-03 session note](file:///Users/ysh/obsidian/midnight-mobile/Rust%20DID%20port/Session%202026-06-03%20%E2%80%94%20autonomous%20DID%20port.md):

1. **Crate split.** Stay at 2 crates or split now? Answer drafted in
   [ADR 0003](./adr/0003-crate-split-2-to-4-with-umbrella.md): wait for
   a concrete trigger (wallet dep in api, or first crates.io publish).
2. **Async runtime choice.** Tokio-mandatory or runtime-agnostic?
   Answer: stay runtime-agnostic. Async-trait + Pin/Box/Future on
   public surfaces; consumers pick the runtime. See
   [ADR 0001](./adr/0001-async-only-api.md).
3. **`midnight-did` crate name.** Today the runtime crate is named
   `midnight-did` matching the TS package name. On publication, rename
   to `midnight-did-runtime` and reserve `midnight-did` for the
   umbrella crate. See [ADR 0003](./adr/0003-crate-split-2-to-4-with-umbrella.md).
4. **Witness coupling.** `localSecretKey()` reads through the
   `PrivateStateStore` trait. Wallet SDK integration confirmation
   pending. See [ADR 0004](./adr/0004-private-state-as-trait.md).
5. **Codegen gap closure ordering.** Hand-write circuit shims or fix
   codegen first? Both, incrementally: stub bodies now, hand-shim where
   needed, close codegen gaps in priority order. See
   [ADR 0005](./adr/0005-codegen-gap-handling.md).

**Roadmap items** (no milestone vocabulary; ordered by sequencing):

- Refresh the `third_party/midnight-ledger` pin so `midnight-did`
  builds. Trigger: halo2 ParamsKZG API match.
- Hand-write the 11 circuit-body shims in
  `crates/midnight-did/src/contract/extensions.rs`.
- Capture TS `ContractState.serialize()` bytes for the nine
  byte-parity fixtures.
- Wire `midnight-did` into CI once the build is green.
- Extract `midnight-did-method` from `midnight-did-api` and rename
  `midnight-did` → `midnight-did-runtime`. Introduce the umbrella
  `midnight-did` re-export.
- Add the UniFFI wrapper crate `midnight-did-uniffi`.
- ~~Add a wasm-target build proof~~ — done (CI `wasm-build` job
  builds `midnight-did-domain` + `midnight-did-api` against
  `wasm32-unknown-unknown` on every PR). Future work: a thin
  browser-side wrapper crate (wasm-bindgen + serde-wasm-bindgen)
  exposing the resolver path to JS.
- Publish `midnight-did-domain` + `midnight-did-method` to crates.io
  ahead of the runtime crate (no halo2 dep, fast to ship).

---

## 8. Pointers

**Repo entry points:**
- [README](../README.md) — CI badge + project elevator pitch.
- [`Cargo.toml`](../Cargo.toml) — workspace manifest, ledger crate pins.
- [`crates/midnight-did-domain/src/lib.rs`](../crates/midnight-did-domain/src/lib.rs)
  — pure-data crate module index + rustdoc tour.
- [`crates/midnight-did-api/src/lib.rs`](../crates/midnight-did-api/src/lib.rs)
  — api crate module index + the `DidContract`-trait rationale block.
- [`crates/midnight-did/src/contract/generated.rs`](../crates/midnight-did/src/contract/generated.rs)
  — codegen-rust output; do not edit by hand.

**Architecture Decision Records:**
- [ADR 0001 — Async-only API](./adr/0001-async-only-api.md)
- [ADR 0002 — Trait erasure for contract calls](./adr/0002-trait-erasure-for-contract.md)
- [ADR 0003 — Crate split path: 2 → 4 + umbrella](./adr/0003-crate-split-2-to-4-with-umbrella.md)
- [ADR 0004 — Private state as a trait](./adr/0004-private-state-as-trait.md)
- [ADR 0005 — Codegen-gap handling strategy](./adr/0005-codegen-gap-handling.md)

**Upstream references:**
- TS source: `@midnight-ntwrk/midnight-did` (develop branch, mirrored
  at `third_party/midnight-did/`).
- Compact codegen: `compactc --rust` from the codegen-rust toolchain
  branch.
- W3C DID Core 1.0 spec: https://www.w3.org/TR/did-core/
