# Bug Zoo v0 Design

Date: 2026-05-06
Status: Draft for review

## Purpose

Bug Zoo is a reproducible laboratory of bug species. Its job is to turn broad ProvekIt claims into concrete receipts:

1. a specimen-native library or project looks plausible and passes its normal gate;
2. that specimen's own kit/lifter lifts contracts already latent in the specimen's code, framework annotations, schemas, tests, or metadata through the kit RPC boundary;
3. the Rust `provekit` CLI orchestrates the specimen and reports the missing `p => q` edge as a red squiggle or build refusal;
4. an optional dropper pass independently emits a native-language edge-closing shape and re-verifies closure.

Bug Zoo is not a patch museum. It does not lead with vulnerable-versus-fixed diffs, and it does not grade droppers by whether they match the historical remediation. The primary artifact is vulnerable-and-exposed: the same code engineers might ship, caught by ProvekIt before it ships. When a dropped specimen exists, the claim is that ProvekIt discovered the missing obligation and synthesized a verified closure from the exposed shape itself.

The adoption claim is just as important as the verification claim: ProvekIt-native contracts are a reference surface, not a required authoring style. The zoo should show that many contracts were already built into ordinary code through frameworks and libraries; ProvekIt turns those latent contracts into universal ProofIR and shows where the graph stops composing.

The deeper claim is that the exposed edge becomes universal, comparable, solvable, translatable, content-addressable, and signable. A Spring annotation, a ProvekIt-native annotation, an OpenAPI schema, a Zod validator, and a historical OSS commit can all name the same contract edge once lifted. The edge has canonical bytes, a CID, and signing semantics, so it can move across language domains, repositories, package ecosystems, commits, and time without becoming folklore again.

ProofIR is allowed to be lossy. It is not a universal language for re-expressing every implementation detail of every programming language. It is a universal language for contract boundaries: preconditions, postconditions, invariants, protocol obligations, value predicates, resource states, signer claims, and the implication edges that connect them. That boundary language looks like first-order logic because contract composition is predicate composition: given `p`, can this edge establish required `q`?

## Core Terms

**Species.** A recurring bug shape reducible to predicates and an implication obligation. Example: SQL identifier injection is a species where some boundary predicate reaches a SQL identifier sink without proving `safe_sql_identifier`.

**Lab specimen.** A controlled, runnable fixture that isolates a species. It should fit in one or two pages, but it must look like realistic production-shaped code or metadata rather than a toy line.

**Specimen tooling.** The build files, harnesses, lifter entrypoint, and kit RPC adapter that belong to one specimen. This tooling lives inside the specimen, not in a central zoo library. A Java specimen may expose the same species through ProvekIt-native annotations, Spring, Bean Validation, Swagger, JML, or another realistic Java surface. The important thing is that each surface is lifted by the Java kit and compared through ProofIR.

**Surface-equivalent exposure.** Two or more exposed variants of the same species that express the same contract in different source surfaces and lift to identical canonical ProofIR. Example: a ProvekIt-native Java annotation and a Spring request annotation can be separate exposed variants; the zoo should show their lifters produce the same contract bytes/CID before checking the missing edge.

**Boundary-preserving lift.** A lift that may discard host-language implementation detail while preserving the contract boundary needed for verification. This is the key to cross-domain equivalence: two very different source artifacts can collapse to the same ProofIR when they assert the same boundary predicate.

**Wild specimen.** Real OSS code pinned at a vulnerable commit, with an exposure artifact showing ProvekIt reports the same missing edge. A historical remediation may be linked for context, but it is neither the proof nor the target. The exposure is the proof.

**Exposed.** The state where ProvekIt has lifted the specimen and emitted the red squiggle/build refusal. This includes expected ProofIR, missing edge, diagnostic location, and SAT witness or countercondition when available.

**Dropped.** The state after a dropper emits a native edge-closing shape and ProvekIt re-lifts and verifies closure. "Dropped" is not "fixed": it means the missing contract edge was discharged by a generated native-language shape and confirmed by the substrate, independent of whatever remediation a maintainer eventually chose.

## Taxonomy

Bug Zoo v0 has two top-level kingdoms.

### Shape Bugs

The graph does not compose. These are ProvekIt's direct structural catches:

- missing implication edge;
- wrong pin rank;
- wrong pin dimension;
- unsatisfied sink precondition;
- unresolved bridge;
- malformed attestation;
- protocol shape mismatch.

These bugs are red-squiggle candidates. The code/config may run, but the ProofIR graph fails.

### Value Bugs

The graph shape composes, but a signed value, axiom, contract, or policy is wrong. ProvekIt cannot make false values true; it makes the false claim explicit, attributable, revocable, and linkable to downstream failures.

Examples:

- OpenAPI claims a value is safe, but the implementation does not enforce it;
- a trusted signer signs a bad reference contract;
- a boundary axiom overstates what an upstream service guarantees;
- a policy memento accepts an overbroad signer set.

Value bugs teach the boundary of the claim: ProvekIt exposes structure and provenance; it does not turn bad axioms into good ones.

## v0 Species Pack

This section is the original expansion sketch, not the current checked-in ID
registry. The implemented null-boundary pack now uses `BZ-SHAPE-005` for Java,
`BZ-SHAPE-006` for TypeScript, and `BZ-SHAPE-007` for C#.

Bug Zoo v0 should start with a mixed receipt pack rather than a single language or single domain.

1. **BZ-SHAPE-001: SQL Identifier Injection**
   - Surface: TypeScript + Zod + Express.
   - Shape: a query parameter is validated for shape but flows into an SQL identifier position.
   - Missing edge: `validated_query_param(sort) => safe_sql_identifier(sort)`.

2. **BZ-SHAPE-002: Path Traversal**
   - Surface: Python + FastAPI + Pydantic.
   - Shape: filename passes string/shape validation but reaches a file-read sink without confinement.
   - Missing edge: `validated_path_param(name) => confined_path(name)`.

3. **BZ-SHAPE-003: Missing Authorization**
   - Surface: OpenAPI plus implementation.
   - Shape: endpoint establishes authentication but sink requires resource-specific authorization.
   - Missing edge: `authenticated(user) => authorized(user, resource)`.

4. **BZ-SHAPE-004: Header or Log Injection**
   - Surface: Java Bean Validation or TypeScript.
   - Shape: bounded/non-null string flows into a header/log sink without sink-specific safety.
   - Missing edge: `bounded_string(x) => safe_for_header(x)` or `safe_for_log(x)`.

5. **BZ-SHAPE-005: Optional or Null Misuse**
   - Surface: TypeScript, Java, or Rust where the species is natural.
   - Shape: optional/maybe-null value reaches a dereference/use sink.
   - Missing edge: `maybe_null(x) => non_null(x)`.

6. **BZ-SHAPE-006: Typestate or Resource Edge**
   - Surface: Rust or Go.
   - Shape: operation requires `resource_open`, `transaction_active`, or `lock_held`; caller has weaker state.
   - Missing edge: weaker state to required typestate predicate.

7. **BZ-SHAPE-007: Wrong Pin Rank**
   - Surface: `.proof` or package metadata.
   - Shape: binary is pinned without the complete contract/witness/binary tuple.
   - Missing relation: rank-1 pin is used where a rank-3 assertion is required.

8. **BZ-VALUE-001: Bad Boundary Axiom**
   - Surface: OpenAPI/schema plus implementation.
   - Shape: graph composes against an asserted boundary claim, but the boundary claim is false or unenforced.
   - Exposure: not a missing edge; the bad value is explicit, signed, attributable, and linked to the relying proof.

## Repository Layout

Bug Zoo is executable evidence, so it should live at the repository root, not only under `docs/`. The root zoo may contain shared README material, but it must not become the place where language-specific truth lives. Each specimen owns the library, harness, and lifter surface needed to make the bug species real.

```text
bug-zoo/
  README.md
  species/
    BZ-SHAPE-001-sql-identifier-injection/
      README.md
      specimen.yaml
      lab/
        library/
          package manifest and source for the vulnerable specimen-native library
        harness/
          code that imports the library and exercises the vulnerable shape
        kit-rpc/
          specimen-owned lifter command or adapter manifest
      exposed/
        provekit-native/
          harness/
            code that depends on the same lab library and exercises the ProvekIt reference surface
          kit-rpc/
            specimen-owned lifter command or adapter manifest
          expected.proofir.json
          expected-diagnostic.txt
        ecosystem-native/
          harness/
            code that depends on the same lab library and exercises an existing framework surface
          kit-rpc/
            specimen-owned lifter command or adapter manifest
          expected.proofir.json
          expected-diagnostic.txt
        equivalence.json
        sat-witness.json
      dropped/
        library/
          generated native edge-closing shape, when a verified dropper exists
        harness/
          code that depends on the dropped library and re-runs the lifter
        closure.proofir.json
        verify-output.txt
      wild/
        README.md
```

For a TypeScript specimen, `library/` might contain `package.json`, `src/app.ts`, and tests. For a Java specimen, it might contain `pom.xml` or `build.gradle`, one ProvekIt-native exposure, one Spring or Bean Validation exposure, source files, and a Java kit RPC entrypoint. For an OpenAPI specimen, the "library" can be the protocol artifact itself plus the harness that consumes it.

The Rust CLI is the orchestrator for all of this. It reads `specimen.yaml`, runs the declared host command, invokes the declared lifter through the kit RPC protocol, and compares the resulting ProofIR/diagnostic behavior to the specimen's exposed or dropped expectations.

This direction intentionally replaces a central TypeScript zoo tool with specimen-owned tooling plus Rust CLI orchestration. A specimen should be portable as a small native project; the zoo runner should be able to treat it as a black box.

Older sketches of the layout used a central `bug-zoo/tools/` directory. v0 should avoid that for language behavior. Shared root tooling is allowed only for documentation helpers or schema linting that does not know how any particular language works.

Canonical specimen state:

```text
bug-zoo/species/BZ-SHAPE-001-sql-identifier-injection/
  specimen.yaml
  lab/
    library/
    harness/
    kit-rpc/
  exposed/
    provekit-native/
      harness/
      kit-rpc/
      expected.proofir.json
      expected-diagnostic.txt
      sat-witness.json
    framework-native/
      harness/
      kit-rpc/
      expected.proofir.json
      expected-diagnostic.txt
      sat-witness.json
    equivalence.json
  dropped/
    library/
    harness/
    closure.proofir.json
    verify-output.txt
  wild/
    README.md
```

`lab/` is required. `exposed/` is required. Each species needs at least one exposed variant; species with a meaningful framework/library surface should prefer two variants: one ProvekIt-native reference surface and one ecosystem-native surface. `dropped/` is optional in v0 and required only for species where a verified dropper exists. `wild/` is optional per species at first, but every v0 species should have room for at least one wild specimen. Each wild child directory must be named from a real advisory ID and project slug, and the validator must reject entries whose advisory URL, commit hash, or affected path cannot be resolved.

When a specimen has multiple exposed variants, `exposed/equivalence.json` records the canonical ProofIR CIDs that must match. This is a first-class receipt: the zoo demonstrates that ProvekIt did not require a new authoring style, only a lifter that could recognize the contract already present in code. The equivalence is intentionally at the contract boundary, not at the implementation level.

Example TypeScript-shaped lab internals:

```text
lab/
  library/
    package.json
    src/
      app.ts
    tests/
      app.test.ts
  harness/
    package.json
    src/
      exercise.ts
  kit-rpc/
    manifest.json
```

Example Java-shaped lab internals:

```text
lab/
  library/
    pom.xml
    src/main/java/
      example/
        ReportController.java
        SortParam.java
  harness/
    pom.xml
    src/test/java/
      example/
        ReportControllerTest.java
  kit-rpc/
    manifest.json
```

Example Java-shaped exposed variants:

```text
exposed/
  provekit-native/
    harness/
      pom.xml
      src/main/java/example/ReportController.java
    kit-rpc/
      manifest.toml
    expected.proofir.json
    expected-diagnostic.txt
  spring-web/
    harness/
      pom.xml
      src/main/java/example/ReportController.java
    kit-rpc/
      manifest.toml
    expected.proofir.json
    expected-diagnostic.txt
  equivalence.json
```

The exact files differ by language, but the dependency direction does not: harnesses depend on the specimen library; the Rust CLI invokes the lifter via RPC; ProvekIt does not import specimen internals as a central library.

Obsolete layout, kept here only as a warning against the wrong dependency direction:

```text
do not build:
bug-zoo/
  tools/
    validate-specimens.ts
    run-lab.ts
```

## Specimen Contract

Every `specimen.yaml` should include the minimum data needed to validate and present the specimen.

```yaml
id: BZ-SHAPE-001
name: SQL Identifier Injection
kingdom: shape
surface: typescript-zod-express
status: lab
paths:
  labLibrary: lab/library
  labHarness: lab/harness
  labKitRpc: lab/kit-rpc
exposures:
  - id: provekit-native
    surface: typescript-provekit-native
    harness: exposed/provekit-native/harness
    kitRpc: exposed/provekit-native/kit-rpc
    liftRpc:
      cwd: exposed/provekit-native/kit-rpc
      argv: ["pnpm", "start", "--", "--rpc", "../harness"]
    proofIrFile: exposed/provekit-native/expected.proofir.json
    diagnosticFile: exposed/provekit-native/expected-diagnostic.txt
  - id: zod-express
    surface: typescript-zod-express
    harness: exposed/zod-express/harness
    kitRpc: exposed/zod-express/kit-rpc
    liftRpc:
      cwd: exposed/zod-express/kit-rpc
      argv: ["pnpm", "start", "--", "--rpc", "../harness"]
    proofIrFile: exposed/zod-express/expected.proofir.json
    diagnosticFile: exposed/zod-express/expected-diagnostic.txt
equivalence:
  required:
    - [provekit-native, zod-express]
commands:
  hostCheck:
    cwd: lab/library
    argv: ["pnpm", "test"]
predicates:
  boundary: validated_query_param(sort)
  sink: safe_sql_identifier(sort)
  missingEdge: validated_query_param(sort) => safe_sql_identifier(sort)
expectations:
  hostCompiler: pass
  ordinaryTests: pass
  provekitVerify: fail
exposure:
  satWitnessFile: exposed/sat-witness.json
dropper:
  available: false
wildSightings: []
```

The command shape is intentionally generic so Java, Python, TypeScript, Rust, OpenAPI, and metadata specimens can remain native. The Rust CLI should understand command execution, kit RPC, and canonical ProofIR equivalence; it should not understand Spring, Zod, Maven, or Swagger semantics.

Wild specimens extend this with pinned upstream context. A wild `specimen.yaml` must include exact advisory ID, project slug, vulnerable commit, affected paths, advisory URL, source URL, species ID, exposure artifact paths, and verdict fields for `provekitExposes` and `dropperAvailable`. Historical remediation links are optional context only. No fake or unresolved wild entry should merge.

## Validation Rules

The v0 validator should enforce structure first, then execution as support lands.

Required structural checks:

- every species has `README.md`, `specimen.yaml`, `lab/library/`, `lab/harness/`, `lab/kit-rpc/`, and `exposed/`;
- every listed exposure has its own `harness/`, `kit-rpc/`, `expected.proofir.json`, and `expected-diagnostic.txt`;
- species IDs are unique and stable;
- shape species define `boundary`, `sink`, and `missingEdge`;
- value species define the bad value/axiom and the proof or consumer that relies on it;
- every command has `cwd` and `argv`, with `cwd` resolving inside the specimen;
- every exposure describes which host-language details were erased and which boundary predicates were preserved;
- each exposure diagnostic names the red-squiggle location and missing edge;
- every `equivalence.required` pair names known exposure IDs;
- `dropped/` may not exist unless it includes closure evidence.

Execution checks, added incrementally:

- run the specimen's ordinary host command and confirm pass;
- invoke each exposure's lifter through the kit RPC command declared in `specimen.yaml`;
- confirm required exposure pairs produce identical canonical ProofIR CIDs;
- confirm exposure equivalence compares boundary predicates, not host implementation details;
- run `provekit verify` through the Rust CLI orchestration and confirm fail for `lab/`;
- when `dropped/` exists, invoke the dropped harness/lifter and confirm `provekit verify` passes;
- for wild specimens, verify the pinned upstream commit exists and the affected file path resolves;
- never compare dropped output to the historical remediation as an acceptance criterion.

## Dropper Relationship

Bug Zoo v0 is primarily an exposure suite. Droppers are a third-stage demonstration:

1. specimen-native lifter exposes missing edge through RPC;
2. Rust CLI verifier refuses;
3. dropper emits edge-closing native shape in the specimen's own language or artifact format;
4. Rust CLI re-invokes the specimen lifter and confirms closure.

The dropper is not trusted by itself. A dropped specimen is accepted only when the verifier confirms the dropped output closes the graph. This mirrors the existing Rust dropper pipeline in `implementations/rust/provekit-walk/src/dropper/`.

The historical remediation for a wild bug is not an oracle. It can help a reader understand why the bug mattered, but it must not drive specimen construction, validator success, or dropper scoring. The stronger claim is independent rediscovery and independent closure.

## Non-Goals

- Bug Zoo v0 does not attempt broad CVE coverage.
- Bug Zoo v0 does not require every species to have a dropper.
- Bug Zoo v0 does not claim all bugs are covered.
- Bug Zoo v0 does not treat historical patches as proof.
- Bug Zoo v0 does not require or prefer the same remediation that landed upstream.
- Bug Zoo v0 does not require vendoring full upstream OSS repositories.

## Success Criteria

Bug Zoo v0 succeeds when:

- at least eight species are represented by minimal realistic lab fixtures;
- each lab fixture has a clear exposed ProofIR/missing-edge artifact;
- at least one species has two exposed variants where a ProvekIt-native surface and an ecosystem-native surface lift to identical canonical ProofIR;
- at least two surfaces are non-code artifacts, such as OpenAPI or `.proof` metadata;
- at least one species has a dropped variant verified by re-lift;
- at least one wild specimen pins real OSS vulnerable code and shows exposure;
- `bug-zoo/README.md` explains the lifecycle in one screen: lab, exposed, dropped, wild;
- the Rust CLI can run a specimen without importing language-specific specimen code into a central zoo package.

## Implementation Decisions For v0

- First implementation target: `BZ-SHAPE-005: Optional or Null Misuse` on Java, because the repository already has Java lifter surfaces for ProvekIt-style contracts, Bean Validation, JML, Cofoja, and Spring Web. This first specimen should prove the corrected architecture: ordinary framework contracts and explicit contract surfaces lift to the same boundary ProofIR, then the missing `maybe_null(x) => non_null(x)` edge is exposed by Rust CLI orchestration.
- Specimen metadata format: YAML, because the files are meant to be read as exhibit labels as much as machine manifests.
- Wild specimen storage: store minimal extracts plus exact upstream commit metadata and source links; fetch full upstream repositories only in optional validation paths.
- Runner architecture: implement a Rust CLI subcommand that reads specimen manifests, executes specimen-declared host checks, invokes each specimen-declared exposure through kit RPC, compares canonical ProofIR equivalence where required, and checks exposed/dropped expectations.
- Language boundary: specimens own lifters and language tooling. Rust CLI orchestration may call RPC and compare outputs, but it must not become a TypeScript, Java, OpenAPI, or Rust specimen library.
- Exposure strategy: prefer paired surfaces when available. One variant may use a ProvekIt-native contract surface as the reference; another should use the ecosystem contract surface engineers already have, such as Spring, Bean Validation, Swagger/OpenAPI, Zod, Pydantic, JML, or framework metadata.
- ProofIR scope: treat ProofIR as boundary-preserving and deliberately lossy. The implementation plan should avoid any goal that requires reconstructing host-language semantics beyond the predicates and edges needed to verify the specimen.
- Dropped phase: include it only where a verified dropper exists. For v0 that means the Rust not-null dropper path is eligible; non-Rust species may remain exposure-only until their droppers exist.
