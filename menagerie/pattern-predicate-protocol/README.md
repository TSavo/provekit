# Pattern Predicate Protocol — Reference Destination

Companion artifact set for the [Pattern Predicate Protocol
spec](../../protocol/specs/2026-05-09-pattern-predicate-protocol.md)
(PPP v0.1.0, dated 2026-05-09).

This destination is **exploratory tooling that demonstrates the spec end
to end on a real codebase**. It is not yet the v1 reference
implementation referenced in PPP Appendix B. The proper reference is a
Rust extension to `provekit-cli` exposing
`provekit pp compile|run|witness|receipt`; the scripts here are the
prototype that motivated the spec and the receipt the spec cites in
Section 8.

The receipt here is the actual experimental log of the work that
produced PPP: lifting `linux@7.1.0-rc2` through the C lifter, running
five SQL predicates against the substrate, triaging the results, and
observing that the cluster-derivation predicate caught CVE-2026-43500
plus a substrate-discovered same-class sibling
(`rxkad_verify_packet_2`) by structure alone.

## Layout

```
predicates/                  # SQL predicates, one per editorial pattern
  borrowed-pages-as-scratch.sql      v1, function-local
  borrowed-pages-as-scratch-v2.sql   v2, caller-aware (kills 3 of 5 FPs)
  copy_from_user-no-bounds.sql       userspace-bounds heuristic
  spin-lock-no-unlock.sql            local lock-pairing heuristic
  rcu-read-lock-no-unlock.sql        RCU read-side pairing
  kmalloc-no-free-locally.sql        local alloc/free pairing

tools/
  lift_kernel.py             walks a kernel subtree, runs the C lifter
                             over each .c file, ingests callEdges into
                             a SQLite substrate.db
  run_predicates.py          runs every .sql in predicates/ against a
                             substrate.db, prints predicate CID +
                             match count + first 25 rows per predicate

experiments/
  2026-05-09-linux-kernel-net.md     experimental record from the run
                                     that produced this destination
```

## Quickstart

```sh
# 1. Build the C lifter (from this repo's root).
make -C implementations/c/provekit-lift-c-kernel-doc

# 2. Get a kernel checkout you want to lift. Shallow + lazy blobs is fine.
git clone --depth 1 --filter=blob:none https://github.com/torvalds/linux.git /tmp/linux

# 3. Fetch the source files for the subtrees you care about.
git -C /tmp/linux checkout HEAD -- net/ipv4 net/ipv6 net/rxrpc net/core net/xfrm

# 4. Lift those subtrees into a substrate.
python3 tools/lift_kernel.py /tmp/linux \
  net/ipv4 net/ipv6 net/rxrpc net/core net/xfrm \
  /tmp/substrate.db

# 5. Run every predicate against the substrate.
python3 tools/run_predicates.py /tmp/substrate.db predicates/
```

## Caveats versus the spec

The Python tooling produces **exploratory output** that demonstrates the
shape of the spec's pipeline. It is intentionally not yet PPP-canonical:

- **Predicate CIDs.** The spec mandates BLAKE3-512 over canonical bytes.
  `run_predicates.py` uses Python's `hashlib.blake2b` because BLAKE3 is
  not in the standard library. The CIDs printed are therefore not
  comparable with `provekit hash` output. The proper reference impl
  must use BLAKE3.
- **Query application memento.** The script does not yet emit a signed
  `PpQueryApplication` memento. It prints a result set to stdout. The
  reference impl must serialize the query application as a `.proof`
  bundle with an Ed25519 signature.
- **Closure witness.** Computing a witness requires running the same
  predicate twice (pre-patch and post-patch substrate). The script
  supports doing each run separately; binding the two runs into a
  signed `PpClosureWitness` is a manual diff today and reference-impl
  work tomorrow.
- **Lifter CID.** The spec mandates `lifterCid` in every query
  application memento. The current scripts do not record it.
- **Substrate-schema version.** The script uses an ad-hoc CREATE TABLE
  that matches the v1 schema described in spec Section 3.1, but it
  does not commit to that schema as a versioned binding. The reference
  impl must.

These are non-architectural; the spec is what it is, and these scripts
are just enough to surface the load-bearing observation that the
predicate's substrate binding determines what patch shapes can witness
closure under it (PPP Section 8). The proper Rust reference impl is
named in PPP Appendix B and has not yet been written.

## Provenance of today's predicates

Each `.sql` in `predicates/` carries its editorial provenance in its
header comment. The borrowed-pages-as-scratch family compiles a
nefariousplan pattern of the same name; the lock / alloc / userspace
heuristics are local conventional checks rather than a particular
named pattern.

When the proper provekit-cli reference impl exists, each predicate
will be paired with a signed compilation memento naming
`{patternCid, predicateCid, schemaVersion, producer}`. Today, the
predicate's identity is the canonical-bytes hash of its `.sql`
contents.
