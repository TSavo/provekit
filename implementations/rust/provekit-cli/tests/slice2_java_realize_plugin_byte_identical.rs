// SPDX-License-Identifier: Apache-2.0
//
// Slice 2 of #760 — Trinity byte-identical assertion:
//   Java RPC plugin (provekit.plugin.invoke) MUST produce byte-identical
//   Java source to Rust's realize_for_bind("java", ...) for all 11 trinity
//   fixture functions.
//
// The bind path uses Term::Unit (no term graph yet), so every function
// becomes a compilable stub: a final class wrapping a static method that
// throws UnsupportedOperationException.
//
// Test structure per function:
//   1. Call realize_for_bind("java", ...) → rust_src
//   2. Send provekit.plugin.invoke to the Java RPC server → java_src
//   3. Assert rust_src == java_src (byte-identical)
//
// §6.2 (PEP 1.7.0): CID is delivery-independent; the content hash of the
// stub source MUST be identical whether produced by the Rust path or the
// Java plugin path.
//
// Constraint: does NOT modify trinity_roundtrip_test.rs or its fixtures.
// Does NOT remove cmd_transport.rs Java match arms.

use std::io::{BufRead, BufReader, Write as IoWrite};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

use provekit_cli::cmd_transport::realize_for_bind;

/// Trinity fixture source file.
fn trinity_src() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/trinity_roundtrip/src/lib.rs");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("could not read trinity fixture: {e}"))
}

/// Path to the Java realize jar (built by Maven).
fn java_jar() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Relative to provekit-cli crate: ../../java/provekit-realize-java-core/target/...
    manifest
        .join("../../java/provekit-realize-java-core/target/provekit-realize-java.jar")
}

/// Serializes the one-time Maven build across parallel test threads.
/// The `OnceLock` holds a `Mutex<()>` so that the first thread to arrive
/// grabs the lock, runs `mvn package`, and releases it. Every subsequent
/// thread acquires and immediately releases (jar already present).
static JAR_BUILD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Build the Java realize jar via Maven if it is not already present.
/// Thread-safe: concurrent callers block until the single build finishes.
/// Panics only if the jar is still missing after the build attempt.
fn ensure_jar_built() {
    let mtx = JAR_BUILD_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = mtx.lock().unwrap_or_else(|p| p.into_inner());

    let jar = java_jar();
    if jar.exists() {
        return;
    }

    let java_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../java");

    let status = Command::new("mvn")
        .args(["package", "-pl", "provekit-realize-java-core", "-am", "-DskipTests"])
        .current_dir(&java_dir)
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn mvn: {e}"));

    if !status.success() {
        panic!(
            "mvn package failed (exit {status}); cannot build {jar}",
            jar = jar.display()
        );
    }

    if !jar.exists() {
        panic!(
            "java jar still not found at {} after mvn package",
            jar.display()
        );
    }
}

/// Send one provekit.plugin.invoke RPC to the Java plugin and return the source string.
fn java_invoke(function: &str, params: &[&str], param_types: &[&str], return_type: &str, concept_name: &str) -> String {
    ensure_jar_built();
    let jar = java_jar();

    let params_json: String = params
        .iter()
        .map(|s| format!("{:?}", s))
        .collect::<Vec<_>>()
        .join(",");
    let param_types_json: String = param_types
        .iter()
        .map(|s| format!("{:?}", s))
        .collect::<Vec<_>>()
        .join(",");

    let request = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"provekit.plugin.invoke","params":{{"function":{fn_q},"params":[{params_json}],"param_types":[{param_types_json}],"return_type":{ret_q},"concept_name":{concept_q}}}}}"#,
        fn_q = format!("{:?}", function),
        params_json = params_json,
        param_types_json = param_types_json,
        ret_q = format!("{:?}", return_type),
        concept_q = format!("{:?}", concept_name),
    );

    let mut child = Command::new("java")
        .args(["-jar", jar.to_str().unwrap(), "--rpc"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn java jar");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        stdin.write_all(request.as_bytes()).expect("write request");
        stdin.write_all(b"\n").expect("write newline");
    }

    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).expect("read response");

    let _ = child.kill();
    let _ = child.wait();

    // Parse: {"jsonrpc":"2.0","id":1,"result":{"source":"..."}}
    let v: serde_json::Value = serde_json::from_str(line.trim())
        .unwrap_or_else(|e| panic!("java response not valid JSON: {e}\nraw: {line}"));
    if let Some(err) = v.get("error") {
        panic!("java plugin invoke returned error: {err}");
    }
    v["result"]["source"]
        .as_str()
        .unwrap_or_else(|| panic!("missing result.source in java response\nraw: {line}"))
        .to_string()
}

/// Run one byte-identical assertion: cmd_transport's realize_for_bind (which
/// dispatches through the Java realize plugin under PR #770) vs the same
/// plugin invoked directly. Post-PR-#770 this is a tautological check that
/// the dispatcher routes the same bytes the kit returns; it is retained as
/// a regression test for the dispatcher contract.
fn assert_byte_identical(
    function: &str,
    params: &[&str],
    _source_text: &str,
    concept_name: &str,
) {
    // Post PR #779: the realize path goes through the kit dispatcher, which
    // resolves the Java realize jar via the substrate-convention filesystem
    // discovery (implementations/java/provekit-realize-java-core/target/...).
    // The dispatcher cleanly refuses with kit-plugin-unavailable when the jar
    // is missing. Build it via Maven before the test exercises the path so the
    // byte-identical assertion has a real comparand. Same one-shot build the
    // pep_describe test already uses; ensure_jar_built serializes concurrent
    // threads via a OnceLock<Mutex>.
    ensure_jar_built();
    let param_strings: Vec<String> = params.iter().map(|s| s.to_string()).collect();
    // Bind-time signature info is supplied by the lift kit per
    // `2026-05-13-bind-ir-lift-result.md`. The slice 2 fixture used to
    // re-derive these via `parse_rust_fn_types`; that helper was retired
    // in PR #770. The fixture deliberately uses uniform `long` params so
    // the test still exercises the byte-identical realize path.
    let param_types: Vec<String> = params.iter().map(|_| "i64".to_string()).collect();
    let return_type = "i64";

    let rust_result = realize_for_bind(
        "java",
        function,
        &param_strings,
        &param_types,
        return_type,
        concept_name,
    )
    .unwrap_or_else(|e| panic!("realize_for_bind failed for {function}: {e}"));
    let rust_src = &rust_result.source;

    let param_type_strs: Vec<&str> = param_types.iter().map(String::as_str).collect();
    let java_src = java_invoke(function, params, &param_type_strs, return_type, concept_name);

    assert_eq!(
        rust_src,
        &java_src,
        "byte-identical failure for {function}\n\
         Rust output:\n{rust_src}\n\
         Java output:\n{java_src}"
    );
}

// Use a single concept name for all stub functions (bind path with Term::Unit).
const CONCEPT: &str = "UNNAMED-CONCEPT-a777b12569a16b07";

#[test]
fn trinity_wrap_identity_byte_identical() {
    let src = trinity_src();
    assert_byte_identical("wrap_identity", &["x"], &src, CONCEPT);
}

#[test]
fn trinity_do_nothing_byte_identical() {
    let src = trinity_src();
    assert_byte_identical("do_nothing", &[], &src, CONCEPT);
}

#[test]
fn trinity_toggle_byte_identical() {
    let src = trinity_src();
    assert_byte_identical("toggle", &["flag"], &src, CONCEPT);
}

#[test]
fn trinity_assert_positive_byte_identical() {
    let src = trinity_src();
    assert_byte_identical("assert_positive", &["x"], &src, CONCEPT);
}

#[test]
fn trinity_maybe_first_byte_identical() {
    let src = trinity_src();
    assert_byte_identical("maybe_first", &["items"], &src, CONCEPT);
}

#[test]
fn trinity_option_bind_double_byte_identical() {
    let src = trinity_src();
    assert_byte_identical("option_bind_double", &["items"], &src, CONCEPT);
}

#[test]
fn trinity_safe_divide_byte_identical() {
    let src = trinity_src();
    assert_byte_identical("safe_divide", &["num", "denom"], &src, CONCEPT);
}

#[test]
fn trinity_safe_divide_then_double_byte_identical() {
    let src = trinity_src();
    assert_byte_identical("safe_divide_then_double", &["num", "denom"], &src, CONCEPT);
}

#[test]
fn trinity_swap_pair_byte_identical() {
    let src = trinity_src();
    assert_byte_identical("swap_pair", &["a", "b"], &src, CONCEPT);
}

#[test]
fn trinity_list_sum_byte_identical() {
    let src = trinity_src();
    assert_byte_identical("list_sum", &["items"], &src, CONCEPT);
}

#[test]
fn trinity_classify_byte_identical() {
    let src = trinity_src();
    assert_byte_identical("classify", &["x"], &src, CONCEPT);
}

#[test]
fn trinity_retry_until_success_byte_identical() {
    let src = trinity_src();
    // retry_until_success has #[requires] and #[ensures] annotations.
    // realize_for_bind lifts these from the source text; the Java plugin
    // does not receive annotation contracts in the invoke params (slice 2
    // scope: stub path only). This test verifies that the stub body and
    // signature are byte-identical; annotation divergence is documented
    // as an in-scope gap for slice 3.
    assert_byte_identical("retry_until_success", &["max_attempts"], &src, CONCEPT);
}

/// §4: Load the Java plugin via PEP 1.7.0 provekit.plugin.describe.
/// The CID in the response must match the pinned java-canonical.json CID.
#[test]
fn pep_describe_returns_valid_sugar_plugin() {
    use provekit_plugin_loader::load_plugin_from_rpc;
    ensure_jar_built();
    let jar = java_jar();
    let endpoint = format!("stdio:java -jar {} --rpc", jar.display());
    let plugin = load_plugin_from_rpc(&endpoint)
        .unwrap_or_else(|e| panic!("load_plugin_from_rpc failed: {e}"));

    assert_eq!(plugin.kind(), "sugar", "plugin kind must be 'sugar'");
    assert_eq!(
        plugin.cid(),
        "blake3-512:b7ad1160f00d892d310fb33ac3372a4ebb2f89fec563cab1719e7006ab3d7593aae2162b882aedbec1b97e44957240b3c7e8ab1675456f0539c4ad3f45d22a7b",
        "java-canonical plugin CID must match pinned value"
    );
    assert!(!plugin.is_critical(), "java-canonical sugar is not critical");
    assert!(
        plugin.header.protocol_versions.contains(&"pep/1.7.0".to_string()),
        "java-canonical must declare pep/1.7.0"
    );
}
