<!--
This file is part of midnightntwrk/midnight-did-rs.
Copyright (C) 2026 Midnight Foundation
SPDX-License-Identifier: Apache-2.0
-->

# midnight-did-rs — Architecture Overview

Status: living document. Last updated 2026-06-26.

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

## 2. Current crate layout (v0.4.0+)

Five crates, all green — four functional layers plus an umbrella. The
initial 4-crate split landed on 2026-06-04 per
[ADR 0003](./adr/0003-crate-split-2-to-4-with-umbrella.md); the
runtime crate joined the all-green column once the upstream halo2 +
codegen blocks closed (v0.3.0 compact pin bump to `960fc26`).

| Crate | Role | Status |
| --- | --- | --- |
| `midnight-did-domain`  | Pure W3C DID Core types + crypto codecs | green |
| `midnight-did-method`  | Midnight method profile (`did:midnight:*`, MOD1 offchain, network map) | green |
| `midnight-did-api`     | Operation builders + ledger mappers + resolution | green |
| `midnight-did-runtime` | Codegen target + `Contract<B: Backend>` + `DidContractCall` enum | green |
| `midnight-did`         | Umbrella re-export crate (for monolithic consumers) | green |

```
                 ┌───────────────────────────────┐
                 │     midnight-did-domain       │
                 │ Pure-data W3C DID Core types  │
                 │ + crypto codecs + URI helpers │
                 │ Zero midnight-* deps          │
                 └──────────────┬────────────────┘
                                │
                                ▼
                 ┌───────────────────────────────┐
                 │     midnight-did-method       │
                 │ did:midnight:* parsing        │
                 │ + MOD1 offchain frame codec   │
                 │ + runtime ↔ domain net map    │
                 └──────────────┬────────────────┘
                                │
                                ▼
                 ┌───────────────────────────────┐
                 │      midnight-did-api         │
                 │ Operation builders            │
                 │ + ledger mappers              │
                 │ + resolution + subject        │
                 └──────────────┬────────────────┘
                                │
                                ▼
                 ┌───────────────────────────────┐
                 │     midnight-did-runtime      │
                 │ compactc --rust output        │
                 │ + Contract<B: Backend>        │
                 │ + DidContractCall enum        │
                 │ + LiveBackend (stub)          │
                 │ + RecordingBackend            │
                 │ + ResolverBackend             │
                 └──────────────┬────────────────┘
                                │
                                ▼
                 ┌───────────────────────────────┐
                 │        midnight-did           │
                 │ Umbrella — re-exports the 4   │
                 │ siblings under stable names.  │
                 └───────────────────────────────┘
```

The dep direction is strict (domain ← method ← api ← runtime ←
umbrella). The api crate took on a hard dep on `midnight-did-runtime`
in v0.4.0 when the contract-abstraction shape moved into the runtime
crate where the wire types live — see §4.6 below.

**`midnight-did-domain`** is pure-data Rust plus serde, hex, and the
ported MOD1 frame encoder. It has **no** dependency on any
`midnight-*` ledger crate, `compact-runtime`, or the wallet SDK. This
buys it three properties: it compiles to wasm without ceremony, it
compiles in milliseconds, and it is unaffected by the upstream halo2
skew that historically blocked the runtime crate.

**`midnight-did-api`** layers operation builders, ledger ↔ domain
mappers, resolution, and `SchnorrJubjubVerificationMethod` wrapping on
top of the domain + method crates. The 12 operation builders take
`&Contract<B: Backend>` directly (v0.4.0); tests drive them through
`Contract<RecordingBackend>` and assert on the recorded
`DidContractCall` envelope.

**`midnight-did-runtime`** is the codegen target. The patched
`compactc --rust` emits
`crates/midnight-did-runtime/src/contract/generated.rs`, which now
compiles `cargo check`-clean for the full `did.compact` source (v0.3.0
`960fc26` compact pin closed the last codegen + halo2 gaps). On top of
the generated module the crate ships:

- `Contract<B: Backend>` — concrete wrapper, 12 inherent async methods
  (one per exported `did.compact` circuit). Each method builds a typed
  `DidContractCall` variant, encodes via `bincode`, and submits via
  `B::submit_tx`. See [ADR 0008](./adr/0008-contract-abstraction-reform.md).
- `Backend` trait — 3 methods (`submit_tx`, `read_state`,
  `read_snapshot`). Implementations: `LiveBackend` (production, both
  call paths still `todo!()` until the wallet+proof+indexer bridge
  lands), `RecordingBackend` (Mutex-guarded in-memory recorder used by
  tests), `ResolverBackend` (read-only).
- `DidContractCall` — 14-variant tagged enum carrying typed circuit
  invocations through `BuiltTx::bytes`. Payload shapes mirror the
  arguments of each exported `did.compact` circuit 1:1.

**`midnight-did`** umbrella crate re-exports the four siblings under
stable names so monolithic consumers (mobile wallet, CLI) can write
`midnight_did::*`. Resolver / wasm consumers continue to depend on
just `midnight-did-method` (or `midnight-did-domain`) and skip the
rest of the tree.

---

## 3. Use-case → dep-cone mapping

Current state (5 crates, all green):

| Use case | Crates pulled in | Notes |
| --- | --- | --- |
| Mobile wallet (Dioxus) | `midnight-did` (umbrella) → re-exports all four | Single dependency, stable namespace. |
| DID resolver | `midnight-did-domain` + `midnight-did-method` | Skips api + runtime entirely. |
| Web / wasm | `midnight-did-domain` + `midnight-did-method` (resolver path) or + `midnight-did-api` (write side) | The two layered crates are gated on `wasm32-unknown-unknown` in CI. |
| Write-side CLI / library | `midnight-did-api` (transitively pulls domain + method + runtime) | What the reference CLI does today. |
| UniFFI binding | `midnight-did-uniffi` → depends on `midnight-did-api` + `uniffi` runtime | UniFFI wrapper deliberately targets the api layer, not the umbrella, to keep the FFI surface focused on operation builders. |

The 5-crate split is **orthogonal** across these use cases: each cone
picks exactly the crates it needs and nothing else. The UniFFI binding
keeps `midnight-did-uniffi` as its own crate so the umbrella stays free
of the `uniffi` proc-macro deps.

---

## 4. Key design patterns

### 4.1 `Contract<B: Backend>` over `BuiltTx::bytes` (v0.4.0)

The api crate's operation builders take `&Contract<B: Backend>`
directly. `Contract<B>` is a concrete wrapper struct living in
`midnight-did-runtime` with one async inherent method per exported
`did.compact` circuit (12 total). Each method:

1. Builds a typed `DidContractCall` variant from its arguments.
2. Encodes the variant via `bincode` into `BuiltTx { bytes: Vec<u8> }`.
3. Hands the `BuiltTx` to `self.backend.submit_tx(tx)`.

`Backend` is a 3-method async trait: `submit_tx`, `read_state`
(low-level `ChargedState`), and `read_snapshot` (high-level
`DidLedgerSnapshot` for the api surface). Three implementations
ship:

- **`LiveBackend`** — production target. Both `submit_tx` and
  `read_snapshot` are `todo!()` until the wallet+proof-server+indexer
  bridge lands. The public Rust API shape is final; only the body is
  pending.
- **`RecordingBackend`** — Mutex-guarded in-memory recorder used by
  every integration test. `submit_tx` decodes the bytes back into a
  `DidContractCall` and appends to an internal `Vec<DidContractCall>`
  accessible via `recorded_calls()`. `read_snapshot` returns a
  fixture-configured snapshot.
- **`ResolverBackend`** — read-only. `submit_tx` rejects.

This shape replaces the trait-erased `&dyn DidContract` seam from
v0.3.0 (ADR 0002). The previous mock (`RecordingContract` in the api
crate) is gone — tests construct
`Contract::new(RecordingBackend::with_snapshot(snapshot), addr, network)`
directly.

**Path 2 rationale.** The R2 design spec
([`doc/specs/2026-06-24-r2-contract-abstraction-design.md`](./specs/2026-06-24-r2-contract-abstraction-design.md))
originally called for `Contract<B>` to delegate to
`generated::Contract<PS, W>::<circuit>`. That delegation requires the
wallet+proof+indexer bridge to be in place. Path 2 sidesteps the
dependency: `DidContractCall` carries the typed call surface through
`BuiltTx::bytes`, so the Rust-facing API shape is final today, test
coverage rides on `RecordingBackend`, and production callers will
swap in `LiveBackend` once the bridge ships. See
[ADR 0008](./adr/0008-contract-abstraction-reform.md).

### 4.1a Builder + decode validation gate (v0.4.1)

The architecture audit (2026-06-26) flagged three at-risk ledger-shape
types — `JubjubPointHex`, `SchnorrJubjubSignature`,
`SchnorrJubjubDigest` — whose public `String` / `[String; 4]` fields
accepted arbitrary garbage in test fixtures and could land malformed
values inside a `DidContractCall` variant. v0.4.1 closes the bypass
on **both sides** of `BuiltTx::bytes`:

- **Encode side** — fields privatised, validating `::new(NewX)`
  constructors. Struct-literal construction of a malformed value is
  no longer possible.
- **Decode side** — `#[serde(try_from = "Repr")]` shims on the two
  struct types plus a hand-rolled `Deserialize` on the
  `#[serde(transparent)]` digest. An incoming envelope decoded via
  `RecordingBackend::submit_tx` (or any future `LiveBackend`
  consuming externally-produced bytes) re-runs `::new` and rejects
  malformed inner values.

Wire format stays byte-identical for valid inputs. Coverage: 19
encode-side tests + 15 decode-side tests in
`crates/midnight-did-api/tests/{builder,decode}_validation.rs`. See
ADR 0008 § "Builder + decode validation gate".

### 4.1b Type-safety sweep (v0.2.0 + v0.3.0)

The R1 type-safety sweep (ADR 0007) progressively eliminated
"deserialize then forget validate" footguns from the domain layer:

- **v0.2.0** — fallible `::new(NewX)` constructors on
  `VerificationMethod`, `Service`, `PublicKeyJwk`. Validating
  `Deserialize` for `PublicKeyJwk` via `#[serde(try_from)]`. New
  `DidKeyId` / `FragmentId` / `ServiceId` newtypes in
  `midnight_did_domain::ids`. Re-export of upstream
  `ContractAddress` / `HashOutput` (drop the `pub String` shadow
  newtypes). Domain-grouped error enums.
- **v0.3.0** — closed steps 4b + 4c (commits `0b875a8` + `65ed7f6`):
  privatized inner fields on `VerificationMethod` / `Service` /
  `PublicKeyJwk` / `DidString` / `DidUrl` / `RelativeUrl`, retired
  the `create_verification_method` / `create_service` free functions,
  migrated ~17 remaining struct-literal sites to `::new(NewX)?`.
  After v0.3.0 the only way to construct these types is the
  validating constructor or the validating `Deserialize` path.

### 4.2 Pure-data crate is dep-free of `midnight-*`

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

### 4.3 Async-only API surface

Every `Backend` method, every `*_operations.rs` function, every
inherent method on `Contract<B>`, and the registrar/resolver traits
in `midnight-did-domain` are `async`. No sync-twin variants.
Mobile-UI consumers run on a tokio-current-thread or async-std
runtime; the wasm consumer brings its own; UniFFI uses the native
async support added in `uniffi` 0.28+. A future
`midnight-did-blocking` facade can wrap the async surface for
sync-only consumers if one materialises.

Rationale: [ADR 0001](./adr/0001-async-only-api.md).

### 4.4 MOD1 frame encoder abstracted via `CompactValueCodec`

Offchain DID encoding produces a binary frame: a 4-byte MOD1 magic
header + length-prefixed chunks for each DID Document field, followed
by a 32-byte blake2s state hash computed by hashing the
Compact-value-serialized form. The
[`CompactValueCodec`](../crates/midnight-did-method/src/offchain.rs)
trait lives in `midnight-did-method`; the value serializer is
injected so the domain crate stays free of any `compact-runtime` dep.
Tests use a `Vec<u8>`-based golden-vector codec.

### 4.5 Private state behind a `PrivateStateStore` trait

The witness `localSecretKey()` (called by the generated contract) reads
through
[`PrivateStateStore`](../crates/midnight-did-api/src/private_state.rs).
`InMemoryPrivateStateStore` covers tests; the mobile wallet swaps in a
file-backed (later keychain-backed) impl; the CLI uses a fresh
in-memory store seeded from a hex seed.

In v0.4.0 `Backend::submit_tx` no longer takes private state — Path 2
(see §4.1) sidesteps the in-circuit invocation. Private state will
re-enter the picture via a `DidWitnesses` impl over
`DidPrivateState` when `LiveBackend::submit_tx` is wired against
`generated::Contract<PS, W>` (Future Work item 4 in ADR 0008). ADR 0004
remains the rationale for the trait shape itself; only the call site
has moved.

Rationale: [ADR 0004](./adr/0004-private-state-as-trait.md) (partially
superseded by ADR 0008).

### 4.6 `midnight-did-api` depends on `midnight-did-runtime` (v0.4.0)

Pre-v0.4.0 the api crate owned the contract-abstraction shape
(`DidContract` trait + `RecordingContract` mock) and did NOT depend on
the runtime crate. R2-2 moved that surface — `Contract<B>`, `Backend`,
`DidContractCall`, the three `Backend` impls — into
`midnight-did-runtime` where the wire types live. The api crate now
imports them.

This adds a transitive dep from api → runtime → `compact-runtime` +
`midnight-ledger`. The resolver path (which stops at
`midnight-did-method`) is unaffected; the wasm gate (which builds
domain + api) tracks whether runtime stays wasm-clean. As of v0.4.1
the wasm32 build is green.

---

## 5. Testing strategy

**Per-module unit tests.** Workspace-wide ~231 tests post-R1 (v0.2.0
+ v0.3.0). Each ports the equivalent TS unit test where one exists;
otherwise covers a single function's contract.

**Integration tests at the api layer.** 56 integration tests across
six files in `crates/midnight-did-api/tests/` drive end-to-end CRUD
flows through `Contract::new(RecordingBackend::with_snapshot(...),
ADDR, NETWORK)`. Each test asserts on the recorded
`Vec<DidContractCall>` via `contract.backend().recorded_calls()`.
Coverage spans the full TS-port matrix for P0 acceptance (ledger
mappers, runtime-domain roundtrip, VM CRUD, controller rotation,
private-state, end-to-end DID API).

**Builder + decode validation tests (v0.4.1).** 34 additional tests
across `crates/midnight-did-api/tests/{builder,decode}_validation.rs`
exercise the encode-side `::new` rejection path (19 cases) and the
decode-side `try_from` / hand-rolled `Deserialize` rejection path
(15 cases including 3 positive round-trip + 2 envelope-level checks).
See §4.1a.

**TS reference fixtures.** Three JSON fixtures captured from the TS
`@midnight-ntwrk/midnight-did-api` test suite at deterministic points
(initial DID Document, post-controller-rotate, post-VM-add). The Rust
`tests/did_api_end_to_end.rs` runs the same operation sequence and
asserts `serde_json::Value`-equality against the fixture. This proves
the domain types serialize identically across runtimes.

**Future: byte-parity tests for `ContractState`.** The on-chain
byte-parity story now ships as work toward `LiveBackend::submit_tx` —
when that lands, the existing
`tests-e2e-rust/tests/codegen_regression.rs` shape from the
codegen-rust toolchain can replay the nine fixture points enumerated
in the port plan against `Contract<LiveBackend>` and assert
byte-equality on `ContractState.serialize()`. The runtime crate
itself builds today (since v0.3.0); the gate is the wallet bridge,
not the runtime build.

**CI.** GitHub Actions workflow runs `cargo fmt --check`, `cargo
clippy -D warnings`, and `cargo test` on the workspace including the
runtime crate.

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

## 6. Open questions and roadmap

Status of the five open questions originally captured in the
2026-06-03 session note:

1. **Crate split.** ✅ Closed — 5 crates (domain, method, api,
   runtime, umbrella). See
   [ADR 0003](./adr/0003-crate-split-2-to-4-with-umbrella.md).
2. **Async runtime choice.** ✅ Closed — runtime-agnostic. Async-trait
   + Pin/Box/Future on public surfaces; consumers pick the runtime.
   See [ADR 0001](./adr/0001-async-only-api.md).
3. **`midnight-did` crate name.** ✅ Closed — runtime crate renamed
   to `midnight-did-runtime`; `midnight-did` is the umbrella.
4. **Witness coupling.** ⏸ Partial — `PrivateStateStore` trait is
   in place; `LiveBackend::submit_tx` will re-thread it via
   `DidWitnesses` once the wallet+proof+indexer bridge lands. See
   [ADR 0004](./adr/0004-private-state-as-trait.md) (partially
   superseded by ADR 0008) and ADR 0008 Future Work item 4.
5. **Codegen gap closure ordering.** ✅ Closed — the codegen-rust
   toolchain (A1-A21 walker gaps + Bug-1..9) closed the gaps;
   `compactc --rust did.compact` succeeds clean as of compact pin
   `960fc26` (v0.3.0). See
   [ADR 0005](./adr/0005-codegen-gap-handling.md).

**Roadmap items** (no milestone vocabulary; ordered by sequencing):

- Wire `LiveBackend::submit_tx` and `LiveBackend::read_snapshot`
  against the wallet+proof-server+indexer bridge — the public Rust
  API shape is already final (v0.4.0). See ADR 0008 Future Work.
- Replay the nine TS `ContractState.serialize()` byte-parity
  fixtures once `LiveBackend::submit_tx` lands.
- Add the UniFFI wrapper crate `midnight-did-uniffi`.
- ~~Add a wasm-target build proof~~ — done (CI `wasm-build` job
  builds `midnight-did-domain` + `midnight-did-api` against
  `wasm32-unknown-unknown` on every PR). Future work: a thin
  browser-side wrapper crate (wasm-bindgen + serde-wasm-bindgen)
  exposing the resolver path to JS.
- Publish `midnight-did-domain` + `midnight-did-method` to crates.io
  ahead of the runtime crate (no halo2 dep, fast to ship).

---

## 7. Pointers

**Repo entry points:**
- [README](../README.md) — CI badge + project elevator pitch +
  Quick-start snippet.
- [`Cargo.toml`](../Cargo.toml) — workspace manifest, ledger crate pins.
- [`CHANGELOG.md`](../CHANGELOG.md) — per-release notes (v0.1 → v0.4.1).
- [`crates/midnight-did-domain/src/lib.rs`](../crates/midnight-did-domain/src/lib.rs)
  — pure-data crate module index + rustdoc tour.
- [`crates/midnight-did-api/src/lib.rs`](../crates/midnight-did-api/src/lib.rs)
  — api crate module index.
- [`crates/midnight-did-runtime/src/backend.rs`](../crates/midnight-did-runtime/src/backend.rs)
  — `Backend` trait + `LiveBackend` / `RecordingBackend` /
  `ResolverBackend`.
- [`crates/midnight-did-runtime/src/contract_call.rs`](../crates/midnight-did-runtime/src/contract_call.rs)
  — `DidContractCall` enum + `DidLedgerSnapshot`.
- [`crates/midnight-did-runtime/src/contract/generated.rs`](../crates/midnight-did-runtime/src/contract/generated.rs)
  — codegen-rust output; do not edit by hand.

**Architecture Decision Records:**
- [ADR 0001 — Async-only API](./adr/0001-async-only-api.md)
- [ADR 0002 — Trait erasure for contract calls](./adr/0002-trait-erasure-for-contract.md) (superseded by ADR 0008)
- [ADR 0003 — Crate split path: 2 → 4 + umbrella](./adr/0003-crate-split-2-to-4-with-umbrella.md)
- [ADR 0004 — Private state as a trait](./adr/0004-private-state-as-trait.md) (partially superseded by ADR 0008)
- [ADR 0005 — Codegen-gap handling strategy](./adr/0005-codegen-gap-handling.md)
- [ADR 0006 — Runtime crate halo2 ParamsKZG block](./adr/0006-runtime-crate-halo2-block.md)
- [ADR 0007 — R1 type-safety sweep](./adr/0007-type-safety-sweep.md)
- [ADR 0008 — R2 contract-abstraction reform](./adr/0008-contract-abstraction-reform.md)

**Upstream references:**
- TS source: `@midnight-ntwrk/midnight-did` (develop branch, mirrored
  at `third_party/midnight-did/`).
- Compact codegen: `compactc --rust` from the codegen-rust toolchain
  branch.
- W3C DID Core 1.0 spec: https://www.w3.org/TR/did-core/
