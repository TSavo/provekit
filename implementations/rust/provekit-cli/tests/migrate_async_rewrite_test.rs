// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use provekit_ir_types::MigrateReceiptEnvelope;

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

fn run_migrate(
    repo: &Path,
    source_dir: &Path,
    out_dir: &Path,
    receipt: &Path,
) -> std::process::Output {
    let target = tempfile::tempdir().expect("fresh target dir");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.current_dir(repo.join("implementations").join("rust"))
        .env("CARGO_TARGET_DIR", target.path())
        .arg("run")
        .arg("-p")
        .arg("provekit-cli")
        .arg("--")
        .arg("bind")
        .arg("--library-from")
        .arg("typescript-better-sqlite3")
        .arg("--library-to")
        .arg("typescript-pg")
        .arg("--source-dir")
        .arg(source_dir)
        .arg("--out-dir")
        .arg(out_dir)
        .arg("--receipt")
        .arg(receipt)
        .arg("--write");
    cmd.output().expect("spawn cargo run migrate bind")
}

#[test]
fn bind_migrates_better_sqlite3_to_pg_with_async_receipt() {
    let repo = repo_root();
    let source_dir = repo
        .join("examples")
        .join("migrate-demo")
        .join("users-better-sqlite3");
    let temp = tempfile::tempdir().expect("temp output");
    let out_dir = temp.path().join("users-pg");
    let receipt_path = temp.path().join("migrate-receipt.proof");

    let output = run_migrate(&repo, &source_dir, &out_dir, &receipt_path);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "migrate bind failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let expected = [
        "4 callsites rewritten",
        "6 functions widened to async",
        "1 boundary handlers already async-capable",
        "1 refused exports because public API forbids promise return",
        "1 lossy callsites: kit-declared dimension divergence",
    ];
    for line in expected {
        assert!(
            stdout.lines().any(|actual| actual == line),
            "missing stdout line {line:?}\nstdout:\n{stdout}"
        );
    }

    let receipt_raw = fs::read_to_string(&receipt_path).expect("receipt written");
    let receipt: MigrateReceiptEnvelope =
        serde_json::from_str(&receipt_raw).expect("parse migrate receipt");
    receipt.validate().expect("receipt validates");
    assert_eq!(receipt.aggregate_summary.rewritten, 4);
    assert_eq!(receipt.aggregate_summary.widened, 6);
    assert_eq!(receipt.aggregate_summary.halted, 1);
    assert_eq!(receipt.aggregate_summary.refused, 1);
    assert_eq!(receipt.aggregate_summary.lossy, 1);
    assert!(!receipt.concept_sites.is_empty(), "concept sites present");
    assert!(
        !receipt.promotion_decisions.is_empty(),
        "promotion decisions present"
    );
    assert!(!receipt.halt_mementos.is_empty(), "halt mementos present");
    assert!(
        !receipt.refusal_mementos.is_empty(),
        "refusal mementos present"
    );
    assert!(!receipt.loss_records.is_empty(), "loss records present");

    let migrated =
        fs::read_to_string(out_dir.join("src").join("users.ts")).expect("migrated users.ts");
    assert!(migrated.contains("import { Pool } from \"pg\";"));
    assert!(migrated.contains("export async function getAllUsers(): Promise<User[]>"));
    assert!(migrated.contains(
        "const result = await pool.query(\"SELECT id, name, email FROM users WHERE id = ?\", [id]);"
    ));
    assert!(migrated.contains("export function exportedFormatter(u: User): string"));
    assert!(
        out_dir.join("package.json").exists(),
        "package.json written"
    );
    assert!(
        out_dir.join("tsconfig.json").exists(),
        "tsconfig.json written"
    );
}
