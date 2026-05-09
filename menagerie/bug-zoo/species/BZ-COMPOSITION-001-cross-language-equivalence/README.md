# BZ-COMPOSITION-001: Cross-Language Equivalence

## Specimen

This species captures the load-bearing federation guarantee of CCP §7: a Rust
chain composing to a ComposedFunctionContract CID `X` and a structurally
equivalent C chain composing to the same CID `X` MUST produce byte-identical
composed CIDs.

The chain under test is:

```
vec_double_then_filter_positive_then_sum(input: &[i32]) -> i32
```

decomposed into three pure single-line helpers:

1. `double(x)`           returns `x * 2`
2. `keep_positive(x)`    returns `true` iff `x > 0`
3. `sum(xs)`             returns the integer sum of `xs`

Both `lab/rust/` and `lab/c/` express the same algebra with the same arithmetic
and the same per-helper pre/post comments. The two sources are line-for-line
correspondences, so any divergence in the composed CID is attributable to a
divergence in the lifters or in the canonical compose primitive, not to a
divergence in the reference algebra.

## Federation property

CCP §7 states the property:

> A test runner that lifts both sources via their respective lifters, composes
> via the canonical compose primitive, and asserts that the resulting
> ComposedFunctionContract CIDs are byte-identical.

If the runner passes, federation across the C and Rust lifters is empirically
confirmed for this chain shape. If it fails, the divergence is the precise
feature request: which side is producing different bytes, and where in the
compose pipeline (lift, canonicalization, effects merge, body hash) the
divergence enters.

This specimen is the empirical guarantee that paper 07 §6's structural
claim holds under the actual implementation. Future lifters (Java, Go,
TypeScript, Python, etc.) extend the specimen with structurally equivalent
source and assert against the same composed CID.

## Assertion

The runner prints both ComposedFunctionContract CIDs and a single equality
verdict line:

```
rust composed cid: blake3-512:...
c    composed cid: blake3-512:...
verdict: EQUAL | DIVERGENT | PENDING
```

`PENDING` is emitted when an upstream binding is not yet wired (see
"Runner status" below). It is not a pass and it is not a fail; it is the
absence of an answer.

## Runner status

The runner depends on:

1. The Rust compose path (CCP §6.1, direct libprovekit linking via the
   provekit-walk Rust lifter and `compose_chain_contracts`).
2. The C compose path (CCP §6.2 C ABI FFI, or §6.3 JSON-RPC subprocess)
   exposing composed contracts to the C lifter family
   (provekit-lift-c-kernel-doc, provekit-lift-c-sparse,
   provekit-lift-c-assertions).

If the C side bindings have not landed yet, the runner stubs the C path and
prints:

```
PENDING: C ABI FFI not yet wired
```

This is the accepted v0 state per the task brief. The specimen is scaffolded
so the assertion can be filled in as soon as the C lifter learns to emit
composed contracts.

## Layout

```
BZ-COMPOSITION-001-cross-language-equivalence/
  README.md          this file
  lab/
    rust/
      Cargo.toml
      src/lib.rs     three pure helpers + chain wrapper
    c/
      chain.c        three pure helpers + chain wrapper
      chain.h        public declarations
  runner.sh          lifts both, composes, asserts CID equality
```

## What this specimen is NOT

It is not a test of the chain's runtime correctness. The runtime semantics of
`vec_double_then_filter_positive_then_sum` are trivial and identical in both
languages by construction. The specimen tests whether the lifters and the
canonical compose primitive agree on the contract bytes.

It is not a test of any one lifter in isolation. A single-language compose
test belongs in that lifter's own test corpus. This specimen exists precisely
to detect cross-language divergence.

It is not a test of effect tracking completeness. All three helpers are pure;
their effect sets are empty by construction. CCP §8.5's "lifter
effects-tracking gap" failure mode is out of scope for this specimen.
