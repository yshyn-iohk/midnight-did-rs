# Cycle 1 — Bootstrap + Compact Spike Design

**Status:** approved · **Date:** 2026-05-29 · **Author:** Claude (brainstormed with @yshyn-iohk)

This is the first design cycle of the Midnight DID Rust port. It scopes the **bootstrap of `~/iohk/midnight-did-rs`** plus a **focused spike of the patched Compact compiler** (`~/iohk/compact` branch `codegen-rust`). All other work (domain types, DID operations, HTTP API, storage, Docker, integration tests, WASM, docs) is deferred to follow-up cycles, each with its own spec.

## 1. Mission

Produce a minimal, production-grade Cargo workspace that can take `did.compact` from the TS reference and emit a Rust crate that compiles. Nothing else.

**Done-when (single gate):** on a fresh clone, `nix develop` → `just codegen` → `cargo build -p midnight-did` → `nix flake check` all succeed.

**Out of scope (explicit):** see §6.

## 2. Context

| Topic | Reference |
|---|---|
| Target repo | `~/iohk/midnight-did-rs` (branch `main`; already a git repo with remote `yshyn-iohk/midnight-did-rs`) |
| TS source of truth | `~/iohk/midnight-identity-workspace/midnight-did` branch `develop`, packages `{api, contract, did, domain, jubjub-schnorr, logs}` |
| Patched compact compiler | `~/iohk/compact` branch `codegen-rust`. Already has `--skip-ts` flag, byte-parity tests for `counter.compact`, and its own design specs at `docs/superpowers/specs/2026-05-25-rust-codegen-*.md` |
| midnight-ledger crates | Branch `dioxus-vc-demo`. Provides `onchain-runtime`, `zkir`, `transient-crypto`, `serialize`, `base-crypto`, etc. (not on crates.io) |
| Reference Rust project layout | `~/iohk/neoprism` — flake-parts, `nix/` modules, `just`, edition 2024 |

## 3. Decisions (locked)

| # | Decision | Rationale |
|---|---|---|
| D1 | **Ledger deps:** Nix flake input + path-style overlay. midnight-ledger pinned by commit in `flake.lock`; the devshell symlinks the source tree into `third_party/midnight-ledger` and `Cargo.toml` uses `path = "../../third_party/midnight-ledger/<crate>"`. | Reproducible; matches neoprism; single pin for all ledger crates; non-nix workflow is not a goal. |
| D2 | **Compact integration:** Nix derivation of the `codegen-rust` branch, pinned in `flake.lock`. Devshell puts `compact` on PATH; `just codegen` drives it. | Reproducible across contributors and CI; no manual builds. |
| D3 | **Ledger branch pin (cycle 1):** `dioxus-vc-demo`. | Closer to integration target than `ledger-8`. **Risk** if codegen-rust targets a different ledger commit — see §8 R1. |
| D4 | **Spike done-when:** emitted crate compiles via `cargo build`. No byte-parity check, no proof round-trip — those are cycle 2. | Smallest meaningful gate; defers semantic-correctness validation to cycle 2 (knowingly). |
| D5 | **Cycle 1 = one crate (`midnight-did`)**, not a multi-crate split. | YAGNI. Split only when there's a real reason. |
| D6 | **No separate jubjub-schnorr crate ever.** | midnight-ledger already provides `transient-crypto::{schnorr, curve}` and `base-crypto::schnorr` — verified during brainstorming. |
| D7 | **Generated artifacts checked into git** (`crates/midnight-did/src/contract/generated.rs` + `crates/midnight-did/assets/keys/*`). | Lets non-nix readers inspect what the crate exports; re-running codegen produces a deterministic diff → regression signal. |

## 4. Architecture

```
[~/iohk/compact @ codegen-rust]  ───┐
                                    ├─► flake inputs ──► devshell
[~/iohk/midnight-ledger @ dioxus-vc-demo] ─┘                 │
                                                             ▼
[git submodule: midnight-identity-workspace/midnight-did]   compact CLI on PATH
        │                                                   + midnight-ledger crates via overlay
        └─► packages/contract/src/did.compact ──► `just codegen` ──► crates/midnight-did/src/contract/
                                                                    + crates/midnight-did/assets/keys/
                                                                          │
                                                                          ▼
                                                                    `cargo build` ✓
```

## 5. Repo layout

```
~/iohk/midnight-did-rs/
├── .envrc                        # direnv → nix develop
├── .gitignore
├── .gitmodules
├── Cargo.toml                    # workspace, resolver = "3", edition = "2024"
├── README.md                     # exists; rewrite
├── LICENSE                       # exists; keep
├── flake.nix                     # flake-parts; see §6
├── flake.lock
├── justfile                      # codegen, build, test, fmt, lint, ci
├── rustfmt.toml                  # copy from neoprism
├── taplo.toml                    # copy from neoprism
├── nix/
│   ├── devShells.nix             # rust toolchain + compact + ledger overlay + just + nextest
│   ├── checks.nix                # nix flake check: fmt, taplo, clippy, build, codegen-no-diff
│   ├── overlays.nix              # symlinks midnight-ledger source into third_party/midnight-ledger
│   ├── compact.nix               # derivation: build patched compact codegen-rust branch
│   └── rustTools.nix             # pinned rust toolchain
├── crates/
│   └── midnight-did/
│       ├── Cargo.toml            # hand-written; path deps into third_party/midnight-ledger/*
│       ├── src/
│       │   ├── lib.rs            # `pub mod contract;`
│       │   └── contract/
│       │       ├── mod.rs        # re-exports from generated
│       │       └── generated.rs  # produced by `just codegen` (committed)
│       └── assets/
│           └── keys/             # .zkir / .prover / .verifier (committed)
├── docs/
│   └── superpowers/
│       ├── specs/                # this doc + cycle-N specs
│       └── plans/                # writing-plans output
├── third_party/
│   ├── midnight-did/             # git submodule: TS source @ develop
│   └── midnight-ledger/          # symlink, populated by devshell from nix store path
└── .github/
    └── workflows/
        └── ci.yml                # nix-based CI
```

Naming: crate uses `midnight-did` (matches TS package family `@midnight-ntwrk/midnight-did-*`). When future cycles split off domain/api/storage, those crates take the natural `midnight-did-{domain,api,storage,…}` names.

## 6. Flake structure

```nix
inputs = {
  nixpkgs.url           = "github:NixOS/nixpkgs/nixpkgs-unstable";
  rust-overlay.url      = "github:oxalica/rust-overlay";
  flake-parts.url       = "github:hercules-ci/flake-parts";

  midnight-ledger.url   = "github:midnightntwrk/midnight-ledger/dioxus-vc-demo"; # pinned by lock
  midnight-ledger.flake = false;  # consume source; mounted via symlink overlay

  compact.url           = "github:midnightntwrk/compact/codegen-rust";           # pinned by lock
  compact.flake         = false;  # build via nix/compact.nix derivation
};
```

| Module | Provides |
|---|---|
| `nix/devShells.nix` | Default `nix develop` shell. Toolchain: rust (pinned channel via rust-overlay), `compact` on PATH (from `nix/compact.nix`), `just`, `cargo-nextest`, `taplo`, `rustfmt`, `clippy`, `cargo-deny` (optional). Shell hook: ensure `third_party/midnight-ledger` is a symlink to the nix-store path of the `midnight-ledger` flake input; create or refresh on entry. |
| `nix/compact.nix` | Derivation that builds the patched compact compiler from the `compact` flake input. Outputs `${out}/bin/compact`. Largest cycle-1 unknown — Scheme/Racket toolchain. Will extend upstream's existing `~/iohk/compact/flake.nix` if it already packages something on `main`. |
| `nix/overlays.nix` | Exposes the midnight-ledger source path (from the flake input) so other modules and the shellHook can reach it. Actual symlink to `third_party/midnight-ledger` is materialized by the shellHook in `nix/devShells.nix`. |
| `nix/rustTools.nix` | Rust toolchain definition (channel pinned; components: rustfmt, clippy, rust-analyzer). |
| `nix/checks.nix` | `nix flake check` entry: `cargo fmt --check`, `taplo fmt --check`, `cargo clippy -- -D warnings`, `cargo build`, and a `just codegen-clean-check` that re-runs codegen and fails on any diff. |

## 7. Compact codegen pipeline

A single `just codegen` recipe is the entire pipeline:

```just
codegen:
    git submodule update --init third_party/midnight-did
    rm -rf target-gen/contract-out
    compact compile --skip-ts \
        third_party/midnight-did/packages/contract/src/did.compact \
        target-gen/contract-out
    cp target-gen/contract-out/src/lib.rs       crates/midnight-did/src/contract/generated.rs
    mkdir -p crates/midnight-did/assets/keys
    cp target-gen/contract-out/*.zkir           crates/midnight-did/assets/keys/
    cp target-gen/contract-out/*.prover         crates/midnight-did/assets/keys/
    cp target-gen/contract-out/*.verifier       crates/midnight-did/assets/keys/
    cargo fmt -p midnight-did
```

The emitted `Cargo.toml` from compact is intentionally **not** copied — `crates/midnight-did/Cargo.toml` is hand-maintained so deps are explicit and auditable. If compact's emitted Cargo.toml lists deps the hand-written one is missing, `cargo build` fails loudly; the dep is added and committed.

No `build.rs`: codegen is explicit and rare, not implicit on every `cargo build`.

## 8. Spike validation

**Single gate.** On a fresh clone, this sequence succeeds:

```bash
git clone git@github.com:yshyn-iohk/midnight-did-rs.git && cd midnight-did-rs
git submodule update --init --recursive
nix develop
just codegen
cargo build -p midnight-did
nix flake check
```

Acceptance evidence: a green CI run on `main` showing `nix flake check` passing.

Explicitly **not** required for cycle 1: byte-parity of zkir/prover/verifier vs upstream, end-to-end proof round-trip, any DID operation, any HTTP endpoint, any test beyond a placeholder `#[test] fn it_compiles() {}`.

## 9. Out of scope

| Item | Cycle |
|---|---|
| Domain types (DID document, schemas, validation) | 4 |
| DID operations (create / update / deactivate / rotate / add-VM / …) | 5 |
| HTTP API surface (axum) | TBD (its own cycle when surfaced) |
| reddb storage layer | 6 |
| Standalone service binary + Docker (nix-ified à la neoprism) | 7 |
| Integration tests ported from TS | 8 |
| WASM target | 9 |
| Byte-parity validation of zkir keys vs upstream | 2 |
| End-to-end proof round-trip | 2/3 |
| Multi-crate split (`midnight-did-domain`, `midnight-did-api`, etc.) | only when justified |
| Re-pinning to `ledger-8` (mainline) | once `dioxus-vc-demo` merges, or when forced by cycle 2 |

## 10. Testing & quality gates

| Gate | Tool | Runs in |
|---|---|---|
| `cargo fmt --check` | `rustfmt.toml` (copy from neoprism) | `just fmt-check`, `nix flake check`, CI |
| `cargo clippy -- -D warnings` | default lints, deny warnings | `just lint`, `nix flake check`, CI |
| `taplo fmt --check` | `taplo.toml` (copy from neoprism) | `just fmt-check`, CI |
| `cargo build --all-targets` | builds the lone crate | `nix flake check`, CI |
| `cargo test` | placeholder `it_compiles` | CI (real tests start cycle 2) |
| `just codegen && git diff --exit-code` | regression: re-running codegen yields no diff | CI job that has `compact` available |
| DCO + GPG signed commits | follows `~/.claude/CLAUDE.md` global rule (`-S -s`) | enforced at commit time; verified in CI |

GitHub Actions workflow uses `nixos/nix-action` + cachix; no separate Rust action. The single source of truth for "green" is `nix flake check`.

## 11. Risks & fallbacks

| # | Risk | Likelihood | Impact | Fallback |
|---|---|---|---|---|
| R1 | Patched compact targets a different midnight-ledger API than `dioxus-vc-demo` exposes | **High** | Spike stalls — emitted crate won't compile | Read `~/iohk/compact/docs/superpowers/specs/2026-05-25-rust-codegen-design.md` first; if mismatch, either re-pin ledger to whatever codegen targets and accept cycle 1 doesn't land on dioxus-vc-demo, or add a thin adapter module in `src/contract/mod.rs`. |
| R2 | `nix/compact.nix` (Scheme/Racket build) is harder than expected | Medium | Cycle 1 slips on toolchain plumbing | Inspect `~/iohk/compact/flake.nix` — they already have a flake for `main`; extend it to build the `codegen-rust` branch. If still hard, ship a `compact-shell.nix` derived from upstream's. |
| R3 | `git+file://` local-checkout flake inputs don't lock cleanly for CI | Medium | CI can't reproduce | Mirror pinned commits to GitHub URLs (`github:midnightntwrk/midnight-ledger?ref=...`, `github:midnightntwrk/compact?ref=codegen-rust`) before pushing. Spec already uses GitHub URLs in §6. |
| R4 | Committing generated artifacts bloats the repo | Low | Slower clones | zkir keys are small (a few MB at worst); generated.rs is text. Acceptable. Add Git LFS for `assets/keys/` only if it gets out of hand. |
| R5 | No semantic-correctness signal in cycle 1 — green build doesn't prove correct circuits | Medium | False confidence; cycle 2 surprises | Accepted per D4. Mitigate by gating cycle-2 entry on byte-parity check vs the TS `packages/contract/src/managed/did/` artifacts before any porting work begins. |
| R6 | Submodule path `third_party/midnight-did` requires access to a private workspace | Low | Contributor friction | Document `git submodule update --init` step in README; require SSH key access. |

## 12. Cultivated context

The `midnight-identity-rust` global skill at `~/.claude/skills/midnight-identity-rust/SKILL.md` carries the cross-cycle context: repo/path table, TS-to-Rust crate mapping (current and future), compact codegen-rust findings, decision log, conventions. Update it as cycles land and as decisions evolve.

## 13. Next steps after this spec lands

1. User reviews this committed spec; revisions if needed.
2. On approval, invoke `superpowers:writing-plans` to produce a step-by-step implementation plan for cycle 1.
3. Execute plan (likely via `superpowers:subagent-driven-development` for parallel-isolated steps).
4. Open cycle-2 brainstorm once cycle 1 ships green CI.
