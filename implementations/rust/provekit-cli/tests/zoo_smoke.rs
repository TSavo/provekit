// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

fn provekit_bin() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_provekit") {
        return PathBuf::from(path);
    }

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace = PathBuf::from(manifest_dir).parent().unwrap().to_path_buf();
    let release = workspace.join("target").join("release").join("provekit");
    let debug = workspace.join("target").join("debug").join("provekit");
    if release.exists() {
        release
    } else {
        debug
    }
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

fn command_succeeds(name: &str, arg: &str, path: Option<&OsString>) -> bool {
    let mut command = Command::new(name);
    command.arg(arg);
    if let Some(path) = path {
        command.env("PATH", path);
    }
    command
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn java_preflight_path() -> Result<OsString, String> {
    if !command_succeeds("mvn", "--version", None) {
        return Err("mvn is not available on PATH".into());
    }

    let mut paths = vec![
        PathBuf::from("/usr/local/opt/openjdk/bin"),
        PathBuf::from("/opt/homebrew/opt/openjdk/bin"),
    ];
    if let Some(current_path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&current_path));
    }
    let path = std::env::join_paths(paths).expect("test PATH entries must be joinable");

    if !command_succeeds("java", "-version", Some(&path)) {
        return Err("java is not available on PATH or Homebrew OpenJDK fallback paths".into());
    }
    if !command_succeeds("javac", "-version", Some(&path)) {
        return Err("javac is not available on PATH or Homebrew OpenJDK fallback paths".into());
    }

    Ok(path)
}

#[test]
fn java_null_boundary_specimen_checks() {
    let path = match java_preflight_path() {
        Ok(path) => path,
        Err(reason) => {
            eprintln!("zoo smoke: skipping Java/Maven-backed specimen test: {reason}");
            return;
        }
    };

    let root = repo_root();
    let specimen = root.join("bug-zoo/species/BZ-SHAPE-005-java-null-boundary-equivalence");
    let out = Command::new(provekit_bin())
        .arg("zoo")
        .arg(&specimen)
        .arg("--json")
        .env("PATH", path)
        .current_dir(&root)
        .output()
        .expect("spawn provekit zoo");

    assert!(
        out.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout must be strict JSON only");
    assert_eq!(json["ok"], true);
    assert_eq!(json["errors"].as_array().map(Vec::is_empty), Some(true));
    assert_eq!(
        json["setupErrors"].as_array().map(Vec::is_empty),
        Some(true)
    );
    assert_eq!(
        json["verificationErrors"].as_array().map(Vec::is_empty),
        Some(true)
    );

    let reports = json["reports"]
        .as_array()
        .expect("reports must be an array");
    assert_eq!(reports.len(), 1, "expected one specimen report");
    let report = &reports[0];
    assert_eq!(report["missingEdge"], "maybe_null(name) => non_null(name)");
    assert_eq!(report["dropperAvailable"], true);
    assert_eq!(report["dropper"]["status"], "closed");
    assert_eq!(report["dropper"]["surface"], "java-provekit-native");
    assert_eq!(report["dropper"]["targetSymbol"], "lookup");
    assert_eq!(report["dropper"]["proofVar"], "name");
    assert_eq!(
        report["dropper"]["proofPlan"]["policyMode"],
        "proof_preferred"
    );
    assert_eq!(
        report["dropper"]["proofPlan"]["violationCondition"],
        "maybe_null(name) && !non_null(name)"
    );
    assert_eq!(
        report["dropper"]["languageDropper"]["operation"],
        "add-boundary-precondition"
    );
    assert_eq!(
        report["dropper"]["languageDropper"]["proofPlanCid"],
        report["dropper"]["proofPlan"]["cid"]
    );

    let native_cid = report["proofIrCids"]["provekit-native"]
        .as_str()
        .expect("provekit-native CID must be a string");
    let spring_cid = report["proofIrCids"]["spring-web"]
        .as_str()
        .expect("spring-web CID must be a string");
    assert!(native_cid.starts_with("blake3-512:"));
    assert_eq!(native_cid, spring_cid);
}

#[test]
fn typescript_discover_cli_finds_null_boundary_with_language_lifter() {
    let root = repo_root();
    let discover = root.join(
        "bug-zoo/species/BZ-SHAPE-006-typescript-null-boundary-equivalence/tools/ts-boundary-discover.ts",
    );
    let harness = root.join(
        "bug-zoo/species/BZ-SHAPE-006-typescript-null-boundary-equivalence/exposed/zod/harness",
    );

    let output = Command::new("pnpm")
        .arg("exec")
        .arg("tsx")
        .arg(discover)
        .arg("zod")
        .arg(harness)
        .current_dir(&root)
        .output()
        .expect("spawn typescript discover cli");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "typescript discover failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("\"kind\":\"bug-zoo-discovery\""));
    assert!(stdout.contains("\"surface\":\"zod\""));
    assert!(stdout.contains("\"lifter\":\"liftPath\""));
    assert!(stdout.contains("\"missingEdge\":\"maybe_null(name) => non_null(name)\""));
    assert!(stdout.contains("\"irEvidenceCid\":"));
}
