// Diagnostic: run the lifter against a workspace and dump per-adapter
// warnings + lifted contract IR-as-debug. Used to inspect what the
// self-lift produced when iterating on the v0.5 whitelist extension.
//
// USAGE: cargo run --release --example dump_self_lift -- <workspace>

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root: PathBuf = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("implementations/rust/provekit-canonicalizer"));

    let report = provekit_lift::lift_path(&root);

    println!("files_scanned: {}", report.files_scanned);
    println!("decls: {}", report.decls.len());
    println!();
    for ar in &report.adapter_reports {
        if ar.seen == 0 && ar.lifted == 0 && ar.warnings.is_empty() {
            continue;
        }
        println!(
            "adapter {}: seen={} lifted={} skipped={}",
            ar.adapter,
            ar.seen,
            ar.lifted,
            ar.warnings.len()
        );
        for w in &ar.warnings {
            println!("  SKIP {} :: {}", w.item_name, w.reason);
        }
        println!();
    }

    println!("=== LIFTED CONTRACTS ({}) ===", report.decls.len());
    for d in &report.decls {
        println!("--- {} ---", d.name);
        println!("inv: {:#?}", d.inv);
    }
}
