<!--
This file is part of Compact.
Copyright (C) 2026 Midnight Foundation
SPDX-License-Identifier: Apache-2.0
Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0
-->

# `midnight-did-cli`

A runnable, end-to-end reference demo for the Rust Midnight DID API. One
binary, one command, seven DID Documents printed to stdout — top-to-bottom
walkthrough of every CRUD shape the API exposes.

## Quick start

```sh
cargo run -p midnight-did-cli -- run
```

You will see seven labelled JSON blocks (`== Step N: <name> ==`), one per
mutation, ending with the deactivated state.

Other forms:

```sh
midnight-did-cli run --compact-json           # one line per document
midnight-did-cli run --step rotate            # single step
midnight-did-cli capture-fixtures /tmp/did    # writes <step>.json files
midnight-did-cli --help                       # full subcommand list
```

## Expected output (snippet)

```text
== Step 1: create ==
{
  "didDocument": {
    "@context": [
      "https://www.w3.org/ns/did/v1",
      "https://w3c.github.io/vc-jws-2020/contexts/v1"
    ],
    "controller": "did:midnight:undeployed:0123...cdef",
    "id":         "did:midnight:undeployed:0123...cdef"
  },
  "didDocumentMetadata": {
    "created":   "2026-06-03T00:00:00Z",
    "updated":   "2026-06-03T00:00:00Z",
    "versionId": "1"
  }
}

== Step 2: set-vm-insert ==
{ ... JsonWebKey method + authentication / assertionMethod scopes ... }

== Step 3: set-service-insert ==
{ ... LinkedDomains @ https://example.com ... }

...
```

## Flow

The default `run` command executes, in order:

1. **create** — `create_did(secret_key=[0x42; 32])` seeds the private state.
2. **set-vm-insert** — insert a `JsonWebKey` verification method (`#key-1`,
   `P-256` JWK) and scope it to both `authentication` and `assertionMethod`.
3. **set-service-insert** — insert a `LinkedDomains` service endpoint
   pointing at `https://example.com`.
4. **set-alsoKnownAs-insert** — insert a `did:web:example.com` alias.
5. **rotate** — rotate the controller key (`0x42…` → `0x55…`).
6. **resolve** — read back the assembled DID Document with every step
   applied.
7. **deactivate** — tombstone the DID; metadata flips `deactivated: true`.

## Comparing against the TS reference

`capture-fixtures <dir>` writes each step's DID-Document JSON to
`<dir>/<step>.json`. Filenames are stable (`create.json`, `set-vm.json`,
`set-service.json`, `set-aka.json`, `rotate.json`, `resolve.json`,
`deactivate.json`), so a TS-side test harness can read the same files and
assert `JSON.stringify`-equal — the fixtures in this repo are aligned to the
TS DID Document shape (`@context`, `controller`, `verificationMethod`,
`service`, `alsoKnownAs`, `didDocumentMetadata`).

Existing TS-shaped fixtures live at
`crates/midnight-did-api/tests/fixtures/`. The Rust end-to-end test
(`tests/did_api_end_to_end.rs`) already asserts parity for three of those
shapes; this CLI extends the coverage to all seven by emitting them on
demand.

## Limitations

This binary is a reference flow, not a production wallet:

- **Mock contract** — uses `midnight_did_api::contract::mock::RecordingContract`.
  No live Midnight node, no compact-runtime invocation, no signature
  checking. The `midnight-did` runtime crate is currently blocked on a
  halo2 dependency mismatch (see workspace README); this CLI is built so
  the API layer can be exercised end-to-end while the runtime catches up.
- **No real key derivation** — the controller public key is a fixed stub
  (`0x07…07`) instead of being derived from the secret via the in-circuit
  `pad(32, "did:controller:pk") ‖ sk → persistentHash` rule.
- **Deterministic ledger snapshots** — every step installs a pre-computed
  ledger snapshot on the mock contract. The shape of the snapshot mirrors
  what the on-chain contract would produce, but the timestamp + version
  bumps are local bookkeeping.
- **Single-DID flow** — there is no support for multiple DIDs, side
  contracts, or DID-to-DID interactions.

If you need a realistic flow against a Midnight node, see the TS package
`@midnight-ntwrk/midnight-did-api`; this CLI is the structural Rust mirror.
