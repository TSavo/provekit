// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::process::Command;

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

#[test]
fn bug_zoo_machinery_is_self_contained() {
    let root = repo_root();
    assert!(
        root.join("bug-zoo/Cargo.toml").exists(),
        "Bug Zoo should own its runnable harness under bug-zoo/"
    );
    assert!(
        !root
            .join("implementations/rust/provekit-cli/src/cmd_zoo.rs")
            .exists(),
        "Bug Zoo should not be embedded as a provekit CLI command"
    );
    assert!(
        !root
            .join("implementations/rust/provekit-cli/tests/support/bug_zoo.rs")
            .exists(),
        "Bug Zoo harness code should live under bug-zoo/, not provekit-cli tests"
    );
}

#[test]
fn provekit_cli_does_not_expose_zoo_subcommand() {
    let output = Command::new(provekit_bin())
        .arg("--help")
        .output()
        .expect("spawn provekit --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit --help failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("zoo"),
        "`provekit zoo` must remain a repo harness, not a public CLI subcommand\nstdout:\n{stdout}"
    );
}
