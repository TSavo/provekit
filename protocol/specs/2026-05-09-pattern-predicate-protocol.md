# Pattern Predicate Protocol (PPP)

**Status:** v0.1.0 draft
**Date:** 2026-05-09
**Layer:** extension protocol over the proof substrate, the lift-plugin-protocol, the obligation realizer (ORP), the fix receipt (FRP), and proofchain composition

## Section 0. Purpose

PPP defines how an editorially-named bug class becomes a content-addressed
substrate query, how that query's output becomes a memento, and how the
output's delta across two substrates discharges a fix receipt's policy.

Existing protocols name what a witness IS (proofchain), what a fix receipt
IS (FRP), what a substrate IS (proof substrate), and how lifters fill it
(lift-plugin-protocol). PPP names the missing edge: **how a producer
authors the policy a fix receipt cites, with cryptographic identity, in a
form a verifier can re-run locally.**

The wire-level claim PPP makes:

```text
pattern (editorial)  ->  predicate (mechanical)  ->  query application  ->  result set
                                                                                |
                                                  pre-lift result \ post-lift result
                                                                                |
                                                                       closure witness
                                                                                |
                                                                          FRP receipt
```

Every arrow in that pipeline is a content-addressed memento with an Ed25519
producer signature. The resulting receipt is locally verifiable, federated
across languages, and admissible to a proofchain head under verifier policy.

## Section 1. Relation to existing protocols

| Existing artifact | PPP role |
|---|---|
| Proof substrate (lifter output) | The relations a predicate binds to. |
| Lift-plugin-protocol (C1-C8 conformance) | The trust root that makes substrate facts admissible. |
| ORP `transform` mode | Produces the candidate bytes a PPP query application observes pre/post. |
| FRP `policyCid` | A predicate CID, surfaced through PPP's `predicate` field. |
| FRP `closureWitnessCid` | The result-set delta CID, defined by Section 5. |
| Proofchain | PPP receipts are links in a chain; the head carries closure under verifier policy. |
| Contract Composition Protocol (CCP, `2026-05-09-contract-composition-protocol.md`) | CCP produces ComposedFunctionContract mementos that PPP MAY query as a substrate relation. CCP also defines the canonical compose primitive whose CIDs PPP federation depends on per Section 7. A `policyCid` MAY reference a ComposedFunctionContract CID when the policy is a chain-level guarantee rather than a per-function predicate. |

PPP does not replace FRP. FRP names the receipt; PPP names the predicate
the receipt cites and the delta the receipt witnesses. Without PPP, FRP's
`policyCid` is unspecified content and `closureWitnessCid` is unspecified
shape. With PPP, both have a wire format.

## Section 2. The pattern, the predicate, and the compilation arrow

A **pattern** is editorial: a named, prose-defined bug class with a stable
slug, a `short_description`, optional kinships to siblings, and a published
provenance. A pattern's CID is computed over the canonical bytes of its
declaration record. Pattern declarations live in a public catalog signed by
the producer who maintains the catalog.

A **predicate** is mechanical: a deterministic query against the substrate
schema that, given a substrate, returns a finite set of `(locus, evidence)`
pairs. A predicate's CID is computed over the canonical bytes of its
expression and the explicit substrate-schema version it binds against.

The compilation arrow is the producer-signed claim:

```text
patternCid    @policy/borrowed-pages-as-scratch
predicateCid  blake3-512:<sql + schema version>
schemaVersion sugar-substrate/v1
producer      <ed25519 fingerprint>
compiledAt    <ISO-8601 timestamp>
```

The producer signs the (patternCid, predicateCid, schemaVersion) tuple as a
single memento. Multiple predicates may compile the same pattern at
different precision levels (v1, v2, ...); each is its own memento.

Different producers may compile the same pattern independently. Their
predicates have different CIDs. A verifier admits whichever ones its
policy trusts. Catalog convergence is social, not protocol-mandated.

## Section 3. Predicate authoring

A predicate is a query expression. PPP requires three properties:

1. **Determinism.** Given the same substrate bytes, the predicate produces
   bit-identical result-set bytes. SQL with explicit `ORDER BY` over a
   stable canonical column set satisfies this; queries that depend on
   non-stable orderings, locale, or wall-clock time do not.

2. **Substrate-schema binding.** The predicate names the substrate schema
   version it requires. Substrate evolution is governed by the protocol-
   versioning spec; predicates that bind to v1 may be admissible against
   v2 substrates only via a substrate compatibility memento that asserts
   the v1-relevant relations have v2-equivalent shape.

3. **Closed extension.** A predicate's code references only the substrate
   relations and the deterministic query language. It MUST NOT call out to
   external services, network resources, or non-deterministic functions.
   A predicate that does any of those is not a PPP predicate; it is an
   imperative procedure that may inform a candidate but cannot serve as
   the policy a fix receipt cites.

The canonical query language for the v1 schema is SQLite-3 SQL with
JSON1, plus an explicit list of admitted built-in functions (Section 9).
Future schema versions MAY admit additional query languages (Datalog,
Cypher, Z3-SMT) provided each carries its own determinism, schema-
binding, and closed-extension proofs.

### 3.1 Predicate schema (v1)

A v1 substrate exposes the following relations. All columns are typed and
their canonical encoding is fixed.

```sql
CREATE TABLE call_edges (
  caller_function TEXT NOT NULL,
  callee_name     TEXT NOT NULL,
  args            TEXT,             -- JSON array of {position, kind, text}
  callsite_path   TEXT NOT NULL,
  callsite_line   INTEGER NOT NULL,
  callsite_column INTEGER NOT NULL
);

CREATE TABLE functions (
  name        TEXT PRIMARY KEY,
  path        TEXT NOT NULL,
  line        INTEGER NOT NULL,
  has_body    INTEGER NOT NULL CHECK (has_body IN (0,1))
);

CREATE TABLE contracts (
  function    TEXT NOT NULL,
  kind        TEXT NOT NULL,        -- e.g. c-kernel-doc.param.nonnull
  binding     TEXT,
  source_cid  TEXT NOT NULL         -- the lifter that produced this row
);

CREATE TABLE lifted_files (
  path        TEXT PRIMARY KEY,
  edge_count  INTEGER NOT NULL,
  source_cid  TEXT NOT NULL         -- CID of the source bytes lifted
);

CREATE TABLE effects (
  function    TEXT NOT NULL,
  kind        TEXT NOT NULL,        -- Reads | Writes | Io | Unsafe | Panics | UnresolvedCall
  target      TEXT,                 -- named target for Reads/Writes/UnresolvedCall; NULL for Io/Unsafe/Panics
  source_cid  TEXT NOT NULL         -- CID of the lifter that produced this row
);

CREATE TABLE composed_contracts (
  composed_cid    TEXT PRIMARY KEY, -- CID of the ComposedFunctionContract memento per CCP
  chain           TEXT NOT NULL,    -- JSON array of atomic FunctionContractMemento CIDs in call-graph order
  effect_set_cid  TEXT NOT NULL,    -- CID of the disjoint-union effect set (empty for pure compositions)
  ccp_version     TEXT NOT NULL,    -- e.g. "1.0.0"
  source_cid      TEXT NOT NULL     -- CID of the producer that materialized the composition
);
```

`effects` is populated by per-language effects extraction (CCP §3 prerequisite).
`composed_contracts` is populated by CCP-canonical composition, materialized
eagerly at lift time or lazily during prove (CCP §4).

Future versions extend this set. Adding columns to existing relations is
a substrate-schema breaking change requiring a v2 designation. Adding new
relations is non-breaking; `effects` and `composed_contracts` were
non-breaking additions to the v1 schema landed 2026-05-09 alongside CCP
v1.0.0.

### 3.2 Predicate result row

Every predicate result row has a stable shape:

```json
{
  "locus": {"path": "...", "line": N, "function": "...", "fragment": "..."},
  "evidence": {"...arbitrary JSON the predicate emits..."}
}
```

The result set is the JSON array of result rows ordered by canonical
locus key. The CID of the result set is BLAKE3-512 over the canonical
bytes of that array.

## Section 4. Query application

A query application is a triple:

```
applicationCid = blake3-512(canonical(predicateCid, substrateCid))
```

The query application memento includes:

```json
{
  "kind": "PpQueryApplication",
  "schemaVersion": "1",
  "predicateCid": "blake3-512:...",
  "substrateCid": "blake3-512:...",
  "lifterCid":   "blake3-512:...",
  "resultSetCid": "blake3-512:...",
  "rowCount": <int>,
  "executedAt": "<ISO-8601>",
  "producer": {"kind": "ci|local|tool", "name": "...", "version": "..."},
  "signature": "<ed25519 over canonical bytes of the above fields>"
}
```

`lifterCid` is mandatory. A query application is locally verifiable only
when the verifier holds the lifter that produced the substrate, the
substrate bytes, and the predicate bytes. Without `lifterCid`, the
substrate bytes are unauthored and the application cannot be re-run.

The `rowCount` is convenience. It MUST equal the length of the result
set deserialized from the bytes hashed into `resultSetCid`. A verifier
that finds disagreement MUST fail closed.

A query application is REPRODUCIBLE: any party holding the lifter, the
source bytes, and the predicate bytes MUST be able to recompute every
CID in the application memento and compare to the stored values. This
is the local-verifiability promise.

## Section 5. Closure witness

A closure witness binds two query applications over the same predicate
into a single evidence object:

```json
{
  "kind": "PpClosureWitness",
  "schemaVersion": "1",
  "predicateCid": "blake3-512:...",
  "preApplicationCid": "blake3-512:...",
  "postApplicationCid": "blake3-512:...",
  "preResultSetCid": "blake3-512:...",
  "postResultSetCid": "blake3-512:...",
  "closure": {
    "shape": "empty | strict-subset | unchanged | grew | non-monotonic",
    "closedRows": [<canonical locus keys>],
    "remainingRows": [<canonical locus keys>],
    "newRows": [<canonical locus keys>]
  },
  "producer": {...},
  "signature": "..."
}
```

The `closure.shape` is computed deterministically from the result-set
deltas:

| pre rows | post rows | shape |
|---|---|---|
| ∅ | ∅ | unchanged (vacuous) |
| R, R ≠ ∅ | ∅ | empty (full closure) |
| R | S, S ⊊ R | strict-subset (partial closure) |
| R | R | unchanged |
| R | S, S ⊋ R | grew |
| R | S, S ∩ R ≠ ∅ ∧ S \ R ≠ ∅ ∧ R \ S ≠ ∅ | non-monotonic |

A closure witness is **discharging** for a fix receipt iff `shape ∈
{empty, strict-subset}` and the receipt's `gapCid` resolves to a row
key in `closedRows`.

A closure witness with `shape = grew` or `non-monotonic` SHOULD be
treated as a regression signal under any sensible verifier policy.
Producers SHOULD attach such witnesses to commits even when no fix
receipt is claimed; verifiers can then refuse merges that introduce
new predicate hits.

## Section 6. Composition with FRP

An FRP receipt that cites a PPP predicate populates as:

```json
{
  "kind": "FixReceipt",
  ...
  "policyCid": "<predicateCid>",
  "gapCid": "<row key from preResultSetCid that this receipt closes>",
  "closureWitnessCid": "<PpClosureWitnessCid>",
  ...
}
```

A receipt MAY cite multiple predicates by emitting one FRP receipt per
(predicate, gap) pair, all signed by the same producer over the same
patch. A patch that closes N gaps under M predicates produces N×M
receipts in the producer's signed set.

PPP does not change FRP's nontriviality rule. A receipt is still
nontrivial only when `preArtifactCid != transformedArtifactCid` and
the closure witness is discharging under Section 5. PPP gives those
fields their content shape; FRP gives them their merge-gate authority.

## Section 7. Cross-language federation

A predicate is federable iff it binds only to relations defined by the
substrate-schema version it cites. v1 predicates that touch only
`call_edges`, `functions`, `contracts`, and `lifted_files` are
language-agnostic in principle: any lifter that produces a v1 substrate
exposes those relations regardless of source language.

The lifter is the boundary. A C lifter produces call_edges from C
source; a Java lifter produces call_edges from JavaParser AST; a Rust
lifter produces them from rustc MIR; a TypeScript lifter from the TS
compiler API. The predicate's bytes are identical across languages; the
substrate's bytes differ; the result set is comparable.

This makes the borrowed-pages-as-scratch predicate fire on:

- C: kernel `aead_request_set_crypt(req, sg, sg, ...)` over `skb_to_sgvec`-derived SGL
- Java: BouncyCastle `cipher.doFinal(buf, 0, len, buf, 0)` over `ByteBuffer.wrap`-derived buffer
- Go: `cipher.Stream.XORKeyStream(dst, src)` where `dst` and `src` alias

A producer that wants federation MUST publish per-language callee tables
that map the predicate's universal callee references to the per-language
function name set. The mapping is itself a content-addressed memento
signed by the producer; verifiers admit it according to their trust
policy on that producer.

The empirical federation guarantee for predicates that join over the
`composed_contracts` relation is the BZ-COMPOSITION-001 specimen at
`menagerie/bug-zoo/species/BZ-COMPOSITION-001-cross-language-equivalence/`,
defined by CCP §7. The specimen lifts a structurally-equivalent chain in
both Rust and C, runs the canonical compose primitive on each, and
asserts the resulting ComposedFunctionContract CIDs are byte-identical.
A passing run is the empirical receipt that PPP predicates joining over
composed CIDs federate correctly across those two lifters.

## Section 8. Worked example: borrowed-pages-as-scratch

The pattern is editorial:

```
patternSlug:        borrowed-pages-as-scratch
patternCid:         blake3-512:<canonical bytes of nefariousplan pattern record>
shortDescription:   "A subsystem performs scratch writes into a destination
                    buffer under an internal contract that it owns the
                    memory. Another subsystem supplies that buffer with
                    foreign-owned pages. The contract is documentation;
                    the legitimate scratch becomes a write primitive
                    across a trust boundary nobody guards."
publicInstances:    CVE-2026-31431 (Copy Fail), CVE-2026-43284 (Dirty Frag, ESP),
                    CVE-2026-43500 (Dirty Frag, RxRPC)
```

The v2 predicate compiles the pattern against schema v1 by binding to
`call_edges`. Canonical bytes of the SQL expression hash to a stable
predicateCid. The predicate's recursive CTE walks each candidate's direct
caller's transitive callees up to a bounded depth, looking for any of a
declared set of mitigation callees (skb_cow_data, skb_unshare,
skb_make_writable, alloc_skb, ...).

Applied to the substrate from Linux 7.1.0-rc2 (pre V4bel patch):

```
preApplication.predicateCid   blake3-512:ece7d50c22ca6678ce372d9a020726fe14d8c...
preApplication.substrateCid   blake3-512:<lift of net/ subtree>
preApplication.lifterCid      blake3-512:<sugar-lift-c-kernel-doc bytes>
preApplication.resultSet      [
  {"locus": {"path": "net/rxrpc/rxkad.c", "line": 429, "function": "rxkad_verify_packet_1"}},
  {"locus": {"path": "net/rxrpc/rxkad.c", "line": 494, "function": "rxkad_verify_packet_2"}}
]
preApplication.rowCount       2
```

V4bel's submitted patch widens `if (skb_cloned(skb))` to
`if (skb_cloned(skb) || skb->data_len)` in `rxrpc_input_call_event` and
`rxrpc_verify_response`. Re-lifting yields `postApplication.substrateCid`
distinct from `preApplication.substrateCid`. Re-running the predicate
against the post substrate yields a new result set.

A v2 predicate that binds only to `call_edges` will report `unchanged`
because the patch does not add or remove any call edge. A v3 predicate
that binds to a future schema version exposing gate conditions WOULD
report `empty`. The predicate the receipt cites determines whether the
receipt closes.

This is the load-bearing observation: **the predicate's substrate
binding determines what patch shapes can witness closure under it.** A
producer that wants their fix to discharge a receipt MUST cite a
predicate whose substrate binding sees the bytes the patch changes.
Producers may publish multiple predicates at different bindings;
verifiers admit whichever satisfies their policy.

## Section 9. Failure modes

### 9.1 Lift drift

A lifter that changes its output schema breaks every predicate that
binds to the old schema. PPP requires that every query application
include the lifter CID; a verifier MUST refuse to compose two query
applications under one closure witness when the two `lifterCid` values
differ unless a substrate compatibility memento between the two lifters
is presented and admitted by policy.

### 9.2 Predicate drift

A predicate's bytes are immutable; a new predicate has a new CID. A
producer that "improves" a predicate publishes a new memento; old
receipts citing the old predicate retain their validity over the bytes
they witnessed. The pattern catalog SHOULD record predicate succession,
but verifiers MUST NOT silently substitute a new predicate for the one
named in a receipt.

### 9.3 Substrate drift

The world changes. A predicate that fired on Linux 7.1.0-rc2 may not
fire on a future kernel. PPP receipts are valid over the bytes-state
they cite. A verifier that asks "does this predicate fire on current
HEAD" is asking a different question than "did this receipt close on
that-bytes-state."

### 9.4 Predicate soundness

A predicate may produce false positives or false negatives. PPP does
not assert predicate soundness; it asserts predicate identity and
result determinism. Soundness is a separate claim, attached to the
predicate as additional mementos: a soundness memento may cite
test-corpus runs, formal verification, or adversarial review.

A receipt that cites an unsound predicate is a receipt over an unsound
policy. The verifier's policy decides whether to admit it.

### 9.5 Determinism violations

A predicate that produces non-bit-identical result-set bytes across
runs against the same substrate is malformed. A verifier MUST refuse
to admit a query application whose `resultSetCid` does not match the
verifier's recomputation. A producer that observes determinism
violations MUST publish a corrected predicate with a new CID and SHOULD
mark the old predicate as withdrawn.

### 9.6 Composition not materialized

A predicate that binds to `composed_contracts` only fires when CCP-
canonical composition has been materialized for the chains the predicate
queries. Eager materialization (lifter-time per CCP §4) populates the
relation at lift; lazy materialization (verifier-time per CCP §4)
populates it during prove. A predicate that runs against a substrate
where neither has occurred returns an empty result set, which under
Section 5's closure-shape table is `unchanged` (vacuous).

A producer that wants a composition-bound predicate to fire reliably
MUST ensure the substrate's lifter chain emits ComposedFunctionContract
mementos eagerly. A consumer that wants the same MAY trigger lazy
materialization by running `sugar prove` over the substrate before
running the predicate.

### 9.7 Federation mismatch

Two lifters claiming to produce the same v1 substrate from the same
source MUST agree bit-for-bit on the result of any v1 predicate. The
bug-zoo cross-language equivalence species exists to surface
disagreements. A verifier that holds two such lifters and observes
disagreement MUST fail closed pending a resolution memento.

## Section 10. Operational notes

### 10.1 Predicate distribution

A predicate is distributed as a `.proof`-bundled memento containing:

- The predicate bytes (canonical query expression).
- The producer signature.
- The patternCid (when one exists).
- The schema version binding.
- An optional soundness memento set.

Predicates SHOULD be discoverable through the same catalog mechanism as
patterns, with a stable mapping from `patternCid` to known compiling
predicates.

### 10.2 Revocation

A producer that withdraws a predicate publishes a withdrawal memento
signed by the same Ed25519 key that signed the original. Verifiers
holding a receipt that cites a withdrawn predicate decide locally
whether to honor it; PPP does not mandate revocation propagation.

### 10.3 Catalog signing

A pattern catalog is itself a memento: a Merkle tree over patternCids
with a producer signature on the root. Verifiers admit catalog roots by
producer trust. The catalog root MAY be published on a blockchain when
public ordering of catalog evolution matters; the protocol does not
require it.

### 10.4 Receipt provenance

A PPP receipt MUST identify its producer. A producer that signs a
receipt asserts that they ran the query application, observed the
result set, and signed in good faith. The signature is over the
canonical bytes of the receipt object; substituting any byte (including
re-ordering JSON keys) breaks the signature.

## Section 11. The pipeline, in one diagram

```text
Editorial pattern (nefariousplan)
        │  (compilation memento, signed)
        ▼
Predicate (SQL bytes, CID, schema-bound)
        │  (predicate distribution, discoverable in catalog)
        ▼
Substrate (lifter output over source bytes, CID)
        │  (query application memento, includes lifterCid)
        ▼
Result set (CID, deterministic bytes)
        │  (closure witness over pre/post pair)
        ▼
Closure witness (shape ∈ {empty, strict-subset, ...})
        │  (cited as closureWitnessCid in FRP receipt)
        ▼
Fix receipt (FRP, signed)
        │  (attached to commit .proof root, gateable at merge)
        ▼
Proofchain head (carries closure under verifier policy, federable)
```

Every box is content-addressed. Every arrow is signed. Every step is
locally re-runnable given the predecessor bytes. The pipeline is
verifiable end-to-end without trusting any node beyond the producer
whose signature anchors each artifact.

## Appendix A. Canonical SQL admitted in v1

The v1 query language is SQLite-3 with the JSON1 module enabled. The
admitted built-in function set is the SQLite core plus JSON1's
`json_extract`, `json_each`, `json_array`, `json_object`,
`json_array_length`, `json_type`, `json_valid`. Functions outside this
set break determinism guarantees and disqualify a query from being a
v1 predicate.

The canonical encoding of a SQL predicate's bytes is the UTF-8 source
text after one normalization pass: leading and trailing whitespace
trimmed; trailing whitespace stripped from each line; line endings
normalized to LF. Comments are preserved (they affect the human-
verifiable identity of the predicate; two predicates with different
comments have different CIDs).

## Appendix B. Reference implementation

A v1 reference implementation lives in
`menagerie/pattern-predicate-protocol/` (planned). It exposes:

- `sugar pp compile <pattern.toml>`: produce a predicate memento.
- `sugar pp run <predicate.sql> <substrate.db>`: produce a query application memento.
- `sugar pp witness <pre-application> <post-application>`: produce a closure witness.
- `sugar pp receipt <closure-witness> <patch.cid>`: produce an FRP receipt citing the witness.

All four commands take and produce `.proof`-bundled mementos. Each is
re-runnable from the predecessor bytes. None requires network access
beyond optional catalog discovery.
