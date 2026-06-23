<!--
This file is part of midnightntwrk/midnight-did-rs.
Copyright (C) 2026 Midnight Foundation
SPDX-License-Identifier: Apache-2.0
-->

# R1 — Type-safety sweep (design spec)

**Status:** Approved 2026-06-23. Implementation in progress.
**Supersedes parts of:** [0002 trait-erasure-for-contract](../adr/0002-trait-erasure-for-contract.md) (defers full reform to R2), [0004 private-state-as-trait](../adr/0004-private-state-as-trait.md) (defers reform to R2).
**Target version:** `v0.2.0` across all crates (clean break, no deprecation shims).

## Motivation

The midnight-did-rs codebase was initially ported from `@midnight-ntwrk/midnight-did` (TypeScript), and the port preserved several TS-flavored patterns that don't pay rent in Rust:

1. **Hex-validated `String` newtypes** (`ContractAddress(pub String)`, `OffchainStateHashHex(pub String)`) where upstream `midnight_coin_structure::contract::ContractAddress` and `midnight_base_crypto::hash::HashOutput` already exist with the full Midnight ledger trait stack (`FieldRepr` / `FromFieldRepr` / `BinaryHashRepr` / `Serializable` / serde).
2. **Public-field structs with separate `validate()` methods** — invalid state is representable, and callers can forget the validator.
3. **String-typed W3C DID identifiers** (`DidKeyId`, `ServiceId`, fragment URIs) with leading-`#` checks scattered as runtime conditionals.
4. **Flat `ApiError` enum** (13 variants) where domain-grouped errors would give callers narrower pattern-match targets.
5. **`Option<[u8; 32]> = None → [0u8; 32]` silent default** on `create_did`'s secret-key parameter — a real footgun.

R1 closes (1)–(5) as one coordinated v0.2.0 release. Async patterns, `DidContract` trait shape, and `PrivateStateStore` reform are explicitly deferred to **R2 — Contract abstraction reform**.

## Goals

- **Drop our shadow primitives** where Midnight upstream already covers them (`ContractAddress`, `HashOutput`).
- **Make invalid state unrepresentable**: composite domain types become parse-on-construction, public fields go private, `validate()` methods are removed.
- **Tighten the W3C-DID newtypes** we keep (`DidKeyId`, `FragmentId`, `ServiceId`): private fields, validating `new()`, validating `Deserialize`.
- **Narrow error returns** so callers can pattern-match per-domain instead of stringifying.
- **Eliminate the silent zero-secret-key**: explicit caller-supplied `[u8; 32]`, CLI gains a `--generate-secret` flag.
- **Maintain JSON / on-chain wire compatibility** with the TypeScript reference. The on-the-wire shape stays byte-identical; only the in-memory Rust representation changes.

## Non-goals (R1)

- Typestate machine for active vs deactivated DID (`enum DidState { Active(ActiveDid), Deactivated(DeactivatedDid) }`) — defer to R2.
- Builder pattern for every CRUD operation (only `DidDocumentBuilder` is introduced, for cross-field validation).
- `DidContract` trait collapse and `Backend` trait at the I/O boundary — R2.
- `PrivateStateStore` trait removal — R2.
- Async / sync split — kept async-only per ADR 0001 (reaffirmed by audit; revisit only if WASM optimisation becomes critical).
- Performance work; no benchmarking targets.

## Design

### Section 1 — Primitive types: reuse upstream

**Action: delete these from `midnight-did-method`.**

- `pub struct ContractAddress(pub String)` ([crates/midnight-did-method/src/midnight_did.rs:74](../../crates/midnight-did-method/src/midnight_did.rs))
- `pub struct OffchainStateHashHex(pub String)` (same file)
- `pub fn parse_contract_address(&str) -> Result<ContractAddress, _>`
- `pub fn parse_offchain_state_hash_hex(&str) -> Result<OffchainStateHashHex, _>`
- The string-typed equality/`Display`/`FromStr` ceremony around them.

**Action: replace usage with upstream types.**

- `ContractAddress` → `compact_runtime::ContractAddress` (which re-exports `midnight_coin_structure::contract::ContractAddress(pub HashOutput)`).
- `OffchainStateHashHex` → `midnight_base_crypto::hash::HashOutput([u8; 32])`.

Both upstream types already derive: `Debug + Default + Copy + Clone + Hash + PartialEq + Eq + PartialOrd + Ord + FieldRepr + FromFieldRepr + BinaryHashRepr + Serializable + Serialize + Deserialize + Dummy + Zeroize` (and constant-time eq for `HashOutput`).

**Action: add a thin extension trait module for hex round-tripping.**

Upstream `Display` for `HashOutput` is intentionally truncated (first 10 hex chars only — for human eyeballing in logs). For DID document hex round-trips we need full-hex.

New module `crates/midnight-did-method/src/hex_ext.rs`:

```rust
use compact_runtime::ContractAddress;
use midnight_base_crypto::hash::HashOutput;

#[derive(Debug, thiserror::Error)]
pub enum ParseHexError {
    #[error("expected 64 hex characters, got {0}")]
    WrongLength(usize),
    #[error("invalid hex: {0}")]
    InvalidHex(#[from] hex::FromHexError),
}

/// Round-trips a 32-byte hash type to / from its full 64-character
/// hex string representation. Upstream `Display for HashOutput` is
/// truncated to 10 chars for logs; this trait covers the full hex
/// shape Midnight DID documents use on the JSON wire.
pub trait HashOutputExt: Sized {
    fn from_hex(s: &str) -> Result<Self, ParseHexError>;
    fn to_hex(&self) -> String;
}

impl HashOutputExt for HashOutput {
    fn from_hex(s: &str) -> Result<Self, ParseHexError> {
        if s.len() != 64 {
            return Err(ParseHexError::WrongLength(s.len()));
        }
        let mut buf = [0u8; 32];
        hex::decode_to_slice(s, &mut buf)?;
        Ok(HashOutput(buf))
    }
    fn to_hex(&self) -> String { hex::encode(self.0) }
}

impl HashOutputExt for ContractAddress {
    fn from_hex(s: &str) -> Result<Self, ParseHexError> {
        HashOutput::from_hex(s).map(Self)
    }
    fn to_hex(&self) -> String { self.0.to_hex() }
}
```

Public re-export at `midnight_did_method::hex_ext::HashOutputExt` and re-exported via the umbrella crate.

### Section 2 — W3C newtypes: keep, tighten

These have no Midnight upstream equivalent (the W3C DID URI grammar is identity-stack-specific). We KEEP them, but tighten the construction path.

New module `crates/midnight-did-domain/src/ids.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum IdError {
    #[error("empty identifier")]
    Empty,
    #[error("fragment id must start with '#'")]
    MissingFragmentPrefix,
    #[error("fragment id contains whitespace or control chars")]
    BadFragmentChars,
    #[error("DID key id must contain a '#' fragment")]
    MissingDidFragment,
    #[error("invalid DID URI: {0}")]
    InvalidDidUri(String),
}

/// W3C DID URI with required fragment, e.g. `did:midnight:abc#key-1`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct DidKeyId(String);

impl DidKeyId {
    pub fn new(s: impl Into<String>) -> Result<Self, IdError> { /* validates did: prefix + '#' fragment present */ }
    pub fn as_str(&self) -> &str { &self.0 }
    pub fn fragment(&self) -> &str { /* returns fragment portion */ }
}

impl<'de> Deserialize<'de> for DidKeyId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for DidKeyId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// W3C fragment-relative reference, e.g. `#key-1`. Always begins with `#`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct FragmentId(String);

impl FragmentId {
    pub fn new(s: impl Into<String>) -> Result<Self, IdError> { /* enforces leading '#', rejects whitespace */ }
    pub fn as_str(&self) -> &str { &self.0 }
}

impl<'de> Deserialize<'de> for FragmentId { /* same as DidKeyId */ }
impl std::fmt::Display for FragmentId { /* same as DidKeyId */ }

/// Service identifier — opaque URI in the W3C DID Document `service[].id` slot.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ServiceId(String);

impl ServiceId {
    pub fn new(s: impl Into<String>) -> Result<Self, IdError> { /* rejects empty + whitespace */ }
    pub fn as_str(&self) -> &str { &self.0 }
}

impl<'de> Deserialize<'de> for ServiceId { /* same shape */ }
impl std::fmt::Display for ServiceId { /* same shape */ }
```

**Construction rule:** every newtype has a `new()` returning `Result<Self, IdError>` and a `Deserialize` impl that delegates to `new()`. JSON round-trip therefore validates. Direct `pub` access to the inner string is forbidden.

### Section 3 — Composite types: fallible constructors

Composite domain types replace public-field structs + `validate()` with private-field structs + fallible `new(NewX)`:

| Type | New constructor signature |
|---|---|
| `VerificationMethod` | `pub fn new(NewVerificationMethod) -> Result<Self, VerificationError>` |
| `Service` | `pub fn new(NewService) -> Result<Self, ServiceError>` |
| `PublicKeyJwk` | `pub fn new(kty: KeyType, crv: CurveType, x: String, y: Option<String>) -> Result<Self, VerificationError>` |
| `DidDocument` | `pub fn builder() -> DidDocumentBuilder`; `DidDocumentBuilder::build(self) -> Result<DidDocument, DocumentError>` |

`NewVerificationMethod`, `NewService` are public **parameter structs** (not builders) that hold the raw inputs. Example:

```rust
pub struct NewVerificationMethod {
    pub id: DidKeyId,
    pub controller: DidString,
    pub public_key_jwk: PublicKeyJwk,
    // type_ omitted — always JsonWebKey for now (the only supported variant
    // per assertSupportedVerificationMethod in did.compact)
}

impl VerificationMethod {
    pub fn new(input: NewVerificationMethod) -> Result<Self, VerificationError> {
        // Cross-field checks: JWK kty / crv coherence
        // Output: a VerificationMethod with private fields, accessors via getters
        Ok(VerificationMethod { /* … */ })
    }
}
```

**`DidDocumentBuilder`** is the only true builder — its purpose is to collect a fan-out of verification methods, services, and relations, then validate cross-references (relation entries reference existing VM ids, no duplicates, etc.) on `build()`. Other composite types stay with the `new(NewX)` shape.

Existing `.validate()` methods are removed. The validation runs inside `new()` / `build()`.

### Section 4 — Error hierarchy

Replace the flat `ApiError` (13 variants) with **5 domain-grouped error enums + an umbrella `ApiError`**:

```rust
// crates/midnight-did-api/src/error.rs

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error("verification method already exists: {id}")]
    DuplicateMethod { id: String },
    #[error("verification method not found: {id}")]
    MethodNotFound { id: String },
    #[error("invalid JWK: {0}")]
    InvalidJwk(String),
    #[error("unsupported key type {kty:?} with curve {crv:?}")]
    UnsupportedKey { kty: String, crv: String },
    // … (covers all VM-related variants in the current ApiError)
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("service already exists: {id}")]
    DuplicateService { id: String },
    #[error("service not found: {id}")]
    ServiceNotFound { id: String },
    #[error("invalid service endpoint: {0}")]
    InvalidEndpoint(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ControllerError {
    #[error("controller key mismatch")]
    WrongController,
    #[error("DID is deactivated")]
    DidDeactivated,
}

#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    #[error("ledger RPC failure: {0}")]
    Rpc(String),
    #[error("ledger state deserialization failed: {0}")]
    Decode(String),
}

#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    #[error("validation: {0}")]
    Validation(String),
    #[error("dangling reference: {kind} -> {id}")]
    DanglingReference { kind: &'static str, id: String },
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error(transparent)] Verification(#[from] VerificationError),
    #[error(transparent)] Service(#[from] ServiceError),
    #[error(transparent)] Controller(#[from] ControllerError),
    #[error(transparent)] Contract(#[from] ContractError),
    #[error(transparent)] Document(#[from] DocumentError),
    #[error("private state: {0}")] PrivateState(String),
    #[error("id error: {0}")] Id(#[from] crate::IdError),
}
```

**Function-signature rule:**
- Operations touching one domain return the **narrow** type.
  - `add_verification_method(...) -> Result<(), VerificationError>`
  - `add_service(...) -> Result<(), ServiceError>`
  - `rotate_controller_key(...) -> Result<(), ControllerError>`
- Operations spanning multiple domains return `ApiError`.
  - `create_did(...) -> Result<MidnightDidId, ApiError>` (touches controller + contract + private-state)
  - `apply_patch(...) -> Result<(), ApiError>`
  - `resolve(...) -> Result<DidDocument, ApiError>`

The `#[from]` impls let an operation that internally does verification + service work bubble both up via `?` without manual matching:

```rust
pub async fn apply_patch<C: DidContract>(...) -> Result<(), ApiError> {
    for op in patch.add_verification_methods {
        add_verification_method(/*…*/).await?;  // VerificationError → ApiError via From
    }
    for op in patch.add_services {
        add_service(/*…*/).await?;              // ServiceError → ApiError via From
    }
    Ok(())
}
```

### Section 5 — `create_did` signature

```rust
// Before
pub async fn create_did<C, S>(
    store: &S,
    contract: &C,
    secret_key: Option<[u8; 32]>,     // silently defaults to [0u8; 32]
    address: ContractAddress,
    network: Network,
) -> Result<MidnightDidId, ApiError> where … { … }

// After
pub async fn create_did<C, S>(
    store: &S,
    contract: &C,
    secret_key: [u8; 32],              // required, caller chooses
    address: ContractAddress,
    network: Network,
) -> Result<MidnightDidId, ApiError> where … { … }
```

**CLI change:** [crates/midnight-did-cli/src/](../../crates/midnight-did-cli/src/) grows a `--generate-secret` flag (mutually exclusive with `--secret-key <hex>`). When `--generate-secret` is set, the CLI calls `rand::thread_rng().r#gen::<[u8; 32]>()` and prints the generated key on stderr (so users can save it for later operations).

**Why no library-side RNG**: the library never decides for the caller whether key material is generated or supplied. This is the smallest possible API surface that preserves explicitness; if more callers grow a need for a "generate or accept" sealed enum, that can ride into R2.

## Test strategy

Test coverage is **non-negotiable** for R1 — every behaviour change must land with tests that would have caught the bug if the change had been made wrong. Existing tests stay green; new tests cover new behaviour.

### Coverage targets per section

| Section | New tests | Existing tests touched |
|---|---|---|
| §1 hex_ext + upstream primitive reuse | `hash_output_round_trip_hex`, `contract_address_round_trip_hex`, `wrong_length_rejected`, `non_hex_rejected`, `display_full_64_chars` | All 13 TS byte-parity fixtures in [crates/midnight-did-api/tests/fixtures/](../../crates/midnight-did-api/tests/fixtures/) — must still produce identical JSON. |
| §2 W3C newtypes | `did_key_id_requires_fragment`, `fragment_id_requires_hash_prefix`, `service_id_rejects_empty`, `deserialize_validates_each_type`, `serde_round_trip_each_type` | 34 tests in midnight-did-domain — replace direct `String` constructions with `::new(...)?`. |
| §3 fallible constructors | `verification_method_rejects_kty_crv_mismatch`, `service_rejects_invalid_endpoint`, `did_document_builder_detects_dangling_relation`, `did_document_builder_detects_duplicate_vm_ids`, `validate_method_removed` (negative regression — code that used to call `.validate()` no longer compiles) | All composite-type construction sites in midnight-did-domain + midnight-did-api. |
| §4 error hierarchy | `verification_error_displays`, `service_error_displays`, `umbrella_from_impls_lift_correctly`, `narrow_error_pattern_match` (smoke: `match err { VerificationError::DuplicateMethod { .. } => … }`) | All 102 tests in midnight-did-api — update `Err(ApiError::Variant(...))` matches to either narrow types or `ApiError::Verification(VerificationError::...)`. |
| §5 `create_did` signature | `create_did_accepts_explicit_secret`, `create_did_signature_no_longer_takes_option` (compile-fail test), `cli_generate_secret_flag_works`, `cli_secret_key_hex_flag_works`, `cli_generate_and_secret_key_mutually_exclusive` | CLI test for `cargo run -- run` must still pass after the flag rewire. |

### Test-discipline rules (per CONTRIBUTING.md, TDD ethos)

1. **Tests-first** for new behaviour: write failing test, watch it fail with the expected message, then implement. For pure refactors (re-shaping existing behaviour) the existing tests act as the regression net.
2. **Negative cases mandatory**: every fallible constructor / `Deserialize` impl gets a test that confirms it REJECTS the invalid input — not just that it accepts the valid one.
3. **Wire-format byte-parity**: the 13 TS reference JSON fixtures stay byte-identical after this change. CI must enforce.
4. **Property-style tests for round-trips**: `from_hex(s).to_hex() == s` for all valid `s`. `serde_json::to_string(&value).then(from_str)` for each newtype.
5. **No mock-only tests**: tests exercise real code paths, not just trait-object dispatches.
6. **Public API tests at the crate root**: each crate's `tests/` directory includes at least one end-to-end test that uses ONLY the public API surface — catches accidental private-field leakage.

### CI gate updates

- The existing 3-job GitHub Actions workflow (pre-check, test on Linux+macOS, wasm-build) is unchanged in shape.
- Add a `cargo test --workspace --all-features` invocation specifically (currently only `--workspace`). Captures any feature-gated paths that compile-stale-out otherwise.
- WASM build gate stays as-is; the type-safety sweep doesn't touch `cfg(target_arch = "wasm32")` paths.

## Migration plan

Eight atomic commits, each leaves the workspace green:

| # | Commit | Scope | Verification |
|---|---|---|---|
| **1** | feat(method): add `hex_ext` module + `HashOutputExt` trait | New module. Tests for round-trip, errors. | `cargo test -p midnight-did-method` |
| **2** | refactor(method): drop `ContractAddress` / `OffchainStateHashHex` shadow types, use upstream | Replace all internal use sites. Adjust [midnight-did-runtime/src/contract/generated.rs](../../crates/midnight-did-runtime/src/contract/generated.rs) input parsing (only the boundary). | `cargo test --workspace --exclude midnight-did-runtime` |
| **3** | feat(domain): introduce `DidKeyId`, `FragmentId`, `ServiceId` newtypes in `ids` module | Tests for `::new`, `Deserialize`, errors. No call-site changes yet (added alongside existing types). | `cargo test -p midnight-did-domain` |
| **4** | refactor(domain): switch composite types to newtype-typed fields + fallible `new(NewX)` | Replace `pub String` with `DidKeyId` etc. Make `VerificationMethod::new`, `Service::new`, `PublicKeyJwk::new` fallible. Drop `.validate()`. | `cargo test --workspace` |
| **5** | feat(domain): add `DidDocumentBuilder` for cross-field validation | Builder pattern only for DidDocument; cross-reference checks on `build()`. | `cargo test -p midnight-did-domain` |
| **6** | refactor(api): split `ApiError` into 5 domain-grouped enums + umbrella | Update all operation return types. Narrow types for single-domain operations. | `cargo test -p midnight-did-api` (102 tests rewired) |
| **7** | refactor(api,cli): `create_did` requires explicit `secret_key: [u8; 32]` | CLI grows `--generate-secret` flag (mutually exclusive with `--secret-key`). Library never decides. | `cargo run -p midnight-did-cli -- run --generate-secret` end-to-end |
| **8** | release: bump all crate versions to 0.2.0 + CHANGELOG entry | Single commit covering all 7 crates. Update `=0.1.0` lockstep pins to `=0.2.0`. | `cargo publish --dry-run -p midnight-did-domain` (and chain) |

Each commit must independently pass `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.

## Documentation deliverables

- This spec, committed to `doc/specs/2026-06-23-r1-type-safety-sweep-design.md`.
- New ADR `doc/adr/0007-type-safety-sweep.md` summarising decisions + supersession of parts of 0002 / 0004.
- `CHANGELOG.md` entry under `[0.2.0]` with breaking-change list.
- README updates to the affected crates: link to migration notes for any downstream consumer.

## Rollback

If R1 turns out to introduce a hidden interop break (e.g. some byte-parity fixture diverges), each commit is independently revertable. The most likely revert target is commit **#2** (upstream primitive switch) — keeping our shadow types as a fallback is mechanically straightforward. The other commits are pure surface-shape refactors that don't touch wire format.

## Out of scope (explicit non-decisions)

- We do NOT add a `rand` dep to any library crate. The CLI gets one; the library remains RNG-free.
- We do NOT change how witnesses are passed. ADR 0004's `PrivateStateStore` trait stays unchanged; R2 considers reform.
- We do NOT touch the codegen-emitted `crates/midnight-did-runtime/src/contract/generated.rs` body except at the input-parsing boundary in commit #2 (changing the `String` arg to upstream `ContractAddress`). Walker gap A6+ work in compact's codegen-rust branch is independent.
- We do NOT add `bech32` parsing for `ContractAddress`. Hex is the only wire format used today. If bech32 becomes needed, that's a separate extension to `hex_ext`.

## References

- [ADR 0001 — async-only API](../adr/0001-async-only-api.md) (kept, reaffirmed by audit)
- [ADR 0002 — trait erasure for contract](../adr/0002-trait-erasure-for-contract.md) (parts deferred to R2)
- [ADR 0003 — crate split 2 → 4 + umbrella](../adr/0003-crate-split-2-to-4-with-umbrella.md) (unchanged)
- [ADR 0004 — private state as trait](../adr/0004-private-state-as-trait.md) (parts deferred to R2)
- [ADR 0005 — codegen gap handling](../adr/0005-codegen-gap-handling.md) (unchanged)
- [ADR 0006 — runtime crate halo2 block](../adr/0006-runtime-crate-halo2-block.md) (unchanged)
- Upstream `midnight_base_crypto::hash::HashOutput` (third_party/midnight-ledger/base-crypto/src/hash.rs:48)
- Upstream `midnight_coin_structure::contract::ContractAddress` (third_party/midnight-ledger/coin-structure/src/contract.rs:49)
