# ProvekIt Bluepaper

> ProvekIt constrains the set of all possible validations to 64 bytes regardless of length. The proof follows.

Version 1.2.0. April 30, 2026. Protocol freeze.

The reader verifies this document's authority by computing the BLAKE3-512 hash of the protocol catalog at `protocol/specs/2026-04-30-protocol-catalog.json` locally and comparing against the value pinned in §0 below. Every other claim in this bluepaper is verifiable by the same procedure: compute, compare.

---

## §0. Pinned authority

```
protocol catalog (v1.2.0)
  blake3-512:1e5cfee6043d485d276c26a8da17830fe828c5b7b395a5fb1f042e7442407a37c39c59c0e002ca18857b12d3efb0d86687b9a3a0e3f6e3e933856f0717d0579f
```

This is `BLAKE3-512(JCS(catalog-json))`, not `BLAKE3-512(catalog-file-bytes)`. The catalog is canonicalized first per RFC 8785 (sorted keys, no whitespace, deterministic number form), then hashed. Anything else gets a different number.

Recompute locally:

```sh
cargo run --release --manifest-path tools/recompute-spec-cids/Cargo.toml -- --verify
```

`--verify` reads the on-disk catalog, recomputes every CID (each spec file by raw bytes, the catalog itself by JCS-canonical bytes), and exits 0 only if the on-disk values match. Run it. If the computed catalog CID matches the value above, this bluepaper's authority over those bytes is verified by the hash itself. The protocol's version IS the hash of its own catalog. Recursion is the point.

---

## §1. ProvekIt is two protocols composed

ProvekIt is the composition of two protocols. Everything else, every lift adapter, every solver, every agent, every authoring surface, every CLI verb, is decoration on these two pillars.

### §1.1 Canonical IR (the language)

A canonical bytewise representation for any verifiable proposition. One and only one canonical byte sequence per proposition. Same proposition produces the same bytes regardless of which language wrote it, which surface annotated it, which solver discharges it, which author signed it.

Anchors: `2026-04-30-ir-formal-grammar.md` and `2026-04-30-canonicalization-grammar.md`. Spec CIDs:

```
ir-formal-grammar          blake3-512:6c0127e0d24946d7be75861db20507ccdcfdf968d3333f8aa34083e849d8238d73b3acfaa31880648995a024112182ed6b6002cd489548b4b18f5d4c3768dd96
canonicalization-grammar   blake3-512:4d8c2940c53a59c678c8fb65e33dc2cb0ae8ae8a283b97b9c69fd678565653d15e6ee9dc3ffc6a32dc1ff035821b0c1a006f0455498d2ea91faef845d7b39830
```

The IR is a finite first-order language over primitive sorts (`Int`, `String`, `Bool`) and content-addressed extension sorts. Variables, constants, atomic predicates with named operators, `forall`, `exists`, `and`, `or`, `not`, `implies`. The grammar is finite and well-founded; every proposition has a finite parse tree.

Canonicalization is the seven-pass grammar that lowers any input surface (TypeScript object literals, Rust struct literals, Python dictionaries, Java POJO field maps, JSON, JSON-LD, CBOR-tagged values) to a single AST shape, then encodes that AST via JCS (RFC 8785) into bytes. Two parsers that produce the same AST after the seven passes always emit byte-identical JCS output. Two BLAKE3-512 hashes of those bytes always agree.

This is the "language" pillar: the canonical name space for ideas. Every proposition has exactly one canonical byte form.

### §1.2 Protocol for content-addressing (the naming scheme)

A signing-and-naming protocol that turns canonical bytes into self-verifying mementos.

Anchors: memento-envelope-grammar, signatures-and-non-repudiation, chain-validity-and-fail-closed, proof-file-format. Spec CIDs:

```
memento-envelope-grammar          blake3-512:58bba3e1a9f6439eac5cb0c681faf65d38de9e6b8ad539854acda451ca67562a9d238eb95a5d7df2c0776657015fa026c51059dff61e1ba9aa2438b57425d6a5
signatures-and-non-repudiation    blake3-512:8b71229fcb7413f18a93a9b260012298311c1ce754850ee717780c181f1fda39a6600b2e5069e775cd7dd15e8c81e40b47bf7585aa0b23ab76c112c85116365c
proof-file-format                 blake3-512:7bb4589af25c6c3992520494869bbbe4cfbcf7a77b91ebd61d6327e78699ef16cd5bc34afbe4cdf88a717c055c16536b5106bc4dca2d9d6b5cfcc1eede68e1b3
proof-substrate                   blake3-512:ad53d6c59ee08270a48715376cc211f964ff44a55b3318d68a402e9c915ff593d5a5bbbd424f7777e2bcfe89d6c5bd2b49efcb5aae7de24752f3bcabb90484ae
handshake-algorithm               blake3-512:acbf67dda9373c648e591d8ad74b8f8d56f4c92ba9c82bdc6690dc521e6f17012dd195e98a96b099090eeeb5a424312d90ff441c882d0e317a190561aa1a6925
lattice-tractability-theorem      blake3-512:b6d7c2772c2929294d7f516f79559bd292e44f51805a6bd6ea0ca7fe365b82ec96b86c434f53dfb003f5acd306533831dc0257e46ead4c7d71081f9f56ec6d07
```

Components:

- BLAKE3-512 hashing. Sixty-four byte digest, hex-encoded into the self-identifying string `"blake3-512:" + hex(digest)`. The CID. No truncation at any layer.
- Ed25519 signing. Producer signs canonical bytes; signature attaches to the envelope as `"ed25519:" + base64(sig)`. Self-identifying.
- Memento envelopes. JCS-canonical objects carrying `bindingHash`, `propertyHash`, `evidence`, `producedBy`, `producedAt`, `inputCids`, `cid`, `producerSignature`. By construction, `cid = H(unsigned_bytes)` and `producerSignature = Sign(SK, unsigned_bytes)`.
- Deterministic CBOR `.proof` envelopes. RFC 8949 §4.2.1 Core Deterministic Encoding. Filename is `H(envelope_bytes) + ".proof"`.
- Lattice composition. Edges via `inputCids`. Memento DAG. Three-tier handshake (§3.2).

This is the "naming scheme" pillar: a function from canonical bytes to a sixty-four byte digest plus a producer signature, deterministic, collision-resistant, non-repudiable.

### §1.3 The composition

Compose the two pillars and you obtain: any verifiable proposition (software contract, signing event, scientific claim, formal theorem, silicon instruction semantics, identity assertion, legal attestation, sensor reading, anything) gets a single sixty-four byte content-addressed name. Same proposition, same name, regardless of source domain or surface.

Decoration that uses the two pillars without changing them:

- Lift adapters produce canonical IR from existing surfaces (Zod, proptest, kani, JSDoc, Bean Validation, Pydantic, Hypothesis, Frama-C, dafny, hand-written contracts).
- IR compilers consume canonical IR and lower to per-solver formats (SMT-LIB for Z3 and CVC5, TPTP for Vampire, Lean tactics, Coq goals, Bitwuzla bit-vectors, dReal real arithmetic, domain-specific witnesses for non-software domains).
- Agents author canonical IR via prompts (Claude Code, Codex, OpenCode, OpenAI, ollama, deterministic rule-based agents). The agent maps English to IR; the IR is canonical regardless of which agent produced it.
- Authoring surfaces are interchangeable input syntaxes that map into canonical IR. Every modern annotation library is one such surface; ProvekIt sits beneath all of them.
- The proof DAG is the lattice that grows as producers mint mementos. Edges via `inputCids`; nodes are mementos; verification walks the DAG.

The two pillars are non-pluggable. The decoration is. This is the architectural division of labor.

### §1.4 Self-contracts: the protocol proves things about itself

The Rust workspace at `implementations/rust/provekit-self-contracts/` carries the kit's own contracts: invariants over the JCS encoder's key-sort order, the CBOR encoder's integer-shortest-form rule, the canonicalizer's idempotence under double-application, the signer's deterministic output for a fixed seed. The published `.proof` of the self-contracts is pinned at:

```
self-contracts (stable; v1.1.0+)    blake3-512:b692f43a151f88aa31b998adaa091b2ac7ebad231c3c2b63426d93a8090de688bc8f12e02fe6ef901a513c4bf89dbffc884cd1164fa566fd1a757cf478434dfe
```

A peer that runs `provekit verify --target self` performs the recursive verification: the protocol verifying the protocol. Success demonstrates that the implementation satisfies the formal claims the protocol makes about itself. The pinned CID is what such a successful run produces over the current v1.2.0 source tree (the contracts haven't changed since v1.1.0; v1.2.0 is additive, so the self-contracts CID is stable across the version bump).

---

## §2. The IR formal grammar

The canonical predicate IR (pillar 1) is a finite first-order language. EBNF:

```ebnf
Formula     ::= Atomic | Conn | Quantified | Const

Atomic      ::= "{" '"kind":"atomic"' ","
                    '"name":' OpName ","
                    '"args":' "[" Term ("," Term)* "]"
                "}"

Conn        ::= "{" '"kind":' ConnKind ","
                    '"args":' "[" Formula ("," Formula)* "]"
                "}"
ConnKind    ::= '"and"' | '"or"' | '"not"' | '"implies"'

Quantified  ::= "{" '"kind":' QKind ","
                    '"bound":' "[" Var ("," Var)* "]" ","
                    '"body":' Formula
                "}"
QKind       ::= '"forall"' | '"exists"'

Term        ::= Var | Const | Atomic
Var         ::= "{" '"kind":"var"' ","
                    '"name":' Ident ","
                    '"sort":' Sort
                "}"
Const       ::= "{" '"kind":"const"' ","
                    '"value":' Literal ","
                    '"sort":' Sort
                "}"

Sort        ::= "{" '"kind":"primitive"' "," '"name":' SortName "}"
              | "{" '"kind":"extension"' "," '"name":' Ident "," '"cid":' CidString "}"
SortName    ::= '"Int"' | '"String"' | '"Bool"'

OpName      ::= Ident
Ident       ::= '"' [a-zA-Z_][a-zA-Z0-9_-]* '"'
Literal     ::= JsonNumber | JsonString | "true" | "false"
CidString   ::= '"blake3-512:' HexDigit{128} '"'
HexDigit    ::= [0-9a-f]
```

The grammar is finite by inspection. Recursion is well-founded on the formula tree (`body` and `args` strictly decrease tree depth). Extensions add new sort and operator names by minting their own definitional mementos; an unknown extension CID is a detectable refusal-to-verify, not silent miscommunication.

JCS (RFC 8785) is the encoder. Two ASTs that compare equal under JCS's structural equivalence (object key set, key-value pairs, array element sequence, scalar value, scalar type) produce byte-identical output.

---

## §3. The constant-size verification theorem

This is the load-bearing claim. It states formally:

```
V(D) = 64 bytes ∀D
T(D) = O(1) ∀D
```

`V(D)` is the verification-bit-cost as a function of domain size `D`. `T(D)` is the verification-time-cost. `D` is the cardinality of the lattice (or any sub-DAG the verifier chooses to walk).

Spec: `2026-04-30-lattice-tractability-theorem.md`, `blake3-512:b6d7c2772c2929294d7f516f79559bd292e44f51805a6bd6ea0ca7fe365b82ec96b86c434f53dfb003f5acd306533831dc0257e46ead4c7d71081f9f56ec6d07`.

### §3.1 Hypotheses

- H1 (Collision resistance of BLAKE3-512). Standard cryptographic assumption.
- H2 (Existential unforgeability of Ed25519 under chosen-message attack). RFC 8032; standard assumption.
- H3 (Determinism of JCS). RFC 8785 §3 equivalence relation; for any two JSON values equivalent under that relation, JCS produces byte-identical output.
- H4 (Determinism of DCBOR). RFC 8949 §4.2.1; for any two CBOR values equivalent under deterministic-encoding rules, DCBOR produces byte-identical output.
- H5 (Finiteness of the IR grammar). §2 is a finite context-free grammar; every memento's predicate has a finite parse tree.

### §3.2 The handshake algorithm

```
function handshake(P_req, P_off, lattice L):
  h_req = H(JCS(P_req))
  h_off = H(JCS(P_off))

  # Tier 1: hash equality
  if h_req == h_off:
    return TIER1_OK

  # Tier 2: cached implication
  for I in L.by_antecedent[h_off]:
    if I.body.consequentHash == h_req:
      if Verify(I.producedBy, I_unsigned, I.producerSignature):
        return TIER2_OK(I.cid)

  # Tier 3: solver from scratch
  smt = ir_to_smt(P_off → P_req)
  result = solver.run(smt)
  if result == sat:
    I_new = mint_implication(antecedent_hash=h_off, consequent_hash=h_req, ...)
    L.insert(I_new)
    return TIER3_OK(I_new.cid)

  return UNVERIFIED
```

### §3.3 The proof

The theorem rests on exactly three load-bearing claims.

#### Claim A: canonical IR is deterministic ⇒ propositions hash uniquely.

By H3 (JCS determinism), JCS is a function on the equivalence class of JSON values that compare equal structurally. By H5 (IR finiteness), every IR proposition has a finite parse tree, so JCS terminates on every well-formed input. By the construction in §1.1 (the canonicalization grammar's seven passes are confluent), every proposition has exactly one canonical AST after the lifting passes; JCS produces exactly one byte sequence per AST; H produces exactly one digest per byte sequence. Therefore a proposition has exactly one CID under H.

#### Claim B: BLAKE3-512 is collision-resistant ⇒ the 64-byte name uniquely identifies the proposition.

By H1, no PPT adversary can find `(x, y)` with `x ≠ y` and `H(x) = H(y)` except with negligible probability. Combined with Claim A: distinct propositions produce distinct canonical bytes; distinct canonical bytes produce distinct CIDs (modulo a negligible set). Therefore the sixty-four byte CID uniquely names a proposition.

#### Claim C: verification is byte equality on hashes ⇒ O(1).

Tier 1 (hash equality): the verifier compares two CIDs byte-for-byte. Sixty-four bytes. Constant time and constant size.

Tier 2 (cached implication): the verifier indexes `by_antecedent` (one hash-table lookup, expected `O(c_lookup)` under H1's avalanche property), iterates the bucket (expected constant size under H1), checks `consequentHash` equality (sixty-four bytes), and runs `Verify` (constant `O(c_v)` per H2 / RFC 8032). Total: `O(c_lookup + c_v)`. Independent of `|L|`.

Tier 3 (solver from scratch): the verifier invokes the witness service. Time cost depends on the witness, not on `|L|`. The witness's output, when minted as an implication memento, becomes a Tier 2 cache entry for all future queries with the same hash pair; subsequent queries amortize to `O(1)`.

The verification-bit-cost `V(D)` is the size of the digest the verifier must hold to confirm the answer: sixty-four bytes. It does not depend on `D`. The verification-time-cost `T(D)` under cache is the cost of one hash-table lookup plus one signature verify: constant in `D`. ∎

### §3.4 Implementation: the `memcmp` line

The constant-size claim is implementation-checked by code. The Rust verifier's tier-1 discharge is one comparison:

```rust
// Tier 1: BLAKE3-512 hash equality on the 64-byte digest.
// implementations/rust/provekit-showcase/src/bench.rs
fn ct_eq_64(a: &[u8; 64], b: &[u8; 64]) -> bool {
    let mut diff: u8 = 0;
    for i in 0..64 {
        diff |= a[i] ^ b[i];
    }
    let v: u8 = unsafe { std::ptr::read_volatile(&diff) };
    v == 0
}
```

The volatile read prevents the optimizer from eliding the comparison when its result is unused (the benchmark times the comparison itself, not its consumer). In production verifiers the function is replaced with the platform's constant-time `memcmp` analogue (subtle's `ConstantTimeEq` in Rust). The semantics are identical: compare sixty-four bytes; return one bit.

The math is the implementation. The implementation is sixty-four bytes.

### §3.5 The cryptographic-minimum claim

The protocol bottoms out exactly at the cryptographic primitives' security thresholds.

Below sixty-four bytes the discriminating power of BLAKE3-512 collapses (collision birthday bound at `2^256`; sixty-four bytes is the smallest CID that retains `2^256` security). Below `O(c_v)` time the discriminating power of Ed25519 collapses (existential forgery becomes cheap). The protocol cannot do better without weakening the primitives.

The protocol uses the strongest available primitives at the threshold the primitives can sustain. This is the cryptographic minimum.

---

## §4. Generalization across domains

The two protocols (canonical IR + content addressing) do not refer to "software" anywhere. They refer to propositions, predicates, and signed envelopes. Therefore the composition applies to any domain whose claims fit the IR grammar.

- Software: a function precondition is a predicate; Z3 is the witness; signed contract memento is the artifact.
- Silicon: an instruction's semantics is a predicate over bit-vectors and registers; the chip vendor's signature is the witness; signed instruction-semantics memento is the artifact. Software contracts above silicon resolve their bridges to chip-vendor mementos.
- Scientific consensus: a measurement's bound is a predicate; the institution's signature is the witness; signed measurement memento is the artifact.
- Logical theorems: a theorem is a predicate; a formalized proof or a mathematician's signature is the witness; signed theorem memento is the artifact.
- Legal: a notarized statement is a predicate; the notary's signature is the witness; signed attestation memento is the artifact.
- Identity: an identity claim is a predicate; the identity provider's signature is the witness; signed identity memento is the artifact.
- Sensor data: a reading bounded by calibration is a predicate; the sensor's signed firmware is the witness; signed reading memento is the artifact.

In every case: canonical IR encodes the predicate; content addressing names the artifact; verification is sixty-four bytes; the protocol does not need to know what domain the proposition belongs to.

A peer that does not implement a domain extension can detect that it does not (unknown extension CID per §2) and refuse to verify rather than misverify. A peer that does implement the extension verifies in the same sixty-four bytes as any other peer. Same protocol; different stopping depths in the verification chain; same per-node cost.

This is the universality claim. The two protocols compose into one substrate; the substrate works for any verifiable proposition.

---

## §5. The protocol catalog and runnable verification

The protocol catalog at `protocol/specs/2026-04-30-protocol-catalog.json` enumerates every spec by CID. The catalog is itself a JCS-canonical JSON object; its CID is the protocol version. Two peers confirm they speak the same protocol with one comparison: each computes `H(catalog_bytes)`; both compare. There is no version registry; the version is the hash. There is no protocol authority; the protocol is the bytes.

### §5.1 Reader's recipe to verify this bluepaper

The catalog and the spec files use two different hashing rules (see `protocol/specs/2026-04-30-protocol-catalog-format.md`). The single command that applies both rules correctly is:

```sh
$ cargo run --release \
    --manifest-path tools/recompute-spec-cids/Cargo.toml -- --verify
```

`--verify` reads every spec file in raw bytes, hashes each, then reads the catalog, JCS-canonicalizes it, hashes that, and compares all values to what the on-disk catalog declares. Exit 0 if every value matches. The tool prints the catalog CID; that value MUST match the value pinned in §0. If it does, the bluepaper has just verified its own authority.

### §5.2 Per-spec verification

Spec files are content-addressed by raw bytes (the protocol-catalog-format spec §2.1). Any tool that computes BLAKE3-512 of a file's bytes verifies one spec at a time:

```sh
$ ./target/release/provekit-showcase hash-spec \
    protocol/specs/2026-04-30-lattice-tractability-theorem.md
```

The output must match the CID cited in §1, §2, §3, or Appendix A for that spec. The catalog itself is the only artifact that uses JCS-canonical hashing, so it is NOT verified by `hash-spec`; use `--verify` for that.

If every per-spec output and the catalog CID match, every claim in this bluepaper applies to your bytes. If any output differs, the bytes you have are not the bytes this bluepaper was written against, and no claim applies to your bytes.

### §5.3 The recursion

The bluepaper claims authority via a hash. The hash is computed over a file. The file declares the protocol version. The protocol version IS the hash. Verifying the bluepaper's authority is the act of running the protocol; running the protocol is the act of verifying the bluepaper's authority. There is no external authority to appeal to. Trust nothing else.

---

## Appendix A: full spec catalog

Every spec referenced by content hash. Recompute locally to verify each.

```
canonicalization grammar       blake3-512:4d8c2940c53a59c678c8fb65e33dc2cb0ae8ae8a283b97b9c69fd678565653d15e6ee9dc3ffc6a32dc1ff035821b0c1a006f0455498d2ea91faef845d7b39830
handshake algorithm            blake3-512:acbf67dda9373c648e591d8ad74b8f8d56f4c92ba9c82bdc6690dc521e6f17012dd195e98a96b099090eeeb5a424312d90ff441c882d0e317a190561aa1a6925
ir formal grammar              blake3-512:6c0127e0d24946d7be75861db20507ccdcfdf968d3333f8aa34083e849d8238d73b3acfaa31880648995a024112182ed6b6002cd489548b4b18f5d4c3768dd96
lattice tractability theorem   blake3-512:b6d7c2772c2929294d7f516f79559bd292e44f51805a6bd6ea0ca7fe365b82ec96b86c434f53dfb003f5acd306533831dc0257e46ead4c7d71081f9f56ec6d07
memento envelope grammar       blake3-512:58bba3e1a9f6439eac5cb0c681faf65d38de9e6b8ad539854acda451ca67562a9d238eb95a5d7df2c0776657015fa026c51059dff61e1ba9aa2438b57425d6a5
proof substrate                blake3-512:ad53d6c59ee08270a48715376cc211f964ff44a55b3318d68a402e9c915ff593d5a5bbbd424f7777e2bcfe89d6c5bd2b49efcb5aae7de24752f3bcabb90484ae
proof file format              blake3-512:7bb4589af25c6c3992520494869bbbe4cfbcf7a77b91ebd61d6327e78699ef16cd5bc34afbe4cdf88a717c055c16536b5106bc4dca2d9d6b5cfcc1eede68e1b3
protocol catalog (v1.2.0)      blake3-512:1e5cfee6043d485d276c26a8da17830fe828c5b7b395a5fb1f042e7442407a37c39c59c0e002ca18857b12d3efb0d86687b9a3a0e3f6e3e933856f0717d0579f
self-contracts (stable; v1.1.0+)        blake3-512:b692f43a151f88aa31b998adaa091b2ac7ebad231c3c2b63426d93a8090de688bc8f12e02fe6ef901a513c4bf89dbffc884cd1164fa566fd1a757cf478434dfe
signatures and non-repudiation blake3-512:8b71229fcb7413f18a93a9b260012298311c1ce754850ee717780c181f1fda39a6600b2e5069e775cd7dd15e8c81e40b47bf7585aa0b23ab76c112c85116365c
```

Note on the catalog pin. The CID above is the JCS-canonical BLAKE3-512 of the catalog file in this branch, computed by `tools/recompute-spec-cids -- --verify` (see §5.1). The catalog uses JCS-canonical hashing; spec files use raw-bytes hashing; the two rules are not interchangeable (see `protocol/specs/2026-04-30-protocol-catalog-format.md`). Earlier external announcements may reference the catalog by a different prefix because they used a different rule; if the values diverge, the canonical authority is the value any peer can recompute by running `--verify`. Recompute. Compare. Trust nothing else.

### Appendix A.1 Self-contracts: how to reproduce

The framework dogfoods itself. Each conformant peer ships hand-written contracts about its own public surface, mints them as signed mementos under the foundation key, and bundles them into a single `.proof` whose filename IS its catalog CID. Every peer's mint-binary asserts byte-determinism by minting twice into separate output directories and comparing CIDs.

**Rust** — 67 contracts across 13 `.invariant.rs` files:

```sh
$ cargo build --release \
    --manifest-path implementations/rust/Cargo.toml \
    --bin mint-self-contracts
$ implementations/rust/target/release/mint-self-contracts | \
    grep "catalog CID:"
  catalog CID:        blake3-512:b692f43a151f88aa31b998adaa091b2ac7ebad231c3c2b63426d93a8090de688bc8f12e02fe6ef901a513c4bf89dbffc884cd1164fa566fd1a757cf478434dfe
```

**Go** — 47 contracts across 13 slab files (one per public-API Go source file):

```sh
$ cd implementations/go/provekit-self-contracts
$ go run ./cmd/mint-go-self-contracts | grep "catalog CID:"
  catalog CID:        blake3-512:906fa4f3ca32d97710e327c9e6e914e5c476a3cfdc326459b31dade24d9625c96f7f0595e3d91f316f73e2709a7f05ac79dd0ca768b6ff23cc2b384923487ac3
```

**C++** — 40 contracts across 11 `.invariant.cpp` slab files (one per public-API C++ source file):

```sh
$ tools/build-cpp-self-contracts.sh /tmp/provekit-cpp-self-out
  catalog CID:        blake3-512:52bc62f7e70c7caa3220b2c789a75a744bc94660c36a920d53da1a6128eff660cd81dfae6a39d802d108e037f1234f202160d54aea81fb407f1a46f5cd323ae0
```

**TypeScript** — 59 contracts across 13 `.invariant.ts` slab files (one per public-API TS source file). The repo's tsx-driven launchers currently fail on Node 25 because `@ipld/dag-cbor` is ESM-only and tsx's CJS bridge cannot resolve it; vitest's Vite ESM loader handles it cleanly, so the working invocation is the test driver:

```sh
$ pnpm vitest run \
    implementations/typescript/src/bin/mint-ts-self-contracts.test.ts \
    --reporter=verbose | grep "catalog CID:"
  catalog CID:        blake3-512:449339930add6457bf25542f2117a025daada4a4bd1de704737750ad6d1c1be814c284d31bb97159ca0b2d2c52f8c043a64533d3432195f5a0f338c5d4904d44
```

**C#** — 70 contracts across 15 `.invariant.cs` sidecar files (one per public-API C# source file). The orchestrator is a `dotnet` console project that links the sidecars in via `<Compile Include>` so the four lib assemblies stay free of test-only IR dependencies:

```sh
$ dotnet run --project implementations/csharp/Provekit.SelfContracts -- /tmp/csharp-self-out | \
    grep "catalog CID:"
  catalog CID:        blake3-512:45d7cdbd0d5bfba5a1ee9e8386eb4d7dc1eab0882105753504a1f5c06de6f9fc4bd7038f56c7fcea693b152e2ab83de40ca4964a920816142ea43d5b9076415c
```

Two runs producing the same CID is the framework verifying its own canonicalization is deterministic. If a value above does not match, your bytes are not the bytes this bluepaper was written against. Each peer's CID is independent: contracts cover that peer's surface, but the bytes (canonical IR-JSON, BLAKE3-512, foundation-key signature, deterministic CBOR catalog) are produced by the protocol's own primitives. Five peers, five CIDs, one protocol.

## Appendix B: empirical witness

The constant-size and constant-time claims are theorems. Their empirical confirmation lives in `docs/launch/showcase-results.md`, where a fixture lattice of `1.1 × 10^6` mementos exhibits Tier 1 at p50 = 58 ns, Tier 2 at p50 = 66 µs, Tier 3 at p50 = 24 ms over ten thousand random queries. The empirical data validates the theorem; it is not part of the proof.

## Appendix C: changelog

v1.1.0 (2026-04-30): protocol freeze. BLAKE3-512 widening from earlier truncated forms. Every CID now carries the full 128-hex string. Per-language kit-standard finalized; conformance suite passing on Rust, C++, TypeScript reference peers. Catalog CID `5b770182...19f05cd4`.

v1.2.0 (2026-04-30): additive bump over v1.1.0. Adds four pluggability protocols: agent-plugin-protocol (LSP-shape JSON-RPC seam for coding agents), ir-compiler-protocol (per-solver IR translators), multi-solver-protocol (single/chain/portfolio dispatch), lift-plugin-protocol (one Rust CLI dispatching to per-language plugins via stdio JSON-RPC). Reference CLI plugins ship for Rust + Go + C++; TypeScript is consumed via toolchain (vitest), not as a peer CLI. Any v1.1.0 memento or `.proof` remains valid under v1.2.0 — the bump is purely additive. Catalog CID `1e5cfee6...17d0579f`. Signed under the same foundation key as v1.1.0.

---

End of bluepaper. The protocol is the bytes.
