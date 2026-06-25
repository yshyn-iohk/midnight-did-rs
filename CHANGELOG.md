<!--
This file is part of midnightntwrk/midnight-did-rs.
Copyright (C) 2026 Midnight Foundation
SPDX-License-Identifier: Apache-2.0
-->

# Changelog

All notable changes to the `midnight-did-rs` workspace are recorded
here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project adheres to [SemVer](https://semver.org/).

## [Unreleased] — v0.4.0

Reserved for the R2 contract abstraction reform follow-up
(`Contract<B: Backend>` wrapping the generated circuit shim once the
wallet+proof+indexer bridge lands). R2-1 (the `Backend` trait + three
impls) shipped against `0.3.0`; R2-2/R2-3 are deferred per
[doc/adr/0008-contract-abstraction-reform-deferred.md](doc/adr/0008-contract-abstraction-reform-deferred.md).

## [0.3.0] — 2026-06-25

### Overview

`0.3.0` is the **R1 finish + R2-1 scaffold** release. It closes the
two `0.2.0`-deferred steps (4b/4c) and lands R2-1 (Backend trait
scaffold) while explicitly deferring R2-2/R2-3 to a future release
(see ADR 0008). Wire format remains byte-identical with the
TypeScript reference. Public Rust API surface has breaking changes
per below.

The cycle's codegen-side milestone — `did.compact` compiles
end-to-end through `compactc --rust` → `cargo check -p
midnight-did-runtime` clean — also lands here via the bumped compact
pin (`960fc26`) and the regenerated `contract/generated.rs`.

### Added

- **`midnight_did_runtime::backend`** — `Backend` async trait
  (`submit_tx`, `read_state`) plus `LiveBackend` (stub — `todo!()`
  until the wallet+proof+indexer bridge lands), `RecordingBackend`
  (Mutex-guarded tx + state snapshot for tests), `ResolverBackend`
  (read-only, rejects `submit_tx`). Forward-compatible scaffold for
  the R2 contract abstraction reform — not yet a callable surface;
  consumers continue to use `DidContract` (`midnight-did-api`) until
  R2-2 lands. See ADR 0008.
- **Accessor methods** on the now-private struct fields:
  `PublicKeyJwk::{kty, crv, x, y, extensions}`,
  `VerificationMethod::{id, type_, controller, public_key_jwk}`,
  `Service::{id, type_, service_endpoint}`, plus `as_str` /
  `into_string` on `DidString` / `DidUrl` / `RelativeUrl`.

### Changed

- **R1 step 4b — privatized fields**: `VerificationMethod`,
  `Service`, `PublicKeyJwk`, `DidString`, `DidUrl`, `RelativeUrl`
  inner fields are now private. The only way to construct these
  values is `::new(NewX) -> Result<Self, ValidationError>`; the only
  way to read them is via accessor methods. Closes the
  "callers can bypass `::new` by struct-literal construction" hole
  `0.2.0` left.
- **R1 step 4c — call-site migration**: the remaining ~17 direct
  struct-literal construction sites across `midnight-did-api`,
  `midnight-did-method`, `midnight-did-cli`, and the integration
  tests are migrated to `::new(NewX)?`. The 4 negative test
  fixtures that previously asserted on
  `verification_method_to_ledger`'s rejection path now assert on
  `PublicKeyJwk::new`'s error path directly (the rejection moved
  upstream when `::new` became fallible).
- **`compact_runtime` pin bumped** (`flake.lock` →
  `yshyn-iohk/compact@960fc26`) — picks up the codegen-rust
  branch's A18/A19, Bug-1..7, R5a/R5b, and Module-1 closures.
  `crates/midnight-did-runtime/src/contract/generated.rs` regen
  output now compiles `cargo check`-clean for the full
  `did.compact` source.

### Removed

- **R1 step 4c — `create_*` helpers retired**:
  `create_verification_method(CreateVerificationMethodParams)`,
  `create_service(CreateServiceParams)`,
  `CreateVerificationMethodParams`, and `CreateServiceParams` are
  gone, along with their `pub use` re-exports from
  `midnight_did_domain::lib`. External callers should switch
  `create_verification_method(p)` →
  `VerificationMethod::new(NewVerificationMethod { ... })?`.

### References

- ADR 0008 — R2-2/R2-3 deferred:
  [doc/adr/0008-contract-abstraction-reform-deferred.md](doc/adr/0008-contract-abstraction-reform-deferred.md)
- ADR 0005 updated with A1–A19 walker-gap closure log:
  [doc/adr/0005-codegen-gap-handling.md](doc/adr/0005-codegen-gap-handling.md)
- Compact codegen-rust branch (cycle's headline milestone):
  https://github.com/yshyn-iohk/compact/tree/codegen-rust

## [0.2.0] — 2026-06-23

### Overview

`0.2.0` is the **type-safety sweep** release: the first
architectural reset after the initial TS port. The audit
(`doc/specs/2026-06-23-r1-type-safety-sweep-design.md`) identified
five high-impact TS-isms that didn't pay rent in Rust; this
release closes six of the eight remediation steps, with the
remaining two (steps 4b/4c — private fields + mass call-site
migration) deferred to `0.3.0`.

Wire format (JSON byte-parity with the TypeScript `@midnight-ntwrk/midnight-did-*`
reference) is **unchanged**. In-memory representations and the
public Rust API surface have breaking changes per below.

### Added

- **`midnight_did_method::hex_ext`** — `HashOutputExt` trait
  providing `from_hex(&str)` / `to_hex()` round-trips for the
  upstream `midnight_base_crypto::hash::HashOutput` and
  `compact_runtime::ContractAddress` types. Upstream `Display`
  is intentionally truncated to 10 hex chars for logs; this
  trait covers the full 64-char round-trip the DID document
  wire format uses.
- **`midnight_did_domain::ids`** — new module containing the
  W3C-DID identifier newtypes `DidKeyId`, `FragmentId`,
  `ServiceId`. Each has a private inner field, validating
  `Self::new(impl Into<String>) -> Result<Self, IdError>`,
  validating `Deserialize` impl that delegates to `::new`, and
  `#[serde(transparent)]` `Serialize` keeping the wire format
  identical.
- **`VerificationMethod::new(NewVerificationMethod)`**,
  **`Service::new(NewService)`**,
  **`PublicKeyJwk::new(NewPublicKeyJwk)`** — fallible inherent
  constructors that return `Result<Self, ValidationError>`. The
  pre-existing `create_verification_method` /
  `create_service` factories now delegate to these.
- **`PublicKeyJwkWire`** — wire-format shim used by
  `PublicKeyJwk`'s validating `Deserialize` via `#[serde(try_from
  = "PublicKeyJwkWire")]`. Invalid JWKs (OKP with `y`, RSA
  without `y`, private-key material in extensions, ...) are
  now rejected at the serde gate, not silently accepted.
- **`DidDocumentBuilder`** — fluent builder for `DidDocument`
  with cross-reference validation on `build()`: subject DID
  parse, no duplicate verification-method ids, no duplicate
  service ids, every relation entry references an existing
  verification-method.
- **`VerificationError`**, **`ControllerError`** — new domain
  error enums. `ContractError` (already existed) joins them as
  the third domain-grouped enum.

### Changed

- **Upstream primitives reused directly.** The local
  `pub struct ContractAddress(pub String)` and
  `pub struct OffchainStateHashHex(pub String)` shadow newtypes
  are gone. Both are now re-exported from the upstream Midnight
  ledger libraries:
  - `ContractAddress` is `midnight_coin_structure::contract::ContractAddress(pub HashOutput)`
    (re-exported via `compact_runtime::ContractAddress`).
  - `OffchainStateHashHex` is `midnight_base_crypto::hash::HashOutput`.
  In-memory shape is now `[u8; 32]` instead of a 64-char String;
  all upstream derives come along (`FieldRepr` / `FromFieldRepr` /
  `BinaryHashRepr` / `Serializable` / serde / `Zeroize` /
  constant-time eq).
- **`MidnightSubjectId::as_hex(&self) -> &str` →
  `MidnightSubjectId::to_hex(&self) -> String`.** The
  pre-v0.2.0 borrowable `&str` form is impossible because
  storage is now bytes; hex rendering is on demand.
- **`parse_contract_address` returns the upstream
  `ContractAddress` type** via `HashOutputExt::from_hex`.
  Mixed-case input is normalised to lowercase internally.
- **`parse_offchain_state_hash` returns `HashOutput`** via the
  same trait; lowercase invariant unchanged.
- **`create_did(.., secret_key: [u8; 32])`** — the parameter is
  now required. The pre-v0.2.0 `Option<[u8; 32]>` shape silently
  fell back to `[0u8; 32]` when `None` was passed; that footgun
  is gone. The library never decides whether to generate or
  accept key material; callers supply explicit bytes.
- **`ApiError` is now an umbrella with domain-grouped lifts.**
  The flat 13-variant enum is split:
  - `ApiError::Verification(VerificationError)` — relation
    add/remove failures.
  - `ApiError::Controller(ControllerError)` — rotation
    orphaned, invalid secret length, controller/subject
    mismatch.
  - `ApiError::Contract(ContractError)` — on-chain call failures.

  Each domain enum lifts into `ApiError` via `#[from]`, so the
  `?` operator continues to work transparently.

### Removed

- `pub struct ContractAddress(pub String)` shadow newtype.
  Migrate by switching to the upstream type:
  `use compact_runtime::ContractAddress;`. Replace any String
  field access with `addr.to_hex()` (via `HashOutputExt`).
- `pub struct OffchainStateHashHex(pub String)` shadow newtype.
  Same migration via `midnight_base_crypto::hash::HashOutput`.
- `parse_contract_address`, `parse_offchain_state_hash` —
  superseded by `HashOutputExt::from_hex`. The wrappers still
  exist with their original names for now, but return the
  upstream type instead of the String shadow.
- `MidnightSubjectId::as_hex(&self) -> &str` — superseded by
  `to_hex(&self) -> String`.
- `ApiError::ControllerRotationOrphaned`,
  `ApiError::RelationAlreadyContains`,
  `ApiError::RelationMissing`, `ApiError::InvalidSecretKey`,
  `ApiError::ControllerSubjectMismatch` — moved into
  domain-grouped enums (see Changed).

### Deferred to 0.3.0

The two remaining steps from the R1 spec:

- **Step 4b** — make `VerificationMethod` / `Service` /
  `PublicKeyJwk` fields private; add accessor methods. Without
  this, callers can still bypass `::new` by constructing via
  `Foo { id: ..., ... }`. The new `::new` constructors are the
  *recommended* path in 0.2.0; 0.3.0 will make them the *only*
  path.
- **Step 4c** — migrate the ~114 existing direct
  struct-literal construction sites to `::new(NewX)` and retire
  the `create_verification_method` / `create_service` free
  functions. Mechanical mass-replace work that benefits from a
  dedicated session.

### Test coverage

Workspace test count: **144 (pre-R1) → 231 (post-R1)** = +87 new
tests across 4 new integration test files
(`tests/hex_ext.rs`, `tests/ids.rs`, `tests/constructors.rs`,
`tests/did_document_builder.rs`, `tests/error_hierarchy.rs`).
All pre-R1 tests still pass.

### References

- Design spec: [doc/specs/2026-06-23-r1-type-safety-sweep-design.md](doc/specs/2026-06-23-r1-type-safety-sweep-design.md)
- Architecture decision record: [doc/adr/0007-type-safety-sweep.md](doc/adr/0007-type-safety-sweep.md)
- Supersedes (partial): [doc/adr/0002-trait-erasure-for-contract.md](doc/adr/0002-trait-erasure-for-contract.md),
  [doc/adr/0004-private-state-as-trait.md](doc/adr/0004-private-state-as-trait.md)

## [0.1.0] — 2026-06-03

Initial release: TypeScript-port baseline of the Midnight DID
Rust crates (`midnight-did-domain`, `-method`, `-api`, `-cli`,
`-uniffi`, `-runtime`) plus the umbrella `midnight-did` crate.
See `doc/architecture.md` and ADRs 0001–0006.
