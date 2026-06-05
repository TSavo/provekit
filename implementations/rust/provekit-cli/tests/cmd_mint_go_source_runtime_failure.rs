// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

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
    let p = std::env::temp_dir().join(format!("provekit-go-source-runtime-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn go_source_lift_command() -> Option<Vec<String>> {
    static GO_SOURCE_LIFT_COMMAND: OnceLock<Option<Vec<String>>> = OnceLock::new();
    GO_SOURCE_LIFT_COMMAND
        .get_or_init(|| {
            if !go_available() {
                eprintln!("go not on PATH: skipping go-source runtime-failure mint test");
                return None;
            }
            let root = repo_root();
            let go_module = root
                .join("implementations")
                .join("go")
                .join("provekit-lift-go");
            let out = std::env::temp_dir().join(format!(
                "provekit-lift-go-source-runtime-{}",
                std::process::id()
            ));
            let built = Command::new("go")
                .current_dir(&go_module)
                .args([
                    "build",
                    "-o",
                    out.to_str().expect("utf8 output path"),
                    "./cmd/provekit-lift-go",
                ])
                .output()
                .expect("spawn go build");
            assert!(
                built.status.success(),
                "go build provekit-lift-go failed\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&built.stdout),
                String::from_utf8_lossy(&built.stderr)
            );
            Some(vec![out.display().to_string(), "--rpc".to_string()])
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

fn stage_go_source_project(command: &[String]) -> PathBuf {
    let project = unique_dir("project");
    fs::write(
        project.join("go.mod"),
        "module example.com/sample\n\ngo 1.22\n",
    )
    .expect("write go.mod");
    fs::write(
        project.join("panic.go"),
        r#"package sample

func Fail() {
panic("boom")
}
"#,
    )
    .expect("write panic.go");

    let provekit = project.join(".provekit");
    fs::create_dir_all(provekit.join("lift").join("go-source"))
        .expect("mkdir .provekit/lift/go-source");
    fs::write(
        provekit.join("config.toml"),
        r#"[[plugins]]
name = "go-source"
kind = "lift"
surface = "go-source"
"#,
    )
    .expect("write config.toml");
    fs::write(
        provekit
            .join("lift")
            .join("go-source")
            .join("manifest.toml"),
        format!(
            r#"name = "go-source"
version = "0.1.0-draft"
protocol_version = "pep/1.7.0"
kind = "lift"
command = {}
working_dir = "."

[capabilities]
authoring_surfaces = ["go-source"]
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
fn go_source_panic_mint_preserves_runtime_failure_locus_and_enumerates_callsite() {
    let Some(command) = go_source_lift_command() else {
        return;
    };
    let project = stage_go_source_project(&command);
    run_mint(&project);

    let pool = provekit_verifier::load_all_proofs::run(&project);
    assert!(
        pool.load_errors.is_empty(),
        "go-source proof must load cleanly: {:?}",
        pool.load_errors
    );

    let loci = contract_runtime_failure_loci(&pool);
    assert_eq!(
        loci,
        vec![json!({
            "effectKind": "concept:panic-freedom",
            "callee": RUNTIME_FAILURE_SITE_CONCEPT,
            "subkind": "explicit-panic",
            "argTerm": {
                "kind": "const",
                "sort": {"kind": "primitive", "name": "String"},
                "value": "boom"
            },
            "file": "panic.go",
            "line": 4,
            "col": 0
        })],
        "mint must preserve the go-source runtime-failure panicLoci row"
    );

    let callsites = provekit_verifier::enumerate_callsites::run(&pool);
    let runtime_failure_sites: Vec<_> = callsites
        .iter()
        .filter(|cs| cs.panic_site && cs.callee.as_deref() == Some(RUNTIME_FAILURE_SITE_CONCEPT))
        .collect();
    assert_eq!(
        runtime_failure_sites.len(),
        1,
        "verifier must surface exactly one Go runtime-failure panic site; got {callsites:#?}"
    );
    assert_eq!(runtime_failure_sites[0].file.as_deref(), Some("panic.go"));
    assert_eq!(runtime_failure_sites[0].line, Some(4));
    assert!(
        runtime_failure_sites[0].bridge_target_cid.is_empty(),
        "no bridge exists yet, so the surfaced callsite must remain undecidable"
    );

    let _ = fs::remove_dir_all(&project);
}
