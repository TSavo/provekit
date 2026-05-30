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
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap_or_else(|_| panic!("mkdir {}", dst.display()));
    for entry in fs::read_dir(src).unwrap_or_else(|_| panic!("read {}", src.display())) {
        let entry = entry.expect("read dir entry");
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if entry.file_type().expect("entry file type").is_dir() {
            copy_dir_recursive(&src_path, &dst_path);
        } else {
            fs::copy(&src_path, &dst_path).unwrap_or_else(|_| {
                panic!("copy {} -> {}", src_path.display(), dst_path.display())
            });
        }
    }
}

fn write_direct_go_recognizer_manifest(project: &Path) {
    let manifest = project.join(".provekit/lift/go-bind/manifest.toml");
    fs::create_dir_all(manifest.parent().unwrap()).expect("mkdir go-bind manifest dir");
    let working_dir = repo_root()
        .join("implementations")
        .join("go")
        .join("provekit-lift-go");
    fs::write(
        manifest,
        format!(
            "name = \"go-bind-lift\"\ncommand = [\"go\", \"run\", \"./cmd/provekit-lift-go\", \"--rpc\"]\nworking_dir = \"{}\"\n",
            working_dir.display()
        ),
    )
    .expect("write go-bind manifest");
}

fn copy_go_recognizer_demo() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("project");
    copy_dir_recursive(
        &repo_root().join("examples").join("recognize-demo-go"),
        &project,
    );
    let _ = fs::remove_dir_all(project.join(".provekit/recognize"));
    write_direct_go_recognizer_manifest(&project);
    temp
}

#[test]
fn go_recognize_write_self_resolves_project_proofs_and_proves() {
    if !go_available() {
        eprintln!("skipping Go recognizer parity test: go toolchain unavailable");
        return;
    }

    let temp = copy_go_recognizer_demo();
    let project = temp.path().join("project");

    let recognize = Command::new(provekit_bin())
        .arg("recognize")
        .arg("--target")
        .arg("go")
        .arg("--project")
        .arg(&project)
        .arg("--source")
        .arg("pkg/ingest/ingest.go")
        .arg("--source")
        .arg("pkg/persist/persist.go")
        .arg("--write")
        .arg("--json")
        .output()
        .expect("spawn provekit recognize");

    let stdout = String::from_utf8_lossy(&recognize.stdout);
    let stderr = String::from_utf8_lossy(&recognize.stderr);
    assert!(
        recognize.status.success(),
        "recognize failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let receipt: serde_json::Value =
        serde_json::from_str(&stdout).expect("recognize JSON receipt parses");
    let tags = receipt["tags"]
        .as_array()
        .expect("recognize receipt has tags array");
    assert_eq!(
        tags.len(),
        2,
        "Go recognizer must resolve from project config, self-resolve the demo-local shim proof, and tag both user callsites without CLI proof paths\nreceipt:\n{receipt:#}"
    );
    let bridge_proof = receipt["bridge_proof"]
        .as_str()
        .expect("recognize --write must mint a bridge proof");
    assert!(
        Path::new(bridge_proof).is_file(),
        "recognize bridge proof path should exist: {bridge_proof}"
    );

    let prove = Command::new(provekit_bin())
        .arg("prove")
        .arg(&project)
        .arg("--json")
        .output()
        .expect("spawn provekit prove");
    let prove_stdout = String::from_utf8_lossy(&prove.stdout);
    let prove_stderr = String::from_utf8_lossy(&prove.stderr);
    assert!(
        prove.status.success(),
        "prove should consume the recognize bridge proof\nstdout:\n{prove_stdout}\nstderr:\n{prove_stderr}"
    );
    let report: serde_json::Value =
        serde_json::from_str(&prove_stdout).expect("prove JSON report parses");
    assert_eq!(report["totalCallsites"].as_u64(), Some(2), "{report:#}");
    assert_eq!(report["discharged"].as_u64(), Some(2), "{report:#}");
    assert_eq!(report["violations"].as_u64(), Some(0), "{report:#}");
}
