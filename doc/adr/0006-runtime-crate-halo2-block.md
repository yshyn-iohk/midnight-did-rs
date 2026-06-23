# ADR 0006 — Runtime crate halo2 ParamsKZG API skew (unblocked, partial)

**Status:** Halo2 block resolved · 4-bucket codegen block: 1/4 closed,
2/4 landed-but-blocked, 1/4 documented (this ADR)
**Date:** 2026-06-04 (updated 2026-06-04 after Bucket-3 close)

## Context

`cargo build -p midnight-did-runtime` previously failed during the
compile of `midnight-transient-crypto` with three E0599 method-not-found
errors on `ParamsKZG`:

```
no function or associated item named `read_mmap_arc`     for `ParamsKZG<E>`
no method named             `write_mmap_companion`       for `Arc<ParamsKZG<Bls12>>`
no function or associated item named `read_custom_lazy`  for `ParamsKZG<E>`
```

These three methods live exclusively on a **patched fork** of
`midnight-proofs` maintained at
[`yshyn-iohk/midnight-zk`](https://github.com/yshyn-iohk/midnight-zk)
on branch `feat/v0.7-h-poly-streaming`. They add:

- `read_mmap_arc(Arc<Mmap>)` — construct a `ParamsKZG` whose `g` /
  `g_lagrange` are slice-views over an `mmap`'d SRS file (saves ~300 MiB
  heap at k=20, lets the OS page cold rows out under pressure).
- `write_mmap_companion(&mut File)` — write the SRS in the layout the
  `read_mmap_arc` path expects.
- `read_custom_lazy(R: Read + Seek)` — seek-past the on-disk
  `g_lagrange` block and recompute Lagrange via inverse-NTT on first
  use; halves peak heap during file parse.

Upstream `midnight-ledger` (this repo's `transient-crypto` snapshot)
already calls these methods unconditionally in
`third_party/midnight-ledger/transient-crypto/src/proofs.rs`. Its own
root `Cargo.toml` carries the patch line

```toml
[patch.crates-io]
midnight-proofs = { path = "/Users/ysh/iohk/midnight-zk/proofs" }
```

…which **does not propagate** when `midnight-ledger` is consumed as a
path-dep by this workspace. `cargo` only honours `[patch.crates-io]`
declared at the **consuming workspace's root**.

## Decision — fix the halo2 surface

Add the patch at the consumer root and surface the fork through the
existing Nix flake materialisation pattern:

1. **`flake.nix`** — add `midnight-zk` input pinned to commit
   `cf60e3ccb87f2de40b1307f7e78abdcb4c696c91` on
   `yshyn-iohk/midnight-zk@feat/v0.7-h-poly-streaming`.
2. **`nix/overlays.nix` + `nix/devShells.nix`** — extend the shellHook
   to materialise `third_party/midnight-zk` as a symlink to the
   Nix-store path (same pattern used for `midnight-ledger` and the
   compact runtime crates under `third_party/compact/`).
3. **Root `Cargo.toml`** — `[patch.crates-io] midnight-proofs = { git, rev }`
   pointing at the same fork commit. The path-dep form was rejected
   because the fork's `proofs/Cargo.toml` uses `workspace = true`
   inheritance from its own root, and cargo resolves that against the
   *consuming* workspace's `[workspace.dependencies]` — not the
   path-dep's own enclosing workspace. The `git` form makes cargo
   build the patched crate inside its own workspace, sidestepping that.
4. **`cargo update -p midnight-proofs`** — refresh `Cargo.lock` so the
   patched `0.7.99` supplants the registry's `0.7.1`.

After this, the three ParamsKZG errors **disappear** and
`midnight-transient-crypto` compiles. `cargo build --workspace`
(excluding `midnight-did-runtime`) stays green; the 144 existing tests
keep passing.

## What we tried before settling on `[patch.crates-io] git`

| Attempt | Outcome |
|--------|---------|
| `[patch.crates-io] midnight-proofs = { path = "third_party/midnight-zk/proofs" }` | Failed — `workspace = true` inheritance resolved against the consuming workspace's root, surfacing `blake2b_simd was not found in workspace.dependencies`. |
| `[patch.crates-io] midnight-proofs = { git = "...", rev = "..." }` plus `cargo update` | **Worked** — cargo builds the patched crate inside its own workspace, the three ParamsKZG methods become available, transient-crypto compiles. |

## Remaining block — codegen gaps (covered by ADR 0005)

With the halo2 surface fixed, `cargo build -p midnight-did-runtime`
initially failed on 97 **codegen-gap** symbol-not-found errors that
the compactc-emitted `generated.rs` referenced but `compact-runtime`
did not yet export:

- `compact_runtime::BinaryHashRepr` (trait)
- `compact_runtime::std_lib::{decode_bool, decode_bytes, persistent_hash_aligned, OpaqueString}`
- bare `new_map(…)`, bare `MemWrite`

### Update (2026-06-04) — compact flake bump closes the symbol surface

Bumping the `compact` flake input from the May-26 snapshot to
`yshyn-iohk/compact@5fc3ec7d` (codegen-rust HEAD, 2026-06-02) pulled
in the post-`std_lib` split runtime crate. **All 97 missing-symbol
errors disappeared.**

The bump required a layout change in `third_party/`:

- The new `compact-runtime` Cargo.toml references its proc-macro
  sibling via the relative path `../runtime-rs-macros`, and the
  proc-macro crate's dev-deps include `compact-runtime = { path =
  "../runtime-rs" }`. Cargo's eager resolution of dev-dep manifests
  (even when the crate is only used as a path-dep) means both sibling
  paths must exist.
- We mirrored compact's in-repo layout: `third_party/compact/{runtime-rs,
  runtime-rs-macros}` (was: a flat `third_party/compact-runtime-rs`).
  The devshell `shellHook` now materialises two symlinks under
  `third_party/compact/` from `compactRuntimeRsSrc` +
  `compactRuntimeRsMacrosSrc`.
- Workspace `Cargo.toml` updated: `compact-runtime = { path =
  "third_party/compact/runtime-rs" }`.

Build state after the bump:

| Crate | Before flake bump | After flake bump |
|-------|-------------------|------------------|
| midnight-did-runtime | 97 errors (missing symbols) | 44 errors (real type/trait mismatches) |
| all other workspace crates | green | green (no regressions) |

The 44 remaining errors break down (`cargo build -p
midnight-did-runtime 2>&1 \| grep '^error' \| sort \| uniq -c`):

- **11× E0308 mismatched types** — `generated.rs` defines its own
  `pub struct ContractAddress { … }` (line 675) that shadows
  `compact_runtime::ContractAddress` (re-exported from
  `midnight-coin-structure`). `QueryContext::new` expects the latter.
  *Cause:* codegen emits a fresh struct rather than referring to the
  runtime re-export. Belongs in compact's rust-passes, not here.
- **8× E0277 + 8× E0599** — `EmbeddedGroupAffine` (re-exported from
  `midnight-transient-crypto` via `compact-runtime` as `JubjubPoint`)
  is missing `FromFieldRepr`, `binary_repr`, `binary_len`,
  `field_repr`, `field_size`. *Cause:* the proxy / wrapper trait impls
  for the Jubjub bundle aren't part of `compact-runtime` yet.
- **14× E0599** — `OpaqueString` missing `binary_repr` / `binary_len`
  (`BinaryHashRepr` trait impl). *Cause:* `OpaqueString` is exported
  as `Default`-able and field-aligned, but the `BinaryHashRepr` impl
  isn't wired up yet.
- **3× E0277** — user enums (`KeyType`, `CurveType`,
  `VerificationMethodType`) need `Default` impls. *Cause:* codegen
  emits the enum but no `#[derive(Default)]` and no manual impl.

All four buckets are **upstream issues** — either compact-rust-passes
(emitter) or compact-runtime (trait impls / proxy plumbing). None can
be fixed inside `midnight-did-rs` without editing `generated.rs`
(forbidden) or the `compact-runtime` source under
`third_party/compact/` (which is a read-only Nix-store symlink).

The runtime crate therefore still stays not-in-CI until ADR 0005's
runtime-export work closes these last 44 errors. The umbrella crate's
`runtime` feature stays off-by-default.

### Update (2026-06-04 cont.) — 4-bucket close attempt

Compact-side work on the four buckets:

| Bucket | Errors | Compact commit | Outcome in midnight-did-runtime |
|--------|--------|----------------|----------------------------------|
| 3 — `OpaqueString::BinaryHashRepr` impl | 14 | `3d23c9a` (runtime-rs) | **Closed.** 44 → 30 errors after flake bump. |
| 1 — `compact_runtime::ContractAddress` FQN in `QueryContext::new` | 11 | `12e50a8` (rust-passes-emit + snapshots + regen of 23 e2e fixtures) | Landed in compactc; **takes effect only after `generated.rs` is regenerated**, which is blocked by an unrelated walker gap (see below). |
| 4 — `Default` derive on user enums (`#[derive(...,Default)]` + `#[default]` on variant 0) | 3 | `12e50a8` (same commit as Bucket 1) | Landed in compactc; same blocker as Bucket 1. |
| 2 — `EmbeddedGroupAffine`/`JubjubPoint` missing `FieldRepr`/`FromFieldRepr`/`BinaryHashRepr` | 16 | not landed | **Blocked by Rust orphan rule.** Both the traits (`midnight_transient_crypto::repr::{FieldRepr, FromFieldRepr}`, `midnight_base_crypto::repr::BinaryHashRepr`) and the type (`midnight_transient_crypto::curve::EmbeddedGroupAffine`) are foreign to compact-runtime. The impls would have to live either in the upstream `midnight-ledger` crates (out of scope by repo policy) or behind a newtype wrapper that the codegen emits instead of `JubjubPoint` (a larger codegen + runtime API change with risk to the 85 passing e2e tests). Deferred. |

Walker gap blocking buckets 1 + 4 from taking effect:

```
$ compactc --rust --skip-zk third_party/midnight-did/packages/contract/src/did.compact …
Exception: did.compact line 221 char 1:
  compactc --rust: unsupported Compact construct (circuit-body-emission):
    no walker shape matched
  circuit body for rotateControllerKey
```

`rotateControllerKey` opens with `assertControllerCanUpdate();` — a
call-stmt to a user-defined void circuit — which the current walker
shapes do not match as the first statement of a multi-statement body.
This is a separate codegen widening that belongs in
`compiler/rust-passes-walker.ss`, scoped on its own.

Pin state after the close attempt: `flake.lock` carries
`yshyn-iohk/compact@12e50a8` — runtime-rs's `OpaqueString` BinaryHashRepr
impl is what's actually picked up by `cargo build`. The Bucket-1 and
Bucket-4 codegen-side fixes are inert until `generated.rs` is
regenerated.

Final error count: **30 (down from 44; -14, all OpaqueString).**
Remaining 30 split:
- 11 ContractAddress shadowing (Bucket 1; codegen-side fix landed,
  awaits regen)
- 16 EmbeddedGroupAffine (Bucket 2; deferred)
- 3 user-enum Default (Bucket 4; codegen-side fix landed, awaits regen)

## Consequences

- `cargo build -p midnight-did-runtime` no longer fails on halo2 method
  resolution.
- The local devshell + CI both pull a reproducible `midnight-zk` snapshot
  via the new flake input; no `/Users/ysh/...` paths leak into any
  manifest.
- The patched `midnight-proofs@0.7.99` becomes the single
  workspace-wide version, used by `midnight-circuits`,
  `midnight-zk-stdlib`, `blake2b_halo2`, `sha3-circuit`, and our own
  `midnight-transient-crypto`. The fork's interface is a strict
  superset of `0.7.1`, so existing consumers are unaffected.
- The next gating issue is codegen-side. When ADR 0005's runtime
  re-exports are complete the runtime crate will finish compiling and
  can join CI.

## How to refresh the pin in the future

```sh
cd ~/iohk/midnight-did-rs

# 1. flake.nix — bump the midnight-zk input ref / rev
$EDITOR flake.nix

# 2. refresh the lock entry
nix flake lock

# 3. refresh the cargo lock for the patched dep
cargo update -p midnight-proofs

# 4. materialise the new symlink (optional, for inspection)
nix develop --command true
```

## Update — 2026-06-04 walker-gap decomposition

A follow-up session traced "walker gap A" (the previously documented
single blocker on rotateControllerKey) and found it was actually a
cluster of four related sub-gaps. Three of them are now closed in
compact's codegen-rust branch (commit `1a8778d`), leaving exactly one
concrete shape to widen before regen unblocks.

### Sub-gap taxonomy

| ID | Description | Status |
|---|---|---|
| **A1** | `body-walkable?` required bare-call statements to be **non-terminal** (`(pair? (cdr stmts))` guard). did.compact's exported circuits all end with `recordUpdate();` — a terminal bare-call — which the predicate rejected. | ✅ Closed in commit `1a8778d`. Bare-call clause in `rust-passes-walker.ss:1265` no longer has the `(pair? (cdr stmts))` guard; emit-body-or-fallback's loop already handles terminal positions correctly. |
| **A2** | `rust-passes.ss` emission filter (line 99-101) only emitted **exported** impure circuits as methods. Non-exported helpers like `recordUpdate`, `assertController`, `assertControllerCanUpdate` were skipped — and their bare-call sites then had no callee to invoke. | ✅ Closed in commit `1a8778d`. Filter relaxed to emit non-exported impure circuits as `pub(crate) fn` when their body is in fact walkable (new `impure-circuit-body-walkable?` pre-flight predicate keeps non-walkable helpers like tiny.compact's `in_state` filtered out exactly as before). |
| **A3** | `classify-call` (`rust-passes-walker.ss:2036`) required `id-exported?` on the impure branch, so non-exported impure-circuit calls in bare-call position classified as `'unknown`. | ✅ Closed in commit `1a8778d`. The export check is dropped; both exported and non-exported impure-circuit calls now classify as `'impure-exported` (tag kept for downstream simplicity — emit shape is identical). |
| **A4** | `recordUpdate`'s OWN body is a new shape: zero-or-more non-write public-ledger ADT-update calls (`operationCount.increment(1); version.increment(1);`) followed by one or more Cell writes (`updated = disclose(currentTimestamp());`). Neither `body-walkable?` (which only accepts a single terminal PL-call) nor `body-streaming-walkable?` matches. | ⏸ **Open.** This is the actual remaining blocker. Closing it requires extending `body-walkable?` + `emit-body-or-fallback` to walk a sequence of non-write public-ledger calls before reaching the write-accumulator phase. Roughly analogous to the existing terminal-PL-call clause but in non-terminal position with the ctx threaded through each call's vm-code expansion. Medium effort. |

### What unlocks when A4 closes

- `recordUpdate` becomes walkable → emit-impure-circuit emits its body
  as `pub(crate) fn record_update` on the Contract impl.
- `assertController` body already walkable (single assert) — emits.
- `assertControllerCanUpdate` body walkable (`assertController();
  assert(active, "...")` — bare-call + assert) — emits.
- Every exported circuit in `did.compact` calls `recordUpdate()` at
  the end. With recordUpdate emitted as a method, the bare-call sites
  from rotateControllerKey / deactivate / setAlsoKnownAs / etc. resolve
  to `self.record_update(ctx, ...)?` and emit cleanly.
- `compactc --rust --skip-ts did.compact` succeeds → midnight-did-runtime's
  `generated.rs` regenerates, picking up the already-landed Bucket 1
  (ContractAddress FQN) + Bucket 4 (enum Default) fixes.
- Bucket 1 + 4 fixes consume 11 + 3 = 14 of the current 30 errors.
- Bucket 2 (EmbeddedGroupAffine, 16 errors) remains. Task #116 is
  marked completed but the fix needs verification — either an upstream
  midnight-transient-crypto patch or a newtype wrapper in compact-runtime.

### Pin state after this update

- compact pinned at `1a8778d` (codegen-rust HEAD after walker
  generalisation).
- The walker generalisation is non-regressive — 144+ tests-e2e-rust
  pass.
- did.compact still fails at line 221 (rotateControllerKey) with the
  hard "no walker shape matched" error, now narrowly because of A4
  rather than the broader A1-A4 cluster.

## References

- `flake.nix` — `midnight-zk` input
- `nix/overlays.nix`, `nix/devShells.nix` — third_party/midnight-zk symlink
- `Cargo.toml` — `[patch.crates-io]` entry
- ADR 0003 — Crate split + umbrella feature gating
- ADR 0005 — Codegen-gap handling (the remaining workstream)
- Compact PR [yshyn-iohk/compact#1](https://github.com/yshyn-iohk/compact/pull/1) — `1a8778d` walker generalisation
