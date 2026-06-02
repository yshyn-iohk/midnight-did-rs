# TS reference fixtures for the Midnight DID API

This directory holds JSON fixtures the Rust API tests use to assert
byte-for-byte parity with the canonical TypeScript `@midnight-ntwrk/midnight-did-api`
resolver output.

## Why these specific fixtures

The TS source's high-fidelity flow (`packages/api/src/test/did.api.test.ts`)
uses `testcontainers`, a live Midnight node, and the wallet SDK; capturing
its full JSON output here would require running that stack — out of scope
for the Rust port iteration that produced these tests.

Instead, each fixture below captures the *deterministic* result of the
`LedgerToDomain.ledgerStateToDIDDocument` mapper for a specific
`Ledger` snapshot, derived from:

- the TS implementation in `packages/did/src/ledger-to-domain.ts`
- the W3C DID Core 1.0 + JWS-2020 context constants pinned in
  `packages/did/src/midnight-did-document.ts`
- the verification-method / service / aka semantics tested in
  `packages/api/src/test/verification-method-operations.test.ts`,
  `packages/api/src/test/ledger-mappers.test.ts`, and the `did.api.test.ts`
  high-fidelity flow.

The fixtures are hand-authored so the Rust + TS pipelines can be compared
without standing up the wallet stack. If the TS pipeline produces a
different shape for these inputs, the Rust assertion fails — surfacing the
divergence early.

## Files

- `initial-state.json` — DID Document immediately after `createDID` /
  contract deploy. No verification methods, no services, no aka, but an
  `@context`, `id`, `controller`, and metadata with a `versionId`.
- `after-rotate-controller-key.json` — DID Document after one
  `rotateControllerKey` call. Same shape as initial; only the
  `versionId` advances (`controllerPublicKey` is private to the
  ledger snapshot and is not surfaced in the document).
- `after-set-verification-method.json` — DID Document after
  inserting a single Ed25519 verification method via
  `addVerificationMethod`. Adds a `verificationMethod` array with one
  entry.

## How to refresh from TS

To regenerate any of these fixtures from the TS source, drop the
following into a sandbox script next to `did.api.test.ts`:

```ts
import { LedgerToDomain } from "@midnight-ntwrk/midnight-did/ledger-to-domain";
import { MidnightNetwork } from "@midnight-ntwrk/midnight-did-domain";
import { parseContractAddress } from "@midnight-ntwrk/midnight-did/midnight";

const network = MidnightNetwork.Undeployed;
const address = parseContractAddress(
  "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
);
const ledger = /* construct the same shape as the Rust DidLedgerSnapshot */;
const doc = LedgerToDomain.ledgerStateToDIDDocument(ledger, network, address);
const metadata = LedgerToDomain.ledgerStateToMetadata(ledger);
console.log(JSON.stringify({ didDocument: doc, didDocumentMetadata: metadata }, null, 2));
```

Constants used in every fixture:
- `contractAddress` = `0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef`
- `network` = `undeployed`
- `created` / `updated` = `1700000000000` ms = `2023-11-14T22:13:20Z`
