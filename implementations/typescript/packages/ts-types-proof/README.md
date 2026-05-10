# @provekit/ts-types-proof

Drop-in shim that delivers the ProvekIt protocol substrate to TypeScript
projects via a single `npm install`.

## What this package is

A wrapper around `@provekit/ir-symbolic` (the TS kit) that ships under
the name `ts-types-proof`. The user installs it because they want typed
wrappers for `parseInt`, `Math.*`, `String.prototype.*`, `Array.prototype.*`,
plus the symbolic-primitives surface for authoring invariants.

As a side effect of importing this package, **13 V8 bridge declarations
register in the ProvekIt protocol's process-local registry** at module
load. Every invariant the user subsequently authors that references one
of those primitives composes through the bridge into the proofHash chain
all the way down to V8's signed catalog (when published).

## What this package isn't

- A replacement for TypeScript's built-in lib (`lib.es*.d.ts`,
  `lib.dom.d.ts`). Those types live in the TypeScript installation and
  describe ECMA-262 / DOM / Node APIs at the type-checker level. This
  shim doesn't shadow them; it adds an orthogonal layer (IR-emitting
  helpers + protocol bridges).
- A signed catalog. v0.1.0 ships the bridge DECLARATIONS but not their
  signatures (signing requires the upstream V8 release team's key,
  which doesn't yet publish a ProvekIt catalog). Signatures land when
  the upstream catalog goes live.

## Why install it

The user installs this for the **types + helpers** they wanted. They
get the **protocol substrate** along for the ride: adoption asymmetry
in action: refusing means giving up typed helpers, which nobody does.

Once installed:
- `parseInt(s)` returns an `IrTerm` instead of computing.
- `must("never returns negative", forAll(Int, x => gte(parseInt(x), num(0))))`
  authors an invariant whose IR is content-addressed.
- The propertyHash for that invariant composes into the consumer's
  binary's proofHash.
- VS Code (with the ProvekIt LSP) shows the propertyHash live in
  hover-info, and red-squiggles any invariants Z3 refutes.

The user did not have to opt in to the protocol. They opted in to the
types. The protocol came along.

## Deprecation roadmap

When upstream ts-types (the canonical types package the ecosystem
converges on) adopts the protocol natively: registering the same
bridge declarations at module load: this shim deprecates with a
one-line dependency swap.

**Critically: propertyHash CIDs stay stable across the migration.**
Content addressing means the shim's `parseInt` produces the same CID
as a protocol-native ts-types' `parseInt`, because both bridge to V8's
parseInt declaration via the same canonical IR. Users who built
invariants under the shim continue to compose with users on the
upstream version. No proof chain breaks.

That's the architectural property that makes the shim safe to
publish: it's not a fork of the protocol, it's a delivery vehicle.

## What this enables

- **Library authors** publishing TS packages: install this in dev,
  author invariants on your public APIs, ship the catalog with your
  package. Consumers automatically get the proofHash chain.
- **Library consumers**: install this, author bridge mementos that
  point at upstream libraries' propertyHashes, get supply-chain
  detection at semantic level (per the supply-chain spec).
- **VS Code users**: install this + the ProvekIt LSP, see live
  diagnostics for invariants in your editor.
- **CI gates**: install this, run `provekit verify`: a PR that
  breaks an invariant fails the gate before reaching production.

All from one `npm install`.

## Status

v0.1.0. Bridge declarations are unsigned today (V8's authoritative
catalog is not yet published). Signatures land in v0.2.0 once the
upstream catalog goes live; the migration is a CID update with no
break.

The protocol catalog this shim conforms to: `sha256:a2d062341e3ca0f0`
(see `docs/specs/2026-04-30-protocol-versioning.md` in the main repo).

## License

MIT OR Apache-2.0.
