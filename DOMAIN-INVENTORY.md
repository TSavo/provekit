# ProvekIt — Honest Domain Inventory

> Goal: before we touch the spec, name **every single thing the product does**, at the
> DOMAIN level (the capability/value), not the implementation level. Then decide
> keep/cut per domain. Only after this is honest do we decompose each kept domain into
> its implementation surface, and *that* is what we spec against — with a portable
> conformance vector per behavior.
>
> Rule of honesty: a domain stays only if we can say plainly what it does FOR someone
> and back it with a content-addressed vector. "We built it" is not a reason to keep it.
>
> Status legend: KEEP / CUT / UNSURE — and `vector?` = does a portable conformance
> vector already pin this, or must we promote one before any clean-room rebuild.

## Candidate domains (seed — to be cut/added/corrected by T)

### D1. Lift — read existing code, extract what it claims to be true
Point ProvekIt at a real codebase in any supported language; it produces a
content-addressed IR of the contracts the code already expresses — from the NATIVE forms
the community already uses (unit tests, assertions, annotations, types, macros, comments,
static-analysis/symbolic results). No bespoke contract language.
- status: **KEEP** (T-confirmed: "lifts contracts to ProofIR")  vector?: ___

### D2. Gap surfacing — name where behavior is unspecified
Identify regions where the behavior itself has no defined notion of right (not "wrong" —
*unframed*). First-class, content-addressed artifact. (Heartbleed/Log4Shell/etc. were gaps.)
- status: ___  vector?: ___

### D3. Bug surfacing — prove a contract violation
Prove that code violates the contract of the concept it claims to implement.
- status: ___  vector?: ___

### D4. Verify — check the contracts (two faces)
(a) **Structural / trust boundary:** execution-free check of a `.proof` — bytes, CIDs,
signatures, memento/header rules. Executes no extension code, no parsers, no checker bytecode.
(b) **Discharge against solvers:** prove the contract LOGIC by emitting to and running
SMT/ATP solvers (Z3 / CVC5 / Vampire / …). This is "verifies those contracts against solvers."
- status: **KEEP** (T-confirmed)  vector?: ___

### D5. Mint / envelope — content-addressed, signed proof artifacts
JCS (RFC 8785) + BLAKE3-512 canonicalization; signed mementos bundled into one `.proof`
envelope; the bundle's filename IS its CID; anyone can recompute.
- status: **KEEP** (T-confirmed: "mints proof envelopes which contain the contracts")  vector?: ___

### D6. Materialize via sugar + boundary — emit native source from contracts
(T-confirmed, core — NOT a cut.) The inverse of lift: realize a contract into runnable
native source by expanding concept → **@sugar** realizations down to **@boundary** leaf
functions (the language/library edge: String.format, JsonNode.asText, .getBytes(UTF_8),
ArrayList::new, …). Each boundary fn is a @boundary with a @sugar realization.
NOTE: the DOMAIN is kept; the experimental scaffolding around it (lower, byte-identical
cycle harnesses) is separately suspect — audit impl, not capability.
- status: **KEEP** (T-confirmed)  vector?: ___

### D7. Migrate / transport — move a contract/proof across languages
M+N (not M×N) via the concept hub; loss-composition; every move returns a trichotomy
receipt: exact / loudly-bounded-lossy / refuse.
- status: ___  vector?: ___

### D8. Concept hub / federation — cross-language admissibility
Promote a contract that appears in N≥2 languages to a cross-language concept; the catalog
of concepts is the M+N topology; libraries/communities author their own lifters (the doors).
- status: ___  vector?: ___

### D9. Catalog / protocol evolution — signed, content-addressed protocol transitions
The protocol catalog (versioned, CID-pinned, signed); PEP transitions become signed
body-claims; the extension-protocol surface (TDP/GCP/CBP/ORP/FRP) as body conventions
over a stable core.
- status: ___  vector?: ___

### D10. Provenance / signing — keys, signatures, trust anchors
Ed25519 signing; signer-independent contract-set CIDs as the trust anchor; signed tags;
the attestation envelope.
- status: **KEEP** (T-confirmed: "it has provenance")  vector?: ___

### D11. Self-application / dogfood — ProvekIt proves itself
The framework proves its own kits against itself, using ONLY native lifted contracts
(post `.invariant` elimination). The honesty test: no private contract language for ourselves.
- status: ___  vector?: ___

### D12. Kit / lift-plugin protocol — how a language participates
The per-language kit contract: the lift-plugin RPC, C1-C8 conformance, kit discovery via
`.provekit/config.toml` (surface → lifter), each kit owning its `.proof` resolution.
- status: ___  vector?: ___

### D13. Demonstration corpus — bug-zoo / menagerie / exhibits
Executable specimens that show the product working (bug zoo, supply-chain rails, bridge
demos). Is this PRODUCT, or marketing/test scaffolding? (candidate CUT-or-relocate)
- status: ___  vector?: ___

---

## Completeness cross-check (honesty = nothing forgotten)
Manual recall will miss behaviors. Before the list is trusted, sweep the actual surface:
- every CLI subcommand (`provekit <cmd>` + flags)
- every `make` target
- every conformance fixture + pinned CID (these ARE the behaviors that must survive)
- the menagerie exhibits
…and confirm each maps to a domain above. Anything unmapped is either a forgotten domain
or cruft. (Optionally seed with the `understand-anything:domain-analyzer` agent, then audit.)

## Next steps (not started)
1. T cuts/adds/corrects the domain list above.
2. For each KEEP domain: enumerate the implementation surface (commands, RPC methods, file formats).
3. For each behavior under a KEEP domain: confirm/promote a portable conformance vector.
4. The set of (kept domains × their vectors) = the spec we clean-room rebuild against.
