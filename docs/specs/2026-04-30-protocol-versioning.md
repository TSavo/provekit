# ProvekIt: Protocol Versioning via Self-Reference

> Author: shared session 2026-04-30 (T + Claude). The protocol is
> content-addressed, including the protocol's own spec.

The protocol's spec is itself a catalog memento (per the
memento-envelope-grammar spec). The catalog's `properties` map names
each spec document to its content-addressed CID. The catalog's own
CID is the protocol version.

## v1.0.0

Protocol catalog: `docs/specs/2026-04-30-protocol-catalog.json`

**Protocol version CID: `sha256:e04b7cc466911b1d`**

**This CID is `provekit.proofHash` for ProvekIt itself.** The same
field a library carries in its `package.json` to declare its
proof-chain root, ProvekIt carries to declare its own protocol
version. The framework eats itself: a library's proofHash is the
CID of its property catalog; ProvekIt's proofHash is the CID of
its protocol-spec catalog. Same primitive, same field name, same
math. ProvekIt is one more library, and the protocol is one more
property catalog.

This CID names a catalog whose entries are:

| Spec | CID (sha256-prefix-16) |
|---|---|
| ir-formal-grammar | `sha256:0c394dbb0bc6da2b` |
| canonicalization-grammar | `sha256:cb2367c97b57ba05` |
| memento-envelope-grammar | `sha256:68f4b1cc55c01667` |
| signatures-and-non-repudiation | `sha256:9b9f86ec1795ff90` |
| chain-validity-and-fail-closed | `sha256:7d7777ef5b0017fe` |
| ir-extension-protocol | `sha256:c48b69c15e1eb7e9` |
| semantic-envelope | `sha256:b667f5c10c37173c` |
| supply-chain-via-semantic-envelope | `sha256:eefec0fb212ef5f3` |

## Conformance declarations

A reference implementation declares which protocol version(s) it
conforms to via the same shape any consumer references a library:

```yaml
# in an implementation's metadata
provekit-protocol-conformance:
  - cid: sha256:e04b7cc466911b1d
    version: v1.0.0
```

A verifier that holds the catalog memento at that CID can check, for
each spec entry, that the corresponding spec's bytes hash to the
listed CID. If any drift, the implementation has changed the spec
without bumping the version — a protocol violation.

## Versioning rules

1. **Any spec change requires a new catalog CID.** Changing a spec's
   bytes changes its CID, which changes the catalog's `properties`
   map, which changes the catalog's CID. Same content addressing as
   propertyHash: the math forces a version bump.

2. **Implementations pin specific catalog CIDs.** Saying "I support
   v1" without a CID is meaningless; v1 is whatever bytes anyone
   happens to have. CID pinning makes the conformance claim
   verifiable.

3. **Multi-version implementations** declare multiple CIDs. A future
   v1.0.1 that adds a single spec doc can be supported alongside
   v1.0.0 by an implementation that declares both CIDs.

4. **Bootstrap signing.** v1.0.0 is unsigned (the signatures spec is
   itself one of the entries; signing the v1.0.0 catalog requires the
   signature machinery defined inside it). v1.0.1 will re-issue the
   catalog as a signed memento, with the v1.0.0 unsigned catalog's
   CID embedded in the signed v1.0.1 as the bootstrap reference.

5. **Spec evolution is mechanical.** A working group editing a spec
   produces a new bytes; a new CID; a new catalog candidate; the
   project's signing authority signs the new catalog; the new version
   is published. Implementations decide whether to upgrade by reading
   the diff of the catalog (which spec CIDs changed) and the
   migration spec (if any) for that bump.

## The recursive payoff

ProvekIt's protocol uses content addressing as its core primitive. The
spec describing that protocol is itself content-addressed via the same
machinery. The version of the protocol is a CID. Implementations
verify their conformance via CID comparison. There is no
out-of-protocol authority deciding what "v1" means; v1 is the bytes
that hash to `e04b7cc466911b1d`.

This is the same self-reference shape as Git (commit hashes refer to
trees that may include other commits), IPFS (DAG addresses include
references to other DAGs), and Bitcoin (block hashes chain backward
through prior blocks). ProvekIt is one more application of the same
primitive.

The framework's promise is total within its scope. The version is a
CID. Conformance is a CID comparison. The TypeScript implementation
in this repository is one realization; alternative implementations in
any language conform to the same CID-pinned spec set or they are not
ProvekIt.
