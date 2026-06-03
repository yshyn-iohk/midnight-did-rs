# ADR 0006 — Runtime crate halo2 ParamsKZG API skew (unblocked, partial)

**Status:** Halo2 block resolved · codegen-gap block remains (see ADR 0005)
**Date:** 2026-06-04

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
   Nix-store path (same pattern used for `midnight-ledger` and
   `compact-runtime-rs`).
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
now fails on **codegen-gap** symbols that the compactc-emitted
`generated.rs` references but `compact-runtime` does not yet export:

- `compact_runtime::BinaryHashRepr` (trait)
- `compact_runtime::std_lib::{decode_bool, decode_bytes, persistent_hash_aligned, OpaqueString}`
- bare `new_map(…)`, bare `MemWrite`

This is exactly the surface ADR 0005 plans to fill incrementally. The
remaining 97 errors are codegen-side, not toolchain-side — they are
*not* tracked here; they belong with the codegen-rust upstream work.

The runtime crate therefore stays not-in-CI until ADR 0005's runtime
re-export checklist closes. The umbrella crate's `runtime` feature
stays off-by-default.

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

## References

- `flake.nix` — `midnight-zk` input
- `nix/overlays.nix`, `nix/devShells.nix` — third_party/midnight-zk symlink
- `Cargo.toml` — `[patch.crates-io]` entry
- ADR 0003 — Crate split + umbrella feature gating
- ADR 0005 — Codegen-gap handling (the remaining workstream)
