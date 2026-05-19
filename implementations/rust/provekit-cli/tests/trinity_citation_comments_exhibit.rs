// SPDX-License-Identifier: Apache-2.0
//
// Trinity citation-comments exhibit for #1068.
//
// Architect ruling locked 2026-05-18: Path B, option 2b. This is a dedicated
// slow-lane job because the exhibit installs and invokes real Python, Java,
// and Rust plugin subprocesses. No fixture stubs. No ignored tests.
//
// v1.0.0 scope: one first-class fixture,
// `menagerie/trinity-exhibit-fixtures/01-arithmetic-add/`.
//
// The Python source lifter currently emits function entries, not top-level
// module harness statements. The source identity check therefore compares the
// normalized lifted function body for `compute_sum`; executable behavior is
// checked separately by importing the original and final Python modules and
// invoking `compute_sum(3, 4)`.

#![cfg(feature = "slow-tests")]

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use libprovekit::core::{
    assert_concept_tier, concept_bind_result_cid, execute_path, BindKit, ConformanceDeclaration,
    Dialect, DomainClaim, HashMapCatalog, HashMapInputCatalog, Input, KitRegistry, LiftKit,
    LowerKit, Path as CorePath, PathAlgebra, Term, Verb,
};
use provekit_cli::kit_dispatch::DispatchRealizeTransport;
use serde_json::{json, Value};

const FIXTURE_DIR: &str = "menagerie/trinity-exhibit-fixtures/01-arithmetic-add";
const FIXTURE_FUNCTION: &str = "compute_sum";
const EXPECTED_CONCEPTS: [&str; 3] = ["concept:add", "concept:mul", "concept:sub"];

#[derive(Debug, Clone)]
struct LegResult {
    lift_claim_cid: libprovekit::core::Cid,
    bind_claim_cid: libprovekit::core::Cid,
    bind_payload_cid: libprovekit::core::Cid,
    lower_claim_cid: libprovekit::core::Cid,
    concept_cids: BTreeMap<String, libprovekit::core::Cid>,
    source: String,
}

#[derive(Debug, Clone)]
struct ChainResult {
    python_to_java: LegResult,
    java_to_rust: LegResult,
    rust_to_python: LegResult,
    final_python: String,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root from provekit-cli manifest dir")
}

fn fixture_source() -> String {
    fs::read_to_string(repo_root().join(FIXTURE_DIR).join("source.py"))
        .expect("read Trinity exhibit fixture 01 source.py")
}

fn rust_target_dir() -> PathBuf {
    repo_root()
        .join("implementations")
        .join("rust")
        .join("target")
}

fn locate_binary(name: &str) -> PathBuf {
    let target = rust_target_dir();
    for profile in ["release", "debug"] {
        let candidate = target.join(profile).join(name);
        if candidate.exists() {
            return candidate;
        }
    }
    if let Some(parent) = PathBuf::from(env!("CARGO_BIN_EXE_provekit")).parent() {
        let sibling = parent.join(name);
        if sibling.exists() {
            return sibling;
        }
    }
    panic!(
        "required binary `{name}` not present under {target:?}; build with `cargo build --release -p provekit-walk -p provekit-realize-rust-core -p provekit-cli` before running this slow-test lane"
    );
}

fn provekit_walk_rpc() -> PathBuf {
    locate_binary("provekit-walk-rpc")
}

fn provekit_realize_rust() -> PathBuf {
    locate_binary("provekit-realize-rust")
}

fn python_bin() -> String {
    std::env::var("PYTHON").unwrap_or_else(|_| "python3".to_string())
}

fn java_bin() -> String {
    if let Some(java_home) = std::env::var_os("JAVA_HOME") {
        let candidate = PathBuf::from(java_home).join("bin").join("java");
        if candidate.exists() {
            return candidate.display().to_string();
        }
    }
    std::env::var("JAVA").unwrap_or_else(|_| "java".to_string())
}

fn python_module_available(module: &str) -> bool {
    Command::new(python_bin())
        .arg("-c")
        .arg(format!("import {module}"))
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn require_python_modules(modules: &[&str]) {
    let missing = modules
        .iter()
        .copied()
        .filter(|module| !python_module_available(module))
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "required python modules unavailable: {missing:?}. Install the Python lift and realize kits before running this slow-test lane."
    );
}

fn require_file(path: &Path, label: &str) {
    assert!(
        path.is_file(),
        "{label} missing at {}; build the corresponding real plugin before running this slow-test lane",
        path.display()
    );
}

fn java_lift_jar() -> PathBuf {
    repo_root()
        .join("implementations")
        .join("java")
        .join("provekit-lift-java-source")
        .join("target")
        .join("provekit-lift-java-source.jar")
}

fn java_realize_jar() -> PathBuf {
    repo_root()
        .join("implementations")
        .join("java")
        .join("provekit-realize-java-core")
        .join("target")
        .join("provekit-realize-java.jar")
}

fn python_source_lift_command() -> Vec<String> {
    let repo = repo_root();
    let src = repo
        .join("implementations")
        .join("python")
        .join("provekit-lift-python-source")
        .join("src");
    let pytests_src = repo
        .join("implementations")
        .join("python")
        .join("provekit-lift-py-tests")
        .join("src");
    let joined_pythonpath = std::env::join_paths([src, pytests_src])
        .expect("join PYTHONPATH for python lift")
        .into_string()
        .expect("PYTHONPATH is UTF-8");
    vec![
        "env".to_string(),
        format!("PYTHONPATH={joined_pythonpath}"),
        python_bin(),
        "-m".to_string(),
        "provekit_lift_python_source.bind_rpc".to_string(),
    ]
}

fn java_source_lift_command() -> Vec<String> {
    let jar = java_lift_jar();
    require_file(&jar, "Java source lift jar");
    vec![
        java_bin(),
        "-jar".to_string(),
        jar.display().to_string(),
        "--rpc".to_string(),
    ]
}

fn write_source_workspace(root: &Path, lang: &str, source: &str) {
    fs::create_dir_all(root.join("src")).expect("create source dir");
    fs::create_dir_all(root.join(".provekit")).expect("create .provekit");
    match lang {
        "python" => {
            fs::write(root.join("src/lib.py"), source).expect("write python source");
            fs::write(
                root.join(".provekit/config.toml"),
                "[authoring.lift]\nsurface = \"python\"\n",
            )
            .expect("write python config");
        }
        "java" => {
            fs::write(root.join("src/Lib.java"), source).expect("write java source");
            fs::write(
                root.join(".provekit/config.toml"),
                "[authoring.lift]\nsurface = \"java\"\n",
            )
            .expect("write java config");
        }
        "rust" => {
            fs::write(root.join("src/lib.rs"), source).expect("write rust source");
            fs::write(
                root.join(".provekit/config.toml"),
                "[authoring.lift]\nsurface = \"rust\"\n",
            )
            .expect("write rust config");
        }
        other => panic!("unsupported source language `{other}`"),
    }
}

fn command_toml_array(command: &[String]) -> String {
    command
        .iter()
        .map(|part| serde_json::to_string(part).expect("quote TOML command part"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn write_realize_manifest(root: &Path, target: &str) {
    let command = match target {
        "python" => {
            let realize_src = repo_root()
                .join("implementations")
                .join("python")
                .join("provekit-realize-python-core")
                .join("src");
            vec![
                "env".to_string(),
                format!("PYTHONPATH={}", realize_src.display()),
                python_bin(),
                "-m".to_string(),
                "provekit_realize_python_core".to_string(),
                "--rpc".to_string(),
            ]
        }
        "java" => {
            let jar = java_realize_jar();
            require_file(&jar, "Java realize jar");
            vec![
                java_bin(),
                "-jar".to_string(),
                jar.display().to_string(),
                "--rpc".to_string(),
            ]
        }
        "rust" => vec![
            provekit_realize_rust().display().to_string(),
            "--rpc".to_string(),
        ],
        other => panic!("unsupported target language `{other}`"),
    };
    let manifest_dir = root.join(".provekit").join("realize").join(target);
    fs::create_dir_all(&manifest_dir).expect("create realize manifest dir");
    fs::write(
        manifest_dir.join("manifest.toml"),
        format!(
            "name = \"{target}-realize\"\nlibrary_tag = \"default\"\ncommand = [{}]\nworking_dir = \".\"\n",
            command_toml_array(&command)
        ),
    )
    .expect("write realize manifest");
}

fn lift_source_input(lang: &str, _workspace_root: &Path) -> Input {
    let source_paths = match lang {
        "python" => vec!["src/lib.py"],
        "java" => vec!["src/Lib.java"],
        "rust" => vec!["."],
        other => panic!("unsupported lift language `{other}`"),
    };
    let request = json!({
        "surface": lang,
        "workspace_root": ".",
        "config_path": ".provekit/config.toml",
        "source_paths": source_paths,
        "options": { "layer": "all", "identifyOnly": false }
    });
    Input::Source {
        dialect: dialect(lang),
        bytes: serde_json::to_vec(&request).expect("encode lift request"),
    }
}

fn dialect(lang: &str) -> Dialect {
    match lang {
        "rust" => Dialect::Rust,
        other => Dialect::Other(other.to_string()),
    }
}

fn register_lift(registry: &mut KitRegistry, lang: &str, workspace_root: &Path) {
    let command = match lang {
        "python" => python_source_lift_command(),
        "java" => java_source_lift_command(),
        "rust" => vec![provekit_walk_rpc().display().to_string()],
        other => panic!("unsupported lift language `{other}`"),
    };
    registry.register(
        format!("lift-{lang}"),
        LiftKit::new(
            dialect(lang),
            lang,
            command,
            Some(workspace_root.to_path_buf()),
        ),
        ConformanceDeclaration::NonCarrier {
            reason: "lifts source bytes to DomainClaim via real source lifter subprocess",
        },
    );
}

fn register_bind(registry: &mut KitRegistry) {
    registry.register(
        "bind-default",
        BindKit::default(),
        ConformanceDeclaration::NonCarrier {
            reason: "transforms Input::Term to NamedTerm DomainClaim",
        },
    );
}

fn register_lower(registry: &mut KitRegistry, target: &str, workspace_root: &Path) {
    registry.register(
        format!("lower-{target}"),
        LowerKit::new(
            workspace_root.to_path_buf(),
            target,
            None,
            DispatchRealizeTransport,
        ),
        ConformanceDeclaration::Carrier {
            fixtures_path: repo_root()
                .join("implementations")
                .join(target)
                .join("conformance")
                .join("fixtures"),
            platform_semantics: None,
        },
    );
}

fn execute_lift(lang: &str, workspace: &Path) -> DomainClaim {
    let mut inputs = HashMapInputCatalog::default();
    let source_cid = inputs.insert(lift_source_input(lang, workspace));
    let mut registry = KitRegistry::default();
    register_lift(&mut registry, lang, workspace);
    let path = Input::Path(Box::new(CorePath {
        algebra: vec![PathAlgebra {
            name: "lift".to_string(),
            kit: format!("lift-{lang}"),
            inputs: vec![source_cid],
            depends_on: vec![],
            verb: Verb::Transform,
        }],
    }));
    execute_path(&path, &registry, &inputs)
        .unwrap_or_else(|error| panic!("{lang} lift must execute: {error}"))
        .terminal_claim()
        .clone()
}

fn execute_bind(source_lang: &str, lift_claim: &DomainClaim) -> DomainClaim {
    let payload = lift_claim
        .payload
        .clone()
        .expect("lift claim must carry payload for bind");
    let mut inputs = HashMapInputCatalog::default();
    let term_cid = inputs.insert(Input::Term(payload));
    let mut registry = KitRegistry::default();
    register_bind(&mut registry);
    let path = Input::Path(Box::new(CorePath {
        algebra: vec![PathAlgebra {
            name: "bind".to_string(),
            kit: "bind-default".to_string(),
            inputs: vec![term_cid],
            depends_on: vec![],
            verb: Verb::Transform,
        }],
    }));
    let claim = execute_path(&path, &registry, &inputs)
        .unwrap_or_else(|error| panic!("{source_lang} bind must execute: {error}"))
        .terminal_claim()
        .clone();
    assert!(
        claim.from.contains(&lift_claim.to),
        "{source_lang} bind claim must cite the lift payload CID in `from`"
    );
    claim
}

fn execute_lower(target: &str, workspace: &Path, bind_claim: &DomainClaim) -> DomainClaim {
    let mut inputs = HashMapInputCatalog::default();
    let claim_cid = inputs.insert(Input::Claim(bind_claim.clone()));
    let mut registry = KitRegistry::default();
    register_lower(&mut registry, target, workspace);
    let path = Input::Path(Box::new(CorePath {
        algebra: vec![PathAlgebra {
            name: "lower".to_string(),
            kit: format!("lower-{target}"),
            inputs: vec![claim_cid],
            depends_on: vec![],
            verb: Verb::Transform,
        }],
    }));
    let claim = execute_path(&path, &registry, &inputs)
        .unwrap_or_else(|error| panic!("lower to {target} must execute: {error}"))
        .terminal_claim()
        .clone();
    assert!(
        claim.premises.contains(&bind_claim.cid()),
        "lower to {target} claim must cite the bind claim CID in `premises`"
    );
    claim
}

fn concept_catalog() -> HashMapCatalog {
    let root = repo_root();
    let catalog_root = root
        .join("menagerie")
        .join("concept-shapes")
        .join("catalog");
    let index_path = catalog_root.join("index.json");
    let index: Value =
        serde_json::from_str(&fs::read_to_string(&index_path).expect("read concept catalog index"))
            .expect("parse concept catalog index");
    let entries = index
        .get("entries")
        .and_then(Value::as_object)
        .expect("concept catalog index entries object");
    let mut catalog = HashMapCatalog::default();
    for (cid_text, entry) in entries {
        let Ok(cid) = libprovekit::core::Cid::parse(cid_text.clone()) else {
            continue;
        };
        let bytes = entry
            .get("path")
            .and_then(Value::as_str)
            .and_then(|rel| fs::read(catalog_root.join(rel)).ok())
            .unwrap_or_default();
        catalog.put(cid, bytes);
    }
    catalog
}

fn assert_claim_concept_tier(label: &str, claim: &DomainClaim, catalog: &HashMapCatalog) {
    if let Some(payload) = &claim.payload {
        assert_concept_tier(payload, catalog)
            .unwrap_or_else(|error| panic!("{label} payload left concept tier: {error}"));
    }
}

fn assert_bind_payload_shape(label: &str, claim: &DomainClaim) {
    let payload = claim.payload.as_ref().expect("bind claim payload");
    match payload {
        Term::Op { op_cid, args, .. } => {
            assert_eq!(
                op_cid,
                &concept_bind_result_cid(),
                "{label} bind output must be concept:bind-result"
            );
            assert_eq!(
                args.len(),
                2,
                "{label} concept:bind-result must wrap [original, named]"
            );
        }
        other => panic!("{label} bind payload must be Term::Op, got {other:?}"),
    }
}

fn concept_cids_from_claim(claim: &DomainClaim) -> BTreeMap<String, libprovekit::core::Cid> {
    let mut out = BTreeMap::new();
    if let Some(payload) = &claim.payload {
        for node in payload.walk() {
            if node.op_name.starts_with("concept:") {
                out.entry(node.op_name.to_string())
                    .or_insert_with(|| node.op_cid.clone());
            }
        }
    }
    out
}

fn assert_expected_concepts_present(label: &str, cids: &BTreeMap<String, libprovekit::core::Cid>) {
    for concept in EXPECTED_CONCEPTS {
        assert!(
            cids.contains_key(concept),
            "{label} missing expected fixture concept `{concept}`; saw {:?}",
            cids.keys().collect::<Vec<_>>()
        );
    }
}

fn assert_expected_concepts_stable(chain: &ChainResult) {
    for concept in EXPECTED_CONCEPTS {
        let py = chain
            .python_to_java
            .concept_cids
            .get(concept)
            .unwrap_or_else(|| panic!("python bind missing {concept}"));
        let java = chain
            .java_to_rust
            .concept_cids
            .get(concept)
            .unwrap_or_else(|| panic!("java bind missing {concept}"));
        let rust = chain
            .rust_to_python
            .concept_cids
            .get(concept)
            .unwrap_or_else(|| panic!("rust bind missing {concept}"));
        assert_eq!(py, java, "{concept} CID must survive Python to Java relift");
        assert_eq!(py, rust, "{concept} CID must survive Python to Rust relift");
    }
}

fn run_leg(
    source_lang: &str,
    target_lang: &str,
    source: &str,
    catalog: &HashMapCatalog,
) -> LegResult {
    run_leg_with_expected_concepts(source_lang, target_lang, source, catalog, true)
}

fn run_leg_with_expected_concepts(
    source_lang: &str,
    target_lang: &str,
    source: &str,
    catalog: &HashMapCatalog,
    require_expected_concepts: bool,
) -> LegResult {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path();
    write_source_workspace(workspace, source_lang, source);
    write_realize_manifest(workspace, target_lang);

    let lift_claim = execute_lift(source_lang, workspace);
    assert_claim_concept_tier(&format!("{source_lang} lift"), &lift_claim, catalog);

    let bind_claim = execute_bind(source_lang, &lift_claim);
    assert_bind_payload_shape(&format!("{source_lang} bind"), &bind_claim);
    assert_claim_concept_tier(&format!("{source_lang} bind"), &bind_claim, catalog);
    let concept_cids = concept_cids_from_claim(&bind_claim);
    if require_expected_concepts {
        assert_expected_concepts_present(&format!("{source_lang} bind"), &concept_cids);
    }

    let lower_claim = execute_lower(target_lang, workspace, &bind_claim);
    assert_claim_concept_tier(&format!("{target_lang} lower"), &lower_claim, catalog);
    let realized = LowerKit::<DispatchRealizeTransport>::realized_source_from_claim(&lower_claim)
        .expect("recover realized source from lower claim");
    assert!(
        !realized.is_stub,
        "lower to {target_lang} must use real body templates, not a stub:\n{}",
        realized.source
    );

    LegResult {
        lift_claim_cid: lift_claim.cid(),
        bind_claim_cid: bind_claim.cid(),
        bind_payload_cid: bind_claim.to.clone(),
        lower_claim_cid: lower_claim.cid(),
        concept_cids,
        source: realized.source,
    }
}

fn run_trinity_chain(source: &str) -> ChainResult {
    require_python_modules(&[
        "provekit_lift_py_tests",
        "provekit_lift_python_source",
        "provekit_realize_python_core",
        "blake3",
    ]);
    let catalog = concept_catalog();
    let python_to_java = run_leg("python", "java", source, &catalog);
    let java_to_rust = run_leg("java", "rust", &python_to_java.source, &catalog);
    let rust_to_python = run_leg("rust", "python", &java_to_rust.source, &catalog);
    let final_python = rust_to_python.source.clone();
    ChainResult {
        python_to_java,
        java_to_rust,
        rust_to_python,
        final_python,
    }
}

fn normalized_python_function_source(source: &str, function: &str) -> String {
    let script = r#"
import ast
import sys

function = sys.argv[1]
source = sys.stdin.read()
module = ast.parse(source)
matches = [
    node for node in module.body
    if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name == function
]
if len(matches) != 1:
    raise SystemExit(f"expected exactly one function named {function}, got {len(matches)}")
node = matches[0]
node.returns = None
for arg in [*node.args.posonlyargs, *node.args.args, *node.args.kwonlyargs]:
    arg.annotation = None
if node.args.vararg is not None:
    node.args.vararg.annotation = None
if node.args.kwarg is not None:
    node.args.kwarg.annotation = None
ast.fix_missing_locations(node)
print(ast.unparse(node))
"#;
    let mut child = Command::new(python_bin())
        .arg("-c")
        .arg(script)
        .arg(function)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn python normalizer");
    child
        .stdin
        .take()
        .expect("python normalizer stdin")
        .write_all(source.as_bytes())
        .expect("write python normalizer input");
    let output = child
        .wait_with_output()
        .expect("wait for python normalizer");
    assert!(
        output.status.success(),
        "python function normalizer failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("normalizer stdout is UTF-8")
}

fn python_function_stdout(source: &str, function: &str, args: &[i64]) -> String {
    let temp = tempfile::tempdir().expect("tempdir");
    let module_path = temp.path().join("module.py");
    fs::write(&module_path, source).expect("write python module");
    let script = r#"
import importlib.util
import json
import sys

path = sys.argv[1]
function = sys.argv[2]
args = json.loads(sys.argv[3])
spec = importlib.util.spec_from_file_location("trinity_fixture_module", path)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
print(getattr(module, function)(*args))
"#;
    let output = Command::new(python_bin())
        .arg("-c")
        .arg(script)
        .arg(&module_path)
        .arg(function)
        .arg(serde_json::to_string(args).expect("encode python args"))
        .output()
        .expect("spawn python behavior check");
    assert!(
        output.status.success(),
        "python behavior check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("python stdout is UTF-8")
}

fn mutate_java_middle_hop(source: &str) -> String {
    let mutated = source.replacen(" + ", " - ", 1);
    assert_ne!(
        mutated, source,
        "middle-hop discrimination mutation did not find a Java add expression:\n{source}"
    );
    mutated
}

#[test]
fn fixture_01_positive_full_python_java_rust_python_identity_holds() {
    let original = fixture_source();
    let chain = run_trinity_chain(&original);

    assert_expected_concepts_stable(&chain);

    let original_normalized = normalized_python_function_source(&original, FIXTURE_FUNCTION);
    let final_normalized = normalized_python_function_source(&chain.final_python, FIXTURE_FUNCTION);
    assert_eq!(
        final_normalized, original_normalized,
        "final Python function must be source-equivalent modulo formatter normalization"
    );

    let original_stdout = python_function_stdout(&original, FIXTURE_FUNCTION, &[3, 4]);
    let final_stdout = python_function_stdout(&chain.final_python, FIXTURE_FUNCTION, &[3, 4]);
    assert_eq!(original_stdout, "13\n");
    assert_eq!(final_stdout, original_stdout);
}

#[test]
fn fixture_01_discrimination_middle_hop_mutation_changes_final_output() {
    let original = fixture_source();
    let catalog = concept_catalog();
    let first_leg = run_leg("python", "java", &original, &catalog);
    let mutated_java = mutate_java_middle_hop(&first_leg.source);
    let java_to_rust =
        run_leg_with_expected_concepts("java", "rust", &mutated_java, &catalog, false);
    let rust_to_python =
        run_leg_with_expected_concepts("rust", "python", &java_to_rust.source, &catalog, false);

    let original_stdout = python_function_stdout(&original, FIXTURE_FUNCTION, &[3, 4]);
    let mutated_stdout = python_function_stdout(&rust_to_python.source, FIXTURE_FUNCTION, &[3, 4]);
    assert_ne!(
        mutated_stdout, original_stdout,
        "middle-hop source mutation must change final Python behavior"
    );
}

#[test]
fn fixture_01_structural_claim_cids_are_deterministic_across_reruns() {
    let original = fixture_source();
    let first = run_trinity_chain(&original);
    let second = run_trinity_chain(&original);

    assert_eq!(
        first.python_to_java.lift_claim_cid, second.python_to_java.lift_claim_cid,
        "Python lift claim CID must be deterministic"
    );
    assert_eq!(
        first.java_to_rust.lift_claim_cid, second.java_to_rust.lift_claim_cid,
        "Java lift claim CID must be deterministic"
    );
    assert_eq!(
        first.rust_to_python.lift_claim_cid, second.rust_to_python.lift_claim_cid,
        "Rust lift claim CID must be deterministic"
    );
    assert_eq!(
        first.python_to_java.bind_claim_cid, second.python_to_java.bind_claim_cid,
        "Python bind claim CID must be deterministic"
    );
    assert_eq!(
        first.java_to_rust.bind_claim_cid, second.java_to_rust.bind_claim_cid,
        "Java bind claim CID must be deterministic"
    );
    assert_eq!(
        first.rust_to_python.bind_claim_cid, second.rust_to_python.bind_claim_cid,
        "Rust bind claim CID must be deterministic"
    );
    assert_eq!(
        first.python_to_java.bind_payload_cid, second.python_to_java.bind_payload_cid,
        "Python bind payload CID must be deterministic"
    );
    assert_eq!(
        first.java_to_rust.bind_payload_cid, second.java_to_rust.bind_payload_cid,
        "Java bind payload CID must be deterministic"
    );
    assert_eq!(
        first.rust_to_python.bind_payload_cid, second.rust_to_python.bind_payload_cid,
        "Rust bind payload CID must be deterministic"
    );
    assert_eq!(
        first.python_to_java.lower_claim_cid, second.python_to_java.lower_claim_cid,
        "Python to Java lower claim CID must be deterministic"
    );
    assert_eq!(
        first.java_to_rust.lower_claim_cid, second.java_to_rust.lower_claim_cid,
        "Java to Rust lower claim CID must be deterministic"
    );
    assert_eq!(
        first.rust_to_python.lower_claim_cid, second.rust_to_python.lower_claim_cid,
        "Rust to Python lower claim CID must be deterministic"
    );
    assert_eq!(
        normalized_python_function_source(&first.final_python, FIXTURE_FUNCTION),
        normalized_python_function_source(&second.final_python, FIXTURE_FUNCTION),
        "final normalized Python source must be deterministic"
    );
}
