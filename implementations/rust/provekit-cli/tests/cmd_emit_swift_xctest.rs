// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

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

fn swift_available() -> bool {
    Command::new("swift")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn build_swift_xctest_emitter() -> PathBuf {
    let swift_root = repo_root().join("implementations").join("swift");
    let built = Command::new("swift")
        .current_dir(&swift_root)
        .args(["build", "--product", "provekit-emit-swift-xctest"])
        .output()
        .expect("spawn swift build provekit-emit-swift-xctest");
    assert!(
        built.status.success(),
        "swift build provekit-emit-swift-xctest failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    let emitter = swift_root
        .join(".build")
        .join("debug")
        .join("provekit-emit-swift-xctest");
    assert!(emitter.exists(), "missing emitter {}", emitter.display());
    emitter
}

fn install_emit_registration(project: &Path, emitter: &Path) {
    let provekit_dir = project.join(".provekit");
    fs::create_dir_all(&provekit_dir).expect("mkdir .provekit");
    fs::write(
        provekit_dir.join("config.toml"),
        "exam_manifest_cid = \"blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000\"\n\
         \n\
         [[plugins]]\n\
         name = \"swift-xctest\"\n\
         surface = \"swift-xctest\"\n\
         emit = \"xctest\"\n",
    )
    .expect("write project config");

    let manifest = project
        .join(".provekit")
        .join("emit")
        .join("swift-xctest")
        .join("manifest.toml");
    fs::create_dir_all(manifest.parent().unwrap()).expect("mkdir manifest");
    fs::write(
        manifest,
        format!(
            "name = \"swift-xctest\"\ncommand = [\"{}\", \"--rpc\"]\nworking_dir = \".\"\nprotocol_versions = [\"pep/1.7.0\"]\n",
            emitter
                .display()
                .to_string()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
        ),
    )
    .expect("write emit manifest");
}

fn write_swift_package(root: &Path) {
    fs::create_dir_all(root.join("Sources/ProvekitSwiftEmitCheck")).expect("mkdir Sources");
    fs::write(
        root.join("Package.swift"),
        r#"// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "ProvekitSwiftEmitCheck",
    products: [
        .library(name: "ProvekitSwiftEmitCheck", targets: ["ProvekitSwiftEmitCheck"]),
    ],
    targets: [
        .target(name: "ProvekitSwiftEmitCheck"),
        .testTarget(
            name: "ProvekitEmittedTests",
            dependencies: ["ProvekitSwiftEmitCheck"]
        ),
    ]
)
"#,
    )
    .expect("write Package.swift");
    fs::write(
        root.join("Sources/ProvekitSwiftEmitCheck/Stub.swift"),
        "public enum ProvekitSwiftEmitCheck { public static let ok = true }\n",
    )
    .expect("write source stub");
}

#[test]
fn emit_swift_xctest_dispatches_real_emitter_and_swift_parse_checks_output() {
    if !swift_available() {
        eprintln!("skipping: swift not on PATH");
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("project");
    let out_dir = temp.path().join("swift-package");
    fs::create_dir_all(&project).expect("mkdir project");
    fs::create_dir_all(&out_dir).expect("mkdir out");
    write_swift_package(&out_dir);

    let emitter = build_swift_xctest_emitter();
    install_emit_registration(&project, &emitter);

    let plan = project.join("plan.json");
    fs::write(
        &plan,
        serde_json::to_vec_pretty(&serde_json::json!({
            "contract_id": "concept:eq",
            "function": "identity",
            "params": ["a", "b"],
            "param_types": ["Int", "Int"],
            "predicates": [{
                "kind": "op",
                "name": "concept:eq",
                "args": [
                    {"kind": "const", "value": 2, "sort": {"kind": "primitive", "name": "Int"}},
                    {"kind": "const", "value": 2, "sort": {"kind": "primitive", "name": "Int"}}
                ]
            }]
        }))
        .expect("encode plan"),
    )
    .expect("write plan");

    let output = Command::new(provekit_bin())
        .arg("emit")
        .arg("--project")
        .arg(&project)
        .arg("--target")
        .arg("swift")
        .arg("--framework")
        .arg("xctest")
        .arg("--plan")
        .arg(&plan)
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--compile-check")
        .arg("--json")
        .output()
        .expect("spawn provekit emit swift");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit emit swift failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let receipt: Value = serde_json::from_str(&stdout).expect("emit stdout is JSON");
    assert_eq!(receipt["ok"], true, "receipt: {receipt}");
    assert_eq!(receipt["targetLanguage"], "swift", "receipt: {receipt}");
    assert_eq!(receipt["targetFramework"], "xctest", "receipt: {receipt}");
    assert_eq!(receipt["isComplete"], true, "receipt: {receipt}");
    assert!(
        receipt["emittedArtifactCid"]
            .as_str()
            .unwrap_or("")
            .starts_with("blake3-512:"),
        "receipt: {receipt}"
    );

    let emitted_path = out_dir.join("Tests/ProvekitEmittedTests/ProvekitEmittedTests.swift");
    let emitted = fs::read_to_string(&emitted_path)
        .unwrap_or_else(|_| panic!("read emitted {}", emitted_path.display()));
    assert!(emitted.contains("import XCTest"), "emitted:\n{emitted}");
    assert!(
        emitted.contains("XCTAssertEqual(2, 2)"),
        "emitted:\n{emitted}"
    );
}
