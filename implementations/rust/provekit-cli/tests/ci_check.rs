// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use serde_json::{json, Value as Json};

fn make_unique_dir(suffix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("provekit-ci-check-{stamp}-{suffix}"));
    fs::create_dir_all(&dir).expect("mkdir");
    dir
}

fn cid(ch: char) -> String {
    format!("blake3-512:{}", ch.to_string().repeat(128))
}

fn to_cvalue(v: &Json) -> Arc<CValue> {
    match v {
        Json::Null => CValue::null(),
        Json::Bool(b) => CValue::boolean(*b),
        Json::Number(n) => CValue::integer(n.as_i64().expect("integer JSON number")),
        Json::String(s) => CValue::string(s.clone()),
        Json::Array(items) => CValue::array(items.iter().map(to_cvalue).collect()),
        Json::Object(map) => CValue::object(map.iter().map(|(k, v)| (k.clone(), to_cvalue(v)))),
    }
}

fn jcs_cid(v: &Json) -> String {
    let jcs = encode_jcs(&to_cvalue(v));
    blake3_512_of(jcs.as_bytes())
}

fn write_json(path: &Path, value: &Json) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("mkdir parent");
    }
    let mut bytes = serde_json::to_string_pretty(value).expect("serialize");
    bytes.push('\n');
    fs::write(path, bytes).expect("write json");
}

fn write_file(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("mkdir parent");
    }
    fs::write(path, text).expect("write file");
}

fn raw_cid(text: &str) -> String {
    blake3_512_of(text.as_bytes())
}

fn make_shadow_repo(suffix: &str) -> PathBuf {
    let repo = make_unique_dir(suffix);

    write_file(
        &repo.join("protocol/specs/2026-04-30-protocol-catalog.json"),
        r#"{"protocol":"provekit","version":"test"}"#,
    );
    write_file(
        &repo.join("protocol/specs/2026-05-07-content-addressed-ci-protocol.md"),
        "# Content-Addressed CI Protocol\n\nCICP test spec v1.\n",
    );
    write_file(
        &repo.join("protocol/conformance/cicp/vectors.json"),
        r#"{"vectors":[]}"#,
    );
    write_file(
        &repo.join("Makefile"),
        "prove-rust:\n\tcargo test\nprove-go:\n\tgo test ./...\n",
    );
    write_file(&repo.join(".github/workflows/ci.yml"), "name: CI\n");

    write_file(
        &repo.join("implementations/rust/Cargo.toml"),
        "[workspace]\n",
    );
    write_file(&repo.join("implementations/rust/Cargo.lock"), "# lock\n");
    write_file(
        &repo.join("implementations/rust/src/lib.rs"),
        "pub fn rust_kit() {}\n",
    );

    write_file(
        &repo.join("implementations/go/go.mod"),
        "module example.com/provekit-go\n",
    );
    write_file(&repo.join("implementations/go/go.sum"), "# sum\n");
    write_file(
        &repo.join("implementations/go/main.go"),
        "package main\nfunc main() {}\n",
    );

    repo
}

fn run_ci_shadow(repo: &Path, kit: &str) -> Json {
    let out_dir = repo.join(".provekit/ci-shadow").join(kit);
    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("ci")
        .arg("shadow")
        .arg("--repo")
        .arg(repo)
        .arg("--kit")
        .arg(kit)
        .arg("--out-dir")
        .arg(out_dir)
        .arg("--runner-identity")
        .arg("github-actions/Linux/X64")
        .arg("--json")
        .output()
        .expect("run provekit ci shadow");

    assert!(
        output.status.success(),
        "status={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    serde_json::from_slice(&output.stdout).expect("shadow summary JSON")
}

fn blast_radius_body() -> Json {
    json!({
        "kind": "CIBlastRadius",
        "schemaVersion": "1",
        "jobKey": "provekit/conformance/rust",
        "subjectKind": "kit",
        "subject": "rust",
        "protocolCatalogCid": cid('1'),
        "jobDefinitionCid": cid('2'),
        "commandCid": cid('3'),
        "runnerIdentityCid": cid('4'),
        "toolchainCids": [cid('5')],
        "sourceClosureCid": cid('6'),
        "lockfileCids": [cid('7')],
        "generatedInputCids": [cid('8')],
        "fixtureCids": [cid('9')],
        "relevantSpecCids": [cid('a')],
        "policyCid": cid('b'),
        "nondeterminism": {
            "network": "forbidden",
            "clock": "forbidden",
            "secrets": "forbidden",
            "randomness": "forbidden"
        },
        "inputCids": [
            cid('1'), cid('2'), cid('3'), cid('4'), cid('5'), cid('6'),
            cid('7'), cid('8'), cid('9'), cid('a'), cid('b')
        ]
    })
}

fn job_result_body(job_key: &str, blast_radius_cid: &str, result: &str, policy_cid: &str) -> Json {
    json!({
        "kind": "CIJobResultBodyClaim",
        "schemaVersion": "1",
        "jobKey": job_key,
        "blastRadiusCid": blast_radius_cid,
        "result": result,
        "outputCid": cid('d'),
        "logCid": cid('e'),
        "startedAt": "2026-05-07T00:00:00Z",
        "finishedAt": "2026-05-07T00:01:00Z",
        "runnerIdentityCid": cid('4'),
        "policyCid": policy_cid,
        "inputCids": [
            blast_radius_cid,
            cid('d'),
            cid('e'),
            cid('4'),
            policy_cid
        ],
        "producer": {
            "kind": "ci-runner",
            "name": "provekit-cli-test",
            "version": "reuse-admission"
        }
    })
}

fn run_ci_reuse(
    current_blast_radius: &Path,
    previous_result: &Path,
    reuse_out: &Path,
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("ci")
        .arg("reuse")
        .arg("--current-blast-radius")
        .arg(current_blast_radius)
        .arg("--previous-result")
        .arg(previous_result)
        .arg("--reuse-out")
        .arg(reuse_out)
        .arg("--json")
        .output()
        .expect("run provekit ci reuse")
}

fn run_ci_reuse_from_accepted(
    current_blast_radius: &Path,
    accepted_dir: &Path,
    reuse_out: &Path,
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("ci")
        .arg("reuse")
        .arg("--current-blast-radius")
        .arg(current_blast_radius)
        .arg("--accepted-dir")
        .arg(accepted_dir)
        .arg("--reuse-out")
        .arg(reuse_out)
        .arg("--json")
        .output()
        .expect("run provekit ci reuse from accepted store")
}

fn run_ci_result(blast_radius: &Path, out: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("ci")
        .arg("result")
        .arg("--blast-radius")
        .arg(blast_radius)
        .arg("--out")
        .arg(out)
        .arg("--json")
        .output()
        .expect("run provekit ci result")
}

fn run_ci_accept(
    repo: &Path,
    kit: &str,
    accepted_dir: &Path,
    extra_args: &[&str],
) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_provekit"));
    command
        .arg("ci")
        .arg("accept")
        .arg("--repo")
        .arg(repo)
        .arg("--kit")
        .arg(kit)
        .arg("--out")
        .arg(accepted_dir)
        .arg("--json");
    for arg in extra_args {
        command.arg(arg);
    }
    command.output().expect("run provekit ci accept")
}

fn git_commit_all(repo: &Path, message: &str) {
    let init = Command::new("git")
        .arg("init")
        .arg(repo)
        .output()
        .expect("git init");
    assert!(
        init.status.success(),
        "git init failed\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&init.stdout),
        String::from_utf8_lossy(&init.stderr)
    );

    let add = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("add")
        .arg(".")
        .output()
        .expect("git add");
    assert!(
        add.status.success(),
        "git add failed\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&add.stdout),
        String::from_utf8_lossy(&add.stderr)
    );

    let commit = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("-c")
        .arg("user.email=provekit@example.test")
        .arg("-c")
        .arg("user.name=ProvekIt Test")
        .arg("commit")
        .arg("-m")
        .arg(message)
        .output()
        .expect("git commit");
    assert!(
        commit.status.success(),
        "git commit failed\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&commit.stdout),
        String::from_utf8_lossy(&commit.stderr)
    );
}

#[test]
fn ci_accept_imports_candidate_job_result_by_default() {
    let repo = make_shadow_repo("accept-generate");
    let accepted_dir = repo.join(".provekit/ci/accepted");
    let shadow = run_ci_shadow(&repo, "rust");
    let blast_cid = shadow["blastRadiusCid"].as_str().expect("blast cid");
    let candidate_path = repo.join(".provekit/ci-shadow/rust/job-result.json");
    let output = run_ci_result(
        &repo.join(".provekit/ci-shadow/rust/blast-radius.json"),
        &candidate_path,
    );
    assert!(
        output.status.success(),
        "candidate result should be emitted\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let output = run_ci_accept(&repo, "rust", &accepted_dir, &[]);

    assert!(
        output.status.success(),
        "status={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: Json = serde_json::from_slice(&output.stdout).expect("accept summary JSON");
    assert_eq!(summary["kind"], "CIAccept");
    assert_eq!(summary["ok"], true);
    assert_eq!(summary["mode"], "write");
    assert_eq!(summary["addedCount"], 1);
    assert_eq!(summary["missingCount"], 0);
    assert_eq!(summary["verifiedCount"], 1);

    assert_eq!(summary["results"][0]["blastRadiusCid"], blast_cid);
    assert_eq!(summary["results"][0]["source"], "candidate-result");
    assert_eq!(
        summary["results"][0]["candidateResultPath"],
        candidate_path.canonicalize().unwrap().display().to_string()
    );
    let accepted_path = accepted_dir
        .join("rust")
        .join(format!("{blast_cid}.job-result.json"));
    assert!(accepted_path.exists(), "accepted witness was written");

    let result: Json = serde_json::from_slice(&fs::read(&accepted_path).expect("read result"))
        .expect("parse result");
    assert_eq!(result["kind"], "CIJobResultBodyClaim");
    assert_eq!(result["blastRadiusCid"], blast_cid);
    assert_eq!(result["result"], "pass");
    let candidate: Json = serde_json::from_slice(&fs::read(&candidate_path).expect("read result"))
        .expect("parse result");
    assert_eq!(result, candidate);

    let current = run_ci_shadow(&repo, "rust");
    assert_eq!(current["blastRadiusCid"], blast_cid);

    let reuse_path = repo.join(".provekit/ci-shadow/reuse.json");
    let output = run_ci_reuse_from_accepted(
        &repo.join(".provekit/ci-shadow/rust/blast-radius.json"),
        &accepted_dir,
        &reuse_path,
    );
    assert!(
        output.status.success(),
        "generated witness should admit reuse\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn ci_accept_write_requires_candidate_result_or_explicit_bootstrap() {
    let repo = make_shadow_repo("accept-requires-result");
    let accepted_dir = repo.join(".provekit/ci/accepted");

    let output = run_ci_accept(&repo, "rust", &accepted_dir, &[]);

    assert!(
        !output.status.success(),
        "write mode should not invent a passing result by default\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(".provekit/ci-shadow/rust/job-result.json"),
        "stderr={stderr}"
    );
    assert!(stderr.contains("--assume-pass"), "stderr={stderr}");
    assert!(
        !accepted_dir.exists(),
        "failed accept must not create accepted witnesses"
    );

    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn ci_accept_check_reports_missing_witnesses_without_writing() {
    let repo = make_shadow_repo("accept-check");
    let accepted_dir = repo.join(".provekit/ci/accepted");

    let output = run_ci_accept(&repo, "rust", &accepted_dir, &["--check"]);

    assert!(
        !output.status.success(),
        "check mode should refuse stale accepted store\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("CICP accepted witnesses are stale"),
        "stderr={stderr}"
    );
    assert!(
        stderr.contains("make ci-accept-refresh"),
        "stderr should point at the release-built repair target\nstderr={stderr}"
    );
    assert!(
        stderr.contains("make ci-accept-check"),
        "stderr should point at the release-built validation target\nstderr={stderr}"
    );
    assert!(
        !accepted_dir.exists(),
        "check mode must not create accepted witnesses"
    );

    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn ci_accept_clean_uses_detached_git_worktree() {
    let repo = make_shadow_repo("accept-clean");
    git_commit_all(&repo, "fixture repo");
    let accepted_dir = repo.join(".provekit/ci/accepted");

    let output = run_ci_accept(&repo, "rust", &accepted_dir, &["--clean", "--assume-pass"]);

    assert!(
        output.status.success(),
        "clean accept should use a git worktree\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: Json = serde_json::from_slice(&output.stdout).expect("accept summary JSON");
    assert_eq!(summary["clean"], true);
    assert_eq!(summary["addedCount"], 1);
    assert_eq!(summary["verifiedCount"], 1);

    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn ci_result_emits_pass_result_for_blast_radius() {
    let dir = make_unique_dir("result-pass");
    let blast_path = dir.join("blast-radius.json");
    let result_path = dir.join("job-result.json");
    let blast = blast_radius_body();
    let blast_cid = jcs_cid(&blast);
    write_json(&blast_path, &blast);

    let output = run_ci_result(&blast_path, &result_path);

    assert!(
        output.status.success(),
        "status={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: Json = serde_json::from_slice(&output.stdout).expect("result summary JSON");
    assert_eq!(summary["kind"], "CIResult");
    assert_eq!(summary["ok"], true);
    assert_eq!(summary["result"], "pass");
    assert_eq!(summary["blastRadiusCid"], blast_cid);
    assert_eq!(summary["bodyPath"], result_path.display().to_string());

    let result: Json = serde_json::from_slice(&fs::read(&result_path).expect("read result body"))
        .expect("parse result body");
    assert_eq!(result["kind"], "CIJobResultBodyClaim");
    assert_eq!(result["jobKey"], "provekit/conformance/rust");
    assert_eq!(result["blastRadiusCid"], blast_cid);
    assert_eq!(result["result"], "pass");
    assert_eq!(result["runnerIdentityCid"], cid('4'));
    assert_eq!(result["policyCid"], cid('b'));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn ci_reuse_admits_identical_pass_result_and_writes_skip_witness() {
    let dir = make_unique_dir("reuse-admit");
    let blast_path = dir.join("blast-radius.json");
    let result_path = dir.join("previous-result.json");
    let reuse_path = dir.join("reuse.json");

    let blast = blast_radius_body();
    let blast_cid = jcs_cid(&blast);
    let policy_cid = blast["policyCid"].as_str().expect("policy cid");
    let result = job_result_body("provekit/conformance/rust", &blast_cid, "pass", policy_cid);
    let result_cid = jcs_cid(&result);
    write_json(&blast_path, &blast);
    write_json(&result_path, &result);

    let output = run_ci_reuse(&blast_path, &result_path, &reuse_path);

    assert!(
        output.status.success(),
        "status={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: Json = serde_json::from_slice(&output.stdout).expect("reuse summary JSON");
    assert_eq!(summary["kind"], "CIReuseAdmission");
    assert_eq!(summary["ok"], true);
    assert_eq!(summary["wouldSkip"], true);
    assert_eq!(summary["skipReason"], "accepted-identical-input-closure");
    assert_eq!(summary["currentBlastRadiusCid"], blast_cid);
    assert_eq!(summary["previousResultWitnessCid"], result_cid);
    assert_eq!(summary["reuseBodyPath"], reuse_path.display().to_string());

    let reuse: Json = serde_json::from_slice(&fs::read(&reuse_path).expect("read reuse witness"))
        .expect("parse reuse witness");
    assert_eq!(reuse["kind"], "CIReuseBodyClaim");
    assert_eq!(reuse["reuseReason"], "identical-input-closure");
    assert_eq!(reuse["currentBlastRadiusCid"], blast_cid);
    assert_eq!(reuse["previousBlastRadiusCid"], blast_cid);
    assert_eq!(reuse["previousResultWitnessCid"], result_cid);
    assert_eq!(reuse["policyCid"], policy_cid);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn ci_reuse_discovers_checked_in_accepted_witness_by_blast_radius() {
    let dir = make_unique_dir("reuse-accepted-store");
    let blast_path = dir.join("blast-radius.json");
    let accepted_dir = dir.join(".provekit/ci/accepted");
    let reuse_path = dir.join("reuse.json");

    let blast = blast_radius_body();
    let blast_cid = jcs_cid(&blast);
    let policy_cid = blast["policyCid"].as_str().expect("policy cid");
    let result = job_result_body("provekit/conformance/rust", &blast_cid, "pass", policy_cid);
    let result_cid = jcs_cid(&result);
    let accepted_path = accepted_dir
        .join("rust")
        .join(format!("{blast_cid}.job-result.json"));
    write_json(&blast_path, &blast);
    write_json(&accepted_path, &result);

    let output = run_ci_reuse_from_accepted(&blast_path, &accepted_dir, &reuse_path);

    assert!(
        output.status.success(),
        "status={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: Json = serde_json::from_slice(&output.stdout).expect("reuse summary JSON");
    assert_eq!(summary["wouldSkip"], true);
    assert_eq!(
        summary["acceptedResultPath"],
        accepted_path.display().to_string()
    );
    assert_eq!(summary["previousResultWitnessCid"], result_cid);
    assert!(reuse_path.exists(), "accepted lookup writes reuse witness");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn ci_reuse_refuses_missing_checked_in_accepted_witness() {
    let dir = make_unique_dir("reuse-missing-accepted");
    let blast_path = dir.join("blast-radius.json");
    let accepted_dir = dir.join(".provekit/ci/accepted");
    let reuse_path = dir.join("reuse.json");

    let blast = blast_radius_body();
    write_json(&blast_path, &blast);

    let output = run_ci_reuse_from_accepted(&blast_path, &accepted_dir, &reuse_path);

    assert!(
        !output.status.success(),
        "missing accepted witness should not admit reuse\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("no accepted result witness"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !reuse_path.exists(),
        "missing accepted result writes nothing"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn ci_reuse_refuses_previous_result_for_different_blast_radius() {
    let dir = make_unique_dir("reuse-refuse-radius");
    let blast_path = dir.join("blast-radius.json");
    let result_path = dir.join("previous-result.json");
    let reuse_path = dir.join("reuse.json");

    let blast = blast_radius_body();
    let policy_cid = blast["policyCid"].as_str().expect("policy cid");
    let result = job_result_body("provekit/conformance/rust", &cid('c'), "pass", policy_cid);
    write_json(&blast_path, &blast);
    write_json(&result_path, &result);

    let output = run_ci_reuse(&blast_path, &result_path, &reuse_path);

    assert!(
        !output.status.success(),
        "changed blast radius should be refused\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("previous result blastRadiusCid does not match current blast radius"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !reuse_path.exists(),
        "refused reuse must not write a skip witness"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn ci_shadow_emits_distinct_per_kit_blast_radii_with_protocol_inputs() {
    let repo = make_shadow_repo("shadow-distinct");

    let rust = run_ci_shadow(&repo, "rust");
    let go = run_ci_shadow(&repo, "go");

    assert_eq!(rust["kind"], "CIShadow");
    assert_eq!(rust["kit"], "rust");
    assert_eq!(rust["wouldSkip"], false);
    assert_eq!(rust["blastRadius"]["jobKey"], "provekit/ci/rust");
    assert_eq!(go["blastRadius"]["jobKey"], "provekit/ci/go");

    assert_ne!(
        rust["blastRadiusCid"], go["blastRadiusCid"],
        "each kit must have its own blast-radius CID"
    );
    assert_ne!(
        rust["blastRadius"]["sourceClosureCid"], go["blastRadius"]["sourceClosureCid"],
        "source closures must stay kit-specific"
    );

    let rust_body_path = repo.join(
        rust["blastRadiusPath"]
            .as_str()
            .expect("blastRadiusPath string"),
    );
    assert!(
        rust_body_path.exists(),
        "body path exists: {rust_body_path:?}"
    );

    let body: Json =
        serde_json::from_slice(&fs::read(&rust_body_path).expect("read blast-radius body"))
            .expect("parse body");
    let relevant = body["relevantSpecCids"]
        .as_array()
        .expect("relevantSpecCids array");
    assert!(
        relevant.iter().any(|cid| cid
            == &Json::String(raw_cid(
                "# Content-Addressed CI Protocol\n\nCICP test spec v1.\n"
            ))),
        "CICP protocol spec CID must be in the kit blast radius"
    );
    assert!(
        relevant.iter().any(|cid| cid
            == &Json::String(raw_cid(r#"{"protocol":"provekit","version":"test"}"#))),
        "protocol catalog spec file CID must be in the kit blast radius"
    );

    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn ci_shadow_protocol_spec_change_invalidates_same_kit_radius() {
    let repo = make_shadow_repo("shadow-protocol-change");

    let before = run_ci_shadow(&repo, "rust");
    write_file(
        &repo.join("protocol/specs/2026-05-07-content-addressed-ci-protocol.md"),
        "# Content-Addressed CI Protocol\n\nCICP test spec v2.\n",
    );
    let after = run_ci_shadow(&repo, "rust");

    assert_ne!(
        before["blastRadiusCid"], after["blastRadiusCid"],
        "protocol spec edits must invalidate kit blast-radius CIDs"
    );
    assert_ne!(
        before["blastRadius"]["relevantSpecCids"], after["blastRadius"]["relevantSpecCids"],
        "the protocol spec CID set should reflect the edited spec"
    );

    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn ci_check_accepts_valid_blast_radius_body() {
    let dir = make_unique_dir("accept");
    let body_path = dir.join("blast-radius.json");
    let body = blast_radius_body();
    write_json(&body_path, &body);

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("ci")
        .arg("check")
        .arg("--body")
        .arg(&body_path)
        .arg("--json")
        .output()
        .expect("run provekit ci check");

    assert!(
        output.status.success(),
        "status={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let summary: Json = serde_json::from_slice(&output.stdout).expect("summary JSON");
    assert_eq!(summary["kind"], "CICheck");
    assert_eq!(summary["ok"], true);
    assert_eq!(summary["bodyKind"], "CIBlastRadius");
    assert_eq!(summary["bodyCid"], jcs_cid(&body));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn ci_check_refuses_open_input_closure() {
    let dir = make_unique_dir("refuse");
    let body_path = dir.join("blast-radius-open.json");
    let mut body = blast_radius_body();
    body["inputCids"] = json!([cid('1')]);
    write_json(&body_path, &body);

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("ci")
        .arg("check")
        .arg("--body")
        .arg(&body_path)
        .output()
        .expect("run provekit ci check");

    assert!(
        !output.status.success(),
        "open input closure should be refused\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("inputCids missing required CID"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}
