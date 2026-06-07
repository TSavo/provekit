# Sugar compared to runtime testing frameworks

Tests catch what proofs don't; proofs catch what tests don't. They are complementary, not competitive. Sugar does not replace tests. This doc walks the boundary.

## The fundamental difference

**Tests verify behavior on a finite point set: the inputs you wrote.**

```python
def test_parse_int_zero():
    assert parse_int("0") == 0

def test_parse_int_positive():
    assert parse_int("42") == 42
```

Two assertions. Two inputs. The function might still fail on input 43, or on input "0x10", or on input " 42 ". You'd need to test those cases too.

**Proofs verify properties over an input domain.**

```
forall s: String. parse_int(s) = Some n -> string_of_int(n) = s
```

One assertion. Holds for all inputs in the domain. Verified once; covers the entire input space.

Tests are concrete; proofs are universal. The domain of universal verification is exactly what hand-written tests can't cover.

## What tests catch that proofs don't

### 1. Real I/O, real concurrency, real environment

A function's specification might be perfect, but the function might fail when it actually runs:

- Real network calls timing out.
- Real disk writes failing.
- Real concurrent access races.
- Real OS state being unexpected.
- Real configuration being misread.
- Real timing dependencies.

Proofs are about behavior in the formal model. Tests are about behavior in the real world. The gap matters.

### 2. Integration

Two functions, each correctly implementing their contract, can compose to produce wrong behavior. Tests at the integration level catch this. Proofs at the function level can miss it.

### 3. Performance

A function that returns the correct value in time exponential in input size satisfies the behavioral contract. Tests measure actual runtime; proofs (typically) don't.

### 4. The unverified slice

Sugar's lattice covers the contracts the lift adapter could lift. Coverage is per-adapter and tier-graded. The residue (the slice not covered by any adapter) is unverified. Tests are the only coverage for this slice.

### 5. Regressions

If a maintainer changes code in a way that doesn't affect the proof's canonical IR, the proof still holds. But the change might still be a regression in some unverified property. Tests guard against this.

## What proofs catch that tests don't

### 1. The input you didn't write

Tests cover the inputs the test author thought of. Adversaries find inputs the test author didn't think of. The "wide open universe" of possible inputs is precisely what tests can't enumerate.

### 2. Cross-dependency interactions

A consumer's pre-condition needs to align with a dependency's post-condition. If the alignment is wrong, every call site is broken; tests at the consumer level might pass on common inputs and fail on adversarial inputs. The handshake at the proof level catches the misalignment statically.

### 3. The dependency closure

Your tests cover your code. The dependency's tests cover the dependency's code. There is no test that covers the interaction across the boundary: "did your code's expectations match the dependency's actual behavior?" Cross-language proofs do exactly that.

### 4. Cache amortization

Tests run every time. Proofs are minted once and cached. For dependencies, the proof is provided by the dependency author; consumers don't re-test what the dependency already verified. Tests don't have this property.

## Use both

A typical project ships:

- **Unit tests** for line-level coverage.
- **Integration tests** for cross-component coverage.
- **End-to-end tests** for real-world flows.
- **Property-based tests** (fast-check, hypothesis, proptest) for the universe coverage that hand-written tests can't reach.
- **A `.proof` file** for cross-domain behavioral contracts, dependency-graph-scale verification, supply-chain anchors.

Removing tests because "we have proofs now" is a mistake. Removing proofs because "we have tests" is a different mistake.

## The relationship to property-based testing

Property-based testing (Hypothesis, fast-check, proptest, QuickCheck) sits between conventional tests and proofs:

- Like tests, runs against actual inputs.
- Like proofs, expresses properties over input domains.
- Unlike proofs, only checks a sample of the domain (not every input).

Sugar's lift adapters for `proptest` and `hypothesis` (planned) lift the property-test specifications into canonical IR. The lifted contracts are the same shape as proof-level contracts; the property-test author's specification becomes a proof candidate.

This is a sweet spot: property tests are easier to write than proofs, and the lift adapter promotes them into the protocol's substrate. Whether the property is then re-verified by Z3 (or another backend) at Tier 3 (promoting the test into a proof) is the verifier's choice.

## Decision tree for testing strategy

```
Goal: confidence in correctness.
  ↓
Do you have time for both proofs and tests?
  Yes → Both. Tests for real-world coverage; proofs for universal coverage.
  No → Tests, then proofs as you can afford them.

Goal: dependency boundary correctness.
  ↓
Tests at your boundary aren't enough. The proof of dependency contracts comes from the dependency, not your tests. Use Sugar.

Goal: supply-chain verification.
  ↓
Tests don't help. Sugar's rank-3 pin (`contractCid`, `witnessCid`, `binaryCid` per [`multi-dimensional-pinning.md`](../../security/multi-dimensional-pinning.md)) plus signed contracts is the path.

Goal: cross-language behavioral guarantees.
  ↓
Tests in language A don't help language B. Sugar's bridges are the path.

Goal: small startup, ship fast, low budget.
  ↓
Tests, especially property-based tests. Sugar is value-add as the codebase scales.
```

## What a `.proof` can replace from a testing point of view

- **Some redundant testing of dependencies.** If a dependency ships a `.proof`, you don't need to test the dependency's behavior; you verify the proof.
- **Some property-based testing within the contracted scope.** Once a property is lifted to canonical IR and discharged at Tier 1+2+3, it's verified. Re-running property tests adds little (other than catching regressions in the canonicalization or lifting).
- **Some cross-language test setup.** If your TypeScript code calls a Rust binary's function, the contract makes the boundary explicit; testing it dynamically becomes optional.

What a `.proof` does not replace: real-world tests, performance tests, fuzz tests for the unverified slice, integration tests at higher abstraction levels.

## Where existing testing literature gets Sugar wrong

A common misreading: "Sugar is just verification, so it replaces tests." This misreads on two axes.

First, Sugar is not a verifier; it's a substrate over which verifications are published. The verifier (Z3, Coq, Kani, etc.) is a separate question.

Second, behavioral contracts at a function's boundary are specifically the kind of property tests would test if you wrote enough property tests. The lift adapter lifts property-style annotations; Z3 (or another backend) discharges them. The work tests would do is amortized into the lattice.

The right framing: Sugar formalizes and federates the contract-style work tests cover. Tests for non-contract-style work (timing, integration with real I/O, environmental dependencies) remain.

## Read next

- [coq-fstar-lean.md](coq-fstar-lean.md): interactive theorem provers.
- [kani-prusti-creusot.md](kani-prusti-creusot.md): Rust-specific provers.
- [`../boundaries.md`](../boundaries.md): what Sugar is NOT.
- [`../cold-start.md`](../cold-start.md): when proofs amortize and when they don't.
