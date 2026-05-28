# Cycle 1 — Bootstrap + Compact Spike Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Cargo workspace at `~/iohk/midnight-did-rs` whose lone crate is generated from `did.compact` by the patched Compact compiler (codegen-rust branch), driven by Nix, that compiles cleanly under `cargo build` and `nix flake check`.

**Architecture:** flake-parts with two pinned inputs: `midnight-ledger@dioxus-vc-demo` (Rust crates exposed via symlink overlay to `third_party/midnight-ledger`) and `compact@codegen-rust` (compiler binary built by `nix/compact.nix`; runtime helper crate exposed via symlink overlay to `third_party/compact-runtime-rs`). `just codegen` runs `compact compile --skip-ts third_party/midnight-did/packages/contract/src/did.compact …` and slices the emitted Rust into `crates/midnight-did/`.

**Tech Stack:** Rust nightly-2026-03-18 (matching neoprism), Cargo workspace edition 2024 resolver 3, Nix flakes with flake-parts, rust-overlay, `just`, `cargo-nextest`, `taplo`, GitHub Actions w/ DeterminateSystems/nix-installer-action.

**Pre-flight findings (verified during planning):**
- `~/iohk/midnight-did-rs` exists, branch `main`, remote `yshyn-iohk/midnight-did-rs.git`, currently contains only `LICENSE` + `README.md` + one `Initial commit` plus the design doc commit (`daa4d80`). DCO+GPG config is already set.
- The patched compact (`origin/codegen-rust`) is forward of `main` by ~20 commits, all on Rust emission. Its flake input pins `midnight-ledger@ledger-8.0.2`. `dioxus-vc-demo` is `ledger-8.0.2 + JS-only fixes` (merge-base = ledger-8.0.2 commit), so the Rust crate API surface matches.
- The emitted Rust crate depends on `compact-runtime = { path = "../runtime-rs" }`. That crate lives inside the compact repo at `runtime-rs/`, so our flake exposes it the same way we expose midnight-ledger crates.
- midnight-ledger Cargo packages use `midnight-` prefix in `[package] name` (e.g. `midnight-onchain-runtime`, `midnight-base-crypto`, `midnight-zkir`, `midnight-serialize`). Verify before writing path deps — directory names omit the prefix.

---

## File map

| Path (under `~/iohk/midnight-did-rs/`) | Created in | Purpose |
|---|---|---|
| `.envrc` | T1 | `use flake` for direnv |
| `.gitignore` | T1 | exclude `target/`, `target-gen/`, `result`, `result-*`, `.direnv/`, `third_party/midnight-ledger` (symlink), `third_party/compact-runtime-rs` (symlink) |
| `.gitmodules` | T1 | references TS source submodule |
| `rustfmt.toml` | T1 | copy of neoprism's |
| `taplo.toml` | T1 | copy of neoprism's |
| `third_party/midnight-did/` | T1 | git submodule of `midnight-identity-workspace/midnight-did@develop` |
| `Cargo.toml` (workspace) | T2 | resolver 3, edition 2024, `members = ["crates/midnight-did"]` |
| `crates/midnight-did/Cargo.toml` | T2 | placeholder; just `[package] name="midnight-did"` |
| `crates/midnight-did/src/lib.rs` | T2 | `pub mod contract;` once codegen runs; placeholder until then |
| `flake.nix` | T3 | flake-parts entry, imports `nix/*.nix` |
| `nix/rustTools.nix` | T3 | rust toolchain pin |
| `nix/devShells.nix` | T3 | default devshell + shellHook for symlink materialization |
| `nix/checks.nix` | T9 | `nix flake check` entry |
| `nix/overlays.nix` | T4 | exposes midnight-ledger + compact-runtime-rs source paths |
| `nix/compact.nix` | T5 | derivation that builds patched compact binary |
| `justfile` | T7 | `codegen`, `build`, `test`, `fmt`, `fmt-check`, `lint`, `ci` recipes |
| `crates/midnight-did/src/contract/mod.rs` | T7 | re-exports from `generated` |
| `crates/midnight-did/src/contract/generated.rs` | T8 (auto) | populated by `just codegen` |
| `crates/midnight-did/assets/keys/*.{zkir,prover,verifier}` | T8 (auto) | populated by `just codegen` |
| `.github/workflows/ci.yml` | T10 | nix-based CI |
| `README.md` | T11 | rewrite from placeholder |

`docs/superpowers/specs/2026-05-29-cycle-1-bootstrap-compact-spike-design.md` already exists (committed `daa4d80`).

---

## Conventions for every commit in this plan

```bash
git commit -S -s -m "<type>(<scope>): <subject>" -m "<body>" -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

The `-S` enables GPG, `-s` adds DCO. The repo's git config is already set up.
**Do NOT** add `Signed-off-by:` manually in the message body — `-s` adds it once; including it twice creates duplicate trailers.

After every commit:
```bash
git log --format="%h %G? %s" -1
```
Expect leading column `G` (good signature). If you see `B` or `N`, amend immediately: `git commit --amend --no-edit -S -s`.

---

## Task 1: Repo skeleton + TS source submodule

**Files:**
- Create: `~/iohk/midnight-did-rs/.envrc`
- Create: `~/iohk/midnight-did-rs/.gitignore`
- Create: `~/iohk/midnight-did-rs/rustfmt.toml`
- Create: `~/iohk/midnight-did-rs/taplo.toml`
- Create (submodule init): `~/iohk/midnight-did-rs/.gitmodules`, `~/iohk/midnight-did-rs/third_party/midnight-did`

- [ ] **Step 1.1: `cd ~/iohk/midnight-did-rs` and confirm clean state**

Run:
```bash
cd ~/iohk/midnight-did-rs && git status
```
Expected: `On branch main … nothing to commit, working tree clean` (after the spec was committed in `daa4d80`).

- [ ] **Step 1.2: Write `.envrc`**

```
use flake
```

- [ ] **Step 1.3: Write `.gitignore`**

```gitignore
# Cargo
/target
/target-gen
**/*.rs.bk

# Nix build outputs
/result
/result-*
.direnv

# Symlinks materialised by the devshell (not vendored)
/third_party/midnight-ledger
/third_party/compact-runtime-rs

# Editor / OS
.idea
.vscode
.DS_Store
```

- [ ] **Step 1.4: Write `rustfmt.toml`** (verbatim copy of neoprism's, already verified during planning)

```toml
max_width = 120
edition   = "2024"

format_code_in_doc_comments = true

imports_granularity = "Module"
group_imports       = "StdExternalCrate"
```

- [ ] **Step 1.5: Write `taplo.toml`** (verbatim copy of neoprism's)

```toml
[formatting]
align_entries       = true
column_width        = 100
allowed_blank_lines = 1
indent_string       = "  "
array_auto_collapse = false
array_auto_expand   = false
compact_arrays      = false
```

- [ ] **Step 1.6: Add the TS source as a submodule**

Run:
```bash
cd ~/iohk/midnight-did-rs
git submodule add -b develop \
  /Users/ysh/iohk/midnight-identity-workspace/midnight-did \
  third_party/midnight-did
```

Expected: creates `.gitmodules` and clones the submodule.

> **Note:** local `file://`-style submodule URLs work for development but cannot be cloned by CI. Once the spike runs locally, switch the URL to the public GitHub URL of the TS repo by editing `.gitmodules` and running `git submodule sync`. Defer this to T10 where CI is wired.

- [ ] **Step 1.7: Verify the submodule contains `did.compact`**

Run:
```bash
ls third_party/midnight-did/packages/contract/src/did.compact
```
Expected: file exists.

- [ ] **Step 1.8: Commit**

```bash
git add .envrc .gitignore rustfmt.toml taplo.toml .gitmodules third_party/midnight-did
git commit -S -s -m "chore(skel): add .envrc, .gitignore, fmt configs, TS source submodule" \
  -m "Establishes the repo skeleton for cycle 1. The TS midnight-did is mounted as a submodule under third_party/midnight-did so the patched compact compiler can read packages/contract/src/did.compact." \
  -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
git log --format="%h %G? %s" -1
```
Expected output: `<hash> G chore(skel): …`

---

## Task 2: Workspace + empty `midnight-did` crate

**Files:**
- Create: `~/iohk/midnight-did-rs/Cargo.toml`
- Create: `~/iohk/midnight-did-rs/crates/midnight-did/Cargo.toml`
- Create: `~/iohk/midnight-did-rs/crates/midnight-did/src/lib.rs`

- [ ] **Step 2.1: Write workspace `Cargo.toml`**

```toml
[workspace]
resolver = "3"
members  = ["crates/midnight-did"]

[workspace.package]
version      = "0.1.0"
edition      = "2024"
rust-version = "1.85"
license      = "Apache-2.0"
authors      = ["Midnight Foundation"]
repository   = "https://github.com/yshyn-iohk/midnight-did-rs"

[workspace.dependencies]
# midnight-ledger crates (path-mounted by devshell into third_party/midnight-ledger)
midnight-base-crypto      = { path = "third_party/midnight-ledger/base-crypto" }
midnight-coin-structure   = { path = "third_party/midnight-ledger/coin-structure" }
midnight-onchain-runtime  = { path = "third_party/midnight-ledger/onchain-runtime" }
midnight-onchain-state    = { path = "third_party/midnight-ledger/onchain-state" }
midnight-onchain-vm       = { path = "third_party/midnight-ledger/onchain-vm" }
midnight-serialize        = { path = "third_party/midnight-ledger/serialize" }
midnight-storage          = { path = "third_party/midnight-ledger/storage" }
midnight-transient-crypto = { path = "third_party/midnight-ledger/transient-crypto" }
midnight-zkir             = { path = "third_party/midnight-ledger/zkir" }
# compact's runtime helper crate (path-mounted by devshell into third_party/compact-runtime-rs)
compact-runtime           = { path = "third_party/compact-runtime-rs" }
# common
serde      = { version = "1", features = ["derive"] }
serde_json = "1"
hex        = "0.4"

[profile.dev]
debug = "limited"

[profile.release]
lto       = "thin"
codegen-units = 1
```

> **Note:** the `[package] name` of each midnight-ledger crate must match the workspace dep key. Verify by reading `~/iohk/midnight-ledger/<crate-dir>/Cargo.toml` `[package].name` in the next step. If a name differs (e.g. directory `base-crypto` produces package `midnight-base-crypto` — confirmed at planning time), the path dep works as-is. If any crate's package name is different from `midnight-<dir>`, adjust the workspace dep key here.

- [ ] **Step 2.2: Confirm midnight-ledger package names match**

Run:
```bash
for d in base-crypto coin-structure onchain-runtime onchain-state onchain-vm serialize storage transient-crypto zkir; do
  echo -n "$d -> "
  grep -m1 '^name' ~/iohk/midnight-ledger/$d/Cargo.toml
done
```
Expected: each prints e.g. `base-crypto -> name = "midnight-base-crypto"`. If any deviation, fix Cargo.toml in Step 2.1 to match.

- [ ] **Step 2.3: Write `crates/midnight-did/Cargo.toml` (placeholder, no deps yet)**

```toml
[package]
name         = "midnight-did"
version.workspace      = true
edition.workspace      = true
rust-version.workspace = true
license.workspace      = true
authors.workspace      = true
repository.workspace   = true
description  = "Native Rust implementation of the Midnight DID Method."

[lib]
path = "src/lib.rs"
```

- [ ] **Step 2.4: Write `crates/midnight-did/src/lib.rs` (placeholder)**

```rust
//! Native Rust implementation of the Midnight DID Method.
//!
//! Cycle 1: scaffold only. The `contract` module is populated by `just codegen`.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms, clippy::all)]

/// Crate version reported by the build.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_compiles() {
        assert!(!VERSION.is_empty());
    }
}
```

- [ ] **Step 2.5: Commit**

```bash
git add Cargo.toml crates/midnight-did/Cargo.toml crates/midnight-did/src/lib.rs
git commit -S -s -m "chore(workspace): add Cargo workspace + empty midnight-did crate" \
  -m "Single-member workspace at resolver=3, edition=2024. The midnight-did crate is a placeholder; its src/contract/ module will be populated by 'just codegen' in T7-T8. Workspace deps point at third_party/midnight-ledger and third_party/compact-runtime-rs, which are symlinks materialised by the nix devshell in T4-T6." \
  -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
git log --format="%h %G? %s" -1
```

> No `cargo build` verification yet — the path deps don't resolve until T4 materializes the symlinks. We commit a known-broken state because the next task fixes it.

---

## Task 3: Flake skeleton + rust toolchain

**Files:**
- Create: `~/iohk/midnight-did-rs/flake.nix`
- Create: `~/iohk/midnight-did-rs/nix/rustTools.nix`
- Create: `~/iohk/midnight-did-rs/nix/devShells.nix`

- [ ] **Step 3.1: Write `flake.nix`** (minimal — only rust toolchain; midnight-ledger and compact added in T4 and T5)

```nix
{
  description = "Midnight DID — native Rust implementation";

  nixConfig = {
    extra-substituters     = [ "https://cache.iog.io" ];
    extra-trusted-public-keys = [ "hydra.iohk.io:f/Ea+s+dFdN+3Y/G+FDgSq+a5NEWhJGzdjvKNGv0/EQ=" ];
  };

  inputs = {
    nixpkgs.url      = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-parts.url  = "github:hercules-ci/flake-parts";
  };

  outputs =
    { nixpkgs, rust-overlay, flake-parts, ... }@inputs:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-darwin"
      ];

      imports = [
        ./nix/devShells.nix
      ];

      perSystem =
        { system, ... }:
        {
          _module.args = {
            inherit rust-overlay;
            pkgs = import nixpkgs {
              inherit system;
              overlays = [ (import rust-overlay) ];
            };
            midnightDidRsLib = {
              rustTools = import ./nix/rustTools.nix {
                rust-bin     = (import nixpkgs { inherit system; overlays = [ (import rust-overlay) ]; }).rust-bin;
                rust-overlay = rust-overlay;
              };
            };
          };
        };
    };
}
```

- [ ] **Step 3.2: Write `nix/rustTools.nix`** (modeled on neoprism, same nightly pin)

```nix
{ rust-bin, rust-overlay }:

let
  nightlyVersion = "2026-03-18";
  rustOverrideArgs = {
    extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
    targets    = [ ];
  };
in
{
  rust =
    rust-bin.nightly.${nightlyVersion}.default.override rustOverrideArgs;
}
```

- [ ] **Step 3.3: Write `nix/devShells.nix`** (minimal — rust, just, taplo only; symlinks added in T4)

```nix
{ ... }:
{
  perSystem =
    { pkgs, midnightDidRsLib, ... }:
    let
      inherit (midnightDidRsLib.rustTools) rust;
    in
    {
      devShells.default = pkgs.mkShell {
        packages = with pkgs; [
          rust
          just
          taplo
          cargo-nextest
          git
          jq
        ];

        shellHook = ''
          export ROOT_DIR=$(${pkgs.git}/bin/git rev-parse --show-toplevel)
          cd "$ROOT_DIR"
          echo "Entered midnight-did-rs devshell. Run 'just --list' for available commands."
        '';

        env = {
          RUST_LOG = "info";
        };
      };
    };
}
```

- [ ] **Step 3.4: Generate `flake.lock`**

Run:
```bash
cd ~/iohk/midnight-did-rs
nix flake lock --extra-experimental-features "nix-command flakes"
```
Expected: writes `flake.lock`. If errors mention auth, retry with `--no-write-lock-file --recreate-lock-file`. If errors mention input fetching, network/cache issue — retry.

- [ ] **Step 3.5: Verify devshell enters and rust toolchain is available**

Run:
```bash
nix --extra-experimental-features "nix-command flakes" develop --command bash -c 'rustc --version && cargo --version && just --version && taplo --version'
```
Expected: 4 version lines, no errors. `rustc` should be `nightly-2026-03-18` build.

- [ ] **Step 3.6: Commit**

```bash
git add flake.nix flake.lock nix/rustTools.nix nix/devShells.nix
git commit -S -s -m "chore(nix): add flake skeleton + rust nightly-2026-03-18 toolchain" \
  -m "flake-parts based; matches neoprism's rust pin. Devshell currently exposes rust + just + taplo. Subsequent tasks add midnight-ledger overlay (T4), compact derivation (T5), and compact-runtime overlay (T6)." \
  -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
git log --format="%h %G? %s" -1
```

---

## Task 4: midnight-ledger flake input + overlay symlink

**Files:**
- Modify: `~/iohk/midnight-did-rs/flake.nix`
- Create: `~/iohk/midnight-did-rs/nix/overlays.nix`
- Modify: `~/iohk/midnight-did-rs/nix/devShells.nix`

- [ ] **Step 4.1: Add `midnight-ledger` as a flake input**

Edit `flake.nix`:
- In `inputs = { … }`, append:
  ```nix
  midnight-ledger = {
    url = "github:midnightntwrk/midnight-ledger/dioxus-vc-demo";
    flake = false;
  };
  ```
- In `outputs = { nixpkgs, rust-overlay, flake-parts, ... }@inputs`, append `midnight-ledger,` to the destructure (or rely on the `...@inputs` capture).
- In `perSystem._module.args`, expose:
  ```nix
  midnightDidRsLib = {
    rustTools = import ./nix/rustTools.nix { /* … as before */ };
    sources = {
      midnight-ledger = inputs.midnight-ledger;
    };
  };
  ```
- In `imports`, append `./nix/overlays.nix`.

- [ ] **Step 4.2: Write `nix/overlays.nix`**

```nix
{ ... }:
{
  perSystem =
    { pkgs, midnightDidRsLib, ... }:
    {
      # Nothing to declare at flake-module level for now. The symlink to
      # third_party/midnight-ledger is materialised by the devShells shellHook
      # using midnightDidRsLib.sources.midnight-ledger.
      _module.args.midnightLedgerSrc = midnightDidRsLib.sources.midnight-ledger;
    };
}
```

- [ ] **Step 4.3: Update `nix/devShells.nix` shellHook to materialize the symlink**

Replace `shellHook` block with:

```nix
shellHook = ''
  export ROOT_DIR=$(${pkgs.git}/bin/git rev-parse --show-toplevel)
  cd "$ROOT_DIR"

  # Materialize third_party/midnight-ledger as a symlink to the nix-store path.
  TARGET="${midnightLedgerSrc}"
  LINK="$ROOT_DIR/third_party/midnight-ledger"
  mkdir -p "$ROOT_DIR/third_party"
  if [ -L "$LINK" ] && [ "$(readlink "$LINK")" = "$TARGET" ]; then
    :
  else
    rm -rf "$LINK"
    ln -s "$TARGET" "$LINK"
    echo "Linked $LINK -> $TARGET"
  fi

  echo "Entered midnight-did-rs devshell. Run 'just --list' for available commands."
'';
```

> The `midnightLedgerSrc` argument is now visible because `overlays.nix` sets `_module.args.midnightLedgerSrc`. Add it to the destructure: `{ pkgs, midnightDidRsLib, midnightLedgerSrc, ... }`.

- [ ] **Step 4.4: Re-lock and re-enter the devshell**

```bash
cd ~/iohk/midnight-did-rs
nix --extra-experimental-features "nix-command flakes" flake lock
nix --extra-experimental-features "nix-command flakes" develop --command bash -c '
  ls -la third_party/midnight-ledger
  ls third_party/midnight-ledger | head
'
```
Expected: symlink resolves to `/nix/store/…-source`, ledger crates visible (`base-crypto`, `coin-structure`, `ledger`, `onchain-runtime`, etc.).

- [ ] **Step 4.5: Verify `cargo build` of empty crate now resolves path deps**

The crate doesn't yet *use* any ledger types, but the path deps must at least resolve.

Run inside the devshell:
```bash
nix --extra-experimental-features "nix-command flakes" develop --command bash -c 'cargo build -p midnight-did'
```

Expected: builds cleanly. If `cargo build` fails with "could not find Cargo.toml in …midnight-ledger/<crate>", the symlink isn't resolving or a package name in workspace.dependencies doesn't match the on-disk crate name — re-run Step 2.2.

- [ ] **Step 4.6: Commit**

```bash
git add flake.nix flake.lock nix/overlays.nix nix/devShells.nix
git commit -S -s -m "feat(nix): pin midnight-ledger@dioxus-vc-demo + materialize overlay symlink" \
  -m "Adds midnight-ledger as a non-flake input; devshell shellHook materializes third_party/midnight-ledger as a symlink to the nix-store source path so Cargo path deps resolve. dioxus-vc-demo is ledger-8.0.2 + JS-only fixes, so the Rust API surface matches what the patched compact targets." \
  -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
git log --format="%h %G? %s" -1
```

---

## Task 5: Compact compiler derivation

This is the highest-risk task. Strategy: extend or reuse the upstream compact flake's own packaging.

**Files:**
- Create: `~/iohk/midnight-did-rs/nix/compact.nix`
- Modify: `~/iohk/midnight-did-rs/flake.nix` (add compact input)
- Modify: `~/iohk/midnight-did-rs/nix/devShells.nix` (put compact on PATH)

- [ ] **Step 5.1: Reconnaissance — read compact's flake outputs**

Run:
```bash
cd ~/iohk/compact
git show origin/codegen-rust:flake.nix | wc -l
nix --extra-experimental-features "nix-command flakes" flake show github:midnightntwrk/compact/codegen-rust 2>&1 | head -40
```
Expected: enumerates packages exposed by the flake. Look for a package named like `compact`, `compactc`, or `compact-compiler` for the current system. Record the exact attribute path.

- [ ] **Step 5.2: Add compact as a flake input**

Edit `flake.nix` inputs:
```nix
compact = {
  url = "github:midnightntwrk/compact/codegen-rust";
};
```

Note: this IS a flake (the upstream `flake.nix` exists). We consume its outputs directly.

- [ ] **Step 5.3: Write `nix/compact.nix` — re-export the upstream package**

```nix
{ ... }:
{
  perSystem =
    { system, ... }:
    {
      _module.args.compactPkg = inputs.compact.packages.${system}.<attr-from-step-5.1>;
    };
}
```

Replace `<attr-from-step-5.1>` with the actual package name found in Step 5.1 (likely `compact` or `compactc`). If the upstream flake doesn't expose a binary package for the current system, fall back to the next step.

- [ ] **Step 5.3a (fallback if 5.3 doesn't expose a usable package): write our own derivation**

Inspect `~/iohk/compact/flake.nix` lines around `outputs = { … }: …` to find how upstream builds compact. Mirror that derivation here. Likely it uses `chez-exe` (Chez Scheme native compilation). Concretely:

```nix
{ pkgs, inputs, system, ... }:

let
  chez-exe = inputs.compact.inputs.chez-exe.packages.${system}.default;
  compactSrc = inputs.compact;
in
pkgs.stdenv.mkDerivation {
  pname   = "compact";
  version = "codegen-rust";
  src     = compactSrc;
  nativeBuildInputs = [ chez-exe pkgs.makeWrapper ];
  buildPhase = ''
    # invoke compact repo's own build, e.g. via make / a script under compiler/
    # exact command will be discovered during reconnaissance — copy from
    # upstream's derivation buildPhase.
    make -C compiler
  '';
  installPhase = ''
    install -Dm755 compiler/compact $out/bin/compact
  '';
}
```

> If this path is needed, **read `~/iohk/compact/flake.nix` carefully and copy its build steps verbatim**. Do not guess.

- [ ] **Step 5.4: Add compact to devshell**

Edit `nix/devShells.nix`:
- Add `compactPkg` to the perSystem destructure: `{ pkgs, midnightDidRsLib, midnightLedgerSrc, compactPkg, ... }`
- In `packages = with pkgs; [ … ]`, append `compactPkg` (outside the `with pkgs` block):
  ```nix
  packages = [ compactPkg ] ++ (with pkgs; [
    rust
    just
    taplo
    cargo-nextest
    git
    jq
  ]);
  ```
- In `imports`, ensure `flake.nix` adds `./nix/compact.nix`.

- [ ] **Step 5.5: Re-lock and verify `compact --version`**

```bash
cd ~/iohk/midnight-did-rs
nix --extra-experimental-features "nix-command flakes" flake lock
nix --extra-experimental-features "nix-command flakes" develop --command bash -c '
  which compact
  compact --version
  compact --help | head -20
'
```
Expected: prints a compact binary path inside `/nix/store/…`, a version string, and help text mentioning `--skip-ts` flag.

- [ ] **Step 5.6: If `compact --version` works, commit**

```bash
git add flake.nix flake.lock nix/compact.nix nix/devShells.nix
git commit -S -s -m "feat(nix): pin compact@codegen-rust + expose binary in devshell" \
  -m "Pins the patched compact compiler (Rust-emitting fork) by commit. Uses upstream's own derivation where available; otherwise falls back to a local derivation that mirrors upstream's build steps. 'compact --version' is available inside 'nix develop'." \
  -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
git log --format="%h %G? %s" -1
```

> **Fallback if T5 stalls:** If after a reasonable timebox (say 2-3 hours of investigation) the compact derivation refuses to build, **pause and escalate** by writing a `docs/superpowers/notes/2026-05-29-compact-derivation-blocker.md` describing what was tried, what failed, and what's needed (e.g. a fix in upstream's flake). Do not proceed to T6-T10 — those depend on `compact` being on PATH.

---

## Task 6: compact-runtime-rs overlay

**Files:**
- Modify: `~/iohk/midnight-did-rs/nix/overlays.nix`
- Modify: `~/iohk/midnight-did-rs/nix/devShells.nix`

- [ ] **Step 6.1: Expose compact's `runtime-rs/` subtree**

In `nix/overlays.nix`, set:

```nix
_module.args.compactRuntimeRsSrc = "${inputs.compact}/runtime-rs";
```

- [ ] **Step 6.2: Materialize symlink in devshell**

In `nix/devShells.nix` shellHook, after the midnight-ledger symlink block, add:

```bash
TARGET="${compactRuntimeRsSrc}"
LINK="$ROOT_DIR/third_party/compact-runtime-rs"
if [ -L "$LINK" ] && [ "$(readlink "$LINK")" = "$TARGET" ]; then
  :
else
  rm -rf "$LINK"
  ln -s "$TARGET" "$LINK"
  echo "Linked $LINK -> $TARGET"
fi
```

Also add `compactRuntimeRsSrc` to the perSystem destructure.

- [ ] **Step 6.3: Verify the symlink resolves and contains `Cargo.toml`**

```bash
nix --extra-experimental-features "nix-command flakes" develop --command bash -c '
  ls -la third_party/compact-runtime-rs
  head -5 third_party/compact-runtime-rs/Cargo.toml
'
```
Expected: symlink resolves; `Cargo.toml` `[package] name = "compact-runtime"`.

- [ ] **Step 6.4: Verify `cargo build -p midnight-did` still passes**

```bash
nix --extra-experimental-features "nix-command flakes" develop --command bash -c 'cargo build -p midnight-did'
```
Expected: builds cleanly. The path dep on `compact-runtime` now resolves even though `midnight-did` doesn't yet use it.

- [ ] **Step 6.5: Commit**

```bash
git add nix/overlays.nix nix/devShells.nix
git commit -S -s -m "feat(nix): expose compact runtime-rs via third_party symlink" \
  -m "The patched compact emits Rust code that depends on compact-runtime (path = ../runtime-rs in the emitted Cargo.toml). We mount the compact repo's runtime-rs subdir as third_party/compact-runtime-rs so our hand-written Cargo.toml can declare it as a path dep." \
  -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
git log --format="%h %G? %s" -1
```

---

## Task 7: `just codegen` recipe

**Files:**
- Create: `~/iohk/midnight-did-rs/justfile`
- Create: `~/iohk/midnight-did-rs/crates/midnight-did/src/contract/mod.rs`

- [ ] **Step 7.1: Write `justfile`**

```just
# Default target lists available recipes.
default:
    @just --list

# Re-generate Rust code from did.compact via the patched compact compiler.
codegen:
    @mkdir -p target-gen
    @rm -rf target-gen/contract-out
    git submodule update --init third_party/midnight-did
    compact compile --skip-ts \
        third_party/midnight-did/packages/contract/src/did.compact \
        target-gen/contract-out
    @mkdir -p crates/midnight-did/src/contract
    cp target-gen/contract-out/src/lib.rs       crates/midnight-did/src/contract/generated.rs
    @mkdir -p crates/midnight-did/assets/keys
    cp target-gen/contract-out/*.zkir           crates/midnight-did/assets/keys/
    cp target-gen/contract-out/*.prover         crates/midnight-did/assets/keys/
    cp target-gen/contract-out/*.verifier       crates/midnight-did/assets/keys/
    cargo fmt -p midnight-did

# Verify re-running codegen produces no diff (regression signal for CI).
codegen-check: codegen
    git diff --exit-code -- crates/midnight-did/src/contract crates/midnight-did/assets/keys

build:
    cargo build --all-targets

test:
    cargo nextest run

fmt:
    cargo fmt
    taplo fmt

fmt-check:
    cargo fmt -- --check
    taplo fmt --check

lint:
    cargo clippy --all-targets -- -D warnings

ci: fmt-check lint build test
```

- [ ] **Step 7.2: Write `crates/midnight-did/src/contract/mod.rs`** (re-export glue, exists before generated.rs is produced)

```rust
//! DID contract — Rust code emitted from `did.compact` by the patched compact compiler.
//!
//! Do not edit `generated.rs` by hand. Re-run `just codegen` after updating
//! `third_party/midnight-did/packages/contract/src/did.compact` or after bumping
//! the compact flake input.

#![allow(missing_docs)] // generated module has its own docs

mod generated;

pub use generated::*;
```

- [ ] **Step 7.3: Update `crates/midnight-did/src/lib.rs` to expose `contract`**

Replace existing `lib.rs` with:

```rust
//! Native Rust implementation of the Midnight DID Method.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms, clippy::all)]

/// Crate version reported by the build.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod contract;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_compiles() {
        assert!(!VERSION.is_empty());
    }
}
```

- [ ] **Step 7.4: Commit the recipe & glue (pre-codegen)**

```bash
git add justfile crates/midnight-did/src/contract/mod.rs crates/midnight-did/src/lib.rs
git commit -S -s -m "feat(justfile): add codegen + ci recipes; wire contract module" \
  -m "just codegen drives compact --skip-ts against did.compact and slices the emitted lib.rs + zkir keys into crates/midnight-did/. The contract module re-exports everything in generated.rs (populated by T8)." \
  -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
git log --format="%h %G? %s" -1
```

> The crate WILL NOT compile right now because `contract/mod.rs` declares `mod generated;` but `generated.rs` doesn't exist yet. That's intentional — T8 produces it. The codegen-no-diff CI gate fires after T8 once the file exists; if you tried `cargo build` here it would fail with "file not found for module `generated`".

---

## Task 8: First codegen run + cargo build

**Files (auto-created/modified):**
- Create (auto): `~/iohk/midnight-did-rs/crates/midnight-did/src/contract/generated.rs`
- Create (auto): `~/iohk/midnight-did-rs/crates/midnight-did/assets/keys/*.{zkir,prover,verifier}`
- Probably modify: `~/iohk/midnight-did-rs/crates/midnight-did/Cargo.toml` (add deps the generated code needs)

- [ ] **Step 8.1: Run codegen**

```bash
cd ~/iohk/midnight-did-rs
nix --extra-experimental-features "nix-command flakes" develop --command just codegen
```

Expected: `target-gen/contract-out/` is populated with `src/lib.rs`, `Cargo.toml`, and `*.{zkir,prover,verifier}` files; those are sliced into `crates/midnight-did/src/contract/generated.rs` and `crates/midnight-did/assets/keys/`.

If `compact compile` errors out:
- Read the error. Common causes: `did.compact` references runtime functions not in the patched compact's vocabulary, or a path issue.
- Cross-check with how the TS build invokes compact: `cat ~/iohk/midnight-identity-workspace/midnight-did/packages/contract/package.json | grep -A2 '"compact"'`
- If `--skip-ts` is unrecognized, the binary on PATH isn't the patched one — recheck T5.

- [ ] **Step 8.2: Inspect the emitted Cargo.toml to learn what deps are needed**

```bash
cat target-gen/contract-out/Cargo.toml
```

Note every entry under `[dependencies]`. These are what the generated `lib.rs` consumes.

- [ ] **Step 8.3: Update `crates/midnight-did/Cargo.toml` to declare those deps**

Edit `crates/midnight-did/Cargo.toml` and add a `[dependencies]` block listing every dep from Step 8.2, using workspace-level keys. Example (real list comes from Step 8.2):

```toml
[dependencies]
compact-runtime           = { workspace = true }
midnight-base-crypto      = { workspace = true }
midnight-coin-structure   = { workspace = true }
midnight-onchain-runtime  = { workspace = true }
midnight-onchain-state    = { workspace = true }
midnight-onchain-vm       = { workspace = true }
midnight-serialize        = { workspace = true }
midnight-storage          = { workspace = true }
midnight-transient-crypto = { workspace = true }
midnight-zkir             = { workspace = true }
serde                     = { workspace = true, features = ["derive"] }
hex                       = { workspace = true }
```

If a dep from the emitted Cargo.toml isn't in workspace.dependencies, add it to the workspace `Cargo.toml` `[workspace.dependencies]` block first.

- [ ] **Step 8.4: Run `cargo build -p midnight-did`**

```bash
nix --extra-experimental-features "nix-command flakes" develop --command cargo build -p midnight-did
```

**Expected outcome (cycle-1 done-when, D4):** `cargo build` succeeds.

If it fails:
- "unresolved import" / "cannot find type X in crate Y" — the dep is missing from `Cargo.toml` or its feature set is wrong. Cross-reference with the emitted `Cargo.toml`.
- Trait/method mismatch — risk R1 manifesting (compact emitted code targets a slightly different ledger API). Inspect the failing call site, then either (a) patch the ledger crate in `third_party/midnight-ledger`, (b) add a thin adapter in `contract/mod.rs` wrapping the divergent call, or (c) escalate per the fallback in T5.
- Lifetime / generics errors specific to `compact-runtime` — likely a real bug in codegen; capture the error, file in `docs/superpowers/notes/`, and escalate.

- [ ] **Step 8.5: Run the placeholder test**

```bash
nix --extra-experimental-features "nix-command flakes" develop --command cargo test -p midnight-did
```
Expected: 1 passed, 0 failed.

- [ ] **Step 8.6: Commit the generated artifacts and Cargo.toml updates**

```bash
git add crates/midnight-did/Cargo.toml \
        crates/midnight-did/src/contract/generated.rs \
        crates/midnight-did/assets/keys/
# Also pick up any workspace.dependencies additions:
git add Cargo.toml || true
git commit -S -s -m "feat(contract): generate Rust code + zkir keys from did.compact" \
  -m "First successful codegen run. The contract module is populated from packages/contract/src/did.compact via 'compact compile --skip-ts'. Generated artifacts are checked in so non-nix readers can inspect the public surface; re-running 'just codegen' must produce no diff (enforced by 'just codegen-check' and the CI nix flake check job)." \
  -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
git log --format="%h %G? %s" -1
```

**This is the moment the spike completes.**

---

## Task 9: nix flake check

**Files:**
- Create: `~/iohk/midnight-did-rs/nix/checks.nix`
- Modify: `~/iohk/midnight-did-rs/flake.nix` (add import)

- [ ] **Step 9.1: Write `nix/checks.nix`**

```nix
{ ... }:
{
  perSystem =
    { pkgs, midnightDidRsLib, midnightLedgerSrc, compactRuntimeRsSrc, compactPkg, ... }:
    let
      inherit (midnightDidRsLib.rustTools) rust;

      # Common shell prelude that materialises symlinks so cargo path deps resolve.
      preludeSh = ''
        mkdir -p third_party
        ln -snf ${midnightLedgerSrc}    third_party/midnight-ledger
        ln -snf ${compactRuntimeRsSrc}  third_party/compact-runtime-rs
      '';
    in
    {
      checks = {
        cargo-fmt = pkgs.runCommand "cargo-fmt-check" {
          buildInputs = [ rust ];
          src = ../.;
        } ''
          cp -r $src/. ./
          ${preludeSh}
          cargo fmt -- --check
          touch $out
        '';

        cargo-build = pkgs.runCommand "cargo-build-check" {
          buildInputs = [ rust pkgs.pkg-config ];
          src = ../.;
        } ''
          cp -r $src/. ./
          ${preludeSh}
          export CARGO_HOME=$TMPDIR/cargo-home
          cargo build -p midnight-did
          touch $out
        '';

        # codegen-no-diff and cargo-clippy run via CI workflow only (they need
        # compact on PATH which is already there inside `compactPkg`).
        cargo-clippy = pkgs.runCommand "cargo-clippy-check" {
          buildInputs = [ rust ];
          src = ../.;
        } ''
          cp -r $src/. ./
          ${preludeSh}
          export CARGO_HOME=$TMPDIR/cargo-home
          cargo clippy --all-targets -- -D warnings
          touch $out
        '';

        taplo-fmt = pkgs.runCommand "taplo-fmt-check" {
          buildInputs = [ pkgs.taplo ];
          src = ../.;
        } ''
          cp -r $src/. ./
          taplo fmt --check
          touch $out
        '';
      };
    };
}
```

- [ ] **Step 9.2: Add the import in `flake.nix`**

In the `imports = [ … ]` list, append `./nix/checks.nix`.

- [ ] **Step 9.3: Run `nix flake check`**

```bash
cd ~/iohk/midnight-did-rs
nix --extra-experimental-features "nix-command flakes" flake check --keep-going
```
Expected: all four checks (`cargo-fmt`, `cargo-build`, `cargo-clippy`, `taplo-fmt`) pass. If clippy fires warnings, fix them inline — `-D warnings` is by design.

> The `codegen-no-diff` check is intentionally NOT part of `nix flake check` because it needs `compact` and a populated submodule, which doesn't fit the pure-nix sandbox cleanly. It runs in CI as a separate workflow step (T10).

- [ ] **Step 9.4: Commit**

```bash
git add flake.nix nix/checks.nix
git commit -S -s -m "feat(nix): add flake checks (fmt, build, clippy, taplo)" \
  -m "'nix flake check' is the single source of truth for 'green'. Four checks: cargo fmt --check, cargo build -p midnight-did, cargo clippy -- -D warnings, taplo fmt --check. The codegen-no-diff regression check runs in CI as a separate step (needs compact + submodule)." \
  -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
git log --format="%h %G? %s" -1
```

---

## Task 10: CI workflow

**Files:**
- Create: `~/iohk/midnight-did-rs/.github/workflows/ci.yml`
- Modify: `~/iohk/midnight-did-rs/.gitmodules` (swap submodule URL to public GitHub)

- [ ] **Step 10.1: Switch the submodule URL to public GitHub**

Local `file://` URLs in `.gitmodules` cannot be cloned by CI. Find the GitHub URL of the upstream TS repo, then:

```bash
cd ~/iohk/midnight-did-rs
git config -f .gitmodules submodule.third_party/midnight-did.url \
  "https://github.com/midnightntwrk/midnight-did.git"   # confirm exact org/repo
git submodule sync
```

> If the TS repo lives in a private org and CI doesn't have access, this needs a deploy key. Document the access requirement in README in T11.

- [ ] **Step 10.2: Write `.github/workflows/ci.yml`**

```yaml
name: ci

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

permissions:
  contents: read

jobs:
  nix-check:
    runs-on: ubuntu-latest
    timeout-minutes: 60
    steps:
      - uses: actions/checkout@v4
        with:
          submodules: recursive

      - uses: DeterminateSystems/nix-installer-action@main
        with:
          extra-conf: |
            extra-substituters = https://cache.iog.io
            extra-trusted-public-keys = hydra.iohk.io:f/Ea+s+dFdN+3Y/G+FDgSq+a5NEWhJGzdjvKNGv0/EQ=

      - uses: DeterminateSystems/magic-nix-cache-action@main

      - name: nix flake check
        run: nix flake check --keep-going --print-build-logs

      - name: just codegen (regression)
        run: |
          nix develop --command just codegen
          git diff --exit-code -- crates/midnight-did/src/contract crates/midnight-did/assets/keys
```

- [ ] **Step 10.3: Commit and push**

```bash
git add .gitmodules .github/workflows/ci.yml
git commit -S -s -m "ci: add GitHub Actions workflow running nix flake check + codegen regression" \
  -m "Single CI job: nix flake check (fmt, build, clippy, taplo) followed by a 'just codegen' regression step that fails if re-running codegen produces any diff." \
  -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
git log --format="%h %G? %s" -1
git push origin main
```

- [ ] **Step 10.4: Watch the CI run**

Run:
```bash
gh run watch || gh run list --limit 1
```
Expected: green check on the most recent commit.

If CI fails on `nix flake check` but it passes locally, investigate the diff in environment (e.g. missing GitHub token to fetch the submodule from a private repo).

---

## Task 11: README + memory update

**Files:**
- Modify: `~/iohk/midnight-did-rs/README.md`
- Modify: `~/.claude/skills/midnight-identity-rust/SKILL.md`

- [ ] **Step 11.1: Rewrite `README.md`**

```markdown
# midnight-did-rs

Native Rust implementation of the Midnight DID Method. Cycle 1 (this commit-range): a Cargo workspace that takes `did.compact` from the upstream TS repo and emits a Rust crate via the patched [Compact compiler](https://github.com/midnightntwrk/compact/tree/codegen-rust) (`codegen-rust` branch).

## Status

Cycle 1: **bootstrap + spike** (this is what's currently implemented).
Follow-up cycles add domain types, DID operations, HTTP API, storage, Docker, integration tests, and a WASM target. See `docs/superpowers/specs/` for design documents.

## Prerequisites

- Nix with flakes enabled
- (Optional) direnv for automatic shell entry
- Access to `midnightntwrk/midnight-did` (referenced as a git submodule under `third_party/midnight-did`)

## Quickstart

```bash
git clone https://github.com/yshyn-iohk/midnight-did-rs.git
cd midnight-did-rs
git submodule update --init --recursive
nix develop
just codegen
cargo build -p midnight-did
nix flake check
```

## Layout

| Path | Purpose |
|---|---|
| `crates/midnight-did/` | The single Rust crate. `src/contract/generated.rs` is emitted from `did.compact`. |
| `nix/` | flake-parts modules: devshell, checks, overlays, compact derivation, rust toolchain. |
| `third_party/midnight-did/` | git submodule of the TS reference (source of `did.compact`). |
| `third_party/midnight-ledger/` | symlink to nix-store path of the ledger crates (materialized by devshell). |
| `third_party/compact-runtime-rs/` | symlink to compact's `runtime-rs/` (materialized by devshell). |
| `docs/superpowers/specs/` | design docs per cycle. |
| `docs/superpowers/plans/` | implementation plans per cycle. |

## Conventions

- Commits: GPG signed + DCO sign-off (`git commit -S -s`).
- Code: Rust edition 2024, nightly-2026-03-18 pinned.
- Generated artifacts under `crates/midnight-did/src/contract/generated.rs` and `crates/midnight-did/assets/keys/` are checked in. Re-running `just codegen` must produce no diff — that's the regression signal.

## License

Apache-2.0. See `LICENSE`.
```

- [ ] **Step 11.2: Update the skill's decision log**

Add to `~/.claude/skills/midnight-identity-rust/SKILL.md` under "Architectural Decisions (running log)":

```markdown
- **2026-05-29** — Cycle 1 implementation landed. Single crate `midnight-did` builds against `dioxus-vc-demo` (== ledger-8.0.2 + JS-only fixes — verified by merge-base). Patched compact pulled in as flake input; `runtime-rs` exposed as `third_party/compact-runtime-rs`. CI green on first push.
```

- [ ] **Step 11.3: Commit & push**

```bash
cd ~/iohk/midnight-did-rs
git add README.md
git commit -S -s -m "docs: rewrite README for cycle-1 status + usage" \
  -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
git log --format="%h %G? %s" -1
git push origin main
```

> The skill update is a separate concern (lives in `~/.claude/`), no commit required there.

- [ ] **Step 11.4: Final state check**

Run:
```bash
cd ~/iohk/midnight-did-rs
git log --format="%h %G? %s" -10
gh run list --limit 3
```
Expected: every cycle-1 commit shows `G` for good signature; latest CI run is green.

---

## Self-review (run after writing this plan; already done at planning time)

- **Spec coverage:** every section of the spec (Mission, Context, Decisions D1-D7, Architecture, Repo layout, Flake structure, Codegen pipeline, Spike validation, Out of scope, Testing & gates, Risks) maps to one or more tasks. T1 (skeleton), T2 (workspace), T3-T6 (flake = D1, D2), T6 (compact-runtime overlay, missed in spec), T7 (codegen pipeline), T8 (validation = D4), T9 (gates), T10 (CI), T11 (README). ✓
- **Placeholder scan:** no "TBD" / "implement later" / "handle edge cases" / vague handwaving in any task. The only intentional unknowns are the exact compact-package attribute path (T5.1 — a recon step, not a placeholder) and the exact dependency list emitted by codegen (T8.2 — a recon step). Both are bounded and verifiable. ✓
- **Type consistency:** `midnight-did` crate name used consistently. Workspace dep keys use `midnight-` prefix throughout (T2.1 list, T8.3 list). Symlink paths (`third_party/midnight-ledger`, `third_party/compact-runtime-rs`) consistent across T1 (.gitignore), T4, T6, T9 (preludeSh), T10 (CI checkout), T11 (README). ✓
- **One overflow:** spec did not mention the `compact-runtime-rs` overlay; added in T6 with rationale.
