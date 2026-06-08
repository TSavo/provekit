# Applicative Encoding Ratification: McCarthy Desugar Is the Substrate Trajectory

Date: 2026-05-18
Status: Active. Implemented in libsugar/src/core/bind.rs via PR #1205.

## Ruling

The substrate's bind output uses applicative (McCarthy) encoding for operations, NOT nominal-direct encoding. This is the load-bearing shape going forward; all downstream consumers must read the substrate this way.

### Direct (pre-PR-#1205) shape

```
concept:add(x, y)
  =>
Term::Op {
  op_cid: <concept:add CID>,
  name: "concept:add",
  args: [Term, Term]
}
```

The operation name lived on `Term::Op.name`. Walkers extracted the operation identity via `node.op_name`.

### Applicative (post-PR-#1205) shape

```
concept:add(x, y)
  =>
Term::Op {
  op_cid: <concept:op-application CID>,
  name: "concept:op-application",
  args: [
    Term::Const { value: { "conceptName": "concept:add", "conceptCid": "<CID>", ... }, sort: ConceptCitation },
    Term, Term  // operands
  ]
}
```

The operation identity now lives inside a citation `Term::Const` as the FIRST arg of the op-application. The op_name field on the outer Term::Op is always `"concept:op-application"` (or `"concept:bind-result"` at the root).

## Why applicative

Four load-bearing reasons, in priority order:

1. **M+N hub federation literal.** Per the transport architecture (`project_sugar_transport_architecture` memory): cross-language transport via the `concept:*` hub. Applicative encoding makes `concept:op-application` THE hub. Every kit implements one primitive (apply-this-CID-to-args) instead of M op-shapes. Adding a new target-op never requires kit-side syntax additions; it just registers a CID. The transport becomes "lift to op-application form, transport CIDs, realize via apply-by-CID."

2. **Content-addressing alignment.** Sugar's substrate is content-addressed: `k(I)=t` is a claim about CIDs. `concept:add` is a NAME for human readability; the CID is what the substrate compares. Applicative encoding puts the CID where it's load-bearing (as a value), not where it's a label (as op_name). The substrate's syntax matches its semantics.

3. **Higher-order is on the roadmap.** The abstraction-layer impl chain (closure, dynamic-dispatch, exception, reference, iterator, generic) requires operations to be first-class values. Direct encoding would force special-case syntax for each higher-order primitive. Applicative gets them all uniformly because operations-as-values is the default.

4. **Per-application effect/contract tracking.** Stage 4 (#1147) introduced per-callsite divergence characterization with kit-declared dimension-value mementos as the effect alphabet. The natural next step is per-application-site effects attached to each op-application node. Direct encoding had no slot for this; applicative encoding does (the citation Term::Const).

## Costs (accepted)

- **Verbosity.** Trees roughly double in depth. CID computation has one more layer. Acceptable: CIDs are constant-size; nesting is shallow; serialization compresses.
- **Walk/query patterns rewire.** Every consumer that walked Term trees looking for `op_name.starts_with("concept:")` is now broken. Each consumer needs to walk into `Term::Const` args to recover the operation identity. This is one-time migration cost across consumers, paid once.
- **Human-readability.** Raw JSON now requires CID-to-name resolution to read. Tooling must provide the resolution. Acceptable cost given the tooling exists/can be built.

## Discipline

- ALL substrate consumers (lifters, realizers, transport, tests, FFI) MUST read the applicative shape when walking bind output. Direct-encoded readers are obsolete invariants.
- The McCarthy desugar lives at `named_tree_op_tree` in `libsugar/src/core/bind.rs`. Any new substrate transformation that emits Term trees MUST produce applicative shape from the start. Adding new direct-encoded emission paths is a regression.
- Concept-op identity recovery from a citation Term::Const is via `value.get("conceptName").and_then(Value::as_str)` for the name and `value.get("conceptCid").and_then(Value::as_str)` for the CID.
- The catalog fallback in `GrammarOpRegistry::resolved_name_and_cid` returns `("concept:op-application", op-application-CID)` for unknown concept names. This is the "uncharacterized operation" case. Consumers reading `name = "concept:op-application"` AND finding no underlying conceptName in the citation should treat this as a refused operation, not a successful unknown.

## Future work

#1207 tracks the audit: which pipeline layers (lift, realize, transport) still operate on direct-encoded trees and need to migrate. PR #1205 made the change at bind; the rest catches up.

## Cross-references

- PR #1205: implementation
- #1207: pipeline audit follow-up
- #1208: trinity_citation_comments_exhibit test re-spec (first downstream consumer surfaced as failing)
- [[2026-05-18-platform-semantics-binding-kit-compose-ruling]] (M+N hub framing)
- `project_sugar_transport_architecture` memory
- `project_sugar_three_key_composition` memory (k'' lift, k' transport, k abstraction)
