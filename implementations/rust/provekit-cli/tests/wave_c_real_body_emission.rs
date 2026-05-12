// SPDX-License-Identifier: Apache-2.0
//
// Wave-C real-body emission test (closes the `bind-stub-body-emitted`
// transport gap for the trinity-roundtrip slice; PR #748).
//
// Verifies the load-bearing behavioural contract from
// `protocol/specs/2026-05-12-trinity-java-roundtrip-transport-gaps.md`:
//
//   - bind v0 was emitting Java `throw new UnsupportedOperationException`
//     stubs for every binding; this PR pushes a wave-c-lifted Term graph
//     through `realize_for_bind` so emitted bodies are REAL.
//
// The acceptance shape (Supra omnia rectum):
//
//   Axis 1 (parse / syntax validity):
//     emitted Java contains no `UnsupportedOperationException` stub markers
//     and DOES contain idiomatic Java statements (`return`, `if (...)`,
//     `for (... : ...)`, `throw new RuntimeException`).
//   Axis 2 (address-space): each function's emitted class is uniquely named
//     and carries a `// concept: <name>` annotation matching the Rust-side
//     fixture annotation (Gap 3 closure; previously `identity`/`bool-cell`
//     mis-collapsed onto `concept:pair`).
//   Axis 3 (concept-space): NOT verified here. The Java re-lifter is a
//     separate Maven project; concept-CID round-trip remains characterized
//     by the loudly-bounded-lossy verdict in the transport-gaps spec.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

fn trinity_fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("trinity_roundtrip")
}

fn run_bind_java(root: &std::path::Path, out: &std::path::Path) -> std::process::Output {
    Command::new(provekit_bin())
        .arg("bind")
        .arg("--root")
        .arg(root)
        .arg("--lang")
        .arg("rust")
        .arg("--output")
        .arg(out)
        .arg("--rewrite")
        .arg("canonical")
        .arg("--mode")
        .arg("monitor")
        .arg("--target-language")
        .arg("java")
        .arg("--quiet")
        .output()
        .expect("spawn provekit bind")
}

/// (function name, expected `// concept: <name>` annotation, must-contain
/// substrings inside the realized body).
///
/// These substrings are deliberately small and structural — they exercise
/// statements that are NEW to the realizer slice in this PR (real `return`,
/// real `if (...) { ... }`, real `throw new RuntimeException` for `panic!`,
/// real `for (var v : iter)` for slice iteration). The absence of the stub
/// marker (`UnsupportedOperationException`) is the falsifiable claim that
/// `bind-stub-body-emitted` is closed for this fixture.
const TRINITY_EXPECTATIONS: &[(&str, &str, &[&str])] = &[
    ("WrapIdentityTransported", "identity", &["return x;"]),
    ("DoNothingTransported", "unit", &["return;"]),
    ("ToggleTransported", "bool-cell", &["return !flag;"]),
    (
        "AssertPositiveTransported",
        "assert",
        &[
            "if (x <= 0L)",
            "throw new RuntimeException",
            "return x;",
        ],
    ),
    (
        "MaybeFirstTransported",
        "option",
        &["items.length == 0", "return -1L;", "items[(int) 0L]"],
    ),
    (
        "OptionBindDoubleTransported",
        "option-bind",
        &["items.length == 0", "return -1L;", "var v = items[(int) 0L];"],
    ),
    (
        "SafeDivideTransported",
        "result",
        &["if (denom == 0L)", "return num / denom;"],
    ),
    (
        "SafeDivideThenDoubleTransported",
        "result-bind",
        &["if (denom == 0L)", "var q = num / denom;"],
    ),
    (
        "SwapPairTransported",
        "pair",
        &["return new long[] { b, a };"],
    ),
    (
        "ListSumTransported",
        "list",
        &["for (var v : items)", "acc = acc + v", "return acc;"],
    ),
    (
        "ClassifyTransported",
        "tagged-union",
        &["if (x < 0L)", "return 0L;", "return 1L;", "return 2L;"],
    ),
    (
        "RetryUntilSuccessTransported",
        "retry-loop",
        &["while (attempt < max_attempts)", "return true;", "return false;"],
    ),
];

#[test]
fn wave_c_emits_real_bodies_for_trinity_concepts() {
    let fixture = trinity_fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let r = run_bind_java(&fixture, &out);
    assert!(
        r.status.success(),
        "bind must succeed; stderr:\n{}",
        String::from_utf8_lossy(&r.stderr)
    );

    let java = fs::read_to_string(out.join("translated").join("java").join("lib.java"))
        .expect("emitted lib.java must exist");

    // Axis 1: NO stub marker survives anywhere in the emitted source. This is
    // the precise inverse of the pre-wave-c claim and the only assertion that
    // directly falsifies the `bind-stub-body-emitted` gap for this fixture.
    assert!(
        !java.contains("UnsupportedOperationException"),
        "no class may emit the bind-canonical stub marker; emitted java =\n{java}"
    );
    assert!(
        !java.contains("provekit-bind canonical: "),
        "no class may carry the canonical-stub message; emitted java =\n{java}"
    );

    // Per-function structural checks.
    for (class, concept, snippets) in TRINITY_EXPECTATIONS {
        let needle = format!("class {class}");
        let class_start = java
            .find(&needle)
            .unwrap_or_else(|| panic!("missing class {class} in emitted lib.java\n{java}"));
        let class_chunk = &java[class_start..];
        let end = class_chunk
            .find("\nfinal class ")
            .or_else(|| class_chunk.find("\n// substrate-origin:"))
            .or_else(|| class_chunk.find("\n// canonical rewrite:"))
            .unwrap_or(class_chunk.len());
        let class_chunk = &class_chunk[..end];

        let concept_line = format!("// concept: {concept}");
        assert!(
            class_chunk.contains(&concept_line),
            "class {class} must carry '{concept_line}'; got chunk:\n{class_chunk}"
        );

        for snip in *snippets {
            assert!(
                class_chunk.contains(snip),
                "class {class} body must contain `{snip}`; got chunk:\n{class_chunk}"
            );
        }
    }

    // Gaps.json must record `bind-real-body-emitted` and NOT
    // `bind-stub-body-emitted` for this fixture (all 12 bindings are in
    // the wave-c slice).
    let gaps_raw =
        fs::read_to_string(out.join("gaps.json")).expect("gaps.json must exist after bind");
    let gaps: serde_json::Value = serde_json::from_str(&gaps_raw).expect("gaps.json must be JSON");
    let kinds: Vec<&str> = gaps["gaps"]
        .as_array()
        .map(|a| a.iter().filter_map(|g| g["kind"].as_str()).collect())
        .unwrap_or_default();
    assert!(
        kinds.contains(&"bind-real-body-emitted"),
        "gaps.json must record bind-real-body-emitted; kinds={kinds:?}"
    );
    assert!(
        !kinds.contains(&"bind-stub-body-emitted"),
        "gaps.json MUST NOT record bind-stub-body-emitted for trinity fixture \
         (all 12 bindings fit the wave-c slice); kinds={kinds:?}"
    );
}

/// Optional javac axis-1 check: when `javac` is on PATH the emitted lib.java
/// must compile cleanly. We don't bake the JDK into the test environment.
#[test]
fn wave_c_emitted_java_compiles_when_javac_available() {
    if Command::new("javac").arg("--version").output().is_err() {
        eprintln!("javac not on PATH; skipping axis-1 javac check");
        return;
    }

    let fixture = trinity_fixture_root();
    let out = tempfile::tempdir().expect("tempdir").into_path();
    let r = run_bind_java(&fixture, &out);
    assert!(
        r.status.success(),
        "bind must succeed; stderr:\n{}",
        String::from_utf8_lossy(&r.stderr)
    );

    let java_dir = out.join("translated").join("java");
    let lib = java_dir.join("lib.java");
    assert!(lib.exists(), "lib.java missing after bind");

    let javac_out = Command::new("javac")
        .arg(&lib)
        .output()
        .expect("spawn javac");
    if !javac_out.status.success() {
        let src = fs::read_to_string(&lib).unwrap_or_default();
        panic!(
            "emitted lib.java did NOT compile cleanly under javac.\n\
             status={:?}\n\
             stderr=\n{}\n\
             source=\n{src}",
            javac_out.status,
            String::from_utf8_lossy(&javac_out.stderr)
        );
    }
}
