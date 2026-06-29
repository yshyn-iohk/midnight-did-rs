# midnight-did-rs

[![CI](https://github.com/yshyn-iohk/midnight-did-rs/actions/workflows/ci.yml/badge.svg?branch=cycle-1-bootstrap)](https://github.com/yshyn-iohk/midnight-did-rs/actions/workflows/ci.yml)
[![version](https://img.shields.io/badge/version-v0.4.1-blue)](./CHANGELOG.md)

Midnight DID Method, in Rust. Native port of the TypeScript reference
implementation (`@midnight-ntwrk/midnight-did-*`), byte-parity with the
TS wire format on both the offchain DID URL frame and the on-chain
contract envelope.

## Highlights

- **5-crate split** — pure-data domain layer, Midnight method profile,
  operation builders + ledger mappers, runtime (codegen target), and an
  umbrella re-export crate. Each downstream consumer (wallet, resolver,
  wasm, UniFFI) pulls only the cones it needs.
- **DID CRUD via `Contract<B: Backend>`** — v0.4.0 retired the
  `DidContract` async trait; the concrete `Contract<B>` wrapper +
  3-method `Backend` trait is the new seam between operation builders
  and the transport layer. See
  [ADR 0008](./doc/adr/0008-contract-abstraction-reform.md).
- **Builder + decode validation gates (v0.4.1)** — both sides of
  `BuiltTx::bytes` are locked for the SchnorrJubjub ledger-shape
  types: callers can no longer struct-literal a malformed value, and
  incoming envelopes can't smuggle one in either.
- **TS reference byte-parity** — 13 JSON fixtures captured from the
  TS test suite, replayed against the Rust types.
- **56 integration + 34 builder/decode validation tests** at the api
  layer, plus 231 workspace-wide unit tests.

## Crate layout (v0.4.0+)

```
                ┌───────────────────────────────┐
                │     midnight-did-domain       │   pure-data W3C DID Core
                │   (no midnight-* deps)        │   + crypto codecs
                └──────────────┬────────────────┘
                               │
                               ▼
                ┌───────────────────────────────┐
                │     midnight-did-method       │   did:midnight:* parsing
                │                               │   + MOD1 offchain codec
                └──────────────┬────────────────┘
                               │
                               ▼
                ┌───────────────────────────────┐
                │      midnight-did-api         │   operation builders
                │                               │   + ledger mappers
                └──────────────┬────────────────┘
                               │
                               ▼
                ┌───────────────────────────────┐
                │    midnight-did-runtime       │   compactc --rust output
                │   Contract<B> + Backend trait │   + DidContractCall enum
                └──────────────┬────────────────┘
                               │
                               ▼
                ┌───────────────────────────────┐
                │        midnight-did           │   umbrella re-export
                │                               │   (monolithic consumers)
                └───────────────────────────────┘
```

The dep direction is strict: domain ← method ← api ← runtime ← umbrella.
The resolver use case stops at `midnight-did-method`; the wallet pulls
the umbrella. See [`doc/architecture.md`](./doc/architecture.md) for
the full breakdown.

## Quick start

In a test, drive the contract through `RecordingBackend` — every
`submit_tx` is decoded back into a typed `DidContractCall` you can
assert on without spinning up a halo2 prover:

```rust
use midnight_did_runtime::{
    Contract,
    backend::RecordingBackend,
    contract_call::DidLedgerSnapshot,
};
use compact_runtime::ContractAddress;

let snapshot = DidLedgerSnapshot::default(); // or a real fixture
let addr = ContractAddress::default();
let network = midnight_did_method::Network::Undeployed;

let contract = Contract::new(
    RecordingBackend::with_snapshot(snapshot),
    addr,
    network,
);

// drive any of the 12 inherent `Contract<B>` methods,
// or hand `&contract` to the operation builders in midnight-did-api…

let recorded = contract.backend().recorded_calls();
assert_eq!(recorded.len(), 1);
```

In production code, the same surface accepts a `LiveBackend`:

```rust
use midnight_did_runtime::backend::LiveBackend;

// NOTE: LiveBackend::{submit_tx, read_snapshot} are todo!() in v0.4.1.
// The Rust API shape is final; the implementation lands once the
// wallet+proof-server+indexer bridge is wired up. See ADR 0008,
// "Future work".
let contract = Contract::new(LiveBackend::new(/* …deps… */), addr, network);
```

## CI + wasm gate

Every PR is built on Linux + macOS (host target) **and** against
`wasm32-unknown-unknown` for the `midnight-did-domain` +
`midnight-did-api` crates. The wasm gate enforces the design claim
that the domain + api layers are runtime-agnostic and free of any
`midnight-*` deps that would block in-browser use (see
[`doc/architecture.md`](./doc/architecture.md) §6).

## Pointers

- [`CHANGELOG.md`](./CHANGELOG.md) — release notes.
- [`doc/architecture.md`](./doc/architecture.md) — full architecture
  overview.
- [`doc/adr/`](./doc/adr/) — Architecture Decision Records (ADRs
  0001–0008).
- [`doc/specs/`](./doc/specs/) — implementation specs (R1 type-safety
  sweep, R2 contract-abstraction reform).
