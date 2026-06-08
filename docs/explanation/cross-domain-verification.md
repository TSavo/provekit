# Cross-domain verification

The bridge mechanism makes cross-domain transfer automatic. A proof about JavaScript's `parseInt` transfers to Rust's `str::parse` because both bridge to the same reference contract. The bridge is a hash-bounded claim: "contract A (CID X) implies contract B (CID Y)." The implication is verified once, cached forever, and every verifier in every language hits the cache.

This is the deepest claim of the protocol's polyglot story. This doc unpacks the mechanism in detail.

## The setup

Imagine three host languages (JavaScript, Rust, and Python), each with their own implementation of a function that parses integers. Each implementation has annotations:

```javascript
// JavaScript (using zod)
const ParseIntInput = z.string();
const ParseIntOutput = z.number().int();
```

```rust
// Rust (using contracts)
#[contracts::requires(s.len() > 0)]
#[contracts::ensures(ret => ret.is_ok() ==> ret.unwrap() >= -2147483648 && ret.unwrap() <= 2147483647)]
fn parse_int(s: &str) -> Result<i32, std::num::ParseIntError> { ... }
```

```python
# Python (using pydantic)
class ParseIntInput(BaseModel):
    s: str

class ParseIntOutput(BaseModel):
    n: int = Field(..., ge=-2147483648, le=2147483647)
```

Three different ecosystems, three different annotation libraries, three different syntaxes. Each implementation has been verified individually (perhaps by different tools: Z3 for the Rust contracts, a TypeScript checker for the zod schemas, pydantic's runtime validation paired with a property test).

## The reference contract

A canonical reference contract is published in [`reference-contracts/`](../reference-contracts/) (when populated):

```
ref-parseInt-v1
contract:
  forall s: String. forall n: Int.
    parseInt(s) = Some(n) ->
      string_of_int(n) ⊆ s
    and -2147483648 ≤ n ≤ 2147483647
```

This is the canonical contract: "if `parseInt` returns Some(n) for input string s, then n's string representation is a substring of s, and n is in the int32 range."

The CID of this canonical contract is, say, `blake3-512:bafy...ref-parseInt-v1`.

## The bridges

Each implementation publishes a bridge memento binding its language-native contract to the reference:

### JavaScript bridge

```json
{
  "kind": "bridge",
  "sourceContractCid": "blake3-512:bafy...zod-parseInt-v1",
  "targetContractCid": "blake3-512:bafy...ref-parseInt-v1",
  "targetProofCid": "blake3-512:bafy...reference-contracts-bundle-v1",
  "implicationProof": <evidence: Z3 unsat core showing zod-parseInt-v1 → ref-parseInt-v1>,
  "boundCallSiteSymbol": "lodash.parseInt",
  "boundCallSiteSorts": ["String"],
  "boundReturnSort": "Int",
  "metadata": { ... }
}
```

The JavaScript implementation's lift adapter recognized `z.string()` → `z.number().int()`, produced canonical IR for that, and published a bridge: "this implementation's contract implies the reference contract."

### Rust bridge

```json
{
  "kind": "bridge",
  "sourceContractCid": "blake3-512:bafy...rust-parseInt-v1",
  "targetContractCid": "blake3-512:bafy...ref-parseInt-v1",
  "targetProofCid": "blake3-512:bafy...reference-contracts-bundle-v1",
  "implicationProof": <evidence: Z3 unsat core showing rust-parseInt-v1 → ref-parseInt-v1>,
  "boundCallSiteSymbol": "std::str::parse::<i32>",
  "boundCallSiteSorts": ["String"],
  "boundReturnSort": "Int",
  "metadata": { ... }
}
```

Same shape. Different source contract (the Rust implementation's lifted canonical IR). Same target contract (the reference).

### Python bridge

```json
{
  "kind": "bridge",
  "sourceContractCid": "blake3-512:bafy...pydantic-parseInt-v1",
  "targetContractCid": "blake3-512:bafy...ref-parseInt-v1",
  "targetProofCid": "blake3-512:bafy...reference-contracts-bundle-v1",
  "implicationProof": <evidence: Z3 unsat core showing pydantic-parseInt-v1 → ref-parseInt-v1>,
  "boundCallSiteSymbol": "int",
  "boundCallSiteSorts": ["String"],
  "boundReturnSort": "Int",
  "metadata": { ... }
}
```

Same target. Three bridges, all converging on `ref-parseInt-v1`.

## Cross-language transfer

A consumer in JavaScript imports a Python ML library that calls `int(input_string)`. The Python library has shipped a `.proof` containing the Python bridge above.

The JavaScript consumer's verifier sees the call site (Python's `int(...)`) and does the handshake:

1. **Tier 1**: does the JavaScript consumer's pre-condition (whatever it is, at the call site) match the canonical IR for `ref-parseInt-v1`? If yes, hash equality discharges without theorem proving.
2. **Tier 2**: if no exact hash match, is there a cached implication memento for `(consumer-pre, ref-parseInt-v1)`? If yes, signature check discharges.
3. **Tier 3**: solver fallback. Z3 invoked once per genuinely-novel pair.

Most consumers' pre-conditions for `parseInt`-style calls match `ref-parseInt-v1` exactly (it's the canonical reference; they were authored to match). Tier 1 fires. The handshake completes with no solver invocation, no signature check, no language-specific work.

The protocol walks: consumer's pre-condition → ref-parseInt-v1 ← Python's source contract. The Python bridge's evidence (showing Python's contract implies the reference) was minted once, by the Python kit author, and is cached forever. The JavaScript consumer doesn't re-derive it.

The JavaScript consumer's Tier 1 identity check is just CID equality. The Python
implementation's bridge to `ref-parseInt-v1`, verified once by the Python kit
author and admitted by local policy, can now be reused by JavaScript consumers
without re-deriving the Python proof.

## Why this works

Three properties combine:

### 1. Content-addressing

The CIDs are deterministic. JavaScript's `bafy...zod-parseInt-v1` is the same byte sequence regardless of which kit emitted it; `bafy...ref-parseInt-v1` is the same regardless of who looks at it. Consumer and producer can compare CIDs without ambiguity.

### 2. Bridge composition

The handshake walks the bridge DAG. From a consumer's pre-condition, find the bridge whose `targetContractCid` matches; check the evidence; cache the result. From there, find the source contract; in the consumer's case, that's a Python implementation contract.

The DAG walk is just a graph traversal over CIDs. Each step is hash-bounded.

### 3. Reference contracts

Without reference contracts, two implementations of `parseInt` in two different languages have nothing in common. Each has its own contract; neither bridges to anything. The cross-language story is impossible.

With reference contracts, both implementations bridge to the same target. The reference contract is the lingua franca. The bridge is the connection.

This is why reference contracts are first-class in the protocol's design. They are not an afterthought; they are the load-bearing element.

## Without bridges

If you have two implementations of `parseInt` and no shared reference contract:

- Each contract is verified separately by its language-native verifier.
- Cross-language consumers cannot inherit the verification.
- Each consumer must re-verify independently.

This is the world without Sugar's bridge mechanism. It is the world most polyglot codebases live in today: every team verifies independently, no transfer.

## What gets verified at Tier 1 vs. Tier 2

When two implementations bridge to the same reference contract, the implications are pre-computed:

- Implementation A → reference (verified once, cached, signed).
- Implementation B → reference (verified once, cached, signed).

A consumer of B with a pre-condition that matches the reference: Tier 1 (hash equality between consumer's pre and reference). Free.

A consumer of B with a pre-condition that doesn't quite match the reference but does match a previous implication: Tier 2 (cached implication, signature check). Sub-millisecond.

A consumer of B with a pre-condition no one's seen before: Tier 3 (Z3 derives the implication). Slow first time; cached after.

The fraction of call sites that hit Tier 1 is the headline metric. Reference contracts are pre-populated lattice points; consumers tend to write pre-conditions matching them (because they're canonical); Tier 1 fires often.

## What this changes about cold start

[Cold-start](cold-start.md) discusses the bootstrap problem: how does a fresh adopter get to a high Tier 1 fraction?

Reference contracts are the answer. A pre-curated set of canonical bridge anchors covers the most common call sites:

- `ref-parseInt-v1`, `ref-parseFloat-v1` for numeric parsing.
- `ref-email-format-v1` for email validation.
- `ref-uuid-v1` for UUID validation.
- `ref-ip-address-v1` for IP validation.
- `ref-iso8601-date-v1` for date parsing.
- ... and so on.

For each, multiple implementations across languages bridge in. Consumers' pre-conditions tend to match these references. Tier 1 fires. Even brand-new codebases with no project-specific lattice see immediate Tier 1 discharge for canonical call sites.

The reference-contracts library is the bootstrap accelerant. See [`../reference-contracts/`](../reference-contracts/) (when populated).

## Limitations

Cross-domain verification is not free for everything:

### 1. Adapter mis-translation

If the JavaScript adapter or the Rust adapter mis-translates an annotation, the source contract's canonical IR doesn't reflect the implementation's actual behavior. The bridge says "this contract implies the reference"; the contract is wrong; the bridge is wrong.

See [`../security/adapter-trust.md`](../security/adapter-trust.md). Cross-adapter parity tests catch many mis-translations; they don't catch all.

### 2. Reference contract drift

If the reference contract is wrong (its canonical IR doesn't match what the canonical reference actually says), every implementation that bridges to it inherits the wrongness. Reference contracts are themselves curated; their canonical IR must be reviewed.

The reference-contracts library has its own governance; see [`../reference-contracts/README.md`](../reference-contracts/README.md) (when written).

### 3. Implementation deviates from contract

If a Rust implementation's actual behavior differs from its claimed contract, the bridge claims "this contract implies the reference"; the contract describes wrong behavior; the bridge is signed but the implementation doesn't satisfy what the contract claims.

This is the "lying contracts" non-catch from [`../security/threat-model.md`](../security/threat-model.md). The signature attests to the signer; not to truth.

### 4. The reference contract doesn't capture everything

If `ref-parseInt-v1` captures int32 range but an implementation has additional concerns (locale-specific digits, leading whitespace, base prefixes), those concerns are outside the reference. Bridges that go through the reference don't carry the additional concerns. Consumers who need the additional concerns must verify them separately.

The reference contract is the canonical anchor for a *specific* slice of behavior. Other slices need their own references or per-implementation contracts.

## What this enables

When working as designed:

- A polyglot codebase shares verification across languages.
- New consumers benefit from existing implementations' verification work.
- Reference contracts are seed material; the lattice starts populated.
- The discharge fraction approaches the asymptote within months of adoption.
- Every bridge added by one party is reusable by every future party.

The combined effect is exactly what "compose across the dependency graph" means: behavioral verification flows across language boundaries automatically, mediated by content-addressed bridges to canonical references.

## Read next

- [content-addressing-not-registry.md](content-addressing-not-registry.md): the primitive cross-domain transfer is built on.
- [monotonic-provability.md](monotonic-provability.md): why bridges remain valid forever.
- [`../../examples/`](../../examples/): the worked demo.
- [`../reference-contracts/README.md`](../reference-contracts/README.md) (when written): the curated bridge anchors.
- [thesis.md](thesis.md): the full claim.
