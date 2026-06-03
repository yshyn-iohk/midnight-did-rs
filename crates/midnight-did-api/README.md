# midnight-did-api

[![docs.rs](https://img.shields.io/docsrs/midnight-did-api)](https://docs.rs/midnight-did-api)
[![crates.io](https://img.shields.io/crates/v/midnight-did-api.svg)](https://crates.io/crates/midnight-did-api)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://www.apache.org/licenses/LICENSE-2.0)

Async operation builders, DID resolver, and contract abstraction traits for
the [Midnight DID Method][spec]. This crate is the Rust port of the
operation half of the TypeScript `@midnight-ntwrk/midnight-did-api` package.
It sits on top of [`midnight-did-domain`][domain] and abstracts the on-chain
contract behind the [`DidContract`] trait so the entire API surface is
testable without the runtime/halo2 stack.

## What this crate provides

- `contract` — `DidContract` trait, `DidLedgerSnapshot` view, mutation tags,
  and a `RecordingContract` mock.
- `*_operations` — async operation builders for controller / verification
  method / service / document / DID CRUD families.
- `resolution` — ledger snapshot → `DidDocument` resolver.
- `ledger_mappers` — domain → ledger conversion helpers.
- `private_state` — controller private-state lifecycle + storage trait.
- `network_mapping`, `subject` — runtime/domain glue + DID subject helpers.

## Trait erasure

The `DidContract` trait is the seam between this crate and the (blocked)
`midnight-did` runtime crate. Operations consume `&impl DidContract` and
never name the runtime — see [ADR 0002][adr2] for the rationale.

## Quick start

```rust,no_run
use midnight_did_api::{
    contract::mock::{RecordedCall, RecordingContract},
    controller_operations::rotate_controller_key,
    private_state::InMemoryPrivateStateStore,
};
use midnight_did_domain::midnight::MidnightNetwork;

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let addr = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
let contract = RecordingContract::new(addr, MidnightNetwork::Testnet);
let store = InMemoryPrivateStateStore::new();

let new_sk = [4u8; 32];
let new_pk = [9u8; 32];
rotate_controller_key(&contract, &store, new_sk, new_pk).await?;

let calls = contract.calls();
assert!(matches!(
    calls.first(),
    Some(RecordedCall::RotateControllerKey(pk)) if *pk == new_pk
));
# Ok(()) }
```

## Status

- 75 unit + integration tests passing. Every operation is exercised through
  the `RecordingContract` mock; the same trait is satisfied by the real
  runtime once it builds.
- See [architecture][arch] and [ADR 0001][adr1] (async-only API).

## Related crates

- [`midnight-did-domain`](https://crates.io/crates/midnight-did-domain) —
  pure-data DID Document model + MOD1 offchain encoder.

## License

Apache-2.0.

[spec]: https://github.com/midnight-ntwrk/midnight-did
[domain]: https://crates.io/crates/midnight-did-domain
[`DidContract`]: https://docs.rs/midnight-did-api/latest/midnight_did_api/contract/trait.DidContract.html
[arch]: ../../doc/architecture.md
[adr1]: ../../doc/adr/0001-async-only-api.md
[adr2]: ../../doc/adr/0002-trait-erasure-for-contract.md
