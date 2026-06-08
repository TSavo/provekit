# Sugar: IR Extension Protocol

> Author: shared session 2026-04-30 (T + Claude). The protocol piece
> that makes the IR total: every meaningful invariant about code can
> be expressed in the IR, and any new domain (financial arithmetic,
> time-with-timezone, regex, capability bits, fixed-point, RNG, BV
> widths beyond what's bootstrapped) is added by **publishing an
> extension declaration memento**, not by forking the framework.

> Status: protocol spec, authoritative. Reference implementations
> (TypeScript, Rust, Go, C++) conform to this; if they drift, the
> code is a bug to fix. The grammar IS the spec; surrounding prose
> is exposition.

> Companion specs: `2026-04-30-ir-formal-grammar.md` (IR wire format,
> permissive about extension names),
> `2026-04-30-canonicalization-grammar.md` (how IR bytes hash;
> extension names canonicalize identically to built-ins),
> `2026-04-30-signatures-and-non-repudiation.md` (extension
> declarations are signed; same Ed25519 machinery),
> `2026-04-30-chain-validity-and-fail-closed.md` (fail-closed cases
> for unresolved extensions).

## 1. Why this protocol exists

Without protocol-level extension, every domain that needs a new sort,
predicate, or term constructor (fixed-point arithmetic, time-with-
timezone, regex string semantics, capability bits, BV widths beyond a
fixed set, modular arithmetic, probability distributions, vector
linear algebra, ...) becomes "fork the framework." That's the failure
mode the protocol-not-codebase frame was meant to prevent.

This spec formalizes the alternative: a new domain is an
**extension declaration memento**. The memento names the new
sort/predicate/ctor, declares its semantic content, names the compilers
that can handle it, is signed by its author, and is content-addressed
by CID. Verifiers resolve unknown names through these mementos; trust
the signatures; respect the compatibility metadata; **fail closed** on
anything they cannot resolve.

Architectural commitment captured here: **the IR is total via this
protocol.** No code-level invariant is "out of scope for Sugar"
because the framework's response to "we need a new sort" is "publish an
extension declaration," not "edit the framework."

## 2. The bootstrapping core

These names are NOT extensions. They're the protocol's bedrock,
universally understood by every conforming implementation, and are the
expression vocabulary an extension declaration may use to define new
items axiomatically:

### 2.1 Bootstrapping sorts

```
core-sort = "Bool" / "Int" / "Real" / "String"
```

Compound bootstrapping sorts (parameterized, not extensions):

```
bootstrap-sort = core-sort
              / set-sort       ; (Set element-sort)
              / tuple-sort     ; (Tuple sort1 sort2 ...)
              / function-sort  ; (Function dom1 dom2 ... -> range)
```

### 2.2 Bootstrapping predicates

```
core-predicate = "=" / "≠" / "<" / "≤" / ">" / "≥"
              / "true" / "false"
              / "subset" / "member"
```

### 2.3 Bootstrapping connectives + quantifiers

```
core-connective = "and" / "or" / "not" / "implies" / "iff"
core-quantifier = "forall" / "exists"
```

### 2.4 What's NOT bootstrapping

Domain primitives (parseInt, abs, floor, ceil, sqrt, sign, isNan,
isFinite, isInteger, max, min, stringLength, stringIncludes,
arrayLength, arrayIncludes), bitvector operators, IEEE-754 floating
point, set comprehension, regex, time arithmetic, fixed-point — all
extensions. Each one is published as an extension declaration even
when the kit ships with built-in helper functions; the kit's helpers
mint IR that REFERENCES the extension declaration's name, and a
verifier without the corresponding extension declaration in scope
fails closed when it encounters that name.

This is deliberate. The "core" is a minimum viable language for
expressing axioms over which extensions are defined. Anything that
can be defined in terms of the core is an extension; the core itself
is the irreducible base.

## 3. Extension declaration memento

The wire format. CDDL grammar:

```cddl
extension-declaration = {
  kind: "extension-declaration",

  ; What's being introduced. Exactly one of "sort", "predicate", or
  ; "ctor". Determines which of the type-signature variants applies.
  introduces: "sort" / "predicate" / "ctor",

  ; The name string that appears in IR JSON formulas referencing this
  ; extension. The protocol REQUIRES that name be unique across the
  ; resolver's scope; collisions are surfaced by the verifier as
  ; fail-closed conditions (see §5.3).
  name: tstr,

  ; Type signature. The active variant is determined by `introduces`.
  signature: sort-signature / predicate-signature / ctor-signature,

  ; Semantic declaration: at least one of the following is REQUIRED.
  ; A verifier MAY refuse to operate on an extension whose semantic
  ; declarations are insufficient for its compiler/solver target.
  semantics: [+ semantic-declaration],

  ; Compatibility metadata: which IR-compiler names (per sugar.config.yaml's
  ; SolverEntry.compiler field) can handle this extension. A verifier
  ; whose active compiler is not in this list MUST refuse to resolve
  ; this extension (see §5.4).
  compilers: [+ tstr],

  ; Signer block. Per signatures-and-non-repudiation spec.
  signer: cid,         ; CID of the signer's public key memento
  signature: bstr,     ; Ed25519 signature over canonical bytes of all
                       ; preceding fields (per canonicalization spec)

  ; Optional: a structured set of CIDs this extension declaration
  ; depends on (other extensions referenced in its semantic axioms).
  ; Verifiers MUST resolve every dependency before resolving this
  ; extension; cyclic dependencies fail closed.
  ? depends-on: [* cid],

  ; Optional: ISO-8601 timestamp of declaration; useful for revocation
  ; ordering. Signed bytes include this field.
  ? declaredAt: tstr,
}

sort-signature = {
  kind: "sort",
  ; Sort declarations may be parameterized (e.g. BitVec[N] takes a
  ; positive integer width). Parameter list MAY be empty.
  ? params: [* sort-param],
}

sort-param = {
  name: tstr,
  paramSort: "Int" / "Bool" / "String",
  ; Sort parameters at this version are scalar; higher-kinded
  ; parameters (sort-valued parameters) are out of scope until a
  ; later spec version.
}

predicate-signature = {
  kind: "predicate",
  ; Argument sorts. References to bootstrapping core sorts use their
  ; literal name; references to other extension sorts use their CID
  ; (so the dependency is content-addressed and resolvable).
  argSorts: [+ sort-ref],
}

ctor-signature = {
  kind: "ctor",
  argSorts: [* sort-ref],   ; may be nullary (constants)
  returnSort: sort-ref,
}

sort-ref = tstr / cid / parametric-sort-ref
parametric-sort-ref = {
  base: cid,                ; CID of the parameterized sort declaration
  args: [+ tstr / int],     ; concrete parameter values
}

semantic-declaration =
    smt-lib-theory-ref      ; references a stable formal theory
  / axiom-set                ; axioms expressed in the core IR + already-resolved extensions
  / proof-assistant-ref     ; Lean/Coq theorem statement reference
  / natural-language        ; lowest trust; verifiers MAY refuse

smt-lib-theory-ref = {
  kind: "smt-lib-theory",
  ; The official SMT-LIB theory name (e.g. "FixedSizeBitVectors",
  ; "FloatingPoint", "ArraysEx"). Verifiers whose compiler is
  ; smt-lib-conformant trust the SMT-LIB theory's published
  ; semantics; no further axioms required.
  theory: tstr,
  ; Optional version pin; default is "current" (whatever the
  ; verifier's solver supports).
  ? version: tstr,
}

axiom-set = {
  kind: "axiom-set",
  ; A list of IrFormula values (per the IR formal grammar) expressing
  ; the extension's semantics. Every reference inside these axioms
  ; MUST resolve to either bootstrapping core or a transitively-
  ; resolved dependency.
  axioms: [+ ir-formula],
}

proof-assistant-ref = {
  kind: "proof-assistant",
  ; Which proof assistant (lean4, coq, isabelle, ...).
  system: tstr,
  ; Identifier the assistant uses to retrieve the theorem statement
  ; (file path, mathlib lemma name, etc.).
  identifier: tstr,
  ; CID of the bytes that constitute the proof artifact, when
  ; available locally. Verifiers can re-check the proof if they
  ; have the corresponding compiler.
  ? proofCid: cid,
}

natural-language = {
  kind: "natural-language",
  text: tstr,
  ; Human description. Verifiers MUST treat this as the lowest-trust
  ; tier; conformant verifiers SHOULD refuse to resolve extensions
  ; whose ONLY semantic declaration is natural-language unless an
  ; explicit policy override is configured.
}

cid = tstr            ; multibase-encoded CID per IPLD; opaque to this spec
ir-formula = any      ; per the IR formal grammar (sibling spec)
```

The CDDL is the spec. Implementations MAY parse this directly via a
CDDL toolchain to obtain a validator; alternative encodings of the
same shape (e.g. JSON Schema generated from this CDDL) are conformant
iff they accept exactly the same documents.

## 4. Extension catalog

Extension declarations are organized into **extension catalogs** — the
set of declarations a verifier consults when resolving names. Same
shape as the property-catalog memento (the `proofHash`'s catalog), but
keyed on extension declaration CIDs rather than property CIDs.

```cddl
extension-catalog = {
  kind: "extension-catalog",
  ; Map from extension name (a string that appears in IR formulas) to
  ; the CID of the extension-declaration memento that defines it.
  entries: { + tstr => cid },

  ; Same signer block as other mementos. Catalogs that aggregate
  ; third-party extensions are signed by the catalog publisher; that
  ; publisher's signature certifies the AGGREGATE, not the individual
  ; extension declarations (which carry their own signatures).
  signer: cid,
  signature: bstr,
  ? declaredAt: tstr,
}
```

A verifier may have multiple catalogs in scope (project catalog, kit
catalog, transitively-included library catalogs). Resolution searches
all in-scope catalogs; collisions on the same name across catalogs
MUST surface as a fail-closed condition (§5.3).

## 5. Resolver semantics

The verifier's resolver is invoked when an IR formula references a
name that is not bootstrapping core. Inputs: the name string + the
verifier's active context (set of in-scope catalogs, verifier's
trusted-keys policy, active compiler). Output: a `ResolvedExtension`
or a fail-closed verdict.

### 5.1 Resolution algorithm (pseudocode)

```
function resolve(name, context):
    candidates = []
    for catalog in context.catalogs:
        if name in catalog.entries:
            cid = catalog.entries[name]
            candidates.append((cid, catalog))

    if len(candidates) == 0:
        FAIL_CLOSED(reason = "no extension declaration in scope for name", name = name)

    if len(candidates) > 1 and not all_same_cid(candidates):
        FAIL_CLOSED(reason = "extension name collision across catalogs", name = name, cids = candidates)

    cid = candidates[0].cid
    decl = context.mementoStore.fetch(cid)
    if decl == null:
        FAIL_CLOSED(reason = "extension declaration CID does not resolve", cid = cid)

    if not verify_signature(decl, context.trustedKeys):
        FAIL_CLOSED(reason = "extension declaration signature invalid", cid = cid)

    if not key_active_at(decl.signer, decl.declaredAt, context):
        FAIL_CLOSED(reason = "extension signer's key was revoked", cid = cid)

    if context.activeCompiler not in decl.compilers:
        FAIL_CLOSED(reason = "extension not compatible with active compiler",
                    cid = cid, active = context.activeCompiler, compatible = decl.compilers)

    for dep_cid in decl.depends_on or []:
        dep = context.mementoStore.fetch(dep_cid)
        if dep == null or not resolve_recursive(dep, context):
            FAIL_CLOSED(reason = "extension dependency unresolvable", cid = cid, dep = dep_cid)

    return ResolvedExtension(decl, source_catalog = candidates[0].catalog)
```

### 5.2 Validity rules (Datalog form)

The fail-closed cases stated as Datalog. Each rule is mechanically
checkable; if any rule's body holds, the verifier MUST reject.

```datalog
% 5.2.1 Unresolvable name
fail_closed(verifier V, name N, "no_resolution") :-
    references(formula F, N),
    not is_core_name(N),
    not exists_catalog_entry(V, N).

% 5.2.2 Cross-catalog collision
fail_closed(verifier V, name N, "name_collision") :-
    catalog_entry(V, C1, N, CID1),
    catalog_entry(V, C2, N, CID2),
    C1 != C2,
    CID1 != CID2.

% 5.2.3 Declaration CID does not resolve in store
fail_closed(verifier V, cid C, "declaration_missing") :-
    catalog_entry(V, _, _, C),
    not memento_store_has(V, C).

% 5.2.4 Signature invalid
fail_closed(verifier V, cid C, "invalid_signature") :-
    declaration(C, decl),
    not signature_valid(decl, V).

% 5.2.5 Signer key revoked at declaration time
fail_closed(verifier V, cid C, "signer_key_revoked") :-
    declaration(C, decl),
    key_revoked_at(V, decl.signer, decl.declaredAt).

% 5.2.6 Compiler incompatibility
fail_closed(verifier V, cid C, "compiler_incompatible") :-
    declaration(C, decl),
    active_compiler(V, A),
    not member(A, decl.compilers).

% 5.2.7 Dependency unresolvable (recursive)
fail_closed(verifier V, cid C, "dependency_unresolvable") :-
    declaration(C, decl),
    member(D, decl.depends_on),
    fail_closed(V, D, _).

% 5.2.8 Cyclic dependency
fail_closed(verifier V, cid C, "cyclic_dependency") :-
    declaration(C, decl),
    transitively_depends_on(decl, C).
```

### 5.3 Cross-catalog name collision (5.2.2)

Two different CIDs published under the same name in two distinct
catalogs is a load-bearing fail-closed condition. The protocol does
NOT auto-resolve collisions by precedence (e.g. "project catalog
wins"). The reasoning: two libraries silently disagreeing about what
a name means is exactly the failure mode propertyHash composition
exists to detect. Surfacing the collision forces the consumer to
disambiguate (alias, scope, choose one explicitly).

The same CID published under the same name in two catalogs is NOT a
collision; it's redundancy and resolves cleanly.

### 5.4 Compiler compatibility (5.2.6)

An extension whose `compilers` list is `["smt-lib"]` cannot be
resolved by a verifier whose active compiler is `lean`. The verifier
fails closed. This prevents an SMT-only extension from being silently
"resolved" through a Lean theorem-proving path that doesn't actually
support it.

The list is open: future compilers (`coq`, `isabelle`, `tla+`,
`bitwuzla-extensions`) appear here as the framework grows.

## 6. The IR's reference syntax for extensions

Extensions appear in IR formulas exactly as core names do. From the
IR formal grammar's perspective, no new syntax is introduced — the
grammar's `primitive-sort` rule already accepts arbitrary names, and
`atomic-predicate` and `ctor-name` already allow any string. The
extension protocol layers on top of that grammar to **resolve** the
names.

Example: a financial-arithmetic extension might publish a
`fixed-point-mul` ctor:

```json
{
  "kind": "ctor",
  "name": "fixed-point-mul",
  "args": [{ "kind": "var", "name": "_x0", "sort": {"kind": "primitive", "name": "FixedPoint8"} },
           { "kind": "var", "name": "_x1", "sort": {"kind": "primitive", "name": "FixedPoint8"} }],
  "sort": {"kind": "primitive", "name": "FixedPoint8"}
}
```

The IR JSON references `fixed-point-mul` and `FixedPoint8` by name.
The verifier resolves both through the catalog. If the extension
declarations for either are not in scope, signed correctly, or
compatible with the active compiler, the verifier fails closed.

## 7. Conformance criteria

A verifier conforms to this protocol iff it:

1. **MUST** maintain an extension catalog scope (one or more
   `extension-catalog` mementos active during verification).

2. **MUST** invoke the resolution algorithm (§5.1) for every name in
   an IR formula that is not in the bootstrapping core (§2).

3. **MUST** apply every fail-closed rule in §5.2; rejecting on the
   first match. No silent passthrough of unresolved names.

4. **MUST** verify Ed25519 signatures on every extension declaration
   it consults, per the signatures-and-non-repudiation spec.

5. **MUST** check signer-key revocation status as of the declaration's
   `declaredAt` timestamp.

6. **MUST** check compiler compatibility before resolving.

7. **MUST** resolve transitive dependencies before treating an
   extension as available; cyclic dependencies fail closed.

8. **MUST** surface cross-catalog name collisions as fail-closed
   conditions; not auto-resolve them.

9. **MUST** accept duplicate declarations of the same CID under the
   same name across catalogs as redundant, not collisional.

10. **SHOULD** refuse to resolve extensions whose only semantic
    declaration is `natural-language`, unless the verifier's policy
    explicitly allows it.

## 8. Worked examples

### 8.1 New sort: FixedPoint8 (8-bit fixed-point arithmetic)

Extension declaration memento body (CDDL-conformant JSON
representation):

```json
{
  "kind": "extension-declaration",
  "introduces": "sort",
  "name": "FixedPoint8",
  "signature": {
    "kind": "sort",
    "params": []
  },
  "semantics": [
    {
      "kind": "smt-lib-theory",
      "theory": "FixedSizeBitVectors",
      "version": "current"
    },
    {
      "kind": "axiom-set",
      "axioms": [
        ...   // forall x : FixedPoint8, exists y : Int, encoding(x) = y
              // ... fixed-point semantics axiomatized
      ]
    }
  ],
  "compilers": ["smt-lib"],
  "signer": "bafy...alice-pubkey-cid...",
  "signature": "...ed25519-bytes..."
}
```

A verifier resolves `FixedPoint8` by looking up the catalog entry,
fetching the declaration, verifying Alice's signature, checking the
SMT-LIB compiler is in scope, then admitting `FixedPoint8` as a usable
sort.

### 8.2 New predicate: is-prime over Int

```json
{
  "kind": "extension-declaration",
  "introduces": "predicate",
  "name": "is-prime",
  "signature": {
    "kind": "predicate",
    "argSorts": ["Int"]
  },
  "semantics": [
    {
      "kind": "axiom-set",
      "axioms": [
        // forall n : Int, is-prime(n) ↔ (n > 1 ∧ ∀ d : Int, (1 < d ∧ d < n) → ¬ divides(d, n))
        ...
      ]
    }
  ],
  "compilers": ["smt-lib", "lean4"],
  "signer": "bafy...bob-pubkey-cid...",
  "signature": "..."
}
```

The Lean compiler resolves this through Mathlib's `Nat.Prime`; the
SMT-LIB compiler resolves through the axiom-set form. Both
compatible; the extension is genuinely cross-paradigm.

### 8.3 Language-local extension: Rust borrow-checker claim

Some extensions are language-local. A Rust kit might publish
`rust-borrow-immutable-during-call` — a predicate over Rust function
references that asserts no &mut borrow exists during a function call.
This is meaningful only in the Rust kit's compilation context.

```json
{
  "kind": "extension-declaration",
  "introduces": "predicate",
  "name": "rust-borrow-immutable-during-call",
  "signature": {
    "kind": "predicate",
    "argSorts": ["bafy...rust-FnRef-sort-cid..."]
  },
  "semantics": [
    {
      "kind": "natural-language",
      "text": "Asserts that no &mut borrow of the function's referent exists during the function's execution. Verified by the Rust kit's borrow-checker integration; not directly translatable to SMT."
    }
  ],
  "compilers": ["rust-kit-borrow-checker"],
  "signer": "...",
  "signature": "..."
}
```

A TS consumer that imports the Rust library and references this
property via a bridge memento DOES NOT load Rust. The TS consumer's
verifier:

1. Sees the bridge's targetContractCid.
2. Fetches the property memento at that CID.
3. Verifies its signature.
4. Checks the verdict mementos that say the property holds (these
   were minted by the Rust toolchain that DID resolve the extension).
5. The TS consumer's verifier does NOT need to resolve
   `rust-borrow-immutable-during-call` itself; it only needs to verify
   the verdict that the property holds. The verdict is signed by a
   verifier the TS consumer trusts.

This is the language-local payoff: domain-specific extensions stay
domain-specific. Cross-language composition happens at the
propertyHash level, not the extension level. The Rust toolchain solves
Rust problems; TypeScript consumers verify the propertyHash chain
without loading any Rust code.

## 9. Versioning

This document is version 1 of the IR extension protocol. Future
versions live in separate spec docs and add a `protocolVersion` field
to the extension-declaration memento. v1 declarations have an implicit
`protocolVersion: 1`. Verifiers MUST refuse to resolve extensions
whose `protocolVersion` they don't support.

## 10. The architectural commitment, restated

The IR is total via this protocol. Every meaningful invariant about
code can be expressed in the IR — the bootstrapping core covers
universal first-order logic with sorts; every domain-specific concept
becomes an extension declaration. New sorts, predicates, and term
constructors are added by publishing extension declarations, not by
forking the framework.

Conversely: the framework MUST refuse to operate on names it cannot
resolve through this protocol. Soft acceptance of unknown names —
"we'll just trust whatever's there" — breaks content addressing,
breaks signatures, breaks the entire chain validity gate. The
fail-closed rules in §5.2 are non-negotiable.

If a domain cannot be expressed within bootstrapping core + extension
declarations, that's a sign the domain isn't a code-level invariant
in the sense Sugar cares about (probabilistic correctness, certain
termination claims that need well-founded relations beyond first
order). Those cases are explicitly out of scope by design; not gaps
in the protocol.

The protocol's promise is total within its scope. Within scope, the
answer to "we need a new sort" is always "publish an extension
declaration" — never "the IR is too restrictive."
