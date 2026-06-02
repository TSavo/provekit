// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

fn typescript_env_enabled() -> bool {
    std::env::var("BCARGO_TYPESCRIPT_ENV").map_or(true, |value| value != "0")
}

fn skip_when_typescript_env_disabled(test_name: &str) -> bool {
    if typescript_env_enabled() {
        return false;
    }
    eprintln!("skipping: BCARGO_TYPESCRIPT_ENV=0 for {test_name}");
    true
}

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn tsx_cli() -> Option<PathBuf> {
    let path = repo_root()
        .join("node_modules")
        .join("tsx")
        .join("dist")
        .join("cli.mjs");
    path.exists().then_some(path)
}

fn unique_dir(suffix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("provekit-ts-source-runtime-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn stage_typescript_source_project() -> PathBuf {
    let tsx = tsx_cli().expect("tsx CLI must exist; run pnpm install at repo root");
    let project = unique_dir("project");
    fs::create_dir_all(project.join("src")).expect("mkdir src");
    fs::write(
        project.join("src").join("panic.ts"),
        r#"export function fail(reason: unknown): void {
  throw reason;
}

export function failObject(): void {
  throw { code: 500, message: "bad" };
}
"#,
    )
    .expect("write src/panic.ts");

    let provekit = project.join(".provekit");
    fs::create_dir_all(provekit.join("lift").join("typescript-source"))
        .expect("mkdir .provekit/lift/typescript-source");
    fs::write(
        provekit.join("config.toml"),
        r#"[[plugins]]
name = "typescript-source"
surface = "typescript-source"
layer = "verify"
"#,
    )
    .expect("write config.toml");

    let ts_source_bin = repo_root()
        .join("implementations")
        .join("typescript")
        .join("src")
        .join("lift")
        .join("typescript-source")
        .join("bin.ts");
    fs::write(
        provekit
            .join("lift")
            .join("typescript-source")
            .join("manifest.toml"),
        format!(
            "name = \"typescript-source\"\ncommand = [\"node\", \"{}\", \"{}\", \"--rpc\"]\nworking_dir = \".\"\n",
            tsx.display(),
            ts_source_bin.display()
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
fn typescript_source_throw_mint_preserves_runtime_failure_locus_and_enumerates_callsite() {
    if skip_when_typescript_env_disabled("TypeScript source runtime-failure mint test") {
        return;
    }
    if !node_available() {
        eprintln!("node not on PATH: skipping TypeScript source runtime-failure mint test");
        return;
    }
    if tsx_cli().is_none() {
        eprintln!("tsx not installed at repo root: skipping; run pnpm install at repo root");
        return;
    }

    let project = stage_typescript_source_project();
    run_mint(&project);

    let pool = provekit_verifier::load_all_proofs::run(&project);
    assert!(
        pool.load_errors.is_empty(),
        "typescript-source proof must load cleanly: {:?}",
        pool.load_errors
    );

    let mut loci = contract_runtime_failure_loci(&pool);
    loci.sort_by_key(|locus| locus.get("line").and_then(Json::as_i64).unwrap_or_default());
    assert_eq!(
        loci,
        vec![
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "explicit-throw",
                "argTerm": {
                    "kind": "var",
                    "name": "reason"
                },
                "file": "src/panic.ts",
                "line": 2,
                "col": 2
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "explicit-throw",
                "argTerm": {
                    "kind": "ctor",
                    "name": "ts:object-literal",
                    "args": [
                        {
                            "kind": "ctor",
                            "name": "ts:property",
                            "args": [
                                {
                                    "kind": "const",
                                    "sort": { "kind": "primitive", "name": "String" },
                                    "value": "code"
                                },
                                {
                                    "kind": "const",
                                    "sort": { "kind": "primitive", "name": "Int" },
                                    "value": 500
                                }
                            ]
                        },
                        {
                            "kind": "ctor",
                            "name": "ts:property",
                            "args": [
                                {
                                    "kind": "const",
                                    "sort": { "kind": "primitive", "name": "String" },
                                    "value": "message"
                                },
                                {
                                    "kind": "const",
                                    "sort": { "kind": "primitive", "name": "String" },
                                    "value": "bad"
                                }
                            ]
                        }
                    ]
                },
                "file": "src/panic.ts",
                "line": 6,
                "col": 2
            }),
        ],
        "mint must preserve the typescript-source runtime-failure panicLoci row"
    );

    let callsites = provekit_verifier::enumerate_callsites::run(&pool);
    let mut runtime_failure_sites: Vec<_> = callsites
        .iter()
        .filter(|cs| cs.panic_site && cs.callee.as_deref() == Some(RUNTIME_FAILURE_SITE_CONCEPT))
        .collect();
    runtime_failure_sites.sort_by_key(|cs| cs.line.unwrap_or_default());
    assert_eq!(
        runtime_failure_sites.len(),
        2,
        "verifier must surface exactly two TypeScript runtime-failure panic sites; got {callsites:#?}"
    );
    assert_eq!(
        runtime_failure_sites[0].file.as_deref(),
        Some("src/panic.ts")
    );
    assert_eq!(runtime_failure_sites[0].line, Some(2));
    assert_eq!(
        runtime_failure_sites[1].file.as_deref(),
        Some("src/panic.ts")
    );
    assert_eq!(runtime_failure_sites[1].line, Some(6));
    assert!(
        runtime_failure_sites
            .iter()
            .all(|site| site.bridge_target_cid.is_empty()),
        "no bridge exists yet, so the surfaced callsite must remain undecidable"
    );

    let _ = fs::remove_dir_all(&project);
}
