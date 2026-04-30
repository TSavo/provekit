# ProvekIt: Handshake Algorithm

**Date:** 2026-04-30
**Status:** Specification. Companion to memento-envelope-grammar (contract + implication roles).

## What this document specifies

The verifier-side algorithm that resolves call sites against
contract mementos using a three-tier discharge: hash equality
(free), implication-memento cache (one solver query, cached), Z3
fallback (per-call-site).

The handshake is the cost model for verification at scale. Without
it, every call site costs one solver query. With it, most call
sites discharge for free, the residue runs Z3 once per (post,
pre) pair and publishes the result for everyone else, and only the
genuinely-novel pairs pay per-call-site cost.

This is the composition theorem of Hoare logic with content
addressing as the cache key. Formula equality is hash equality.
Implications are publishable mementos. The lattice of proven facts
amortizes across the entire ecosystem.

## The setup

A consumer's verifier loads a project root and walks every
`.proof` file under `<projectRoot>` and `<projectRoot>/node_modules/{*,@*/*}/`.
The walk produces a memento pool keyed by CID.

The verifier indexes the pool:

- **`bridges_by_symbol`**: bridge memento → indexed by `evidence.body.sourceSymbol`.
- **`contracts_by_cid`**: contract memento → indexed by `cid`.
- **`contracts_by_pre_hash`**: contract memento → indexed by
  `evidence.body.preHash` (when present); used to find consumer-
  shaped preconditions.
- **`contracts_by_post_hash`**: contract memento → indexed by
  `evidence.body.postHash` (when present); used to find publisher-
  shaped postconditions.
- **`implications_by_pair`**: implication memento → indexed by
  `(evidence.body.antecedentHash, evidence.body.consequentHash)`.

Indices are O(1)-lookup maps. Building them is one linear pass
over the memento pool.

## The handshake table

For each call site `g(f(x), ...)` the verifier discovers via
enumerate-callsites:

1. **Resolve f's contract** by walking the bridge for f's symbol →
   contract memento at `targetContractCid`.
2. **Resolve g's contract** the same way (the call site is inside
   g's body or g's invariants, so g's contract is the enclosing
   memento walked into).
3. The handshake question is: does `f.post` (publisher's guarantee)
   imply `g.pre` (consumer's requirement)?

The handshake is decided at **(post-formula, pre-formula)**
granularity, not (call site) granularity. One handshake decision
covers every call site sharing the same `(post-hash, pre-hash)`
pair.

## Three-tier discharge

```
function dischargeHandshake(postHash, preHash, postCid, preCid):
    # Tier 1: hash equality. Free.
    if postHash == preHash:
        return DISCHARGED_BY_HASH

    # Tier 2: implication-memento cache. One signature verification.
    impl = implications_by_pair.get((postHash, preHash))
    if impl is not None and verify_signature(impl):
        return DISCHARGED_BY_CACHE

    # Tier 3: Z3 fallback. One solver query per (post, pre) pair.
    smtScript = emit_implication_check(post_formula, pre_formula)
    z3_verdict = run_z3(smtScript)
    if z3_verdict == "unsat":
        # The implication holds. Mint and publish a new implication
        # memento; future verifiers (this one's later runs and other
        # parties downstream) will hit Tier 2 instead of Tier 3.
        mint_implication_memento(
            antecedentCid=postCid,
            consequentCid=preCid,
            antecedentHash=postHash,
            consequentHash=preHash,
            antecedentSlot="post",
            consequentSlot="pre",
            prover="z3@<version>",
            smtLibInput=smtScript,
        )
        return DISCHARGED_BY_SOLVER

    # Tier 3 failed. The handshake is not a free call.
    return REQUIRES_PER_CALLSITE
```

When the handshake returns `DISCHARGED_BY_*`, **every call site
sharing this (post, pre) pair is discharged**. The verifier marks
the pair as resolved and moves on.

When the handshake returns `REQUIRES_PER_CALLSITE`, the verifier
falls back to today's per-call-site discharge: substitute the
call's actual argument into the consumer's `pre`, ask Z3 for that
specific instance.

## Tier-1 example: hash equality

A common case in well-typed code: a library function ships
`post = forall s. length(out) > 0` and a consumer call site
expects `pre = forall s. length(s) > 0` for some downstream
function. After canonicalization, both formulas hash to the same
preHash/postHash. The handshake is decided in O(1) hash lookup;
no Z3 involvement.

## Tier-2 example: cached implication

A library ships `post = forall n. n > 0`. Consumer needs
`pre = forall n. n >= 0`. The hashes differ. But an implication
memento exists in some `.proof` file (possibly the library's own
`.proof`, possibly published by a third party): "Q implies P,
witnessed by Z3, signed by z3@4.13.4." The verifier checks the
memento's signature, accepts the implication, and discharges every
call site of the same shape for free.

## Tier-3 example: Z3 fallback + memento minting

First time a verifier in the ecosystem encounters a particular
(post, pre) pair with no cached implication: Z3 runs once. The
result is minted as an implication memento and signed. The
verifier writes the new memento to its own `.proof` output (or
to a local memento store, depending on configuration). Future
runs of the same verifier — and any other verifier that sees the
same `.proof` — hit Tier 2 instead.

## What gets published

A verifier that mints implication mementos as a side effect of
its work has three options for where they go:

1. **Local memento store only.** The cache survives across local
   runs but does not propagate. Lowest commitment; useful for
   private codebases where implication memento publication is not
   appropriate.
2. **Project's `.proof` file.** The implication mementos become
   part of the project's published catalog. Other consumers of the
   project's code benefit. The project's package author has signed
   off on shipping the implications.
3. **Public registry (implication server).** The implications are
   pushed to a registry that crawls and indexes them. The cache
   amortizes globally. Pushed implications stay signed by their
   prover; a registry is an indexer, not a re-signer.

Options 1, 2, and 3 are not exclusive. A verifier can write
locally + opt-in publish to the project + opt-in publish to
public registries.

## Substitution at the call site

When a handshake is `DISCHARGED_BY_*`, the call site's specific
argument is irrelevant — the implication holds universally over
the input domain. The verifier records "this call site is
discharged via handshake (post-hash, pre-hash)" and moves on.

When a handshake is `REQUIRES_PER_CALLSITE`, the verifier:

1. Reads the call's actual argument expression from the IR.
2. Substitutes the argument for the consumer's `pre` quantifier
   variable.
3. Asks Z3 whether the resulting concrete formula holds.

This is today's algorithm (the "instantiate at call site" stage
of the existing verifier). The handshake is layered on top: it
short-circuits the per-call-site path when it can.

## Reporting

The verifier's report includes a handshake breakdown:

```
total call sites:        N
discharged by hash:      M    (free; structural equality after canonicalization)
discharged by cache:     K    (Tier 2; cached implication memento)
discharged by solver:    L    (Tier 3; Z3 ran once, memento minted)
flagged per call site:   J    (residue; per-call-site Z3)
violations:              V    (Z3 returned sat; counterexample)
```

`M + K + L + J + V == N`. The `M / N` ratio is the **hash-discharge
fraction** — the fraction of call sites that cost zero solver work
because the publisher and consumer agreed on shape. This number is
the headline metric: a high `M / N` means the ecosystem's contracts
are composing well.

## Conformance criteria

A conforming verifier:

1. Builds the four indices over the memento pool at load time.
2. For each (post-hash, pre-hash) pair encountered at call sites,
   tries Tier 1, then Tier 2, then Tier 3, in order.
3. On Tier 3 success, mints an implication memento and writes it
   per its publish-policy configuration.
4. Falls back to per-call-site Z3 only when all three handshake
   tiers fail.
5. Reports the handshake breakdown in its output.

A verifier that skips the handshake and runs per-call-site Z3 for
every site is functionally correct but does not conform: it
ignores the cost-amortization the protocol enables, fails to mint
implications other verifiers would benefit from, and misses the
performance characteristic the protocol is designed around.

## The deeper claim

The handshake algorithm is the operational form of the protocol's
content-addressing claim. Hashes are verification barriers: if you
hold the matching hash, you've verified. Formula equality reduces
to hash equality. Implication is a memento. Composition is a
lattice over those mementos. A `.proof` file is a leaf in the
lattice; the implication server is a global view of it.

ProvekIt's promise — "your library's contracts compose with my
call sites at scale" — operationalizes through this algorithm. The
contract memento is the unit of specification; the implication
memento is the unit of cached reasoning; the handshake is how
they meet.
