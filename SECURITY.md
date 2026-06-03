<!--
This file is part of midnightntwrk/midnight-did-rs.
Copyright (C) 2026 Midnight Foundation
SPDX-License-Identifier: Apache-2.0
-->

# Security Policy

This document describes how to report security vulnerabilities in
`midnight-did-rs` and what we consider in-scope. It follows the
Midnight Foundation
[security policy](https://github.com/midnightntwrk/midnight-did/blob/main/SECURITY.md)
and the
[Linux Foundation vulnerability management guidance](https://www.linuxfoundation.org/security)
applied to the upstream TypeScript reference.

## Supported versions

`midnight-did-rs` has not yet cut a tagged release. All active
development happens on the `cycle-1-bootstrap` branch (and, once it
lands, `main`). Once releases begin, the policy is:

| Version              | Supported          |
| -------------------- | ------------------ |
| Latest tagged release | ✅                |
| Previous tagged release | ✅              |
| Older releases       | ❌                 |
| `main` (untagged)    | Best-effort        |

Until the first tagged release, please report against `main` or the
relevant development branch.

## Reporting a vulnerability

**Do not open a public GitHub issue for security problems.** Use one
of the private channels:

- **Preferred:** GitHub's
  [private vulnerability reporting](https://docs.github.com/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability)
  on the `midnightntwrk/midnight-did-rs` repository.
- **Fallback:** email
  [security@midnight.foundation](mailto:security@midnight.foundation)
  if the GitHub flow is unavailable.

Please include as much of the following as you can:

- Repository name + commit / branch / tag affected.
- Type of issue (e.g. memory safety, cryptographic flaw, byte-parity
  divergence, dependency confusion, supply-chain).
- Full file paths and a pointer to the relevant code.
- Step-by-step reproducer, ideally with a minimal test case.
- A proof-of-concept exploit if you have one.
- Your assessment of impact and any suggested fix.

**Response time.** A maintainer will acknowledge your report within
**five (5) business days** and follow up with a more detailed triage
note within an additional five (5) business days. We aim to keep you
updated through the fix and coordinated-disclosure process.

## In scope

Anything that produces a verifiably wrong, unsafe, or
parity-breaking result in:

- `crates/midnight-did-domain` — DID Core types, Midnight method
  profile, identifier parsing.
- `crates/midnight-did-method` — Midnight-specific method logic
  (once landed; currently part of `midnight-did-api`).
- `crates/midnight-did-api` — async contract-operation surface,
  error types, integration with the runtime.
- `crates/midnight-did-runtime` — ledger integration glue (once
  buildable; tracked in the workspace plan).
- `crates/midnight-did` — umbrella crate and the
  Compact-emitted contract module.
- `crates/midnight-did-uniffi` — FFI bindings used by mobile
  consumers.
- The `compactc --rust` pipeline that emits
  `crates/midnight-did/src/contract/generated.rs` and the associated
  ZKIR / prover / verifier keys.
- TS reference fixtures under
  `crates/midnight-did-api/tests/fixtures/`. These fixtures gate the
  byte-parity tests; compromising them would mask real bugs in the
  Rust port and is treated as a security issue.

## Out of scope

The following live in other repos. We will gladly forward reports,
but please report them directly to the upstream maintainers for the
fastest fix:

- **Upstream `midnight-*` crates** (`midnight-ledger`,
  `midnight-base-crypto`, `midnight-onchain-runtime`, …) — report to
  the
  [midnight-ledger maintainers](https://github.com/midnightntwrk).
- **The Compact compiler** (`compactc`, `compact-runtime`,
  codegen passes) — report to the Compact maintainers.
- **The TypeScript reference**
  [`midnightntwrk/midnight-did`](https://github.com/midnightntwrk/midnight-did)
   — follow its own
  [`SECURITY.md`](https://github.com/midnightntwrk/midnight-did/blob/main/SECURITY.md).

If you're unsure where a report belongs, send it to us anyway and
we'll route it.

## Vulnerability management

On receipt of a report, a primary handler is assigned. They will:

- Confirm and reproduce the issue.
- Determine affected versions / branches.
- Audit adjacent code for similar problems.
- Prepare fixes for all supported releases under maintenance.
- Coordinate disclosure timing with the reporter.

## Preferred languages

English.

## Bug bounty

None at this stage of development. We may introduce one once releases
stabilise; until then, contributors are credited in the release notes
of the fix and (with permission) in any associated advisory.

## Suggesting changes to this policy

Open an issue or PR. Policy improvements are welcome.
