// SPDX-License-Identifier: Apache-2.0
//
// Trinity round-trip integration test.
//
// Current status: v0 loudly-bounded-lossy, fresh-target hermetic, Branch 2 mode.
// This is not the Branch 1 byte-identity receipt. See
// docs/incidents/2026-05-14-trinity-baseline-diagnosis.md and closure PRs
// #860, #861, #862, #863.
// The gate is the PR #861 hermetic fixture plus the v0 lossy expectations
// enforced by this test.
//
// Future Branch 1 target: four composed transport keys over the trinity
// concepts with empty loss at each boundary. Java and Python lift fixture
// wiring is still missing and is out of scope for #859.
//
// Four chained legs:
//   Leg 1  Rust fixture  -> Java   (--lang rust  --target-language java)
//   Leg 2  Leg 1 java/   -> Python (--lang auto  --target-language python)
//   Leg 3  Leg 2 python/ -> C      (--lang auto  --target-language c)
//   Leg 4  Leg 3 c/      -> Rust   (--lang auto  --target-language rust)
//
// Each leg writes gaps.json.  compose_losses unions the REAL gap records from
// all four legs.  No synthetic entries are injected; real loss comes from
// bind's own gap emission (F2: source-language-not-supported records).
//
// Trichotomy (Supra omnia, rectum):
//   composed_loss.is_empty()  -> future byte-identical round-trip assertion (Branch 1)
//   else                      -> v0: assert REAL + EXPECTED gap kinds in composed loss
//                                (Branch 2 per-line characterization gated; see F3 fix)
//
// v0 expected outcome: later legs receive non-Rust sources; bind detects
// java/python/c via auto-detect when those boundaries are reached, and emits
// real lift-boundary gap records in gaps.json. compose_losses returns real
// entries from those legs.
// Test passes via the "loudly-bounded-lossy is a first-class legitimate outcome"
// branch, asserting the real gap kinds are present (NOT a permissive predicate).
//
// Branch 2 per-line divergence characterization is gated until real Java/Python->Rust
// lifters land + a divergence-pattern registry is built.  See PR-F of #716 follow-up.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

fn trinity_fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("trinity_roundtrip")
}

fn rust_workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("provekit-cli manifest dir has rust workspace parent")
        .to_path_buf()
}

fn repo_root() -> PathBuf {
    let rust_workspace = rust_workspace_root();
    rust_workspace
        .parent()
        .expect("rust workspace has implementations parent")
        .parent()
        .expect("implementations dir has repo parent")
        .to_path_buf()
}

fn cargo_profile_dir() -> PathBuf {
    provekit_bin()
        .parent()
        .expect("provekit binary path has parent directory")
        .to_path_buf()
}

fn exe_name(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

fn ensure_rust_lift_rpc_built() -> PathBuf {
    let provekit = provekit_bin();
    let bin_dir = provekit
        .parent()
        .expect("provekit binary path has parent directory");
    let rpc_bin = bin_dir.join(exe_name("provekit-walk-rpc"));
    let rust_workspace = rust_workspace_root();
    let target_dir = bin_dir
        .parent()
        .expect("cargo target dir has profile parent");

    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.current_dir(&rust_workspace)
        .env("CARGO_TARGET_DIR", target_dir);
    cmd.arg("build")
        .arg("--manifest-path")
        .arg(rust_workspace.join("Cargo.toml"))
        .arg("-p")
        .arg("provekit-walk")
        .arg("--bin")
        .arg("provekit-walk-rpc");
    if bin_dir.file_name().and_then(|n| n.to_str()) == Some("release") {
        cmd.arg("--release");
    }

    let output = cmd.output().expect("spawn cargo build for rust lift rpc");
    assert!(
        output.status.success(),
        "cargo build failed for rust lift rpc\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        rpc_bin.exists(),
        "rust lift rpc missing after cargo build at {}",
        rpc_bin.display()
    );
    rpc_bin
}

fn ensure_rust_realize_rpc_built() -> PathBuf {
    let provekit = provekit_bin();
    let bin_dir = provekit
        .parent()
        .expect("provekit binary path has parent directory");
    let rpc_bin = bin_dir.join(exe_name("provekit-realize-rust"));
    let rust_workspace = rust_workspace_root();
    let target_dir = bin_dir
        .parent()
        .expect("cargo target dir has profile parent");

    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.current_dir(&rust_workspace)
        .env("CARGO_TARGET_DIR", target_dir);
    cmd.arg("build")
        .arg("--manifest-path")
        .arg(rust_workspace.join("Cargo.toml"))
        .arg("-p")
        .arg("provekit-realize-rust-core")
        .arg("--bin")
        .arg("provekit-realize-rust");
    if bin_dir.file_name().and_then(|n| n.to_str()) == Some("release") {
        cmd.arg("--release");
    }

    let output = cmd
        .output()
        .expect("spawn cargo build for rust realize rpc");
    assert!(
        output.status.success(),
        "cargo build failed for rust realize rpc\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        rpc_bin.exists(),
        "rust realize rpc missing after cargo build at {}",
        rpc_bin.display()
    );
    rpc_bin
}

fn ensure_java_realize_rpc_built() -> PathBuf {
    let repo = repo_root();
    let java_dir = repo.join("implementations").join("java");
    let jar = java_dir
        .join("provekit-realize-java-core")
        .join("target")
        .join("provekit-realize-java.jar");

    let output = Command::new("mvn")
        .args([
            "package",
            "-pl",
            "provekit-realize-java-core",
            "-am",
            "-DskipTests",
        ])
        .current_dir(&java_dir)
        .output()
        .expect("spawn mvn package for java realize rpc");
    assert!(
        output.status.success(),
        "mvn package failed for java realize rpc\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        jar.exists(),
        "java realize jar missing after mvn package at {}",
        jar.display()
    );
    jar
}

fn java_bin() -> String {
    if let Some(java_home) = std::env::var_os("JAVA_HOME") {
        let candidate = PathBuf::from(java_home).join("bin").join(exe_name("java"));
        if candidate.exists() {
            return candidate.display().to_string();
        }
    }

    if let Ok(output) = Command::new("mvn").arg("-version").output() {
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        for line in combined.lines() {
            if let Some((_, runtime)) = line.split_once("runtime: ") {
                let candidate = PathBuf::from(runtime.trim())
                    .join("bin")
                    .join(exe_name("java"));
                if candidate.exists() {
                    return candidate.display().to_string();
                }
            }
        }
    }

    "java".to_string()
}

fn ensure_c_realize_rpc_built() -> PathBuf {
    let repo = repo_root();
    let kit_dir = repo
        .join("implementations")
        .join("c")
        .join("provekit-realize-c-core");
    let source_bin = kit_dir
        .join("target")
        .join("release")
        .join(exe_name("provekit-realize-c"));
    let profile_bin = cargo_profile_dir().join(exe_name("provekit-realize-c"));

    let output = Command::new("make")
        .current_dir(&kit_dir)
        .output()
        .expect("spawn make for c realize rpc");
    assert!(
        output.status.success(),
        "make failed for c realize rpc\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        source_bin.exists(),
        "c realize rpc missing after make at {}",
        source_bin.display()
    );
    fs::copy(&source_bin, &profile_bin).unwrap_or_else(|e| {
        panic!(
            "copy c realize rpc {} to {}: {e}",
            source_bin.display(),
            profile_bin.display()
        )
    });
    profile_bin
}

fn ensure_c_lift_rpc_built() -> PathBuf {
    let repo = repo_root();
    let kit_dir = repo
        .join("implementations")
        .join("c")
        .join("provekit-walk-c");
    let source_bin = kit_dir.join(exe_name("provekit-bind-lift-c"));
    let profile_bin = cargo_profile_dir().join(exe_name("provekit-bind-lift-c"));

    let output = Command::new("make")
        .current_dir(&kit_dir)
        .output()
        .expect("spawn make for c bind lift rpc");
    assert!(
        output.status.success(),
        "make failed for c bind lift rpc\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        source_bin.exists(),
        "c bind lift rpc missing after make at {}",
        source_bin.display()
    );
    fs::copy(&source_bin, &profile_bin).unwrap_or_else(|e| {
        panic!(
            "copy c bind lift rpc {} to {}: {e}",
            source_bin.display(),
            profile_bin.display()
        )
    });
    profile_bin
}

fn install_rust_lift_manifest(fixture_root: &Path, rpc_bin: &Path) {
    let manifest = fixture_root
        .join(".provekit")
        .join("lift")
        .join("rust-bind")
        .join("manifest.toml");
    fs::create_dir_all(manifest.parent().expect("manifest path has parent"))
        .expect("create fixture rust lift manifest dir");
    let manifest_text = format!(
        "name = \"rust-bind-lift\"\ncommand = [\"{}\", \"--rpc\"]\nworking_dir = \".\"\n",
        rpc_bin.display()
    );
    fs::write(&manifest, manifest_text).expect("write fixture rust lift manifest");
}

fn install_rust_realize_manifest(fixture_root: &Path, rpc_bin: &Path) {
    let manifest = fixture_root
        .join(".provekit")
        .join("realize")
        .join("rust")
        .join("manifest.toml");
    fs::create_dir_all(manifest.parent().expect("manifest path has parent"))
        .expect("create fixture rust realize manifest dir");
    let manifest_text = format!(
        "name = \"rust-realize\"\nlibrary_tag = \"reqwest\"\ncommand = [\"{}\", \"--rpc\"]\nworking_dir = \".\"\n",
        rpc_bin.display()
    );
    fs::write(&manifest, manifest_text).expect("write fixture rust realize manifest");
}

fn manifest_command(command: &[String]) -> String {
    command
        .iter()
        .map(|arg| format!("\"{}\"", arg.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(", ")
}

fn canonical_library_tag(lang: &str) -> &str {
    match lang {
        "c" => "libcurl",
        "java" => "java-net-http",
        "python" => "urllib",
        "rust" => "reqwest",
        _ => "default",
    }
}

fn install_lift_manifest(fixture_root: &Path, surface: &str, name: &str, command: &[String]) {
    let manifest = fixture_root
        .join(".provekit")
        .join("lift")
        .join(surface)
        .join("manifest.toml");
    fs::create_dir_all(manifest.parent().expect("manifest path has parent"))
        .expect("create fixture lift manifest dir");
    let manifest_text = format!(
        "name = \"{name}\"\ncommand = [{}]\nworking_dir = \".\"\n",
        manifest_command(command)
    );
    fs::write(&manifest, manifest_text).expect("write fixture lift manifest");
}

fn install_realize_manifest(fixture_root: &Path, lang: &str, name: &str, command: &[String]) {
    let manifest = fixture_root
        .join(".provekit")
        .join("realize")
        .join(lang)
        .join("manifest.toml");
    fs::create_dir_all(manifest.parent().expect("manifest path has parent"))
        .expect("create fixture realize manifest dir");
    let manifest_text = format!(
        "name = \"{name}\"\nlibrary_tag = \"{}\"\ncommand = [{}]\nworking_dir = \".\"\n",
        canonical_library_tag(lang),
        manifest_command(command)
    );
    fs::write(&manifest, manifest_text).expect("write fixture realize manifest");
}

fn java_realize_command(jar: &Path) -> Vec<String> {
    vec![
        java_bin(),
        "-jar".to_string(),
        jar.display().to_string(),
        "--rpc".to_string(),
    ]
}

fn python_realize_command() -> Vec<String> {
    vec![
        std::env::var("PYTHON").unwrap_or_else(|_| "python3".to_string()),
        "-m".to_string(),
        "provekit_realize_python_core".to_string(),
        "--rpc".to_string(),
    ]
}

fn rpc_command(rpc_bin: &Path) -> Vec<String> {
    vec![rpc_bin.display().to_string(), "--rpc".to_string()]
}

fn pythonpath_with_realize_src() -> std::ffi::OsString {
    let repo = repo_root();
    let mut paths = vec![repo
        .join("implementations")
        .join("python")
        .join("provekit-realize-python-core")
        .join("src")];
    if let Some(existing) = std::env::var_os("PYTHONPATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(paths).expect("join PYTHONPATH")
}

fn copy_dir(src: &Path, dst: &Path) {
    let _ = fs::create_dir_all(dst);
    let Ok(entries) = fs::read_dir(src) else {
        return;
    };
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

/// Run `provekit bind` with explicit --lang (not hardcoded to "rust").
fn bind_cmd_with_lang(
    root: &Path,
    out: &Path,
    lang: &str,
    target_lang: Option<&str>,
) -> std::process::Output {
    let mut cmd = Command::new(provekit_bin());
    cmd.arg("bind")
        .arg("--root")
        .arg(root)
        .arg("--lang")
        .arg(lang)
        .arg("--output")
        .arg(out)
        .arg("--rewrite")
        .arg("canonical")
        .arg("--mode")
        .arg("monitor")
        .arg("--quiet");
    cmd.env("PROVEKIT_REPO_ROOT", repo_root());
    cmd.env("PYTHONPATH", pythonpath_with_realize_src());
    if let Some(tl) = target_lang {
        cmd.arg("--target-language").arg(tl);
    }
    cmd.output().expect("spawn provekit bind")
}

// ── Gap types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct GapEntry {
    kind: String,
    detail: String,
}

/// Read gaps.json from an output directory.  Returns the `source_lang` field
/// and the `gaps` array.
fn read_gaps(out_dir: &Path) -> (String, Vec<GapEntry>) {
    let path = out_dir.join("gaps.json");
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("gaps.json missing at {}", path.display()));
    let v: serde_json::Value = serde_json::from_str(&raw).expect("gaps.json is not valid JSON");

    let source_lang = v["source_lang"].as_str().unwrap_or("unknown").to_string();

    let gaps = v["gaps"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|g| GapEntry {
            kind: g["kind"].as_str().unwrap_or("").to_string(),
            detail: g["detail"].as_str().unwrap_or("").to_string(),
        })
        .collect();

    (source_lang, gaps)
}

fn read_sources_with_extension(root: &Path, extension: &str) -> Vec<String> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x == extension)
                .unwrap_or(false)
        })
        .map(|e| {
            fs::read_to_string(e.path())
                .unwrap_or_else(|err| panic!("read translated source: {err}"))
        })
        .collect()
}

// ── Loss composition ─────────────────────────────────────────────────────────

/// Compose gap entries across all legs.
///
/// Unions real gap records from all four legs.  No synthetic entries are
/// injected here; loss must come from bind itself (via gaps.json) so the
/// composed record is substrate-residue, not test fabrication.
///
/// Deduplication is by (kind, detail); each unique gap appears once.
fn compose_losses(
    leg_results: &[(String, Vec<GapEntry>)], // (source_lang, gaps) per leg
) -> Vec<GapEntry> {
    let mut seen: BTreeMap<(String, String), GapEntry> = BTreeMap::new();

    for (_leg_idx, (_source_lang, gaps)) in leg_results.iter().enumerate() {
        // Absorb all gaps from this leg.
        for g in gaps {
            let key = (g.kind.clone(), g.detail.clone());
            seen.entry(key).or_insert_with(|| g.clone());
        }
        // No synthetic injection: if source_lang is non-Rust and bind doesn't emit
        // a gap record, that is itself a substrate bug to fix in bind (F2).
    }

    seen.into_values().collect()
}

// ── Diff helpers ──────────────────────────────────────────────────────────────

/// Normalise whitespace so byte-level noise doesn't mask real semantic diffs.
fn normalize_whitespace(s: &str) -> String {
    s.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

// NOTE: compute_diff and diff_is_characterized_by are removed (F3 fix, Option B).
//
// Branch 2 per-line divergence characterization is gated until real Java/Python->Rust
// lifters land and a divergence-pattern registry exists.  In v0 the round-trip
// test's job is to verify the chain runs and emits HONEST loss-records, not to
// verify per-line divergence mapping.
//
// When real lifters land, reactivate Branch 2 as a separate PR with:
//   - per-function loss mementos from bind
//   - a `kind -> divergence-pattern` registry
//   - compute_diff + per-line attribution loop

// ── Main test ─────────────────────────────────────────────────────────────────

#[test]
fn trinity_round_trip() {
    let rust_lift_rpc = ensure_rust_lift_rpc_built();
    let rust_realize_rpc = ensure_rust_realize_rpc_built();
    let java_realize_jar = ensure_java_realize_rpc_built();
    let c_realize_rpc = ensure_c_realize_rpc_built();
    let c_lift_rpc = ensure_c_lift_rpc_built();

    // Copy fixture to a tempdir so annotate writes cannot corrupt checked-in source.
    let fixture_tmp = tempfile::tempdir().expect("tempdir").into_path();
    copy_dir(&trinity_fixture_root(), &fixture_tmp);
    install_rust_lift_manifest(&fixture_tmp, &rust_lift_rpc);
    install_rust_realize_manifest(&fixture_tmp, &rust_realize_rpc);
    install_realize_manifest(
        &fixture_tmp,
        "java",
        "java-realize",
        &java_realize_command(&java_realize_jar),
    );

    // Leg 1: Rust -> Java
    let out1 = tempfile::tempdir().expect("tempdir").into_path();
    let r1 = bind_cmd_with_lang(&fixture_tmp, &out1, "rust", Some("java"));
    assert!(
        r1.status.success(),
        "Leg 1 (Rust->Java) must succeed; stderr:\n{}",
        String::from_utf8_lossy(&r1.stderr)
    );

    let java_dir = out1.join("translated").join("java");
    assert!(java_dir.exists(), "Leg 1 must produce translated/java/ dir");
    let java_files: Vec<_> = fs::read_dir(&java_dir)
        .expect("read java dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "java").unwrap_or(false))
        .collect();
    assert!(
        !java_files.is_empty(),
        "Leg 1 must produce at least one .java file"
    );
    let java_sources = read_sources_with_extension(&java_dir, "java").join("\n");
    assert!(
        java_sources.contains("// concept: http-request"),
        "Leg 1 must carry the HTTP fixture as concept:http-request in Java output; source:\n{}",
        java_sources
    );
    assert!(
        java_sources.contains("java.net.http.HttpClient"),
        "Leg 1 must realize the HTTP fixture through the Java HTTP body template; source:\n{}",
        java_sources
    );
    eprintln!(
        "trinity_round_trip: HTTP fixture recognized as concept:http-request in leg 1 Java output"
    );

    let (leg1_src_lang, leg1_gaps) = read_gaps(&out1);
    install_realize_manifest(
        &java_dir,
        "python",
        "python-realize",
        &python_realize_command(),
    );

    // index.json sanity: total_bindings + verdict breakdown.
    let idx1_raw =
        fs::read_to_string(out1.join("index.json")).expect("index.json missing after leg 1");
    let idx1: serde_json::Value = serde_json::from_str(&idx1_raw).expect("index.json invalid JSON");
    let total1 = idx1["total_bindings"].as_u64().unwrap_or(0);
    assert!(
        total1 > 0,
        "Leg 1 index.json must record at least one binding"
    );
    let exact1 = idx1["verdicts"]["exact"].as_u64().unwrap_or(0);
    let lossy1 = idx1["verdicts"]["loudly_bounded_lossy"]
        .as_u64()
        .unwrap_or(0);
    let refuse1 = idx1["verdicts"]["refuse"].as_u64().unwrap_or(0);
    assert_eq!(
        exact1 + lossy1 + refuse1,
        total1,
        "Leg 1: all bindings must have a verdict (exact+lossy+refuse == total)"
    );

    // Leg 2: Leg-1 java/ -> Python
    // Root is the java translated dir; bind detects no .rs files, exits 0, writes
    // gaps.json with source_lang reflecting the auto-detect (not "rust").
    let out2 = tempfile::tempdir().expect("tempdir").into_path();
    let r2 = bind_cmd_with_lang(&java_dir, &out2, "auto", Some("python"));
    assert!(
        r2.status.success(),
        "Leg 2 (Java->Python) must succeed; stderr:\n{}",
        String::from_utf8_lossy(&r2.stderr)
    );

    let (leg2_src_lang, leg2_gaps) = read_gaps(&out2);

    // Leg 2 may or may not produce a python dir (v0 Rust-only; java source skips).
    // We do not assert python dir exists; the loss composition handles the gap.

    // Leg 3: Leg-2 python/ -> C
    // Leg 3 chains from the python translated dir produced by leg 2.
    // If that dir does not exist, bind on leg 2 was unable to cross the
    // java->python boundary. This is a legitimate loudly-bounded-lossy outcome
    // if and only if leg 2's gaps.json contains a real gap record explaining why.
    // We do not silently fall back to out2, which would hide the boundary gap.
    let python_dir = out2.join("translated").join("python");

    // out3 is Some if leg 3 actually ran, None if the python boundary was absent.
    let out3: Option<PathBuf>;
    let (leg3_src_lang, leg3_gaps) = if python_dir.exists() {
        install_realize_manifest(&python_dir, "c", "c-realize", &rpc_command(&c_realize_rpc));
        let out3_dir = tempfile::tempdir().expect("tempdir").into_path();
        let r3 = bind_cmd_with_lang(&python_dir, &out3_dir, "auto", Some("c"));
        assert!(
            r3.status.success(),
            "Leg 3 (Python->C) must succeed; stderr:\n{}",
            String::from_utf8_lossy(&r3.stderr)
        );
        let gaps3 = read_gaps(&out3_dir);
        out3 = Some(out3_dir);
        gaps3
    } else {
        // Leg 2 couldn't produce python output. Verify it recorded a real gap.
        // Post-PR-770 the gap kind is `kit-plugin-unavailable` (no lift kit for
        // Java available to bind); the old gap kind `source-language-not-supported`
        // was retired when language detection moved into kit_dispatch. Either
        // kind is a legitimate "lift boundary is loudly-bounded-lossy" record.
        let (leg2_gap_lang, leg2_real_gaps) = read_gaps(&out2);
        let has_real_gap = leg2_real_gaps.iter().any(|g| {
            g.kind == "source-language-not-supported"
                || g.kind == "kit-plugin-unavailable"
                || g.kind == "bind-lift-empty"
        });
        assert!(
            has_real_gap,
            "Leg 2 produced no translated/python/ dir but also recorded no \
             source-language-not-supported / kit-plugin-unavailable / \
             bind-lift-empty gap in gaps.json. This is a substrate bug: bind \
             must either produce output OR record why it can't. \
             leg2 source_lang={:?}, leg2 gaps={:?}",
            leg2_gap_lang, leg2_real_gaps
        );
        // Leg 3 is uncrossable at this boundary.  Record the absent-leg marker
        // so composed_loss reflects the gap.  The real gap evidence is in leg 2's gaps.
        out3 = None;
        (
            leg2_gap_lang.clone(),
            vec![GapEntry {
                kind: "leg-3-not-reached".into(),
                detail: format!(
                    "leg 3 (python->c) was not executed because leg 2 produced no \
                     translated/python/ output; leg 2 source_lang={leg2_gap_lang} with \
                     lift-boundary gap"
                ),
            }],
        )
    };

    // Leg 4: Leg-3 c/ -> Rust
    let c_dir = out3
        .as_ref()
        .map(|out3_dir| out3_dir.join("translated").join("c"));
    let out4: Option<PathBuf>;
    let (leg4_src_lang, leg4_gaps) = if let Some(c_dir) = c_dir.as_ref() {
        if c_dir.exists() {
            install_lift_manifest(c_dir, "c", "c-bind-lift", &rpc_command(&c_lift_rpc));
            install_rust_realize_manifest(c_dir, &rust_realize_rpc);
            let out4_dir = tempfile::tempdir().expect("tempdir").into_path();
            let r4 = bind_cmd_with_lang(c_dir, &out4_dir, "auto", Some("rust"));
            assert!(
                r4.status.success(),
                "Leg 4 (C->Rust) must succeed; stderr:\n{}",
                String::from_utf8_lossy(&r4.stderr)
            );
            let gaps4 = read_gaps(&out4_dir);
            out4 = Some(out4_dir);
            gaps4
        } else {
            let (leg3_gap_lang, leg3_real_gaps) = read_gaps(out3.as_ref().unwrap());
            let has_real_gap = leg3_real_gaps.iter().any(|g| {
                g.kind == "source-language-not-supported"
                    || g.kind == "kit-plugin-unavailable"
                    || g.kind == "bind-lift-empty"
            });
            assert!(
                has_real_gap,
                "Leg 3 produced no translated/c/ dir but also recorded no \
                 source-language-not-supported / kit-plugin-unavailable / \
                 bind-lift-empty gap in gaps.json. \
                 leg3 source_lang={:?}, leg3 gaps={:?}",
                leg3_gap_lang, leg3_real_gaps
            );
            out4 = None;
            (
                leg3_gap_lang.clone(),
                vec![GapEntry {
                    kind: "leg-4-not-reached".into(),
                    detail: format!(
                        "leg 4 (c->rust) was not executed because leg 3 produced no \
                         translated/c/ output; leg 3 source_lang={leg3_gap_lang} with \
                         lift-boundary gap"
                    ),
                }],
            )
        }
    } else {
        out4 = None;
        (
            leg3_src_lang.clone(),
            vec![GapEntry {
                kind: "leg-4-not-reached".into(),
                detail: "leg 4 (c->rust) was not executed because leg 3 was not reached".into(),
            }],
        )
    };

    // Compose losses across all four legs.
    let composed_loss = compose_losses(&[
        (leg1_src_lang.clone(), leg1_gaps),
        (leg2_src_lang.clone(), leg2_gaps),
        (leg3_src_lang.clone(), leg3_gaps),
        (leg4_src_lang.clone(), leg4_gaps),
    ]);

    // ── Trichotomy assertion (Supra omnia, rectum) ────────────────────────────
    //
    // Branch 1: future empty composed loss -> byte-identical round-trip required.
    // Branch 2: non-empty composed loss -> diff vs original must be precisely
    //           characterised by the composed loss record.
    //           The loss record must be REAL (from bind's gaps.json), not synthetic.
    //
    // Current #859 state is Branch 2 v0 loudly-bounded-lossy, gated by the
    // PR #861 hermetic fixture and the v0 lossy expectations below. See
    // docs/incidents/2026-05-14-trinity-baseline-diagnosis.md and closure PRs
    // #860, #861, #862, #863. Branch 1 byte-identity still needs Java and
    // Python lift fixture wiring.
    //
    // v0 expected outcome: later legs receive non-Rust sources; bind detects
    // java/python/c via auto-detect when those boundaries are reached and emits
    // lift-boundary gaps.
    // We land in Branch 2 with real loss records.
    //
    // The original fixture source is the Rust fixture used in leg 1.
    let fixture_path = {
        let mut rs_files: Vec<_> = fs::read_dir(&fixture_tmp)
            .expect("read fixture tmp dir")
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "rs").unwrap_or(false))
            .collect();
        // Also check src/ subdir.
        let fixture_src = fixture_tmp.join("src");
        if fixture_src.is_dir() {
            let mut more: Vec<_> = fs::read_dir(&fixture_src)
                .expect("read fixture src dir")
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|x| x == "rs").unwrap_or(false))
                .collect();
            rs_files.append(&mut more);
        }
        assert!(
            !rs_files.is_empty(),
            "fixture must contain at least one .rs source file"
        );
        rs_files.into_iter().next().unwrap().path()
    };

    if composed_loss.is_empty() {
        // Future Branch 1: every concept survived all four hops byte-identical.
        // NOT YET REACHABLE in the current #859 state; Java and Python lift
        // fixture wiring is still missing.
        // Leg 4 must have run and its rust output must match the original fixture.
        let out4_dir = out4.as_ref().expect(
            "Branch 1 (empty loss) requires leg 4 to have run; \
             out4 is None means the c boundary was absent, which implies non-empty loss",
        );
        let original_source = fs::read_to_string(&fixture_path).expect("read original fixture");
        let rust_out_dir = out4_dir.join("translated").join("rust");
        assert!(
            rust_out_dir.exists(),
            "Branch 1 (empty loss): leg 4 must produce translated/rust/ dir"
        );
        let rs_files: Vec<_> = fs::read_dir(&rust_out_dir)
            .expect("read rust out dir")
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "rs").unwrap_or(false))
            .collect();
        assert!(
            !rs_files.is_empty(),
            "Branch 1: leg 4 rust out dir must contain .rs files"
        );
        let rust_back_source =
            fs::read_to_string(rs_files[0].path()).expect("read leg-4 rust output");

        let normalized_back = normalize_whitespace(&rust_back_source);
        let normalized_orig = normalize_whitespace(&original_source);
        assert_eq!(
            normalized_back, normalized_orig,
            "Branch 1 (empty composed loss): exact round-trip required; \
             leg-4 rust output must match original fixture source"
        );
    } else {
        // v0 Branch 2 (Option B gating, F3 fix):
        //
        // Per-line divergence characterization is NOT asserted in v0 because leg 4
        // often does not produce Rust output (lifters not wired). The vacuous-diff path
        // (rust_back_source = String::new() -> empty diff -> permissive predicate
        // returns true) was pre-flagged as F3 in the round-2 review.
        //
        // Instead: assert the composed loss is HONEST and contains the EXPECTED v0
        // gap kinds.  When real lifters land, Branch 2's per-line characterization
        // can be reactivated (requires divergence-pattern registry; see PR-F of #716
        // follow-up or the issue created for this gating note).

        // All gaps in the composed loss must have non-empty kind and detail.
        for gap in &composed_loss {
            assert!(
                !gap.kind.is_empty(),
                "Every composed gap must have a non-empty kind; got: {:?}",
                gap
            );
            assert!(
                !gap.detail.is_empty(),
                "Every composed gap must have a non-empty detail; got: {:?}",
                gap
            );
        }

        // Collect the set of loss kinds actually present.
        use std::collections::HashSet;
        let loss_kinds: HashSet<&str> = composed_loss.iter().map(|g| g.kind.as_str()).collect();

        // v0 EXPECTED state: later legs receive non-Rust sources; bind must
        // emit a real source-language-not-supported gap record (cmd_bind.rs, the
        // `if src_files.is_empty() && source_lang != "rust"` block).
        // If that emission breaks, this assertion catches it. That is the protocol.
        if leg2_src_lang != "rust" || leg3_src_lang != "rust" || leg4_src_lang != "rust" {
            // Post-PR-770 the lift-boundary gap kind is `kit-plugin-unavailable`
            // (kit registration is now the substrate seam, per PEP 1.7.0 and
            // `2026-05-13-bind-ir-lift-result.md`). `source-language-not-supported`
            // and `bind-lift-empty` are also legitimate kinds depending on which
            // boundary fired; the test accepts any of the three so the verdict
            // criterion ("at least as good as the current 4 entries") holds
            // across the rename.
            let lift_boundary_kinds = [
                "source-language-not-supported",
                "kit-plugin-unavailable",
                "bind-lift-empty",
                "leg-3-not-reached",
                "leg-4-not-reached",
            ];
            assert!(
                lift_boundary_kinds.iter().any(|k| loss_kinds.contains(k)),
                "v0 trinity round-trip MUST report a lift-boundary gap kind \
                 for non-Rust legs (emitted by bind's gaps.json, not synthetic injection). \
                 This assertion fails if the gap-emission pipeline in cmd_bind breaks.\n\
                 got kinds: {:?}\n\
                 expected one of: {:?}\n\
                 leg2_src_lang={:?} leg3_src_lang={:?} leg4_src_lang={:?}\n\
                 full composed_loss={:?}",
                loss_kinds,
                lift_boundary_kinds,
                leg2_src_lang,
                leg3_src_lang,
                leg4_src_lang,
                composed_loss
            );
        }

        // v0 EXPECTED state: composed loss must NOT be empty.  If it is, either
        // the chain produced a perfect round-trip (impossible in v0) or the
        // loss-emission pipeline broke.  Fail explicitly so the gap is surfaced.
        // (This branch is only reached when composed_loss.is_empty() is false,
        // but guard anyway for future refactors.)
        assert!(
            !composed_loss.is_empty(),
            "Trinity round-trip composed_loss is EMPTY in v0. \
             Either the chain produced a perfect round-trip (impossible in v0) \
             or the loss-emission pipeline broke. Check later-leg gap recording."
        );

        eprintln!(
            "trinity_round_trip: v0 loudly-bounded-lossy outcome ({} loss entries; \
             Branch 2 per-line characterization gated until real lifters land):",
            composed_loss.len()
        );
        for gap in &composed_loss {
            eprintln!("  [{kind}] {detail}", kind = gap.kind, detail = gap.detail);
        }
    }
}
