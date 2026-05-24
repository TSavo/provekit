// SPDX-License-Identifier: Apache-2.0
//
// GO AUTHORING SURFACE gauntlet: a Go library AUTHOR declares a boundary on a
// function (`//provekit:sugar(concept="identity")`) and the library GETS a
// contract -- the same way rust (`#[provekit::sugar(...)]`) and java authors
// do. This is the central piece: today's verify-side proved "a Go library gets
// a contract" by running the lifter internally; this proves the AUTHORING
// surface -- the DECLARATION drives emission.
//
// The config mirrors rust's authoring shape: two `[[plugins]]` --
//   go-bind      (layer = "library-bindings")  -> library-sugar-binding-entry
//   go-contracts (emit = "ir-document")        -> function-contract + callsite
// both resolving to provekit-lift-go-verify, which emits different IR per
// surface so a function is not minted twice.
//
// Closed loop, asserted here:
//   - DECLARE: only the annotated `Id` is lifted; `Unannotated` is NOT.
//   - GET A CONTRACT: mint writes the binding-entry catalog (the declaration)
//     AND the function-contract + auto-bridge (tool-written).
//   - VERIFY: the harvested `Id(3) == 3` discharges through the body `x` ->
//     `3 == 3` -> z3 -> signed witness, exit 0. Broken body -> Unsatisfied,
//     exit 1, no witness.
// The MATERIALIZE half of the loop (same `identity` concept -> Go sugar that
// compiles) is gated by `go_realize_materialize.rs`.
//
// Requires `go` and `z3` on PATH; guards skip loudly otherwise.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value as Json;

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn z3_available() -> bool {
    Command::new("z3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn go_available() -> bool {
    Command::new("go")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn unique_dir(suffix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("provekit-go-authoring-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn build_go_lift_verify() -> PathBuf {
    let go_module = repo_root().join("implementations").join("go");
    let out = std::env::temp_dir().join(format!("provekit-lift-go-verify-auth-{}", std::process::id()));
    let built = Command::new("go")
        .current_dir(&go_module)
        .args(["build", "-o", out.to_str().unwrap(), "./cmd/provekit-lift-go-verify"])
        .output()
        .expect("spawn go build");
    assert!(
        built.status.success(),
        "go build provekit-lift-go-verify failed\n  stderr: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    out
}

/// Stage the `examples/go-identity` authoring example into a tempdir, rewrite
/// both lift manifests to the built binary, and (for the negative case) break
/// the annotated body so `Id` no longer returns its argument.
fn stage_authoring_project(suffix: &str, lift_bin: &Path, broken: bool) -> PathBuf {
    let example = repo_root().join("examples").join("go-identity");
    let project = unique_dir(suffix);

    // id.go: keep the annotation; mutate the body for the negative case.
    let body = if broken { "x + 1" } else { "x" };
    let id_src = format!(
        "package sample\n\n//provekit:sugar(concept=\"identity\", library=\"builtin\", version=\"1\")\nfunc Id(x int) int {{\n\treturn {body}\n}}\n\nfunc Unannotated(y int) int {{\n\treturn y + 1\n}}\n"
    );
    fs::write(project.join("id.go"), id_src).expect("write id.go");
    fs::copy(example.join("id_test.go"), project.join("id_test.go")).expect("copy id_test.go");
    fs::copy(example.join("go.mod"), project.join("go.mod")).expect("copy go.mod");

    let provekit = project.join(".provekit");
    fs::create_dir_all(provekit.join("lift").join("go-bind")).unwrap();
    fs::create_dir_all(provekit.join("lift").join("go-contracts")).unwrap();
    fs::copy(
        example.join(".provekit").join("config.toml"),
        provekit.join("config.toml"),
    )
    .expect("copy config.toml");

    for surface in ["go-bind", "go-contracts"] {
        let manifest = format!(
            "name = \"{surface}\"\ncommand = [\"{}\", \"--rpc\"]\nworking_dir = \".\"\n",
            lift_bin.display()
        );
        fs::write(
            provekit.join("lift").join(surface).join("manifest.toml"),
            manifest,
        )
        .expect("write manifest");
    }

    // HERMETIC: mint's body-template projection
    // (project_body_templates_for_sugar_bindings) writes
    // `<menagerie-root>/<lang>-language-signature/specs/body-templates/...`,
    // locating the menagerie root by walking UP from the process CWD. Give the
    // staged project its OWN menagerie/ dir and run mint with CWD set here (see
    // run_mint), so the projection lands in the tempdir and NEVER clobbers the
    // repo's tracked artifacts. (The non-hermetic walk-up is a tracked
    // follow-up against the shared spine, out of scope for this fix.)
    fs::create_dir_all(project.join("menagerie")).expect("mkdir menagerie");
    project
}

fn run_mint(project: &Path) {
    let out = Command::new(provekit_bin())
        .current_dir(project) // hermetic projection: locate menagerie/ in the tempdir
        .args(["mint", "--project"])
        .arg(project)
        .arg("--out")
        .arg(project)
        .args(["--no-attest", "--quiet"])
        .output()
        .expect("spawn mint");
    assert!(
        out.status.success(),
        "mint must succeed\n  stdout: {}\n  stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn run_verify(project: &Path, witness_dir: &Path) -> (Json, i32) {
    let out = Command::new(provekit_bin())
        .args(["verify", "--project"])
        .arg(project)
        .arg("--emit-witnesses")
        .arg(witness_dir)
        .arg("--json")
        .output()
        .expect("spawn verify");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let receipt = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("verify JSON parse failed: {e}\nstdout: {stdout}"));
    (receipt, out.status.code().unwrap_or(-1))
}

/// DECLARE -> GET A CONTRACT: the authoring surface mints (a) the
/// `library-sugar-binding-entry` declaration catalog for the annotated `Id`,
/// and (b) the function-contract + auto-bridge. `Unannotated` is NOT minted.
#[test]
fn go_authoring_surface_emits_only_declared_boundary() {
    if !go_available() {
        eprintln!("go not on PATH: skipping go authoring-surface test");
        return;
    }
    let lift_bin = build_go_lift_verify();
    let project = stage_authoring_project("declare", &lift_bin, false);
    run_mint(&project);

    let pool = provekit_verifier::load_all_proofs::run(&project);
    assert!(
        pool.load_errors.is_empty(),
        "tool-minted bundle must load cleanly: {:?}",
        pool.load_errors
    );

    // (a) The DECLARATION catalog: a library-sugar-binding-entry for `Id`
    // carrying the declared concept. (b) The auto-bridge for `Id`.
    //
    // The binding-entry envelope shape is the flat `{body, header,
    // schemaVersion}` mint writes (mint_library_sugar_binding_entry); read its
    // fields directly from the envelope (the verifier's `memento_kind` helper
    // only handles the contract/bridge envelope shapes).
    let mut saw_binding_entry_for_id = false;
    for env in pool.mementos.values() {
        let kind = env
            .pointer("/header/kind")
            .or_else(|| env.pointer("/body/kind"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if kind != "library-sugar-binding-entry" {
            continue;
        }
        let body = env.pointer("/body").unwrap_or(env);
        let concept = body
            .get("concept_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let sfn = body
            .get("source_function_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if sfn == "Id" && concept == "identity" {
            saw_binding_entry_for_id = true;
        }
        // Discrimination: the unannotated function must NOT have a
        // declaration entry.
        assert_ne!(
            sfn, "Unannotated",
            "unannotated function must not get a binding-entry"
        );
    }
    assert!(
        saw_binding_entry_for_id,
        "authoring surface must mint a library-sugar-binding-entry for the declared Id (concept=identity); indexed bridges: {:?}",
        pool.bridges_by_symbol.keys().collect::<Vec<_>>()
    );

    // The tool-written bridge for the declared boundary.
    assert!(
        pool.bridges_by_symbol.contains_key("Id"),
        "mint must auto-write a bridge for the declared Id; got {:?}",
        pool.bridges_by_symbol.keys().collect::<Vec<_>>()
    );
    // Discrimination: no bridge for the undeclared function.
    assert!(
        !pool.bridges_by_symbol.contains_key("Unannotated"),
        "the undeclared function must not get a bridge"
    );

    let _ = fs::remove_dir_all(&project);
}

/// VERIFY (positive): the declared boundary's contract discharges through the
/// body, signed witness, exit 0.
#[test]
fn go_authoring_surface_declared_contract_discharges() {
    if !go_available() {
        eprintln!("go not on PATH: skipping");
        return;
    }
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping");
        return;
    }
    let lift_bin = build_go_lift_verify();
    let project = stage_authoring_project("pos", &lift_bin, false);
    run_mint(&project);

    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify(&project, &witnesses);

    assert_eq!(receipt["totalClaims"], 1, "receipt: {receipt}");
    let claim = &receipt["claims"].as_array().expect("claims")[0];
    assert_eq!(claim["pass"], true, "Id(3)==3 must discharge; claim: {claim}");
    assert_eq!(claim["status"], "discharged", "claim: {claim}");
    let witness_cid = claim["witnessCid"].as_str().expect("witness minted");
    assert!(witness_cid.starts_with("blake3-512:"));
    eprintln!("GO_AUTHORING_POSITIVE_WITNESS_CID={witness_cid}");
    assert_eq!(receipt["ok"], true, "receipt: {receipt}");
    assert_eq!(code, 0, "positive run exits clean; got {code}");

    let _ = fs::remove_dir_all(&project);
}

/// VERIFY (negative): break the declared function's body so it no longer
/// satisfies the harvested assertion -> Unsatisfied, exit 1, no witness.
#[test]
fn go_authoring_surface_broken_declared_body_fails() {
    if !go_available() {
        eprintln!("go not on PATH: skipping");
        return;
    }
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping");
        return;
    }
    let lift_bin = build_go_lift_verify();
    let project = stage_authoring_project("neg", &lift_bin, true);
    run_mint(&project);

    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify(&project, &witnesses);

    assert_eq!(receipt["totalClaims"], 1, "receipt: {receipt}");
    let claim = &receipt["claims"].as_array().expect("claims")[0];
    assert_eq!(
        claim["status"], "unsatisfied",
        "broken declared body must be UNSATISFIED; claim: {claim}"
    );
    assert_eq!(claim["pass"], false, "claim: {claim}");
    assert!(claim["witnessCid"].is_null(), "no witness; claim: {claim}");
    assert_eq!(receipt["ok"], false, "receipt: {receipt}");
    assert_eq!(code, 1, "broken-body claim must exit 1; got {code}");
    eprintln!("GO_AUTHORING_NEGATIVE_EXIT_CODE={code}");

    let _ = fs::remove_dir_all(&project);
}
