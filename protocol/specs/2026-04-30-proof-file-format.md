# ProvekIt: `.proof` File Format

> A `.proof` file is a content-addressed bundle of mementos that ships
> with a software package. It contains a catalog memento at the root
> plus every member memento the catalog references, all in one
> self-contained artifact.

> The format is a **protocol-level standard**, not a framework artifact.
> Any conformant tool can produce or consume `.proof` files; ProvekIt's
> `mint catalog` is one implementation. Same shape as `.html` (W3C),
> `.json` (ECMA), `.toml` (TOML spec) — extension reflects the protocol,
> not the producer.

> Companion specs: memento envelope grammar
> (`2026-04-30-memento-envelope-grammar.md`) defines per-variant CDDL;
> canonicalization grammar (`2026-04-30-canonicalization-grammar.md`)
> defines the byte-level encoding; this spec defines how those mementos
> compose into a single shippable file.

## 1. What's in a `.proof` file

A `.proof` file's bytes are the **deterministic CBOR encoding**
(RFC 8949 §4.2.1, "Core Deterministic Encoding") of a single
catalog memento body with all member memento bodies embedded inline.

CBOR is mandatory for the envelope. JSON is wrong for this artifact:
signatures and public keys are byte strings, not strings; member
bodies may be CBOR or JSON depending on memento type; embedding raw
bytes in JSON requires base64 wrapping that lies about what the hash
covers. Canonical CBOR is honest about bytes, smaller on disk, and
already the primitive IPLD uses for content addressing.

```cddl
proof-file = catalog-memento-with-embedded-members

catalog-memento-with-embedded-members = {
  kind: "catalog",
  name: tstr,
  version: tstr,

  ; Optional: hash of the compiled binary this proof bundle covers.
  ; When present, the framework checks that the running binary's hash
  ; matches before trusting any claims in this bundle. This is the
  ; supply chain anchor: change any bit in the binary and the proof
  ; becomes invalid.
  ? binaryCid: cid,

  ; Map from member CID to the member's full SIGNED envelope, encoded
  ; as canonical bytes (JCS for memento envelopes per the memento
  ; envelope grammar) and embedded as a CBOR byte string. The map key
  ; MUST equal the envelope's own CID per the memento envelope grammar's
  ; CID rule ("blake3-512:" + hex(blake3_512(canonical(envelope_without_cid_and_signature)))).
  ; Verifiers re-derive the CID from the embedded envelope's body fields
  ; (with cid + signature elided) and reject on mismatch. They also
  ; verify each member's signature against its declared signer.
  members: { + cid => bstr },

  ; Optional metadata: key-value map for tooling, diagnostics, and
  ; human review. Included in the signed payload (tamper-evident) but
  ; explicitly NON-NORMATIVE: verifiers MUST NOT use metadata for
  ; verification logic, only for display. Keys are strings; values are
  ; strings. Unknown keys are preserved but ignored.
  ? metadata: { + tstr => tstr },

  ; Optional dependency manifest: catalogs this catalog references
  ; transitively (other .proof files in dependency packages).
  ? depends-on: [* cid],

  signer: cid,
  signature: bstr,    ; raw Ed25519 signature bytes; not base64
  declaredAt: tstr,   ; RFC 3339 timestamp
}

cid = tstr            ; multibase-encoded CID per IPLD
```

The member bodies are FULL memento bodies (per the memento envelope
grammar), embedded as opaque byte strings. A consumer that reads a
`.proof` file has everything needed to verify the chain locally — no
network fetches, no dependency resolution, no missing references.

Inspection tooling (`provekit dump <file>.proof`) renders the CBOR
envelope as JSON for human review. The on-disk format is binary; the
operator-facing format is JSON-via-tool.

## 2. Filename convention and trust root

```
<cid>.proof
```

**The filename IS the trust root.** It encodes the CID of the file's
bytes. A verifier recomputes the hash, compares to the filename, and
that single check is the entire identity statement. No external
manifest is consulted to establish trust; package.json is irrelevant
to integrity.

A package conventionally ships exactly one `.proof` file at its root
(parallel to `package.json` in npm packages, `Cargo.toml` in Rust
crates, `go.mod` in Go modules). Package manifests MAY carry a hint
field — `provekit.proofHash` in `package.json`, `[package.metadata.provekit].proof-hash`
in `Cargo.toml` — but this is a **discovery convenience, not part of
the trust chain**. If the hint disagrees with the filename actually
on disk, the verifier trusts the file and SHOULD warn about a stale
hint. It MUST NOT reject solely on hint mismatch.

```
my-package/
├── package.json                  # OPTIONAL hint: "provekit": { "proofHash": "bafy_X..." }
├── bafy_X....proof               # filename IS the CID; bytes hash to bafy_X...
├── src/
└── ...
```

A package MAY ship multiple `.proof` files (e.g., during a deprecation
window). Discovery picks the file whose filename matches the manifest
hint, or — if no hint — the most recently signed file. The trust root
is always the filename's CID, regardless of how the file was selected.

## 3. Integrity rules

A `.proof` file passes integrity verification iff:

1. **Filename matches content.** The file's bytes hash to a CID equal
   to the filename (without `.proof`). Verifiers MUST recompute and
   reject mismatches. This is the trust root; failing this rule
   invalidates everything.

2. **Embedded member CIDs match envelope identities.** For each entry
   `members[cid] = bytes`, the bytes decode as a memento envelope, and
   `computeEnvelopeCid(decoded)` (per the memento envelope grammar's
   CID rule: `"blake3-512:" + hex(blake3_512(canonical(envelope_without_cid_and_signature)))`)
   MUST equal `cid` (the map key). Verifiers MUST recompute and reject
   mismatches.

3. **Catalog signature is valid.** `signer` resolves to a public-key
   memento (which may itself be embedded in `members`); `signature` is
   a valid Ed25519 signature over the canonical catalog bytes (with
   the `signature` field omitted from the signing payload).

4. **Member signatures are valid** for each member that requires
   signing per the memento envelope grammar.

5. **Binary CID matches running artifact (when present).** If the
   catalog includes `binaryCid`, the verifier MAY check that the
   hash of the currently executing binary equals `binaryCid`. This
   is the supply chain anchor: a `.proof` bundle whose `binaryCid`
   matches the running binary attests to "this proof was produced
   by THIS binary, not a tampered fork." Recompilation, runtime
   patches, or supply-chain injections change the binary CID; a
   verifier that performs this check rejects mismatches.

   v1.3.0 status: this rule is MAY (not MUST). The protocol defines
   the field and the check; no v1.3.0 verifier implements
   `hash(running_binary)` yet. Producers may emit `binaryCid` (Rust
   does, defaulting to `None`); consumers may verify. A future v1.4.0
   minor bump considers promoting to MUST once a reference verifier
   ships the running-binary hash routine end-to-end.

Fail-closed by default. Any rule violation produces a structured
rejection with the failing rule's ID; verifiers MUST NOT accept
partially-valid bundles.

A manifest hint mismatch (e.g., `package.json.provekit.proofHash` not
equal to the on-disk filename CID) is NOT a verification failure. The
verifier SHOULD warn but MUST NOT reject; the file's identity is its
hash, not a manifest's claim.

## 4. Distribution

A package ships its `.proof` file alongside its source. For npm:

```jsonc
// package.json
{
  "name": "@example/lib",
  "version": "1.2.3",
  "files": [
    "src/",
    "lib/",
    "*.proof"           // include .proof files in the published tarball
  ],
  "provekit": {
    "proofHash": "blake3-512:e04b..."
  }
}
```

For Cargo: `[package].include` array. For Go modules: any file in the
module is shipped by default. For Conan/vcpkg: package layout per the
respective conventions.

The `.proof` file is the FRAMEWORK-AGNOSTIC distribution artifact. A
package that ships a `.proof` file can be consumed by any conformant
verifier in any language; the verifier reads the file by its filename
(= CID) and walks the embedded member bodies.

## 5. Walking a `.proof` file

A consumer's verifier walks a `.proof` file as follows:

```
1. Discover the .proof file:
   a. If a manifest hint is present (e.g., package.json.provekit.proofHash),
      try <packageRoot>/<hint>.proof.
   b. Otherwise (or if hint file missing), enumerate <packageRoot>/*.proof
      and select the most recently signed.
2. Read the chosen file's bytes
3. Recompute hash(bytes); verify equals filename's CID; else REJECT
   (trust-root rule — failing this invalidates the bundle)
4. Decode CBOR; parse as catalog memento
5. For each (memberCid, memberBytes) in members:
   - Recompute hash(memberBytes); verify equals memberCid; else REJECT
   - Verify member's signature per the memento envelope grammar
   - Register the member in the verifier's local registry
     (extension declarations, bridge declarations, property mementos
     all integrate into their respective registries)
6. Verify the catalog's own signature
7. If `binaryCid` is present: compute hash(running_binary); verify
   equals `binaryCid`; else REJECT (supply chain anchor)
8. Walk depends-on for transitive .proof files in other packages
9. (Optional) If a manifest hint was present and disagreed with the
   discovered file's CID, emit a warning. Do NOT reject.
```

No network fetches. No dependency resolution beyond reading other
packages' `.proof` files. The bundle is self-contained.

## 6. Versioning

This spec is v1 of the `.proof` file format. Future versions add a
top-level `formatVersion` field to the catalog body. v1 implicitly is
`formatVersion: 1`. Verifiers MUST reject formats they don't support;
the framework's response to an unrecognized version is fail-closed,
not best-effort interpretation.

## 7. Conformance criteria

A producer conforms to this format iff it:

1. Outputs a single file with extension `.proof`.
2. Names the file `<cid>.proof` where `cid` is the self-identifying
   hash of the file's bytes per the canonicalization grammar:
   `<algorithm-tag>:<lowercase-hex-digest>`. v1.1.0 uses
   `blake3-512:<128 hex chars>` — full 64-byte BLAKE3 digest, no
   truncation. Filenames are ~150 chars; that is intentional.
3. Encodes the envelope as deterministic CBOR (RFC 8949 §4.2.1).
4. Embeds every member memento body referenced by the catalog as an
   opaque byte string (no dangling CIDs).
5. Signs the catalog and every signed-required member per the
   signatures spec.
6. Recomputes and verifies all CIDs before writing (sanity check that
   the producer didn't construct an invalid bundle).

A consumer conforms iff it:

1. Discovers the `.proof` file by manifest hint OR by extension scan;
   never trusts a manifest hint as authoritative.
2. Recomputes and verifies the file's bytes hash matches the filename
   (this is the trust root).
3. Decodes the CBOR envelope (rejecting non-deterministic encodings).
4. Recomputes and verifies each member body's hash matches its CID.
5. Verifies all required signatures.
6. If `binaryCid` is present, verifies it matches the running binary.
7. Fail-closes on any verification failure (rules 2–6).
8. Warns but does NOT fail on a manifest hint mismatch.

## 8. The architectural commitment, restated

A `.proof` file is the protocol's distribution artifact. It is
content-addressed at every level — file bytes hash to the filename
CID; member bytes hash to their listed CIDs; signatures bind to
canonical bytes — so tampering anywhere is detectable everywhere.

The format is the protocol. ProvekIt's `mint catalog` is one
implementation; future tools in any language conform to this spec
or they don't write `.proof` files. The wire format is independent
of the framework.

A consumer who installs a package, reads its `.proof` file, and
verifies its integrity has the entire proof chain in hand — no
network, no central authority, no shared state with the framework.
That's the distribution artifact's promise: self-contained, content-
addressed, fail-closed.
