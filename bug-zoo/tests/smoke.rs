// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::process::Command;

use provekit_bug_zoo::{run, OutputFlags, ZooArgs};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn runner_help_is_self_contained() {
    let output = Command::new(env!("CARGO_BIN_EXE_provekit-bug-zoo"))
        .arg("--help")
        .output()
        .expect("spawn provekit-bug-zoo --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit-bug-zoo --help failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("provekit-bug-zoo"));
    assert!(stdout.contains("--all"));
    assert!(!stdout.contains("provekit zoo"));
}

#[test]
fn all_specimens_pass() {
    let root = repo_root();
    let code = run(ZooArgs {
        specimen: Some(root.join("bug-zoo/species")),
        all: true,
        out: OutputFlags {
            quiet: true,
            json: false,
        },
    });
    assert_eq!(code, 0, "one or more bug zoo specimens failed");
}

#[test]
fn csharp_discover_cli_finds_null_boundary_with_language_lifter() {
    let root = repo_root();
    let project = root.join("implementations/csharp/Provekit.BugZoo/Provekit.BugZoo.csproj");
    let harness = root.join(
        "bug-zoo/species/BZ-SHAPE-007-csharp-null-boundary-equivalence/exposed/linq-where/harness",
    );

    let output = Command::new("dotnet")
        .arg("run")
        .arg("--project")
        .arg(project)
        .arg("--no-restore")
        .arg("--")
        .arg("discover")
        .arg("csharp-linq")
        .arg(harness)
        .current_dir(&root)
        .output()
        .expect("spawn csharp discover cli");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "csharp discover failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("\"kind\":\"bug-zoo-discovery\""));
    assert!(stdout.contains("\"surface\":\"csharp-linq\""));
    assert!(stdout.contains("\"lifter\":\"LinqLifter\""));
    assert!(stdout.contains("\"missingEdge\":\"maybe_null(name) => non_null(name)\""));
    assert!(stdout.contains("\"irEvidenceCid\":"));
}

#[test]
fn typescript_discover_cli_finds_null_boundary_with_language_lifter() {
    let root = repo_root();
    let discover = root.join(
        "bug-zoo/species/BZ-SHAPE-006-typescript-null-boundary-equivalence/tools/ts-boundary-discover.ts",
    );
    let harness = root.join(
        "bug-zoo/species/BZ-SHAPE-006-typescript-null-boundary-equivalence/exposed/zod/harness",
    );

    let output = Command::new("pnpm")
        .arg("exec")
        .arg("tsx")
        .arg(discover)
        .arg("zod")
        .arg(harness)
        .current_dir(&root)
        .output()
        .expect("spawn typescript discover cli");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "typescript discover failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("\"kind\":\"bug-zoo-discovery\""));
    assert!(stdout.contains("\"surface\":\"zod\""));
    assert!(stdout.contains("\"lifter\":\"liftPath\""));
    assert!(stdout.contains("\"missingEdge\":\"maybe_null(name) => non_null(name)\""));
    assert!(stdout.contains("\"irEvidenceCid\":"));
}
