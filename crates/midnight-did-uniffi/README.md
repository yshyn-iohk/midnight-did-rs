<!--
This file is part of midnightntwrk/midnight-did-rs.
Copyright (C) 2026 Midnight Foundation
SPDX-License-Identifier: Apache-2.0
-->

# midnight-did-uniffi

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://www.apache.org/licenses/LICENSE-2.0)

**UniFFI proof-of-concept** exposing the Midnight DID async Rust API to
Swift, Kotlin, and Python via [uniffi-rs] 0.29. The crate is intentionally
small — its sole purpose is to demonstrate that the
[`midnight-did-api`](../midnight-did-api) surface flattens cleanly across the
FFI boundary without an explosion of intermediate types or generics.

[uniffi-rs]: https://github.com/mozilla/uniffi-rs

## FFI surface

| Function                            | Returns                            |
|-------------------------------------|------------------------------------|
| `create_did(handle, seed, pk)`      | JSON `{did, controller_public_key_hex}` |
| `rotate_controller_key(handle, …)`  | JSON `{did, tx_hash, block_height}`     |
| `resolve_did(handle, did)`          | JSON DID Document + metadata            |
| `deactivate(handle, did)`           | JSON `{did, tx_hash, block_height}`     |

Plus one opaque `DidServiceHandle` object with a default constructor and two
small accessors (`contract_address()`, `network()`).

All four functions are `async`. UniFFI 0.29 with the `tokio` feature maps
them onto:

- **Swift**: `async throws -> String`
- **Kotlin**: `suspend fun … : String` (throws `FlatException`)
- **Python**: `async def …` raising `FlatError`

## Design notes

- **JSON strings at the boundary.** Returning the DID Document as a JSON
  `String` keeps the FFI shape generic-free; foreign-language callers decode
  with their native JSON library (`JSONDecoder` on Swift,
  `kotlinx.serialization` on Kotlin, `json.loads` on Python). This avoids
  mirroring every `midnight-did-domain` struct in the uniffi schema and keeps
  the wire format stable across domain-crate refactors.
- **Flat error enum.** [`FlatError`](src/error.rs) has one variant per
  failure category (`InvalidInput`, `Contract`, `Validation`, `Serde`,
  `NotFound`) with a single `message` field. The richer
  [`ApiError`](../midnight-did-api/src/error.rs) is mapped through
  `impl From<ApiError> for FlatError` in
  [`src/error.rs`](src/error.rs).
- **Opaque handle with tokio Mutex.** `DidServiceHandle` wraps the
  `RecordingContract` mock inside an `Arc<tokio::sync::Mutex<_>>`. Concurrent
  FFI calls from Swift structured concurrency or Kotlin coroutines cannot
  race on the mock's in-memory state.
- **No generics, no `&dyn Trait`.** Every public signature uses
  `Arc<DidServiceHandle>`, `String`, or `Result<String, FlatError>` —
  uniffi-compatible primitives only.

## Generating Swift bindings

```sh
# Build the cdylib first.
cargo build -p midnight-did-uniffi

# Generate Swift source from the compiled library.
cargo run --features cli -p midnight-did-uniffi --bin uniffi-bindgen -- \
    generate \
    --library target/debug/libmidnight_did_uniffi.dylib \
    --language swift \
    --out-dir generated-swift
ls generated-swift
# midnight_did_uniffi.swift
# midnight_did_uniffiFFI.h
# midnight_did_uniffiFFI.modulemap
```

Sample of the generated Swift signature:

```swift
public func createDid(
    handle: DidServiceHandle,
    seedHex: String,
    controllerPublicKeyHex: String
) async throws -> String

public func resolveDid(
    handle: DidServiceHandle,
    didSubject: String
) async throws -> String
```

## Generating Kotlin bindings

```sh
cargo run --features cli -p midnight-did-uniffi --bin uniffi-bindgen -- \
    generate \
    --library target/debug/libmidnight_did_uniffi.dylib \
    --language kotlin \
    --out-dir generated-kotlin
find generated-kotlin -name '*.kt'
# generated-kotlin/uniffi/midnight_did_uniffi/midnight_did_uniffi.kt
```

## Generating Python bindings

```sh
cargo run --features cli -p midnight-did-uniffi --bin uniffi-bindgen -- \
    generate \
    --library target/debug/libmidnight_did_uniffi.dylib \
    --language python \
    --out-dir generated-python
ls generated-python
# midnight_did_uniffi.py
```

## Workaround if `uniffi-bindgen` is not on PATH

If you don't want to build the in-crate `uniffi-bindgen` binary, install the
standalone CLI:

```sh
cargo install --version 0.29 uniffi-bindgen-cli
```

The standalone CLI accepts the same `generate --library … --language …`
arguments shown above.

## Limitations

- **Mock contract.** The current `DidServiceHandle::new()` constructs a
  `RecordingContract` (the in-memory mock from `midnight-did-api`). The mock
  records calls but does not commit state changes; resolving a freshly
  constructed handle returns a default DID Document. The real contract impl
  will be slotted in once the [`midnight-did`](../midnight-did) runtime crate
  builds end-to-end (currently blocked on halo2 `ParamsKZG` API skew — see
  the [architecture doc](../../doc/architecture.md)).
- **4 operations only.** Full CRUD coverage (verification methods, services,
  relations) follows once the FFI shape is settled. The 4 chosen operations
  are enough to validate the binding generation across all three target
  languages.
- **No async streams.** UniFFI 0.29 supports `Future`-style async only;
  long-running subscriptions (e.g. ledger event streams) need a callback
  interface in a follow-up.

## See also

- [ADR 0001](../../doc/adr/0001-async-only-api.md) — Async-only API
  decision (the foundation this crate builds on).
- [Architecture overview](../../doc/architecture.md) — How `domain`,
  `api`, `uniffi`, and the runtime crate fit together.
- [`midnight-did-api`](../midnight-did-api) — The async Rust API being
  exported.
