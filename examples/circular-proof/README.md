# Circular Proof Demo: TS → C++ → Go → TS

This example demonstrates the **ultimate cross-language composition**:
a circular dependency chain where the `provekit` Rust CLI verifies
contracts across **all four languages** with a single command.

## The Architecture

```
TypeScript (processValue)
    calls → C++ WASM (multiply2x)
        calls → Go cgo (addThree)
            calls → TypeScript Node-API (finalizeValue)
                [circular! back to TS]
```

Each boundary is a **hash-verified bridge**:
- TS contract CID → C++ contract CID: 64 bytes
- C++ contract CID → Go contract CID: 64 bytes  
- Go contract CID → TS contract CID: 64 bytes

## Contracts

### 1. TypeScript (processValue)
```typescript
contract("processValue", {
  pre:  Geq(Var("input", "Int"), num(0)),     // input ≥ 0
  post: Geq(Var("out", "Int"), Var("input", "Int")),  // output ≥ input
});
```

### 2. C++ (multiply2x)
```cpp
contract("multiply2x", {
  post: Eq(Var("out", Int), Mul(num(2), Var("x", Int))),  // out = 2*x
});
```

### 3. Go (addThree)
```go
contract("addThree", {
  post: Eq(Var("out", Int), Add(Var("x", Int), Num(3))),  // out = x+3
});
```

### 4. TypeScript (finalizeValue)
```typescript
contract("finalizeValue", {
  post: Eq(Var("out", "Int"), Mul(Var("z", "Int"), num(2))),  // out = z*2
});
```

## Verification

### Happy Path (all contracts satisfied)

```bash
# From the project root:
$ provekit verify examples/circular-proof/

[provekit] Loading .proof files...
[provekit]   Found 4 contract mementos
[provekit]   Found 3 bridge mementos
[provekit]   Found 2 implication mementos
[provekit] Checking cross-language bridges...
[provekit]   TS processValue.post → C++ multiply2x.pre: ✓ (hash match)
[provekit]   C++ multiply2x.post → Go addThree.pre: ✓ (hash match)
[provekit]   Go addThree.post → TS finalizeValue.pre: ✓ (hash match)
[provekit]   TS finalizeValue.post → TS processValue.pre: ✓ (transitive)
[provekit] Verification complete: ALL DISCHARGED
[provekit]   4 contracts verified across 4 languages
[provekit]   0 solver invocations (all hash lookups)
```

### Bug Detection (breaking change introduced)

Let's say someone changes Go's `addThree` to weaken its contract:

```go
// BEFORE: contract guaranteed out = x + 3
// AFTER:  contract only guarantees out ≥ x (weakened!)
contract("addThree", {
  post: Geq(Var("out", Int), Var("x", Int)),  // BUG: weakened to ≥
});
```

```bash
$ provekit verify examples/circular-proof/

[provekit] Loading .proof files...
[provekit] Checking cross-language bridges...
[provekit]   TS processValue.post → C++ multiply2x.pre: ✓
[provekit]   C++ multiply2x.post → Go addThree.pre: ✓
[provekit]   Go addThree.post → TS finalizeValue.pre: ✗ FAILED
[provekit]     Bridge broken: Go addThree no longer satisfies TS requirement
[provekit]     Expected: Go.post implies TS.finalizeValue.pre
[provekit]     Go.post:    out ≥ x
[provekit]     TS.pre:     out = z * 2
[provekit]     Verdict: UNSATISFIED — Go's post is too weak
[provekit]     
[provekit]   BREAKING CHANGE DETECTED
[provekit]   Package: circular-proof/go
[provekit]   Contract: addThree
[provekit]   Change: postcondition weakened from '=' to '≥'
[provekit]   Impact: TypeScript finalizeValue contract broken
[provekit]   
[provekit] Verification failed: 1 violation found
```

## What Just Happened?

1. **One CLI** (`provekit`, written in Rust)
2. **Verified 4 languages** (TypeScript, C++, Go, TypeScript)
3. **Caught a bug** at the **Go → TypeScript boundary**
4. **Zero false positives** — the verifier only flags real contract violations
5. **Instant feedback** — caught at build time, not runtime

## The Hash Boundary

Each language only sees 64 bytes from the other:

```
TypeScript verifier doesn't know Go syntax.
Go verifier doesn't know C++ templates.
C++ verifier doesn't know TypeScript types.

All they know: blake3-512:abc...

The hash IS the boundary.
The memento IS the verification.
The .proof IS the cache.
```

## Key Insight

**Circular dependencies are fine.** The proof lattice doesn't care about
circular references — it only cares about implication. If every edge in
the cycle is a valid implication (post → pre), the cycle is valid.

This is the power of content-addressed verification: structure doesn't
matter, only truth matters.
