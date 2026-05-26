// SPDX-License-Identifier: Apache-2.0
//
// JAVA end-to-end production-bridge gauntlet: real Java source is lifted by
// the Java kit, `provekit mint` auto-writes the body-discharge bridge, and
// `provekit verify` discharges a nonzero claim through the shared verifier.
//
// The Rust CLI stays language-neutral here: it only consumes normalized IR
// returned by the Java lift RPC. Java parsing and source interpretation remain
// Java kit work.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde_json::Value as Json;

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

/// CARGO_MANIFEST_DIR = .../implementations/rust/provekit-cli; three parents up
/// is the repo root.
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

fn java_bin() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(java_home) = std::env::var_os("JAVA_HOME") {
        candidates.push(PathBuf::from(java_home).join("bin").join("java"));
    }
    candidates.push(PathBuf::from("java"));
    candidates.push(PathBuf::from(
        "/usr/local/opt/openjdk/libexec/openjdk.jdk/Contents/Home/bin/java",
    ));
    candidates.push(PathBuf::from(
        "/opt/homebrew/opt/openjdk/libexec/openjdk.jdk/Contents/Home/bin/java",
    ));

    candidates.into_iter().find(|candidate| {
        run_with_timeout(
            Command::new(candidate).arg("-version"),
            Duration::from_secs(5),
        )
        .map(|o| o.status.success())
        .unwrap_or(false)
    })
}

fn javac_bin(java: &Path) -> PathBuf {
    java.parent()
        .map(|bin| bin.join("javac"))
        .unwrap_or_else(|| PathBuf::from("javac"))
}

fn maven_available() -> bool {
    run_with_timeout(Command::new("mvn").arg("-version"), Duration::from_secs(8))
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn unique_dir(suffix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("provekit-java-prod-bridge-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir project");
    p
}

#[derive(Clone, Debug)]
struct JavaLiftCommand {
    command: Vec<String>,
}

/// Build the real Java lift kit once and return the command used by the
/// `.provekit/lift/java/manifest.toml` fixture. Maven is the preferred path;
/// a bounded javac fallback keeps this gauntlet runnable on local machines
/// where `mvn` itself is unavailable or blocked before it reaches the build.
fn java_lift_command() -> Option<JavaLiftCommand> {
    static JAVA_LIFT_COMMAND: OnceLock<Option<JavaLiftCommand>> = OnceLock::new();
    JAVA_LIFT_COMMAND
        .get_or_init(|| {
            let Some(java) = java_bin() else {
                eprintln!("no working java binary found: skipping java production-bridge test");
                return None;
            };

            let root = repo_root();
            if maven_available() {
                let mut mvn = Command::new("mvn");
                mvn.current_dir(&root).args([
                    "-q",
                    "-f",
                    "implementations/java/pom.xml",
                    "-pl",
                    "provekit-lift-java-core",
                    "-am",
                    "package",
                    "-DskipTests",
                ]);
                let out = run_with_timeout(&mut mvn, Duration::from_secs(120))
                    .expect("spawn mvn package");
                assert!(
                    out.status.success(),
                    "mvn package provekit-lift-java-core failed\n  stdout: {}\n  stderr: {}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );
                let jar = root
                    .join("implementations")
                    .join("java")
                    .join("provekit-lift-java-core")
                    .join("target")
                    .join("provekit-lsp-java.jar");
                assert!(
                    jar.exists(),
                    "maven build produced no jar at {}",
                    jar.display()
                );
                return Some(JavaLiftCommand {
                    command: vec![
                        java.display().to_string(),
                        "-jar".to_string(),
                        jar.display().to_string(),
                        "--rpc".to_string(),
                    ],
                });
            }

            eprintln!("mvn unavailable or timed out: using bounded javac build for java lifter");
            build_java_lift_with_javac(&root, &java).map(|classpath| JavaLiftCommand {
                command: vec![
                    java.display().to_string(),
                    "-cp".to_string(),
                    classpath,
                    "com.provekit.lift.Main".to_string(),
                    "--rpc".to_string(),
                ],
            })
        })
        .clone()
}

/// Write a real Java source fixture plus project lift config. The positive case
/// returns `x * 2`; the negative case returns `x * 3` while the harvested
/// assertion remains `twice(3) == 6`.
fn stage_java_project(suffix: &str, lift: &JavaLiftCommand, body_factor: i64) -> PathBuf {
    let project = unique_dir(suffix);
    let source = format!(
        r#"public class App {{
    static int twice(int x) {{
        return x * {body_factor};
    }}

    static void check() {{
        assert twice(3) == 6;
    }}
}}
"#
    );
    fs::write(project.join("App.java"), source).expect("write App.java");

    let provekit = project.join(".provekit");
    fs::create_dir_all(provekit.join("lift").join("java")).expect("mkdir .provekit/lift/java");

    let config = r#"[authoring]
surface = "java"

[solvers]
default = "z3"

[solvers.dispatch]
linear_arithmetic = "z3"
default = "z3"

[solvers.z3]
binary = "z3"
flags = ["-smt2", "-in"]
"#;
    fs::write(provekit.join("config.toml"), config).expect("write config.toml");

    let manifest = format!(
        "name = \"java\"\ncommand = {}\nworking_dir = \".\"\n",
        toml_string_array(&lift.command)
    );
    fs::write(
        provekit.join("lift").join("java").join("manifest.toml"),
        manifest,
    )
    .expect("write manifest.toml");

    project
}

fn run_mint(project: &Path) {
    let out = Command::new(provekit_bin())
        .arg("mint")
        .arg("--project")
        .arg(project)
        .arg("--out")
        .arg(project)
        .arg("--no-attest")
        .arg("--quiet")
        .output()
        .expect("spawn provekit mint");
    assert!(
        out.status.success(),
        "provekit mint must succeed\n  stdout: {}\n  stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn run_verify_json_with_code(project: &Path, witness_dir: &Path) -> (Json, i32) {
    let out = Command::new(provekit_bin())
        .arg("verify")
        .arg("--project")
        .arg(project)
        .arg("--emit-witnesses")
        .arg(witness_dir)
        .arg("--json")
        .output()
        .expect("spawn provekit verify");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let receipt = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("verify JSON parse failed: {e}\nstdout: {stdout}"));
    (receipt, out.status.code().unwrap_or(-1))
}

fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> std::io::Result<Output> {
    let mut child = cmd.spawn()?;
    let started = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            return child.wait_with_output();
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            return child.wait_with_output();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn build_java_lift_with_javac(root: &Path, java: &Path) -> Option<String> {
    let javac = javac_bin(java);
    if !javac.exists() {
        eprintln!("javac not found next to {}; skipping", java.display());
        return None;
    }

    let javaparser = home_path(
        ".m2/repository/com/github/javaparser/javaparser-core/3.26.4/javaparser-core-3.26.4.jar",
    );
    let bcprov = home_path(
        ".m2/repository/org/bouncycastle/bcprov-jdk18on/1.78.1/bcprov-jdk18on-1.78.1.jar",
    );
    if !javaparser.exists() || !bcprov.exists() {
        eprintln!(
            "javac fallback dependencies missing ({} or {}); skipping",
            javaparser.display(),
            bcprov.display()
        );
        return None;
    }

    let classes =
        std::env::temp_dir().join(format!("provekit-java-lift-classes-{}", std::process::id()));
    let _ = fs::remove_dir_all(&classes);
    fs::create_dir_all(&classes).expect("mkdir java classes");

    let mut sources = Vec::new();
    collect_java_sources(
        &root
            .join("implementations")
            .join("java")
            .join("provekit-ir")
            .join("src")
            .join("main")
            .join("java"),
        &mut sources,
    );
    collect_java_sources(
        &root
            .join("implementations")
            .join("java")
            .join("provekit-lift-java-core")
            .join("src")
            .join("main")
            .join("java"),
        &mut sources,
    );
    sources.sort();

    let dep_classpath = format!("{}:{}", javaparser.display(), bcprov.display());
    let mut javac_cmd = Command::new(&javac);
    javac_cmd
        .arg("-cp")
        .arg(&dep_classpath)
        .arg("-d")
        .arg(&classes)
        .args(&sources);
    let out = run_with_timeout(&mut javac_cmd, Duration::from_secs(60)).expect("spawn javac");
    assert!(
        out.status.success(),
        "javac fallback build failed\n  stdout: {}\n  stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    Some(format!("{}:{}", classes.display(), dep_classpath))
}

fn collect_java_sources(dir: &Path, sources: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_java_sources(&path, sources);
        } else if path.extension().and_then(|s| s.to_str()) == Some("java") {
            sources.push(path);
        }
    }
}

fn home_path(relative: &str) -> PathBuf {
    let home = std::env::var_os("HOME").expect("HOME set");
    PathBuf::from(home).join(relative)
}

fn toml_string_array(values: &[String]) -> String {
    let quoted = values
        .iter()
        .map(|v| format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{quoted}]")
}

#[test]
fn java_mint_auto_writes_body_discharge_bridge() {
    let Some(lift) = java_lift_command() else {
        return;
    };
    let project = stage_java_project("bridge", &lift, 2);
    run_mint(&project);

    let pool = provekit_verifier::load_all_proofs::run(&project);
    assert!(
        pool.load_errors.is_empty(),
        "tool-minted bundle must load cleanly: {:?}",
        pool.load_errors
    );

    let bridge = pool.bridges_by_symbol.get("twice").unwrap_or_else(|| {
        panic!(
            "mint must auto-write + index a bridge with sourceSymbol=twice; indexed: {:?}",
            pool.bridges_by_symbol.keys().collect::<Vec<_>>()
        )
    });

    let target_cid = provekit_verifier::types::memento_body_field(bridge, "targetContractCid")
        .and_then(|v| v.as_str())
        .expect("bridge must carry targetContractCid")
        .to_string();

    let target = pool.mementos.get(&target_cid).unwrap_or_else(|| {
        panic!("bridge.targetContractCid {target_cid} must resolve to a member")
    });
    assert_eq!(
        provekit_verifier::types::memento_kind(target),
        Some("contract"),
        "bridge target must be a contract memento"
    );
    let formals = provekit_verifier::types::memento_body_field(target, "formals")
        .and_then(|v| v.as_array())
        .expect("tool-written op-contract must carry formals");
    assert_eq!(
        formals.first().and_then(|v| v.as_str()),
        Some("x"),
        "op-contract formals must be [x]"
    );
    assert!(
        provekit_verifier::types::memento_body_field(target, "post").is_some(),
        "op-contract must carry the body-derived post"
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn java_production_path_assertion_discharges_and_mints_witness() {
    let Some(lift) = java_lift_command() else {
        return;
    };
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping java production-bridge positive test");
        return;
    }
    let project = stage_java_project("pos", &lift, 2);
    run_mint(&project);

    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify_json_with_code(&project, &witnesses);

    assert_eq!(
        receipt["kind"], "verification-receipt",
        "receipt: {receipt}"
    );
    assert_eq!(
        receipt["totalClaims"], 1,
        "exactly one body-bearing callsite (the tool-written bridge made twice(3) enumerate); receipt: {receipt}"
    );
    let claim = &receipt["claims"].as_array().expect("claims")[0];
    assert_eq!(
        claim["pass"], true,
        "twice(3)==6 must discharge through body (* x 2); claim: {claim}"
    );
    assert_eq!(claim["status"], "discharged", "claim: {claim}");

    let solver = claim["dischargingSolver"].as_str().unwrap_or("");
    assert!(
        solver.starts_with("z3@"),
        "discharging solver must be z3; got `{solver}`"
    );

    let witness_cid = claim["witnessCid"].as_str().expect("witness minted");
    assert!(witness_cid.starts_with("blake3-512:"));
    eprintln!("JAVA_PRODUCTION_POSITIVE_WITNESS_CID={witness_cid}");

    assert_eq!(receipt["ok"], true, "receipt: {receipt}");
    assert_eq!(code, 0, "positive run exits clean; got {code}");

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn java_production_path_broken_body_fails_unsatisfied_no_witness() {
    let Some(lift) = java_lift_command() else {
        return;
    };
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping java production-bridge negative test");
        return;
    }
    let project = stage_java_project("neg", &lift, 3);
    run_mint(&project);

    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify_json_with_code(&project, &witnesses);

    assert_eq!(receipt["totalClaims"], 1, "receipt: {receipt}");
    let claim = &receipt["claims"].as_array().expect("claims")[0];
    assert_eq!(
        claim["status"], "unsatisfied",
        "broken body x*3 must be UNSATISFIED (not undecidable); claim: {claim}"
    );
    assert_eq!(claim["pass"], false, "claim: {claim}");
    assert!(
        claim["witnessCid"].is_null(),
        "no witness for a violated claim; claim: {claim}"
    );
    assert_eq!(receipt["ok"], false, "receipt: {receipt}");

    let witness_files: Vec<_> = fs::read_dir(&witnesses)
        .map(|rd| rd.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();
    assert!(
        witness_files.is_empty(),
        "witness dir must be empty for a violated claim; found {} files",
        witness_files.len()
    );

    assert_eq!(
        code, 1,
        "broken-body claim must exit 1 (EXIT_VERIFY_FAIL, not 3=undecidable); got {code}"
    );
    eprintln!(
        "JAVA_PRODUCTION_NEGATIVE_EXIT_CODE={code} STATUS={}",
        claim["status"]
    );

    let _ = fs::remove_dir_all(&project);
}
