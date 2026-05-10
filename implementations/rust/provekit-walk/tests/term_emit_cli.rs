// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn term_emit_cli_skips_unrepresentable_macro_without_writing_term_json() {
    let dir = unique_temp_dir();
    fs::create_dir_all(&dir).expect("create temp dir");
    let source_path = dir.join("bad.rs");
    let output_path = dir.join("bad.term.json");
    fs::write(
        &source_path,
        r#"
            fn bad() {
                println!("not representable");
            }
        "#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_provekit-walk-emit"))
        .arg("term")
        .arg(&source_path)
        .arg("bad")
        .arg(&output_path)
        .output()
        .expect("run provekit-walk-emit");

    assert!(!output.status.success());
    assert!(
        !output_path.exists(),
        "unsupported term emission must not write output"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("term-emit skipped fn=bad:"),
        "unexpected stderr: {stderr}"
    );
    assert!(
        stderr.contains("unsupported statement Stmt::Macro"),
        "unexpected stderr: {stderr}"
    );

    let _ = fs::remove_dir_all(&dir);
}

fn unique_temp_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "provekit-walk-term-emit-{}-{nanos}",
        std::process::id()
    ))
}
