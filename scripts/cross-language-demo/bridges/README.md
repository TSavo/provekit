# Layered bridges demo: the DAG forms via bridge mementos

> The bridge is how the DAG forms. Encounter a bridge hash: done verifying.

## What this demonstrates

The TS-kit's `parseInt.invariant.ts` doesn't have to redefine parseInt's
contract. It can BRIDGE from the TS surface symbol to V8's published
contract. V8's contract bridges to ECMA-262. ECMA-262 bridges to IEEE
754. IEEE 754 bridges to hardware FPU verification artifacts.

Each bridge is a small content-addressed memento. Each declares "this
surface is the realization of that deeper contract." The bridges are
the EDGES of the DAG; they're how the layers compose.

```
Layer 1 (user code)   src/billing/invoice.ts:47 calls parseInt(...)
   ↓ bridges to
Layer 2 (TS-kit)      global.parseInt: the JS surface
   ↓ bridges to
Layer 3 (V8)          v8::Number::parseInt: the C++ implementation
   ↓ bridges to
Layer 4 (ECMA-262)    §7.1.4.1: the standardized definition
   ↓ bridges to
Layer 5 (IEEE 754)    §5.4: integer conversion semantics
   ↓ bridges to
Layer 6 (hardware)    Intel FPU verified per IEEE 754
```

Each layer's contract is published once by its canonical authority.
Each upper layer's bridge memento has `inputCids: [lower_layer.cid]`.
The DAG composes mechanically.

## Run it

```sh
npx tsx scripts/cross-language-demo/bridges/layered-bridges-demo.ts
```

Output: 6 JSON mementos at `scripts/output/layered-bridges/`. ~5KB
total. All signatures verify. Deterministic across runs.

## The bridge is the stopping point

When a verifier walks the DAG and encounters a bridge memento, it has
reached a content-addressed STOPPING POINT. The deeper layer's verdict
is the deeper layer's responsibility. The verifier can stop walking
at the bridge unless its policy requires deeper traversal.

**This is what makes the substrate scale at consumer-side
verification cost.** Most consumer verifications stop after 1-2
bridge layers: your code → your kit's bridge to V8. You never need
to re-walk ECMA-262, IEEE 754, and silicon for routine commit-gate
verification. Those layers are referenced by hash but not traversed.

A regulated bank's compliance audit might traverse all the way to
hardware (chain attested end-to-end). A hobbyist's commit gate stops
at the TS-kit bridge (trust V8). Same primitive, different walk depths.

## Why parseInt.invariant.ts changes shape

Before bridges: parseInt.invariant.ts had to define every property
about parseInt directly: ~17 properties, ~120 lines of TS-IR-language.

After bridges: parseInt.invariant.ts is a 3-line bridge:

```ts
import { property, bridge } from "provekit/ir";

property("parseIntBridgesV8",
  bridge("global.parseInt", "v8::parseInt@12.4")
);
```

The deep contract is V8's job. The TS-kit just bridges. The contract
WORK was done once by V8's maintainers; we compose by hash; consumers
inherit the chain.

**parseInt.invariant.ts as a 3-line file collapses 122 lines of
redundant work into a hash reference.** Same pattern for every
runtime built-in. The kit's catalog is mostly bridges: small,
referencing the canonical layers.

## What changed in the framework

Added evidence variant to `src/claimEnvelope/types.ts`:

```ts
export interface BridgeEvidence {
  kind: "bridge";
  schema: string;
  body: {
    sourceSymbol: string;       // "global.parseInt"
    sourceLayer: string;        // "TS-kit@1.0"
    targetContractCid: string;  // CID of the deeper-layer contract
    targetLayer: string;        // "V8@12.4 parseInt"
    notes?: string;
  };
}
```

Plus the schema CID in `src/claimEnvelope/variants/index.ts`:

```ts
"bridge": "0000000000000000c0000000000000c0",
```

Bridges compose into the DAG via `inputCids: [targetContractCid]`.
The verifier walks `inputCids` to traverse layers.

## What this enables

**Layered contracts model.** Each layer of the language stack
publishes its contract ONCE. Every layer above just references hashes.
Total verification work in the ecosystem approaches a constant as the
number of consumers grows: same hash composition primitive applied
to the LANGUAGE STACK.

**Cross-language convergence.** Python's `int(s)` bridges to CPython,
which references IEEE 754. JS's `parseInt` bridges to V8, which
references IEEE 754. Both layers ultimately ground at the same
hardware contracts. **Cross-language equivalence becomes mechanical
when the bridges all converge at canonical leaves.**

**Adversarial re-verification at any layer.** A consumer's proofkit
can re-verify ANY layer's contract (down to hardware) by walking the
bridges. Trust is hash-rooted, not signer-rooted; any consumer can
demand verification at any depth.

**The framework is the substrate; the bridges are how it composes.**
