// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

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
    let p = std::env::temp_dir().join(format!("provekit-java-implications-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> std::io::Result<std::process::Output> {
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

fn java_lift_command() -> Option<Vec<String>> {
    static JAVA_LIFT_COMMAND: OnceLock<Option<Vec<String>>> = OnceLock::new();
    JAVA_LIFT_COMMAND
        .get_or_init(|| {
            let Some(java) = java_bin() else {
                eprintln!("no working java binary found: skipping Java implication consumer test");
                return None;
            };
            if !maven_available() {
                eprintln!("mvn unavailable: skipping Java implication consumer test");
                return None;
            }
            let root = repo_root();
            let mut mvn = Command::new("mvn");
            mvn.current_dir(root.join("implementations").join("java"))
                .args([
                    "-B",
                    "-ntp",
                    "-pl",
                    "provekit-lift-java-core",
                    "-am",
                    "-DskipTests",
                    "package",
                ]);
            let out =
                run_with_timeout(&mut mvn, Duration::from_secs(120)).expect("spawn mvn package");
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

fn stage_java_project(command: &[String]) -> PathBuf {
    let project = unique_dir("project");
    fs::write(
        project.join("App.java"),
        r#"public class App {
    static int caller(int x) {
        return callee(x);
    }

    static int callee(int x) {
        return x + 1;
    }
}
"#,
    )
    .expect("write App.java");

    let provekit = project.join(".provekit");
    fs::create_dir_all(provekit.join("lift").join("java-contracts")).unwrap();
    fs::create_dir_all(provekit.join("lift").join("java-implications")).unwrap();
    fs::write(
        provekit.join("config.toml"),
        r#"[[plugins]]
name = "java-contracts"
surface = "java-contracts"
emit = "ir-document"

[[plugins]]
name = "java-implications"
surface = "java-implications"
"#,
    )
    .expect("write config.toml");

    fs::write(
        provekit
            .join("lift")
            .join("java-contracts")
            .join("manifest.toml"),
        format!(
            "name = \"java-contracts\"\ncommand = {}\nworking_dir = \".\"\n",
            toml_string_array(command)
        ),
    )
    .expect("write java-contracts manifest");
    fs::write(
        provekit
            .join("lift")
            .join("java-implications")
            .join("manifest.toml"),
        format!(
            "name = \"java-implications\"\ncommand = {}\nworking_dir = \".\"\nmethod = \"provekit.plugin.lift_implications\"\nphase = \"consumer\"\n",
            toml_string_array(command)
        ),
    )
    .expect("write java-implications manifest");

    project
}

#[test]
fn java_implication_consumer_mints_bridge_from_manifest_rpc() {
    let Some(command) = java_lift_command() else {
        return;
    };
    let project = stage_java_project(&command);
    let out_dir = unique_dir("out");

    let output = Command::new(provekit_bin())
        .arg("mint")
        .arg("--project")
        .arg(&project)
        .arg("--out")
        .arg(&out_dir)
        .arg("--no-attest")
        .arg("--quiet")
        .arg("--json")
        .output()
        .expect("spawn provekit mint");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "java implication mint should dispatch producer lift then consumer lift_implications\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("no-contract-for-callee"),
        "matched callee call should not surface a lift-gap\nstdout:\n{stdout}"
    );

    let proof_files: Vec<PathBuf> = fs::read_dir(&out_dir)
        .expect("read out dir")
        .filter_map(|entry| {
            let path = entry.expect("out dir entry").path();
            (path.extension().and_then(|s| s.to_str()) == Some("proof")).then_some(path)
        })
        .collect();
    assert_eq!(
        proof_files.len(),
        1,
        "producer + Java implication consumer should conjoin into one proof"
    );

    let pool = provekit_verifier::load_all_proofs::run_with_files(
        Path::new("/no-such-project"),
        &proof_files,
    );
    assert!(
        pool.load_errors.is_empty(),
        "conjoined Java proof must load cleanly: {:?}",
        pool.load_errors
    );
    let saw_implication_bridge = pool.mementos.values().any(|env| {
        provekit_verifier::types::memento_kind(env).as_deref() == Some("bridge")
            && provekit_verifier::types::memento_body_field(env, "sourceSymbol")
                .and_then(|v| v.as_str())
                == Some("callee")
            && provekit_verifier::types::memento_body_field(env, "notes").and_then(|v| v.as_str())
                == Some("implication-lifted callsite bridge")
    });
    assert!(
        saw_implication_bridge,
        "Java consumer must contribute the implication-lifted callsite bridge"
    );

    let _ = fs::remove_dir_all(&project);
    let _ = fs::remove_dir_all(&out_dir);
}
