# ProvekIt IR — Formal Grammar (External JSON Encoding)

**Date:** 2026-04-30
**Status:** Specification + reference parser, paired with `src/ir/grammar/parse.ts`
**Scope:** The textual JSON encoding emitted by every ProvekIt IR symbolic-primitives kit (TypeScript, Rust, Go, C++) when serializing a `Declaration[]`.

## What this document specifies

This grammar is the formal description of the **kit-emitted IR JSON** — the
textual form every kit produces from its `_resetCollector() / beginCollecting() /
property(...) / finish()` cycle. Today, four kits emit byte-identical JSON for
the same logical claim; this is enforced by the regression harness in
`scripts/cross-lang-equivalence/`. Until now there has been no formal grammar:
the contract was "whatever the kits all happen to emit."

This document promotes that implicit contract to an explicit one.

### What this is *not*

This grammar is **not** the canonical form used to compute `propertyHash`. The
canonicalizer (`src/canonicalizer/`) consumes IR values, runs them through eight
passes (de Bruijn, sort/predicate canonicalization, implies removal, NNF, AC
normalization), and then serializes the result with JCS (RFC 8785) before
hashing. The JCS form sorts object keys lexicographically; the kit-emitted form
locked here uses **insertion-order** keys (a fixed order per node kind). They
are different encodings at different layers.

```
        kit symbolic primitives (per language)
                       │
                       ▼
   ─────────  this grammar  ─────────
   kit-emitted IR JSON (Declaration[])    ← byte-equal across kits
   ──────────────────────────────────
                       │
                       ▼
            canonicalizer (passes 1..6)
                       │
                       ▼
              CanonicalFolAst
                       │
                       ▼
          JCS serialize (pass 7, RFC 8785)
                       │
                       ▼
            sha256-prefix-16 (pass 8)  =  propertyHash
```

The grammar below describes only the top arrow. The canonicalizer pipeline is
described elsewhere and is not affected by anything in this document.

## Notation

The grammar is written in EBNF with the following conventions:

- Terminals are quoted JSON literals (e.g. `"\"kind\""`).
- A literal `,` between productions denotes the JSON comma separator with
  exactly one byte (`U+002C`) and **no** surrounding whitespace.
- A literal `:` between key and value denotes the JSON name-separator with
  exactly one byte (`U+003A`) and **no** surrounding whitespace.
- `[` `]` denote JSON-array brackets; `{` `}` denote JSON-object brackets.
- `String`, `Number`, `Bool`, `Null` denote the corresponding JSON tokens (per
  RFC 8259).
- `*` means zero-or-more; `?` means optional; `|` means alternation.
- Whitespace between tokens is **not permitted** in conformant output. The
  emitted form is the compact, no-whitespace form `JSON.stringify(value)`
  produces with no `space` argument.

The grammar is *deterministic* (one parse tree per valid input) and
*reverse-deterministic* (one canonical text per valid IR value), provided the
emitter respects the locked key order specified for each node kind.

## Formal Invariants Index

This section provides a cross-reference index of all formal invariants defined
throughout this specification. Each invariant is expressed in first-order logic
with accompanying English explanation.

### Invariants by Section

**ContractDeclaration** (Section: Declarations)
| Invariant | Formula |
|-----------|---------|
| HasOutBinding | `∀c → HasKey(c, "outBinding") ∧ IsString ∧ c ≠ ""` |
| HasAtLeastOneFormula | `∀c → (IsIrFormula(pre) ∨ IsIrFormula(post) ∨ IsIrFormula(inv))` |
| ValidFreeVariables | `∀c,f → f = outBinding ∨ IsParam(f)` |

**BridgeDeclaration** (Section: Declarations)
| Invariant | Formula |
|-----------|---------|
| RequiredFields | `∀b → HasKey(name) ∧ HasKey(sourceSymbol) ∧ ...` |
| ValidCidFormat | `∀b → IsValidCidFormat(targetContractCid)` |

**QuantifierFormula** (Section: Formulas)
| Invariant | Formula |
|-----------|---------|
| HasSort | `∀q → HasKey("sort") ∧ IsSort(q.sort)` |
| HasBody | `∀q → HasKey("body") ∧ IsIrFormula(q.body)` |

**ConnectiveFormula** (Section: Formulas)
| Invariant | Formula |
|-----------|---------|
| NotArity | `∀c (c.kind="not") → Len(operands)=1` |
| ImpliesArity | `∀c (c.kind="implies") → Len(operands)=2` |
| AndOrArity | `∀c (c.kind="and"∨c.kind="or") → Len(operands)≥2` |

**AtomicFormula** (Section: Formulas)
| Invariant | Formula |
|-----------|---------|
| HasName | `∀a → HasKey("name") ∧ IsString(a.name)` |
| HasArgs | `∀a → HasKey("args") ∧ IsArray(a.args)` |
| KnownPredicate | `∀a → IsBuiltIn(a.name) ∨ IsKitDefined(a.name)` |

**VarTerm** (Section: Terms)
| Invariant | Formula |
|-----------|---------|
| NoSortField | `∀t → ¬HasKey(t, "sort")` |
| SortFromQuantifier | `∀v,q,env (InScope(v,q) ∧ v.name=q.name) → Sort(v)=q.sort` |
| SortFromSubstitution | `∀v,s,env (v∈FreeVars(s) ∧ SubstitutedBy(s,v)=e) → Sort(v)=Sort(e)` |

**ConstTerm** (Section: Terms)
| Invariant | Formula |
|-----------|---------|
| HasSort | `∀t → HasKey("sort") ∧ IsSort(t.sort)` |

**CtorTerm** (Section: Terms)
| Invariant | Formula |
|-----------|---------|
| NoSortField | `∀t → ¬HasKey(t, "sort")` |

**LambdaTerm** (Section: Terms)
| Invariant | Formula |
|-----------|---------|
| HasParamSort | `∀t → HasKey("paramSort") ∧ IsSort(t.paramSort)` |
| HasBody | `∀t → HasKey("body") ∧ IsIrTerm(t.body)` |
| NoSortField | `∀t → ¬HasKey(t, "sort")` |
| ParamSortFromEnclosing | `∀λ,env → Sort(λ.paramName, env) = λ.paramSort` |

**LetTerm** (Section: Terms)
| Invariant | Formula |
|-----------|---------|
| NonEmptyBindings | `∀t → HasKey("bindings") ∧ Len(bindings) ≥ 1` |
| HasBody | `∀t → HasKey("body") ∧ IsIrTerm(t.body)` |
| BindingSortPropagation | `∀l,env,i → Sort(l.bindings[i].name, env) = Sort(l.bindings[i].boundTerm, env)` |

**ChoiceFormula** (Section: Formulas)
| Invariant | Formula |
|-----------|---------|
| HasVarName | `∀c → HasKey("varName") ∧ IsString(c.varName)` |
| HasSort | `∀c → HasKey("sort") ∧ IsSort(c.sort)` |
| HasBody | `∀c → HasKey("body") ∧ IsIrFormula(c.body)` |
| Uniqueness | `∀c → ∃! x: c.sort. c.body[x/c.varName]` |

**EvidenceTerm** (Section: Evidence)
| Invariant | Formula |
|-----------|---------|
| HasProofType | `∀e → HasKey("proofType") ∧ IsProofType(e.proofType)` |
| HasCertificate | `∀e → HasKey("certificate") ∧ IsObject(e.certificate)` |
| FormulaHashMatches | `∀e,f (e.attachedTo=f) → e.certificate.formulaHash = Hash(f)` |

**PrimitiveSort** (Section: Sorts)
| Invariant | Formula |
|-----------|---------|
| ValidName | `∀s → HasKey("kind")∧s.kind="primitive"∧HasKey("name")∧IsString` |

**BitvecSort** (Section: Sorts)
| Invariant | Formula |
|-----------|---------|
| ValidWidth | `∀s → HasKey("kind")∧s.kind="bitvec"∧HasKey("width")∧width>0` |

**SetSort** (Section: Sorts)
| Invariant | Formula |
|-----------|---------|
| ValidElement | `∀s → HasKey("element") ∧ IsSort(s.element)` |

**TupleSort** (Section: Sorts)
| Invariant | Formula |
|-----------|---------|
| ValidElements | `∀s → HasKey("elements") ∧ ∀e∈elements → IsSort(e)` |

**FunctionSort** (Section: Sorts)

| Invariant | Formula |
|-----------|---------|
| ValidArgsAndReturn | `∀s → HasKey("args")∧IsArray(args)∧len(args)≥1∧∀a∈args→IsSort(a) ∧ HasKey("return")∧IsSort(return)` |

**DependentSort** (Section: Sorts)

| Invariant | Formula |
|-----------|---------|
| ValidFields | `∀s → HasKey("name")∧IsString(name) ∧ HasKey("indexVar")∧IsString(indexVar) ∧ HasKey("indexSort")∧IsSort(indexSort)` |

**FloatSort** (Section: Sorts)

| Invariant | Formula |
|-----------|---------|
| ValidWidth | `∀s → HasKey("width")∧IsPositiveInteger(width) ∧ width ∈ {16,32,64,128}` |

**RegionSort** (Section: Sorts)

| Invariant | Formula |
|-----------|---------|
| ValidName | `∀s → HasKey("name")∧IsString(name)∧len(name)>0` |
| OpaqueToBackends | `∀s, backend∈{SMT-LIB,Coq} → ¬ReachesBackend(s,backend)` |

**Strict Mode** (Section: Reference Parser)
| Invariant | Formula |
|-----------|---------|
| KeyOrder | `∀n InStrict → EmitOrder(n)=ExpectedKeyOrder(n.kind)` |
| PredicateName | `∀a InStrict → IsBuiltIn(a.name) ∨ ValidIdentifier(a.name)` |
| PrimitiveSortName | `∀s InStrict → s.name ∈ CanonicalNames` |

**Round-trip Property** (Section: Reference Parser)
| Invariant | Formula |
|-----------|---------|
| ParserPreservesStructure | `∀B GrammarAccepts(B) → IsValidDocument(Parse(B))` |
| EmitterPreservesOrder | `∀d IsValidDocument(d) → GrammarAccepts(Emit(d))` |
| FixedPoint | `∀B GrammarAccepts(B) → Emit(Parse(B)) = B` |

**Test Plan** (Section: Conformance Test Plan)
| Invariant | Formula |
|-----------|---------|
| CoverageComplete | `∀k NodeKind → PosFixtures(k)≠∅ ∧ NegFixtures(k)≠∅` |
| NegativeTestsReject | `∀n NegTestCase → ThrowsGrammarParseError` |

## Top-level production

```ebnf
Document    ::= "[" ( Declaration ( "," Declaration )* )? "]"

Declaration ::= ContractDeclaration
              | BridgeDeclaration
```

A document is a JSON array of declarations. Empty (`[]`) is valid.

## Declarations

### ContractDeclaration

Locked key order: `kind`, `name`, `outBinding`, `pre`, `post`, `inv`.
The `pre`, `post`, and `inv` fields are each optional but at least
one MUST be present. When present, each is an `IrFormula`. When
absent, the entire key is omitted (never emitted as `null` —
matches the JCS canonicalization rule "omit absent keys"). The
`outBinding` field is REQUIRED and names the free variable that
`post` uses to refer to the function's return value
(conventionally `"out"`).

```ebnf
ContractDeclaration ::= "{"
                          "\"kind\"" ":" "\"contract\"" ","
                          "\"name\"" ":" String ","
                          "\"outBinding\"" ":" String
                          ( "," "\"pre\"" ":" IrFormula )?
                          ( "," "\"post\"" ":" IrFormula )?
                          ( "," "\"inv\"" ":" IrFormula )?
                        "}"
```

The `post` formula's body MAY contain free occurrences of a
variable whose `name` equals `outBinding`. The verifier substitutes
the call expression's symbolic output for that variable at use
sites (per the handshake algorithm spec). All other free variables
in any of `pre`/`post`/`inv` are quantified by an enclosing
`forall` whose `varName` matches the function's parameter name; a
ContractDeclaration whose `pre`/`post`/`inv` contains a free
variable that is neither `outBinding` nor a parameter is malformed.

**INVARIANT ContractDeclaration.HasOutBinding:**
```
∀c: ContractDeclaration → HasKey(c, "outBinding") ∧ IsString(c.outBinding) ∧ c.outBinding ≠ ""
```
Every contract declaration MUST have a non-empty `outBinding` field naming
the variable that represents the function's return value in the postcondition.

**INVARIANT ContractDeclaration.HasAtLeastOneFormula:**
```
∀c: ContractDeclaration → (IsIrFormula(c.pre) ∨ IsIrFormula(c.post) ∨ IsIrFormula(c.inv))
```
At least one of `pre`, `post`, or `inv` must be present. A contract with none
of these formulas is malformed.

**INVARIANT ContractDeclaration.ValidFreeVariables:**
```
∀c: ContractDeclaration, f: FreeVariables(c.pre ∪ c.post ∪ c.inv)
  f = c.outBinding ∨ IsFunctionParameter(f)
```
All free variables in a contract's formulas must either be the `outBinding`
or a function parameter. Any other free variable indicates a malformed contract.

### BridgeDeclaration

Locked key order: `kind`, `name`, `sourceSymbol`, `sourceLayer`,
`sourceContractCid`, `targetContractCid`, `targetProofCid`, `targetLayer`, `notes` (optional, omitted when absent).

```ebnf
BridgeDeclaration ::= "{"
                        "\"kind\"" ":" "\"bridge\"" ","
                        "\"name\"" ":" String ","
                        "\"sourceSymbol\"" ":" String ","
                        "\"sourceLayer\"" ":" String ","
                        "\"sourceContractCid\"" ":" String ","
                        "\"targetContractCid\"" ":" String ","
                        "\"targetProofCid\"" ":" String ","
                        "\"targetLayer\"" ":" String
                        ( "," "\"notes\"" ":" String )?
                      "}"
```

A bridge is a **verifiable claim** that a source contract satisfies a target
contract. The `targetProofCid` tells the framework which `.proof` bundle
contains the target contract, enabling cross-bundle lookup without scanning
all available `.proof` files.

There are **two classes of bridges**:

**Intra-bundle bridges** (lightweight, most common):
- Live inside the same `.proof` bundle as their source contract
- Inherit the bundle's signature (no separate minting)
- Example: `@types/node` bundles 1000 contracts + 1000 bridges; one signature

**Inter-bundle bridges** (heavyweight, rare):
- Cross organizational boundaries (e.g., ECMAScript spec → V8 implementation)
- Are themselves signed mementos with independent verification
- Example: TC39's formal proof that ECMAScript `parseInt` refines to V8's

For the common case, a JavaScript `.proof` manifest ships with `@types/node`:

```json
{
  "kind": "contract",
  "name": "js-parseInt-v24",
  ...
},
{
  "kind": "bridge",
  "name": "js-parseInt-to-ref",
  "sourceSymbol": "parseInt",
  "sourceLayer": "javascript",
  "sourceContractCid": "bafy...js-parseInt-v24",
  "targetContractCid": "bafy...ref-parseInt-v1",
  "targetProofCid": "bafy...ecma262-v14-proof",
  "targetLayer": "reference"
}
```

A shim library then declares:

```json
{
  "kind": "bridge",
  "name": "myParseInt-implements-node24",
  "sourceSymbol": "myParseInt",
  "sourceLayer": "javascript",
  "sourceContractCid": "bafy...myParseInt-v1",
  "targetContractCid": "bafy...js-parseInt-v24",
  "targetProofCid": "bafy...node-v24-proof",
  "targetLayer": "javascript"
}
```

The `targetProofCid` is what makes the lookup explicit. The framework fetches
the target `.proof` by CID, finds the contract inside it, and verifies the
implication.

The `notes` field is **omitted entirely** when undefined; it is never emitted
as `null`. (Rationale: the TS kit destructures `...(spec.notes !== undefined ? { notes } : {})`;
the Rust kit declares `notes: Option<String>` with `serde(skip_serializing_if = "Option::is_none")`.
This rule is what keeps the four kits byte-equal when bridges have no notes.)

**INVARIANT BridgeDeclaration.RequiredFields:**
```
∀b: BridgeDeclaration →
  HasKey(b, "name") ∧ IsString(b.name) ∧
  HasKey(b, "sourceSymbol") ∧ IsString(b.sourceSymbol) ∧
  HasKey(b, "sourceLayer") ∧ IsString(b.sourceLayer) ∧
  HasKey(b, "sourceContractCid") ∧ IsString(b.sourceContractCid) ∧
  HasKey(b, "targetContractCid") ∧ IsString(b.targetContractCid) ∧
  HasKey(b, "targetProofCid") ∧ IsString(b.targetProofCid) ∧
  HasKey(b, "targetLayer") ∧ IsString(b.targetLayer)
```
All required fields must be present and non-empty strings.

**INVARIANT BridgeDeclaration.ValidSourceCid:**
```
∀b: BridgeDeclaration →
  IsValidCidFormat(b.sourceContractCid)
```

**INVARIANT BridgeDeclaration.ValidTargetCid:**
```
∀b: BridgeDeclaration →
  IsValidCidFormat(b.targetContractCid)
```

**INVARIANT BridgeDeclaration.ValidTargetProofCid:**
```
∀b: BridgeDeclaration →
  IsValidCidFormat(b.targetProofCid)
```

**INVARIANT BridgeDeclaration.CrossDomainVerification:**
```
∀b: BridgeDeclaration →
  VerifyContractImplication(b.sourceContractCid, b.targetContractCid)
```
A bridge is valid only if there exists a verified memento proving that the
source contract's postcondition implies the target contract's postcondition.
This is what enables cross-domain claim transfer: a proof about the source
contract transfers to any other contract that bridges to the same target.

---

### Why bridges pin proofs (and why proofs pin binaries)

The bridge carries `targetProofCid` so that the framework can fetch the
**exact** `.proof` bundle that contains the target contract, not just any
bundle with a contract of the same name. This is what makes cross-platform
verification **sound** rather than merely convenient.

Consider the transitive chain:

```
my_parse_int (your Rust function)
  → myParseInt-v1 (your contract, CID A)
    → bridge: A → js-parseInt-v24 @ node-v24-proof (CID P1)
      → bridge: js-parseInt-v24 → ref-parseInt-v1 @ ecma262-v14-proof (CID P2)
        → witnessed proof: ref-parseInt-v1 verified by Coq (CID W)
```

Every hop is a **hash lookup**. There are no string names in the trusted
computation. The framework resolves `A` to `P1` to `P2` to `W` by content
address alone. If `@types/node` publishes a new version, `P1` changes, the
`targetProofCid` in your bridge no longer resolves, and your build **fails**
at compile time until you re-verify against the new proof.

---

### Bridge target pinning: the shim-poisoning vector

The previous subsection explains why bridges pin proofs in the normal case.
This subsection names the **attack** that the pin defeats, so that conformant
implementations cannot omit the check on the grounds that "the bridge already
points at a contract."

#### Statement of the attack

A bridge declaration carries two outbound CIDs into the target side:

1. `targetContractCid`: the antecedent. The contract whose `pre`/`post` the
   source claims to satisfy.
2. `targetProofCid`: the consequent. The specific `.proof` bundle whose
   discharge mementos witness that satisfaction.

Without the second pin, a bridge commits **only to the antecedent shape**.
Any `.proof` bundle that happens to contain a contract memento with the
matching `targetContractCid` is treated as a valid witness, regardless of
which binary that bundle was minted for. The verifier accepts the
substitution because, syntactically, the obligation is discharged: the
named contract is present, its discharge memento is present, the bundle
signature is valid for some signer.

The semantic guarantee is broken. The verifier has no way to refuse a
discharge that came from a **different binary's** proof bundle, even though
the source contract's claim was made against a specific consequent.

This is **shim poisoning**: an attacker mints a `.proof` bundle whose
contract memento matches the bridge's `targetContractCid` by content but
whose witnessed proof memento was discharged against a poisoned shim
binary. The bridge's antecedent matches by hash. The consequent does not,
but the bridge had no way to say which consequent it meant.

The forward pin closes this hole at the protocol layer, before any
discharge logic runs.

#### Worked example

A Rust function `my_parse_int` claims to satisfy `ref-parseInt-v1` by way
of a bridge through `js-parseInt-v24`. The honest chain pins both ends:

```json
{
  "kind": "bridge",
  "name": "myParseInt-implements-node24",
  "sourceContractCid": "blake3-512:source...",
  "targetContractCid": "blake3-512:js-parseInt-v24...",
  "targetProofCid":   "blake3-512:node-v24-proof-honest...",
  "sourceLayer": "rust",
  "targetLayer": "javascript"
}
```

**Scenario A (shim poisoning, no forward pin).** Suppose `targetProofCid`
were absent or unenforced. The attacker publishes a separate `.proof`
bundle, `node-v24-proof-poisoned`, whose contract member is byte-equal to
`js-parseInt-v24` (same `targetContractCid`) but whose discharge memento
was minted against a poisoned shim binary that returns attacker-controlled
output for chosen inputs. The verifier loads either bundle by name. It
finds a contract memento with the expected CID. It finds a discharge that
references that contract. The bridge's antecedent obligation is met. The
verifier ships **green**, with the poisoned discharge silently
substituted.

**Scenario B (forward pin enforced).** With `targetProofCid` enforced, the
verifier first resolves the bridge's `targetProofCid` to a specific bundle
CID and refuses to consume any other bundle as the consequent, regardless
of name overlap. The poisoned bundle's CID does not match
`node-v24-proof-honest`. Substitution is rejected at protocol layer,
before any contract-membership or discharge-validity check runs.

The two scenarios differ by **one CID equality check**. That check is the
entire mitigation.

#### Mitigation: status of the forward pin

`targetProofCid` is REQUIRED in the BridgeDeclaration grammar (see EBNF
above) and is enforced by `INVARIANT BridgeDeclaration.RequiredFields`
and `INVARIANT BridgeDeclaration.ValidTargetProofCid`. The field MUST be
present and MUST be a syntactically valid CID; producers MUST NOT emit
bridges without it.

The grammar-level requirement is necessary but not sufficient. Conformant
verifiers MUST also use the field at resolve time:

**INVARIANT BridgeDeclaration.ConsequentBundlePinned (NORMATIVE):**
```
∀b: BridgeDeclaration, P: ProofBundle →
  AcceptedAsConsequentFor(P, b) ⇒ Cid(P) = b.targetProofCid
```
A verifier MUST NOT accept any `.proof` bundle as the consequent for a
bridge unless that bundle's CID is bit-equal to the bridge's
`targetProofCid`. Bundles whose contract members happen to share the
bridge's `targetContractCid` MUST NOT be substituted for the pinned
bundle.

#### Two-pin closure with `binaryCid`

The forward pin (bridge → bundle) closes only half the loop. The bundle
itself can still be a wrapper around an unrelated binary unless the bundle
back-pins the binary it attests to. That back pin is `binaryCid`, defined
in `2026-05-02-binary-attestation-protocol.md` §2 and §5. Together:

| Pin | Direction | Field | Specified in |
|---|---|---|---|
| Forward | bridge → bundle | `targetProofCid` | this spec, §BridgeDeclaration |
| Back | bundle → binary | `binaryCid` | binary-attestation §2 |

A verifier that enforces both pins refuses (a) substitution of a poisoned
bundle for the bridge's intended consequent, and (b) substitution of a
poisoned binary under an honestly-pinned bundle. Either pin alone leaves
the other half of the substitution surface open. See
`2026-05-02-binary-attestation-protocol.md` §5 for the back-pin half of
this argument.

---

### The supply chain security model

A `.proof` bundle is a **signed bill of materials**. The developer signs it
with Ed25519. The bundle contains:

| Field | Purpose |
|---|---|
| `name` | Human-readable package identifier |
| `version` | Semantic version |
| `binary_cid` | **Hash of the compiled binary this proof covers** |
| `members` | Map from CID → canonical bytes of every memento in the bundle |
| `signer` | CID of the signer's public-key memento |
| `signature` | Ed25519 signature of the unsigned body |

The `binary_cid` is the **supply chain anchor**. When the framework loads a
`.proof` bundle, it checks:

1. Signature is valid (developer signed this bundle)
2. `binary_cid` matches the hash of the currently running binary
3. Every member CID matches the BLAKE3-512 of its canonical bytes

If any check fails, the bundle is **rejected**. This means:

- **Compiler backdoors** are detected (different binary → different CID)
- **Runtime patches** are detected (JIT override → binary hash changes)
- **Supply chain injection** is detected (wrong package → wrong CID)
- **Dependency confusion** is detected (wrong version → wrong CID)

### The witnessed proof memento

Separately from the `.proof` bundle, the **build script** mints a **witnessed
proof memento** when it verifies your function body against the target
contract:

```
Memento {
  bindingHash: hash({ sourceLayer, sourceSymbol }),
  propertyHash: hash("bridge:" + sourceSymbol),
  verdict: "holds",
  producer: "z3@4.13",
  inputCids: [A, B],
  evidence: {
    kind: "evidence",
    proofType: "smt-lib",
    certificate: {
      tool: "z3",
      version: "4.13.0",
      formulaHash: hash(formula),
      proofData: "(unsat)"
    }
  }
}
```

This memento is **not** inside the `.proof` bundle (which is signed by the
package author). It is minted by **your** build script, signed by **your**
build key, and stored in **your** `target/provekit/` directory. It is the
witness that says: "I, the build system, checked that the body of
`my_parse_int` satisfies contract `A`, and here is the Z3 model to prove it."

Together, the `.proof` bundle (developer-signed, pins the binary) and the
witnessed proof memento (build-system-signed, pins the verification event)
form a **closed loop**:

```
Developer: "Here is my binary and its contracts, signed by me."
Build system: "I verified your function body against those contracts,
signed by me."
Framework: "Both signatures check out, hashes match, transitive bridge
resolves. The claim transfers to the reference spec."
```

Any change — source edit, compiler upgrade, dependency bump — changes a CID,
breaks the chain, and trips an alarm at compile time. This is by design.
Verification is not a runtime check; it is a **compile-time hash-chain gate**.

## Formulas

### IrFormula

```ebnf
IrFormula ::= QuantifierFormula
            | ConnectiveFormula
            | AtomicFormula
            | ChoiceFormula
```

The `kind` field is the discriminator for every formula and term node. It is
always the first key.

The maximal-uniformity rule for the IR: every node has `kind`, then `name`
(when applicable), then payload (`sort` / `body` / `args` / `operands` /
`value`). There is no `varName` (variable names use `name`); there is no
`conjuncts` / `disjuncts` / `antecedent` / `consequent` (boolean connectives
use `operands`); there is no `lambda` wrapper around a quantifier's body
(the quantifier carries its bound variable directly). The reader holds the
entire IR in their head.

### QuantifierFormula

Locked key order: `kind`, `name`, `sort`, `body`.

```ebnf
QuantifierFormula ::= "{"
                        "\"kind\"" ":" QuantifierKind ","
                        "\"name\"" ":" String ","
                        "\"sort\"" ":" Sort ","
                        "\"body\"" ":" IrFormula
                      "}"

QuantifierKind ::= "\"forall\"" | "\"exists\""
```

The `name` field is the bound variable's identifier. References to this
variable inside `body` are `VarTerm` nodes whose `name` matches.

**INVARIANT QuantifierFormula.HasSort:**
```
∀q: QuantifierFormula → HasKey(q, "sort") ∧ IsSort(q.sort)
```
Every quantifier (forall/exists) MUST have a `sort` field specifying the type
of its bound variable. This sort is authoritative for all VarTerms within
the quantifier's body that have matching name.

**INVARIANT QuantifierFormula.HasBody:**
```
∀q: QuantifierFormula → HasKey(q, "body") ∧ IsIrFormula(q.body)
```
Every quantifier MUST have a non-null `body` field containing a valid IrFormula.

### ConnectiveFormula

Locked key order: `kind`, `operands`.

```ebnf
ConnectiveFormula ::= "{"
                        "\"kind\"" ":" ConnectiveKind ","
                        "\"operands\"" ":" "[" IrFormula ( "," IrFormula )* "]"
                      "}"

ConnectiveKind ::= "\"and\"" | "\"or\"" | "\"not\"" | "\"implies\""
```

**Arity rules** (post-grammar):

- `not` MUST have exactly 1 operand.
- `implies` MUST have exactly 2 operands; `operands[0]` is the antecedent,
  `operands[1]` the consequent.
- `and` and `or` MUST have 2 or more operands. Empty/singleton `and`/`or` is
  not a valid IR shape; the canonicalizer's AC pass produces 2+ operands or
  collapses to a non-connective form.

Validators reject ConnectiveFormula nodes with arity violations.

**INVARIANT ConnectiveFormula.NotArity:**
```
∀c: ConnectiveFormula
  (c.kind = "not") → Len(c.operands) = 1
```
The `not` connective must have exactly one operand.

**INVARIANT ConnectiveFormula.ImpliesArity:**
```
∀c: ConnectiveFormula
  (c.kind = "implies") → Len(c.operands) = 2
```
The `implies` connective must have exactly two operands, where the first is
the antecedent and the second is the consequent.

**INVARIANT ConnectiveFormula.AndOrArity:**
```
∀c: ConnectiveFormula
  (c.kind = "and" ∨ c.kind = "or") → Len(c.operands) ≥ 2
```
The `and` and `or` connectives must have at least two operands. Empty or
singleton connectives are not valid IR.

### AtomicFormula

Locked key order: `kind`, `name`, `args`.

```ebnf
AtomicFormula ::= "{"
                    "\"kind\"" ":" "\"atomic\"" ","
                    "\"name\"" ":" String ","
                    "\"args\"" ":" "[" ( IrTerm ( "," IrTerm )* )? "]"
                  "}"

AtomicName ::= "\"=\"" | "\"≠\"" | "\"<\"" | "\"≤\""
             | "\">\"" | "\"≥\""
             | "\"true\"" | "\"false\""
             | "\"subset\"" | "\"member\""
             | "\"kind-of\"" | "\"data-flows-to\""
             | "\"dominates\"" | "\"post-dominates\""
             | "\"transition-from-to\"" | "\"on-path\""
             | "\"bvult\"" | "\"bvule\"" | "\"bvugt\"" | "\"bvuge\""
             | "\"bvslt\"" | "\"bvsle\"" | "\"bvsgt\"" | "\"bvsge\""
             | KitDefinedAtomicName
```

`KitDefinedAtomicName` is any String that does not collide with a built-in
atomic name. The parser does **not** reject unknown names: kits may define
new atomic predicates without rev-locking the parser. (Strict mode is
offered as a parser option; see "Strict mode" below.)

The use of `name` (not `predicate`) for the atomic's identifier matches
every other named node in the IR. The kind discriminator (`"atomic"`) carries
the information that this `name` is an atomic-predicate name; no separate
field key is needed to communicate that.

**INVARIANT AtomicFormula.HasName:**
```
∀a: AtomicFormula → HasKey(a, "name") ∧ IsString(a.name)
```
Every atomic formula MUST have a non-empty `name` field identifying the predicate.

**INVARIANT AtomicFormula.HasArgs:**
```
∀a: AtomicFormula → HasKey(a, "args") ∧ IsArray(a.args)
```
Every atomic formula MUST have an `args` field containing an array of IrTerms.

**INVARIANT AtomicFormula.KnownPredicate:**
```
∀a: AtomicFormula → IsBuiltInPredicate(a.name) ∨ IsKitDefinedPredicate(a.name)
```
The atomic's `name` must be either a built-in predicate (standard SMT-LIB
operators like =, <, >, etc.) or a kit-defined predicate. Unknown predicates
are only rejected in strict mode.

### ChoiceFormula

Locked key order: `kind`, `varName`, `sort`, `body`.

```ebnf
ChoiceFormula ::= "{"
                   "\"kind\"" ":" "\"choice\"" ","
                   "\"varName\"" ":" String ","
                   "\"sort\"" ":" Sort ","
                   "\"body\"" ":" IrFormula
                 "}"
```

A `ChoiceFormula` represents the **definite description** operator (εx. P(x)),
also known as "the unique x such that P(x)". This asserts that there exists
exactly one element satisfying the body formula, and binds that element to
`varName` for use within the formula.

This is more powerful than `exists` because it asserts **uniqueness**, making
it suitable for specifications that reference the exact value produced by a
computation (e.g., "the result of parsing string s is x").

**INVARIANT ChoiceFormula.HasVarName:**
```
∀c: ChoiceFormula → HasKey(c, "varName") ∧ IsString(c.varName)
```
Every choice formula MUST have a `varName` field identifying the chosen variable.

**INVARIANT ChoiceFormula.HasSort:**
```
∀c: ChoiceFormula → HasKey(c, "sort") ∧ IsSort(c.sort)
```
Every choice formula MUST have a `sort` field specifying the type of the
chosen element.

**INVARIANT ChoiceFormula.HasBody:**
```
∀c: ChoiceFormula → HasKey(c, "body") ∧ IsIrFormula(c.body)
```
Every choice formula MUST have a `body` field containing a valid IrFormula.

## Terms

### IrTerm

```ebnf
IrTerm ::= VarTerm | ConstTerm | CtorTerm | LambdaTerm | LetTerm
```

### VarTerm

Locked key order: `kind`, `name`.

```ebnf
VarTerm ::= "{"
              "\"kind\"" ":" "\"var\"" ","
              "\"name\"" ":" String
            "}"
```

A `VarTerm` carries no sort. The variable's sort is determined by the
enclosing `QuantifierFormula` whose `name` matches, or — for free variables
introduced by a contract memento's `outBinding` — by the substitution rule
at call sites (the substituted expression's sort). Producers MUST NOT add a
`sort` field; validators MUST reject `VarTerm`s with extra fields.

### ConstTerm

Locked key order: `kind`, `value`, `sort`.

```ebnf
ConstTerm ::= "{"
                "\"kind\"" ":" "\"const\"" ","
                "\"value\"" ":" ConstValue ","
                "\"sort\"" ":" Sort
              "}"

ConstValue ::= Number | String | Bool | Null
```

A `ConstTerm` is the only term kind that carries `sort`: the literal value's
type is not derivable from binding scope or signature. `Number`, `String`,
`Bool`, and `Null` are the permitted JSON value shapes; the `sort` field
disambiguates (e.g. `42` could be `Int` or `Real`).

Bigint values that exceed JavaScript's safe integer range MAY be emitted as
a JSON Number (current TS behavior) or as a String with prefix
`"bigint:<digits>"` (canonicalizer's convention). Parsers MUST accept either
shape.

### CtorTerm

Locked key order: `kind`, `name`, `args`.

```ebnf
CtorTerm ::= "{"
               "\"kind\"" ":" "\"ctor\"" ","
               "\"name\"" ":" String ","
               "\"args\"" ":" "[" ( IrTerm ( "," IrTerm )* )? "]"
             "}"
```

A `CtorTerm` carries no sort. The ctor's return sort is determined by its
declaration in a kit's bridge or extension memento (`irReturnSort` field).
Producers MUST NOT add a `sort` field; validators MUST reject `CtorTerm`s
with extra fields. Two `CtorTerm` nodes with the same `name` and `args`
must hash identically regardless of where they appear; carrying a `sort`
field would make textually-equal ctor invocations hash differently in
different scopes, which defeats the canonicalization promise.

`args` MAY be empty (a nullary constructor like `parseInt()` taking no
arguments — uncommon but permitted by the IR types).

### LambdaTerm

Locked key order: `kind`, `paramName`, `paramSort`, `body`.

```ebnf
LambdaTerm ::= "{"
                "\"kind\"" ":" "\"lambda\"" ","
                "\"paramName\"" ":" String ","
                "\"paramSort\"" ":" Sort ","
                "\"body\"" ":" IrTerm
              "}"
```

A `LambdaTerm` represents a first-class function (λx: τ. body). The
`paramName` is the bound variable, `paramSort` is its type, and `body` is the
function's computation. Lambda terms enable higher-order reasoning and can be
applied to arguments via `AppTerm`.

Producers MUST NOT add a `sort` field; the return sort is derived from the
body's type via the enclosing context.

**INVARIANT LambdaTerm.HasParamSort:**
```
∀λ: LambdaTerm → HasKey(λ, "paramSort") ∧ IsSort(λ.paramSort)
```
Every lambda MUST have a `paramSort` field specifying the type of its
parameter.

**INVARIANT LambdaTerm.HasBody:**
```
∀λ: LambdaTerm → HasKey(λ, "body") ∧ IsIrTerm(λ.body)
```
Every lambda MUST have a `body` field containing a valid IrTerm.

### LetTerm

Locked key order: `kind`, `bindings`, `body`.

```ebnf
LetTerm ::= "{"
             "\"kind\"" ":" "\"let\"" ","
             "\"bindings\"" ":" "[" LetBinding ( "," LetBinding )* "]" ","
             "\"body\"" ":" IrTerm
           "}"

LetBinding ::= "{"
                "\"name\"" ":" String ","
                "\"boundTerm\"" ":" IrTerm
              "}"
```

A `LetTerm` provides local bindings (let x = e1 in e2). The `bindings` array
contains one or more name-term pairs that are in scope for evaluating `body`.
Bindings are evaluated sequentially (later bindings can reference earlier ones).

Producers MUST NOT add a `sort` field; the body's sort is propagated to the
enclosing context.

**INVARIANT LetTerm.NonEmptyBindings:**
```
∀l: LetTerm → HasKey(l, "bindings") ∧ Len(l.bindings) ≥ 1
```
A let expression MUST have at least one binding.

**INVARIANT LetTerm.HasBody:**
```
∀l: LetTerm → HasKey(l, "body") ∧ IsIrTerm(l.body)
```
A let expression MUST have a body term.

### Formal Invariants

**INVARIANT VarTerm.NoSortField:**
```
∀t: VarTerm → ¬HasKey(t, "sort")
```
Every VarTerm MUST NOT contain a `sort` field. This is required because the
variable's sort is determined by its lexical context, not by the term itself.

**INVARIANT VarTerm.SortFromQuantifier:**
```
∀v: VarTerm, q: QuantifierFormula, env: Environment
  (InScope(v, q) ∧ v.name = q.name) → Sort(v, env) = q.sort
```
A variable that appears in the body of a quantifier with matching name inherits
its sort from that quantifier's `sort` field. This is the authoritative source
for bound variable sorts.

**INVARIANT VarTerm.SortFromSubstitution:**
```
∀v: VarTerm, s: Substitution, env: Environment
  (v ∈ FreeVars(s) ∧ SubstitutedBy(s, v) = e) → Sort(v, env) = Sort(e, env)
```
A free variable introduced by outBinding substitution derives its sort from
the substituting expression. The sort of the argument expression propagates to
the variable it replaces.

**INVARIANT ConstTerm.HasSort:**
```
∀t: ConstTerm → HasKey(t, "sort") ∧ IsSort(t.sort)
```
Every ConstTerm MUST have a `sort` field containing a valid Sort. This is required
because literal values (like `42`) are ambiguous without type information.

**INVARIANT CtorTerm.NoSortField:**
```
∀t: CtorTerm → ¬HasKey(t, "sort")
```
A CtorTerm MUST NOT contain a `sort` field. The return sort is determined by
the constructor's declaration in the kit's bridge, not by the term itself.
Including a sort field would break canonicalization guarantees.

**INVARIANT LambdaTerm.NoSortField:**
```
∀t: LambdaTerm → ¬HasKey(t, "sort")
```
A LambdaTerm MUST NOT contain a `sort` field at the top level. The return sort
is derived from the body's type via the enclosing context. The `paramSort` field
specifies the parameter's type, not the lambda's result type.

**INVARIANT LambdaTerm.ParamSortFromEnclosing:**
```
∀λ: LambdaTerm, env: Environment → Sort(λ.paramName, env) = λ.paramSort
```
The parameter name of a lambda is bound with the lambda's `paramSort` in the
environment when type-checking the body. The body's sort propagates to the
enclosing context.

**INVARIANT LetTerm.BindingSortPropagation:**
```
∀l: LetTerm, env: Environment, i: Index
  Sort(l.bindings[i].name, env) = Sort(l.bindings[i].boundTerm, env)
```
Each let binding introduces its name into the environment with the sort of its
bound term. Subsequent bindings and the body can reference this name.

**INVARIANT ChoiceFormula.Uniqueness:**
```
∀c: ChoiceFormula → 
  ∃! x: c.sort. c.body[x/c.varName]
```
A ChoiceFormula asserts that there exists exactly one element satisfying the
body formula. This is stronger than `exists` (which only asserts existence)
and enables definite description: "the unique x such that P(x)".

**INVARIANT EvidenceTerm.FormulaHashMatches:**
```
∀e: EvidenceTerm, f: IrFormula
  (e.attachedTo = f) → e.certificate.formulaHash = Hash(f)
```
When evidence is attached to a formula, the certificate's formulaHash MUST
match the hash of the attached formula. This prevents evidence forgery by
ensuring the proof is for the correct claim.

## Sorts

### Sort

```ebnf
Sort ::= PrimitiveSort | BitvecSort | SetSort | TupleSort | FunctionSort | DependentSort | FloatSort | RegionSort
```

### PrimitiveSort

Locked key order: `kind`, `name`.

```ebnf
PrimitiveSort ::= "{"
                    "\"kind\"" ":" "\"primitive\"" ","
                    "\"name\"" ":" String
                  "}"
```

The grammar allows any String as a primitive sort name. The canonical built-in
names are `"Bool"`, `"Int"`, `"Real"`, `"String"`, `"Ref"`, `"Node"`, `"Edge"`,
`"Region"`, `"Time"`. Kit-defined extensions (e.g. `"Address"`) are accepted
in non-strict mode.

### BitvecSort

Locked key order: `kind`, `width`.

```ebnf
BitvecSort ::= "{"
                 "\"kind\"" ":" "\"bitvec\"" ","
                 "\"width\"" ":" PositiveInteger
               "}"

PositiveInteger ::= Number  /* must be a positive integer ≤ 2^53 - 1 */
```

### SetSort

Locked key order: `kind`, `element`.

```ebnf
SetSort ::= "{"
              "\"kind\"" ":" "\"set\"" ","
              "\"element\"" ":" Sort
            "}"
```

### TupleSort

Locked key order: `kind`, `elements`.

```ebnf
TupleSort ::= "{"
                "\"kind\"" ":" "\"tuple\"" ","
                "\"elements\"" ":" "[" ( Sort ( "," Sort )* )? "]"
              "}"
```

### FunctionSort

Locked key order: `kind`, `args`, `return`.

```ebnf
FunctionSort ::= "{"
                   "\"kind\"" ":" "\"function\"" ","
                   "\"args\"" ":" "[" Sort ( "," Sort )* "]" ","
                   "\"return\"" ":" Sort
                 "}"
```

The `args` array must contain one or more Sort elements. The `return` field
is a single Sort.

### DependentSort

Locked key order: `kind`, `name`, `indexVar`, `indexSort`.

```ebnf
DependentSort ::= "{"
                    "\"kind\"" ":" "\"dependent\"" ","
                    "\"name\"" ":" String ","
                    "\"indexVar\"" ":" String ","
                    "\"indexSort\"" ":" Sort
                  "}"
```

`name` is a type-level name (e.g. `"Vec"`). `indexVar` is a value-level
variable the type depends on (e.g. `"n"` for `Vec<n>`). `indexSort`
constrains the sort of the index variable.

### FloatSort

Locked key order: `kind`, `width`.

```ebnf
FloatSort ::= "{"
                "\"kind\"" ":" "\"float\"" ","
                "\"width\"" ":" PositiveInteger
              "}"
```

`width` is the bit-width of the IEEE-754 float: 16, 32, 64, or 128.
FloatSort is opaque at the SMT-LIB and Coq layers (bit-pattern encoding);
full floating-point theory is deferred to a follow-up RFC (#385).

### RegionSort

Locked key order: `kind`, `name` (alphabetical).

```ebnf
RegionSort ::= "{"
                 "\"kind\"" ":" "\"region\"" ","
                 "\"name\"" ":" String
               "}"
```

`name` is a Rust lifetime name: e.g. `"'a"`, `"'static"`, or a fresh
region variable like `"'r0"`. RegionSort is an opaque carrier for
borrow-checker lifetime variables so that lifted Rust functions with
lifetime parameters receive well-typed contracts without collapsing
lifetimes into a primitive sort (which would break CID stability and
the sort-collapse invariants from #384 A.1).

RegionSort MUST NOT reach the SMT-LIB or Coq backends — regions are
pre-resolved in composition. Prerequisite for #384 C.9 (Outlives
predicates).

### Formal Invariants

**INVARIANT PrimitiveSort.ValidName:**
```
∀s: PrimitiveSort → HasKey(s, "kind") ∧ s.kind = "primitive" ∧
                     HasKey(s, "name") ∧ IsString(s.name)
```
A PrimitiveSort must have `kind: "primitive"` and a string `name` field.

**INVARIANT BitvecSort.ValidWidth:**
```
∀s: BitvecSort → HasKey(s, "kind") ∧ s.kind = "bitvec" ∧
                 HasKey(s, "width") ∧ IsPositiveInteger(s.width) ∧ s.width > 0
```
A BitvecSort must have a positive integer width greater than 0.

**INVARIANT SetSort.ValidElement:**
```
∀s: SetSort → HasKey(s, "kind") ∧ s.kind = "set" ∧
              HasKey(s, "element") ∧ IsSort(s.element)
```
A SetSort must have an `element` field containing a valid Sort.

**INVARIANT TupleSort.ValidElements:**
```
∀s: TupleSort → HasKey(s, "kind") ∧ s.kind = "tuple" ∧
                HasKey(s, "elements") ∧ IsArray(s.elements) ∧
                ∀e ∈ s.elements → IsSort(e)
```
A TupleSort must have an `elements` array containing at least one valid Sort.

**INVARIANT FunctionSort.ValidArgsAndReturn:**
```
∀s: FunctionSort → HasKey(s, "kind") ∧ s.kind = "function" ∧
                    HasKey(s, "args") ∧ IsArray(s.args) ∧
                    ∀a ∈ s.args → IsSort(a) ∧ len(s.args) >= 1 ∧
                    HasKey(s, "return") ∧ IsSort(s.return)
```
A FunctionSort must have a non-empty `args` array of Sorts and a valid `return` Sort.

**INVARIANT DependentSort.ValidFields:**
```
∀s: DependentSort → HasKey(s, "kind") ∧ s.kind = "dependent" ∧
                     HasKey(s, "name") ∧ IsString(s.name) ∧ len(s.name) > 0 ∧
                     HasKey(s, "indexVar") ∧ IsString(s.indexVar) ∧ len(s.indexVar) > 0 ∧
                     HasKey(s, "indexSort") ∧ IsSort(s.indexSort)
```
A DependentSort must have a non-empty `name`, a non-empty `indexVar`, and a valid `indexSort`.

**INVARIANT FloatSort.ValidWidth:**
```
∀s: FloatSort → HasKey(s, "kind") ∧ s.kind = "float" ∧
                HasKey(s, "width") ∧ IsPositiveInteger(s.width) ∧ s.width ∈ {16, 32, 64, 128}
```
A FloatSort must have a `width` field that is one of the four IEEE-754 standard widths.

**INVARIANT RegionSort.ValidName:**
```
∀s: RegionSort → HasKey(s, "kind") ∧ s.kind = "region" ∧
                  HasKey(s, "name") ∧ IsString(s.name) ∧ len(s.name) > 0
```
A RegionSort must have a non-empty string `name` field carrying the lifetime name.

**INVARIANT RegionSort.OpaqueToBackends:**
```
∀s: RegionSort, backend: Backend →
  backend ∈ {SMT-LIB, Coq} → ¬ReachesBackend(s, backend)
```
RegionSorts MUST be pre-resolved before reaching the SMT-LIB or Coq compiler layers.

### Sort Examples

**FunctionSort — minimal (identity function on Int):**
```json
{
  "kind": "function",
  "args": [{"kind": "primitive", "name": "Int"}],
  "return": {"kind": "primitive", "name": "Int"}
}
```

**FunctionSort — nested (two-arg function returning a function):**
```json
{
  "kind": "function",
  "args": [
    {"kind": "primitive", "name": "Int"},
    {"kind": "primitive", "name": "Bool"}
  ],
  "return": {
    "kind": "function",
    "args": [{"kind": "primitive", "name": "String"}],
    "return": {"kind": "primitive", "name": "Real"}
  }
}
```

**DependentSort — minimal (Vec indexed by Int):**
```json
{
  "kind": "dependent",
  "name": "Vec",
  "indexVar": "n",
  "indexSort": {"kind": "primitive", "name": "Int"}
}
```

**DependentSort — nested (FinSet whose index is itself a function sort):**
```json
{
  "kind": "dependent",
  "name": "FinSet",
  "indexVar": "card",
  "indexSort": {
    "kind": "function",
    "args": [{"kind": "primitive", "name": "Int"}],
    "return": {"kind": "primitive", "name": "Bool"}
  }
}
```

**FloatSort — 32-bit float:**
```json
{"kind": "float", "width": 32}
```

**FloatSort — 64-bit float:**
```json
{"kind": "float", "width": 64}
```

**RegionSort — named lifetime:**
```json
{"kind": "region", "name": "'a"}
```

**RegionSort — static lifetime:**
```json
{"kind": "region", "name": "'static"}
```

**RegionSort — fresh region variable:**
```json
{"kind": "region", "name": "'r0"}
```

## Source positions

### Locus

A `Locus` identifies a position in a source file. It is the canonical source-position type used by every memento that needs to point at a location in a developer's code, including (but not limited to) call-edge mementos (per `2026-05-03-bridge-linkage-protocol.md` §1), invariant fix-loop mementos (per `2026-04-27-standing-invariant-runtime.md`), and lift-time diagnostics.

Locked key order (alphabetical, per JCS): `column`, `file`, `line`. The grammar:

```ebnf
Locus       ::= "{" '"column"' ":" Column "," '"file"' ":" File "," '"line"' ":" Line "}"

Column      ::= NaturalInteger          (* 1-based column index *)
File        ::= JsonString              (* canonical, slash-separated POSIX-style relative path *)
Line        ::= NaturalInteger          (* 1-based line index *)

NaturalInteger ::= "0" | DigitSansZero ( Digit )*
```

```hoare
{ true } IsLocus(o) { ⇔ IsObject(o) ∧ HasKey(o, "column") ∧ HasKey(o, "file") ∧ HasKey(o, "line") ∧
                       IsNaturalInteger(o.column) ∧ IsString(o.file) ∧ IsNaturalInteger(o.line) }
```

**Required fields, no defaults.** All three keys MUST be present. A `Locus` with a missing field is a hard parse error in strict mode and a fatal lifter bug in lenient mode. There is no implicit zero, no synthetic placeholder. If the lifter cannot determine a real source position (e.g., for a derived contract with no source backing), it MUST omit the `Locus`-bearing field entirely rather than emit a Locus with garbage values.

**File field semantics.** The `file` value is a relative POSIX-style path (forward slashes only), rooted at the project's lift-time root directory. Lifters MUST emit forward slashes regardless of host filesystem (Windows lifters convert `\` to `/`). Lifters MUST NOT include drive letters, file:// URLs, or absolute paths; cross-machine byte-equivalence depends on every kit emitting identical relative paths for identical projects.

**Line and column conventions.** `line` and `column` are both 1-based natural integers. Column counts UTF-16 code units to match LSP semantics (per `2026-05-03-lsp-protocol.md`); kits whose host language uses byte offsets or grapheme clusters MUST convert to UTF-16 code units before emission. Tab handling: tabs count as one column position; lifters MUST NOT expand tabs for column counting (otherwise the same source produces different Locus values on different render configurations).

**JCS encoding.** Per `2026-04-30-canonicalization-grammar.md`, every Locus is JCS-encoded with keys sorted alphabetically: `column` first, `file` second, `line` third. The integers MUST be emitted as bare decimal digits (no leading zeros except for `0` itself, no exponent, no decimal point); the string MUST be emitted with the JCS-mandated escape sequences. Two Locus values referring to the same position MUST produce byte-identical JCS encoding across all conforming kits.

**Why this matters.** Locus is a leaf type embedded in many higher-level mementos. A drift in Locus encoding (a kit emitting `{file, line, column}` order, or expanding tabs, or using byte offsets) cascades into every memento that contains a Locus, which cascades into every contractCid and bridgeCid that hashes those mementos, which breaks cross-kit byte-equivalence at the substrate level. The stake is the §11/§12 pin convergence: if Locus drifts, identical content produces different addresses across kits, and the pin breaks. Locus is normative because every kit must converge on its bytes for the substrate's content-addressing to hold.

### Formal Invariants

**Test Plan** (Section: Locus Conformance)
| Invariant | Formula |
|-----------|---------|
| LocusKeyOrder | `∀l Locus → JsonKeys(JCS(l)) = ["column","file","line"]` |
| LocusFileForwardSlash | `∀l Locus → ¬Contains(l.file, "\\")` |
| LocusFileRelative | `∀l Locus → ¬StartsWith(l.file, "/") ∧ ¬Matches(l.file, /^[A-Za-z]:/)` |
| LocusLineColumnOneBased | `∀l Locus → l.line ≥ 1 ∧ l.column ≥ 1` |

## Determinism rules

These are global constraints that apply to all productions above. They are
what makes the grammar **byte-deterministic**.

1. **Key order is fixed per node kind.** The grammar above lists keys in their
   emitted order. Emitters MUST produce keys in this order. Parsers SHOULD
   accept any key order during ingest; conformant emitters never produce a
   reorder. (See "Strict mode" for a parser option that enforces emit order.)

2. **No whitespace.** No spaces, tabs, or newlines between tokens. JSON
   permits whitespace; the kit-emit form does not.

3. **No trailing commas.** Standard JSON.

4. **Numbers in canonical JSON form.** Integers serialize without a fractional
   part; doubles use V8's `Number.prototype.toString` rendering (the same one
   the canonicalizer's pass 7 relies on). NaN and ±Infinity are not permitted
   in any IR value and the parser MUST reject them.

   *Note on parser-side number normalization.* `JSON.parse` silently
   normalizes some non-canonical number forms (e.g. `1.0` becomes the same
   in-memory `1` as `1`). Hand-crafted JSON containing a non-canonical
   numeric form will parse, but its re-emit will use the canonical form, so
   non-canonical input does NOT round-trip byte-identically. This is fine
   for kit-emitted input (the kits always emit canonical numbers) and is a
   documented divergence between "what the grammar accepts" and "what the
   round-trip property guarantees."

5. **String escaping is JSON-standard.** No unnecessary escapes; no `\/`
   solidus escape; non-ASCII characters MAY be emitted literally (UTF-8) or as
   `\uXXXX` escapes — kits are not required to agree on this beyond what their
   stdlib serializers produce. The fixtures currently used round-trip
   identically across kits with literal UTF-8 (the `≠`, `≤`, `≥` predicate
   names appear as raw three-byte UTF-8 sequences in the emitted JSON).

6. **Closed objects.** No node kind admits "extra" keys beyond those listed
   in its production. Parsers MUST reject documents containing unknown keys
   on a known node kind. (This is what makes the grammar tight; without it,
   kits could drift by silently emitting trailing fields.)

## Reference parser

The reference parser lives at `src/ir/grammar/parse.ts`. It exposes:

```typescript
export function parseDocument(json: string): Declaration[]
export function parseFormula(json: string): IrFormula
export function parseTerm(json: string): IrTerm
export function parseSort(json: string): Sort
```

Each parser:

- Accepts UTF-8 input encoded as a JavaScript string.
- Produces typed IR values matching `src/ir/formulas.ts` and
  `src/ir/symbolic/property.ts`.
- Throws a `GrammarParseError` (extends `Error`) on malformed input. The error
  carries:
  - `path`: a JSON Pointer (RFC 6901) to the offending node;
  - `expected`: a description of what was expected;
  - `actual`: the offending value (truncated for readability).

### Strict mode

`parseDocument(json, { strict: true })` additionally enforces:

- Key order matches the emit order specified in this document.
- Predicate name is one of the locked built-ins or matches `^[a-zA-Z_][a-zA-Z0-9_-]*$`.
- Primitive sort name is one of the nine canonical names.

Strict mode is what cross-language fixtures are validated under. Non-strict
mode is the parser's default (kits ship new predicates between releases; the
parser doesn't need a rev to ingest them).

**INVARIANT StrictMode.KeyOrder:**
```
∀d: Document, n: Node
  InStrictMode → EmitOrder(n) = ExpectedKeyOrder(n.kind)
```
In strict mode, the parser enforces that keys appear in the exact order
specified in the grammar for each node kind.

**INVARIANT StrictMode.PredicateName:**
```
∀a: AtomicFormula
  InStrictMode → (IsBuiltInPredicate(a.name) ∨ ValidIdentifier(a.name))
```
In strict mode, predicate names must be either built-in or match the regex
`^[a-zA-Z_][a-zA-Z0-9_-]*$`.

**INVARIANT StrictMode.PrimitiveSortName:**
```
∀s: PrimitiveSort
  InStrictMode → s.name ∈ {"Bool", "Int", "Real", "String", "Ref", "Node", "Edge", "Region", "Time"}
```
In strict mode, primitive sort names must be one of the nine canonical names.

### Round-trip property

The parser-emitter pair satisfies the following fixed-point property:

> For every byte sequence `B` that the grammar accepts,
> `emit(parseDocument(B)) === B`.

This is verified at test time against the three locked cross-language
fixtures (`scripts/cross-lang-equivalence/fixtures.txt`) and against
hand-built coverage examples for every node kind.

**INVARIANT RoundTrip.ParserPreservesStructure:**
```
∀B: ByteString
  (GrammarAccepts(B) ∧ ParseDocument(B) = d) → IsValidDocument(d)
```
If the grammar accepts a byte sequence, parsing it produces a valid document.

**INVARIANT RoundTrip.EmitterPreservesOrder:**
```
∀d: Document
  (IsValidDocument(d) ∧ Emit(d) = B) → GrammarAccepts(B)
```
If a document is valid, re-emitting it produces a byte sequence that the grammar accepts.

**INVARIANT RoundTrip.FixedPoint:**
```
∀B: ByteString
  (GrammarAccepts(B) ∧ ParseDocument(B) = d ∧ Emit(d) = B') → B = B'
```
Parsing then emitting returns the original bytes. This is the formal statement of the round-trip property.

## Relationship to the existing kits

The kits are the producers; the grammar is the spec; the parser is the
reference consumer. Each kit's serialization path independently must conform
to the grammar.

### Currently-conforming behavior

| Kit              | Conforms (today) | How                                                                                         |
|------------------|------------------|---------------------------------------------------------------------------------------------|
| TypeScript       | yes              | Manual object literals with deterministic key order; runs in `src/ir/symbolic/`.            |
| Rust             | yes              | `serde::Serialize` with field declaration order matching this document.                    |
| Go               | yes              | `encoding/json` with struct field order matching this document.                             |
| C++              | yes              | Hand-written JSON serialization in `implementations/cpp/provekit-ir-symbolic/include/`.                |

Conformance today is a *fact* (the harness verifies byte-equality on three
fixtures). This grammar promotes it to a *contract* — any future kit, or any
modification to an existing kit, must validate against the grammar.

### Conformance test plan (sketch)

A future `scripts/grammar-conformance/` harness would extend the existing
cross-language equivalence harness:

1. **Per-kit emit test.** For each fixture, run the kit, capture the JSON,
   feed it through the reference parser in **strict mode**. Pass = parser
   accepts. Fail = grammar violation (kit drift).

2. **Round-trip test.** For each fixture, parse the kit's JSON, then re-emit
   via a reference emitter (also lives at `src/ir/grammar/parse.ts`, exposed
   as `emit(value)`). Assert byte equality with the kit's original output.

3. **Negative tests.** Hand-craft documents that violate each rule (extra
   keys, wrong key order in strict mode, NaN, missing required fields, etc.)
   and assert the parser rejects each with a structured error.

4. **Coverage matrix.** Each node kind (forall, exists, and, or, not,
   implies, atomic, var, const, ctor, primitive sort, bitvec sort, set sort,
   tuple sort, function sort, lambda, property declaration, bridge
   declaration) has at least one positive fixture and at least one negative
   fixture.

**INVARIANT TestPlan.CoverageComplete:**
```
∀k: NodeKind → (PositiveFixtures(k) ≠ ∅) ∧ (NegativeFixtures(k) ≠ ∅)
```
Every node kind must have at least one positive test case (valid input that
should parse) and one negative test case (invalid input that should be rejected).

**INVARIANT TestPlan.NegativeTestsReject:**
```
∀n: NegativeTestCase
  (ParseDocument(n.input) throws GrammarParseError)
```
All negative test cases must be rejected by the parser with a GrammarParseError.

Step (1) is the load-bearing one for cross-language drift detection. Today
the harness in `scripts/cross-lang-equivalence/` verifies kit-vs-kit
byte-equality; under the grammar, it would additionally verify each kit
against an *external* spec, catching the case where all kits drift together.

The current harness's golden hashes (`scripts/cross-lang-equivalence/goldens.txt`)
are computed over the kit-emit form described by this grammar. Promotion to
a grammar-conformance regime does **not** invalidate those goldens; the
grammar describes exactly what produced them.

## Evidence

The IR can carry evidence (proof certificates) alongside formulas. This enables
efficient verification without re-proving: a previously-verified proof can be
attached to a claim and checked for validity rather than re-computed.

### EvidenceTerm

Locked key order: `kind`, `proofType`, `certificate`.

```ebnf
EvidenceTerm ::= "{"
                   "\"kind\"" ":" "\"evidence\"" ","
                   "\"proofType\"" ":" ProofType ","
                   "\"certificate\"" ":" EvidenceCertificate
                 "}"

ProofType ::= "\"smt-lib\"" | "\"coq\"" | "\"custom\""

EvidenceCertificate ::= "{"
                         "\"tool\"" ":" String ","
                         "\"version\"" ":" String ","
                         "\"formulaHash\"" ":" String ","
                         "\"proofData\"" ":" String
                       "}"
```

An `EvidenceTerm` attaches a proof certificate to a formula. The `proofType`
identifies the solver that produced the proof (smt-lib for Z3/CVC5, coq for
Coq, custom for other backends). The `certificate` contains:
- `tool`: solver name
- `version`: solver version
- `formulaHash`: SHA-256 of the proven formula (for cross-checking)
- `proofData`: solver-specific proof artifact

**INVARIANT EvidenceTerm.HasProofType:**
```
∀e: EvidenceTerm → HasKey(e, "proofType") ∧ IsProofType(e.proofType)
```
Every evidence term MUST have a valid `proofType`.

**INVARIANT EvidenceTerm.HasCertificate:**
```
∀e: EvidenceTerm → HasKey(e, "certificate") ∧ IsObject(e.certificate)
```
Every evidence term MUST have a `certificate` object.

### Integration with Declarations

Evidence can be attached to any formula-bearing declaration:

```ebnf
ContractDeclaration ::= "{"
                         "\"kind\"" ":" "\"property\"" ","
                         "\"name\"" ":" String ","
                         "\"formula\"" ":" IrFormula ","
                         "\"evidence\"" ":" EvidenceTerm "?"  // optional
                       "}"
```

When `evidence` is present, the verifier SHOULD:
1. Compute `formulaHash` of the attached formula
2. Compare against `certificate.formulaHash`
3. Validate `proofData` according to `proofType`

If evidence is absent or validation fails, the verifier recomputes the proof.

## Appendix A — Worked example: `forall_int_gt_zero`

The TS kit, given:

```typescript
property("forall_int_gt_zero", forAll(Int, (x) => gt(x, num(0))))
```

emits exactly this byte sequence (golden SHA256
`b4377644994579d5faafdd65c1d64fd0a70ec44639ac8218612f58892f91342e`):

```json
[{"kind":"property","name":"forall_int_gt_zero","formula":{"kind":"forall","sort":{"kind":"primitive","name":"Int"},"predicate":{"kind":"lambda","varName":"_x0","sort":{"kind":"primitive","name":"Int"},"body":{"kind":"atomic","predicate":">","args":[{"kind":"var","name":"_x0","sort":{"kind":"primitive","name":"Int"}},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]}}}}]
```

Every key here appears in the order locked by the corresponding production
above. The reference parser ingests this string, returns a typed
`Declaration[]` of length 1, and the reference emitter recovers the same
byte sequence. Strict-mode parse + round-trip is part of the test suite at
`src/ir/grammar/parse.test.ts`.

## Appendix B — Grammar choices and rationale

- **EBNF over PEG.** EBNF reads more naturally for a spec audience and
  doesn't need ordered choice (the `kind` discriminator does the work that
  PEG ordered choice would otherwise do). The grammar is unambiguous as
  written.

- **Insertion-order keys, not lexical order.** RFC 8785 (JCS) sorts keys
  lexicographically; this grammar locks insertion order instead. Rationale:
  the kits already emit insertion order (TS literals, Rust serde field
  order, Go struct order, C++ hand-written), and the cross-language goldens
  encode that. Switching to lex order would require simultaneous reissue of
  every kit and re-locking of every golden — a meaningless churn. The
  canonicalizer pipeline still uses JCS where it needs to (pass 7 / hash);
  the grammar describes a different layer.

- **Closed-object policy.** Strictness on extra keys keeps the grammar tight
  and prevents silent kit drift. New IR concepts (e.g. a future `iff`
  formula) require an explicit grammar update.

- **Open predicate names.** The TypeScript IR type allows `string` for
  AtomicPredicate as an open extension. The grammar reflects this in
  default mode and lets strict mode lock to the published list.
