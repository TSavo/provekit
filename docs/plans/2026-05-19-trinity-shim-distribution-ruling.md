# Trinity Shim Distribution Ruling

Date: 2026-05-19
Status: Active. Ratified by Sir 2026-05-19. Formalizes the architecture for Phase B (shim distribution) of the parent audit's section 7.1 distribution evolution.

## Ruling

Trinity-demo library kits (rusqlite, sqlite-jdbc, python-sqlite3, and any future per-ecosystem library kit) ship as shim packages via each ecosystem's existing package registry. Four substrate-uniform principles govern shim distribution; each principle routes through EXISTING substrate machinery. No new substrate primitives are introduced by shim distribution itself.

## Background

Per the parent audit (`docs/audits/2026-05-18-kit-as-substrate-participant-vision.md`) section 7.1, library kit distribution evolves through three phases:

- **Phase A (Bootstrap-resident):** kit declarations and body templates ship inside the substrate's own source tree (`implementations/rust/libsugar/src/core/platform_semantics/<tag>.rs`, `menagerie/<lang>-language-signature/specs/body-templates/`). Today's state for better-sqlite3, pg, python-sqlite3, python-aiosqlite. No vendor commitment required.
- **Phase B (Shim distribution):** shim packages sibling to library packages, distributed via each ecosystem's existing registry. Library author's commitment is zero; anyone can publish a shim.
- **Phase C (Vendor adoption):** library author ships the `.proof` envelope inside the library's own package; shims become unnecessary for adopted libraries. Library author signs once; the official envelope supersedes the shim.

This ruling governs Phase B. Phase C transitions inherit the same admission-tier mechanics (see §5 below).

## §1. Naming convention per ecosystem

Shim packages follow each ecosystem's natural namespacing convention. The substrate's namespace is `sugar-shim` (kebab-case where ecosystems use flat namespaces; scoped where ecosystems support scoping; reverse-DNS where ecosystems use Java-style groupIds).

| Ecosystem | Shim package name pattern | Example |
|---|---|---|
| Cargo (Rust) | `sugar-shim-<library-tag>` | `sugar-shim-rusqlite` |
| Pip (Python) | `sugar-shim-<library-tag>` | `sugar-shim-python-sqlite3` |
| npm (JS/TS) | `@sugar-shim/<library-tag>` | `@sugar-shim/typescript-better-sqlite3` |
| Maven (Java) | `org.sugar-shim:<library-tag>-proof` | `org.sugar-shim:java-sqlite-jdbc-proof` |

The `<library-tag>` field is the SUBSTRATE's canonical library tag (the second component produced by `split_library_surface`), not the ecosystem-specific package name. Example: `sugar-shim-sqlite-jdbc` (substrate tag) even though the corresponding Maven library artifact is `org.xerial:sqlite-jdbc`.

The substrate discovers shim packages by scanning each ecosystem's installed dependencies for matches against these naming patterns. Per §3, the discovery uses each ecosystem's standard resolution mechanism.

## §2. `.proof` location per ecosystem: internal to the kit binary, NEVER substrate-discovered

The substrate-CLI does NOT directly read `.proof` files from inside shim packages. Per the existing kit-dispatch protocol (PEP 1.7.0 over JSON-RPC), the substrate-CLI invokes a NATIVE KIT BINARY; the kit binary loads its own `.proof` envelope internally using its language's native resource-loading mechanism, then serves the envelope (or queries derived from it) over JSON-RPC.

This is the substrate-as-pure-protocol pattern. The substrate-CLI knows how to invoke binaries and speak JSON-RPC. It does NOT know about cargo crates, pip packages, Maven jars, or npm tarballs. Each kit owns its own resource-loading semantics.

**Per-kit loading mechanism (kit-author's responsibility; informational reference):**

| Ecosystem | Kit binary loads `.proof` via | Conventional location inside package |
|---|---|---|
| Cargo | `include_bytes!("...")` at compile time, OR runtime read from crate asset path | `assets/sugar.proof` (or per kit author's choice) |
| Pip | `importlib.resources.files("sugar_shim_<tag>") / "sugar.proof"` | `sugar_shim_<tag>/sugar.proof` per `pyproject.toml` package-data |
| npm | `fs.readFileSync(__dirname + '/sugar.proof')` | Package root or `dist/` per `package.json` files declaration |
| Maven | `getClass().getResourceAsStream("/META-INF/sugar/sugar.proof")` | `META-INF/sugar/sugar.proof` (JVM classpath convention) |

These conventions are entirely internal to the kit. The substrate does not enforce them; the kit author publishes the kit binary, and the binary handles its own resource loading. Any future kit author may choose a different internal convention; the substrate is indifferent because it never touches the package directly.

**What flows over JSON-RPC:**

The kit binary's RPC methods (per PEP 1.7.0) include `initialize` (returns kit metadata) and per-surface methods (`lift`, `realize`, etc.). For shim discovery, the kit binary's `initialize` response carries:

- The kit's signed `.proof` envelope bytes (or a kit-declaration projection derived from it).
- The kit's `bound_library_cids` field per §4.
- The kit's signature(s) per §5.
- The kit's protocol_version and capability advertisement.

The substrate-CLI verifies the signature against consumer policy (per §5), parses the kit-declaration, and proceeds with bind / realize as usual. The `.proof` envelope's BYTES transit the RPC; the envelope's STORAGE inside the kit's package is invisible to the substrate.

## §3. Substrate discovery: existing PATH probe handles all four ecosystems

The substrate-CLI's existing kit-dispatch tiered resolution at `kit_dispatch.rs:521-583` handles shim discovery WITHOUT per-ecosystem substrate-side code:

1. Project-local manifest (`.sugar/lift/<surface>/manifest.toml`).
2. Env-var override (`SUGAR_BIND_LIFT_<LANG>_BIN`).
3. Built-in convention (workspace-relative compile-time path).
4. **PATH probe (`sugar-bind-lift-<source_lang>` on PATH)** — this tier handles shim binaries uniformly.

Each ecosystem's package manager installs the shim's binary onto PATH as part of its standard package-install behavior:

- Cargo: `cargo install sugar-shim-rusqlite` puts the shim binary in `~/.cargo/bin/` (on PATH for cargo users).
- Pip: `pip install sugar-shim-python-sqlite3` installs the package's console_script entry point in the venv's `bin/`.
- Maven: `mvn dependency:get` plus a launcher script (or `jar -m org.sugar-shim:java-sqlite-jdbc-proof` invocation) provides the binary.
- npm: `npm install @sugar-shim/typescript-better-sqlite3` installs the bin entry in `node_modules/.bin/`.

The substrate-CLI's existing tier 4 PATH probe finds the binary; invokes it with `--rpc`; talks JSON-RPC. No ecosystem-specific substrate code; no `dispatch_shim_resolve` primitive needed; the existing `dispatch_bind_lift` and `dispatch_realize` primitives consume shim-served kit-declarations transparently.

**Binary naming convention:** the shim binary follows the substrate's existing per-surface PATH-probe convention. For a shim of library `<library>` providing the bind-lift surface for source language `<lang>`, the binary on PATH is named per the existing convention: `sugar-bind-lift-<lang>` (with the shim's PATH location ensuring it resolves to the shim's binary for the relevant library context). For realize surfaces: `sugar-realize-<lang>-<library-tag>` per existing convention.

Per-kit publishing responsibility: each shim package's publish step ensures the appropriate binary lands on PATH under the substrate-convention name. The kit may name its internal binary anything; what matters is what shows up on PATH after install.

**Native overriding:** the existing tier 1 project-local manifest and tier 2 env-var override work transparently. A consumer who wants to substitute a local development binary sets the env var; the substrate's existing tiered resolution picks it up.

## §4. Versioning via content-addressed multi-pinning

The substrate does NOT use version-range strings to express shim coverage. Per paper 04 §4.1 (rank-N tuple pinning) and paper 14 §L6 (CVE blast-radius is SELECT), version-range strings are precisely what the substrate dissolves into content-addressed pin sets.

### §4.1 Bound library CIDs

Each shim's `.proof` envelope carries a `bound_library_cids` field (or equivalent shape per the binding-memento spec):

```
bound_library_cids: [<library-version-CID-1>, <library-version-CID-2>, ...]
```

Each CID is the content-addressed library version (the bytes of the released library at that version's tag/commit/release-artifact) the shim asserts bindings against. The shim publisher computes the CID over the library's release-artifact bytes (per the ecosystem's release convention: cargo's released crate bytes, npm's tarball bytes, Maven's jar bytes, pip's wheel bytes).

### §4.2 Discovery-time verification

At substrate-CLI time, the shim-discovery primitive:

1. Locates the shim package (per §3).
2. Reads the shim's `.proof` envelope's `bound_library_cids` set.
3. Computes the CID of the user's installed library version (the bytes of the library at the user's installed version).
4. Verifies the installed library's CID is in the shim's `bound_library_cids` set.

If yes: admit the shim's bindings.
If no: refuse loudly via gap record (per existing trichotomy refuse-leg ruling at `docs/plans/2026-05-18-refuse-leg-short-circuit-ruling.md`). The substrate is honest about coverage; the consumer sees a clear "shim does not cover this library version" message.

### §4.3 Maintenance posture

Shim publishers extend the `bound_library_cids` set as they verify compatibility against new library releases. A library's patch release (no semantic change to surface ops) usually requires the shim publisher to add the new patch version's CID to the set after testing. A library's major release (potentially changing surface op semantics) requires a new shim release with a re-evaluated bound set.

Per paper 14 §L6's blast-radius reasoning: when a vulnerable library version emerges, the substrate's `bound_library_cids` SELECT identifies every shim that admits the vulnerable CID. CVE blast-radius is a content-addressed query, not a prose exercise over version-range strings.

### §4.4 Phase A / Phase B / Phase C coexistence

Phase A bootstrap-resident kits also use `bound_library_cids` (extending the in-source kit declarations to carry the field). The mechanism is uniform: every kit declaration in every phase pins library CIDs the same way.

When a vendor adopts (Phase C transition), the library's own published `.proof` envelope carries `bound_library_cids = [<this-version's-CID>]` (single entry per release). The shim and the Authored envelope coexist; consumer policy (§5) decides which to trust.

## §5. Trust via paper 23's admission tier model

The substrate does NOT enforce shim trust centrally. Per paper 23 §6, trust is consumer-policy-driven through four admission tiers.

### §5.1 Four admission tiers

| Tier | Source | Signature | Trust currency |
|---|---|---|---|
| Authored | Library author | Library author's key | Author's signature |
| Self-Attested | Library author asserts without independent discharge | Author's key | Author's assertion |
| Third-party (Inferred) | Third party writes bindings; author hasn't shipped envelope | Third party's key | Third-party signer |
| Third-party (Discharged) | Third party bindings + independent prover discharge | Third party + prover keys | Discharge proof verification |

### §5.2 Phase B shims are Third-party (Inferred)

Trinity-demo shims published by the sugar-project (or trusted maintainers acting as third parties) ship as the Third-party (Inferred) tier. The shim's `.proof` envelope is signed by the publisher's key; the consumer's verifier policy decides whether to trust the publisher's key.

The sugar-project's signing key (per project memory `reference_sugar_provenance_keys`) is the initial bootstrap trust anchor. The Ed25519 key at vault `secret/sugar/provenance-ed25519` signs shims published by the project. Consumer verifier policies that trust the sugar-project key admit these shims at the Third-party (Inferred) tier.

Additional trusted maintainers (community contributors who establish reputation) can publish shims under their own keys; consumer policies decide independently whether to trust those publishers.

### §5.3 Phase C transition: Inferred → Authored

When a library author absorbs the shim and ships their own `.proof` envelope (Phase C / Authored tier), the substrate's discovery encounters both envelopes simultaneously. The consumer's policy resolves the conflict:

- Default policy: Authored supersedes Third-party (Inferred). Library author's signature is canonical.
- Consumer override: a consumer who specifically trusts a third-party shim's discharge work over the library author's silence can configure a policy that prefers Third-party (Discharged) over Authored.

Both envelopes remain available; neither mechanically supersedes the other. Consumer policy decides per paper 23 §6's framing: "package-author keys are the trust currency, and the trust is on bindings, not on the right to ship a package."

### §5.4 Per-ecosystem signature mechanisms

The substrate's verifier reads each ecosystem's standard signature infrastructure:

- Cargo: crates.io publisher verification; sigstore-rs (when wired).
- Pip: PEP 740 (sigstore-based signing; when wired).
- Maven: GPG signatures (Maven Central infrastructure; required for Central publication).
- npm: sigstore via npm provenance + package signing (when wired).

Per-ecosystem signature-verification implementations live as separate sub-issues (see §6). The substrate's verifier composes consumer policy + ecosystem signature mechanism + admission-tier table at discovery time.

### §5.5 No substrate-central enforcement

The substrate provides:
- The four admission tiers (paper 23 §6).
- The signature-reading infrastructure per ecosystem.
- The verifier discipline that composes consumer policy with admission tier and signer key.

The substrate does NOT provide:
- A central allowlist of trusted publishers.
- A central revocation registry.
- A strict-mode override that forces tier-N-or-refuse.
- A consumer-policy template that overrides individual configuration.

Consumer policy is the gate. Substrate is uniform infrastructure.

## §6. Implementation sub-issues

Per §3, the substrate-CLI does NOT gain ecosystem-specific code. The existing PATH probe + JSON-RPC dispatch handle all four ecosystems uniformly. The implementation sub-issues collapse dramatically: each is a KIT-SIDE deliverable plus signature-verification wiring on the substrate side.

Each sub-issue:

1. **Kit-side:** ship a shim package in the target ecosystem that:
   - Bundles the signed `.proof` envelope using the ecosystem's standard resource convention.
   - Provides a binary speaking PEP 1.7.0 over JSON-RPC, named per the substrate's existing PATH-probe convention.
   - The binary's `initialize` RPC response returns the kit's signed envelope (or projection).
   - The binary's per-surface RPC methods (`lift`, `realize`, etc.) serve the kit's declaration.
2. **Substrate-side (signature verification):** wire the per-ecosystem signature-verification adapter (per §5.4) into the substrate-CLI's existing verifier. This is the ONLY substrate-side per-ecosystem work; no new discovery primitive needed.
3. **End-to-end test:** verify the Trinity demo's shim discovery for the affected ecosystem.

Sub-issues to file (not yet filed; each references this ruling):

- **D13a-Cargo:** ship `sugar-shim-rusqlite` crate (Rust binary, sigstore-rs verification adapter). Substrate-side: wire crates.io publisher + sigstore-rs verification.
- **D13a-Pip:** ship `sugar-shim-python-sqlite3` package (Python binary via console_script, PEP 740 verification adapter). Substrate-side: wire sigstore verification per PEP 740.
- **D13a-Maven:** ship `org.sugar-shim:java-sqlite-jdbc-proof` (Java jar with executable main class, GPG signature verification adapter). Substrate-side: wire GPG signature verification.
- **D13a-Npm:** ship `@sugar-shim/typescript-better-sqlite3` and similar (Node binary via package.json bin, sigstore-via-npm verification adapter). Substrate-side: wire sigstore-via-npm verification.

The kit-side work (shipping the shim packages) is the bulk of each sub-issue. The substrate-side work (signature-verification adapter) is a small per-ecosystem addition to the existing verifier; no new discovery primitive, no new dispatcher tier, no new substrate machinery.

## §7. What this ruling deliberately does NOT do

- Does NOT enumerate per-ecosystem packaging mechanics in detail. Each kit's publication is the kit author's responsibility per the ecosystem's standard conventions.
- Does NOT pre-allowlist any signer key. Consumer policy decides.
- Does NOT impose substrate-side restrictions on shim publishers. Permissionless publication per paper 23 §8.
- Does NOT block adoption: sugar-project ships the bootstrap shims; community publishers can ship additional shims; vendor adoption transitions Phase B → Phase C transparently.
- Does NOT introduce a "Sugar shim registry" central infrastructure. The catalog is the union of every published shim across every ecosystem; federation is the ecosystem's existing infrastructure.

## §8. Cross-references

- Paper 04 §4.1 (rank-N tuple pinning model): `docs/papers/04-vertical-stack-and-standardization.md`.
- Paper 14 §L6 (CVE blast-radius is SELECT over content-addressed facts): `docs/papers/14-after-trust-the-universal-correctness-bundle.md`.
- Paper 22 (migration as source transformation; the workflow shims participate in): `docs/papers/22-after-vendoring-migration-as-source-transformation.md`.
- Paper 23 §6 (proof envelope carries the binding; four admission tiers): `docs/papers/23-after-packages-the-proof-envelope-carries-the-binding.md`.
- Project memory `project_sugar_pin_all_three`: k(I)=t requires all three pinned per run.
- Project memory `reference_sugar_provenance_keys`: substrate's signing key infrastructure.
- Rules of engagement: `docs/explanation/substrate-uniform-pattern.md`.
- Parent audit row D13a: `docs/audits/2026-05-18-kit-as-substrate-participant-vision.md` section 6.
- Phase A/B/C distribution evolution: parent audit section 7.1.
- Refuse-leg ruling: `docs/plans/2026-05-18-refuse-leg-short-circuit-ruling.md`.
- Existing kit-dispatch tiered resolution: `implementations/rust/sugar-cli/src/kit_dispatch.rs:521-583`.

## §9. Discipline

This ruling formalizes the answers to four ecosystem-specific architect calls; it does NOT extend the substrate's primitive set. Every mechanism in this ruling routes through existing substrate machinery:

- Naming convention is a packaging convention; ecosystem-natural.
- `.proof` location is kit-declared; substrate respects.
- Versioning is multi-pinning per existing rank-N tuple model.
- Trust is consumer-policy-driven per paper 23's admission tiers.

If a future shim-distribution PR proposes new substrate machinery, STOP and re-read this ruling + the substrate-uniform-pattern doc.
