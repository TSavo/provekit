# Bug Zoo Null-Boundary Exhibits Design

Date: 2026-05-07
Status: Updated scope

## Purpose

The checked-in null-boundary Bug Zoo pack currently treats Java, TypeScript, and
C# as separate species:

- `BZ-SHAPE-005`: Java null boundary.
- `BZ-SHAPE-006`: TypeScript null boundary.
- `BZ-SHAPE-007`: C# null boundary.

That overstates the taxonomy. All three entries expose the same bug species:
a maybe-null value reaches a boundary that requires non-null.

```text
maybe_null(name) => non_null(name)
```

They also intentionally lift to the same canonical ProofIR CID. The languages
are not different species; they are different exhibits of one species.

## Decision

Keep one semantic species:

```text
BZ-SHAPE-005: Null Boundary Equivalence
```

Model Java, TypeScript, and C# as language-scoped exhibits under that species.
Each language owns its own evidence states:

- `lab/`: ordinary host-language specimen code that passes its normal checks.
- `exhibit/`: contract/framework surfaces that expose the missing edge through
  the language kit and lift to canonical ProofIR.
- `fixed/`: the paired exhibit source with the boundary closed; it uses the
  same contract/framework surface and must lift cleanly.
- `wild/`: real-world pinned sightings, created only when an advisory, commit,
  affected path, and evidence are available.

`wild/` is not a placeholder. If there is no pinned sighting for a language, the
language directory does not contain `wild/`.

## Layout

The target layout is:

```text
bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/
  README.md
  specimen.yaml

  java/
    lab/
    exhibit/
      provekit-native/
      spring-web/
      equivalence.json
      sat-witness.json
    fixed/
      provekit-native/
      spring-web/

  typescript/
    lab/
    exhibit/
      zod/
      class-validator/
      equivalence.json
      sat-witness.json
    fixed/
      zod/
      class-validator/

  csharp/
    lab/
    exhibit/
      data-annotations/
      provekit-annotations/
      linq-where/
      equivalence.json
      sat-witness.json
    fixed/
      data-annotations/
      provekit-annotations/
      linq-where/
```

Future real-world evidence is added under the relevant language only:

```text
bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/
  java/
    wild/
      CVE-or-advisory-slug/
  typescript/
    wild/
      GHSA-or-commit-slug/
```

## Evidence Model

The repeated code across `lab`, `exhibit`, and `fixed` is intentional. These
folders are separate evidentiary states, not duplicate source for convenience.

`lab/` proves the specimen is normal host-language code. It should compile,
run, and pass ordinary checks while leaving the Sugar obligation latent.

`exhibit/` proves native source surfaces can express the boundary. For Java,
that can be Sugar-native annotations and Spring Web. For TypeScript, that can
be zod and class-validator. For C#, that can be DataAnnotations,
`//provekit:` annotations, and LINQ. Each exhibit lifts through its language kit
and must match the species-level missing edge.

`fixed/` proves closure. It contains the same exhibit surface after the source
is changed to close the null boundary. The fixed artifact is accepted only when
re-lift shows the expected ProofIR and the diagnostic no longer contains the
missing edge.

`wild/` proves field relevance. It contains pinned real-world sightings with
source, advisory or commit identity, affected path, and evidence. It is created
only when that evidence exists.

## Manifest Shape

The species manifest should describe one species with language exhibits, rather
than one manifest per language species. Conceptually:

```yaml
id: BZ-SHAPE-005
name: Null Boundary Equivalence
kingdom: shape
predicates:
  boundary: maybe_null(name)
  sink: non_null(name)
  missingEdge: maybe_null(name) => non_null(name)
languages:
  - id: java
    lab: java/lab
    exhibits:
      - id: provekit-native
        fixed:
          id: provekit-native
      - id: spring-web
        fixed:
          id: spring-web
  - id: typescript
    lab: typescript/lab
    exhibits:
      - id: zod
        fixed:
          id: zod
      - id: class-validator
        fixed:
          id: class-validator
  - id: csharp
    lab: csharp/lab
    exhibits:
      - id: data-annotations
        fixed:
          id: data-annotations
      - id: provekit-annotations
        fixed:
          id: provekit-annotations
      - id: linq-where
        fixed:
          id: linq-where
```

The implementation can choose the exact YAML schema, but the model must preserve
the species/language/state hierarchy:

```text
species -> language -> lab | exhibit | fixed | wild
```

## ID Policy

`BZ-SHAPE-005` remains the null-boundary species ID. The current
`BZ-SHAPE-006` and `BZ-SHAPE-007` meanings are retired from the checked-in
registry because they were language-specific aliases of the same species.

Future `BZ-SHAPE-006` and `BZ-SHAPE-007` IDs may be reused only after the docs
and tests no longer refer to TypeScript and C# null-boundary exhibits by those
species IDs. The historical design note can mention the retirement to avoid
confusing old commit history with the active taxonomy.

## Migration Rules

The migration should be mechanical:

- Move the Java `lab` and `exposed` material under `java/`.
- Rename `exposed/` to `exhibit/`.
- Create checked-in `fixed/` pairs for each exhibit.
- Move the TypeScript material under `typescript/`.
- Move the C# material under `csharp/`.
- Delete placeholder `wild/` directories and README files unless they contain a
  pinned real sighting.
- Update docs, tests, and runner defaults to point at the single species.
- Preserve the shared ProofIR CID and equivalence checks.

No behavior should change during the restructure. The Bug Zoo runner should
still prove that every exhibit reaches the expected canonical ProofIR and that
each fixed artifact closes the missing edge.

## Validation

The restructure is complete when:

- `cargo run --manifest-path bug-zoo/Cargo.toml -- --all` passes.
- Direct TypeScript discovery still works for zod and class-validator.
- Direct C# discovery still works for DataAnnotations, Sugar annotations,
  and LINQ.
- Java exhibit lifters still produce the same expected ProofIR.
- Java Sugar-native `@Requires("name != null")` and `@NotNull` emit the same
  IR for the null case.
- Bug Zoo docs describe one null-boundary species with three language exhibits.
- No empty `wild/` directory remains in the null-boundary species.

## Non-Goals

This design does not add new NPE sub-shapes. Nested-field null dereferences,
nullable lookup misuse, and nullable return misuse can become future species or
subspecies, but this migration only corrects the taxonomy of the existing
null-boundary receipts.

This design does not require adding real-world sightings. `wild/` remains absent
until a sighting is pinned with evidence.

This design does not add a ProofIR compiler, template database, dropper,
realizer, or fix-receipt generator. Those are later systems that can consume the
checked-in `exhibit/` and `fixed/` pair.
