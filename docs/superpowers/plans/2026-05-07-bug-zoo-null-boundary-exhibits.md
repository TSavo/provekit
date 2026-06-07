# Bug Zoo Null-Boundary Exhibits Implementation Plan

> **Superseded scope note (2026-05-07):** The accepted implementation no longer
> uses `dropped/`, droppers, realizers, or checked-in fix receipts for this null
> boundary specimen. The active story is Green/Red/Green: `lab/` is ordinary
> passing host code, `exhibit/` adds a contract surface that ProveKit reports as
> the missing edge, and `fixed/` carries the same exhibit surface with source
> fixed so ProveKit runs clean. Treat the remaining task text below as historical
> planning context, not active architecture.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure the checked-in null-boundary Bug Zoo pack into one `BZ-SHAPE-005` species with Java, TypeScript, and C# language exhibits.

**Architecture:** Keep the Bug Zoo runner as the single verifier, but change its manifest model from one flat language specimen to one species manifest containing language entries. Move files mechanically under `species -> language -> lab | exhibit | dropped`, delete placeholder `wild/` directories, then update docs and smoke tests to reference the single species.

**Tech Stack:** Rust `bug-zoo` crate, `serde_yaml`, `serde_json`, Java shell harnesses, TypeScript `pnpm exec tsx`, C# `dotnet`, Markdown docs.

---

## File Structure

- Modify: `bug-zoo/src/lib.rs`
  - Replace `SpecimenManifest`'s flat `paths`, `commands`, `exposures`, `equivalence`, `exposure`, and `dropper` fields with `languages: Vec<LanguageSpecimen>`.
  - Keep `Predicates`, `Exposure`, `Dropper`, `CommandSpec`, and most verification helpers; run them once per language.
  - Change the default target to `bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence`.
  - Return JSON reports with one species id and per-language results.
- Modify: `bug-zoo/tests/smoke.rs`
  - Update direct TypeScript and C# paths to the new single-species layout.
  - Add a smoke assertion that `--all` now reports one species, not three language species.
- Move/create/delete: `bug-zoo/species/**`
  - Create: `bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/`.
  - Move Java material under `java/`, TypeScript under `typescript/`, C# under `csharp/`.
  - Rename every `exposed/` directory to `exhibit/`.
  - Delete empty `wild/` placeholders.
  - Remove the retired `BZ-SHAPE-005-java-*`, `BZ-SHAPE-006-typescript-*`, and `BZ-SHAPE-007-csharp-*` species directories after their contents are moved.
- Modify: `bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/specimen.yaml`
  - Write one species manifest with `languages:` entries for `java`, `typescript`, and `csharp`.
- Modify: `bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/README.md`
  - Describe the single species and its language exhibits.
- Modify: `bug-zoo/README.md`, `docs/explanation/bug-zoo.md`, `docs/how-to/bug-zoo.md`, `docs/explanation/use-cases.md`, `docs/reference/protocol-extensions.md`
  - Replace the old language-as-species wording and paths.
- Modify: `docs/superpowers/specs/2026-05-06-bug-zoo-design.md`
  - Add a short note that the checked-in registry now folds the null-boundary pack into one species with exhibits.

---

### Task 1: Add Failing Tests For The Species Manifest

**Files:**
- Modify: `bug-zoo/src/lib.rs`
- Modify: `bug-zoo/tests/smoke.rs`

- [ ] **Step 1: Add a unit test for the new manifest shape**

In `bug-zoo/src/lib.rs`, inside `#[cfg(test)] mod tests`, add this test after `parses_manifest_with_java_dropper_realizer`:

```rust
    #[test]
    fn parses_species_manifest_with_language_exhibits() {
        let raw = r#"
id: BZ-SHAPE-005
name: Null Boundary Equivalence
kingdom: shape
status: lab
predicates:
  boundary: maybe_null(name)
  sink: non_null(name)
  missingEdge: maybe_null(name) => non_null(name)
languages:
  - id: java
    surface: java-provekit-native-and-spring-web
    paths:
      labLibrary: java/lab/library
      labHarness: java/lab/harness
      labKitRpc: java/lab/kit-rpc
    commands:
      hostCheck:
        cwd: java/lab/harness
        argv: ["./run.sh"]
    exhibits:
      - id: provekit-native
        surface: java-provekit-native
        harness: java/exhibit/provekit-native/harness
        kitRpc: java/exhibit/provekit-native/kit-rpc
        liftRpc:
          cwd: java/exhibit/provekit-native/kit-rpc
          argv: ["./run-java-lifter.sh"]
        proofIrFile: java/exhibit/provekit-native/expected.proofir.json
        diagnosticFile: java/exhibit/provekit-native/expected-diagnostic.txt
        lossiness:
          erased: ["Java body"]
          preserved: ["precondition neq(name, null)"]
    equivalence:
      required: []
    exposure:
      satWitnessFile: java/exhibit/sat-witness.json
    dropper:
      available: false
  - id: typescript
    surface: typescript-zod-and-class-validator
    paths:
      labLibrary: typescript/lab/library
      labHarness: typescript/lab/harness
      labKitRpc: typescript/lab/kit-rpc
    commands:
      hostCheck:
        cwd: typescript/lab/harness
        argv: ["./run.sh"]
    exhibits:
      - id: zod
        surface: typescript-zod
        harness: typescript/exhibit/zod/harness
        kitRpc: typescript/exhibit/zod/kit-rpc
        liftRpc:
          cwd: typescript/exhibit/zod/kit-rpc
          argv: ["./run-ts-lifter.sh"]
        proofIrFile: typescript/exhibit/zod/expected.proofir.json
        diagnosticFile: typescript/exhibit/zod/expected-diagnostic.txt
        lossiness:
          erased: ["TypeScript body"]
          preserved: ["precondition neq(name, null)"]
    equivalence:
      required: []
    exposure:
      satWitnessFile: typescript/exhibit/sat-witness.json
    dropper:
      available: false
wildSightings: []
"#;
        let manifest: SpecimenManifest = serde_yaml::from_str(raw).expect("parse manifest");

        assert_eq!(manifest.id, "BZ-SHAPE-005");
        assert_eq!(manifest.languages.len(), 2);
        assert_eq!(manifest.languages[0].id, "java");
        assert_eq!(manifest.languages[0].exhibits[0].id, "provekit-native");
        assert_eq!(manifest.languages[1].id, "typescript");
        assert_eq!(
            manifest.languages[1].exhibits[0].harness,
            PathBuf::from("typescript/exhibit/zod/harness")
        );
    }
```

- [ ] **Step 2: Add a smoke assertion that all-species JSON reports one species**

In `bug-zoo/tests/smoke.rs`, add this test after `all_specimens_pass`:

```rust
#[test]
fn all_specimens_reports_one_null_boundary_species() {
    let root = repo_root();
    let output = Command::new(env!("CARGO_BIN_EXE_provekit-bug-zoo"))
        .arg(root.join("bug-zoo/species"))
        .arg("--all")
        .arg("--json")
        .current_dir(&root)
        .output()
        .expect("spawn provekit-bug-zoo --all --json");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit-bug-zoo --all --json failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("bug zoo JSON report parses");
    assert_eq!(report["ok"], true);
    let reports = report["reports"].as_array().expect("reports is an array");
    assert_eq!(reports.len(), 1, "null-boundary is one species");
    assert_eq!(reports[0]["id"], "BZ-SHAPE-005");
    assert_eq!(reports[0]["languages"].as_array().unwrap().len(), 3);
}
```

- [ ] **Step 3: Run the focused tests and verify they fail**

Run:

```sh
cargo test --manifest-path bug-zoo/Cargo.toml parses_species_manifest_with_language_exhibits
cargo test --manifest-path bug-zoo/Cargo.toml all_specimens_reports_one_null_boundary_species
```

Expected:

- The first command fails to compile or run because `SpecimenManifest` has no `languages` field yet.
- The second command fails because the filesystem still contains three species and reports three entries.

- [ ] **Step 4: Leave the tests red for Task 2**

Do not commit this task by itself. The tests are intentionally red until Task 2
updates the runner model; commit the tests together with the runner changes once
the focused unit tests are green.

---

### Task 2: Update The Runner Manifest Model

**Files:**
- Modify: `bug-zoo/src/lib.rs`

- [ ] **Step 1: Replace the flat manifest structs**

In `bug-zoo/src/lib.rs`, replace the `SpecimenManifest` definition with:

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpecimenManifest {
    id: String,
    name: String,
    kingdom: String,
    status: String,
    predicates: Predicates,
    languages: Vec<LanguageSpecimen>,
    #[serde(default)]
    wild_sightings: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LanguageSpecimen {
    id: String,
    surface: String,
    paths: SpecimenPaths,
    commands: SpecimenCommands,
    #[serde(rename = "exhibits")]
    exhibits: Vec<Exposure>,
    equivalence: Equivalence,
    exposure: ExposureFiles,
    dropper: Dropper,
    #[serde(default)]
    wild_sightings: Vec<String>,
}
```

Keep `SpecimenPaths`, `SpecimenCommands`, `CommandSpec`, `Predicates`, `Exposure`, `Lossiness`, `Equivalence`, `Expectations`, `ExposureFiles`, and `Dropper`.

- [ ] **Step 2: Remove `Expectations` from reports**

Delete the `Expectations` struct and all references to:

```rust
expectations: Expectations,
```

The new manifest does not need per-language expectation strings because every checked-in language exhibit in this species has the same expectation: host check passes, exhibits expose the missing edge, and dropped artifacts close it.

- [ ] **Step 3: Change the default specimen target**

In `resolve_targets`, change the default path to:

```rust
PathBuf::from("bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence")
```

- [ ] **Step 4: Replace `check_specimen`'s flat verification loop**

In `check_specimen`, after parsing and validating the manifest, replace the flat host/exposure/dropper logic with:

```rust
    let mut language_reports = Vec::new();
    let mut all_cids = BTreeMap::new();

    for language in &manifest.languages {
        run_host_check(specimen_dir, &language.commands.host_check).map_err(ZooError::verify)?;

        let mut cids = BTreeMap::new();
        for exhibit in &language.exhibits {
            let lifted = invoke_lift_rpc(specimen_dir, exhibit).map_err(ZooError::verify)?;
            let expected =
                read_json(specimen_dir.join(&exhibit.proof_ir_file)).map_err(ZooError::setup)?;
            let lifted_ir = lifted
                .get("ir")
                .cloned()
                .ok_or_else(|| ZooError::verify(format!("exhibit `{}` missing ir", exhibit.id)))?;
            let lifted_cid = proof_ir_cid(&lifted_ir).map_err(ZooError::verify)?;
            let expected_cid = proof_ir_cid(&expected).map_err(ZooError::setup)?;
            if lifted_cid != expected_cid {
                return Err(ZooError::verify(format!(
                    "language `{}` exhibit `{}` ProofIR CID mismatch: lifted {lifted_cid}, expected {expected_cid}",
                    language.id, exhibit.id
                )));
            }

            let diagnostic_path = specimen_dir.join(&exhibit.diagnostic_file);
            let diag = std::fs::read_to_string(&diagnostic_path).map_err(|e| {
                ZooError::verify(format!(
                    "read diagnostic {} for `{}`: {e}",
                    diagnostic_path.display(),
                    exhibit.id
                ))
            })?;
            if !diag.contains(&manifest.predicates.missing_edge) {
                return Err(ZooError::verify(format!(
                    "diagnostic for `{}` does not mention missing edge `{}`",
                    exhibit.id, manifest.predicates.missing_edge
                )));
            }

            cids.insert(exhibit.id.clone(), lifted_cid.clone());
            all_cids.insert(format!("{}:{}", language.id, exhibit.id), lifted_cid);
        }

        for [left, right] in &language.equivalence.required {
            if cids.get(left) != cids.get(right) {
                return Err(ZooError::verify(format!(
                    "language `{}` equivalence failed: `{left}` CID {:?} != `{right}` CID {:?}",
                    language.id,
                    cids.get(left),
                    cids.get(right)
                )));
            }
        }

        let sat_witness = read_json(specimen_dir.join(&language.exposure.sat_witness_file))
            .map_err(ZooError::setup)?;
        let dropper_report =
            verify_dropper(specimen_dir, &manifest.predicates, &language.dropper)
                .map_err(ZooError::verify)?;

        if !quiet {
            println!("zoo: {} {} hostCheck PASS", manifest.id, language.id);
            for (id, cid) in &cids {
                println!("zoo: {} exhibit {id} {cid}", language.id);
            }
            for [left, right] in &language.equivalence.required {
                println!("zoo: {} equivalence {left} == {right} PASS", language.id);
            }
            println!(
                "zoo: {} expected verify failure {} PASS",
                language.id, manifest.predicates.missing_edge
            );
            if dropper_report.is_some() {
                println!(
                    "zoo: {} dropper closed {} PASS",
                    language.id, manifest.predicates.missing_edge
                );
            }
        }

        language_reports.push(json!({
            "id": language.id,
            "surface": language.surface,
            "proofIrCids": cids,
            "dropperAvailable": language.dropper.available,
            "dropper": dropper_report,
            "wildSightings": language.wild_sightings,
            "satWitness": sat_witness,
        }));
    }
```

Then return:

```rust
    Ok(json!({
        "id": manifest.id,
        "name": manifest.name,
        "kingdom": manifest.kingdom,
        "status": manifest.status,
        "missingEdge": manifest.predicates.missing_edge,
        "proofIrCids": all_cids,
        "languages": language_reports,
        "wildSightings": manifest.wild_sightings,
    }))
```

- [ ] **Step 5: Update validation for language entries**

Replace `validate_manifest_shape` with a language-aware version:

```rust
fn validate_manifest_shape(manifest: &SpecimenManifest) -> Vec<String> {
    let mut errors = Vec::new();

    if manifest.id.trim().is_empty() {
        errors.push("id is required".into());
    }
    if manifest.name.trim().is_empty() {
        errors.push("name is required".into());
    }
    if manifest.kingdom.trim().is_empty() {
        errors.push("kingdom is required".into());
    }
    if manifest.status.trim().is_empty() {
        errors.push("status is required".into());
    }
    if manifest.predicates.boundary.trim().is_empty() {
        errors.push("predicates.boundary is required".into());
    }
    if manifest.predicates.sink.trim().is_empty() {
        errors.push("predicates.sink is required".into());
    }
    if manifest.predicates.missing_edge.trim().is_empty() {
        errors.push("predicates.missingEdge is required".into());
    }
    if manifest.languages.is_empty() {
        errors.push("at least one language is required".into());
    }

    let mut language_ids = BTreeSet::new();
    for language in &manifest.languages {
        if !language_ids.insert(language.id.clone()) {
            errors.push(format!("duplicate language id `{}`", language.id));
        }
        errors.extend(validate_language_shape(language));
    }

    errors
}

fn validate_language_shape(language: &LanguageSpecimen) -> Vec<String> {
    let mut errors = Vec::new();

    if language.id.trim().is_empty() {
        errors.push("language.id is required".into());
    }
    if language.surface.trim().is_empty() {
        errors.push(format!("language `{}` surface is required", language.id));
    }
    if language.exhibits.is_empty() {
        errors.push(format!("language `{}` at least one exhibit is required", language.id));
    }
    if language.commands.host_check.argv.is_empty() {
        errors.push(format!("language `{}` commands.hostCheck.argv is required", language.id));
    }

    let mut exhibit_ids = BTreeSet::new();
    for exhibit in &language.exhibits {
        if !exhibit_ids.insert(exhibit.id.clone()) {
            errors.push(format!("language `{}` duplicate exhibit id `{}`", language.id, exhibit.id));
        }
        if exhibit.lift_rpc.argv.is_empty() {
            errors.push(format!(
                "language `{}` exhibit `{}` liftRpc.argv is required",
                language.id, exhibit.id
            ));
        }
        if exhibit.lossiness.erased.is_empty() || exhibit.lossiness.preserved.is_empty() {
            errors.push(format!(
                "language `{}` exhibit `{}` must describe lossiness erased and preserved boundaries",
                language.id, exhibit.id
            ));
        }
    }

    for [left, right] in &language.equivalence.required {
        if !exhibit_ids.contains(left) {
            errors.push(format!("language `{}` equivalence references unknown exhibit `{left}`", language.id));
        }
        if !exhibit_ids.contains(right) {
            errors.push(format!("language `{}` equivalence references unknown exhibit `{right}`", language.id));
        }
    }

    errors.extend(validate_dropper_shape(&language.dropper, &language.id));
    errors
}
```

Move the existing `dropper.available` checks into:

```rust
fn validate_dropper_shape(dropper: &Dropper, language_id: &str) -> Vec<String> {
    let mut errors = Vec::new();
    if !dropper.available {
        return errors;
    }
    if dropper.surface.as_deref().unwrap_or("").trim().is_empty() {
        errors.push(format!("language `{language_id}` dropper.surface is required when dropper.available is true"));
    }
    if dropper.source.is_none() {
        errors.push(format!("language `{language_id}` dropper.source is required when dropper.available is true"));
    }
    if dropper.target_symbol.as_deref().unwrap_or("").trim().is_empty() {
        errors.push(format!("language `{language_id}` dropper.targetSymbol is required when dropper.available is true"));
    }
    if dropper.proof_var.as_deref().unwrap_or("").trim().is_empty() {
        errors.push(format!("language `{language_id}` dropper.proofVar is required when dropper.available is true"));
    }
    match &dropper.realizer_rpc {
        Some(command) if command.argv.is_empty() => errors.push(format!("language `{language_id}` dropper.realizerRpc.argv is required when dropper.available is true")),
        None => errors.push(format!("language `{language_id}` dropper.realizerRpc is required when dropper.available is true")),
        Some(_) => {}
    }
    if dropper.output_source.is_none() {
        errors.push(format!("language `{language_id}` dropper.outputSource is required when dropper.available is true"));
    }
    if dropper.language_dropper_file.is_some() && dropper.proof_plan_file.is_none() {
        errors.push(format!("language `{language_id}` dropper.proofPlanFile is required when dropper.languageDropperFile is set"));
    }
    if dropper.closure_proof_ir_file.is_none() {
        errors.push(format!("language `{language_id}` dropper.closureProofIrFile is required when dropper.available is true"));
    }
    if dropper.fix_receipt_file.is_none() && dropper.verify_output_file.is_none() {
        errors.push(format!("language `{language_id}` dropper.fixReceiptFile is required when dropper.available is true"));
    }
    errors
}
```

- [ ] **Step 6: Update path validation for language entries**

Replace `validate_paths` with:

```rust
fn validate_paths(specimen_dir: &Path, manifest: &SpecimenManifest) -> Vec<String> {
    let mut errors = Vec::new();
    for language in &manifest.languages {
        errors.extend(validate_language_paths(specimen_dir, language));
    }
    errors
}
```

Move the existing path checks into `validate_language_paths(specimen_dir: &Path, language: &LanguageSpecimen) -> Vec<String>`, replacing `manifest.paths` with `language.paths`, `manifest.exposure` with `language.exposure`, `manifest.exposures` with `language.exhibits`, and `manifest.dropper` with `language.dropper`.

- [ ] **Step 7: Update dropper verification signature**

Change:

```rust
fn verify_dropper(
    specimen_dir: &Path,
    manifest: &SpecimenManifest,
) -> Result<Option<Value>, String>
```

to:

```rust
fn verify_dropper(
    specimen_dir: &Path,
    predicates: &Predicates,
    dropper: &Dropper,
) -> Result<Option<Value>, String>
```

Inside the function, replace `manifest.dropper` with `dropper` and `manifest.predicates` with `predicates`.

- [ ] **Step 8: Update unit test helpers**

Update `valid_manifest()` to return a species manifest with one language:

```rust
fn valid_manifest() -> SpecimenManifest {
    SpecimenManifest {
        id: "BZ-SHAPE-005".into(),
        name: "Null Boundary Equivalence".into(),
        kingdom: "shape".into(),
        status: "lab".into(),
        predicates: Predicates {
            boundary: "maybe_null(name)".into(),
            sink: "non_null(name)".into(),
            missing_edge: "maybe_null(name) => non_null(name)".into(),
        },
        languages: vec![LanguageSpecimen {
            id: "java".into(),
            surface: "java".into(),
            paths: SpecimenPaths {
                lab_library: "java/lab/library".into(),
                lab_harness: "java/lab/harness".into(),
                lab_kit_rpc: "java/lab/kit-rpc".into(),
            },
            commands: SpecimenCommands {
                host_check: CommandSpec {
                    cwd: "java/lab/harness".into(),
                    argv: vec!["./run.sh".into()],
                },
            },
            exhibits: vec![Exposure {
                id: "spring-web".into(),
                surface: "java-spring-web".into(),
                harness: "java/exhibit/spring-web/harness".into(),
                kit_rpc: "java/exhibit/spring-web/kit-rpc".into(),
                lift_rpc: CommandSpec {
                    cwd: "java/exhibit/spring-web/kit-rpc".into(),
                    argv: vec!["./run-java-lifter.sh".into()],
                },
                proof_ir_file: "java/exhibit/spring-web/expected.proofir.json".into(),
                diagnostic_file: "java/exhibit/spring-web/expected-diagnostic.txt".into(),
                lossiness: Lossiness {
                    erased: vec!["Spring binding".into()],
                    preserved: vec!["precondition neq(name, null)".into()],
                },
            }],
            equivalence: Equivalence { required: vec![] },
            exposure: ExposureFiles {
                sat_witness_file: "java/exhibit/sat-witness.json".into(),
            },
            dropper: unavailable_dropper(),
            wild_sightings: vec![],
        }],
        wild_sightings: vec![],
    }
}
```

Update helper references from `manifest.exposures[0]` to:

```rust
let language = &manifest.languages[0];
let exhibit = &language.exhibits[0];
```

Update the remaining validation tests with the same nesting:

- `validation_rejects_missing_lossiness`: clear `manifest.languages[0].exhibits[0].lossiness.erased`.
- `validation_rejects_duplicate_exposure_ids`: rename the test to `validation_rejects_duplicate_exhibit_ids`, push the duplicate onto `manifest.languages[0].exhibits`, use `java/exhibit/provekit-native/...` paths, and assert the error contains `duplicate exhibit id`.
- `validation_rejects_unknown_equivalence_references`: set `manifest.languages[0].equivalence.required`.
- `validation_rejects_empty_command_argv`: clear `manifest.languages[0].commands.host_check.argv` and `manifest.languages[0].exhibits[0].lift_rpc.argv`; assert the messages contain `commands.hostCheck.argv is required` and `exhibit \`spring-web\` liftRpc.argv is required`.
- `validation_rejects_language_dropper_without_proof_plan`: assign the `Dropper` to `manifest.languages[0].dropper` and use `java/...` paths.
- `path_validation_rejects_escape_paths`: mutate `manifest.languages[0].paths.lab_library` and `manifest.languages[0].exhibits[0].proof_ir_file`.
- `path_validation_reports_missing_files`: remove `manifest.languages[0].exposure.sat_witness_file` and `manifest.languages[0].exhibits[0].proof_ir_file`.

- [ ] **Step 9: Run focused tests**

Run:

```sh
cargo test --manifest-path bug-zoo/Cargo.toml parses_species_manifest_with_language_exhibits
cargo test --manifest-path bug-zoo/Cargo.toml parses_manifest_with_exposures_and_equivalence
cargo test --manifest-path bug-zoo/Cargo.toml validation_rejects_missing_lossiness
cargo test --manifest-path bug-zoo/Cargo.toml validation_rejects_duplicate_exhibit_ids
cargo test --manifest-path bug-zoo/Cargo.toml validation_rejects_unknown_equivalence_references
cargo test --manifest-path bug-zoo/Cargo.toml validation_rejects_empty_command_argv
cargo test --manifest-path bug-zoo/Cargo.toml validation_rejects_language_dropper_without_proof_plan
cargo test --manifest-path bug-zoo/Cargo.toml path_validation_rejects_escape_paths
cargo test --manifest-path bug-zoo/Cargo.toml path_validation_reports_missing_files
```

Expected: all pass except the all-species smoke test from Task 1, which still waits for the filesystem migration.

- [ ] **Step 10: Commit the runner model**

```sh
git add bug-zoo/src/lib.rs bug-zoo/tests/smoke.rs
git commit -m "refactor: model bug zoo languages as exhibits"
```

---

### Task 3: Move The Null-Boundary Species Files

**Files:**
- Move/delete/create under `bug-zoo/species/`

- [ ] **Step 1: Create the new species directory**

Run:

```sh
mkdir -p bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence
```

- [ ] **Step 2: Move Java material**

Run:

```sh
mkdir -p bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/java
git mv bug-zoo/species/BZ-SHAPE-005-java-null-boundary-equivalence/lab bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/java/lab
git mv bug-zoo/species/BZ-SHAPE-005-java-null-boundary-equivalence/exposed bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/java/exhibit
git mv bug-zoo/species/BZ-SHAPE-005-java-null-boundary-equivalence/dropped bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/java/dropped
```

- [ ] **Step 3: Move TypeScript material**

Run:

```sh
mkdir -p bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript
git mv bug-zoo/species/BZ-SHAPE-006-typescript-null-boundary-equivalence/lab bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/lab
git mv bug-zoo/species/BZ-SHAPE-006-typescript-null-boundary-equivalence/exposed bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/exhibit
git mv bug-zoo/species/BZ-SHAPE-006-typescript-null-boundary-equivalence/dropped bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/dropped
git mv bug-zoo/species/BZ-SHAPE-006-typescript-null-boundary-equivalence/tools bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/tools
```

- [ ] **Step 4: Move C# material**

Run:

```sh
mkdir -p bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/csharp
git mv bug-zoo/species/BZ-SHAPE-007-csharp-null-boundary-equivalence/lab bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/csharp/lab
git mv bug-zoo/species/BZ-SHAPE-007-csharp-null-boundary-equivalence/exposed bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/csharp/exhibit
git mv bug-zoo/species/BZ-SHAPE-007-csharp-null-boundary-equivalence/dropped bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/csharp/dropped
```

- [ ] **Step 5: Remove placeholder wild directories and old species shells**

Run:

```sh
rm -rf bug-zoo/species/BZ-SHAPE-005-java-null-boundary-equivalence/wild
rm -rf bug-zoo/species/BZ-SHAPE-006-typescript-null-boundary-equivalence/wild
rm -rf bug-zoo/species/BZ-SHAPE-007-csharp-null-boundary-equivalence/wild
rm -rf bug-zoo/species/BZ-SHAPE-005-java-null-boundary-equivalence
rm -rf bug-zoo/species/BZ-SHAPE-006-typescript-null-boundary-equivalence
rm -rf bug-zoo/species/BZ-SHAPE-007-csharp-null-boundary-equivalence
```

- [ ] **Step 6: Verify no empty wild directory exists**

Run:

```sh
find bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence -path '*/wild' -type d -print
```

Expected: no output.

- [ ] **Step 7: Commit the mechanical move**

```sh
git add -A bug-zoo/species
git commit -m "refactor: fold null-boundary species into language exhibits"
```

---

### Task 4: Write The Unified Species Manifest And README

**Files:**
- Create: `bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/specimen.yaml`
- Create: `bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/README.md`
- Modify: moved receipt JSON files that refer to old paths

- [ ] **Step 1: Create the unified manifest**

Create `bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/specimen.yaml` with this content:

```yaml
id: BZ-SHAPE-005
name: Null Boundary Equivalence
kingdom: shape
status: lab
predicates:
  boundary: maybe_null(name)
  sink: non_null(name)
  missingEdge: maybe_null(name) => non_null(name)
languages:
  - id: java
    surface: java-provekit-native-and-spring-web
    paths:
      labLibrary: java/lab/library
      labHarness: java/lab/harness
      labKitRpc: java/lab/kit-rpc
    commands:
      hostCheck:
        cwd: java/lab/harness
        argv: ["./run.sh"]
    exhibits:
      - id: provekit-native
        surface: java-provekit-native
        harness: java/exhibit/provekit-native/harness
        kitRpc: java/exhibit/provekit-native/kit-rpc
        liftRpc:
          cwd: java/exhibit/provekit-native/kit-rpc
          argv: ["./run-java-lifter.sh"]
        proofIrFile: java/exhibit/provekit-native/expected.proofir.json
        diagnosticFile: java/exhibit/provekit-native/expected-diagnostic.txt
        lossiness:
          erased:
            - Java method body string concatenation
            - concrete annotation package name
          preserved:
            - precondition neq(name, null)
      - id: spring-web
        surface: java-spring-web
        harness: java/exhibit/spring-web/harness
        kitRpc: java/exhibit/spring-web/kit-rpc
        liftRpc:
          cwd: java/exhibit/spring-web/kit-rpc
          argv: ["./run-java-lifter.sh"]
        proofIrFile: java/exhibit/spring-web/expected.proofir.json
        diagnosticFile: java/exhibit/spring-web/expected-diagnostic.txt
        lossiness:
          erased:
            - Spring request binding machinery
            - Java method body string concatenation
          preserved:
            - precondition neq(name, null)
    equivalence:
      required:
        - [provekit-native, spring-web]
    exposure:
      satWitnessFile: java/exhibit/sat-witness.json
    dropper:
      available: true
      surface: java-provekit-native
      source: java/lab/library/src/main/java/zoo/UserDirectory.java
      targetSymbol: lookup
      proofVar: name
      realizerRpc:
        cwd: java/dropped/provekit-native/kit-rpc
        argv: ["./run-java-realizer.sh"]
      outputSource: java/dropped/provekit-native/library/src/main/java/zoo/UserDirectory.java
      proofPlanFile: java/dropped/provekit-native/proof-plan.json
      languageDropperFile: java/dropped/provekit-native/language-dropper.json
      closureProofIrFile: java/dropped/provekit-native/closure.proofir.json
      fixReceiptFile: java/dropped/provekit-native/fix-receipt.json
  - id: typescript
    surface: typescript-zod-and-class-validator
    paths:
      labLibrary: typescript/lab/library
      labHarness: typescript/lab/harness
      labKitRpc: typescript/lab/kit-rpc
    commands:
      hostCheck:
        cwd: typescript/lab/harness
        argv: ["./run.sh"]
    exhibits:
      - id: zod
        surface: typescript-zod
        harness: typescript/exhibit/zod/harness
        kitRpc: typescript/exhibit/zod/kit-rpc
        liftRpc:
          cwd: typescript/exhibit/zod/kit-rpc
          argv: ["./run-ts-lifter.sh"]
        proofIrFile: typescript/exhibit/zod/expected.proofir.json
        diagnosticFile: typescript/exhibit/zod/expected-diagnostic.txt
        lossiness:
          erased:
            - TypeScript function body string concatenation
            - concrete zod parser runtime implementation
          preserved:
            - precondition neq(name, null)
      - id: class-validator
        surface: typescript-class-validator
        harness: typescript/exhibit/class-validator/harness
        kitRpc: typescript/exhibit/class-validator/kit-rpc
        liftRpc:
          cwd: typescript/exhibit/class-validator/kit-rpc
          argv: ["./run-ts-lifter.sh"]
        proofIrFile: typescript/exhibit/class-validator/expected.proofir.json
        diagnosticFile: typescript/exhibit/class-validator/expected-diagnostic.txt
        lossiness:
          erased:
            - TypeScript function body string concatenation
            - concrete class-validator decorator package name
          preserved:
            - precondition neq(name, null)
    equivalence:
      required:
        - [zod, class-validator]
    exposure:
      satWitnessFile: typescript/exhibit/sat-witness.json
    dropper:
      available: true
      surface: typescript-native
      source: typescript/lab/library/src/UserDirectory.ts
      targetSymbol: lookup
      proofVar: name
      realizerRpc:
        cwd: typescript/dropped/typescript-native/kit-rpc
        argv: ["./run-ts-realizer.sh"]
      outputSource: typescript/dropped/typescript-native/library/src/UserDirectory.ts
      proofPlanFile: typescript/dropped/typescript-native/proof-plan.json
      languageDropperFile: typescript/dropped/typescript-native/language-dropper.json
      closureProofIrFile: typescript/dropped/typescript-native/closure.proofir.json
      fixReceiptFile: typescript/dropped/typescript-native/fix-receipt.json
  - id: csharp
    surface: csharp-data-annotations-provekit-annotations-and-linq
    paths:
      labLibrary: csharp/lab/library
      labHarness: csharp/lab/harness
      labKitRpc: csharp/lab/kit-rpc
    commands:
      hostCheck:
        cwd: csharp/lab/harness
        argv: ["./run.sh"]
    exhibits:
      - id: data-annotations
        surface: csharp-data-annotations
        harness: csharp/exhibit/data-annotations/harness
        kitRpc: csharp/exhibit/data-annotations/kit-rpc
        liftRpc:
          cwd: csharp/exhibit/data-annotations/kit-rpc
          argv: ["./run-csharp-lifter.sh"]
        proofIrFile: csharp/exhibit/data-annotations/expected.proofir.json
        diagnosticFile: csharp/exhibit/data-annotations/expected-diagnostic.txt
        lossiness:
          erased:
            - C# method body string concatenation
            - concrete DataAnnotations runtime implementation
          preserved:
            - precondition neq(name, null)
      - id: provekit-annotations
        surface: csharp-provekit-annotations
        harness: csharp/exhibit/provekit-annotations/harness
        kitRpc: csharp/exhibit/provekit-annotations/kit-rpc
        liftRpc:
          cwd: csharp/exhibit/provekit-annotations/kit-rpc
          argv: ["./run-csharp-lifter.sh"]
        proofIrFile: csharp/exhibit/provekit-annotations/expected.proofir.json
        diagnosticFile: csharp/exhibit/provekit-annotations/expected-diagnostic.txt
        lossiness:
          erased:
            - C# method body string concatenation
            - concrete provekit annotation scanner declaration shape
          preserved:
            - precondition neq(name, null)
      - id: linq-where
        surface: csharp-linq
        harness: csharp/exhibit/linq-where/harness
        kitRpc: csharp/exhibit/linq-where/kit-rpc
        liftRpc:
          cwd: csharp/exhibit/linq-where/kit-rpc
          argv: ["./run-csharp-lifter.sh"]
        proofIrFile: csharp/exhibit/linq-where/expected.proofir.json
        diagnosticFile: csharp/exhibit/linq-where/expected-diagnostic.txt
        lossiness:
          erased:
            - C# method body string concatenation
            - concrete LINQ membership quantifier shape
          preserved:
            - precondition neq(name, null)
    equivalence:
      required:
        - [data-annotations, provekit-annotations]
        - [data-annotations, linq-where]
    exposure:
      satWitnessFile: csharp/exhibit/sat-witness.json
    dropper:
      available: true
      surface: csharp-native
      source: csharp/lab/library/src/UserDirectory.cs
      targetSymbol: lookup
      proofVar: name
      realizerRpc:
        cwd: csharp/dropped/csharp-native/kit-rpc
        argv: ["./run-csharp-realizer.sh"]
      outputSource: csharp/dropped/csharp-native/library/src/UserDirectory.cs
      proofPlanFile: csharp/dropped/csharp-native/proof-plan.json
      languageDropperFile: csharp/dropped/csharp-native/language-dropper.json
      closureProofIrFile: csharp/dropped/csharp-native/closure.proofir.json
      fixReceiptFile: csharp/dropped/csharp-native/fix-receipt.json
wildSightings: []
```

- [ ] **Step 2: Update receipt path fields inside moved JSON files**

Run:

```sh
rg -n "exposed/|lab/|dropped/" bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/*/dropped -S
```

Update path-bearing fields in `proof-plan.json`, `language-dropper.json`, and `fix-receipt.json` so they match the new language-prefixed paths. For Java, for example:

```json
"witnessFile": "java/exhibit/sat-witness.json"
```

and:

```json
"source": "java/lab/library/src/main/java/zoo/UserDirectory.java",
"outputSource": "java/dropped/provekit-native/library/src/main/java/zoo/UserDirectory.java",
"postLift": {
  "proofIrFile": "java/dropped/provekit-native/closure.proofir.json"
}
```

Apply the same rule to TypeScript and C# with their language prefixes.

Because `proof-plan.json` and `language-dropper.json` are content-addressed,
also update the receipt CIDs to these exact values after the path changes:

```text
java proofPlanCid: blake3-512:e8e76731bd8b9d585629f1ca075d1711182ab16cbfe96a01fc51a313fdb9f71b1c35dc60d05c8c691f287e8cea3d037990d363ceb6c728e22646c59fbacb6ef5
java languageDropperCid: blake3-512:32fa9247c30c2312092d343a62c6367bdb04bf545fe330bcba9a61044a76fdb9481f3d0df8baf27b8ffdb1342b9394827306cd079ff609dab8f284051e0dc97a
typescript proofPlanCid: blake3-512:ceb33e824d92a95cd5eb463f44379be8923f523927555b136a793138c06944f147baf17fcbb1f42c49049c5494256debfba4aeb041ea4fd4278ed3e9b9a7a0c7
typescript languageDropperCid: blake3-512:9cba50214a3feb2e21c09dedb13a5cfddfbf5a2f34d75d6a9550d39807d019d33f2f7aec15a0faf9bebe468aea574e20225d1e8fdbf42ad95b29b66ebd1713f0
csharp proofPlanCid: blake3-512:6183e8bbfccfa83927494a62d8787a71ff33f82b522e1d2c445a0eb21445e364105ce9f9117ac8c4aa223c4bf90f7169f1c7c9f23f8f7b426559105d1374ad6e
csharp languageDropperCid: blake3-512:5d8e64397e72528fc6ac97a6e58a6e0f8e7c0ba465f87a19dff36a5ce96e78161f809157b087ded766dc202e634ad66f1d0e03191bc9775b245e5d019999bf6f
```

- [ ] **Step 3: Write the unified README**

Create `bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/README.md`:

```markdown
# BZ-SHAPE-005: Null Boundary Equivalence

This species captures the null-boundary bug: a value that may be null reaches a
boundary that requires non-null.

```text
maybe_null(name) => non_null(name)
```

Java, TypeScript, and C# are exhibits of the same species. Their host syntax,
frameworks, and runtime failures differ, but their lifted contract boundary is
the same ProofIR precondition: `neq(name, null)`.

## Exhibits

- `java/`: Sugar-native annotations and Spring Web `@RequestParam`.
- `typescript/`: zod and class-validator.
- `csharp/`: DataAnnotations, `//provekit:` annotations, and LINQ.

Each language keeps separate evidence states:

- `lab/`: ordinary host-language code that passes normal checks.
- `exhibit/`: native contract surfaces that expose the missing edge.
- `dropped/`: realized edge-closing code accepted only after re-lift.

`wild/` is intentionally absent until a real sighting is pinned with advisory or
commit identity, affected path, and evidence.
```

- [ ] **Step 4: Run the runner**

Run:

```sh
cargo run --manifest-path bug-zoo/Cargo.toml -- bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence --json
```

Expected: success, JSON report id `BZ-SHAPE-005`, and three language reports.

- [ ] **Step 5: Commit the unified manifest and README**

```sh
git add -A bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence
git commit -m "docs: describe null-boundary language exhibits"
```

---

### Task 5: Update Direct Discovery Tests And Documentation

**Files:**
- Modify: `bug-zoo/tests/smoke.rs`
- Modify: `bug-zoo/README.md`
- Modify: `docs/explanation/bug-zoo.md`
- Modify: `docs/how-to/bug-zoo.md`
- Modify: `docs/explanation/use-cases.md`
- Modify: `docs/reference/protocol-extensions.md`
- Modify: `docs/superpowers/specs/2026-05-06-bug-zoo-design.md`

- [ ] **Step 1: Update direct test paths**

In `bug-zoo/tests/smoke.rs`, replace:

```rust
"bug-zoo/species/BZ-SHAPE-007-csharp-null-boundary-equivalence/exposed/linq-where/harness"
```

with:

```rust
"bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/csharp/exhibit/linq-where/harness"
```

Replace:

```rust
"bug-zoo/species/BZ-SHAPE-006-typescript-null-boundary-equivalence/tools/ts-boundary-discover.ts"
```

with:

```rust
"bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/tools/ts-boundary-discover.ts"
```

Replace:

```rust
"bug-zoo/species/BZ-SHAPE-006-typescript-null-boundary-equivalence/exposed/zod/harness"
```

with:

```rust
"bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/exhibit/zod/harness"
```

- [ ] **Step 2: Update root Bug Zoo README**

In `bug-zoo/README.md`, replace the "Specimen States" list with language-aware wording:

```markdown
Each species can carry language exhibits. Each language exhibit can carry four
states:

- `lab/`: normal code or metadata that passes its ordinary host checks.
- `exhibit/`: the same bug species lifted through one or more native surfaces
  until ProofIR exposes the missing contract edge.
- `dropped/`: proof-first plan plus language-dropper projection that closes the
  edge, accepted only after re-lift verifies closure.
- `wild/`: real OSS sightings pinned by advisory, commit, affected path, and
  evidence. This directory exists only when the sighting is pinned.
```

Replace the current species table with:

```markdown
| Species | Language exhibits | Missing edge | Shared ProofIR CID |
|---|---|---|---|
| `BZ-SHAPE-005` | Java, TypeScript, C# | `maybe_null(name) => non_null(name)` | `blake3-512:0d611d8478a205ff040e7d0bcf6c21b12051340ecc5f00c3953af632b23fc01e069b4ad8a8699869163e135b9fde85792eba6acc54cd75cb3d3cc6a40a99ded4` |
```

Update command examples to:

```sh
pnpm exec tsx bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/tools/ts-boundary-discover.ts zod bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/exhibit/zod/harness

dotnet run --project implementations/csharp/Provekit.BugZoo/Provekit.BugZoo.csproj -- discover csharp-linq bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/csharp/exhibit/linq-where/harness
```

- [ ] **Step 3: Update explanation docs**

In `docs/explanation/bug-zoo.md`, replace the "Current Null-Boundary Receipts" bullets with:

```markdown
The current zoo includes one null-boundary species:

- `BZ-SHAPE-005`: null boundary through Java, TypeScript, and C# language
  exhibits.

The Java exhibit uses Sugar-native annotations and Spring Web
`@RequestParam`. The TypeScript exhibit uses zod and class-validator. The C#
exhibit uses DataAnnotations, `//provekit:` annotations, and LINQ.
```

Update direct discovery paths with the new paths from Step 2.

- [ ] **Step 4: Update how-to docs**

In `docs/how-to/bug-zoo.md`, replace old specimen paths with:

```text
bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence
```

Replace mentions of `exposed/` as a state with `exhibit/`, and update manifest wording from `exposures[]` to `languages[].exhibits[]`.

- [ ] **Step 5: Update use-case wording**

In `docs/explanation/use-cases.md`, replace:

```markdown
It moves to `exposed/`, where one or more surfaces lift the missing edge into ProofIR.
```

with:

```markdown
It moves to `exhibit/`, where one or more language-native surfaces lift the missing edge into ProofIR.
```

- [ ] **Step 6: Update protocol reference if needed**

In `docs/reference/protocol-extensions.md`, change "exposures" in the Bug Zoo row to "exhibits":

```markdown
Run self-contained Bug Zoo specimens through host checks, exhibits, equivalence checks, and optional dropper closure.
```

- [ ] **Step 7: Update the older design note**

In `docs/superpowers/specs/2026-05-06-bug-zoo-design.md`, replace the checked-in registry note with:

```markdown
This section is the original expansion sketch, not the current checked-in ID
registry. The implemented null-boundary pack now keeps `BZ-SHAPE-005` as one
species with Java, TypeScript, and C# language exhibits. The previous
language-specific `BZ-SHAPE-006` and `BZ-SHAPE-007` null-boundary meanings are
retired aliases, not active species.
```

- [ ] **Step 8: Search for stale old paths and names**

Run:

```sh
rg -n "BZ-SHAPE-006|BZ-SHAPE-007|java-null-boundary|typescript-null-boundary|csharp-null-boundary|exposed/" bug-zoo docs README.md implementations -S
```

Expected: only historical plan/spec files may mention old paths. Active docs, tests, and code should not.

- [ ] **Step 9: Commit docs and smoke path updates**

```sh
git add bug-zoo/tests/smoke.rs bug-zoo/README.md docs/explanation/bug-zoo.md docs/how-to/bug-zoo.md docs/explanation/use-cases.md docs/reference/protocol-extensions.md docs/superpowers/specs/2026-05-06-bug-zoo-design.md
git commit -m "docs: present null-boundary as one species"
```

---

### Task 6: Final Verification

**Files:**
- No planned edits unless verification exposes a bug.

- [ ] **Step 1: Run formatter**

Run:

```sh
cargo fmt --manifest-path bug-zoo/Cargo.toml
```

Expected: exits 0.

- [ ] **Step 2: Run Bug Zoo tests**

Run:

```sh
cargo test --manifest-path bug-zoo/Cargo.toml
```

Expected: all tests pass.

- [ ] **Step 3: Run all species**

Run:

```sh
cargo run --manifest-path bug-zoo/Cargo.toml -- --all
```

Expected: one species report for `BZ-SHAPE-005` with Java, TypeScript, and C# language exhibits passing host checks, exhibit CID checks, equivalence checks, and dropper closure.

- [ ] **Step 4: Run direct TypeScript discovery**

Run:

```sh
pnpm exec tsx bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/tools/ts-boundary-discover.ts zod bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/exhibit/zod/harness
```

Expected stdout contains:

```text
"kind":"bug-zoo-discovery"
"surface":"zod"
"missingEdge":"maybe_null(name) => non_null(name)"
"irEvidenceCid":
```

- [ ] **Step 5: Run direct C# discovery**

Run:

```sh
dotnet run --project implementations/csharp/Provekit.BugZoo/Provekit.BugZoo.csproj -- discover csharp-linq bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/csharp/exhibit/linq-where/harness
```

Expected stdout contains:

```text
"kind":"bug-zoo-discovery"
"surface":"csharp-linq"
"missingEdge":"maybe_null(name) => non_null(name)"
"irEvidenceCid":
```

- [ ] **Step 6: Check no placeholder wild directories exist**

Run:

```sh
find bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence -path '*/wild' -type d -print
```

Expected: no output.

- [ ] **Step 7: Check git status**

Run:

```sh
git status --short --branch
```

Expected: only unrelated pre-existing untracked `.vscode/` and `provekit-warnings/` remain, unless the implementation deliberately leaves no other changes.

- [ ] **Step 8: Commit final fixes if formatter/docs changed anything**

If `git status --short` shows tracked modifications after verification, commit them:

```sh
git add -A bug-zoo docs README.md implementations
git commit -m "chore: verify null-boundary exhibit migration"
```
