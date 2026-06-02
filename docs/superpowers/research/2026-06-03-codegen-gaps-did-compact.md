# Codegen gap survey — `did.compact` against `compactc --rust`

## Header

- **Date**: 2026-06-03
- **Source file**: `third_party/midnight-did/packages/contract/src/did.compact`
- **Source submodule SHA**: `6274cff616054ed0add943988ba7af25abcba3bb` (midnight-did `heads/develop`)
- **Compactc**: `0.31.104` from worktree `admiring-lehmann-05e4d9`, commit `5fc3ec7` (`release: bump toolchain 0.31.103 → 0.31.104 + comprehensive docs`)
- **Method**: Iteratively stub each circuit body in a working copy under `target-gen/did-gap-survey-tree/`, re-run `compactc --rust --skip-zk --skip-ts`, observe the next `(circuit-body-emission)` hard-fail, attempt minimal body-reductions to isolate the offending IR shape, then record + restore stub.
- **Working copy location**: `/Users/ysh/iohk/midnight-did-rs/target-gen/did-gap-survey-tree/packages/contract/src/did.compact` (mirrors the submodule's relative import path so `import "../../jubjub-schnorr/src/schnorr"` resolves against a co-located copy under `target-gen/did-gap-survey-tree/packages/jubjub-schnorr/src/`).
- **Partial lib.rs emitted at**: `/tmp/did-gen-attempt/contract/lib.rs` — 1320 lines — SHA1 `860bc2d2773728df3468bbd9f89e8f9982ed6281`.

**TL;DR**: 11 distinct exported-circuit body shapes blocked `compactc --rust` emission. 12 of 23 user circuits (every non-exported circuit) plus the constructor emit cleanly with no stub. 11 of 23 circuits (every exported impure circuit) required stubbing. All 11 failures surface as the same compactc feature tag `(circuit-body-emission): no walker shape matched` — the codegen has shape-walkers tuned to the existing fixture corpus and `did.compact`'s exported-circuit bodies do not match any. The non-exported impure circuits succeed because they share IR shapes already exercised by zerocash/election/F-set fixtures. The export boundary appears to expand the body-emission walker requirements (likely because exported circuits go through `CircuitResults<PS, R>`-returning emission with extra wrap/unwrap shape constraints).

## Gap table

The "circuit count" column is the number of `did.compact` exported circuits that hit this gap. Several rows in the source share an IR shape; the table merges them and notes the line range. Effort estimates use the codegen-rust history as calibration: **S** = stdlib-symbol routing / single-walker arm (similar to "Iter 8 bounded Uint", "Prod-13 Uint literal RHS"); **M** = new walker arm + runtime helper (similar to "Iter 7 follow-up non-identity lambda", "E5 cross-circuit call to exported impure"); **L** = multi-arm walker + runtime data-path (similar to "Prod-9 hashToCurve byte-parity", "Iter 6 fold").

| # | Line(s) | Circuit(s) | Feature tag | Body shape | Effort | Notes |
|---|---|---|---|---|---|---|
| 1 | 221 | `rotateControllerKey` | `circuit-body-emission` | `Bytes<32>` ledger-cell write in exported circuit body. Even reduced to a single `controllerPublicKey = pad(32, "stub");` (no `disclose`, no cross-circuit calls, no assert) the walker rejects it. Adding a Uint cell-write before it does **not** rescue the shape. | **M** | The constructor walker accepts `Bytes<32>` cell-writes (the constructor body emits cleanly). The `tiny.compact` `set` circuit also writes `Bytes<32>` (the `authority = apk;` line) so the walker has an arm for this — but only when the body matches the exact `[assert; const sk = witness(); const apk = pure_call(sk); cell = apk; cell = disclose(arg); cell = enum_literal]` shape (5-stmt fixed shape). `did.compact`'s 1-stmt `Bytes<32>` cell-write does not match. Closing the gap means adding a "simple `Bytes<N>` cell-write" walker arm — a single new shape arm in the export walker; pattern parallels `Prod-13`. |
| 2 | 230 | `setAlsoKnownAs` | `circuit-body-emission` | `Set<Opaque<"string">>.insert(disclose(arg))` (or 2-stmt form: `const x = disclose(arg); set.insert(x);`). Minimal reproducer fails. The F-set fixture passes `Set<Bytes<32>>` but **not** `Set<Opaque<"string">>` with this body shape. | **M** | Likely a missing dispatch for `Opaque<"string">` as the Set element-type in the `insert` walker arm (the F-set fixture uses `Bytes<32>` and the walker pattern-matches the element type when generating the AlignedValue encoding). Same pattern as Iter 3 List<T> closure work — add `Opaque<"string">` element to the Set.insert walker arm. |
| 3 | 232 | `setVerificationMethod` | `circuit-body-emission` | Multi-stmt body: `cross_circuit_call; const x = disclose(struct_arg); const m = disclose(enum_arg); cross_call(m); cross_call(x); if (m == Enum.X) { assert(map.member(x.id)); map.remove(x.id); } else if (m == Enum.Y) { assert(!cross_pure_call(x.id)); } map.insert(x.id, x);`. The combined shape uses `Map<Opaque<"string">, struct>.insert(struct_arg.id, struct_arg)` (struct-field access in BOTH key and value position), if-else-if on enum-equality dispatching distinct Map mutations, and a final post-branch `Map.insert`. | **L** | This pattern combines three known-partial gaps: (a) `Map<Opaque<"string">, struct>` with struct-field-derived key (G-map fixture only uses primitive keys), (b) if-else-if with Map.remove in one branch and only a guard-assert in the other (E6.2 covers impure if mid-body but with symmetric arms), (c) post-branch unconditional `Map.insert` after a conditional `Map.remove`. Each piece is a small walker arm; the combination probably needs a new "mutation-then-insert" walker shape — Iter 3 / E4-class work. |
| 4 | 236 | `removeVerificationMethod` | `circuit-body-emission` | `cross_circuit_call(); const x = disclose(arg); assert(map.member(x)); cross_call(x); map.remove(x);` where `arg: Opaque<"string">`. | **M** | Shape is similar to fixtures that pass, but `Map.remove` on an `Opaque<"string">` key may need an explicit walker arm. The G-map fixture (`map_fixture.compact`) likely doesn't exercise `Map.remove` (only `Map.insert` per task #58). One walker arm + the F-set pattern of "5-stmt body with cross-call, disclose, assert.member, cross-call, ADT-op". |
| 5 | 246 | `setSchnorrJubjubVerificationMethod` | `circuit-body-emission` | Same combined shape as gap #3 (`setVerificationMethod`) but the Map's value type is `SchnorrJubjubVerificationMethod` which embeds a `JubjubPoint` (stdlib struct, not a user struct). | **L** | Subsumed by gap #3's closure, plus a one-line check that the struct-with-stdlib-field walker arm handles `JubjubPoint` (zerocash + election exercise stdlib structs but only `MerkleTreePath` / `MerkleTreeDigest` not `JubjubPoint`). |
| 6 | 265 | `removeSchnorrJubjubVerificationMethod` | `circuit-body-emission` | Same as gap #4 (`removeVerificationMethod`) — pattern is identical, only the target Map name differs. | **M** | Closed once gap #4 closes. |
| 7 | 277 | `verifySchnorrJubjubDigestSignature` | `circuit-body-emission` | `assert(active, ...); const x = disclose(arg); assert(map.member(x)); const v = map.lookup(x); Schnorr_schnorrVerifyDigest(arg2, arg3, v.field);`. Key gaps: (a) `Map<Opaque<"string">, struct>.lookup(key)` bound to a `const` (the F1.2/2 task added Set.member + HMT.checkRoot but `Map.lookup`-into-const may not have a walker arm), (b) call to a prefixed-import circuit (`Schnorr_schnorrVerifyDigest` from `import ... prefix Schnorr_`) with a struct-field-access argument (`v.publicKey: JubjubPoint`), (c) the leading `assert(active, ...)` on a Boolean ledger cell. | **L** | The biggest gap of the survey. Closing (a) is one walker arm. Closing (b) is one walker arm — Iter 10 module work already proves prefixed imports flatten correctly in the IR, but no fixture exercises *calling* a prefixed-imported impure circuit from a body. Closing (c) is shared with gap #11 (Boolean ledger-cell reads/writes in body). Likely needs a dedicated jubjub-schnorr fixture + the cross-prefix-call walker arm. |
| 8 | 289 | `setVerificationMethodRelation` | `circuit-body-emission` | `cross_call(); const r = disclose(arg1); const x = disclose(arg2); const m = disclose(arg3); cross_call(m); assert(cross_pure_call(x)); assert(r != Enum.Undefined); const p = cross_pure_call(r, x); if (m == Enum.A) { assert(!p); cross_call(r, x); } else if (m == Enum.B) { assert(p); cross_call(r, x); } cross_call();`. The walker rejects: (a) Boolean result of a cross-circuit *pure* call bound to a `const` then used in an `assert(!p)` AND a branch guard, (b) enum-inequality assert `r != Enum.X` (the E51 work added enum-aware *equality*, not inequality), (c) multiple disclosed enum args used to dispatch different cross-circuit impure calls in if-else-if branches. | **L** | Three sub-gaps. The enum-inequality assert is a tiny walker arm (one rule). The Boolean-cross-call-to-const is a slightly larger arm (needs to materialise the result through the IR-typed value path). The branch-dispatch shape is an extension of Iter 7 + E6.2 walker arms. |
| 9 | 313 | `setService` | `circuit-body-emission` | Same shape as gap #3 (`setVerificationMethod`) — `Map<Opaque<"string">, struct>` with struct-field key, if-else-if Map mutation, post-branch Map.insert. Only the value type differs (`Service` — a 3-Opaque<"string">-field struct). | **L** | Closed once gap #3 closes. |
| 10 | 329 | `removeService` | `circuit-body-emission` | Same shape as gap #4 (`removeVerificationMethod`). | **M** | Closed once gap #4 closes. |
| 11 | 338 | `deactivate` | `circuit-body-emission` | `cross_call(); assert(active, ...); active = false; deactivated = true; cross_call();`. Reduced to a single `active = false;` Boolean cell-write in an exported circuit and still fails. Adding a leading `assert(true, "stub");` does **not** rescue the shape; nor does combining with `deactivated = true;`. | **M** | Boolean ledger-cell write in exported body is not a walker arm. Mirror of gap #1 (`Bytes<N>` cell-write) — same fix shape, different element type. Two walker arms (Boolean + Bytes<N>) or one polymorphic arm. Sealed-ledger fixture (Prod-5a) writes Booleans but only inside the constructor, not an exported circuit body. |

### Gap summary by IR shape (de-duped)

| Shape group | Gaps | Suggested closure |
|---|---|---|
| **A. Single ledger-cell write to non-Uint type in exported body** (Bytes<N>, Boolean) | #1, #11 | One walker arm covering `cell-write` where the RHS is a constant/`pad`/`default<T>` literal — small. |
| **B. ADT mutation on `Opaque<"string">` element/key with a disclose-to-const prologue** (Set.insert, Map.remove, Map.lookup on `Opaque<"string">` key) | #2, #4, #6, #7 (partial), #10 | Extend the existing Set/Map walker arms to dispatch on `Opaque<"string">` element-type — pattern parallels the F-set fixture work but with `Opaque<"string">` instead of `Bytes<32>`. |
| **C. if-else-if Map mutation followed by unconditional Map.insert** (struct value with struct-field key) | #3, #5, #9 | New walker arm for "mutation-then-insert" sequence with struct-field key access. Largest single piece of work; shares closure with the F-map fixture. |
| **D. Prefixed-import circuit call from body + Map.lookup-to-const + Boolean ledger read in assert** | #7 | New jubjub-schnorr fixture + cross-prefix-call walker arm. |
| **E. Enum-inequality assert + Boolean-result-from-cross-pure-call bound to const** | #8 | Two small walker arms; the enum-inequality piece is trivial (Prod-13-class), the cross-pure-call-Boolean piece is medium. |

## What emits cleanly

The constructor and **all 12 non-exported circuits** emit without any modification:

| Circuit | Line | Body shape (validates IR coverage) |
|---|---|---|
| `verificationMethodExists` | 106 | Boolean expr with `||` of two Map.member calls (Map<Opaque<"string">, _>) |
| `controllerKey` | 110 | `return persistentHash<Vector<2, Bytes<32>>>([pad(32, "..."), sk]);` — single hash expr with mixed `pad` + arg |
| `assertController` | 114 | `assert(pure_call(witness()) == ledger_cell, ...)` — Bytes<32> equality assert on ledger cell |
| `assertControllerCanUpdate` | 118 | `cross_call(); assert(ledger_bool, ...);` — two-stmt assert prelude |
| `recordUpdate` | 123 | `counter.increment(1); counter.increment(1); cell = disclose(witness());` — Uint cell-write OK |
| `assertMapMutationDefined` | 129 | enum-equality disjunction in assert |
| `assertSetMutationDefined` | 133 | enum-equality disjunction in assert |
| `assertSupportedVerificationMethod` | 137 | if-else-if-else on **nested-struct-field** enum equality + `assert(false, ...)` final else |
| `verificationMethodRelationMember` | 148 | if-else-if returning Map.member dispatched on enum |
| `insertVerificationMethodRelation` | 166 | if-else-if Set.insert dispatched on enum |
| `removeVerificationMethodRelationFromLedger` | 183 | if-else-if Set.remove dispatched on enum |
| `assertVerificationMethodIsNotReferenced` | 200 | 5×`assert(!set.member(arg), ...)` — Boolean-negation on Set.member |
| `constructor` | 209 | Uint + Bytes<32> + Boolean cell writes interleaved with `disclose(pure_call(witness()))` and `kernel.self()` |

Coverage validation: the non-exported circuits cover persistent-hashing, witness calls, Set.member/insert/remove, Map.member, cross-circuit pure+impure calls, enum-dispatch if-else-if chains, nested-struct-field access, ledger reads of Boolean/Bytes<N>/Counter, and `assert(false, ...)`. The export boundary is what makes the difference, not these individual constructs.

## Workarounds applied

The working copy at `target-gen/did-gap-survey-tree/packages/contract/src/did.compact` has these stubs applied (each replaces the original body; surveyors after me can reproduce partial codegen by applying the same set):

| Line | Circuit | Stub body |
|---|---|---|
| 222 | `rotateControllerKey` | `contractVersion = 1;` |
| 227 | `setAlsoKnownAs` | `contractVersion = 1;` |
| 235 | `setVerificationMethod` | `contractVersion = 1;` |
| 240 | `removeVerificationMethod` | `contractVersion = 1;` |
| 248 | `setSchnorrJubjubVerificationMethod` | `contractVersion = 1;` |
| 253 | `removeSchnorrJubjubVerificationMethod` | `contractVersion = 1;` |
| 264 | `verifySchnorrJubjubDigestSignature` | `contractVersion = 1;` |
| 273 | `setVerificationMethodRelation` | `contractVersion = 1;` |
| 278 | `setService` | `contractVersion = 1;` |
| 283 | `removeService` | `contractVersion = 1;` |
| 288 | `deactivate` | `contractVersion = 1;` |

Every stub is a single Uint<32> ledger-cell write — the **only** single-statement impure body shape the export walker accepts. (Single Set.insert, Map.insert, counter.increment, Bytes<N> cell-write, and Boolean cell-write all fail; only Uint-typed cell-write passes.)

The constructor and all 12 non-exported circuits keep their **original** bodies — they emit cleanly without modification.

Reproduction: copy the working copy back, run `compactc --rust --skip-zk --skip-ts target-gen/did-gap-survey-tree/packages/contract/src/did.compact /tmp/did-gen-attempt`. Expect `/tmp/did-gen-attempt/contract/lib.rs` to match SHA1 `860bc2d2773728df3468bbd9f89e8f9982ed6281` (1320 lines).

## Next-steps recommendation

1. **Close shape group A first (S/M, unblocks gaps #1 + #11)**: A single new "exported-body simple cell-write" walker arm that accepts `cell = literal | default<T> | pad(N, "...")` for any cell-type. This is the smallest patch and immediately unblocks `rotateControllerKey` and `deactivate`. Estimated effort: 1 small Scheme patch + 1 byte-parity fixture (`exported_cell_write_fixture.compact` writing Bytes<8> + Bool + Field, modeled on `sealed_ledger_fixture.compact` but with the writes inside an exported circuit body).

2. **Close shape group B next (M, unblocks gaps #2 + #4 + #6 + #10 + partial #7)**: Extend the Set/Map walker arms to accept `Opaque<"string">` as the element/key type. Pattern matches the F-set fixture closure work (task #42). Estimated effort: 1 Scheme patch parameterising the existing arm + 1 new fixture (`opaque_string_set_map_fixture.compact`). This is the highest-leverage gap by circuit-count.

3. **Defer shape group C (L, gaps #3 + #5 + #9) for a dedicated iteration**: The "if-else-if Map mutation + post-branch Map.insert" pattern is the largest single piece of work — it combines three existing walker arms plus a new "mutation-then-insert" sequence. Should be its own iteration with a dedicated `map_mutation_dispatch_fixture.compact` fixture mirroring `did.compact`'s set/setVerificationMethod shape. Consider also: TS `did.compact` could be refactored to use a single `Map.put(k, v)` style (insert-or-update) if the runtime were extended — would eliminate the gap at the source rather than the codegen.

4. **Defer shape group D (L, gap #7) entirely**: The `verifySchnorrJubjubDigestSignature` body is the only one that calls a prefixed-imported impure circuit AND does a Map.lookup-to-const AND reads a Boolean ledger cell. Three sub-gaps in one circuit, none of them critical for the core DID CRUD operations (this circuit is non-mutating). Stub it in the Rust port and revisit when the Schnorr verification path is wanted. Could be done as a separate Iter 13 with a Schnorr-specific fixture pair.

5. **Skip shape group E pattern recognition (M, gap #8) until after groups A+B+C close**: `setVerificationMethodRelation` has the most complex body (cross-pure-call-to-Boolean-const + enum-inequality assert + branch-dispatch with impure cross-calls). Each sub-gap is small but the combination is fiddly. The body can be refactored in the TS source to eliminate the `currentPresent` binding (inline the call into each branch's guard) — would close the gap without codegen work. Strongly consider the source-refactor path here.

### Practical port plan for midnight-did-rs

For DID-2 (skeleton generation): use the partial lib.rs at `/tmp/did-gen-attempt/contract/lib.rs` as the skeleton. The 11 exported circuits will compile to Rust methods that no-op except for `contract_version = 1;` writes — these are stubs to be filled by hand-written wrappers around the runtime ADT operations until groups A + B + C land in `compactc --rust`. The constructor + 12 non-exported circuits emit faithful bodies and can be used as-is, which gives the Rust port a working domain-layer foundation (controller-key derivation, mutation-dispatch helpers, verification-method existence checks) immediately.
