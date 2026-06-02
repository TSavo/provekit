// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde_json::{json, Value as Json};

const RUNTIME_FAILURE_SITE_CONCEPT: &str = "concept:panic-freedom.leaf.runtime-failure-site";

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

fn unique_dir(suffix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("provekit-java-source-runtime-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
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

fn maven_available() -> bool {
    run_with_timeout(Command::new("mvn").arg("-version"), Duration::from_secs(8))
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn java_source_lift_command() -> Option<Vec<String>> {
    static JAVA_SOURCE_LIFT_COMMAND: OnceLock<Option<Vec<String>>> = OnceLock::new();
    JAVA_SOURCE_LIFT_COMMAND
        .get_or_init(|| {
            let Some(java) = java_bin() else {
                eprintln!(
                    "no working java binary found: skipping java-source runtime-failure mint test"
                );
                return None;
            };
            if !maven_available() {
                eprintln!("mvn unavailable: skipping java-source runtime-failure mint test");
                return None;
            }

            let root = repo_root();
            let mut mvn = Command::new("mvn");
            mvn.current_dir(root.join("implementations").join("java"))
                .args([
                    "-B",
                    "-ntp",
                    "-pl",
                    "provekit-lift-java-source",
                    "-am",
                    "-DskipTests",
                    "package",
                ]);
            let out =
                run_with_timeout(&mut mvn, Duration::from_secs(120)).expect("spawn mvn package");
            assert!(
                out.status.success(),
                "mvn package provekit-lift-java-source failed\n  stdout: {}\n  stderr: {}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let jar = root
                .join("implementations")
                .join("java")
                .join("provekit-lift-java-source")
                .join("target")
                .join("provekit-lift-java-source.jar");
            assert!(
                jar.exists(),
                "maven build produced no jar at {}",
                jar.display()
            );
            Some(vec![
                java.display().to_string(),
                "-jar".to_string(),
                jar.display().to_string(),
                "--rpc".to_string(),
            ])
        })
        .clone()
}

fn toml_string_array(values: &[String]) -> String {
    let quoted = values
        .iter()
        .map(|v| format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{quoted}]")
}

fn stage_java_source_project(command: &[String]) -> PathBuf {
    let project = unique_dir("project");
    fs::write(
        project.join("Thrower.java"),
        r#"public class Thrower {
static int fail(int x) {
if (x < 0) {
throw new IllegalStateException("neg");
}
return x;
}
}
"#,
    )
    .expect("write Thrower.java");

    let provekit = project.join(".provekit");
    fs::create_dir_all(provekit.join("lift").join("java-source"))
        .expect("mkdir .provekit/lift/java-source");
    fs::write(
        provekit.join("config.toml"),
        r#"[[plugins]]
name = "java-source"
kind = "lift"
surface = "java-source"
"#,
    )
    .expect("write config.toml");
    fs::write(
        provekit
            .join("lift")
            .join("java-source")
            .join("manifest.toml"),
        format!(
            r#"name = "java-source"
version = "0.1.0-draft"
protocol_version = "pep/1.7.0"
kind = "lift"
command = {}
working_dir = "."

[capabilities]
authoring_surfaces = ["java-source"]
ir_version = "v1.1.0"
emits_signed_mementos = false
"#,
            toml_string_array(command)
        ),
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
        "provekit mint must succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn contract_runtime_failure_loci(pool: &provekit_verifier::types::MementoPool) -> Vec<Json> {
    pool.mementos
        .values()
        .filter(|env| provekit_verifier::types::memento_kind(env) == Some("contract"))
        .filter_map(|env| provekit_verifier::types::memento_body_field(env, "panicLoci"))
        .filter_map(|value| value.as_array())
        .flat_map(|items| items.iter().cloned())
        .collect()
}

#[test]
fn java_source_throw_mint_preserves_runtime_failure_locus_and_enumerates_callsite() {
    let Some(command) = java_source_lift_command() else {
        return;
    };
    let project = stage_java_source_project(&command);
    run_mint(&project);

    let pool = provekit_verifier::load_all_proofs::run(&project);
    assert!(
        pool.load_errors.is_empty(),
        "java-source proof must load cleanly: {:?}",
        pool.load_errors
    );

    let loci = contract_runtime_failure_loci(&pool);
    assert_eq!(
        loci,
        vec![json!({
            "effectKind": "concept:panic-freedom",
            "callee": RUNTIME_FAILURE_SITE_CONCEPT,
            "subkind": "explicit-throw",
            "exceptionClass": "IllegalStateException",
            "argTerm": {
                "kind": "ctor",
                "name": "java:new",
                "args": [
                    {
                        "kind": "const",
                        "sort": {"kind": "primitive", "name": "String"},
                        "value": "IllegalStateException"
                    },
                    {
                        "kind": "const",
                        "sort": {"kind": "primitive", "name": "String"},
                        "value": "neg"
                    }
                ]
            },
            "file": "Thrower.java",
            "line": 4,
            "col": 1
        })],
        "mint must preserve the java-source runtime-failure panicLoci row"
    );

    let callsites = provekit_verifier::enumerate_callsites::run(&pool);
    let runtime_failure_sites: Vec<_> = callsites
        .iter()
        .filter(|cs| cs.panic_site && cs.callee.as_deref() == Some(RUNTIME_FAILURE_SITE_CONCEPT))
        .collect();
    assert_eq!(
        runtime_failure_sites.len(),
        1,
        "verifier must surface exactly one Java runtime-failure panic site; got {callsites:#?}"
    );
    assert_eq!(
        runtime_failure_sites[0].file.as_deref(),
        Some("Thrower.java")
    );
    assert_eq!(runtime_failure_sites[0].line, Some(4));
    assert!(
        runtime_failure_sites[0].bridge_target_cid.is_empty(),
        "no bridge exists yet, so the surfaced callsite must remain undecidable"
    );

    let _ = fs::remove_dir_all(&project);
}
