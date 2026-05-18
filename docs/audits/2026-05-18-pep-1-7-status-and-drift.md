# PEP 1.7.0 status + drift audit

Date: 2026-05-18
Spec: `protocol/specs/2026-05-12-plugin-protocol.md` (662 lines)
Amendments tracked:
- #1108 / PR #1114 (merged 2026-05-17): exam_manifest_cid federation handshake -- adds optional `exam_manifest_cid` and `exam_manifest_set` to `PluginRegistryMemento` header (§9.1, §9.3, §10.1, §10.2)
- #1123 / PR #1127 (merged 2026-05-17): minted boundary-contract catalog (sibling to concepts; does not edit the plugin spec but supplies one of the catalogs `concept-extension` / `realizer` plugins will consume)
- Trinity floor completion (#1024 path B, today): each Trinity-realize kit declares its `library_tag` via the realize-manifest convention (`.provekit/realize/<surface>/manifest.toml`)

Scope: this audit is READ-ONLY on substrate code. The only file written is this audit.

## Headline finding

PEP 1.7.0 is implemented as TWO parallel surfaces that do not meet:

1. **`provekit-plugin-loader` + `cmd_plugin.rs`** -- a faithful PEP 1.7.0 implementation (§1, §3, §4, §6, §7, §8, §9). It accepts `--plugin <kind>:<source>` flags, loads JSON files or stdio-RPC plugins, validates plugin-memento headers, computes/verifies CIDs, seals `PluginRegistryMemento`. Carries zero default plugins (`cmd_plugin.rs` line 367: "v0 ships zero default plugins; the flags are parsed but are no-ops"). `provekit-plugin-loader/src/lib.rs` lines 13-15 state explicitly: "Consumer plugin crates ... will depend on this crate" (future tense).

2. **`kit_dispatch.rs`** -- the lift / realize / exam-manifest dispatcher used by every actual subcommand (`cmd_bind`, `cmd_lower`, `cmd_exam`, `cmd_lift`). It has its own `parse_manifest` (line 343) reading `.provekit/lift/<lang>/manifest.toml` and `.provekit/realize/<surface>/manifest.toml` by filesystem convention, then spawns the child process directly via `Command::new`. It IMPORTS `provekit_plugin_loader::{PluginRegistry, PluginRegistryMemento}` (line 52) but uses them ONLY in `seal_plugin_registry_for_project` / `federate_plugin_registries` / `configured_exam_manifest_cid` (lines 1073-1131) -- i.e., to mint and federate the registry memento. It NEVER consults the registry when choosing which kit to dispatch.

Consequence: every kit currently dispatched by the CLI is dispatched by the legacy `.provekit/<role>/<surface>/manifest.toml` convention path, not via the PEP 1.7.0 `--plugin` flag surface. The plugin-memento CID computed by the loader is not used to select kits. Conformance with §6.2 ("delivery does not affect CID; federation by reference") is therefore an unobserved property: no kit is loaded "by CID" today.

## Spec section status

| § | Topic | Status | Evidence |
|---|---|---|---|
| §0.1, §0.2, §0.3 | Trichotomy + scope-out | SPECIFIED | -- |
| §0.4 | Legacy token migration window (`provekit-agent/1`, `provekit-lift/1` -> `pep/1.7.0`) | PARTIAL | Loader accepts only `pep/1.7.0` (RUNTIME_PROTOCOL_VERSIONS = `["pep/1.7.0"]`, `loader.rs` line 21). Legacy tokens NOT accepted with `deprecated-protocol-identifier` reason; §0.4 says current minor SHOULD accept both. Producer-side emission compliant (every lift manifest emits `protocol_version = "pep/1.7.0"`). |
| §1.1 | Plugin memento CDDL | IMPLEMENTED | `provekit-plugin-loader/src/types.rs` (180 lines) |
| §2.1 | Canonical kind labels | PARTIAL | Loader is kind-agnostic at validation (§1.2 "Validators of this protocol MUST NOT validate the inner shape"). The CLI surface (`cmd_plugin.rs` lines 117-131) hardwires aliases for `sugar`, `loss-function`, `lifter`. No code path for `agent`, `realizer`, `effect-signature`, `concept-extension`, `pattern-predicate`, `bundle-attestation`, `ir-extension`. The `exam-manifest` kind appears in `kit_dispatch.rs` line 1017 (`EXAM_MANIFEST_KIND`) but is NOT in the §2.1 table -- intentional open-enum extension (§2.1 "validators MUST accept unknown kinds at shape level"), but worth noting that the federation amendment (#1108/#1114) added the `exam_manifest_cid` registry field WITHOUT promoting `exam-manifest` into the canonical §2.1 row set. |
| §3 | File interface | IMPLEMENTED | `provekit-plugin-loader/src/loader.rs::load_plugin_from_file` |
| §3.1 | CLI flag form | IMPLEMENTED | `cmd_plugin.rs::PluginFlags` (440 lines) with B4 argv-order recovery via `indices_of` |
| §3.2 | Multi-load order | IMPLEMENTED | `cmd_plugin.rs::ordered_plugins`, `registry.rs::load_order` |
| §4.1 | Endpoint grammar | PARTIAL | `stdio:` implemented (`loader.rs::load_plugin_from_stdio_rpc`). HTTP/TCP stubbed (`loader.rs` line 78: "http/tcp rpc not yet implemented; use stdio: form"). |
| §4.2.1 | `provekit.plugin.describe` | IMPLEMENTED | `loader.rs` line 117 (JSON-RPC over stdio) |
| §4.2.2 | `provekit.plugin.invoke` | PARTIAL | Loader treats invoke as kind-specific. `kit_dispatch.rs::invoke_realize` (line 852), `kit_dispatch.rs::invoke_exam_manifest` (line 1259), `kit_dispatch.rs::rpc_lift` (line 433) all CALL `provekit.plugin.invoke` over stdio against kit binaries directly. So invoke is implemented per-role, but invocation does not flow through the loader-built registry. |
| §4.2.3 | `provekit.plugin.shutdown` | UNIMPLEMENTED | No grep hit for `provekit.plugin.shutdown` anywhere outside the spec. Child processes are killed after one request (`loader.rs` line 155; `kit_dispatch.rs` line 1309). |
| §4.3 | Error model on the wire | IMPLEMENTED | `loader.rs` lines 170-179 |
| §5 | Version negotiation | IMPLEMENTED | `loader.rs::parse_and_validate` line 236-248 |
| §6.1 | CID construction | IMPLEMENTED | `provekit-plugin-loader/src/cid.rs::compute_plugin_cid` (234 lines: JCS canonicalization + BLAKE3-512) |
| §6.2 | CID-delivery-independence | IMPLEMENTED | Tests at `provekit-plugin-loader/tests/` exercise file+rpc parity |
| §6.3 | Built-in plugins | UNIMPLEMENTED | `lib.rs` line 13: "This crate is loader infrastructure only. ... built-in plugin kinds (§2.1) have no concrete implementations in this PR." `cmd_plugin.rs` line 367: zero defaults shipped. |
| §7 | CLI flag conventions | PARTIAL | `--plugin`, `--sugar`, `--loss-function` (alias `--loss-fn`), `--lifter`, `--no-default-plugins`, `--no-default-plugin <kind>`, `--strict-plugins`, `--plugin-registry-out` all wired. The defaults-suppression flags are NO-OPS in v0 (no defaults to suppress). |
| §8 | `PluginLoadFailureMemento` | IMPLEMENTED | `registry.rs::mint_failure_memento` line 273; full failure-reason-kind enum at `types.rs::FailureReasonKind`. |
| §9 | Registry semantics | IMPLEMENTED + AMENDED | `registry.rs::PluginRegistry`, `emit_registry_memento`. AMENDED per #1108/#1114: `exam_manifest_cid` + `exam_manifest_set` optional fields in `PluginRegistryMementoHeader` (lines 44-48). |
| §9.4 | Provenance propagation | UNIMPLEMENTED | No grep hit demonstrating output mementos cite the registry CID. `provekit-walk` and other emit paths produce signed mementos without referencing a `PluginRegistryMemento.cid`. |
| §10 | Federation handshake | IMPLEMENTED | `kit_dispatch.rs::federate_plugin_registries` line 1094; `exam-manifest-mismatch` refusal path. |
| §11 | Federation mechanics | SPECIFIED, not exercised | No cross-runtime federation test demonstrated. |
| §12 | Cross-references | SPECIFIED | -- |
| §13 | Versioning cadence | SPECIFIED | -- |
| §14 | Blessing | UNIMPLEMENTED | `protocol/blessings/pep-1.7.0.json` does NOT exist. Spec at §14.4 says "actual signed instance for `pep/1.7.0` is minted in a follow-up CI step." Verifier-side requirement (§14.4 steps 1-5) has no code. |
| §15 | Trinity-convergence invariants | IMPLEMENTED in #748 plumbing | Trinity round-trip test exists per #748 acceptance; full address-space-stability + concept-space-stability assertions live in test code, not as a substrate-level verifier. |

## Per-kit implementation

Twelve kits exist on disk under `implementations/{c,cpp,csharp,go,java,php,python,ruby,rust,swift,typescript,zig}`. Each kit ships one or more `.provekit/lift/<surface>/manifest.toml` files. None of the twelve is "registered as a PEP 1.7.0 plugin" in the §1.1 sense (no kit is loaded via `--plugin` and validated against the §6.1 CID). All twelve are dispatched via the legacy `kit_dispatch::parse_manifest` convention path.

Columns:
- **manifest** -- count of `.provekit/lift/<surface>/manifest.toml` files for the kit (excluding `provekit-lift-openapi/` and `provekit-lift-asm-x86-64/` which are not kit-scoped).
- **proto** -- `Y` if the kit's primary surface manifest declares `protocol_version = "pep/1.7.0"`; `N` if no declaration; `P` if some surfaces declare it and others (the legacy `<lang>/manifest.toml`) do not.
- **lift** -- the kit's source-to-IR plugin path. Y = present + dispatch works via `kit_dispatch::dispatch_bind_lift`.
- **realize** -- whether `.provekit/realize/<surface>/manifest.toml` references a kit binary. Trinity floor (today) populated this for c, java, python, rust, typescript.
- **invoke** -- whether the kit binary speaks `provekit.plugin.invoke` over stdio.
- **describe** -- whether the kit binary speaks `provekit.plugin.describe` (required by §4.2.1; never actually called from the legacy dispatch path).
- **library_tag** -- whether the realize manifest declares `library_tag` (Trinity floor #1024 requirement).

| kit | manifest | proto | lift | realize | invoke | describe | library_tag |
|---|---|---|---|---|---|---|---|
| c | 5 (c, c-assertions, c-kernel-doc, c-self-contracts, c-sparse) | P | Y | Y | Y | UNKNOWN | Y (`libcurl`) |
| cpp | 3 (cpp, cpp-self-contracts, cpp-source) | P | Y | N | UNKNOWN | UNKNOWN | N |
| csharp | 3 (clr-bytecode, csharp, csharp-source) | P | Y | N | UNKNOWN | UNKNOWN | N |
| go | 3 (go, go-self-contracts, go-source) | P | Y | N | UNKNOWN | UNKNOWN | N |
| java | 3 (java, java-self-contracts, java-source) | P | Y | Y | Y | UNKNOWN | Y (`java-net-http`) |
| php | 3 (php, php-self-contracts, php-source) | P | UNKNOWN | N | UNKNOWN | UNKNOWN | N |
| python | 3 (python, python-self-contracts, python-source) | P | Y | Y (4 surfaces: python, python-aiosqlite, python-requests, python-sqlite3) | Y | UNKNOWN | Y |
| ruby | 3 (ruby, ruby-self-contracts, ruby-source) | P | Y | N | UNKNOWN | UNKNOWN | N |
| rust | 4 (rust, rust-self-contracts, evm-bytecode, jvm-bytecode) | P | Y | Y | Y | UNKNOWN | Y (`reqwest`) |
| swift | 3 (swift, swift-self-contracts, swift-source) | P | Y | N | UNKNOWN | UNKNOWN | N |
| typescript | 3 (typescript, typescript-self-contracts, typescript-source) | P | Y | Y (3 surfaces: typescript, typescript-better-sqlite3, typescript-pg) | Y | UNKNOWN | Y (`fetch`) |
| zig | 3 (zig, zig-self-contracts, zig-source) | P | Y | N | UNKNOWN | UNKNOWN | N |

The **P** in the `proto` column is universal: every kit has the legacy `<lang>/manifest.toml` lacking `protocol_version` and `kind`, plus the `<lang>-source/` and `<lang>-self-contracts/` variants that DO declare them. Sample drift evidence:

- `implementations/typescript/.provekit/lift/typescript/manifest.toml` -- 4 lines, no `protocol_version`, no `kind`.
- `implementations/typescript/.provekit/lift/typescript-source/manifest.toml` -- declares `protocol_version = "pep/1.7.0"`, `kind = "lift"`, `version`, `[capabilities]`.

`describe` is **UNKNOWN** for every kit. The conformance harness referenced by #746 has not been demonstrated to call `provekit.plugin.describe` against any kit binary. Without that, §6.2 (CID-delivery-independence) is unverified end-to-end: there is no test asserting that `CID(kit-as-file) == CID(kit-as-rpc)` for any kit's plugin memento.

## Drift findings

1. **Singular vs plural protocol-version field.** Every in-tree lift manifest declares `protocol_version = "pep/1.7.0"` (singular, scalar). `kit_dispatch::parse_manifest` (line 377) reads `protocol_versions` (plural, array). The legacy lift dispatch path NEVER reads either field; the version is parsed and stashed in `ParsedManifest.protocol_versions` but `validate_library_tag` / `resolve_lift_command` / `rpc_lift` ignore it. Only `validate_exam_manifest_plugin_manifest` (line 1224-1237) actually enforces the field. Net effect: if you ran the exam-manifest validator against a lift manifest, it would refuse with "manifest must declare protocol_versions = [pep/1.7.0]" -- because the validator expects the plural-array form and the lift manifests emit the singular-scalar form. This is two drifts stacked: (a) field-name shape, (b) the lift path doesn't enforce it at all.

   The spec position is ambiguous. §1.1 line 104 requires `protocol_versions: [+ protocol-version]` (array) on the plugin-memento HEADER. §0.4 emission rule says "emit `\"protocol_version\": \"pep/1.7.0\"` for legacy single-token fields, and `\"protocol_versions\": [\"pep/1.7.0\"]` for the §1.1 plugin-memento array form." The lift manifests are not plugin-memento headers; they are kit-resolution manifests. The spec does not normatively cover that file shape. The drift is therefore implementation-internal, not spec-violation, BUT it means the §0.4 producer-emission claim is invisible to the parser that reads these files.

2. **Realize manifests omit `protocol_version` and `kind` entirely.** Every `.provekit/realize/<surface>/manifest.toml` declares `name`, `library_tag` (Trinity floor), `command`, `working_dir`. None declares `protocol_version` or `kind`. Comments at the top of each say "PEP 1.7.0 `kind = \"realize\"` plugin manifest" -- the metadata claim lives in a comment, not in a parseable field. Same caveat as (1): the parser doesn't read it, but if §0.4's emission rule is meant to apply to these manifests, it does not.

3. **`PluginRegistry` is sealed but never consulted for dispatch.** `cmd_plugin::build_registry` walks the `--plugin` flag set, loads each plugin, computes CIDs, mints a `PluginRegistryMemento`. The memento can be written via `--plugin-registry-out`. But no subcommand consults `PluginRegistry::lookup` or `by_kind` to choose which kit to invoke. `kit_dispatch::dispatch_bind_lift` resolves the lift kit by filesystem convention (`.provekit/lift/<source_lang>/manifest.toml`); `dispatch_realize` resolves the realize kit by filesystem convention (`.provekit/realize/<target_lang>/manifest.toml` plus PATH env fallback). The CID-addressed loader is parallel infrastructure that is built and sealed but otherwise unused.

4. **§9.4 (provenance propagation) is unimplemented.** The spec says "every output memento's `provenance_cid` chain MUST resolve to a `ProvenanceMemento` whose `inputs` array contains the registry CID." Nothing in `provekit-walk`, `provekit-bind`, `provekit-ir`, or `provekit-realize-*` cites a `PluginRegistryMemento.cid` in its provenance. The audit-replay invariant of §9.4 is therefore not enforceable from current outputs.

5. **§14 (blessing memento) is unimplemented.** `protocol/blessings/pep-1.7.0.json` does not exist. The spec at §14.4 defers the signed instance to "a follow-up CI step." Verifier-side §14.4 steps 1-5 have no implementation. The "blessing covers `pep/1.7.0`" claim is therefore unsigned in-tree -- the blessing is documentary, not verifiable per its own §14.4.

6. **`provekit.plugin.shutdown` (§4.2.3) is not spoken.** All RPC sessions terminate by killing the child process after one `describe` or one `invoke` (`loader.rs::load_plugin_from_stdio_rpc` line 155; `kit_dispatch.rs::invoke_realize` line 1309). No graceful shutdown call is issued. Spec says SHOULD exit on stdin EOF; the binaries are not given the chance.

7. **HTTP/TCP transport stubs return refuse.** `loader.rs` line 78: HTTP/HTTPS/TCP endpoints return `LoadError::RpcError { detail: "http/tcp rpc not yet implemented; use stdio: form" }`. §4.1 grammar lists all four; only `stdio:` works. The §3 trichotomy says protocol-version mismatch is a refuse; "unsupported transport" is currently lumped into the same refuse path with no separate reason kind. This is implementation-incomplete, not spec-violating.

8. **Legacy token acceptance (§0.4 read-side) is not implemented.** `RUNTIME_PROTOCOL_VERSIONS = &["pep/1.7.0"]` (`loader.rs` line 21). `provekit-agent/1` and `provekit-lift/1` will fail with `ProtocolVersionMismatch`, not with the spec-required `deprecated-protocol-identifier` reason at `critical = false`. The current minor version is therefore stricter than §0.4 mandates; producers must already have migrated, and any legacy memento refused outright.

## PEP 1.8.0 candidate items

Issues explicitly self-tagged "Queue for PEP 1.8.0":

- **#750** `concept:try` / `catch` / `except` hub extension -- exception-handling control flow. Open architect-call: one op vs three, typed vs untyped catch.
- **#751** `concept:assert` hub extension -- assertion semantics straddle witness-runtime-check and canonical-compile-time-invariant. Open architect-call.
- **#752** `concept:alloc` hub extension -- standalone op vs implicit effect of `concept:new`. Open architect-call from PR #742.

Issues that AMEND PEP 1.7.0 in place (NOT 1.8.0 candidates -- they extend the canonical kind set under 1.7.x patch releases or refine existing surfaces):

- **#754** WitnessMemento canonical form -- explicitly "Scoped under PEP 1.7.0 as the `witness` kind." Add a row to §2.1 in a patch.
- **#755** runtime-mode emission sugars (`witness` / `monitor` / `emitter` / `gate`) -- selects from `concept:contract-observation` per #880. Body-template policy, not new protocol surface.
- **#756** tag carriers (source-visible CID audit trails + relift anchors) -- carrier-class spec; no new plugin kind.
- **#748** Trinity composition under `pep/1.7.0` -- inherited from kit migration, not a new kind.

Migration-wave issues (still open):

- **#732** -- umbrella, parent of the wave.
- **#743** -- "Migration: Rust CLI emit/consume pep/1.7.0." Largely landed: emission in PluginRegistryMemento, federation memento, exam-manifest validation all emit `pep/1.7.0`. Legacy token acceptance NOT landed (see drift #8).
- **#744** -- "Per-language kit emit/consume (12 kits, parallel)." Open. Acceptance is each kit emits and accepts `pep/1.7.0`. Current state: producer-side (manifest `protocol_version`) compliant on `*-source` and `*-self-contracts` surfaces; non-compliant on the bare `<lang>/manifest.toml`. Consumer-side (kit binaries actually validating `pep/1.7.0` on incoming RPC) is UNKNOWN because no in-tree test exercises it.
- **#745** -- "Bug zoo regen on pep/1.7.0." Open; not investigated here.
- **#746** -- "Bridgework + cross-language conformance harness." Open. Critical to closing the loop on per-kit `describe` and CID-delivery-independence (§6.2).
- **#747** -- "Final conformance sweep: cut the legacy cord." Open; gated on #744 + #746.
- **#760** -- "Trinity body emission redo: through provekit-walk + Java kit (federation-correct)." Open. Aims to retire the cmd_transport per-language match arms by moving emission into content-addressed sugar dicts.

The post-#1024 Trinity floor work (today, PR #1159) is a step toward #760 but does not close it: it adds `library_tag` to realize manifests but does not move per-language emission into sugar plugins.

## Recommended next dispatches

Concrete, per-issue, in dispatch order. Each is independent unless noted.

1. **Drift fix #1 (singular vs plural).** Either (a) update `kit_dispatch::parse_manifest` to accept both `protocol_version = "X"` (scalar -> single-element array) and `protocol_versions = ["X"]`, or (b) sweep every lift manifest to declare `protocol_versions = ["pep/1.7.0"]`. Spec is silent on the resolution-manifest shape; (a) preserves byte-stability of existing manifest files. Mechanical. Codex-dispatchable.

2. **Drift fix #2 (realize manifests omit field).** Add `protocol_version = "pep/1.7.0"` and `kind = "realize"` to every `.provekit/realize/<surface>/manifest.toml`. Ten files. Mechanical. Codex-dispatchable. Boy-scout: same patch can also fix the `<lang>/manifest.toml` legacy form drift if the singular-form decision lands.

3. **Drift fix #6 (graceful shutdown).** Send `provekit.plugin.shutdown` JSON-RPC before killing the child in `loader.rs::load_plugin_from_stdio_rpc` and `kit_dispatch.rs::{invoke_realize, invoke_exam_manifest, rpc_lift}`. Spec says SHOULD, not MUST; reasonable to defer.

4. **Drift fix #8 (legacy token acceptance).** Add `provekit-agent/1` and `provekit-lift/1` to `RUNTIME_PROTOCOL_VERSIONS`. When a legacy token is matched, mint a `PluginLoadFailureMemento` with `reason_kind = "deprecated-protocol-identifier"` and `critical = false`, but DO NOT refuse. Spec §0.4 mandates this for the current minor. Codex-dispatchable but needs test fixtures with legacy mementos.

5. **#754 (WitnessMemento canonical form).** Adds the `witness` row to §2.1. Substrate work in `provekit-ir-types`. Needs deep Opus review per the issue body. Should land before #755 (which depends on witness mementos as the runtime evidence).

6. **#746 (bridgework + conformance harness).** Test infrastructure asserting per-kit `provekit.plugin.describe` works, and asserting `CID(kit-as-file) == CID(kit-as-rpc)`. Closes the §6.2 unverified gap. Necessary precondition to #747 cutting the legacy cord.

7. **#760 (cmd_transport refactor to sugar plugins).** The substrate fix for drift #3 (registry is sealed but unused). Trinity body emission redo. Large; needs spec for sugar-dict consumer shape (already exists per #735/#736/#737), then move per-language emission into sugar dicts, then have `cmd_bind` consult `PluginRegistry::by_kind("sugar")` instead of match arms. Architect-track.

8. **§9.4 provenance propagation.** Add `PluginRegistryMemento.cid` to every output memento's `ProvenanceMemento.inputs` array. Touches every emit path. Should land alongside or after #746 (so the registry CID actually corresponds to a verified set of kit CIDs).

9. **§14 blessing memento.** CI mint of `protocol/blessings/pep-1.7.0.json` signed with the substrate maintainer Ed25519 key (per `reference_provekit_provenance_keys`). Verifier-side §14.4 steps 1-5 in `provekit-cli`. Low-risk, high signal: makes the "pep/1.7.0 is blessed" claim auditable.

10. **PEP 1.8.0 architect-call sequence: #752 (alloc), then #750 (try/catch), then #751 (assert).** Each needs a Sir architect-call before queue-add. None are blocked by remaining 1.7.x work. Order is from least-invasive (alloc may stay implicit) to most-invasive (try/catch reshapes control flow).

## Summary

PEP 1.7.0's loader infrastructure is faithful to the spec and well-tested at the unit level. The federation handshake amendment (#1108/#1114) landed cleanly. The kit-dispatch path that actually runs lift/realize/exam in every CLI subcommand is the legacy filesystem-convention path; it does not consult the loader-built registry. Closing that gap (drift #3) is the single largest 1.7.x work item, and #760 is the issue that names it. Producer-side compliance (manifests declaring `protocol_version = "pep/1.7.0"`) is at "most surfaces, not all" -- the bare `<lang>/manifest.toml` and every `.provekit/realize/<surface>/manifest.toml` declare neither `protocol_version` nor `kind`. The blessing memento is undocumented in-tree.

Total open PEP-related issues: 12 (#732, #744, #745, #746, #747, #748, #750, #751, #752, #754, #755, #756, #760, less #743 which is largely landed -- 13 actually). Of those, three are explicit 1.8.0 candidates (#750, #751, #752); the rest are 1.7.x work.
