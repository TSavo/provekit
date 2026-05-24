// SPDX-License-Identifier: Apache-2.0
//
// TEMPLATE TEST for the `.proof`-load-via-RPC + kit-owned-assembly path.
//
// This is the CI coverage for the architecture the per-language fan-out copies:
//   1. Emission body templates come from the @ProveKitSugar shim's signed
//      `.proof`, lifted by `provekit materialize` and fed to the realize plugin
//      over RPC (RealizeRequest.body_templates) — NOT from the on-disk
//      `<lang>-canonical-bodies-<tag>.json` cache.
//   2. The compilation unit is assembled BY THE LANGUAGE KIT over the assemble
//      RPC (package, imports, helper hoisting, class wrapping). The substrate
//      bakes no language syntax; `--compile-check` runs the kit-declared
//      classpath.
//
// The proof that we are on the `.proof` path and not the disk cache: the two
// `java-canonical-bodies-{jackson,gson}.json` files are DELETED in this branch
// (see the PIECE-1 commit). If the realize kit were still disk-loading, both
// carrier sites would REFUSE and these tests would fail at the body asserts.
// `assert_no_canonical_bodies_on_disk` makes that invariant explicit.
//
// Fan-out: each new (language, library) shim gets an analogous test — same
// three assertions (correct library-specific body, `.proof`-not-disk, javac/
// equivalent compile 0). Copy `materialize_via_proof_and_assemble` and swap the
// library tag + expected body fragments + helper.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("provekit-cli has rust workspace parent")
        .parent()
        .expect("rust workspace has implementations parent")
        .parent()
        .expect("implementations dir has repo parent")
        .to_path_buf()
}

/// The shaded realize-java jar the java realize/assemble RPC plugin runs as.
/// The body-templates + assemble both flow through this process; without it
/// the path can't be exercised, so the test skips-loud (matching the existing
/// integration suite's per-toolchain gating, e.g. the python/rust examples).
fn java_realize_jar() -> PathBuf {
    repo_root()
        .join("implementations")
        .join("java")
        .join("provekit-realize-java-core")
        .join("target")
        .join("provekit-realize-java.jar")
}

fn javac_available() -> bool {
    Command::new("javac")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Invariant for the `.proof`-authority claim: the on-disk canonical-bodies
/// cache for jackson/gson must NOT exist. With it absent, the only place the
/// realize kit can get the jackson/gson bodies is the shim `.proof` over RPC —
/// so a passing body assertion below is itself proof of the RPC-authority path.
fn assert_no_canonical_bodies_on_disk() {
    let body_dir = repo_root()
        .join("menagerie")
        .join("java-language-signature")
        .join("specs")
        .join("body-templates");
    for tag in ["jackson", "gson"] {
        let cache = body_dir.join(format!("java-canonical-bodies-{tag}.json"));
        assert!(
            !cache.exists(),
            "RPC-authority path requires the disk cache to be deleted, but {} exists. \
             If it came back, the test would no longer prove templates load from the \
             shim .proof over RPC (it could be reading disk instead).",
            cache.display()
        );
    }
}

/// Run the json-shim demo client through the real CLI for one library, with
/// `--out-dir` + `--compile-check`, and return (stdout, stderr, the emitted
/// ConfigCodec.java contents). Asserts the run succeeded (javac 0).
fn materialize_via_proof_and_assemble(library: &str) -> (String, String, String) {
    let repo = repo_root();
    let out_dir = tempfile::tempdir().expect("out-dir tempdir");
    let source_dir = repo.join("examples").join("json-shim-demo-client").join("src");
    assert!(
        source_dir.join("ConfigCodec.java").is_file(),
        "demo client missing at {}",
        source_dir.display()
    );

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .env("PROVEKIT_REPO_ROOT", &repo)
        .arg("materialize")
        .arg("--library")
        .arg(library)
        .arg("--target")
        .arg("java")
        .arg("--source-dir")
        .arg(&source_dir)
        .arg("--project")
        .arg(&repo)
        .arg("--out-dir")
        .arg(out_dir.path())
        .arg("--compile-check")
        .output()
        .unwrap_or_else(|e| panic!("spawn provekit materialize --library {library}: {e}"));

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "materialize --library {library} --compile-check should succeed (javac 0)\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    // The java kit's assemble RPC emitted the unit and the substrate wrote it.
    assert!(
        stderr.contains("assembled by java kit via RPC"),
        "compilation unit must be assembled by the kit (not the substrate):\n{stderr}"
    );
    assert!(
        stderr.contains("compile-check: javac passed"),
        "--compile-check must report javac passed:\n{stderr}"
    );

    let emitted = std::fs::read_to_string(out_dir.path().join("ConfigCodec.java"))
        .expect("emitted ConfigCodec.java");
    (stdout, stderr, emitted)
}

#[test]
fn materialize_json_client_jackson_loads_from_proof_and_compiles() {
    if !java_realize_jar().exists() {
        eprintln!(
            "skipping jackson .proof-load test: {} is unavailable; build with \
             `mvn -q -f implementations/java/pom.xml -pl provekit-realize-java-core -am package -DskipTests`",
            java_realize_jar().display()
        );
        return;
    }
    if !javac_available() {
        eprintln!("skipping jackson .proof-load test: javac is unavailable on PATH");
        return;
    }
    // The disk cache is deleted; a green body assert below == RPC-authority path.
    assert_no_canonical_bodies_on_disk();

    let (_stdout, _stderr, emitted) = materialize_via_proof_and_assemble("jackson");

    // Library-specific body: Jackson's ObjectMapper.
    assert!(
        emitted.contains("MAPPER.readTree(s)"),
        "jackson json-parse must emit MAPPER.readTree:\n{emitted}"
    );
    assert!(
        emitted.contains("MAPPER.writeValueAsString(v)"),
        "jackson json-serialize must emit MAPPER.writeValueAsString:\n{emitted}"
    );
    // Kit-owned assembly: real imports + hoisted helper field (not a comment).
    assert!(
        emitted.contains("import com.fasterxml.jackson.databind.JsonNode;")
            && emitted.contains("import com.fasterxml.jackson.databind.ObjectMapper;"),
        "jackson must pull real com.fasterxml.jackson.* imports:\n{emitted}"
    );
    assert!(
        emitted.contains("static final ObjectMapper MAPPER = new ObjectMapper();"),
        "jackson must hoist the ObjectMapper helper field:\n{emitted}"
    );
    // Must be Jackson, not Gson.
    assert!(
        !emitted.contains("com.google.gson"),
        "jackson output must not reference gson:\n{emitted}"
    );
}

#[test]
fn materialize_json_client_gson_loads_from_proof_and_compiles() {
    if !java_realize_jar().exists() {
        eprintln!(
            "skipping gson .proof-load test: {} is unavailable; build with \
             `mvn -q -f implementations/java/pom.xml -pl provekit-realize-java-core -am package -DskipTests`",
            java_realize_jar().display()
        );
        return;
    }
    if !javac_available() {
        eprintln!("skipping gson .proof-load test: javac is unavailable on PATH");
        return;
    }
    assert_no_canonical_bodies_on_disk();

    let (_stdout, _stderr, emitted) = materialize_via_proof_and_assemble("gson");

    // Library-specific body: Gson's JsonParser / Gson instance.
    assert!(
        emitted.contains("JsonParser.parseString(s)"),
        "gson json-parse must emit JsonParser.parseString:\n{emitted}"
    );
    assert!(
        emitted.contains("GSON.toJson(v)"),
        "gson json-serialize must emit GSON.toJson:\n{emitted}"
    );
    // Kit-owned assembly: real imports + hoisted helper field.
    assert!(
        emitted.contains("import com.google.gson.Gson;")
            && emitted.contains("import com.google.gson.JsonParser;"),
        "gson must pull real com.google.gson.* imports:\n{emitted}"
    );
    assert!(
        emitted.contains("static final Gson GSON = new Gson();"),
        "gson must hoist the Gson helper field:\n{emitted}"
    );
    // Must be Gson, not Jackson.
    assert!(
        !emitted.contains("com.fasterxml.jackson"),
        "gson output must not reference jackson:\n{emitted}"
    );
}
