<!--
This file is part of midnightntwrk/midnight-did-rs.
Copyright (C) 2026 Midnight Foundation
SPDX-License-Identifier: Apache-2.0
-->

# midnight-did-method

Midnight method profile for the Rust port of the [Midnight DID Method][adr].

[adr]: https://github.com/yshyn-iohk/midnight-did-rs/blob/main/doc/adr/0003-crate-split-2-to-4-with-umbrella.md

This crate sits between [`midnight-did-domain`][domain] (pure W3C DID Core
types) and [`midnight-did-api`][api] (the async operation layer). It hosts
the pieces that are **specific to the Midnight method** but do not need
the on-chain runtime or the operation-layer abstractions.

[domain]: ../midnight-did-domain
[api]: ../midnight-did-api

## What lives here

- `midnight_did` — `did:midnight:<network>:<id>` string types, parsing,
  and subject-id helpers (moved from `midnight-did-domain::midnight`).
- `network_mapping` — runtime ↔ domain network identifier mapping
  (moved from `midnight-did-api::network_mapping`).

## When to depend on this crate vs. the others

- **Resolver-only** consumers (read a `did:midnight:*` from the ledger,
  return a DID Document JSON): depend on
  `midnight-did-domain` + `midnight-did-method` + a public-data provider.
  Skip the api crate entirely.
- **Write-side** consumers (mutate a DID — create / update / deactivate):
  depend on `midnight-did-api`, which transitively pulls this crate in.
- **Mobile / monolithic** consumers can pull the
  [`midnight-did`](../midnight-did) umbrella crate and let it re-export
  everything.

See [ADR 0003][adr] for the dependency-layering rationale.
