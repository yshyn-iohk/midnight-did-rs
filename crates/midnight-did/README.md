<!--
This file is part of midnightntwrk/midnight-did-rs.
Copyright (C) 2026 Midnight Foundation
SPDX-License-Identifier: Apache-2.0
-->

# midnight-did

Umbrella crate re-exporting the full Midnight DID Rust stack.

This crate is the canonical entry point for **monolithic consumers**
— mobile wallets, end-user CLIs, and other applications that want every
layer of the stack behind a single dependency.

```toml
[dependencies]
midnight-did = "0.1"
```

```rust
use midnight_did::{DidDocument, MidnightDidString, MidnightNetwork};
use midnight_did::api::DidContract;
```

## The 4-crate stack

```text
+-------------------------+
|   midnight-did (this)   |  ← re-exports all 4 siblings
+-------------------------+
| midnight-did-runtime    |  codegen target (opt-in, behind `runtime` feature)
| midnight-did-api        |  async DidContract + operation builders
| midnight-did-method     |  Midnight method profile (did:midnight:*)
| midnight-did-domain     |  pure W3C DID Core types
+-------------------------+
```

## When to depend on this crate vs the siblings

- **Resolver / wasm consumer**: depend on
  [`midnight-did-domain`](../midnight-did-domain) +
  [`midnight-did-method`](../midnight-did-method) directly. Skip the api
  and runtime layers. Saves a substantial dependency cone in browser
  bundles.
- **Write-side CLI / library**: depend on
  [`midnight-did-api`](../midnight-did-api) directly. It transitively
  pulls in domain + method.
- **Mobile / Dioxus / monolithic app**: depend on **this** umbrella
  crate. Opt into `features = ["runtime"]` to also pull the codegen
  target.

See
[ADR 0003](../../doc/adr/0003-crate-split-2-to-4-with-umbrella.md)
for the design rationale.

## Features

- `runtime` (off by default) — pulls in `midnight-did-runtime`. The
  runtime crate is currently blocked on an upstream `halo2`
  `ParamsKZG` API skew and will not compile end-to-end until that is
  resolved. See DID-P2-2.
