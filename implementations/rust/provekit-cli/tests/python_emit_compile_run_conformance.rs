// SPDX-License-Identifier: Apache-2.0
//
// Python per-kit emit compile run conformance for issue #1039.
//
// Lane: slow-tests feature gate. This test invokes the real Python LiftKit
// subprocess, the substrate BindKit, and the real Python LowerKit transport.

#![cfg(feature = "slow-tests")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use libprovekit::core::{
    address, execute_path, BindKit, ConformanceDeclaration, Dialect, HashMapInputCatalog, Input,
    KitRegistry, LiftKit, LowerKit, Path as CorePath, PathAlgebra, Verb,
};
use provekit_cli::kit_dispatch::DispatchRealizeTransport;
use serde::Deserialize;
use serde_json::{json, Value};

const REQUIRED_FIXTURE_TYPES: [&str; 5] = [
    "hello_world",
    "recursive_function",
    "arithmetic",
    "control_flow",
    "transported_op_via_concept_citation_comment",
];

#[derive(Debug, Deserialize)]
struct FixtureExpectation {
    fixture_type: String,
    function: String,
    cases: Vec<FixtureCase>,
}

#[derive(Debug, Deserialize)]
struct FixtureCase {
    args: Vec<Value>,
    expected: Value,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root from provekit-cli manifest dir")
}

fn python_bin() -> String {
    std::env::var("PYTHON").unwrap_or_else(|_| "python3".to_string())
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

fn python_lift_source_input(workspace_root: &Path) -> Input {
    let request = json!({
        "surface": "python",
        "workspace_root": workspace_root,
        "config_path": ".provekit/config.toml",
        "source_paths": ["src/lib.py"],
        "options": { "layer": "all", "identifyOnly": false }
    });
    Input::Source {
        dialect: Dialect::Other("python".to_string()),
        bytes: serde_json::to_vec(&request).expect("encode python lift request"),
    }
}

fn write_python_workspace(root: &Path, source: &str) {
    fs::create_dir_all(root.join("src")).expect("create src dir");
    fs::write(root.join("src/lib.py"), source).expect("write python source");
    fs::create_dir_all(root.join(".provekit")).expect("create .provekit");
    fs::write(
        root.join(".provekit/config.toml"),
        "[authoring.lift]\nsurface = \"python\"\n",
    )
    .expect("write config");
}

fn write_python_realize_manifest(root: &Path) {
    let repo = repo_root();
    let realize_src = repo
        .join("implementations")
        .join("python")
        .join("provekit-realize-python-core")
        .join("src");
    let manifest_dir = root.join(".provekit").join("realize").join("python");
    fs::create_dir_all(&manifest_dir).expect("create realize manifest dir");
    fs::write(
        manifest_dir.join("manifest.toml"),
        format!(
            "name = \"python-realize\"\nlibrary_tag = \"default\"\ncommand = [\"env\", \"PYTHONPATH={}\", \"{}\", \"-m\", \"provekit_realize_python_core\", \"--rpc\"]\nworking_dir = \".\"\n",
            realize_src.display(),
            python_bin(),
        ),
    )
    .expect("write python realize manifest");
}

fn register_python_lift(registry: &mut KitRegistry, workspace_root: &Path) {
    registry.register(
        "lift-python",
        LiftKit::new(
            Dialect::Other("python".to_string()),
            "python",
            python_source_lift_command(),
            Some(workspace_root.to_path_buf()),
        ),
        ConformanceDeclaration::NonCarrier {
            reason: "lifts python source bytes to DomainClaim via provekit-lift-python-source",
        },
    );
}

fn register_bind(registry: &mut KitRegistry) {
    registry.register(
        "bind-default",
        BindKit::default(),
        ConformanceDeclaration::NonCarrier {
            reason: "transforms Input::Term to NamedTerm DomainClaim; emits no target source",
        },
    );
}

fn register_python_lower(registry: &mut KitRegistry, workspace_root: &Path) {
    registry.register(
        "lower-python",
        LowerKit::new(
            workspace_root.to_path_buf(),
            "python",
            None,
            DispatchRealizeTransport,
        ),
        ConformanceDeclaration::Carrier {
            fixtures_path: repo_root()
                .join("implementations")
                .join("python")
                .join("conformance")
                .join("fixtures"),
        },
    );
}

fn assert_python_carrier_fixture_set(registry: &KitRegistry) {
    let Some(ConformanceDeclaration::Carrier { fixtures_path }) =
        registry.conformance("lower-python")
    else {
        panic!("lower-python must register as a Carrier kit");
    };
    assert_fixture_set(fixtures_path);
}

fn assert_fixture_set(fixtures_path: &Path) {
    assert!(
        fixtures_path.is_dir(),
        "Carrier fixtures_path must resolve to a directory: {}",
        fixtures_path.display()
    );
    for required in REQUIRED_FIXTURE_TYPES {
        let fixture = fixtures_path.join(required);
        assert!(
            fixture.join("original.py").is_file(),
            "fixture `{required}` missing original.py"
        );
        let expected = read_expected(&fixture);
        assert_eq!(expected.fixture_type, required);
        assert!(
            !expected.cases.is_empty(),
            "fixture `{required}` must declare behavior cases"
        );
    }
}

fn read_expected(fixture_dir: &Path) -> FixtureExpectation {
    let raw = fs::read_to_string(fixture_dir.join("expected.json")).expect("read expected.json");
    serde_json::from_str(&raw).expect("decode expected.json")
}

fn fixture_dirs() -> Vec<PathBuf> {
    let root = repo_root()
        .join("implementations")
        .join("python")
        .join("conformance")
        .join("fixtures");
    let mut dirs = fs::read_dir(root)
        .expect("read python conformance fixtures")
        .map(|entry| entry.expect("fixture dir entry").path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    dirs.sort();
    dirs
}

fn lift_bind_lower_python(source: &str) -> String {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path();
    write_python_workspace(workspace, source);
    write_python_realize_manifest(workspace);

    let mut inputs = HashMapInputCatalog::default();
    let source_cid = inputs.insert(python_lift_source_input(workspace));

    let mut registry = KitRegistry::default();
    register_python_lift(&mut registry, workspace);
    register_bind(&mut registry);
    register_python_lower(&mut registry, workspace);
    assert_python_carrier_fixture_set(&registry);

    let lift_only = Input::Path(Box::new(CorePath {
        algebra: vec![PathAlgebra {
            name: "lift".to_string(),
            kit: "lift-python".to_string(),
            inputs: vec![source_cid.clone()],
            depends_on: vec![],
            verb: Verb::Transform,
        }],
    }));
    let lift_chain =
        execute_path(&lift_only, &registry, &inputs).expect("python lift step must execute");
    let lift_to = lift_chain.terminal_claim().to.clone();

    let lift_bind = Input::Path(Box::new(CorePath {
        algebra: vec![
            PathAlgebra {
                name: "lift".to_string(),
                kit: "lift-python".to_string(),
                inputs: vec![source_cid.clone()],
                depends_on: vec![],
                verb: Verb::Transform,
            },
            PathAlgebra {
                name: "bind".to_string(),
                kit: "bind-default".to_string(),
                inputs: vec![lift_to.clone()],
                depends_on: vec!["lift".to_string()],
                verb: Verb::Transform,
            },
        ],
    }));
    let bind_chain =
        execute_path(&lift_bind, &registry, &inputs).expect("python lift bind path must execute");
    let bind_claim = bind_chain
        .claim_at_step("bind")
        .expect("bind step claim must exist")
        .clone();
    let bind_claim_input_cid = address(&Input::Claim(bind_claim));

    let path = Input::Path(Box::new(CorePath {
        algebra: vec![
            PathAlgebra {
                name: "lift".to_string(),
                kit: "lift-python".to_string(),
                inputs: vec![source_cid],
                depends_on: vec![],
                verb: Verb::Transform,
            },
            PathAlgebra {
                name: "bind".to_string(),
                kit: "bind-default".to_string(),
                inputs: vec![lift_to],
                depends_on: vec!["lift".to_string()],
                verb: Verb::Transform,
            },
            PathAlgebra {
                name: "lower".to_string(),
                kit: "lower-python".to_string(),
                inputs: vec![bind_claim_input_cid],
                depends_on: vec!["bind".to_string()],
                verb: Verb::Transform,
            },
        ],
    }));
    let chain =
        execute_path(&path, &registry, &inputs).expect("python lift bind lower path must execute");
    let lower_claim = chain
        .claim_at_step("lower")
        .expect("lower step claim must exist");
    LowerKit::<DispatchRealizeTransport>::realized_source_from_claim(lower_claim)
        .expect("recover python realized source")
        .source
}

fn write_executable_module(path: &Path, source: &str, function: &str, cases: &[FixtureCase]) {
    let harness = json!({
        "function": function,
        "cases": cases
            .iter()
            .map(|case| json!({ "args": case.args.clone() }))
            .collect::<Vec<_>>(),
    });
    let harness_json = serde_json::to_string(&harness).expect("encode fixture harness");
    fs::write(
        path,
        format!(
            "{source}\nif __name__ == \"__main__\":\n    import json\n    _fixture = json.loads({harness_json:?})\n    _fn = globals()[_fixture[\"function\"]]\n    _out = [_fn(*case[\"args\"]) for case in _fixture[\"cases\"]]\n    print(json.dumps(_out, separators=(\",\", \":\"), sort_keys=True))\n"
        ),
    )
    .expect("write executable python module");
}

fn py_compile(path: &Path) -> Result<(), String> {
    let output = Command::new(python_bin())
        .arg("-m")
        .arg("py_compile")
        .arg(path)
        .output()
        .expect("spawn python py_compile");
    if output.status.success() {
        return Ok(());
    }
    Err(String::from_utf8_lossy(&output.stderr).to_string())
}

fn run_python(path: &Path) -> Result<Vec<Value>, String> {
    let output = Command::new(python_bin())
        .arg(path)
        .output()
        .expect("spawn python emitted module");
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("decode python stdout as JSON: {error}"))
}

#[test]
fn python_carrier_registry_points_to_required_fixture_set() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut registry = KitRegistry::default();
    register_python_lower(&mut registry, temp.path());
    assert_python_carrier_fixture_set(&registry);
}

#[test]
fn python_emit_compile_run_fixtures_match_original_behavior() {
    require_python_modules(&[
        "provekit_lift_py_tests",
        "provekit_lift_python_source",
        "provekit_realize_python_core",
        "blake3",
    ]);

    for fixture_dir in fixture_dirs() {
        let expected = read_expected(&fixture_dir);
        let original_source =
            fs::read_to_string(fixture_dir.join("original.py")).expect("read original.py");
        let expected_outputs = expected
            .cases
            .iter()
            .map(|case| case.expected.clone())
            .collect::<Vec<_>>();
        let emitted_source = lift_bind_lower_python(&original_source);

        let temp = tempfile::tempdir().expect("tempdir");
        let original_path = temp.path().join("original.py");
        let emitted_path = temp.path().join("emitted.py");
        write_executable_module(
            &original_path,
            &original_source,
            &expected.function,
            &expected.cases,
        );
        write_executable_module(
            &emitted_path,
            &emitted_source,
            &expected.function,
            &expected.cases,
        );

        py_compile(&emitted_path).unwrap_or_else(|stderr| {
            panic!(
                "CompositionRefusalMemento failure_kind=target-compile-failure fixture={} detail={stderr}",
                expected.fixture_type
            )
        });

        let original_outputs = run_python(&original_path)
            .unwrap_or_else(|stderr| panic!("original fixture execution failed: {stderr}"));
        assert_eq!(original_outputs, expected_outputs);

        let emitted_outputs = run_python(&emitted_path).unwrap_or_else(|stderr| {
            panic!(
                "CompositionRefusalMemento failure_kind=target-behavior-divergence fixture={} detail={stderr}",
                expected.fixture_type
            )
        });
        assert_eq!(
            emitted_outputs, original_outputs,
            "CompositionRefusalMemento failure_kind=target-behavior-divergence fixture={}",
            expected.fixture_type
        );
    }
}
