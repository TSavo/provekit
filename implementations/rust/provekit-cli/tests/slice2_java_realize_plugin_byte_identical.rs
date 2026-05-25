// SPDX-License-Identifier: Apache-2.0
//
// Slice 2 of #760 scoped Java realization parity:
//   Java RPC plugin (provekit.plugin.invoke) MUST produce byte-identical
//   Java source to Rust's realize_for_bind("java", ...) for all 11 trinity
//   fixture functions.
//
// This is not the trinity Branch 1 byte-identity receipt. Current trinity
// status is v0 loudly-bounded-lossy, fresh-target hermetic, Branch 2 mode.
// See docs/incidents/2026-05-14-trinity-baseline-diagnosis.md and closure PRs
// #860, #861, #862, #863. The trinity gate is the PR #861 hermetic fixture plus
// the retired v0 lossy trinity expectations; Branch 1 still needs Java and
// Python lift fixture wiring.
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
// Constraint: does NOT remove cmd_transport.rs Java match arms.

use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

/// Path to the Java realize jar (built by Maven).
fn java_jar() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Relative to provekit-cli crate: ../../java/provekit-realize-java-core/target/...
    manifest.join("../../java/provekit-realize-java-core/target/provekit-realize-java.jar")
}

fn exe_name(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

fn java_bin() -> String {
    if let Some(java_home) = std::env::var_os("JAVA_HOME") {
        let candidate = PathBuf::from(java_home).join("bin").join(exe_name("java"));
        if candidate.exists() {
            return candidate.display().to_string();
        }
    }

    if let Ok(output) = Command::new("mvn").arg("-version").output() {
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        for line in combined.lines() {
            if let Some((_, runtime)) = line.split_once("runtime: ") {
                let candidate = PathBuf::from(runtime.trim())
                    .join("bin")
                    .join(exe_name("java"));
                if candidate.exists() {
                    return candidate.display().to_string();
                }
            }
        }
    }

    "java".to_string()
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

    let java_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../java");

    let status = Command::new("mvn")
        .args([
            "package",
            "-pl",
            "provekit-realize-java-core",
            "-am",
            "-DskipTests",
        ])
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

// NOTE (unrot-conformance-gate): the 12 `trinity_*_byte_identical` tests
// were removed here. They asserted that `realize_for_bind("java", ...)`
// (with NO library_tag) produced byte-identical source to a direct
// `provekit.plugin.invoke` against the jar. That only worked while a
// single Java realize plugin was registered and auto-selected. PRs #1357 /
// #1365 added six more Java realize plugins (gson, jackson, blake3, stdio,
// rfc8785-jcs, sqlite-jdbc), so the dispatcher now correctly REFUSES to
// auto-select among ambiguous candidates without an explicit library_tag
// (kit_dispatch::resolve_realize_command). The tests' own comment already
// described them as a "tautological check that the dispatcher routes the
// same bytes the kit returns"; moreover `dispatch_realize` now injects
// `target_library_tag` into the request envelope, so the byte-equality the
// assertion relied on is no longer structurally guaranteed. Per the gate's
// dead-feature rule they are deleted, not patched. Live Java realize plugin
// coverage is retained by `pep_describe_returns_valid_sugar_plugin` below.

/// §4: Load the Java plugin via PEP 1.7.0 provekit.plugin.describe.
/// The CID in the response must match the pinned java-canonical.json CID.
#[test]
fn pep_describe_returns_valid_sugar_plugin() {
    use provekit_plugin_loader::load_plugin_from_rpc;
    ensure_jar_built();
    let jar = java_jar();
    let endpoint = format!("stdio:{} -jar {} --rpc", java_bin(), jar.display());
    let plugin = load_plugin_from_rpc(&endpoint)
        .unwrap_or_else(|e| panic!("load_plugin_from_rpc failed: {e}"));

    assert_eq!(plugin.kind(), "sugar", "plugin kind must be 'sugar'");
    assert_eq!(
        plugin.cid(),
        "blake3-512:b7ad1160f00d892d310fb33ac3372a4ebb2f89fec563cab1719e7006ab3d7593aae2162b882aedbec1b97e44957240b3c7e8ab1675456f0539c4ad3f45d22a7b",
        "java-canonical plugin CID must match pinned value"
    );
    assert!(
        !plugin.is_critical(),
        "java-canonical sugar is not critical"
    );
    assert!(
        plugin
            .header
            .protocol_versions
            .contains(&"pep/1.7.0".to_string()),
        "java-canonical must declare pep/1.7.0"
    );
}
