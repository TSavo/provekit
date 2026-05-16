// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use libprovekit::core::{
    address, execute_path, BindKit, ConformanceDeclaration, Dialect, HashMapInputCatalog, Input,
    Kit, KitRegistry, LiftKit, Path as CorePath, PathAlgebra, Term, Verb,
};

const BIND_NONCARRIER: ConformanceDeclaration = ConformanceDeclaration::NonCarrier {
    reason: "transforms Input::Term to NamedTerm DomainClaim; emits no target source",
};
const LIFT_NONCARRIER: ConformanceDeclaration = ConformanceDeclaration::NonCarrier {
    reason: "lifts source bytes to DomainClaim; no target source produced",
};
use provekit_ir_types::Sort;
use serde_json::{json, Value};

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

fn primitive_sort(name: &str) -> Sort {
    Sort::Primitive {
        name: name.to_string(),
    }
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
    printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"kind":"ir-document","sourceLanguage":"rust","ir":[{"kind":"bind-lift-entry","file":"src/lib.rs","fn_name":"id","fn_line":1,"concept_annotation":"identity","param_names":["x"],"param_types":["i64"],"return_type":"i64","term_shape":{"kind":"var","name":"x"},"term_shape_cid":"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","witnesses":[]}]}}'
  elif [[ "$line" == *'"method":"shutdown"'* ]]; then
    printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":null}'
    exit 0
  fi
done
"#,
    );
    script
}

fn bind_input_value() -> Value {
    json!({
        "kind": "ir-document",
        "sourceLanguage": "rust",
        "workspaceRoot": "/tmp/provekit-bind-path-test",
        "ir": [{
            "kind": "bind-lift-entry",
            "file": "src/lib.rs",
            "fn_name": "deposit",
            "fn_line": 14,
            "concept_annotation": "deposit-then-balance",
            "param_names": ["balance", "amount"],
            "param_types": ["i64", "i64"],
            "return_type": "i64",
            "term_shape": {
                "kind": "body",
                "stmts": [
                    {"kind": "let"},
                    {"kind": "bin", "op": "+"}
                ]
            },
            "term_shape_cid": "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
            "witnesses": [{
                "role": "post",
                "predicate_text": "out == balance + amount",
                "source_kind": "annotation"
            }]
        }]
    })
}

fn run_bind_cli(term_value: &Value) -> Vec<u8> {
    let mut child = Command::new(provekit_bin())
        .arg("bind")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bind");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(term_value.to_string().as_bytes())
        .expect("write term");
    let output = child.wait_with_output().expect("wait bind");
    assert!(
        output.status.success(),
        "bind cli failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[test]
fn bind_path_executor_matches_cmd_bind_named_term_document_bytes() {
    let term_value = bind_input_value();
    let input_term = Term::Const {
        value: term_value.clone(),
        sort: primitive_sort("LiftPluginResponse"),
    };
    let mut inputs = HashMapInputCatalog::default();
    let term_cid = address(&input_term);
    inputs.put(term_cid.clone(), Input::Term(input_term.clone()));
    let path = Input::Path(Box::new(CorePath {
        algebra: vec![PathAlgebra {
            name: "bind".to_string(),
            kit: "bind-default".to_string(),
            inputs: vec![term_cid],
            depends_on: vec![],
            verb: Verb::Transform,
        }],
    }));
    let mut registry = KitRegistry::default();
    registry.register("bind-default", BindKit::default(), BIND_NONCARRIER);

    let chain = execute_path(&path, &registry, &inputs).expect("bind path executes");
    let claim = chain.terminal_claim();
    let cli_bytes = run_bind_cli(&term_value);
    let cli_value: Value = serde_json::from_slice(&cli_bytes).expect("cmd_bind output parses");
    let cli_cid = libprovekit::canonical::json_cid(&cli_value).expect("cmd_bind output cids");

    assert_eq!(claim.to.as_str(), cli_cid);
    assert_eq!(claim.from, vec![address(&input_term)]);
    let payload = claim.payload.as_ref().expect("bind claim payload");
    let Term::Const { value, .. } = payload else {
        panic!("bind payload should be a named term document const");
    };
    assert_eq!(
        libprovekit::canonical::json_jcs(&value)
            .expect("payload canonicalizes")
            .as_bytes(),
        cli_bytes.as_slice()
    );
}

#[test]
fn lift_then_bind_path_carries_lift_output_and_claim_premise() {
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
            "identifyOnly": false
        }
    });
    let command = vec!["bash".to_string(), script.display().to_string()];
    let source = Input::Source {
        dialect: Dialect::Rust,
        bytes: serde_json::to_vec(&request).expect("encode lift request"),
    };
    let lift_kit = LiftKit::new(
        Dialect::Rust,
        "rust",
        command.clone(),
        Some(temp.path().to_path_buf()),
    );
    let lift_claim = lift_kit
        .transform(&source)
        .expect("lift transform succeeds");

    let mut inputs = HashMapInputCatalog::default();
    let source_cid = inputs.insert(source);
    let path = Input::Path(Box::new(CorePath {
        algebra: vec![
            PathAlgebra {
                name: "lift".to_string(),
                kit: "lift-rust".to_string(),
                inputs: vec![source_cid],
                depends_on: vec![],
                verb: Verb::Transform,
            },
            PathAlgebra {
                name: "bind".to_string(),
                kit: "bind-default".to_string(),
                inputs: vec![lift_claim.to.clone()],
                depends_on: vec!["lift".to_string()],
                verb: Verb::Transform,
            },
        ],
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
        LIFT_NONCARRIER,
    );
    registry.register("bind-default", BindKit::default(), BIND_NONCARRIER);

    let chain = execute_path(&path, &registry, &inputs).expect("lift bind path executes");
    let bind_claim = chain.terminal_claim();

    assert_eq!(bind_claim.from, vec![lift_claim.to.clone()]);
    assert_eq!(bind_claim.premises, vec![lift_claim.cid()]);
}

#[test]
fn bind_path_refuses_unregistered_bind_variant_with_composition_refusal_memento() {
    let term_value = bind_input_value();
    let input_term = Term::Const {
        value: term_value,
        sort: primitive_sort("LiftPluginResponse"),
    };
    let mut inputs = HashMapInputCatalog::default();
    let term_cid = address(&input_term);
    inputs.put(term_cid.clone(), Input::Term(input_term));
    let path = Input::Path(Box::new(CorePath {
        algebra: vec![PathAlgebra {
            name: "bind".to_string(),
            kit: "bind-missing".to_string(),
            inputs: vec![term_cid],
            depends_on: vec![],
            verb: Verb::Transform,
        }],
    }));
    let registry = KitRegistry::default();

    let error = execute_path(&path, &registry, &inputs).expect_err("missing bind kit refuses");
    let refusal = error
        .composition_refusal()
        .expect("missing bind kit emits refusal memento");
    assert_eq!(refusal.header.failure_kind, "memento-required-missing");
    assert_eq!(
        refusal
            .header
            .missing_memento_requirements
            .as_ref()
            .unwrap()[0]
            .role,
        Some("kit-registry".to_string())
    );
}
