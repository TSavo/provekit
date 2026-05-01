# Stage 4 cross-language handshake demo

This is the demo that puts numbers on the trojan-horse pitch:
**most cross-language verification cost goes away**, because most
publisher / consumer pairs that meet at a call site already agree on
shape; the residue runs Z3 once, and everyone after that hits a
content-addressed cache.

Three players, three languages, one protocol:

- `validate-kit` (Go): publishes a `post` formula for `validateInput`.
- `parse-kit` (Rust): publishes a `pre` formula for `parseInt`.
- A Rust consumer: writes `parseInt(validateInput(s))` and asks the
  Rust verifier to check the call site.

Nothing crosses the language boundary except the protocol bytes:
deterministic-CBOR `.proof` files containing JCS-canonical mementos,
each signed BLAKE3-512 + ed25519. The Go publisher hashes its
formulas exactly the same way the Rust verifier does.

## Run it

```bash
bash examples/stage4-handshake-demo/run.sh
```

Requirements: `cargo`, `go`, `z3` on PATH. Override the z3 binary
with `PROVEKIT_Z3=/path/to/z3` if needed.

## Four scripted runs

Each run exercises one tier of the verifier's discharge ladder.

### Run A: Tier 1, hash equality

Both kits ship `forall n: Int. n > 0` for `validateInput.post` and
`parseInt.pre`. The protocol-level `propertyHash` (BLAKE3-512 over the
JCS-canonical formula bytes) of the post equals the propertyHash of
the pre. The verifier discharges the call site by hash equality.
**Zero Z3 invocations.** This is the headline number.

### Run B: Tier 1 miss → Tier 3 mints → Tier 2 hits on warm replay

Go switches `validateInput.post` to `forall n: Int. n >= 1`. The
formula is logically equivalent to `> 0` over `Int`, but the bytes
differ, so the propertyHash differs. Tier 1 misses. With the cache
empty, Tier 2 also misses. The verifier falls through to Tier 3,
which builds the implication obligation
`forall x: Int. (x >= 1) -> (x > 0)`, hands it to Z3, gets `unsat`
(the implication holds), and **mints a signed implication memento**
into `.provekit/cache/`. The memento's `propertyHash` is
`BLAKE3("implication:<post_hash>:<pre_hash>")`.

The script then re-runs the same scenario with the cache warm. Tier
1 still misses; Tier 2 finds the memento by content-derived key,
verifies its ed25519 signature, and discharges. **Zero Z3
invocations** on the warm replay.

### Run C: cache "invalidation" via content addressing

Go switches its post to `forall n: Int. n >= 0`. The post's
propertyHash changes. The cache key the verifier computes for this
new pair is `BLAKE3("implication:<NEW post_hash>:<pre_hash>")`, which
matches no file in `.provekit/cache/`. The cached memento from Run B
indexed under the *old* post_hash is not invalidated; it is simply
not addressed by the new key. Tier 2 misses without any cache
maintenance event ever firing.

Tier 3 fires. Z3 reports `sat`: the implication
`(n >= 0) -> (n > 0)` is falsifiable (`n = 0`). The call site is
**flagged as a violation**. The launch-quality demonstration: when
the publisher's contract weakens, the consumer sees the regression
the next time the verifier runs.

### Run D: re-cache after fix

Go restores `validateInput.post = forall n. n >= 1`. The
propertyHash on the post returns to its Run B value. The cache key
the verifier computes once again addresses the implication memento
that was minted in Run B — still on disk, never deleted. Tier 2
hits. **Zero Z3 invocations.** No "re-validation" step is needed;
the content-addressed cache restored itself the moment the input
shape did.

## What the demo proves

1. **Cross-language by bytes, not by adapters.** A Go publisher and
   a Rust consumer both compute the same propertyHash for the same
   formula because they share the protocol's canonicalization rules
   (JCS + BLAKE3-512). No language-specific bridge code appears
   anywhere in the verifier's discharge logic.

2. **Hash equality is most of the work.** Run A: zero Z3 work,
   guaranteed correct.

3. **The implication memento IS the cache invalidation primitive.**
   Run C demonstrates that when a publisher's contract drifts, the
   cache stops being addressable for the new pair. The memento is
   neither evicted nor re-validated; it simply doesn't apply.

4. **Z3 does work once per (post, pre) pair, project-wide.** Run B
   cold mints a memento. Every consumer that ever pairs the same
   `(post, pre)` will get a Tier 2 hit and skip Z3 forever — until
   the bytes change.

## Headline metrics

```
format: hash=M cache=K vacuous=W z3+mint=L residue=J violations=V z3_invocations=Z

Run A (hash equality):  hash=1 cache=0 vacuous=1 solved_minted=0 residue=0 violations=0 z3_invocations=0
Run B (cold, mint):     hash=0 cache=0 vacuous=1 solved_minted=1 residue=0 violations=0 z3_invocations=1
Run B (warm, cached):   hash=0 cache=1 vacuous=1 solved_minted=0 residue=0 violations=0 z3_invocations=0
Run C (violation):      hash=0 cache=0 vacuous=1 solved_minted=0 residue=1 violations=1 z3_invocations=1
Run D (re-cached):      hash=0 cache=1 vacuous=1 solved_minted=0 residue=0 violations=0 z3_invocations=0
```

The `hash` and `cache` columns count *real handshake* discharge
events: a publisher post and a consumer pre were paired, and either
their canonical hashes matched (Tier 1) or a signed implication
memento covering the pair was on disk (Tier 2). `vacuous` separately
counts call sites whose bridged target had only a `post` slot —
those are vacuously discharged, but they don't represent real
handshake work, so they get their own counter.

The headline pitch reads off Z3 invocations: **0 / 1 / 0 / 1 / 0**.
A from-scratch verification of Runs A through D would have spawned
five Z3 processes per call site; the handshake reduces it to two
across the full sequence.

## Files

```
examples/stage4-handshake-demo/
├── README.md                this file
├── run.sh                   orchestrates all four runs
├── stage4_driver.rs         per-run Rust driver (added as an
│                            example to provekit-verifier)
└── go-validate-kit/
    ├── go.mod               module replace pointing at the in-tree
    │                        Go ir-symbolic kit
    └── main.go              Go publisher; --shape gt0 | gte1 | gte0
```

The artifacts each run produces (one Go-published .proof, two
Rust-published .proofs, one signed implication memento) live under
`$TMPDIR/provekit-stage4-<timestamp>/`. Set `STAGE4_KEEP=1` to keep
them on disk for inspection.

## Verifier internals

The Tier 1/2/3 ladder lives in
`implementations/rust/provekit-verifier/src/runner.rs::work_one`. The
content-addressed cache scanner is
`implementations/rust/provekit-verifier/src/handshake.rs`. Both run
unchanged for any consumer of `provekit-verifier::Runner` that
populates `RunnerConfig::cache_dir`, `mint_seed`, and
`mint_producer_id`.

The minted implication memento is a v1.1.0 ClaimEnvelope with
`evidence.kind = "implication"`. It carries the producer's pubkey
inline so any Tier-2 reader can verify the signature without an
external key store. The memento's `bindingHash` and `propertyHash`
are derived per
`protocol/specs/2026-04-30-memento-envelope-grammar.md`. Any other
ProvekIt verifier (Go, C++, TypeScript) that loads the same .proof
catalog will see the same memento and verify the same signature.
