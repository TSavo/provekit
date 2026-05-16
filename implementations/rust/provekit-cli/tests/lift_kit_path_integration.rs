// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use libprovekit::core::{
    address, execute_path, Dialect, HashMapInputCatalog, Input, KitRegistry, LiftKit,
    LiftPluginKit, Path as CorePath, PathAlgebra, Term,
};
use provekit_ir_types::Sort;
use serde_json::json;

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).expect("write script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod script");
    }
}

fn fake_lifter(root: &Path) -> PathBuf {
    let script = root.join("fake-rust-lifter.sh");
    write_executable(
        &script,
        r#"#!/usr/bin/env bash
set -euo pipefail
while IFS= read -r line; do
  if [[ "$line" == *'"method":"initialize"'* ]]; then
    printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"name":"fake-rust-lifter","protocol_version":"pep/1.7.0","capabilities":{"surfaces":["rust"]}}}'
  elif [[ "$line" == *'"method":"lift"'* ]]; then
    printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"kind":"ir-document","sourceLanguage":"rust","ir":[{"kind":"bind-lift-entry","file":"src/lib.rs","fn_name":"id","fn_line":1,"param_names":["x"],"param_types":["i64"],"return_type":"i64","term_shape":{"kind":"var","name":"x"},"term_shape_cid":"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","witnesses":[]}],"diagnostics":[]}}'
  elif [[ "$line" == *'"method":"shutdown"'* ]]; then
    printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":null}'
    exit 0
  fi
done
"#,
    );
    script
}

#[test]
fn lift_rust_path_executor_matches_existing_cmd_lift_transport_term_cid() {
    let temp = tempfile::tempdir().expect("tempdir");
    let script = fake_lifter(temp.path());
    let workspace_root = temp
        .path()
        .canonicalize()
        .unwrap_or_else(|_| temp.path().to_path_buf());
    let request = json!({
        "surface": "rust",
        "workspace_root": workspace_root,
        "config_path": ".provekit/config.toml",
        "source_paths": ["."],
        "options": {
            "layer": "all",
            "identifyOnly": false,
        }
    });
    let command = vec!["bash".to_string(), script.display().to_string()];
    let existing = LiftPluginKit::new("rust", command.clone(), Some(temp.path().to_path_buf()))
        .parse_session(&Input::Spec(request.clone()))
        .expect("existing lift plugin transport succeeds");

    let source = Input::Source {
        dialect: Dialect::Rust,
        bytes: serde_json::to_vec(&request).expect("encode lift request"),
    };
    let mut inputs = HashMapInputCatalog::default();
    let source_cid = inputs.insert(source);
    let path = Input::Path(Box::new(CorePath {
        algebra: vec![PathAlgebra {
            name: "lift".to_string(),
            kit: "lift-rust".to_string(),
            inputs: vec![source_cid],
            depends_on: vec![],
        }],
    }));
    let mut registry = KitRegistry::default();
    registry.register(
        "lift-rust",
        LiftKit::new(
            Dialect::Rust,
            "rust",
            command,
            Some(temp.path().to_path_buf()),
        ),
    );

    let claim = execute_path(&path, &registry, &inputs).expect("lift path executes");
    assert_eq!(claim.to, existing.claim.to);
    assert_eq!(claim.artifacts, existing.claim.artifacts);
    assert_eq!(
        claim.payload.as_ref(),
        Some(&Term::Const {
            value: json!({
                "kind": "ir-document",
                "sourceLanguage": "rust",
                "ir": [{
                    "kind": "bind-lift-entry",
                    "file": "src/lib.rs",
                    "fn_name": "id",
                    "fn_line": 1,
                    "param_names": ["x"],
                    "param_types": ["i64"],
                    "return_type": "i64",
                    "term_shape": {"kind": "var", "name": "x"},
                    "term_shape_cid": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "witnesses": []
                }],
                "diagnostics": []
            }),
            sort: Sort::Primitive {
                name: "LiftPluginResponse".to_string(),
            },
        })
    );
    assert_eq!(
        claim.to,
        address(claim.payload.as_ref().expect("payload term"))
    );
}

#[test]
fn lift_cli_refuses_unregistered_dialect_with_composition_refusal_memento() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("project");
    fs::create_dir_all(project.join(".provekit")).expect("create project config dir");
    fs::write(
        project.join(".provekit/config.toml"),
        "[authoring.lift]\nsurface = \"neverlang\"\n",
    )
    .expect("write config");

    let output = Command::new(provekit_bin())
        .arg("lift")
        .arg(&project)
        .arg("--json")
        .arg("--quiet")
        .output()
        .expect("spawn provekit lift");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "unknown lift dialect must fail\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("composition-refusal")
            && stderr.contains("memento-required-missing")
            && stderr.contains("kit-registry"),
        "stderr should carry the registry refusal memento\nstderr:\n{stderr}"
    );
}
