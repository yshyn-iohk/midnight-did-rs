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

Each fixture pairs with a test method in
`tests/did_api_end_to_end.rs` that drives the corresponding api
operation against the recording mock contract and asserts the
resolved document is structurally identical to the fixture.

### Baseline (3 fixtures)

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

### Extended mutation coverage (10 fixtures)

Together these cover the 11 mutation circuits the contract exports.
`setAlsoKnownAs` and `setVerificationMethod` are each represented by
both their insert and remove/update side. `setVerificationMethodRelation`
is represented by the insert side (the remove side produces the same
shape as the initial state plus a residual verification method).

- `after-set-aka-insert.json` — `setAlsoKnownAs(uri, Insert)`. Adds
  a single alias to `alsoKnownAs`.
- `after-set-aka-remove.json` — `setAlsoKnownAs(uri, Remove)`. Removes
  the only alias; the `alsoKnownAs` field disappears entirely.
- `after-set-vm-update.json` — `setVerificationMethod(vm, Update)`.
  Replaces the JWK `x` for an existing `#key-1`.
- `after-remove-vm.json` — `removeVerificationMethod(id)`. After
  purging all relation memberships and removing the method itself
  the document body collapses back to the initial shape.
- `after-set-schnorr-jubjub-vm-insert.json` —
  `setSchnorrJubjubVerificationMethod(vm, Insert)`. The Schnorr-Jubjub
  reconstruction maps `{ x: "01", y: "02" }` (hex, right-padded to 32
  bytes) into an `EC + Jubjub` JWK with base64url-encoded coordinates.
- `after-remove-schnorr-jubjub-vm.json` —
  `removeSchnorrJubjubVerificationMethod(id)`. Same shape as initial,
  versionId advanced.
- `after-set-vm-relation-insert.json` —
  `setVerificationMethodRelation(Authentication, "#key-1", Insert)`.
  The fixture includes the `verificationMethod` array (the relation
  must reference an existing method) plus an `authentication`
  array carrying the fragment-form id.
- `after-set-service-insert.json` — `setService(svc, Insert)`. Single
  URI endpoint, single-string `type`. `parseServiceEndpoint` round
  trips the JSON-encoded URI back to a bare URL string.
- `after-remove-service.json` — `removeService(id)`. Same shape as
  initial, versionId advanced.
- `after-deactivate.json` — `deactivate()`. Document body unchanged;
  `didDocumentMetadata.deactivated = true` flips on as soon as
  `deactivated || !active` holds in the ledger snapshot.

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
- `created` = `1700000000000` ms = `2023-11-14T22:13:20Z`
- `updated` for the post-rotate / post-set-vm pair = `1_700_001_000_000`
  / `1_700_001_900_000` (matches the original three fixtures).
- `updated` for the extended fixtures = `1_700_002_800_000`
  (`2023-11-14T23:00:00Z`) for the first mutation in a flow, and
  `1_700_003_700_000` (`2023-11-14T23:15:00Z`) for a second mutation
  that follows an insert/seed step.

### Authoring notes

The Schnorr-Jubjub fixture relies on
`LedgerToDomain.schnorrJubjubPkToJwk`, which hex-decodes each
coordinate, right-pads to 32 bytes, then base64url-encodes the result.
Inputs `x = "01"`, `y = "02"` yield the JWK coordinates
`AQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA` and
`AgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA`.

For `setVerificationMethodRelation` the document still carries the
underlying `verificationMethod` array because the relation must point
at an existing method. The relation members themselves are normalised
to their fragment form (e.g. `#key-1`) by
`LedgerToDomain.ledgerStateToDIDDocument`.

For `removeService` / `removeVerificationMethod` / `removeAlsoKnownAs`
/ `removeSchnorrJubjubVerificationMethod` the resulting document
collapses back to the shape of `initial-state.json` with an advanced
`versionId` and `updated` timestamp — those operations are still
asserted explicitly so a regression that changes the empty-set
encoding (e.g. emitting `"alsoKnownAs": []` instead of omitting the
field) is caught.

The extended fixtures are hand-authored against the
`LedgerToDomain.ledgerStateToDIDDocument` and `ledgerStateToMetadata`
semantics in `packages/did/src/ledger-to-domain.ts` and the
`*-to-ledger` helpers in `packages/api/src/ledger-mappers.ts`.
