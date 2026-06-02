# 2026-06-03 — Autonomous session summary

A snapshot of what landed on `cycle-1-bootstrap` while the maintainer was AFK.
Pushed to `origin/cycle-1-bootstrap`.

## TL;DR

**6 commits**, **~7,500 LOC of Rust shipped**, **126 tests passing**, all
clippy-clean. The patched `compactc --rust` baseline is in the repo; the
Midnight DID **domain + api layers** are ported end-to-end and verified
against TS reference fixtures.

| Layer | Crate | LOC | Tests |
|---|---|---:|---:|
| Codegen baseline | `midnight-did/src/contract/generated.rs` | 1,320 | — |
| Domain | `midnight-did-domain` | 3,100 | 34 |
| API | `midnight-did-api` | 3,112 + tests | 92 (36 unit + 56 integration) |
| **Total** | | **~7,500** | **126** |

`cargo test -p midnight-did-domain -p midnight-did-api` — 126 passed, 0 failed.
`cargo clippy -p midnight-did-domain -p midnight-did-api --tests -- -D warnings` — clean.

## Commits (in order)

1. **`66fd85e`** — `feat(contract): land patched compact-rust codegen baseline + gap inventory`
   - Emits the first `generated.rs` from `did.compact` via the codegen-rust compactc.
   - All types, witnesses, ledger view, initial_state, and 12 non-exported pure
     circuits emit cleanly.
   - 11 exported impure circuit bodies hit codegen gaps clustering into 5
     distinct shape closures (A-E) — stubbed in a working copy so `generated.rs`
     is buildable on its own. Documented in
     [`docs/superpowers/research/2026-06-03-codegen-gaps-did-compact.md`](../research/2026-06-03-codegen-gaps-did-compact.md).
   - Companion: [`docs/superpowers/research/2026-06-03-ts-port-plan.md`](../research/2026-06-03-ts-port-plan.md)
     surveyed the 5,482 LOC of TS source + 7,338 LOC of TS tests and produced the
     port plan that drove all subsequent work.

2. **`aa0fd66`** — `feat(domain): port midnight-did's TS domain layer to Rust`
   - 3,100 LOC pure-data Rust crate `midnight-did-domain`.
   - 34 tests passing (26 unit + 8 ported integration).
   - Ports: DID Document data model + cross-consistency validator
     (zod → thiserror), DID resolver / registrar traits, JWK + public-key
     crypto codecs, MOD1-tagged offchain frame encoder/decoder, URI
     normalisation, Midnight-method types, ledger-utils helpers.
   - **Zero `midnight-*` deps** — immune to the upstream halo2 skew that blocks
     the runtime crate's build.

3. **`5b35fce`** — `feat(api): port midnight-did API + operations layer (DID-5)`
   - 3,112 LOC `midnight-did-api` crate. 36 unit tests passing.
   - Introduces a **`DidContract` async trait** so the API layer is testable
     against an in-memory mock (`RecordingContract`) without needing the real
     contract to compile.
   - Ports: ledger-mappers, controller / VM / service / document operations,
     resolution, private-state, DID subject helpers, network mapping,
     high-level CRUD aggregations (`create_did`, `apply_patch`,
     `rotate_controller_key_with_derivation`, `deactivate_and_clear`).
   - JubjubPoint coords carried as hex strings to avoid a bigint dep here —
     the runtime impl will decode field elements when wired.

4. **`d528f38`** — `test(api): port ledger-mapper + runtime-domain roundtrip tests`
5. **`943c6bc`** — `test(api): port VM + controller + private-state CRUD tests`
6. **`8737619`** — `test(api): TS reference fixtures + end-to-end CRUD interop`
   - +56 integration tests across 6 test files.
   - 3 TS reference fixtures captured (initial state / post-rotate / post-VM-add).
   - Reference-fixture capture command documented in
     `crates/midnight-did-api/tests/fixtures/README.md`.

## TS test port matrix (final)

### Ported (P0)
| TS test | Rust target | Tests | Notes |
|---|---|---:|---|
| `ledger-mappers.test.ts` | `tests/ledger_mappers.rs` | 9 | Bidirectional mapper |
| `runtime-to-domain.test.ts` + `domain-to-runtime.test.ts` | `tests/runtime_domain_roundtrip.rs` | 5 | Round-trip correctness |
| `verification-method-operations.test.ts` + part of `verification-method-relations.test.ts` | `tests/verification_methods.rs` | 11 | VM CRUD + relation purge |
| `controller-operations.test.ts` | `tests/controller.rs` | 4 | Rotation + recovery |
| `private-state.test.ts` | `tests/private_state.rs` | 13 | In-memory store lifecycle |
| `did.api.test.ts` (partial) | `tests/did_api_end_to_end.rs` | 14 | Deterministic CRUD flow + JSON fixture matches |

### Skipped (with rationale)
- `wallet*.test.ts` — wallet SDK couplings, no Rust equivalent yet.
- `seed.test.ts`, `api-logger.test.ts`, `logger-utils.test.ts`, `package-paths.test.ts`, `config.test.ts`, `private-state-storage.test.ts`, `providers.test.ts`, `index.test.ts`, `lib.unit.test.ts`, `types.test.ts` — utilities / barrel files.
- `compatibility-shims.test.ts`, `transaction-intents.test.ts`, `contract-lifecycle-operations.test.ts` — depend on runtime / wallet plumbing.

## TS ↔ Rust interop story so far

### What's proven at this commit
- **Domain types serialize identically.** The 3 TS reference fixtures
  (initial DID doc / post-rotate / post-VM-add) round-trip through
  `serde_json::Value`-equality against TS's `LedgerToDomain.ledgerStateToDIDDocument`
  output.
- **MOD1 offchain frame encoder/decoder is byte-format faithful** (frame, blake2s
  state hash, JWK ↔ key-kind table, 5-bit relationship mask). The inner
  Compact value serializer is abstracted via a `CompactValueCodec` trait so the
  domain crate stays decoupled; the runtime crate plugs in the upstream
  descriptors. Golden vectors deferred to a follow-up.

### What's still needed for full byte-parity (post-blocker)
- **On-chain `ContractState` byte-parity** — requires the contract crate to
  build, which is blocked by the upstream halo2 `ParamsKZG` API skew flagged
  in commit `66fd85e`. Once that's resolved, the existing
  `tests-e2e-rust/tests/codegen_regression.rs` pattern (from the codegen-rust
  PR) drops in cleanly here.
- **11 hand-filled exported circuit bodies** — the codegen-gap report
  documents the 5 shape closures (A-E). Closing A (cell-write in exported
  body) + B (ADT mutation with disclose prologue) unblocks 6 of 11; closing C
  (if-else-if Map mutation) unblocks 3 more. D + E can be hand-shimmed.

## Build state

- `cargo build -p midnight-did-domain` — clean.
- `cargo build -p midnight-did-api` — clean.
- `cargo build -p midnight-did` — **fails**: upstream `midnight-transient-crypto`
  in the Nix-pinned `dioxus-vc-demo` snapshot uses `ParamsKZG::unsafe_setup` /
  `from_parts` / `read_custom` that don't exist on the halo2 version it links
  against. **Not our code** — a Nix flake input refresh (point at a
  midnight-ledger commit where halo2 lines up).
- `cargo test -p midnight-did-domain -p midnight-did-api` — **126 passed**.

## Suggested next iterations

In priority order:

1. **Fix the Nix flake's `midnight-ledger` pin** so `cargo build -p midnight-did`
   succeeds. Could be as simple as bumping `flake.lock` to a commit where halo2
   API matches; or pointing `third_party/midnight-ledger` at the same revision
   the codegen-rust runtime-rs builds against (which is known-green).

2. **Close codegen gap A** (cell-write in exported body — `rotateControllerKey`,
   `deactivate`). Per the report, S/M effort. Unblocks 2 of the 11 stubbed
   circuits.

3. **Hand-write the 11 circuit-body shims** in a sibling module
   `crates/midnight-did/src/contract/extensions.rs`. Each shim uses
   `compact-runtime`'s `OpProgramVerify` builders to construct the same
   transcript the generated code would. Once written + tested, the gap-A/B/C
   codegen closures can replace them mechanically.

4. **Capture additional TS reference fixtures** for the on-chain state
   (`ContractState.serialize()` bytes) so the byte-parity regression guard
   can be wired in once the contract builds.

5. **Wire CI** for the two clean crates: `cargo fmt --check`, `cargo clippy
   -D warnings`, `cargo test -p midnight-did-domain -p midnight-did-api`. The
   `midnight-did` crate stays out of CI until the upstream pin is refreshed.

## Open questions for the maintainer

1. **Crate split.** I created `midnight-did-domain` + `midnight-did-api` as
   sibling crates (port plan recommended 4 — see
   `2026-06-03-ts-port-plan.md`). Should I split further into
   `midnight-did-method` (DID-Core spec layer) + `midnight-did-api` (runtime
   orchestration), or is the current 2-crate split fine?

2. **Async runtime choice.** The `DidContract` trait uses `async-trait`. The
   resolution + registrar traits in `midnight-did-domain` use
   `Pin<Box<dyn Future + Send>>` to stay runtime-agnostic. Confirm Tokio is
   the right concrete choice for downstream users, or should we ship runtime-
   agnostic only?

3. **Crate name `midnight-did`.** The runtime crate is named `midnight-did`
   matching the TS package name. Is that the intended publication name on
   crates.io, or should it move under a `midnight-did-runtime` / similar?

4. **Witness coupling.** The `localSecretKey()` witness reads from
   `PrivateStateStore`. Confirm the interface shape matches what you want for
   the wallet SDK integration, or flag the changes needed.

## Path to upstream merge / publication

Once the build env is fixed:
- `cargo build -p midnight-did` clean.
- `cargo test --workspace` clean.
- Add a `tests-e2e-did` byte-parity crate mirroring the codegen-rust's e2e
  pattern.
- Wire CI workflow.
- Publish `midnight-did-domain` and `midnight-did-api` to crates.io (immune to
  halo2 skew, no upstream deps). `midnight-did` (runtime) follows once compact-
  runtime is on crates.io per the publication plan in the codegen-rust PR.
