# midnight-did-domain

[![docs.rs](https://img.shields.io/docsrs/midnight-did-domain)](https://docs.rs/midnight-did-domain)
[![crates.io](https://img.shields.io/crates/v/midnight-did-domain.svg)](https://crates.io/crates/midnight-did-domain)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://www.apache.org/licenses/LICENSE-2.0)

Pure-data Rust **domain layer** for the [Midnight DID Method][spec]. This
crate holds the W3C-aligned DID Document model, MOD1 offchain frame encoder,
crypto codecs, and resolver/registrar traits — and **nothing else**. It does
not depend on any `midnight-*` runtime/ledger crate, so it builds on stable
Rust, wasm32, and embedded targets without pulling in halo2 or the Midnight
proving stack.

## What this crate provides

- `did_document` — `DidDocument`, `VerificationMethod`, `Service`,
  `PublicKeyJwk`, validators, and JSON (de)serialization.
- `offchain` — MOD1 frame encoder/decoder used for off-chain DID document
  storage (byte-parity with the TypeScript reference implementation).
- `crypto_codecs` — Jubjub point + multibase codecs, BLAKE2b helpers,
  digest types.
- `did_resolver` / `did_registrar` — abstract resolver + registrar traits
  the API layer plugs into.
- `midnight` — `MidnightNetwork` enum (Testnet / Mainnet / DevNet / Undeployed).
- `uri` — DID URI parser + builder.
- `ledger_utils` — hex helpers shared with the API layer.

## Layering

```
midnight-did-domain   ← THIS CRATE (no midnight-* deps, wasm-clean)
       ↑
midnight-did-api      (async ops + DidContract trait, see sibling crate)
       ↑
midnight-did          (runtime, blocked on upstream halo2 skew)
```

The trait-erasure split is documented in [ADR 0002][adr2] and the four-crate
shape in [ADR 0003][adr3].

## Status

- 51 unit tests passing.
- Byte-parity with the TS `@midnight-ntwrk/midnight-did-domain` package for
  MOD1 frame encoding.
- See [architecture][arch] for the full layering rationale.

## Related crates

- [`midnight-did-api`](https://crates.io/crates/midnight-did-api) — async
  operation builders, DID resolver, and `DidContract` trait. Depends on this
  crate.

## License

Apache-2.0. See `LICENSE` at the workspace root.

[spec]: https://github.com/midnight-ntwrk/midnight-did
[arch]: ../../doc/architecture.md
[adr2]: ../../doc/adr/0002-trait-erasure-for-contract.md
[adr3]: ../../doc/adr/0003-crate-split-2-to-4-with-umbrella.md
