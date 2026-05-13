// SPDX-License-Identifier: Apache-2.0
//
// D1 (2026-05-13): Body-template memento + Java emitter rip-out.
//
// Asserts that `provekit bind --target-language=java --rewrite=canonical` emits
// REAL function bodies for the three concepts covered by
// `menagerie/java-language-signature/specs/body-templates/java-canonical-bodies.json`
// v1.0.0 (concept_name: "identity", "bool-cell", "unit"), and stubs for the
// other 9 concepts whose body templates are not yet authored.
//
// This is the empirical "java in rust is a lie" guard: Java emission flows
// through the provekit-realize-java-core plugin via PEP 1.7.0
// `provekit.plugin.invoke`. cmd_transport.rs holds no body-template content
// for Java; templates and rendering live entirely in SugarRealizer.java.

use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

/// Path to the Java realize jar (built by Maven).
fn java_jar() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("../../java/provekit-realize-java-core/target/provekit-realize-java.jar")
}

/// Serialises the one-time Maven build across parallel test threads.
static JAR_BUILD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn ensure_jar_built() {
    let mtx = JAR_BUILD_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = mtx.lock().unwrap_or_else(|p| p.into_inner());

    let jar = java_jar();
    if jar.exists() {
        return;
    }

    let java_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../java");
    let status = Command::new("mvn")
        .args(["package", "-pl", "provekit-realize-java-core", "-am", "-DskipTests"])
        .current_dir(&java_dir)
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn mvn: {e}"));
    assert!(status.success(), "mvn package failed");
    assert!(jar.exists(), "jar still missing after mvn package");
}

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/trinity_roundtrip")
}

#[test]
fn d1_java_body_templates_emit_real_bodies_for_templated_concepts() {
    ensure_jar_built();

    let out = tempfile::tempdir().expect("tempdir").keep();
    let status = Command::new(provekit_bin())
        .arg("bind")
        .arg("--root")
        .arg(fixture_root())
        .arg("--lang")
        .arg("rust")
        .arg("--target-language")
        .arg("java")
        .arg("--rewrite")
        .arg("canonical")
        .arg("--output")
        .arg(&out)
        .status()
        .expect("spawn provekit bind");
    assert!(status.success(), "bind must succeed");

    let java_file = out.join("translated").join("java").join("lib.java");
    assert!(java_file.exists(), "translated/java/lib.java must exist");
    let java_src = std::fs::read_to_string(&java_file).expect("read lib.java");

    // identity: `return x;` (the parameter name from wrap_identity)
    assert!(
        java_src.contains("public static long wrap_identity(long x) {")
            && java_src.contains("        return x;"),
        "concept:identity must emit `return x;` body via body-template plugin; got:\n{java_src}"
    );

    // bool-cell: `return !flag;`
    assert!(
        java_src.contains("public static boolean toggle(boolean flag) {")
            && java_src.contains("        return !flag;"),
        "concept:bool-cell must emit `return !flag;` body; got:\n{java_src}"
    );

    // unit: `return;` (void)
    assert!(
        java_src.contains("public static void do_nothing() {")
            && java_src.contains("        return;"),
        "concept:unit must emit `return;` body; got:\n{java_src}"
    );

    // Untemplated concepts (assert, option, option-bind, result, result-bind,
    // pair, list, tagged-union, retry-loop) should still fall through to the
    // language stub. Spot-check one:
    assert!(
        java_src.contains("throw new UnsupportedOperationException(\"provekit-bind canonical: assert\");"),
        "concept:assert is not templated yet; must fall through to stub; got:\n{java_src}"
    );
}
