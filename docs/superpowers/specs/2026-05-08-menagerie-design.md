# Menagerie Design

## Purpose

Sugar needs a top-level home for executable proof-workflow exhibits. The
current `bug-zoo/` directory is valuable, but it accidentally suggests that bug
rediscovery is the whole project. The project is broader: `prove k(I)=t` applies
to any workflow where an artifact is projected into a canonical, signed,
content-addressed truth claim.

The Menagerie is that home. It is both product framing and executable repo
structure:

- framing: a map of the major proof workflows Sugar supports;
- harness: runnable exhibits with receipts, passing cases, and refusal cases;
- migration: `bug-zoo/` moves physically under `menagerie/bug-zoo/`.

Bug Zoo becomes one destination in the Menagerie, not a sibling of the idea it
is meant to demonstrate.

## Goals

1. Establish `menagerie/` as the repo-owned place for executable proof-workflow
   exhibits.
2. Move `bug-zoo/` to `menagerie/bug-zoo/` and update all code, docs, tests,
   Cargo paths, and self-contract references.
3. Add Menagerie-level documentation that explains the common shape:
   `artifact I -> projection k -> truth claim t -> signed CID graph`.
4. Define the first destination set:
   - Bug Zoo: bugs as missing edges and fixes as closure receipts.
   - Hashbound Mainline: cross-domain implication chains compressed to 64-byte
     verification.
   - Supply Chain Rails: rank-3 pins, `binaryCid`, and CICP attack refusal.
   - Bridgeworks: proof preserved across language, package, and domain bridges.
   - Protocol Switchyard: protocol versions as roots and migrations as
     witnessed edges.
   - Change Station: commits as `p -> q` proof-carrying transitions.
5. Make Hashbound Mainline a first-class exhibit with both a runnable toy
   vertical stack and an aspirational real-world vertical-stack map.

## Non-Goals

- Do not make Menagerie a public `sugar` CLI subcommand in this change.
- Do not claim real quantum-to-silicon-to-software formal verification exists
  in-tree yet.
- Do not fake proofs for the aspirational vertical stack. Use explicit trusted
  stops, external evidence placeholders, and refusal receipts where evidence is
  not executable.
- Do not rewrite the Bug Zoo runner semantics during the move. The first
  migration should preserve behavior.

## Directory Shape

The target structure is:

```text
menagerie/
  README.md
  manifest.yaml
  bug-zoo/
  hashbound-mainline/
  supply-chain-rails/
  bridgeworks/
  protocol-switchyard/
  change-station/
```

`menagerie/bug-zoo/` is the moved existing Bug Zoo crate and species tree. Its
crate name can remain `sugar-bug-zoo`; the path changes, not the package
identity.

The top-level `menagerie/manifest.yaml` is an index, not a protocol artifact.
It lets docs and future harnesses enumerate destinations consistently.

Example shape:

```yaml
destinations:
  - id: bug-zoo
    path: bug-zoo
    claim: bugs as missing edges; fixes as closure receipts
    runnable: true
  - id: hashbound-mainline
    path: hashbound-mainline
    claim: cross-domain implication chains compressed to 64-byte verification
    runnable: true
  - id: supply-chain-rails
    path: supply-chain-rails
    claim: attack and drift refusal through rank-3 pins and CICP
    runnable: planned
```

## Destination Contract

Every Menagerie destination must have:

- a `README.md` explaining the workflow in `k(I)=t` terms;
- a manifest entry with `id`, `path`, `claim`, and runnable status;
- at least one passing receipt or an explicit reason the destination is
  documentation-only for now;
- at least one refusal or mutation case once the destination is executable;
- commands that run from the repository root;
- no hidden network requirement.

For executable exhibits, success means the runner verifies positive receipts and
fails closed on altered bytes, missing witnesses, wrong CIDs, or policy refusal.

## Bug Zoo Migration

Bug Zoo moves from:

```text
bug-zoo/
```

to:

```text
menagerie/bug-zoo/
```

The migration updates:

- `menagerie/bug-zoo/Cargo.toml` path dependencies and workspace path;
- root README and docs links;
- docs under `docs/how-to/`, `docs/explanation/`, `docs/reference/`, and
  `docs/contributing/`;
- test paths in the Bug Zoo smoke tests;
- self-contract includes such as `sugar-self-contracts` references to
  `bug-zoo/src/lib.invariant.rs`;
- command examples from `cargo run --manifest-path bug-zoo/Cargo.toml` to
  `cargo run --manifest-path menagerie/bug-zoo/Cargo.toml`;
- stale references to old species paths.

The old root `bug-zoo/` should not remain as a symlink or compatibility copy in
the repo after the move. The new hierarchy should carry the architecture.

## Hashbound Mainline Destination

Hashbound Mainline demonstrates the 64-byte theorem as a cross-domain proof
chain, not just as a local performance trick. The mainline is the long route:
software claims can travel through compilers, instruction sets, CPU designs,
device models, and physics anchors while the accepted boundary claim remains a
single content address.

It has two tracks.

### Runnable Toy Vertical Stack

The executable toy stack models heterogeneous domains without pretending they
are real industrial proofs:

```text
toy app claim
  <- toy compiler lowering witness
  <- toy ISA witness
  <- toy CPU/gate witness
  <- toy transistor witness
  <- toy physics axiom
  -> compressed root CID
```

The exhibit should include:

- a chain file naming every claim, witness, signer, and parent edge;
- a policy file declaring trusted signers and stopping depth;
- a runner command that prints the expanded chain and compressed root CID;
- a mutation case where swapping one witness changes the root and verification
  fails;
- a stopping-depth case showing that consumers can stop at a trusted anchor
  without expanding every lower layer.

The toy runner proves the structure:

```text
long implication chain + policy -> accepted root CID
consumer boundary check -> compare 64 bytes
```

### Aspirational Real-World Vertical Map

Hashbound Mainline also carries a non-executable map from software down the
vertical stack:

```text
application contract
  <- source language semantics
  <- compiler correctness witness
  <- ISA semantics
  <- microarchitecture witness
  <- RTL/gate-level witness
  <- transistor model witness
  <- semiconductor/physics paper witness
```

This map must label each layer as one of:

- executable in this repo;
- external evidence candidate;
- policy-trusted anchor;
- future work;
- explicit refusal.

The point is to show how proof can cross domains while preserving the root
claim, and how a consumer can receive the implication chain of correctness as a
single content address.

## Supply Chain Rails Destination

Supply Chain Rails demonstrates artifact movement under proof. Contracts,
witnesses, binaries, CI closures, and provenance must stay on the same verified
track, or admission fails closed. This makes clear that Sugar is not merely
about source-level contracts; it is about refusing supply-chain drift and
substitution when the proof graph no longer matches the bytes.

Initial exhibit cases:

- wrong binary: `binaryCid` mismatch;
- stale proof: old `.proof` does not bind to new artifact bytes;
- changed toolchain or protocol catalog: CICP blast-radius CID changes;
- cache substitution: CI reuse refused unless current closure matches an
  accepted result witness;
- dependency confusion: wrong package cannot satisfy the expected rank-3 pin
  `(contractCid, witnessCid, binaryCid)`.

The destination should point to the existing CICP vectors and security docs,
then add a runnable harness around a small scenario.

## Bridgeworks Destination

Bridgeworks demonstrates proof preservation across domains. It is not only
cross-language correctness. It should show that a claim can span language,
package, protocol, and CI boundaries when both sides bridge to the same
content-addressed predicate or contract set.

Initial examples can reuse existing cross-kit bridge fixtures and the
null-boundary shape from Bug Zoo, but the destination should be framed around
bridge mechanics rather than bug rediscovery.

## Protocol Switchyard Destination

Protocol Switchyard demonstrates protocol versions as content-addressed roots
and migrations as witnessed routing edges:

```text
oldProtocolRootCid -> newProtocolRootCid
```

The destination should surface PEP, GCP, CBP, and extension-body conformance as
proof workflows rather than docs-only protocol machinery.

Initial runnable cases can validate existing protocol evolution fixtures and
show policy accepting or refusing a transition.

## Change Station Destination

Change Station demonstrates commits as proof-carrying transitions:

```text
parent proof state p -> child proof state q
```

The destination should be documentation-first until a minimal commit-proof body
exists. Its design should align with the paper:

- preservation receipts;
- fix receipts;
- refusal receipts;
- policy-relative proof state;
- small root, expandable evidence tree.

## Data Flow

At the Menagerie level:

```text
manifest.yaml
  -> destination README
  -> destination runner or receipt command
  -> signed/content-addressed outputs
  -> positive report or fail-closed refusal
```

At the exhibit level:

```text
artifact I
  -> projection k
  -> canonical claim t
  -> CID(t)
  -> witness edge(s)
  -> policy admission
  -> compressed receipt root
```

The destination runner should make both the expanded evidence and compressed
root visible. The user should be able to see what was proven, what was trusted,
what was refused, and which 64-byte CID carried the accepted claim.

## Error Handling

Menagerie exhibits fail closed. They should prefer explicit refusal reports over
ambiguous success.

Common refusal reasons:

- missing witness;
- untrusted signer;
- malformed manifest;
- CID mismatch;
- stale path after the Bug Zoo move;
- policy does not admit the stopping depth;
- mutation changed bytes but expected CID stayed old;
- extension body exists but no policy-aware checker admitted it.

Each executable destination should make at least one refusal case easy to run.

## Testing

Migration tests:

- `cargo test --manifest-path menagerie/bug-zoo/Cargo.toml`;
- `cargo run --manifest-path menagerie/bug-zoo/Cargo.toml -- --all`;
- stale path search for `bug-zoo/` references that should now be
  `menagerie/bug-zoo/`;
- self-contract tests that include the moved Bug Zoo invariant file.

Hashbound tests:

- positive toy vertical stack verifies to the expected root CID;
- mutated witness changes the root and is refused;
- untrusted signer is refused;
- stopping-depth policy accepts a declared anchor and refuses an undeclared
  anchor.

Supply-chain tests:

- CICP vector checks still pass;
- wrong `binaryCid` fixture is refused;
- CI reuse fails when any load-bearing input changes.

Docs tests:

- root README and docs entry points point to Menagerie;
- Bug Zoo docs describe it as a Menagerie destination;
- command examples use `menagerie/bug-zoo/Cargo.toml`.

## Rollout

1. Add `menagerie/` top-level docs and manifest.
2. Move `bug-zoo/` into `menagerie/bug-zoo/`.
3. Update Cargo paths, tests, docs, and self-contract references.
4. Add Hashbound Mainline skeleton with README, toy chain fixture, policy
   fixture, and runner.
5. Add destination READMEs for Supply Chain Rails, Bridgeworks, Protocol
   Switchyard, and Change Station, initially pointing to existing docs/spec
   fixtures where executable receipts are not ready.
6. Run the Bug Zoo and protocol checks.

## Success Criteria

- The repository has no top-level `bug-zoo/` directory after migration.
- `menagerie/bug-zoo/` preserves current Bug Zoo behavior.
- The README narrative no longer suggests Bug Zoo is the whole executable lab.
- Hashbound Mainline clearly demonstrates that a cross-domain implication chain
  can be accepted through one compressed root CID while remaining honest about
  toy vs. real evidence.
- Supply-chain, cross-domain bridge preservation, protocol evolution, and
  proof-carrying commits are visible as Menagerie destinations.
- All changed commands and tests pass, or any environmental blocker is
  documented with exact failure output.
