<!--
This file is part of midnightntwrk/midnight-did-rs.
Copyright (C) 2026 Midnight Foundation
SPDX-License-Identifier: Apache-2.0
-->

# ADR 0007 — R1 type-safety sweep (v0.2.0)

**Status**: Accepted 2026-06-23. Implemented in v0.2.0 (steps 1, 2, 3, 4a, 5, 6, 7). Steps 4b + 4c deferred to v0.3.0.
**Supersedes (partial)**: [ADR 0002 — trait erasure for contract](0002-trait-erasure-for-contract.md), [ADR 0004 — private state as trait](0004-private-state-as-trait.md). R2 will fully address the trait-shape reform; this ADR closes the type-safety side.
**Related**: [Design spec — R1 type-safety sweep](../specs/2026-06-23-r1-type-safety-sweep-design.md).

## Context

The initial Rust port of `@midnight-ntwrk/midnight-did-*` preserved
several TypeScript-flavored patterns that didn't earn their weight in
Rust. The four-axis audit (API surface, type safety, async/sync, contract
abstraction) ran via parallel research subagents and identified five
TS-isms with the highest "impact-per-unit-of-effort":

1. **Hex-validated `String` newtypes** (`ContractAddress`,
   `OffchainStateHashHex`) where upstream
   `midnight_coin_structure::contract::ContractAddress(pub HashOutput)`
   and `midnight_base_crypto::hash::HashOutput([u8; 32])` already exist
   with the full Midnight-ledger trait stack derived.
2. **W3C identifier strings** (`DidKeyId`, `FragmentId`, `ServiceId`)
   stored as `String` with leading-`#` and `did:` checks scattered as
   runtime conditionals across the codebase.
3. **Public-field structs with separate `.validate()` methods** —
   invalid state representable, callers can forget the validator,
   "deserialize then forget validate" footgun.
4. **Flat `ApiError(13 variants)`** where domain-grouped errors would
   give callers narrower pattern-match targets.
5. **`create_did(secret_key: Option<[u8; 32]>) = None → [0u8; 32]`
   silent default** — a real footgun production callers could trip.

A clean break to `v0.2.0` was chosen over deprecation shims to keep
the diff focused and the post-cleanup API legible.

## Decision

### Closed in v0.2.0

| Step | Change | Status |
|---|---|---|
| **R1-1** | Add `midnight_did_method::hex_ext` module + `HashOutputExt` trait for full-hex round-trip on upstream types (upstream `Display` is truncated for logs). | Shipped (commit `80478cb`) |
| **R1-2** | Drop the `ContractAddress(pub String)` + `OffchainStateHashHex(pub String)` shadow newtypes. Re-export upstream types directly. | Shipped (commit `36a1835`) |
| **R1-3** | New `midnight_did_domain::ids` module with `DidKeyId`, `FragmentId`, `ServiceId` newtypes (private inner field, validating `::new`, validating `Deserialize`). | Shipped (commit `b5dccde`) |
| **R1-4a** | Add fallible `VerificationMethod::new`, `Service::new`, `PublicKeyJwk::new` constructors + validating `Deserialize` for `PublicKeyJwk` via `#[serde(try_from)]`. Additive — legacy factories still work. | Shipped (commit `a412f67`) |
| **R1-5** | New `DidDocumentBuilder` with cross-reference validation on `build()`. | Shipped (commit `89e7fab`) |
| **R1-6** | Split `ApiError(13)` into domain-grouped enums: `VerificationError`, `ControllerError`, `ContractError` + umbrella `ApiError` with `#[from]`. | Shipped (commit `9cde8cd`) |
| **R1-7** | `create_did(.., secret_key: [u8; 32])` — drop `Option`, require explicit secret. | Shipped (commit `42aa17f`) |
| **R1-8** | v0.2.0 lockstep bump + CHANGELOG + this ADR. | Shipped (this commit) |

### Deferred to v0.3.0

| Step | Change | Why deferred |
|---|---|---|
| **R1-4b** | Make `VerificationMethod` / `Service` / `PublicKeyJwk` fields private + add accessor methods. | The load-bearing "invariants unrepresentable" change. M-effort on its own but pairs naturally with 4c. |
| **R1-4c** | Migrate the ~114 existing direct struct-literal construction sites in domain / api / method / cli / uniffi / tests to `::new(NewX)` and retire the `create_verification_method` / `create_service` free functions. | L-effort mechanical mass-replace. Benefits from a dedicated session — too risky to squeeze into the v0.2.0 window. |

The 4a path (additive `::new(NewX)` constructors with the legacy
factories continuing to work) **lets v0.2.0 ship the new API surface
without breaking the existing surface**. Callers can opt in to `::new`
immediately; the `0.3.0` cycle finishes the migration when the
mass-replace can land coherently in one focused commit.

## Consequences

### Wins

- **Wire format unchanged**. All 13 TS reference byte-parity JSON
  fixtures pass — the in-memory shape change is invisible to the wire.
- **Validating `Deserialize` closes the "forget to validate" footgun**
  for `PublicKeyJwk` (via `#[serde(try_from)]`) and for every newtype
  in `midnight_did_domain::ids`.
- **Domain-grouped errors** let single-domain callers pattern-match a
  narrow type without handling 9+ unrelated variants. The `?` operator
  still threads everything through the umbrella for multi-domain
  operations.
- **Upstream primitive reuse** eliminates duplicate code paths and
  inherits Midnight's ledger trait stack (`FieldRepr`, `FromFieldRepr`,
  `BinaryHashRepr`, `Serializable`, constant-time `eq`, `Zeroize`).
- **`create_did` no longer has a silent default** — the type system
  enforces explicit secret-key supply.

### Costs

- **Breaking change** to the public Rust API. Migration guidance is
  documented per-section in [CHANGELOG.md](../../CHANGELOG.md) under
  the v0.2.0 entry; no deprecation shims (the cost of dual-API
  maintenance outweighed the benefit for an early-stage crate).
- **Steps 4b/4c not yet shipped** — callers can still construct
  `VerificationMethod { id: ..., ... }` directly, bypassing `::new`'s
  validation. The v0.2.0 surface *recommends* `::new`; v0.3.0 will
  *require* it via private fields.
- **`HashOutput::Display` is truncated upstream** (10-char log
  preview). Anywhere we need full-hex round-trip we now go through
  `HashOutputExt::to_hex`. Confused-cousin risk if a caller uses
  `format!("{h}")` instead of `h.to_hex()`. The trait is re-exported
  prominently to mitigate.

### Test coverage growth

| Metric | Pre-R1 (v0.1) | Post-R1 (v0.2.0) | Δ |
|---|---|---|---|
| Workspace tests | 144 | 231 | +87 |
| Per-newtype validation tests | 0 | 23 (`tests/ids.rs`) | +23 |
| Per-constructor `::new` tests | 0 | 17 (`tests/constructors.rs`) | +17 |
| Cross-reference builder tests | 0 | 12 (`tests/did_document_builder.rs`) | +12 |
| Error-hierarchy lift/match tests | 0 | 12 (`tests/error_hierarchy.rs`) | +12 |
| `hex_ext` round-trip tests | 0 | 13 (`tests/hex_ext.rs`) | +13 |

## How to migrate (v0.1 → v0.2.0)

Most v0.1 call sites continue to work because the breaking changes are
narrow. Concrete migrations:

```rust
// ContractAddress — was: pub String wrapper; now: upstream type
- use midnight_did_method::midnight_did::ContractAddress;
+ use compact_runtime::ContractAddress;
+ use midnight_did_method::hex_ext::HashOutputExt;
- let s = addr.0;            // pre-v0.2: String
+ let s = addr.to_hex();     // v0.2: String, allocates on demand

// MidnightSubjectId::as_hex returns String now (not &str)
- assert_eq!(subj.as_hex(), expected_hex);
+ assert_eq!(subj.to_hex(), expected_hex);

// create_did — secret_key now required
- create_did(&contract, &store, Some([0u8; 32])).await?;
+ create_did(&contract, &store, [0u8; 32]).await?;     // explicit bytes
+ // or in the CLI / ad-hoc cases:
+ create_did(&contract, &store, rand::thread_rng().r#gen()).await?;

// ApiError — domain-grouped variants
- if let Err(ApiError::RelationMissing { .. }) = err { ... }
+ if let Err(ApiError::Verification(VerificationError::RelationMissing { .. })) = err { ... }
+ // or, in functions that only deal with the verification domain,
+ // return `Result<_, VerificationError>` directly and pattern-match
+ // the narrow type.
```

## References

- [ADR 0001 — async-only API](0001-async-only-api.md) — kept, reaffirmed by audit
- [ADR 0002 — trait erasure for contract](0002-trait-erasure-for-contract.md) — partial supersede; full reform in R2
- [ADR 0003 — crate split 2→4 + umbrella](0003-crate-split-2-to-4-with-umbrella.md) — unchanged
- [ADR 0004 — private state as trait](0004-private-state-as-trait.md) — partial supersede; full reform in R2
- [ADR 0005 — codegen gap handling](0005-codegen-gap-handling.md) — unchanged
- [ADR 0006 — runtime crate halo2 block](0006-runtime-crate-halo2-block.md) — unchanged
- [Design spec — R1 type-safety sweep](../specs/2026-06-23-r1-type-safety-sweep-design.md)
