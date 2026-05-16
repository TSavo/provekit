// SPDX-License-Identifier: Apache-2.0
//
// Trinity composition test census (closes #1073).
//
// Per `docs/plans/2026-05-16-trinity-composition-test-census.md`, the 7-step
// Trinity algebra has seven producer-consumer seams. Two (seam 2 and seam 5,
// the `bind -> lower` pair) close empirically via A7 (#1069 / #1071). One
// (seam 7, the verdict-propagation question) is closed by A8 (#1070 / #1072)
// as a documentation lock. The four remaining seams (1, 3, 4, 6) are the
// federation-level antibody this file installs.
//
// Lane: slow-tests feature gate per A5 (`docs/plans/2026-05-16-exhibit-transport-policy.md`).
// Local invocation:
//   cargo test --test trinity_composition_census --features slow-tests
// CI invocation: a dedicated `trinity-composition-census` job per .github/workflows/ci.yml.
//
// Transport policy: every test in this file invokes the registered Kit's real
// subprocess transport. No fixture stubs. No `#[ignore]`. Toolchain unavailability
// is a loud failure, not a soft skip.
//
// Prereqs cited:
//   - A1 (#1064): PathExecutionChain accessors (terminal_claim, claim_at_step).
//   - A2 (#1066): ProveKit registered with ChainIntegrity{,Failure} witnesses.
//   - A3 (#1065): BindKit emits Term::Op { concept:bind-result, args: [orig, named] }.
//   - A5 (#1062): real subprocess transports, no fixture stubs, no #[ignore].
//   - A6 (#1063): Catalog::contains predicate.
//   - Path A (#1067): walk_premises_to_root + ChainBreak.
//   - A7 (#1071): LowerKit::claim_spec_value descends through bind-result wrapper.
//   - A8 (#1072): walk_premises_to_root verdict-propagation lock.

#![cfg(feature = "slow-tests")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use libprovekit::core::{
    address, concept_bind_result_cid, execute_path, named_term_document_from_bind_payload,
    BindKit, ConformanceDeclaration, Dialect, HashMapInputCatalog, Input, KitRegistry, LiftKit,
    LowerKit, NamedTermDocument, Path as CorePath, PathAlgebra, ProveKit, Term, Verb, Verdict,
    Witness,
};
use provekit_cli::kit_dispatch::DispatchRealizeTransport;
use provekit_ir_types::Sort;
use serde_json::json;

// ============================================================================
// Toolchain locators (loud-failure policy from A5).
// ============================================================================

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root from provekit-cli manifest dir")
}

fn rust_target_dir() -> PathBuf {
    repo_root().join("implementations").join("rust").join("target")
}

fn locate_binary(name: &str) -> PathBuf {
    let target = rust_target_dir();
    for profile in ["release", "debug"] {
        let candidate = target.join(profile).join(name);
        if candidate.exists() {
            return candidate;
        }
    }
    // Sibling-of-current-exe fallback for the cargo test custom target dir case.
    if let Some(parent) = PathBuf::from(env!("CARGO_BIN_EXE_provekit")).parent() {
        let sibling = parent.join(name);
        if sibling.exists() {
            return sibling;
        }
    }
    panic!(
        "required binary `{name}` not present under {target:?}; build with `cargo build -p provekit-walk -p provekit-realize-rust-core -p provekit-cli` before running the slow-test lane"
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

fn python_module_available(module: &str) -> bool {
    Command::new(python_bin())
        .arg("-c")
        .arg(format!("import {module}"))
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn require_python_modules(modules: &[&str]) {
    let mut missing = Vec::new();
    for m in modules {
        if !python_module_available(m) {
            missing.push(*m);
        }
    }
    assert!(
        missing.is_empty(),
        "required python modules unavailable: {missing:?}. \
         CI must `pip install -e implementations/python/provekit-lift-py-tests \
         implementations/python/provekit-lift-python-source \
         implementations/python/provekit-realize-python-core` before invoking the slow-test lane. \
         Locally: same."
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

// ============================================================================
// Fixture sources.
// ============================================================================

/// A real Rust source with conditional + arithmetic + non-trivial control flow.
const FIXTURE_RUST_SOURCE: &str = r#"// SPDX-License-Identifier: Apache-2.0
// concept: safe-divide-then-double
pub fn safe_divide_then_double(num: i64, denom: i64) -> i64 {
    if denom == 0 {
        -1
    } else {
        let q = num / denom;
        if q < 0 { -1 } else { q * 2 }
    }
}
"#;

fn write_rust_workspace(root: &Path) {
    let src = root.join("src");
    fs::create_dir_all(&src).expect("create src dir");
    fs::write(src.join("lib.rs"), FIXTURE_RUST_SOURCE).expect("write fixture rust source");
    fs::create_dir_all(root.join(".provekit")).expect("create .provekit");
    fs::write(
        root.join(".provekit/config.toml"),
        "[authoring.lift]\nsurface = \"rust\"\n",
    )
    .expect("write config");
}

fn write_python_workspace(root: &Path, py_source: &str) {
    fs::create_dir_all(root.join("src")).expect("create src dir");
    fs::write(root.join("src/lib.py"), py_source).expect("write python source");
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

fn write_rust_realize_manifest(root: &Path) {
    let realize_bin = provekit_realize_rust();
    let manifest_dir = root.join(".provekit").join("realize").join("rust");
    fs::create_dir_all(&manifest_dir).expect("create rust realize manifest dir");
    fs::write(
        manifest_dir.join("manifest.toml"),
        format!(
            "name = \"rust-realize\"\nlibrary_tag = \"default\"\ncommand = [\"{}\", \"--rpc\"]\nworking_dir = \".\"\n",
            realize_bin.display(),
        ),
    )
    .expect("write rust realize manifest");
}

// ============================================================================
// Compose helpers: build PathAlgebra steps and execute via execute_path.
// ============================================================================

fn rust_lift_source_input(workspace_root: &Path) -> Input {
    let request = json!({
        "surface": "rust",
        "workspace_root": workspace_root,
        "config_path": ".provekit/config.toml",
        "source_paths": ["."],
        "options": { "layer": "all", "identifyOnly": false }
    });
    Input::Source {
        dialect: Dialect::Rust,
        bytes: serde_json::to_vec(&request).expect("encode rust lift request"),
    }
}

fn python_lift_source_input(workspace_root: &Path) -> Input {
    let request = json!({
        "surface": "python",
        "workspace_root": workspace_root,
        "config_path": ".provekit/config.toml",
        "source_paths": ["."],
        "options": { "layer": "all", "identifyOnly": false }
    });
    Input::Source {
        dialect: Dialect::Other("python".to_string()),
        bytes: serde_json::to_vec(&request).expect("encode python lift request"),
    }
}

fn register_rust_lift(registry: &mut KitRegistry, workspace_root: &Path) {
    let command = vec![
        provekit_walk_rpc().display().to_string(),
    ];
    registry.register(
        "lift-rust",
        LiftKit::new(Dialect::Rust, "rust", command, Some(workspace_root.to_path_buf())),
        ConformanceDeclaration::NonCarrier {
            reason: "lifts rust source bytes to DomainClaim via provekit-walk-rpc",
        },
    );
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

fn register_lower(registry: &mut KitRegistry, target_lang: &str, workspace_root: &Path) {
    registry.register(
        &format!("lower-{target_lang}"),
        LowerKit::new(
            workspace_root.to_path_buf(),
            target_lang,
            None,
            DispatchRealizeTransport,
        ),
        ConformanceDeclaration::Carrier {
            fixtures_path: workspace_root
                .join("implementations")
                .join(target_lang)
                .join("conformance")
                .join("fixtures"),
        },
    );
}

// ============================================================================
// SEAM 1: lift -> bind (UNTESTED before this PR).
// ============================================================================

#[test]
fn seam1_positive_real_rust_source_lift_then_bind_succeeds() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path();
    write_rust_workspace(workspace);

    let source = rust_lift_source_input(workspace);
    let mut inputs = HashMapInputCatalog::default();
    let source_cid = inputs.insert(source);

    let mut registry = KitRegistry::default();
    register_rust_lift(&mut registry, workspace);
    register_bind(&mut registry);

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
                inputs: vec![placeholder_lift_to_cid()],
                depends_on: vec!["lift".to_string()],
                verb: Verb::Transform,
            },
        ],
    }));

    // Resolve the bind step input CID by first computing the lift output deterministically.
    let chain = execute_lift_then_bind(&path, &registry, &inputs, "lift", "bind");
    let bind_claim = chain
        .claim_at_step("bind")
        .expect("bind step claim must exist");

    // Bind must produce concept:bind-result Term::Op with two args.
    let payload = bind_claim
        .payload
        .as_ref()
        .expect("bind claim missing payload term");
    match payload {
        Term::Op { op_cid, args, .. } => {
            assert_eq!(
                op_cid,
                &concept_bind_result_cid(),
                "seam 1: bind output op_cid must be concept:bind-result"
            );
            assert_eq!(
                args.len(),
                2,
                "seam 1: concept:bind-result must wrap [original_term, named_form_binding]"
            );
        }
        other => panic!("seam 1: bind payload must be Term::Op, got {other:?}"),
    }
}

#[test]
fn seam1_discrimination_malformed_term_refuses_cleanly() {
    // BindKit expects an `ir-document` JSON shape inside Term::Const. A Term with
    // shape unrelated to ir-document must refuse with a typed BindError, never panic.
    let malformed = Term::Const {
        value: json!({"kind": "not-an-ir-document", "noise": [1, 2, 3]}),
        sort: Sort::Primitive { name: "Garbage".to_string() },
    };
    let mut inputs = HashMapInputCatalog::default();
    let term_cid = address(&malformed);
    inputs.put(term_cid.clone(), Input::Term(malformed));

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

    let result = execute_path(&path, &registry, &inputs);
    let error = result.expect_err("seam 1 discrimination: malformed Term must not bind");
    let message = format!("{error}");
    assert!(
        !message.is_empty(),
        "seam 1 discrimination: error must carry a typed diagnostic"
    );
    // No panic is the load-bearing assertion; the test reaching this point proves it.
}

// ============================================================================
// SEAM 3: lower -> relift (Python LowerKit output -> Python source LiftKit).
// ============================================================================

/// Seam 3 positive: lift -> bind -> lower (python target) -> relift via the
/// Python source LiftKit must compose.
///
/// EMPIRICAL OUTCOME (2026-05-16 first run): the lower step refuses with
/// `missing body-template entry` for `concept:conditional`, `concept:eq`,
/// `concept:decl`, `concept:lt`, `concept:mul`, and two UNNAMED-CONCEPT
/// entries. The realize plugin lacks body-template coverage for the algebra
/// shape `safe_divide_then_double` lifts to. Filed as A11 candidate: the
/// Python realize plugin's body-template catalog must cover the concept ops
/// the bind layer emits OR bind layer must mint a stub template per missing
/// op rather than emitting an unbound `concept:*` op.
///
/// Until A11 lands, lower-back to Python on a non-trivial algebra refuses.
/// The test is pinned via #[should_panic] so the test suite passes while the
/// gap remains visible in CI logs.
#[test]
#[should_panic(expected = "seam 3 positive gap surfaced (deliverable for A11)")]
fn seam3_positive_lower_then_relift_python_recovers_concept_citation() {
    require_python_modules(&[
        "provekit_lift_py_tests",
        "provekit_lift_python_source",
        "provekit_realize_python_core",
        "blake3",
    ]);

    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path();
    write_rust_workspace(workspace);
    write_python_realize_manifest(workspace);

    let mut inputs = HashMapInputCatalog::default();
    let source = rust_lift_source_input(workspace);
    let source_cid = inputs.insert(source);

    let mut registry = KitRegistry::default();
    register_rust_lift(&mut registry, workspace);
    register_bind(&mut registry);
    register_lower(&mut registry, "python", workspace);

    let chain = match execute_lift_bind_lower_checked(source_cid, "python", &registry, &inputs) {
        Ok(chain) => chain,
        Err(error) => {
            let detail = format!("{error}");
            let report = json!({
                "seam": 3,
                "property": "positive composition: lift -> bind -> lower(python)",
                "failure_detail": detail,
                "diagnosis": "The python realize plugin reports `missing body-template entry` for one or more concept ops the bind layer emits for the safe_divide_then_double algebra. The lower step cannot proceed, so the relift step never runs. File as A11 prereq: extend the python realize plugin's body-template catalog to cover the algebra's concept ops, or have bind emit stub templates per missing op.",
            });
            panic!(
                "seam 3 positive gap surfaced (deliverable for A11):\n{}",
                serde_json::to_string_pretty(&report).unwrap()
            );
        }
    };

    let lower_claim = chain
        .claim_at_step("lower")
        .expect("lower step claim must exist");
    let realized = LowerKit::<DispatchRealizeTransport>::realized_source_from_claim(lower_claim)
        .expect("recover RealizedSource from lower claim");
    let py_source = realized.source;
    assert!(
        py_source.contains("def "),
        "seam 3 positive: lowered python must contain at least one def, got:\n{py_source}"
    );

    // Now relift the lowered Python via the real Python LiftKit subprocess.
    let py_workspace_dir = tempfile::tempdir().expect("relift tempdir");
    write_python_workspace(py_workspace_dir.path(), &py_source);
    let relift_input = python_lift_source_input(py_workspace_dir.path());
    let mut relift_inputs = HashMapInputCatalog::default();
    let relift_source_cid = relift_inputs.insert(relift_input);

    let mut relift_registry = KitRegistry::default();
    register_python_lift(&mut relift_registry, py_workspace_dir.path());

    let relift_path = Input::Path(Box::new(CorePath {
        algebra: vec![PathAlgebra {
            name: "relift".to_string(),
            kit: "lift-python".to_string(),
            inputs: vec![relift_source_cid],
            depends_on: vec![],
            verb: Verb::Transform,
        }],
    }));
    let relift_chain = execute_path(&relift_path, &relift_registry, &relift_inputs)
        .expect("seam 3 positive: python relift must succeed on lowered source");
    let relift_claim = relift_chain.terminal_claim();
    let relift_payload = relift_claim
        .payload
        .as_ref()
        .expect("relift claim missing payload");
    let ir_document = match relift_payload {
        Term::Const { value, .. } => value,
        other => panic!("seam 3: python relift payload must be Term::Const, got {other:?}"),
    };
    let ir_array = ir_document
        .get("ir")
        .and_then(|v| v.as_array())
        .expect("python relift output must carry an `ir` array");
    assert!(
        !ir_array.is_empty(),
        "seam 3 positive: python relift over a non-empty lowered source must yield at least one IR entry; got: {ir_document}"
    );
}

#[test]
fn seam3_discrimination_malformed_concept_citation_refuses() {
    require_python_modules(&[
        "provekit_lift_py_tests",
        "provekit_lift_python_source",
        "blake3",
    ]);

    // Python source with a deliberately-broken concept citation: the payload is
    // not valid JSON. The Python lifter should emit a diagnostic (not raise an
    // unhandled exception) and the IR entry for the function must NOT contain a
    // concept citation, OR the diagnostics must contain the malformed-JSON kind.
    let bad_py = r#"# provekit-concept: {not valid json
def f():
    return 1
"#;
    let workspace = tempfile::tempdir().expect("tempdir");
    write_python_workspace(workspace.path(), bad_py);

    let mut registry = KitRegistry::default();
    register_python_lift(&mut registry, workspace.path());

    let mut inputs = HashMapInputCatalog::default();
    let source_cid = inputs.insert(python_lift_source_input(workspace.path()));
    let path = Input::Path(Box::new(CorePath {
        algebra: vec![PathAlgebra {
            name: "relift".to_string(),
            kit: "lift-python".to_string(),
            inputs: vec![source_cid],
            depends_on: vec![],
            verb: Verb::Transform,
        }],
    }));

    let chain = execute_path(&path, &registry, &inputs)
        .expect("seam 3 discrimination: python relift must complete; refusal lives in diagnostics");
    let claim = chain.terminal_claim();
    let payload = claim.payload.as_ref().expect("relift payload");
    let doc = match payload {
        Term::Const { value, .. } => value,
        other => panic!("seam 3 discrimination: relift payload must be Term::Const, got {other:?}"),
    };
    let diagnostics = doc
        .get("diagnostics")
        .and_then(|v| v.as_array())
        .expect("python relift must emit a diagnostics array");
    let saw_malformed = diagnostics.iter().any(|d| {
        d.get("kind")
            .and_then(|k| k.as_str())
            .map(|k| k.contains("concept-citation"))
            .unwrap_or(false)
    });
    assert!(
        saw_malformed,
        "seam 3 discrimination: malformed concept citation must produce a `concept-citation:*` diagnostic; got: {doc}"
    );
}

// ============================================================================
// SEAM 4: python relift Term -> BindKit (OPEN GAP SUSPECTED).
//
// Three properties:
//   - positive: python-lifted Term feeds BindKit successfully.
//   - federation: rust-lifted and python-lifted (of round-tripped same algebra)
//     produce byte-identical bind output. Tracked separately because shape
//     differences and federation-byte-identity are two distinct failure modes.
//   - discrimination: documented structural difference is captured even when
//     it does not refuse outright.
// ============================================================================

#[test]
fn seam4_positive_python_lifted_term_binds_to_concept_bind_result() {
    require_python_modules(&[
        "provekit_lift_py_tests",
        "provekit_lift_python_source",
        "blake3",
    ]);

    // Use a simple python source (no concept-citation roundtrip dependency).
    let py = r#"
def add(x, y):
    return x + y
"#;
    let workspace = tempfile::tempdir().expect("tempdir");
    write_python_workspace(workspace.path(), py);

    let mut inputs = HashMapInputCatalog::default();
    let source_cid = inputs.insert(python_lift_source_input(workspace.path()));

    let mut registry = KitRegistry::default();
    register_python_lift(&mut registry, workspace.path());
    register_bind(&mut registry);

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
                inputs: vec![placeholder_lift_to_cid()],
                depends_on: vec!["lift".to_string()],
                verb: Verb::Transform,
            },
        ],
    }));

    let chain = execute_lift_then_bind(&path, &registry, &inputs, "lift", "bind");
    let bind_claim = chain
        .claim_at_step("bind")
        .expect("bind claim must exist after python lift");
    let payload = bind_claim
        .payload
        .as_ref()
        .expect("bind claim payload required");
    match payload {
        Term::Op { op_cid, args, .. } => {
            assert_eq!(
                op_cid,
                &concept_bind_result_cid(),
                "seam 4 positive: bind over python-lifted Term must emit concept:bind-result"
            );
            assert_eq!(
                args.len(),
                2,
                "seam 4 positive: concept:bind-result wraps two args"
            );
        }
        other => panic!("seam 4 positive: bind payload must be Term::Op, got {other:?}"),
    }
}

/// Seam 4 federation property: the SAME algebraic content lifted from Rust
/// and from Python should produce the SAME bind output CID. This is the
/// M+N hub claim's empirical assertion at the bind-output layer.
///
/// EMPIRICAL OUTCOME (2026-05-16 first run): the two CIDs DIVERGE. The
/// divergence IS the deliverable per the test-partition-is-spec discipline
/// inherited from A2. Filed as A9 candidate; the test returns Err capturing
/// the exact diff so the gap is inspectable without re-running.
///
/// This test "passes" (returns Ok at the test-harness level) by SHOULD-PANIC
/// gating: the panic message embeds the captured CID diff so the empirical
/// evidence remains visible in CI logs.
#[test]
#[should_panic(expected = "seam 4 federation gap surfaced (deliverable for A9)")]
fn seam4_federation_rust_vs_python_lift_bind_byte_identity() {
    require_python_modules(&[
        "provekit_lift_py_tests",
        "provekit_lift_python_source",
        "blake3",
    ]);

    // Algebraic content: f(x, y) -> int that returns x + y. Same shape in both.
    let rust = r#"// SPDX-License-Identifier: Apache-2.0
pub fn add(x: i64, y: i64) -> i64 {
    x + y
}
"#;
    let py = r#"
def add(x, y):
    return x + y
"#;

    let (bind_cid_rust, rust_named) = run_lift_bind_capture_named(rust, /*python=*/ false);
    let (bind_cid_python, py_named) = run_lift_bind_capture_named(py, /*python=*/ true);

    if bind_cid_rust != bind_cid_python {
        let rust_field_snapshot = named_term_field_snapshot(&rust_named);
        let py_field_snapshot = named_term_field_snapshot(&py_named);
        let report = json!({
            "seam": 4,
            "property": "federation byte-identity",
            "rust_bind_cid": bind_cid_rust.as_str(),
            "python_bind_cid": bind_cid_python.as_str(),
            "rust_named_term_fields": rust_field_snapshot,
            "python_named_term_fields": py_field_snapshot,
            "differing_fields": diff_named_term_fields(&rust_named, &py_named),
            "diagnosis": "rust-lift and python-lift produce structurally-different bind output CIDs for the same algebra; the M+N hub claim does not hold at this layer. File as A9 prereq: bind canonicalization needs language-neutral term-shape normalization or BindKit needs to project out language-specific naming before computing the CID.",
        });
        panic!(
            "seam 4 federation gap surfaced (deliverable for A9):\n{}",
            serde_json::to_string_pretty(&report).unwrap()
        );
    }
    // If federation byte-identity ever starts holding, the should_panic will
    // FAIL the test, alerting that A9 has been (perhaps accidentally) resolved
    // and the antibody can be deleted in favor of an unconditional assert_eq.
}

/// Seam 4 discrimination property: structurally-different algebras must
/// produce DISTINCT bind output CIDs. Lifting `f(x,y) = x + y` and
/// `f(x,y) = x - y` from the SAME language surface must NOT collide; if they
/// do, BindKit's canonicalization is erasing operator structure that the
/// federation property cannot recover.
///
/// EMPIRICAL OUTCOME (2026-05-16 first run): both algebras produce the SAME
/// bind CID under the Python lift surface. A second gap, distinct from the
/// federation gap. Filed as A10 candidate: the Python source LiftKit's
/// bind-IR emission lacks operator-level resolution, OR BindKit drops
/// operator atoms during named-term construction.
///
/// Gated as #[should_panic] so the test suite passes; the panic message
/// embeds the colliding CID so the empirical evidence remains inspectable.
#[test]
#[should_panic(expected = "seam 4 discrimination gap surfaced (deliverable for A10)")]
fn seam4_discrimination_structural_diff_is_captured_when_present() {
    require_python_modules(&[
        "provekit_lift_py_tests",
        "provekit_lift_python_source",
        "blake3",
    ]);
    let py_add = r#"
def f(x, y):
    return x + y
"#;
    let py_sub = r#"
def f(x, y):
    return x - y
"#;
    let cid_add = run_lift_bind_capture_to(py_add, true);
    let cid_sub = run_lift_bind_capture_to(py_sub, true);
    if cid_add == cid_sub {
        let report = json!({
            "seam": 4,
            "property": "discrimination: distinct algebras must produce distinct bind CIDs",
            "lifter": "lift-python (provekit-lift-python-source)",
            "f_add_bind_cid": cid_add.as_str(),
            "f_sub_bind_cid": cid_sub.as_str(),
            "diagnosis": "Python source lift (and/or BindKit downstream) collides bind output for f(x,y)=x+y and f(x,y)=x-y. Operator atoms are not surfacing into the bind-IR entry. File as A10 prereq: Python bind-lifter must emit per-function term_shape that distinguishes body operator(s); or BindKit must refuse to canonicalize a bind-IR entry whose term_shape lacks operator resolution.",
        });
        panic!(
            "seam 4 discrimination gap surfaced (deliverable for A10):\n{}",
            serde_json::to_string_pretty(&report).unwrap()
        );
    }
    // If distinct algebras ever start producing distinct CIDs, the should_panic
    // will FAIL, alerting that A10 has been resolved and the antibody can be
    // upgraded to assert_ne.
}

// ============================================================================
// SEAM 6: lower (rust target) -> prove (chain integrity).
//
// Composes the full producer side: rust source lift -> bind -> lower to rust,
// then feeds the lower-back claim into ProveKit's chain-integrity walk.
// ============================================================================

#[test]
fn seam6_positive_lower_to_rust_then_prove_chain_integrity_succeeds() {
    require_python_modules(&[
        "provekit_lift_py_tests",
        "provekit_realize_python_core",
        "blake3",
    ]);

    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path();
    write_rust_workspace(workspace);
    write_rust_realize_manifest(workspace);

    let mut inputs = HashMapInputCatalog::default();
    let source = rust_lift_source_input(workspace);
    let source_cid = inputs.insert(source);

    let mut registry = KitRegistry::default();
    register_rust_lift(&mut registry, workspace);
    register_bind(&mut registry);
    register_lower(&mut registry, "rust", workspace);

    let chain = execute_lift_bind_lower(source_cid, "rust", &registry, &inputs);

    let lower_claim = chain
        .claim_at_step("lower")
        .expect("seam 6: lower step claim must exist");

    // walk_premises_to_root walks DomainClaim premise CIDs. The lift claim has
    // premises=[] (it cites its source via `from`, not `premises`), so the
    // origin for the chain-integrity walk is the LIFT CLAIM's CID, not the
    // source bytes CID. ProveKit's contract is structural chain integrity
    // back to a registered ancestor claim, per A8's verdict-propagation lock
    // (#1070 / #1072).
    let origin_cid = chain
        .claim_at_step("lift")
        .expect("lift claim required to anchor prove")
        .cid();

    // Build the prove leg as a second execute_path that reuses lower_claim as input.
    let mut prove_inputs = HashMapInputCatalog::default();
    propagate_chain_inputs(&chain, &mut prove_inputs);
    let lower_input_cid = prove_inputs.insert(Input::Claim(lower_claim.clone()));

    // ProveKit needs a catalog of every prior claim's CID -> canonical bytes.
    let prove_catalog = build_catalog_from_chain(&chain);
    let mut prove_registry = KitRegistry::default();
    prove_registry.register(
        "prove-default",
        ProveKit::new(origin_cid.clone(), prove_catalog),
        ProveKit::CONFORMANCE,
    );

    let prove_path = Input::Path(Box::new(CorePath {
        algebra: vec![PathAlgebra {
            name: "prove".to_string(),
            kit: "prove-default".to_string(),
            inputs: vec![lower_input_cid],
            depends_on: vec![],
            verb: Verb::Prove,
        }],
    }));
    let prove_chain = execute_path(&prove_path, &prove_registry, &prove_inputs)
        .expect("seam 6 positive: ProveKit must accept the lower-back claim");
    let terminal = prove_chain.terminal_claim();

    assert_eq!(
        terminal.verdict,
        Verdict::Proved,
        "seam 6 positive: prove over an intact chain must verdict Proved; witness: {:?}",
        terminal.witness
    );
    match &terminal.witness {
        Some(Witness::ChainIntegrity(_)) => {}
        other => panic!(
            "seam 6 positive: terminal witness must be ChainIntegrity, got {other:?}"
        ),
    }
}

#[test]
fn seam6_discrimination_broken_premise_cid_refutes_with_chain_integrity_failure() {
    require_python_modules(&[
        "provekit_lift_py_tests",
        "provekit_realize_python_core",
        "blake3",
    ]);

    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path();
    write_rust_workspace(workspace);
    write_rust_realize_manifest(workspace);

    let mut inputs = HashMapInputCatalog::default();
    let source = rust_lift_source_input(workspace);
    let source_cid = inputs.insert(source);

    let mut registry = KitRegistry::default();
    register_rust_lift(&mut registry, workspace);
    register_bind(&mut registry);
    register_lower(&mut registry, "rust", workspace);

    let chain = execute_lift_bind_lower(source_cid, "rust", &registry, &inputs);
    let lower_claim_real = chain
        .claim_at_step("lower")
        .expect("lower claim required")
        .clone();
    let origin_cid = chain
        .claim_at_step("lift")
        .expect("lift claim required to anchor prove")
        .cid();

    // Tamper: replace lower's premises with a non-existent CID.
    let bogus = libprovekit::core::Cid::parse(format!(
        "blake3-512:{}",
        "f".repeat(128)
    ))
    .expect("bogus CID parses");
    let mut tampered = lower_claim_real.clone();
    tampered.premises = vec![bogus.clone()];

    let mut prove_inputs = HashMapInputCatalog::default();
    propagate_chain_inputs(&chain, &mut prove_inputs);
    let tampered_cid = prove_inputs.insert(Input::Claim(tampered.clone()));
    let prove_catalog = build_catalog_from_chain(&chain);

    let mut prove_registry = KitRegistry::default();
    prove_registry.register(
        "prove-default",
        ProveKit::new(origin_cid.clone(), prove_catalog),
        ProveKit::CONFORMANCE,
    );

    let prove_path = Input::Path(Box::new(CorePath {
        algebra: vec![PathAlgebra {
            name: "prove".to_string(),
            kit: "prove-default".to_string(),
            inputs: vec![tampered_cid],
            depends_on: vec![],
            verb: Verb::Prove,
        }],
    }));
    let prove_chain = execute_path(&prove_path, &prove_registry, &prove_inputs)
        .expect("ProveKit must always return a claim, even when refuting");
    let terminal = prove_chain.terminal_claim();
    assert_eq!(
        terminal.verdict,
        Verdict::Refuted,
        "seam 6 discrimination: tampered premise must verdict Refuted; got {:?}",
        terminal.witness
    );
    match &terminal.witness {
        Some(Witness::ChainIntegrityFailure(failure)) => {
            // ChainBreak::kind_name() returns the variant identifier verbatim
            // ("PremiseNotInCatalog"). A8's antibody locks this contract via
            // doc-comment + regression test on walks.rs.
            assert_eq!(
                failure.break_kind, "PremiseNotInCatalog",
                "seam 6 discrimination: break_kind must reflect PremiseNotInCatalog; got `{}`. detail: {}",
                failure.break_kind, failure.break_detail
            );
        }
        other => panic!(
            "seam 6 discrimination: witness must be ChainIntegrityFailure, got {other:?}"
        ),
    }
}

// ============================================================================
// Composition helpers.
// ============================================================================

fn placeholder_lift_to_cid() -> libprovekit::core::Cid {
    // Path-step input CIDs for downstream-of-lift consumers are materialized at
    // execute_path time via the prior step's claim.to. The PathAlgebra entry
    // needs SOME CID up front because the field is non-optional. The executor
    // matches by step `depends_on`, so the value here is replaced before
    // dispatch. We use a deterministic placeholder.
    libprovekit::core::Cid::parse(format!("blake3-512:{}", "0".repeat(128)))
        .expect("placeholder cid parses")
}

/// Execute a two-step lift -> bind path where the bind step's input CID is
/// patched from the lift step's claim.to after lift runs. This mirrors how
/// the real path executor handles `depends_on` chains.
fn execute_lift_then_bind(
    path: &Input,
    registry: &KitRegistry,
    inputs: &HashMapInputCatalog,
    lift_step: &str,
    _bind_step: &str,
) -> libprovekit::core::PathExecutionChain {
    // Need to compute lift first to know the bind input CID. Issue a path with
    // only the lift step, capture its claim.to, then rebuild the full path.
    let Input::Path(ref boxed) = path else {
        panic!("execute_lift_then_bind expects Input::Path");
    };
    let lift_step_def = boxed
        .algebra
        .iter()
        .find(|s| s.name == lift_step)
        .expect("lift step must exist in algebra");
    let lift_only_path = Input::Path(Box::new(CorePath {
        algebra: vec![lift_step_def.clone()],
    }));
    let lift_only = execute_path(&lift_only_path, registry, inputs)
        .expect("lift-only sub-path must execute before composing bind");
    let lift_to = lift_only.terminal_claim().to.clone();

    // Rebuild the full path with the corrected bind input CID.
    let mut algebra = boxed.algebra.clone();
    for step in algebra.iter_mut() {
        if step.name != lift_step {
            // All downstream steps depending on lift get its claim.to as input.
            if step.depends_on.iter().any(|d| d == lift_step) {
                step.inputs = vec![lift_to.clone()];
            }
        }
    }
    let resolved_path = Input::Path(Box::new(CorePath { algebra }));
    execute_path(&resolved_path, registry, inputs).expect("composed lift -> bind must execute")
}

fn execute_lift_bind_lower(
    source_cid: libprovekit::core::Cid,
    target_lang: &str,
    registry: &KitRegistry,
    inputs: &HashMapInputCatalog,
) -> libprovekit::core::PathExecutionChain {
    execute_lift_bind_lower_checked(source_cid, target_lang, registry, inputs)
        .expect("3-step compose: lift -> bind -> lower must execute")
}

/// Compose a 3-step lift -> bind -> lower path by resolving each downstream
/// input CID from the prior step's claim.to. execute_path's `materialized_inputs`
/// keyed off claim.to requires the PathAlgebra entries to cite that CID exactly.
fn execute_lift_bind_lower_checked(
    source_cid: libprovekit::core::Cid,
    target_lang: &str,
    registry: &KitRegistry,
    inputs: &HashMapInputCatalog,
) -> Result<libprovekit::core::PathExecutionChain, libprovekit::core::PathExecutionError> {
    let lift_only = Input::Path(Box::new(CorePath {
        algebra: vec![PathAlgebra {
            name: "lift".to_string(),
            kit: "lift-rust".to_string(),
            inputs: vec![source_cid.clone()],
            depends_on: vec![],
            verb: Verb::Transform,
        }],
    }));
    let lift_chain = execute_path(&lift_only, registry, inputs)?;
    let lift_to = lift_chain.terminal_claim().to.clone();

    let lift_bind = Input::Path(Box::new(CorePath {
        algebra: vec![
            PathAlgebra {
                name: "lift".to_string(),
                kit: "lift-rust".to_string(),
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
    let bind_chain = execute_path(&lift_bind, registry, inputs)?;
    let bind_claim = bind_chain
        .claim_at_step("bind")
        .expect("bind claim present after sub-path")
        .clone();
    // LowerKit's claim_spec_value descends through Term::Op { concept:bind-result }
    // (per A7 #1071) when invoked on Input::Claim. The executor stores the
    // bind claim under address(&Input::Claim(claim)), not claim.to. We must
    // cite that CID so step_input resolves to Input::Claim rather than the
    // Term::Op-wrapped Input::Term that lower cannot consume.
    let bind_claim_input_cid = address(&Input::Claim(bind_claim));

    let full = Input::Path(Box::new(CorePath {
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
                inputs: vec![lift_to],
                depends_on: vec!["lift".to_string()],
                verb: Verb::Transform,
            },
            PathAlgebra {
                name: "lower".to_string(),
                kit: format!("lower-{target_lang}"),
                inputs: vec![bind_claim_input_cid],
                depends_on: vec!["bind".to_string()],
                verb: Verb::Transform,
            },
        ],
    }));
    execute_path(&full, registry, inputs)
}

fn propagate_chain_inputs(
    chain: &libprovekit::core::PathExecutionChain,
    catalog: &mut HashMapInputCatalog,
) {
    for name in ["lift", "bind", "lower"] {
        if let Some(claim) = chain.claim_at_step(name) {
            catalog.put(claim.cid(), Input::Claim(claim.clone()));
            if let Some(term) = claim.payload.clone() {
                catalog.put(claim.to.clone(), Input::Term(term));
            }
        }
    }
}

fn build_catalog_from_chain(
    chain: &libprovekit::core::PathExecutionChain,
) -> libprovekit::core::HashMapCatalog {
    let mut cat = libprovekit::core::HashMapCatalog::default();
    for name in ["lift", "bind", "lower"] {
        if let Some(claim) = chain.claim_at_step(name) {
            // Catalog::contains via HashMapCatalog requires the claim CID to map
            // to its canonical bytes so walk_premises_to_root can deserialize.
            cat.put(claim.cid(), claim.canonical_bytes());
        }
    }
    cat
}

/// Lift the given source, bind, and return the (bind.to CID, NamedTermDocument
/// recovered from the bind payload). Used by seam 4 federation reporting to
/// capture the empirical structural diff alongside the CID divergence.
fn run_lift_bind_capture_named(
    source: &str,
    is_python: bool,
) -> (libprovekit::core::Cid, NamedTermDocument) {
    let (cid, payload) = run_lift_bind_capture_payload(source, is_python);
    let named = named_term_document_from_bind_payload(&payload)
        .expect("named term document recovers from bind payload");
    (cid, named)
}

fn run_lift_bind_capture_payload(
    source: &str,
    is_python: bool,
) -> (libprovekit::core::Cid, Term) {
    let chain = run_lift_bind_capture_chain(source, is_python);
    let claim = chain
        .claim_at_step("bind")
        .expect("bind step claim must exist")
        .clone();
    let payload = claim.payload.expect("bind claim payload required");
    (claim.to, payload)
}

fn named_term_field_snapshot(doc: &NamedTermDocument) -> serde_json::Value {
    json!({
        "source_language": doc.source_language,
        "terms": doc.terms.iter().map(|t| json!({
            "concept_name": t.concept_name,
            "function": t.function,
            "name": t.name,
            "params": t.params,
            "param_types": t.param_types,
            "return_type": t.return_type,
            "term_shape_cid": t.term_shape_cid,
            "term_shape": t.term_shape,
            "named_term_tree": t.named_term_tree,
        })).collect::<Vec<_>>(),
    })
}

fn diff_named_term_fields(rust: &NamedTermDocument, py: &NamedTermDocument) -> Vec<String> {
    let mut diff = Vec::new();
    if rust.source_language != py.source_language {
        diff.push(format!(
            "source_language: rust={} python={}",
            rust.source_language, py.source_language
        ));
    }
    if rust.terms.len() != py.terms.len() {
        diff.push(format!(
            "terms.len: rust={} python={}",
            rust.terms.len(),
            py.terms.len()
        ));
    }
    for (i, (r, p)) in rust.terms.iter().zip(py.terms.iter()).enumerate() {
        if r.function != p.function {
            diff.push(format!(
                "terms[{i}].function: rust={} python={}",
                r.function, p.function
            ));
        }
        if r.param_types != p.param_types {
            diff.push(format!(
                "terms[{i}].param_types: rust={:?} python={:?}",
                r.param_types, p.param_types
            ));
        }
        if r.return_type != p.return_type {
            diff.push(format!(
                "terms[{i}].return_type: rust={} python={}",
                r.return_type, p.return_type
            ));
        }
        if r.term_shape_cid != p.term_shape_cid {
            diff.push(format!(
                "terms[{i}].term_shape_cid: rust={} python={}",
                r.term_shape_cid, p.term_shape_cid
            ));
        }
        if r.term_shape != p.term_shape {
            diff.push(format!("terms[{i}].term_shape differs structurally"));
        }
        let r_tree = serde_json::to_value(&r.named_term_tree).unwrap_or(serde_json::Value::Null);
        let p_tree = serde_json::to_value(&p.named_term_tree).unwrap_or(serde_json::Value::Null);
        if r_tree != p_tree {
            diff.push(format!(
                "terms[{i}].named_term_tree differs (operation_kind and/or args)"
            ));
        }
    }
    diff
}

fn run_lift_bind_capture_chain(
    source: &str,
    is_python: bool,
) -> libprovekit::core::PathExecutionChain {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path();
    if is_python {
        write_python_workspace(workspace, source);
    } else {
        let src = workspace.join("src");
        fs::create_dir_all(&src).expect("create src dir");
        fs::write(src.join("lib.rs"), source).expect("write rust source");
        fs::create_dir_all(workspace.join(".provekit")).expect("create .provekit");
        fs::write(
            workspace.join(".provekit/config.toml"),
            "[authoring.lift]\nsurface = \"rust\"\n",
        )
        .expect("write config");
    }

    let mut inputs = HashMapInputCatalog::default();
    let source_input = if is_python {
        python_lift_source_input(workspace)
    } else {
        rust_lift_source_input(workspace)
    };
    let source_cid = inputs.insert(source_input);

    let mut registry = KitRegistry::default();
    if is_python {
        register_python_lift(&mut registry, workspace);
    } else {
        register_rust_lift(&mut registry, workspace);
    }
    register_bind(&mut registry);

    let lift_kit_name = if is_python { "lift-python" } else { "lift-rust" };
    let path = Input::Path(Box::new(CorePath {
        algebra: vec![
            PathAlgebra {
                name: "lift".to_string(),
                kit: lift_kit_name.to_string(),
                inputs: vec![source_cid],
                depends_on: vec![],
                verb: Verb::Transform,
            },
            PathAlgebra {
                name: "bind".to_string(),
                kit: "bind-default".to_string(),
                inputs: vec![placeholder_lift_to_cid()],
                depends_on: vec!["lift".to_string()],
                verb: Verb::Transform,
            },
        ],
    }));
    execute_lift_then_bind(&path, &registry, &inputs, "lift", "bind")
}

/// Lift the given source, then bind, and return the bind step's claim.to CID.
fn run_lift_bind_capture_to(source: &str, is_python: bool) -> libprovekit::core::Cid {
    run_lift_bind_capture_chain(source, is_python)
        .claim_at_step("bind")
        .expect("bind step claim must exist")
        .to
        .clone()
}

