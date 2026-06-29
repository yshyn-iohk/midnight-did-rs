<!--
This file is part of midnightntwrk/midnight-did-rs.
Copyright (C) 2026 Midnight Foundation
SPDX-License-Identifier: Apache-2.0
-->

# Contributing to midnight-did-rs

Thank you for considering a contribution! `midnight-did-rs` is the native
Rust port of the Midnight DID Method reference implementation (TypeScript:
[`@midnight-ntwrk/midnight-did`](https://github.com/midnightntwrk/midnight-did)),
aimed at running on every target Rust reaches — native server, desktop,
mobile (via UniFFI), and `wasm32-unknown-unknown` in the browser.

Before diving in, please skim:

- [`doc/architecture.md`](./doc/architecture.md) — the living overview of
  the workspace, the 5-crate split, and the design patterns.
- [`doc/adr/`](./doc/adr/) — Architecture Decision Records covering the
  load-bearing choices (async-only API, `Contract<B: Backend>` shape,
  crate layout, private-state trait, codegen gap handling).

The rest of this guide is the practical dev loop and the conventions
we expect every PR to follow.

---

## Quick start

The repo ships a Nix devshell that pins **every** tool you need
(including the patched Compact compiler that emits Rust). The five-step
dev loop:

1. **Clone + enter the devshell.** With [direnv](https://direnv.net/):

   ```bash
   git clone https://github.com/midnightntwrk/midnight-did-rs.git
   cd midnight-did-rs
   direnv allow            # or: nix develop
   ```

2. **Regenerate the contract crate** from `did.compact`:

   ```bash
   just codegen
   ```

   This invokes the patched `compactc --rust --skip-ts` against
   `third_party/midnight-did/packages/contract/src/did.compact`, copies
   `contract/lib.rs` into `crates/midnight-did/src/contract/generated.rs`,
   and lands the ZKIR / prover / verifier keys under
   `crates/midnight-did/assets/keys/`. See ADR-0005 for why we vendor
   the generated file rather than building it on `cargo build`.

3. **Build + test.**

   ```bash
   just build              # cargo build --all-targets
   just test               # cargo nextest run
   ```

4. **Format + lint.** Both are CI gates.

   ```bash
   just fmt-check          # cargo fmt -- --check ; taplo fmt --check
   just lint               # cargo clippy --all-targets -- -D warnings
   ```

   `just ci` runs the full gate (`fmt-check`, `lint`, `build`, `test`)
   locally — run it before pushing.

5. **Open a PR against `main`.** Keep the diff focused; reference the
   ADR(s) you touched (or argue for a new one if you're changing a
   load-bearing decision).

The `just codegen-check` recipe is a CI regression signal: re-running
codegen must produce no diff. If you've modified `did.compact` or
bumped the Compact toolchain, run `just codegen` and commit the
regenerated `generated.rs` + keys in the same PR.

---

## Branching and commit conventions

- **Branch off `main`.** Topic branches use conventional-commit-style
  names where possible: `feat/<short-desc>`, `fix/<short-desc>`,
  `docs/<short-desc>`, `chore/<short-desc>`, `refactor/<short-desc>`.
- **Do not force-push** to a PR branch under review — reviewers lose
  their place. Push fixup commits and let the maintainer squash on
  merge.
- **Every commit must be GPG-signed and DCO-signed-off.** This matches
  the policy of the upstream
  [`midnightntwrk/midnight-did`](https://github.com/midnightntwrk/midnight-did/blob/main/CONTRIBUTING.md)
  repo and is enforced by CI.

  The canonical incantation:

  ```bash
  git commit -S -s -m "feat: short summary"
  ```

  `-S` attaches a GPG signature; `-s` adds the
  `Signed-off-by: Name <email>` trailer that asserts the
  [Developer Certificate of Origin](https://developercertificate.org/).
  Don't add the trailer manually — `-s` does it for you and a duplicate
  trailer will be flagged by review.

- **Verify the signature** after each commit:

  ```bash
  git log --format="%h %G? %s" -1
  ```

  `G` is good, `B` is bad, `N` is missing. Amend immediately if you
  see anything other than `G`.

---

## Code style

- **`rustfmt`** is the source of truth. `rustfmt.toml` at the repo root
  pins the configuration. `cargo fmt` must be a no-op before commit.
- **`cargo clippy --all-targets -- -D warnings`** is the CI gate. New
  lints land as errors; if you have a principled reason to allow one,
  scope the `#[allow(...)]` as narrowly as possible and explain why in
  a comment.
- **Doc comments on every public item.** `#![warn(missing_docs)]` is
  set workspace-wide. Internal helpers can use `//` comments; public
  API needs `///`.
- **License headers on every new file.** Use the existing source files
  as the template — for example,
  [`crates/midnight-did/src/lib.rs`](./crates/midnight-did/src/lib.rs)
  for `.rs` files and [`doc/architecture.md`](./doc/architecture.md)
  for Markdown. The canonical Rust header is:

  ```rust
  // This file is part of midnightntwrk/midnight-did-rs.
  // Copyright (C) 2026 Midnight Foundation
  // SPDX-License-Identifier: Apache-2.0
  // Licensed under the Apache License, Version 2.0 (the "License");
  // you may not use this file except in compliance with the License.
  ```

  Files for which a header comment is not viable (JSON, fixture
  blobs, etc.) are covered by the repo-level `LICENSE`.

---

## Testing requirements

- **Every behaviour change comes with a test.** Bug fixes need a
  regression test that fails before the fix and passes after.
- **Unit tests** live alongside the code they cover, inside
  `#[cfg(test)] mod tests { ... }` blocks.
- **Integration tests** live under `crates/<crate>/tests/`.
- **TS reference fixtures** under `crates/midnight-did-api/tests/fixtures/`
  drive byte-parity tests against the TypeScript reference. To
  regenerate them, run the TS sandbox in the reference repo and copy
  the output — see the fixture directory's `README.md` for the exact
  steps.
- **Codegen regression.** The `just codegen-check` recipe verifies
  that re-running `just codegen` produces no diff against the
  committed `generated.rs` + keys. A similar cross-language byte-parity
  test (modelled on the upstream `codegen-rust` PR's regression suite)
  will land once `midnight-did-runtime` builds end-to-end.

---

## Crate layout

The workspace is intentionally split so the data-model layers stay free
of any `midnight-*` runtime dep — that's what lets the `domain` and
`api` crates compile on `wasm32-unknown-unknown`. Full dependency graph
is in [`doc/architecture.md`](./doc/architecture.md) §3-§6; ADR-0003
explains the 2-to-4 split.

Quick map for "where do I put this change?":

| You're changing... | Touch this crate |
| --- | --- |
| W3C DID Core types, document model | `midnight-did-domain` |
| Midnight method profile, identifiers, resolution rules | `midnight-did-domain` |
| Async surface, traits, error types consumed by integrators | `midnight-did-api` |
| Compact-emitted contract glue | `crates/midnight-did/src/contract/` (re-run `just codegen`) |
| Native runtime wiring, ledger integration | `midnight-did` (umbrella) — and, once landed, `midnight-did-runtime` |
| CLI behaviour | `midnight-did-cli` |
| FFI / mobile bindings | `midnight-did-uniffi` |

If you're not sure, open an issue and ask before writing code — a
five-minute conversation beats a 500-line refactor request in review.

---

## What we don't accept (yet)

- **Code that requires a live Midnight node.** Use the mock
  `DidContract` fixture instead. Network-dependent tests belong in a
  separate, opt-in suite once we have one.
- **Edits to `crates/midnight-did/src/contract/generated.rs`.** This
  file is auto-emitted by `compactc --rust`. To change it, edit
  `did.compact` upstream (or the codegen passes in
  `third_party/compact`) and re-run `just codegen`. See ADR-0005.
- **`midnight-*` runtime deps in `midnight-did-domain` or
  `midnight-did-api`.** These crates must stay wasm-clean; pulling
  a ledger-side dep into them would break the in-browser deployment
  story. The wasm CI gate enforces this.
- **A `CODE_OF_CONDUCT.md` in this repo.** We inherit the parent
  organisation's code of conduct — please don't add one here.

---

## Questions and help

- **Found a bug or have a feature idea?** Open a
  [GitHub issue](https://github.com/midnightntwrk/midnight-did-rs/issues).
- **ADR-level design question?** Tag the maintainers on the issue or
  draft PR — these decisions are worth getting right early.
- **Security issue?** Don't open a public issue — see
  [`SECURITY.md`](./SECURITY.md).

Thanks again for contributing!
