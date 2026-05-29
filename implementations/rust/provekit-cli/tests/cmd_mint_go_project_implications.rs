// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    let p = std::env::temp_dir().join(format!("provekit-go-implications-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn build_go_lift_source() -> PathBuf {
    let go_module = repo_root()
        .join("implementations")
        .join("go")
        .join("provekit-lift-go");
    let out = std::env::temp_dir().join(format!("provekit-lift-go-{}", std::process::id()));
    let built = Command::new("go")
        .current_dir(&go_module)
        .args([
            "build",
            "-o",
            out.to_str().unwrap(),
            "./cmd/provekit-lift-go",
        ])
        .output()
        .expect("spawn go build");
    assert!(
        built.status.success(),
        "go build provekit-lift-go failed\n  stdout: {}\n  stderr: {}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    out
}

fn stage_go_project(lift_bin: &Path) -> PathBuf {
    let project = unique_dir("project");
    fs::write(
        project.join("go.mod"),
        "module example.com/sample\n\ngo 1.22\n",
    )
    .expect("write go.mod");
    fs::write(
        project.join("sample.go"),
        r#"package sample

func Caller(x int) int {
	return Callee(x)
}

func Callee(x int) int {
	return x + 1
}
"#,
    )
    .expect("write sample.go");

    let provekit = project.join(".provekit");
    fs::create_dir_all(provekit.join("lift").join("go-contracts")).unwrap();
    fs::create_dir_all(provekit.join("lift").join("go-implications")).unwrap();
    fs::write(
        provekit.join("config.toml"),
        r#"[[plugins]]
name = "go-contracts"
surface = "go-contracts"
emit = "ir-document"

[[plugins]]
name = "go-implications"
surface = "go-implications"
"#,
    )
    .expect("write config.toml");

    fs::write(
        provekit
            .join("lift")
            .join("go-contracts")
            .join("manifest.toml"),
        format!(
            "name = \"go-contracts\"\ncommand = [\"{}\", \"--rpc\", \"--dialect=core\"]\nworking_dir = \".\"\n",
            lift_bin.display()
        ),
    )
    .expect("write go-contracts manifest");
    fs::write(
        provekit
            .join("lift")
            .join("go-implications")
            .join("manifest.toml"),
        format!(
            "name = \"go-implications\"\ncommand = [\"{}\", \"--rpc\"]\nworking_dir = \".\"\nmethod = \"provekit.plugin.lift_implications\"\nphase = \"consumer\"\n",
            lift_bin.display()
        ),
    )
    .expect("write go-implications manifest");

    project
}

#[test]
fn go_implication_consumer_mints_bridge_from_manifest_rpc() {
    if !go_available() {
        eprintln!("go not on PATH: skipping Go implication consumer test");
        return;
    }
    let lift_bin = build_go_lift_source();
    let project = stage_go_project(&lift_bin);
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
        "go implication mint should dispatch producer lift then consumer lift_implications\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("no-contract-for-callee"),
        "matched Callee call should not surface a lift-gap\nstdout:\n{stdout}"
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
        "producer + Go implication consumer should conjoin into one proof"
    );

    let pool = provekit_verifier::load_all_proofs::run_with_files(
        Path::new("/no-such-project"),
        &proof_files,
    );
    assert!(
        pool.load_errors.is_empty(),
        "conjoined Go proof must load cleanly: {:?}",
        pool.load_errors
    );
    assert!(
        pool.bridges_by_symbol.contains_key("Callee"),
        "Go implication consumer must mint a bridge indexed by sourceSymbol=Callee; indexed: {:?}",
        pool.bridges_by_symbol.keys().collect::<Vec<_>>()
    );
    let saw_implication_bridge = pool.mementos.values().any(|env| {
        provekit_verifier::types::memento_kind(env).as_deref() == Some("bridge")
            && provekit_verifier::types::memento_body_field(env, "sourceSymbol")
                .and_then(|v| v.as_str())
                == Some("Callee")
            && provekit_verifier::types::memento_body_field(env, "notes").and_then(|v| v.as_str())
                == Some("implication-lifted callsite bridge")
    });
    assert!(
        saw_implication_bridge,
        "Go consumer must contribute the implication-lifted callsite bridge, not only rely on producer auto-bridges"
    );

    let _ = fs::remove_dir_all(&project);
    let _ = fs::remove_dir_all(&out_dir);
}
