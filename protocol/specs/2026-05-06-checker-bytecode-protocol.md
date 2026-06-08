# Checker Bytecode Protocol (CBP)

**Status:** v0.1.0 draft extension protocol
**Date:** 2026-05-06
**Layer:** extension protocol over ORP and the ProofIR/memento substrate
**Related:**
- `2026-05-06-extension-protocols.md` - finite core, executable extension layer, DAG ordering
- `2026-05-06-obligation-realizer-protocol.md` - witnesser/dropper/monitor realizer modes
- `2026-05-06-truth-discharge-protocol.md` - unit truth over signed body-claims
- `2026-05-06-grammar-conformance-protocol.md` - checker body grammar and invariant conformance
- `2026-05-03-substrate-layers-envelope-header-body.md` - signed metadata/body layering
- `2026-04-30-ir-formal-grammar.md` - ProofIR formula grammar
- `2026-04-30-ir-compiler-protocol.md` - existing compiler protocol lineage

## Section 0. Purpose

CBP defines how ProofIR predicates and edges may compile to executable checker bytecode for boundary admissibility.

The actual bytecode carrier is the signed metadata/body field of a memento. A checker memento may inline the executable bytes directly in metadata/body or reference them by CID, but in both cases the bytecode is part of the signed/content-addressed letter that extension-aware tooling interprets over the memento DAG.

The checker bytecode target is not application bytecode. It is extension bytecode whose job is to answer:

```
given host bindings and a ProofIR obligation,
does the boundary satisfy the obligation under policy?
```

CBP is an extension protocol. Core verification MUST NOT execute checker bytecode. Core verification verifies the signed memento graph that carries checker bytes, interpreter identities, policies, and witness results. CBP-aware tooling may execute checker bytecode over that graph to produce optional witness mementos.

## Section 1. Non-execution rule

The central rule:

```
Core verification MUST NOT execute checker bytecode.
```

Checker execution is extension execution. It may be Turing complete, host-specific, sandboxed, fueled, interpreted, JIT-compiled, or delegated to a native runtime. None of that affects core memento validity.

If checker execution terminates and emits a valid signed/content-addressed witness, the witness can enter the substrate. If checker execution fails, refuses, exceeds fuel, or does not terminate, no witness is produced.

The absence of a checker result is not evidence that the predicate is false. It is absence of extension evidence.

## Section 2. Vocabulary

**Checker bytecode.** Executable bytes that evaluate a ProofIR predicate or edge against host bindings.

**Checker compiler.** A producer that lowers a ProofIR obligation to checker bytecode for a target runtime.

**Checker runtime.** An interpreter, VM, host function ABI, or native executable environment that runs checker bytecode.

**Checker ABI.** The binding convention between ProofIR variables and host values, source ranges, runtime observations, or symbolic facts.

**CheckerMemento.** A signed memento that identifies checker bytecode, compiler, target runtime, obligation CID, binding ABI, and input CIDs.

**CheckerWitness.** A witness memento emitted by running checker bytecode under a policy.

**CheckerRefusal.** A signed refusal result for unsupported predicates, unsupported bindings, malformed bytecode, policy rejection, timeout, or runtime failure.

## Section 3. Modes of use

The same checker bytecode may be used in three ORP modes.

### Section 3.1 Witnesser mode

The checker runs beside or around the host artifact and emits evidence.

```
checker bytecode + host bindings -> CheckerWitness | CheckerRefusal
```

Example: run a WASM checker over an HTTP request body to witness `1 <= amount <= 10000`.

### Section 3.2 Dropper payload mode

The checker is inserted or attached by a dropper as a native repair candidate. The dropper output is accepted only after re-lift proves closure.

```
missing edge -> insert checker at boundary -> re-lift -> closure witness
```

Example: insert a generated guard before a callsite requiring `not_null(x)`.

### Section 3.3 Monitor mode

The checker attaches to a runtime boundary and emits a stream of witnesses/refusals.

```
checker bytecode + attachment point -> witness stream
```

Example: eBPF checker observes syscall arguments and witnesses resource-state predicates.

Monitor mode is non-normative in v0.1. The wire shapes reserve room for it.

## Section 4. CheckerMemento shape

CBP uses signed metadata/body fields. The following shape is the draft body convention for a checker memento.

```json
{
  "kind": "CheckerMemento",
  "schemaVersion": "1",
  "obligationCid": "blake3-512:...",
  "obligationKind": "predicate",
  "checkerBytecodeCid": "blake3-512:...",
  "checkerBytecodeEncoding": "wasm32-wasi",
  "checkerBytecodeInlineBase64": null,
  "checkerCompilerCid": "blake3-512:...",
  "checkerRuntimeCid": "blake3-512:...",
  "bindingAbiCid": "blake3-512:...",
  "inputCids": ["blake3-512:..."],
  "metadata": {
    "sourceIrCid": "blake3-512:...",
    "compilerName": "sugar-checker-wasm",
    "compilerVersion": "0.1.0"
  }
}
```

Normative fields:

| Field | Meaning |
|---|---|
| `kind` | MUST be `"CheckerMemento"`. |
| `schemaVersion` | MUST be `"1"` for this draft. |
| `obligationCid` | Predicate, edge, or gap CID compiled by the checker. |
| `obligationKind` | `"predicate"`, `"edge"`, or `"gap"`. |
| `checkerBytecodeCid` | CID of the executable checker bytes. |
| `checkerBytecodeEncoding` | Runtime target, e.g. `"wasm32-wasi"`, `"evm-bytecode"`, `"ebpf"`, `"jvm"`, `"native-plugin"`, `"interpreted-proofir"`, `"sugar-checker-ir/0"`, or `"source+compile"`. |
| `checkerBytecodeInlineBase64` | Optional inline executable bytes. If absent/null, resolve `checkerBytecodeCid` from the content-addressed store. |
| `checkerCompilerCid` | CID of compiler/spec artifact used to produce bytecode. |
| `checkerRuntimeCid` | CID of accepted runtime/interpreter spec or implementation. |
| `bindingAbiCid` | CID of the ABI convention mapping host bindings to checker inputs. |
| `inputCids` | Prior artifacts the checker composes against. |
| `metadata` | Signed metadata/body; part of the memento CID. |

The checker bytecode itself may be embedded or referenced. If embedded, `checkerBytecodeInlineBase64` contains the exact executable bytes and the memento body MUST still include `checkerBytecodeCid` computed over those bytes. If referenced, `checkerBytecodeInlineBase64` is absent or null and `checkerBytecodeCid` resolves to the executable bytes.

### Section 4.1 Bytecode is metadata, not a side channel

Checker bytecode is extension metadata/body, not an out-of-band attachment.

The canonical identity rule:

```
checkerBytecodeCid = blake3-512(bytes(checker bytecode))
mementoCid = blake3-512(JCS(envelope/header/metadata including checkerBytecodeCid and any inline bytes))
```

When inline bytecode is present, changing a single byte changes the memento CID. When referenced bytecode is used, changing a single byte changes `checkerBytecodeCid`, which changes the referring memento bytes and therefore the referring memento CID.

This preserves the earlier doctrine: metadata is executable instruction material and part of the signed letter. Extension-aware tooling executes it; the core substrate hashes and verifies it.

### Section 4.2 Minimal instruction-stream form

CBP does not require one universal bytecode target, but it reserves a minimal native instruction-stream encoding for future portable checkers:

```json
{
  "encoding": "sugar-checker-ir/0",
  "instructions": [
    {"op": "load", "dst": "r0", "binding": "amount"},
    {"op": "const_i64", "dst": "r1", "value": 1},
    {"op": "ge_i64", "dst": "r2", "left": "r0", "right": "r1"},
    {"op": "const_i64", "dst": "r3", "value": 10000},
    {"op": "le_i64", "dst": "r4", "left": "r0", "right": "r3"},
    {"op": "and", "dst": "r5", "left": "r2", "right": "r4"},
    {"op": "return", "value": "r5"}
  ]
}
```

This JSON instruction stream, once JCS-canonicalized and UTF-8 encoded, is bytecode for CBP purposes. WASM, eBPF, JVM bytecode, and native plugins are alternate encodings of the same extension principle, not replacements for it.

### Section 4.3 Executable artifact classes

CBP does not privilege one executable form. A checker artifact MAY be:

- direct VM bytecode, e.g. WASM, EVM bytecode, JVM bytecode, or eBPF;
- Sugar-native checker instruction bytes;
- source code that requires a specified compilation DAG before execution;
- a host-native plugin identified by CID and policy;
- proof-carrying bytecode whose `.proof` artifact is embedded in metadata/body or referenced by CID;
- an interpreted extension DAG whose root CID is the executable artifact.

The common rule is identity, not format:

```
artifact bytes -> artifact CID
artifact dependencies -> input edges
artifact execution -> signed witness/refusal memento
```

If a checker artifact cannot be named by CID, cannot name its dependencies by CID, or cannot emit a signed/content-addressed result, it is not a CBP checker artifact.

### Section 4.4 Staged checker realization

Source-based checkers are modeled as staged DAGs.

Example:

```
sourceCid
  + compilerCid
  + compilerFlagsCid
  + dependencyDagRootCid
  + buildPolicyCid
    -> compiledCheckerBytecodeCid
    -> compilationWitnessCid

compiledCheckerBytecodeCid
  + checkerRuntimeCid
  + bindingAbiCid
  + obligationCid
    -> CheckerWitness | CheckerRefusal
```

The compilation witness is itself signed/content-addressed evidence. It may be embedded in metadata/body or referenced by CID, but in both cases it is part of the memento identity. A checker execution that depends on compilation MUST reference the accepted compilation witness or the accepted compilation DAG root.

Proof-carrying checker artifacts use the same pattern:

```
checkerBytecodeCid + checkerProofCid + proofCheckerCid + policyCid + obligationCid
  -> proofAcceptanceWitnessCid

proofAcceptanceWitnessCid + checkerBytecodeCid + bindings -> CheckerWitness
```

The `.proof` is not trusted because it exists. It is trusted only if an accepted proof checker, runtime, policy, and signer produce an accepted witness over it.

### Section 4.5 Parent reliance rule

Consumers MUST distinguish byte identity from accepted evidence.

For an EVM checker artifact:

```
evmBytecodeCid
proofCid
proofCheckerCid
policyCid
obligationCid
  -> proofAcceptanceWitnessCid
```

A parent claim that relies on the proved property SHOULD reference:

```
proofAcceptanceWitnessCid
```

It SHOULD NOT rely only on:

```
evmBytecodeCid
proofCid
```

The raw bytecode CID says which bytecode exists. The proof CID says which proof bytes exist. The proof-acceptance witness CID says that accepted machinery checked that proof against that bytecode, that obligation, and that policy.

The rule:

```
Reference the strongest already-witnessed root.
```

Inlining remains allowed for availability or packaging. Reliance should target the witnessed root.

### Section 4.6 Positive discharge is unit truth over body

CBP uses the Truth Discharge Protocol (TDP) rule for positive checker evidence.

A positive checker witness discharges exactly one proposition:

```
the claim described by this witness body holds under the cited policy
```

The body carries the proposition. For CBP, that body includes at minimum the obligation CID, checker memento CID, binding ABI CID, relevant artifact CIDs, input CIDs, execution policy, runtime identity, and signer identity. A proof-carrying checker body may additionally include or reference proof bytes, proof checker CIDs, compilation witnesses, and bytecode CIDs.

The result value is deliberately narrow:

```
result = "holds"
```

means only:

```
true(body-claim, policy)
```

It does not mean every referenced artifact is universally safe, globally correct, or semantically complete. It means the accepted checker evaluated the exact body-claim and produced a positive discharge. `"violated"` and `"refused"` are evidence records, but they do not discharge a positive obligation.

CBP checker memento bodies, binding ABI bodies, and proof-carrying checker body-claims SHOULD be witnessable under the Grammar Conformance Protocol (GCP). A policy MAY require a GCP witness before accepting a checker memento as a well-formed executable artifact.

## Section 5. Binding ABI

Checker bytecode cannot be meaningful without a binding ABI. The ABI defines:

1. how ProofIR variables map to host values or observations;
2. scalar encodings;
3. object/record traversal;
4. absent/null/error representation;
5. source-location binding, if static;
6. runtime-value binding, if dynamic;
7. symbolic-fact binding, if solver-backed;
8. refusal behavior for unsupported values.

Example binding ABI descriptor:

```json
{
  "kind": "CheckerBindingAbi",
  "schemaVersion": "1",
  "target": "wasm32-wasi",
  "bindings": [
    {
      "proofVar": "amount",
      "hostPath": "body.amount",
      "valueType": "i64",
      "onMissing": "refuse"
    }
  ]
}
```

Binding ABI bytes are content-addressed. Two checkers that use different binding conventions are different checker claims even if their predicate CID is the same.

## Section 6. Execution order

Checker execution follows the extension-protocol DAG ordering rule.

Before executing checker bytecode, a CBP-aware tool MUST:

1. resolve the `CheckerMemento`;
2. verify its signature and CID;
3. resolve and verify `checkerBytecodeCid`;
4. resolve and verify `checkerRuntimeCid`;
5. resolve and verify `bindingAbiCid`;
6. resolve all `inputCids`;
7. evaluate DAG dependencies before the checker that depends on them.

Independent checker executions MAY run in parallel. Deterministic sequential evaluation across independent checker nodes MUST use lexicographic CID order unless the CBP consumer policy specifies another deterministic order.

## Section 7. Execution policy

Checker execution requires policy. A policy MUST decide:

- accepted checker runtime CIDs;
- accepted checker compiler CIDs;
- accepted checker bytecode encodings;
- accepted signer keys;
- fuel/time/memory limits;
- filesystem/network permissions;
- host-observation permissions;
- whether runtime witnesses are acceptable for the obligation kind;
- whether refusal mementos are retained.

Default fail-closed behavior:

```
unsupported runtime -> refusal
unsupported opcode -> refusal
fuel exhausted -> refusal
timeout -> refusal
malformed binding -> refusal
policy denial -> refusal
```

## Section 8. CheckerWitness shape

```json
{
  "kind": "CheckerWitness",
  "schemaVersion": "1",
  "checkerMementoCid": "blake3-512:...",
  "obligationCid": "blake3-512:...",
  "bindingAbiCid": "blake3-512:...",
  "observedArtifactCids": ["blake3-512:..."],
  "result": "holds",
  "execution": {
    "runtimeCid": "blake3-512:...",
    "fuelUsed": 1842,
    "startedAt": "2026-05-06T00:00:00Z",
    "finishedAt": "2026-05-06T00:00:00Z"
  },
  "inputCids": ["blake3-512:..."]
}
```

`result` values:

| Value | Meaning |
|---|---|
| `"holds"` | Checker terminated and witnessed the obligation under policy. |
| `"violated"` | Checker terminated and produced a counterexample or rejection for the obligation. |
| `"refused"` | Checker could not evaluate under policy. |

Only `"holds"` may discharge a positive obligation. `"violated"` and `"refused"` may be useful adversarial or diagnostic evidence but do not discharge.

## Section 9. CheckerRefusal shape

```json
{
  "kind": "CheckerRefusal",
  "schemaVersion": "1",
  "checkerMementoCid": "blake3-512:...",
  "obligationCid": "blake3-512:...",
  "reasonCode": "FUEL_EXHAUSTED",
  "message": "checker exceeded policy fuel limit",
  "counterexampleCid": null,
  "inputCids": ["blake3-512:..."]
}
```

Refusals are content-addressable negative execution records. A refusal never proves the obligation false unless a separate extension protocol defines an accepted adversarial-witness rule for that predicate family.

## Section 10. Self-witnessing

CBP enables reflective self-witnessing.

Example:

```
CBP spec bytes                 -> spec CID
checker compiler bytes         -> compiler CID
checker runtime bytes          -> runtime CID
checker bytecode bytes         -> checker CID
implementation bytes           -> implementation CID
claim: implementation conforms to CBP under checker/runtime policy
  -> CheckerWitness memento
```

This is not circular proof. Core verification checks signed bytes, CIDs, and references. CBP-aware tooling executes the checker under policy. The result, if accepted, is another signed/content-addressed memento.

## Section 11. Security model

Checker bytecode is untrusted until policy accepts:

- the checker memento signature;
- the checker compiler CID;
- the checker runtime CID;
- the binding ABI CID;
- the bytecode encoding;
- the execution limits;
- the witness signer.

The checker runtime MUST be sandboxed for any execution that touches untrusted bytecode or untrusted host observations. A checker MUST NOT be allowed to mutate host artifacts unless it is being used as a dropper payload under ORP `transform` mode, and even then the transform result requires re-lift closure verification.

## Section 12. Non-goals

- Define one universal bytecode target.
- Require runtime checking for every ProofIR predicate.
- Execute checker bytecode during core verification.
- Treat checker output as trusted without a signed witness.
- Replace SMT solvers, proof assistants, or static lifters.
- Define monitor evidence streams in v0.1.

## Section 13. Open questions

1. Should `CheckerMemento` be a new core-recognized memento kind or remain an extension metadata/body convention?
2. Should WASM be the first reference target, or should the first target be interpreted ProofIR predicates?
3. Should checker compiler conformance be witnessed by ORP or by a separate compiler-conformance protocol?
4. Should `"violated"` witnesses share the adversarial-witness shape from witness-pluralism specs?
5. Should checker execution traces be content-addressed as separate artifacts?

## Section 14. Conformance

A CBP v0.1 implementation is conformant if it:

1. Emits `CheckerMemento` shapes with byte-stable JCS canonicalization.
2. Never requires core verification to execute checker bytecode.
3. Resolves and verifies checker bytecode, runtime, compiler, binding ABI, and input CIDs before execution.
4. Fails closed on unsupported bytecode, unsupported runtime, invalid bindings, timeout, or fuel exhaustion.
5. Emits signed/content-addressed `CheckerWitness` or `CheckerRefusal` outputs.
6. Treats `"holds"` as the only positive discharge result.
7. Applies DAG dependency ordering before checker execution.

## Section 15. Citation

Cite as:

> Sugar Protocol Working Notes (2026). *Checker Bytecode Protocol (CBP)*. Draft extension protocol v0.1.0.
