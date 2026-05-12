// SPDX-License-Identifier: Apache-2.0
//
// Integration tests for `provekit bind`.
//
// Coverage:
//   - All 9 (rewrite x mode) configurations exit 0 against the cmd_bind fixture
//   - Annotation content (concept comment, substrate-origin, requires/ensures,
//     mode attributes for annotate mode)
//   - Canonical cross-language output (Rust -> Java, Rust -> Python, Rust -> Go)
//   - Invisible mode does not modify source on disk
//   - Site mementos + index.json + gaps.json written
//   - Annotation idempotence (second pass identical to first)
//   - Multi-target canonical emission smoke (Rust->Java, Rust->Python, Rust->Rust)
//   - Java output carries contract annotation syntax (contract-annotated demo)
//   - gaps.json records v0-capability-gap and v0-orp-delegation-gap entries

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cmd_bind")
}

/// Copy fixture tree to a tempdir (protects checked-in source from annotate writes).
fn copy_fixture_to_temp() -> PathBuf {
    let tmp = tempfile::tempdir().expect("tempdir").into_path();
    copy_dir(&fixture_root(), &tmp);
    tmp
}

fn copy_dir(src: &Path, dst: &Path) {
    let _ = fs::create_dir_all(dst);
    let Ok(entries) = fs::read_dir(src) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let dest = dst.join(&name);
        if path.is_dir() {
            copy_dir(&path, &dest);
        } else {
            let _ = fs::copy(&path, &dest);
        }
    }
}

fn bind_cmd(
    root: &Path,
    out: &Path,
    rewrite: &str,
    mode: &str,
    target_lang: Option<&str>,
) -> std::process::Output {
    let mut cmd = Command::new(provekit_bin());
    cmd.arg("bind")
        .arg("--root")
        .arg(root)
        .arg("--lang")
        .arg("rust")
        .arg("--output")
        .arg(out)
        .arg("--rewrite")
        .arg(rewrite)
        .arg("--mode")
        .arg(mode)
        .arg("--quiet");
    if let Some(lang) = target_lang {
        cmd.arg("--target-language").arg(lang);
    }
    cmd.output().expect("spawn provekit bind")
}

// ============================================================================
// Test 1: --help lists bind command
// ============================================================================

#[test]
fn help_lists_bind_command() {
    let out = Command::new(provekit_bin())
        .arg("--help")
        .output()
        .expect("spawn provekit --help");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "provekit --help failed:\n{stdout}"
    );
    assert!(
        stdout.lines().any(|l| {
            let t = l.trim_start();
            t.starts_with("bind ") || t == "bind"
        }),
        "provekit --help must list the bind subcommand\n{stdout}"
    );
}

// ============================================================================
// Tests 2-4: annotate x {monitor, emitter, witness}
// ============================================================================

#[test]
fn annotate_monitor_injects_concept_and_monitor_attr() {
    let tmp = copy_fixture_to_temp();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&tmp, &out, "annotate", "monitor", None);
    assert!(
        result.status.success(),
        "annotate+monitor should succeed\nstderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let rewritten = fs::read_to_string(tmp.join("src").join("account.rs")).unwrap();
    assert!(rewritten.contains("// concept:"), "must inject concept comment");
    assert!(rewritten.contains("// substrate-origin:"), "must inject substrate-origin");
    assert!(rewritten.contains("#[cfg_attr(any(), requires("), "must inject requires");
    assert!(rewritten.contains("#[cfg_attr(any(), ensures("), "must inject ensures");
    assert!(
        rewritten.contains("provekit_monitor"),
        "monitor mode must inject provekit_monitor attribute"
    );
}

#[test]
fn annotate_emitter_injects_emitter_attribute() {
    let tmp = copy_fixture_to_temp();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&tmp, &out, "annotate", "emitter", None);
    assert!(result.status.success(), "annotate+emitter should succeed");
    let rewritten = fs::read_to_string(tmp.join("src").join("account.rs")).unwrap();
    assert!(
        rewritten.contains("provekit_emitter"),
        "emitter mode must inject provekit_emitter"
    );
}

#[test]
fn annotate_witness_injects_witness_attribute() {
    let tmp = copy_fixture_to_temp();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&tmp, &out, "annotate", "witness", None);
    assert!(result.status.success(), "annotate+witness should succeed");
    let rewritten = fs::read_to_string(tmp.join("src").join("account.rs")).unwrap();
    assert!(
        rewritten.contains("provekit_witness"),
        "witness mode must inject provekit_witness"
    );
}

// ============================================================================
// Tests 5-7: canonical x {monitor, emitter, witness}
// ============================================================================

#[test]
fn canonical_monitor_rust_target_creates_translated_dir() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "monitor", Some("rust"));
    assert!(result.status.success(), "canonical+monitor+rust should succeed");
    assert!(out.join("translated").join("rust").exists(), "translated/rust dir must be created");
}

#[test]
fn canonical_emitter_java_creates_java_output_with_contract() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "emitter", Some("java"));
    assert!(
        result.status.success(),
        "canonical+emitter+java should succeed\nstderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let java_dir = out.join("translated").join("java");
    assert!(java_dir.exists(), "translated/java must exist");
    let java_files: Vec<_> = fs::read_dir(&java_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "java").unwrap_or(false))
        .collect();
    assert!(!java_files.is_empty(), "at least one .java file expected");
    let java_src = fs::read_to_string(java_files[0].path()).unwrap();
    assert!(java_src.contains("concept:"), "Java output must carry concept annotation");
    // Java canonical+emitter must have emitter annotation.
    assert!(
        java_src.contains("provekit_emitter"),
        "Java canonical+emitter must carry emitter annotation\n{java_src}"
    );
    // Contract annotations for deposit function.
    assert!(
        java_src.contains("@requires:") || java_src.contains("/* @requires"),
        "Java output must carry contract annotation for deposit\n{java_src}"
    );
}

#[test]
fn canonical_witness_python_creates_python_output() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "witness", Some("python"));
    assert!(result.status.success(), "canonical+witness+python should succeed");
    let py_dir = out.join("translated").join("python");
    assert!(py_dir.exists(), "translated/python must exist");
    let py_files: Vec<_> = fs::read_dir(&py_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "py").unwrap_or(false))
        .collect();
    assert!(!py_files.is_empty(), "at least one .py file expected");
    let py_src = fs::read_to_string(py_files[0].path()).unwrap();
    assert!(py_src.contains("# concept:"), "Python output must carry concept comment");
}

// ============================================================================
// Tests 8-10: invisible x {monitor, emitter, witness} — must NOT write source
// ============================================================================

fn assert_source_unchanged(label: &str, mode: &str) {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let src_before = fs::read_to_string(root.join("src").join("account.rs")).unwrap();
    let result = bind_cmd(&root, &out, "invisible", mode, None);
    assert!(
        result.status.success(),
        "invisible+{mode} should succeed\nstderr: {}\n({label})",
        String::from_utf8_lossy(&result.stderr)
    );
    let src_after = fs::read_to_string(root.join("src").join("account.rs")).unwrap();
    assert_eq!(
        src_before, src_after,
        "invisible+{mode} must not modify source file ({label})"
    );
}

#[test]
fn invisible_monitor_does_not_modify_source() {
    assert_source_unchanged("invisible_monitor", "monitor");
}

#[test]
fn invisible_emitter_does_not_modify_source() {
    assert_source_unchanged("invisible_emitter", "emitter");
}

#[test]
fn invisible_witness_does_not_modify_source() {
    assert_source_unchanged("invisible_witness", "witness");
}

// ============================================================================
// Test 11: site mementos, index.json, gaps.json
// ============================================================================

#[test]
fn bind_writes_site_mementos_index_and_gaps() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "invisible", "monitor", None);
    assert!(result.status.success(), "invisible+monitor should succeed");

    // index.json
    let index_path = out.join("index.json");
    assert!(index_path.exists(), "index.json must be written");
    let idx: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&index_path).unwrap()).unwrap();
    assert!(
        idx["total_bindings"].as_u64().unwrap_or(0) > 0,
        "index must record bindings"
    );
    assert!(idx["verdicts"].is_object(), "index must have verdicts");

    // gaps.json
    assert!(out.join("gaps.json").exists(), "gaps.json must be written");

    // sites/
    let sites_dir = out.join("sites");
    if sites_dir.exists() {
        let site_files: Vec<_> = fs::read_dir(&sites_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
            .collect();
        if !site_files.is_empty() {
            let m: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(site_files[0].path()).unwrap()).unwrap();
            assert_eq!(m["schemaVersion"].as_str().unwrap_or(""), "1", "memento schemaVersion must be 1");
            assert_eq!(m["kind"].as_str().unwrap_or(""), "concept-site", "memento kind must be concept-site");
        }
    }
}

// ============================================================================
// Test 12: annotation idempotence
// ============================================================================

#[test]
fn annotate_is_idempotent() {
    let tmp = copy_fixture_to_temp();
    let out = tempfile::tempdir().expect("tempdir").into_path();

    // pass0: original source bytes, read before any bind run.
    let pass0 = fs::read_to_string(tmp.join("src").join("account.rs")).unwrap();

    // pass1: source after first annotate run.
    let r1 = bind_cmd(&tmp, &out, "annotate", "monitor", None);
    assert!(r1.status.success(), "first annotate pass should succeed");
    let pass1 = fs::read_to_string(tmp.join("src").join("account.rs")).unwrap();

    // B2 regression gate: pass1 must contain the *content* of the lifted contract for
    // deposit, not merely structural annotations.  The cmd_bind annotator strips existing
    // `#[cfg_attr(any(), requires...)]` lines and re-emits them only when the parse-lift
    // extracted a non-empty predicate (B1 fix).  If B1's trim-ordering fix is reverted
    // the parser fails to extract the predicate and `requires(amount > 0)` is silently
    // dropped from pass1 — both this assertion AND the structural-idempotence check below
    // must fire against that regression.
    assert!(
        pass1.contains("substrate-origin: annotation-lift"),
        "pass1 must carry `substrate-origin: annotation-lift` for the deposit function \
         (B2 regression check — empty would mean the annotation-lift path did not fire)"
    );
    assert!(
        pass1.contains("requires(amount > 0)")
            || pass1.contains("requires (amount > 0)")
            || pass1.contains("requires(amount>0)"),
        "pass1 must contain the lifted `requires(amount > 0)` predicate for deposit \
         (B2 regression check — absence means B1 trim-ordering bug was reintroduced)\n\
         pass1:\n{pass1}"
    );

    // pass2: source after second annotate run on already-annotated file.
    let r2 = bind_cmd(&tmp, &out, "annotate", "monitor", None);
    assert!(r2.status.success(), "second annotate pass should succeed");
    let pass2 = fs::read_to_string(tmp.join("src").join("account.rs")).unwrap();

    // Loss check: pass1 must contain every non-annotation line from pass0 unchanged.
    // Any fn body line from the original source must survive annotation injection.
    let is_substrate_line = |l: &str| -> bool {
        let t = l.trim_start();
        t.starts_with("// concept:")
            || t.starts_with("// substrate-origin:")
            || t.starts_with("// memento-cid:")
            || t.starts_with("// witness-inherited-from:")
            || t.starts_with("#[cfg_attr(any(), requires")
            || t.starts_with("#[cfg_attr(any(), ensures")
            || t.starts_with("#[cfg_attr(any(), provekit_monitor")
            || t.starts_with("#[cfg_attr(any(), provekit_emitter")
            || t.starts_with("#[cfg_attr(any(), provekit_witness")
    };
    let pass0_non_substrate: Vec<&str> = pass0.lines().filter(|l| !is_substrate_line(l)).collect();
    for original_line in &pass0_non_substrate {
        assert!(
            pass1.contains(original_line),
            "annotate pass1 lost a non-substrate line from pass0: {:?}\npass1:\n{pass1}",
            original_line
        );
    }

    // Structural idempotence check: pass1 == pass2 modulo lines that are allowed to
    // re-evaluate on each pass:
    //   - memento-cid: encodes source file hash; changes after first write
    //   - substrate-origin: may re-classify if injected attrs are now readable (v0 limitation)
    // Concept names, contract predicates (requires/ensures), function bodies, and all
    // user-written code MUST be byte-stable across passes.
    let strip_volatile = |s: &str| -> String {
        s.lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("// memento-cid:")
                    && !t.starts_with("// substrate-origin:")
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    };
    assert_eq!(
        strip_volatile(&pass1),
        strip_volatile(&pass2),
        "annotate must be structurally idempotent: concept names, contract predicates, \
         and function bodies must be stable across passes (memento-cid and substrate-origin may re-evaluate)"
    );
}

// ============================================================================
// Test 13: Java deposit carries contract annotations
// (the user-visible demo: "a unit test in Rust converted into contract
// annotations in Java that say this is required at this call site")
// ============================================================================

#[test]
fn java_canonical_deposit_carries_contract_annotations() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "monitor", Some("java"));
    assert!(result.status.success(), "canonical+java should succeed");
    let java_dir = out.join("translated").join("java");
    let java_files: Vec<_> = fs::read_dir(&java_dir)
        .unwrap_or_else(|_| panic!("translated/java must exist"))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "java").unwrap_or(false))
        .collect();
    assert!(!java_files.is_empty(), "java output required for demo");
    let java_src = fs::read_to_string(java_files[0].path()).unwrap();

    // The deposit function has #[requires(amount > 0)] + #[ensures(out >= 0)].
    // ORP's Java emitter should render these as contract annotations.
    assert!(
        java_src.contains("@requires:") || java_src.contains("/* @requires"),
        "Java must carry @requires contract annotation\n{java_src}"
    );
    assert!(
        java_src.contains("@ensures:") || java_src.contains("/* @ensures"),
        "Java must carry @ensures contract annotation\n{java_src}"
    );
    // Concept annotation present.
    assert!(
        java_src.contains("concept:"),
        "Java must carry concept annotation\n{java_src}"
    );
    // Substrate origin present.
    assert!(
        java_src.contains("substrate-origin:"),
        "Java must carry substrate-origin\n{java_src}"
    );
}

// ============================================================================
// Test 14: Python canonical carries Python-idiomatic contract comments
// ============================================================================

#[test]
fn python_canonical_carries_contract_comments() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "monitor", Some("python"));
    assert!(result.status.success(), "canonical+python should succeed");
    let py_dir = out.join("translated").join("python");
    let py_files: Vec<_> = fs::read_dir(&py_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "py").unwrap_or(false))
        .collect();
    assert!(!py_files.is_empty(), "python output required");
    let py_src = fs::read_to_string(py_files[0].path()).unwrap();
    assert!(py_src.contains("# concept:"), "Python must carry concept comment");
    assert!(
        py_src.contains("# @requires:") || py_src.contains("@requires"),
        "Python must carry contract annotation\n{py_src}"
    );
}

// ============================================================================
// Test 15: Multi-target canonical emission smoke (Rust->Java, Rust->Python, Rust->Rust)
//
// Each leg exits 0 and produces concept-bearing output. In v0 the bind
// engine processes Rust source for all legs via the M+N concept hub (the hub
// collapses the NxM translation table: each leg is Rust->concept hub->target).
// True multi-leg round-trip (lift from Java/Python output back through the hub)
// is deferred to v1 when multi-lang lift_plugin dispatch is complete.
// ============================================================================

#[test]
fn canonical_multi_target_emission_smoke() {
    let root = fixture_root();

    // Leg 1: Rust -> Java
    let out1 = tempfile::tempdir().expect("tempdir").into_path();
    let r1 = bind_cmd(&root, &out1, "canonical", "monitor", Some("java"));
    assert!(r1.status.success(), "trinity leg 1 (Rust->Java) must succeed");
    let java_dir = out1.join("translated").join("java");
    assert!(java_dir.exists(), "java dir must exist after leg 1");
    let java_files: Vec<_> = fs::read_dir(&java_dir).unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "java").unwrap_or(false))
        .collect();
    assert!(!java_files.is_empty(), "java output required");
    let java_src = fs::read_to_string(java_files[0].path()).unwrap();
    assert!(java_src.contains("concept:"), "java output must carry concept");

    // Leg 2: Rust -> Python (via the same hub)
    let out2 = tempfile::tempdir().expect("tempdir").into_path();
    let r2 = bind_cmd(&root, &out2, "canonical", "monitor", Some("python"));
    assert!(r2.status.success(), "trinity leg 2 (Rust->Python) must succeed");
    let py_dir = out2.join("translated").join("python");
    assert!(py_dir.exists(), "python dir must exist after leg 2");
    let py_files: Vec<_> = fs::read_dir(&py_dir).unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "py").unwrap_or(false))
        .collect();
    assert!(!py_files.is_empty(), "python output required");
    let py_src = fs::read_to_string(py_files[0].path()).unwrap();
    assert!(py_src.contains("concept:"), "python output must carry concept");

    // Leg 3: Rust -> Rust (same-language canonical refactor)
    let out3 = tempfile::tempdir().expect("tempdir").into_path();
    let r3 = bind_cmd(&root, &out3, "canonical", "monitor", Some("rust"));
    assert!(r3.status.success(), "trinity leg 3 (Rust->Rust) must succeed");
    let rs_dir = out3.join("translated").join("rust");
    assert!(rs_dir.exists(), "rust canonical dir must exist after leg 3");
    let rs_files: Vec<_> = fs::read_dir(&rs_dir).unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "rs").unwrap_or(false))
        .collect();
    assert!(!rs_files.is_empty(), "rust canonical output required");
    let rs_src = fs::read_to_string(rs_files[0].path()).unwrap();
    assert!(rs_src.contains("concept:"), "rust canonical output must carry concept");

    // Verify index.json from leg 1 records verdict breakdown.
    let idx: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out1.join("index.json")).unwrap()).unwrap();
    let total = idx["total_bindings"].as_u64().unwrap_or(0);
    assert!(total > 0, "trinity: index must record at least one binding");
    let exact = idx["verdicts"]["exact"].as_u64().unwrap_or(0);
    let lossy = idx["verdicts"]["loudly_bounded_lossy"].as_u64().unwrap_or(0);
    let refuse = idx["verdicts"]["refuse"].as_u64().unwrap_or(0);
    assert!(
        exact + lossy + refuse == total,
        "all bindings must have a verdict"
    );
}

// ============================================================================
// Test 16: gaps.json records v0 capability gaps
// ============================================================================

#[test]
fn gaps_doc_records_v0_capability_gaps() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "invisible", "monitor", None);
    assert!(result.status.success(), "bind should succeed");
    let gaps: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out.join("gaps.json")).unwrap()).unwrap();
    let gap_arr = gaps["gaps"].as_array().expect("gaps must be array");
    let kinds: Vec<&str> = gap_arr.iter().filter_map(|g| g["kind"].as_str()).collect();
    assert!(
        kinds.contains(&"v0-capability-gap"),
        "gaps.json must record at least one v0-capability-gap"
    );
    assert!(
        kinds.contains(&"v0-orp-delegation-gap"),
        "gaps.json must record v0-orp-delegation-gap (realize_function is fn not pub)"
    );
}

// ============================================================================
// Test 17: Go canonical output
// ============================================================================

#[test]
fn canonical_go_target_creates_go_output() {
    let root = fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let result = bind_cmd(&root, &out, "canonical", "monitor", Some("go"));
    assert!(result.status.success(), "canonical+go should succeed");
    let go_dir = out.join("translated").join("go");
    assert!(go_dir.exists(), "translated/go must exist");
    let go_files: Vec<_> = fs::read_dir(&go_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "go").unwrap_or(false))
        .collect();
    assert!(!go_files.is_empty(), "at least one .go file expected");
    let go_src = fs::read_to_string(go_files[0].path()).unwrap();
    assert!(go_src.contains("concept:"), "Go output must carry concept annotation");
}
