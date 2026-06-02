// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

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

fn python_source_lift_src() -> PathBuf {
    repo_root()
        .join("implementations")
        .join("python")
        .join("provekit-lift-python-source")
        .join("src")
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn python_available() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn unique_dir(suffix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("provekit-py-source-runtime-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn write_executable(path: &Path, body: &str) {
    use std::io::Write as _;
    {
        let mut file = fs::File::create(path).expect("create script");
        file.write_all(body.as_bytes()).expect("write script");
        file.sync_all().expect("sync script");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).expect("stat script").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod script");
    }
}

fn build_python_lift_source() -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let pythonpath = python_source_lift_src()
        .into_os_string()
        .into_string()
        .expect("Python lift source root must be UTF-8");
    let quoted_pythonpath = shell_single_quote(&pythonpath);
    let script = std::env::temp_dir().join(format!(
        "provekit-lift-python-source-{}-{}.sh",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let body = format!(
        "#!/bin/sh\nPYTHON=${{PYTHON:-python3}}\n\
         PYTHONPATH={quoted_pythonpath}${{PYTHONPATH:+:$PYTHONPATH}}\n\
         export PYTHONPATH\n\
         exec \"$PYTHON\" -c \"from provekit_lift_python_source.rpc import run_rpc; run_rpc()\"\n"
    );
    write_executable(&script, &body);
    script
}

fn stage_python_source_project(lift_script: &Path) -> PathBuf {
    let project = unique_dir("project");
    fs::write(
        project.join("boom.py"),
        "def boom():\n    raise ValueError\n",
    )
    .expect("write boom.py");

    let provekit = project.join(".provekit");
    fs::create_dir_all(provekit.join("lift").join("python-source"))
        .expect("mkdir .provekit/lift/python-source");
    fs::write(
        provekit.join("config.toml"),
        r#"[[plugins]]
name = "python-source"
kind = "lift"
surface = "python-source"
"#,
    )
    .expect("write config.toml");
    fs::write(
        provekit
            .join("lift")
            .join("python-source")
            .join("manifest.toml"),
        format!(
            r#"name = "python-source"
version = "0.1.0-draft"
protocol_version = "provekit-lift/1"
kind = "lift"
command = ["{}", "--rpc"]
working_dir = "."

[capabilities]
authoring_surfaces = ["python-source"]
ir_version = "v1.1.0"
emits_signed_mementos = false
"#,
            lift_script.display()
        ),
    )
    .expect("write manifest.toml");

    project
}

fn stage_python_access_project(lift_script: &Path) -> PathBuf {
    let project = unique_dir("access-project");
    fs::write(
        project.join("access.py"),
        "def use(obj, xs, key):\n    attr = obj.name\n    item = xs[key]\n    return attr\n",
    )
    .expect("write access.py");

    let provekit = project.join(".provekit");
    fs::create_dir_all(provekit.join("lift").join("python-source"))
        .expect("mkdir .provekit/lift/python-source");
    fs::write(
        provekit.join("config.toml"),
        r#"[[plugins]]
name = "python-source"
kind = "lift"
surface = "python-source"
"#,
    )
    .expect("write config.toml");
    fs::write(
        provekit
            .join("lift")
            .join("python-source")
            .join("manifest.toml"),
        format!(
            r#"name = "python-source"
version = "0.1.0-draft"
protocol_version = "provekit-lift/1"
kind = "lift"
command = ["{}", "--rpc"]
working_dir = "."

[capabilities]
authoring_surfaces = ["python-source"]
ir_version = "v1.1.0"
emits_signed_mementos = false
"#,
            lift_script.display()
        ),
    )
    .expect("write manifest.toml");

    project
}

fn stage_python_store_project(lift_script: &Path) -> PathBuf {
    let project = unique_dir("store-project");
    fs::write(
        project.join("store.py"),
        "def write(obj, xs, ys, i, key, value):\n    obj.name = value\n    xs[key] = value\n    obj.inner.name = value\n    xs[ys[i]] = value\n    return value\n",
    )
    .expect("write store.py");

    let provekit = project.join(".provekit");
    fs::create_dir_all(provekit.join("lift").join("python-source"))
        .expect("mkdir .provekit/lift/python-source");
    fs::write(
        provekit.join("config.toml"),
        r#"[[plugins]]
name = "python-source"
kind = "lift"
surface = "python-source"
"#,
    )
    .expect("write config.toml");
    fs::write(
        provekit
            .join("lift")
            .join("python-source")
            .join("manifest.toml"),
        format!(
            r#"name = "python-source"
version = "0.1.0-draft"
protocol_version = "provekit-lift/1"
kind = "lift"
command = ["{}", "--rpc"]
working_dir = "."

[capabilities]
authoring_surfaces = ["python-source"]
ir_version = "v1.1.0"
emits_signed_mementos = false
"#,
            lift_script.display()
        ),
    )
    .expect("write manifest.toml");

    project
}

fn stage_python_augassign_project(lift_script: &Path) -> PathBuf {
    let project = unique_dir("augassign-project");
    fs::write(
        project.join("augassign.py"),
        "def bump(obj, xs, ys, i, key, value):\n    obj.name += value\n    xs[key] += value\n    obj.inner.name += value\n    xs[ys[i]] += value\n    return value\n",
    )
    .expect("write augassign.py");

    let provekit = project.join(".provekit");
    fs::create_dir_all(provekit.join("lift").join("python-source"))
        .expect("mkdir .provekit/lift/python-source");
    fs::write(
        provekit.join("config.toml"),
        r#"[[plugins]]
name = "python-source"
kind = "lift"
surface = "python-source"
"#,
    )
    .expect("write config.toml");
    fs::write(
        provekit
            .join("lift")
            .join("python-source")
            .join("manifest.toml"),
        format!(
            r#"name = "python-source"
version = "0.1.0-draft"
protocol_version = "provekit-lift/1"
kind = "lift"
command = ["{}", "--rpc"]
working_dir = "."

[capabilities]
authoring_surfaces = ["python-source"]
ir_version = "v1.1.0"
emits_signed_mementos = false
"#,
            lift_script.display()
        ),
    )
    .expect("write manifest.toml");

    project
}

fn stage_python_annassign_project(lift_script: &Path) -> PathBuf {
    let project = unique_dir("annassign-project");
    fs::write(
        project.join("annassign.py"),
        "def annotate(obj, xs, ys, i, key, value, make):\n    obj.name: int\n    xs[key]: int\n    obj.name: int = value\n    xs[key]: int = value\n    obj.inner.name: int\n    xs[ys[i]]: int\n    obj.inner.name: int = value\n    xs[ys[i]]: int = value\n    make().name: int\n    return value\n",
    )
    .expect("write annassign.py");

    let provekit = project.join(".provekit");
    fs::create_dir_all(provekit.join("lift").join("python-source"))
        .expect("mkdir .provekit/lift/python-source");
    fs::write(
        provekit.join("config.toml"),
        r#"[[plugins]]
name = "python-source"
kind = "lift"
surface = "python-source"
"#,
    )
    .expect("write config.toml");
    fs::write(
        provekit
            .join("lift")
            .join("python-source")
            .join("manifest.toml"),
        format!(
            r#"name = "python-source"
version = "0.1.0-draft"
protocol_version = "provekit-lift/1"
kind = "lift"
command = ["{}", "--rpc"]
working_dir = "."

[capabilities]
authoring_surfaces = ["python-source"]
ir_version = "v1.1.0"
emits_signed_mementos = false
"#,
            lift_script.display()
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
fn python_source_raise_mint_preserves_runtime_failure_locus_and_enumerates_callsite() {
    if !python_available() {
        eprintln!("python3 not on PATH: skipping python-source runtime-failure mint test");
        return;
    }
    let lift_script = build_python_lift_source();
    let project = stage_python_source_project(&lift_script);
    run_mint(&project);

    let pool = provekit_verifier::load_all_proofs::run(&project);
    assert!(
        pool.load_errors.is_empty(),
        "python-source proof must load cleanly: {:?}",
        pool.load_errors
    );

    let loci = contract_runtime_failure_loci(&pool);
    assert_eq!(
        loci,
        vec![json!({
            "effectKind": "concept:panic-freedom",
            "callee": RUNTIME_FAILURE_SITE_CONCEPT,
            "subkind": "explicit-raise",
            "exceptionClass": "ValueError",
            "argTerm": {"kind": "var", "name": "ValueError"},
            "file": "boom.py",
            "line": 2,
            "col": 4
        })],
        "mint must preserve the python-source runtime-failure panicLoci row"
    );

    let callsites = provekit_verifier::enumerate_callsites::run(&pool);
    let runtime_failure_sites: Vec<_> = callsites
        .iter()
        .filter(|cs| cs.panic_site && cs.callee.as_deref() == Some(RUNTIME_FAILURE_SITE_CONCEPT))
        .collect();
    assert_eq!(
        runtime_failure_sites.len(),
        1,
        "verifier must surface exactly one substrate runtime-failure panic site; got {callsites:#?}"
    );
    assert_eq!(runtime_failure_sites[0].file.as_deref(), Some("boom.py"));
    assert_eq!(runtime_failure_sites[0].line, Some(2));
    assert!(
        runtime_failure_sites[0].bridge_target_cid.is_empty(),
        "no bridge exists yet, so the surfaced callsite must remain undecidable"
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn python_source_access_mint_preserves_runtime_failure_loci_and_enumerates_callsites() {
    if !python_available() {
        eprintln!("python3 not on PATH: skipping python-source access runtime-failure mint test");
        return;
    }
    let lift_script = build_python_lift_source();
    let project = stage_python_access_project(&lift_script);
    run_mint(&project);

    let pool = provekit_verifier::load_all_proofs::run(&project);
    assert!(
        pool.load_errors.is_empty(),
        "python-source access proof must load cleanly: {:?}",
        pool.load_errors
    );

    let loci = contract_runtime_failure_loci(&pool);
    assert_eq!(
        loci,
        vec![
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "attribute-access",
                "exceptionClass": "AttributeError",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:attribute",
                    "args": [
                        {"kind": "var", "name": "obj"},
                        {"kind": "const", "value": "name", "sort": {"kind": "primitive", "name": "String"}}
                    ]
                },
                "file": "access.py",
                "line": 2,
                "col": 11
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "subscript-access",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:subscript",
                    "args": [
                        {"kind": "var", "name": "xs"},
                        {"kind": "var", "name": "key"}
                    ]
                },
                "file": "access.py",
                "line": 3,
                "col": 11
            }),
        ],
        "mint must preserve python-source access runtime-failure panicLoci rows"
    );

    let callsites = provekit_verifier::enumerate_callsites::run(&pool);
    let runtime_failure_sites: Vec<_> = callsites
        .iter()
        .filter(|cs| cs.panic_site && cs.callee.as_deref() == Some(RUNTIME_FAILURE_SITE_CONCEPT))
        .collect();
    assert_eq!(
        runtime_failure_sites.len(),
        2,
        "verifier must surface exactly two substrate runtime-failure panic sites; got {callsites:#?}"
    );
    assert_eq!(runtime_failure_sites[0].file.as_deref(), Some("access.py"));
    assert_eq!(runtime_failure_sites[0].line, Some(2));
    assert_eq!(runtime_failure_sites[1].file.as_deref(), Some("access.py"));
    assert_eq!(runtime_failure_sites[1].line, Some(3));
    assert!(
        runtime_failure_sites
            .iter()
            .all(|cs| cs.bridge_target_cid.is_empty()),
        "no bridges exist yet, so surfaced access callsites must remain undecidable"
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn python_source_store_mint_preserves_runtime_failure_loci_and_enumerates_callsites() {
    if !python_available() {
        eprintln!("python3 not on PATH: skipping python-source store runtime-failure mint test");
        return;
    }
    let lift_script = build_python_lift_source();
    let project = stage_python_store_project(&lift_script);
    run_mint(&project);

    let pool = provekit_verifier::load_all_proofs::run(&project);
    assert!(
        pool.load_errors.is_empty(),
        "python-source store proof must load cleanly: {:?}",
        pool.load_errors
    );

    let loci = contract_runtime_failure_loci(&pool);
    assert_eq!(
        loci,
        vec![
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "attribute-write",
                "exceptionClass": "AttributeError",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:attribute",
                    "args": [
                        {"kind": "var", "name": "obj"},
                        {"kind": "const", "value": "name", "sort": {"kind": "primitive", "name": "String"}}
                    ]
                },
                "file": "store.py",
                "line": 2,
                "col": 4
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "subscript-write",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:subscript",
                    "args": [
                        {"kind": "var", "name": "xs"},
                        {"kind": "var", "name": "key"}
                    ]
                },
                "file": "store.py",
                "line": 3,
                "col": 4
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "attribute-access",
                "exceptionClass": "AttributeError",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:attribute",
                    "args": [
                        {"kind": "var", "name": "obj"},
                        {"kind": "const", "value": "inner", "sort": {"kind": "primitive", "name": "String"}}
                    ]
                },
                "file": "store.py",
                "line": 4,
                "col": 4
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "attribute-write",
                "exceptionClass": "AttributeError",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:attribute",
                    "args": [
                        {
                            "kind": "ctor",
                            "name": "python:attribute",
                            "args": [
                                {"kind": "var", "name": "obj"},
                                {"kind": "const", "value": "inner", "sort": {"kind": "primitive", "name": "String"}}
                            ]
                        },
                        {"kind": "const", "value": "name", "sort": {"kind": "primitive", "name": "String"}}
                    ]
                },
                "file": "store.py",
                "line": 4,
                "col": 4
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "subscript-access",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:subscript",
                    "args": [
                        {"kind": "var", "name": "ys"},
                        {"kind": "var", "name": "i"}
                    ]
                },
                "file": "store.py",
                "line": 5,
                "col": 7
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "subscript-write",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:subscript",
                    "args": [
                        {"kind": "var", "name": "xs"},
                        {
                            "kind": "ctor",
                            "name": "python:subscript",
                            "args": [
                                {"kind": "var", "name": "ys"},
                                {"kind": "var", "name": "i"}
                            ]
                        }
                    ]
                },
                "file": "store.py",
                "line": 5,
                "col": 4
            }),
        ],
        "mint must preserve python-source store runtime-failure panicLoci rows"
    );

    let callsites = provekit_verifier::enumerate_callsites::run(&pool);
    let runtime_failure_sites: Vec<_> = callsites
        .iter()
        .filter(|cs| cs.panic_site && cs.callee.as_deref() == Some(RUNTIME_FAILURE_SITE_CONCEPT))
        .collect();
    assert_eq!(
        runtime_failure_sites.len(),
        6,
        "verifier must surface exactly six substrate runtime-failure panic sites; got {callsites:#?}"
    );
    assert!(
        runtime_failure_sites
            .iter()
            .all(|cs| cs.file.as_deref() == Some("store.py")),
        "all surfaced Store callsites must preserve store.py provenance: {runtime_failure_sites:#?}"
    );
    assert_eq!(
        runtime_failure_sites
            .iter()
            .map(|cs| (cs.line, cs.bridge_target_cid.is_empty()))
            .collect::<Vec<_>>(),
        vec![
            (Some(2), true),
            (Some(3), true),
            (Some(4), true),
            (Some(4), true),
            (Some(5), true),
            (Some(5), true),
        ],
        "no bridges exist yet, so surfaced store callsites must remain undecidable"
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn python_source_augassign_mint_preserves_runtime_failure_loci_and_enumerates_callsites() {
    if !python_available() {
        eprintln!(
            "python3 not on PATH: skipping python-source AugAssign runtime-failure mint test"
        );
        return;
    }
    let lift_script = build_python_lift_source();
    let project = stage_python_augassign_project(&lift_script);
    run_mint(&project);

    let pool = provekit_verifier::load_all_proofs::run(&project);
    assert!(
        pool.load_errors.is_empty(),
        "python-source AugAssign proof must load cleanly: {:?}",
        pool.load_errors
    );

    let loci = contract_runtime_failure_loci(&pool);
    assert_eq!(
        loci,
        vec![
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "attribute-access",
                "exceptionClass": "AttributeError",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:attribute",
                    "args": [
                        {"kind": "var", "name": "obj"},
                        {"kind": "const", "value": "name", "sort": {"kind": "primitive", "name": "String"}}
                    ]
                },
                "file": "augassign.py",
                "line": 2,
                "col": 4
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "attribute-write",
                "exceptionClass": "AttributeError",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:attribute",
                    "args": [
                        {"kind": "var", "name": "obj"},
                        {"kind": "const", "value": "name", "sort": {"kind": "primitive", "name": "String"}}
                    ]
                },
                "file": "augassign.py",
                "line": 2,
                "col": 4
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "subscript-access",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:subscript",
                    "args": [
                        {"kind": "var", "name": "xs"},
                        {"kind": "var", "name": "key"}
                    ]
                },
                "file": "augassign.py",
                "line": 3,
                "col": 4
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "subscript-write",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:subscript",
                    "args": [
                        {"kind": "var", "name": "xs"},
                        {"kind": "var", "name": "key"}
                    ]
                },
                "file": "augassign.py",
                "line": 3,
                "col": 4
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "attribute-access",
                "exceptionClass": "AttributeError",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:attribute",
                    "args": [
                        {"kind": "var", "name": "obj"},
                        {"kind": "const", "value": "inner", "sort": {"kind": "primitive", "name": "String"}}
                    ]
                },
                "file": "augassign.py",
                "line": 4,
                "col": 4
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "attribute-access",
                "exceptionClass": "AttributeError",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:attribute",
                    "args": [
                        {
                            "kind": "ctor",
                            "name": "python:attribute",
                            "args": [
                                {"kind": "var", "name": "obj"},
                                {"kind": "const", "value": "inner", "sort": {"kind": "primitive", "name": "String"}}
                            ]
                        },
                        {"kind": "const", "value": "name", "sort": {"kind": "primitive", "name": "String"}}
                    ]
                },
                "file": "augassign.py",
                "line": 4,
                "col": 4
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "attribute-write",
                "exceptionClass": "AttributeError",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:attribute",
                    "args": [
                        {
                            "kind": "ctor",
                            "name": "python:attribute",
                            "args": [
                                {"kind": "var", "name": "obj"},
                                {"kind": "const", "value": "inner", "sort": {"kind": "primitive", "name": "String"}}
                            ]
                        },
                        {"kind": "const", "value": "name", "sort": {"kind": "primitive", "name": "String"}}
                    ]
                },
                "file": "augassign.py",
                "line": 4,
                "col": 4
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "subscript-access",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:subscript",
                    "args": [
                        {"kind": "var", "name": "ys"},
                        {"kind": "var", "name": "i"}
                    ]
                },
                "file": "augassign.py",
                "line": 5,
                "col": 7
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "subscript-access",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:subscript",
                    "args": [
                        {"kind": "var", "name": "xs"},
                        {
                            "kind": "ctor",
                            "name": "python:subscript",
                            "args": [
                                {"kind": "var", "name": "ys"},
                                {"kind": "var", "name": "i"}
                            ]
                        }
                    ]
                },
                "file": "augassign.py",
                "line": 5,
                "col": 4
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "subscript-write",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:subscript",
                    "args": [
                        {"kind": "var", "name": "xs"},
                        {
                            "kind": "ctor",
                            "name": "python:subscript",
                            "args": [
                                {"kind": "var", "name": "ys"},
                                {"kind": "var", "name": "i"}
                            ]
                        }
                    ]
                },
                "file": "augassign.py",
                "line": 5,
                "col": 4
            }),
        ],
        "mint must preserve python-source AugAssign runtime-failure panicLoci rows"
    );

    let callsites = provekit_verifier::enumerate_callsites::run(&pool);
    let runtime_failure_sites: Vec<_> = callsites
        .iter()
        .filter(|cs| cs.panic_site && cs.callee.as_deref() == Some(RUNTIME_FAILURE_SITE_CONCEPT))
        .collect();
    // The proof keeps all ten panicLoci rows above. CallSite enumeration
    // currently deduplicates access/write rows that share callee, file, line,
    // and argTerm because CallSite does not carry panicLoci subkind.
    assert_eq!(
        runtime_failure_sites.len(),
        6,
        "verifier currently surfaces six unique AugAssign runtime-failure obligations; got {callsites:#?}"
    );
    assert!(
        runtime_failure_sites
            .iter()
            .all(|cs| cs.file.as_deref() == Some("augassign.py")),
        "all surfaced AugAssign callsites must preserve augassign.py provenance: {runtime_failure_sites:#?}"
    );
    assert_eq!(
        runtime_failure_sites
            .iter()
            .map(|cs| (cs.line, cs.bridge_target_cid.is_empty()))
            .collect::<Vec<_>>(),
        vec![
            (Some(2), true),
            (Some(3), true),
            (Some(4), true),
            (Some(4), true),
            (Some(5), true),
            (Some(5), true),
        ],
        "no bridges exist yet, so surfaced AugAssign callsites must remain undecidable"
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn python_source_annassign_mint_preserves_runtime_failure_loci_and_enumerates_callsites() {
    if !python_available() {
        eprintln!(
            "python3 not on PATH: skipping python-source AnnAssign runtime-failure mint test"
        );
        return;
    }
    let lift_script = build_python_lift_source();
    let project = stage_python_annassign_project(&lift_script);
    run_mint(&project);

    let pool = provekit_verifier::load_all_proofs::run(&project);
    assert!(
        pool.load_errors.is_empty(),
        "python-source AnnAssign proof must load cleanly: {:?}",
        pool.load_errors
    );

    let loci = contract_runtime_failure_loci(&pool);
    assert_eq!(
        loci,
        vec![
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "attribute-write",
                "exceptionClass": "AttributeError",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:attribute",
                    "args": [
                        {"kind": "var", "name": "obj"},
                        {"kind": "const", "value": "name", "sort": {"kind": "primitive", "name": "String"}}
                    ]
                },
                "file": "annassign.py",
                "line": 4,
                "col": 4
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "subscript-write",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:subscript",
                    "args": [
                        {"kind": "var", "name": "xs"},
                        {"kind": "var", "name": "key"}
                    ]
                },
                "file": "annassign.py",
                "line": 5,
                "col": 4
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "attribute-access",
                "exceptionClass": "AttributeError",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:attribute",
                    "args": [
                        {"kind": "var", "name": "obj"},
                        {"kind": "const", "value": "inner", "sort": {"kind": "primitive", "name": "String"}}
                    ]
                },
                "file": "annassign.py",
                "line": 6,
                "col": 4
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "subscript-access",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:subscript",
                    "args": [
                        {"kind": "var", "name": "ys"},
                        {"kind": "var", "name": "i"}
                    ]
                },
                "file": "annassign.py",
                "line": 7,
                "col": 7
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "attribute-access",
                "exceptionClass": "AttributeError",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:attribute",
                    "args": [
                        {"kind": "var", "name": "obj"},
                        {"kind": "const", "value": "inner", "sort": {"kind": "primitive", "name": "String"}}
                    ]
                },
                "file": "annassign.py",
                "line": 8,
                "col": 4
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "attribute-write",
                "exceptionClass": "AttributeError",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:attribute",
                    "args": [
                        {
                            "kind": "ctor",
                            "name": "python:attribute",
                            "args": [
                                {"kind": "var", "name": "obj"},
                                {"kind": "const", "value": "inner", "sort": {"kind": "primitive", "name": "String"}}
                            ]
                        },
                        {"kind": "const", "value": "name", "sort": {"kind": "primitive", "name": "String"}}
                    ]
                },
                "file": "annassign.py",
                "line": 8,
                "col": 4
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "subscript-access",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:subscript",
                    "args": [
                        {"kind": "var", "name": "ys"},
                        {"kind": "var", "name": "i"}
                    ]
                },
                "file": "annassign.py",
                "line": 9,
                "col": 7
            }),
            json!({
                "effectKind": "concept:panic-freedom",
                "callee": RUNTIME_FAILURE_SITE_CONCEPT,
                "subkind": "subscript-write",
                "argTerm": {
                    "kind": "ctor",
                    "name": "python:subscript",
                    "args": [
                        {"kind": "var", "name": "xs"},
                        {
                            "kind": "ctor",
                            "name": "python:subscript",
                            "args": [
                                {"kind": "var", "name": "ys"},
                                {"kind": "var", "name": "i"}
                            ]
                        }
                    ]
                },
                "file": "annassign.py",
                "line": 9,
                "col": 4
            }),
        ],
        "mint must preserve python-source AnnAssign runtime-failure panicLoci rows"
    );

    let callsites = provekit_verifier::enumerate_callsites::run(&pool);
    let runtime_failure_sites: Vec<_> = callsites
        .iter()
        .filter(|cs| cs.panic_site && cs.callee.as_deref() == Some(RUNTIME_FAILURE_SITE_CONCEPT))
        .collect();
    assert_eq!(
        runtime_failure_sites.len(),
        8,
        "verifier must surface exactly eight AnnAssign runtime-failure obligations; got {callsites:#?}"
    );
    assert!(
        runtime_failure_sites
            .iter()
            .all(|cs| cs.file.as_deref() == Some("annassign.py")),
        "all surfaced AnnAssign callsites must preserve annassign.py provenance: {runtime_failure_sites:#?}"
    );
    assert_eq!(
        runtime_failure_sites
            .iter()
            .map(|cs| (cs.line, cs.bridge_target_cid.is_empty()))
            .collect::<Vec<_>>(),
        vec![
            (Some(4), true),
            (Some(5), true),
            (Some(6), true),
            (Some(7), true),
            (Some(8), true),
            (Some(8), true),
            (Some(9), true),
            (Some(9), true),
        ],
        "no bridges exist yet, so surfaced AnnAssign callsites must remain undecidable"
    );

    let _ = fs::remove_dir_all(&project);
}
