# Grammar Conformance Protocol (GCP)

**Status:** v0.1.0 draft extension protocol
**Date:** 2026-05-06
**Layer:** extension protocol over TDP and the ProofIR/memento substrate
**Related:**
- `2026-05-06-extension-protocols.md` - extension protocols, DAG-of-DAGs, reflective self-witnessing
- `2026-05-06-truth-discharge-protocol.md` - unit truth over signed body-claims
- `2026-05-06-checker-bytecode-protocol.md` - checker bodies and bytecode artifacts
- `2026-05-06-obligation-realizer-protocol.md` - ORP plans/results as grammar-constrained bodies
- `2026-04-30-ir-formal-grammar.md` - existing ProofIR external JSON grammar and formal invariant style
- `2026-05-03-substrate-layers-envelope-header-body.md` - signed header/body letter shape

## Section 0. Purpose

GCP standardizes how a signed body is witnessed as conforming to a formal grammar and an optional set of ProofIR invariants.

The core shape:

```
grammarCid + subjectBodyCid + invariantSetCid + parserCid + policyCid
  -> grammarConformanceWitnessCid
```

GCP exists because extension protocols are body conventions. A body convention can be described by a formal grammar. The grammar can be content-addressed. The parsed shape can be constrained by ProofIR invariants. The conformance claim can be discharged by TDP.

This means extension protocols can define their own admissible body languages without becoming core protocol changes.

## Section 1. The grammar stack

GCP separates four layers:

1. **Bytes.** The signed body bytes carried by a memento.
2. **Grammar.** A content-addressed syntax specification that accepts or rejects those bytes.
3. **Invariants.** ProofIR predicates over the parsed body, its references, and its declared relationships.
4. **Witness.** A signed TDP-compatible truth discharge over the conformance body-claim.

The stack:

```
subject body bytes
  -> parse under grammarCid with parserCid
  -> check invariantSetCid
  -> emit grammarConformanceWitnessCid
```

Core verification stops at signed bytes, CIDs, signatures, references, and core memento/header validity. GCP-aware tooling performs parsing and invariant checking under policy.

## Section 2. Non-execution rule

GCP is an extension protocol.

Core verification MUST NOT run parsers, grammar interpreters, invariant checkers, solvers, or GCP tooling. Core verification verifies the signed byte graph that contains grammar specs, parser identities, invariant sets, policies, subject bodies, and witness results.

If a grammar check fails, refuses, times out, or does not terminate, no positive grammar conformance witness exists.

## Section 3. Vocabulary

**Subject body.** The canonical signed/content-addressed body bytes being checked.

**Grammar artifact.** A formal grammar, schema, parser specification, CDDL, EBNF, PEG, JSON Schema, ProvekIt-native grammar, or other accepted syntax artifact identified by CID.

**Parser.** An accepted parser/interpreter/compiler for a grammar artifact, identified by CID.

**Parse witness.** Evidence that `subjectBodyCid` was accepted by `grammarCid` using `parserCid` under `policyCid`.

**Invariant set.** A content-addressed set of ProofIR predicates or edges over the parsed body and its references.

**Invariant checker.** An accepted checker, solver, interpreter, or proof system used to discharge the invariant set.

**GrammarConformanceBodyClaim.** The TDP body-claim describing exactly which subject, grammar, parser, invariant set, checker, and policy are being asserted.

**GrammarConformanceWitness.** A TDP-compatible positive witness over a grammar conformance body-claim.

**GrammarConformanceRefusal.** A signed refusal showing that conformance was not witnessed.

## Section 4. Body-claim shape

Draft GCP body-claim convention:

```json
{
  "kind": "GrammarConformanceBodyClaim",
  "schemaVersion": "1",
  "subjectBodyCid": "blake3-512:...",
  "subjectKind": "CheckerMemento",
  "grammarCid": "blake3-512:...",
  "grammarKind": "cddl",
  "parserCid": "blake3-512:...",
  "invariantSetCid": "blake3-512:...",
  "invariantCheckerCid": "blake3-512:...",
  "policyCid": "blake3-512:...",
  "inputCids": ["blake3-512:..."]
}
```

Normative fields:

| Field | Meaning |
|---|---|
| `kind` | MUST be `"GrammarConformanceBodyClaim"`. |
| `schemaVersion` | MUST be `"1"` for this draft. |
| `subjectBodyCid` | CID of the body bytes being checked. |
| `subjectKind` | Declared extension body kind, e.g. `"CheckerMemento"`, `"TruthDischargeWitness"`, `"RealizerPlan"`. |
| `grammarCid` | CID of the formal grammar or schema artifact. |
| `grammarKind` | Grammar encoding, e.g. `"cddl"`, `"ebnf"`, `"peg"`, `"json-schema"`, `"provekit-grammar-ir/0"`. |
| `parserCid` | CID of the accepted parser/interpreter for `grammarCid`. |
| `invariantSetCid` | CID of ProofIR invariants or `null` if the grammar alone is being witnessed. |
| `invariantCheckerCid` | CID of checker/solver for `invariantSetCid` or `null` if not used. |
| `policyCid` | CID of acceptance policy. |
| `inputCids` | Prior artifacts this body-claim depends on. |

The body-claim is itself content-addressed. A GCP witness discharges this exact body-claim and no other.

## Section 5. Witness shape

GCP positive witnesses SHOULD be TDP witnesses with:

```json
{
  "kind": "TruthDischargeWitness",
  "schemaVersion": "1",
  "claimBodyCid": "blake3-512:...",
  "claimKind": "grammar-conformance",
  "result": true,
  "verifierCid": "blake3-512:...",
  "policyCid": "blake3-512:...",
  "evidenceRootCid": "blake3-512:...",
  "inputCids": ["blake3-512:..."]
}
```

For GCP, `claimBodyCid` is the CID of a `GrammarConformanceBodyClaim`.

The `verifierCid` SHOULD identify the composite grammar conformance verifier: parser, invariant checker, and any orchestration logic accepted by policy. The component parser and invariant checker CIDs remain named in the body-claim so they are part of the discharged proposition.

## Section 6. Formal grammars as extension bodies

An extension protocol MAY define its valid bodies by publishing a grammar artifact:

```
extensionSpecCid
grammarCid
parserCid
policyCid
```

The grammar says which byte strings are syntactically admissible bodies for that extension. This makes an extension protocol a family of signed, content-addressed body languages rather than an informal prose convention.

GCP does not require one grammar metalanguage. CDDL, EBNF, PEG, JSON Schema, generated parser source, WASM parsers, and ProvekIt-native grammar IR can all be grammar artifacts if policy accepts them and they are identified by CID.

The common rule:

```
grammar bytes -> grammar CID
parser bytes -> parser CID
subject body bytes -> subject body CID
accepted parse -> grammar conformance witness
```

## Section 7. Formal grammars as ProofIR invariants

Grammar acceptance is necessary but not always sufficient.

A grammar can say:

```
field "claimBodyCid" is present and has CID syntax
```

ProofIR invariants can say:

```
claimBodyCid is referenced by inputCids
policyCid accepts verifierCid
result == true implies claimKind is positive-dischargeable
no unsigned body field has semantic authority
all referenced CIDs are reachable in the evidence DAG
```

GCP therefore treats invariants as a second conformance layer:

```
parse tree + referenced DAG + invariantSetCid -> invariant witness/refusal
```

An invariant set SHOULD be expressed as ProofIR predicates or implication edges over a declared projection of the parsed body. If an invariant requires host execution, solver work, or extension interpretation, that work is extension execution and MUST NOT enter core verification.

## Section 8. Self-conformance

GCP can witness extension protocols, including itself.

Example:

```
gcpSpecCid
gcpGrammarCid
gcpInvariantSetCid
gcpParserCid
gcpInvariantCheckerCid
policyCid
  -> gcpBodyClaimCid
  -> gcpTruthWitnessCid
```

Meaning:

```
this GCP body/spec conforms to this GCP grammar and invariant set
under this parser, checker, and policy
```

This is not circular proof. Core verification still relies only on the base kernel:

- canonical byte parsing;
- CID computation;
- signature verification;
- reference resolution;
- policy choice;
- accepted parser/checker CIDs.

Above that base kernel, ProvekIt can carry reflective evidence about its own extension protocols. The self-witness is a signed artifact, not a new core axiom.

### Section 8.1 Formal self-hosting model

Let:

- `K` be the finite core verifier: canonical byte parsing, CID computation, signature verification, reference resolution, and core memento/header validity.
- `E` be an extension protocol spec artifact.
- `B_E` be a signed body governed by `E`.
- `G_E` be a grammar artifact for `E` bodies.
- `P_E` be a parser/interpreter artifact for `G_E`.
- `I_E` be a ProofIR invariant set over parsed `E` bodies.
- `C_E` be an invariant checker or solver artifact for `I_E`.
- `Pol` be an acceptance policy artifact.
- `TDP` be the truth-discharge protocol.

Define core validity:

```
KValid(x) =
  cid(x) is correct
  and signature(x) is valid
  and referenced core objects are resolvable
  and x satisfies core header/memento rules
```

Define grammar conformance under policy:

```
GConforms(B_E, G_E, P_E, I_E, C_E, Pol) =
  Pol accepts G_E, P_E, I_E, C_E
  and P_E parses B_E under G_E
  and C_E discharges I_E over parsed(B_E)
```

GCP represents the conformance claim as a body:

```
claimBody =
  {
    subjectBodyCid: cid(B_E),
    grammarCid: cid(G_E),
    parserCid: cid(P_E),
    invariantSetCid: cid(I_E),
    invariantCheckerCid: cid(C_E),
    policyCid: cid(Pol)
  }
```

TDP represents the positive result:

```
TDPWitness(claimBody, verifierCid, policyCid) = true
```

The witnessed root is:

```
truthWitnessCid = cid(TDPWitness)
```

### Section 8.2 Self-hosting theorem

**Theorem (Stratified Self-Conformance).** An extension protocol `E` may publish a grammar for its own bodies, express semantic constraints as ProofIR invariants, and carry a TDP witness that a body governed by `E` conforms to that grammar and invariant set, without making core verification circular, if core verification is limited to `KValid` and does not depend on `GConforms`.

**Proof sketch.** Core verification of every artifact in the graph is `KValid`: check bytes, CIDs, signatures, references, and core memento/header rules. `KValid` does not parse `B_E` under `G_E`, execute `P_E`, solve `I_E`, run `C_E`, or evaluate TDP semantics. Those are extension computations under `Pol`. If the extension computation terminates and emits a signed TDP witness, the witness is another artifact that core can validate by `KValid`. If the extension computation refuses or does not terminate, no positive witness exists. In neither case does core validity depend on the extension's self-conformance result. Therefore `E` can carry evidence about its own conformance without using that evidence as a core axiom. QED.

### Section 8.3 The protocol-in-protocol stack

The recursive stack is:

```
protocol spec E
  -> grammar G_E
    -> GCP body-claim
      -> TDP truth witness
        -> GCP-conformant TDP body
          -> signed/CID-bearing letter
```

This recursion is bounded by the base kernel `K`. ProvekIt can place a protocol spec inside a protocol spec, and witness conformance while conforming, because each layer reduces to signed bytes and explicit CID edges before any extension semantics are evaluated.

The practical rule:

```
Trust the witnessed root for the layer you rely on.
Never require the core to trust the recursion.
```

## Section 9. Relationship to TDP

GCP is TDP-shaped.

GCP defines a body-claim family:

```
claimKind = "grammar-conformance"
```

TDP supplies the positive result:

```
result = true
```

So a GCP witness means:

```
true(grammar-conformance body-claim, verifier, policy)
```

Parent claims that rely on grammar conformance SHOULD reference the TDP witness root, not only the grammar CID, parser CID, invariant set CID, or subject body CID.

## Section 10. Relationship to CBP

CBP body shapes, checker bytecode descriptors, binding ABIs, and proof-carrying checker claims SHOULD be grammar-conformance witnessable under GCP.

Example:

```
checkerMementoBodyCid
checkerMementoGrammarCid
checkerMementoInvariantSetCid
gcpVerifierCid
policyCid
  -> checkerMementoGrammarConformanceWitnessCid
```

A CBP checker execution may require this witness before accepting a checker memento as well-formed under policy.

## Section 11. Relationship to ORP

ORP plans and outputs SHOULD be grammar-conformance witnessable under GCP.

For transforms, a policy MAY require:

```
RealizerPlan body conforms to ORP grammar
TransformResult body conforms to ORP grammar
post-transform closure body receives TDP witness
```

GCP does not make a dropper trustworthy by origin. It only witnesses that ORP-shaped bodies are syntactically and structurally admissible. The transform still requires re-lift and closure evidence.

## Section 12. Refusals

Draft refusal shape:

```json
{
  "kind": "GrammarConformanceRefusal",
  "schemaVersion": "1",
  "claimBodyCid": "blake3-512:...",
  "reasonCode": "GRAMMAR_REJECTED",
  "message": "subject body did not match the declared grammar",
  "inputCids": ["blake3-512:..."]
}
```

Common refusal reasons:

- `GRAMMAR_REJECTED`
- `PARSER_UNSUPPORTED`
- `PARSER_REFUSED`
- `INVARIANT_VIOLATED`
- `INVARIANT_CHECKER_UNSUPPORTED`
- `POLICY_REJECTED`
- `TIMEOUT`
- `FUEL_EXHAUSTED`
- `MISSING_REFERENCE`

A refusal is not a positive conformance witness. It is a signed diagnostic/evidence record.

## Section 13. Non-goals

- Define one universal grammar metalanguage.
- Require core verification to parse extension bodies.
- Treat grammar conformance as semantic truth beyond the body-claim.
- Make self-witnessing circular or axiomatic.
- Replace TDP, CBP, ORP, SMT solvers, proof assistants, or host lifters.
- Decide which parser implementations a policy must trust.

## Section 14. Open questions

1. Should ProvekIt define `provekit-grammar-ir/0` as a portable grammar IR?
2. Should GCP require `invariantSetCid`, or allow grammar-only conformance as first-class?
3. Should `parserCid` identify parser source, compiled parser bytecode, or a parser conformance witness root?
4. Should GCP define a standard projection from parsed body JSON/CBOR into ProofIR terms?
5. Should protocol catalog entries require a `grammarConformanceWitnessCid` for stable extension protocols?

## Section 15. Conformance

A GCP v0.1 implementation is conformant if it:

1. Emits canonical signed/content-addressed `GrammarConformanceBodyClaim` bodies.
2. Uses TDP-compatible positive witnesses with `claimKind = "grammar-conformance"`.
3. Names subject body, grammar, parser, invariant set, invariant checker, policy, and input CIDs in the body-claim.
4. Fails closed on grammar rejection, parser refusal, invariant violation, unsupported checker, timeout, fuel exhaustion, or policy rejection.
5. Never requires core verification to run parsers, invariant checkers, or GCP tooling.
6. Allows parent claims to rely on the grammar conformance witness root.
7. Supports self-conformance evidence without treating it as a core axiom.

## Section 16. Citation

Cite as:

> ProvekIt Protocol Working Notes (2026). *Grammar Conformance Protocol (GCP)*. Draft extension protocol v0.1.0.
