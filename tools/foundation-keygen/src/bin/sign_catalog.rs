// SPDX-License-Identifier: Apache-2.0
//
// sign-catalog
//
// Compute the protocol catalog's CID, build the canonical attestation
// JSON, sign it under the v0 foundation seed, and write the signed
// attestation to `.provekit/catalog-signatures/v1.1.0.json`.
//
// Pinned `declaredAt = 2026-04-30T15:00:00Z` (matches the catalog's
// own `declaredAt`) so the signed output is bit-identical across runs.

use std::fs;
use std::process::ExitCode;

use foundation_keygen::{
    build_signed_attestation, catalog_path, compute_catalog_cid_from_path, signature_path,
    FOUNDATION_V0_SEED, V1_1_0_DECLARED_AT,
};

fn run() -> Result<(), String> {
    let catalog = catalog_path();
    let cid = compute_catalog_cid_from_path(&catalog)?;
    let attestation =
        build_signed_attestation(&FOUNDATION_V0_SEED, &cid, V1_1_0_DECLARED_AT)?;

    let out_path = signature_path();
    if let Some(dir) = out_path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {}", dir.display(), e))?;
    }
    let mut out = serde_json::to_string_pretty(&attestation)
        .map_err(|e| format!("serialize attestation: {}", e))?;
    out.push('\n');
    fs::write(&out_path, out).map_err(|e| format!("write {}: {}", out_path.display(), e))?;

    println!("# ProvekIt v1.1.0 catalog attestation");
    println!();
    println!("catalog file:    {}", catalog.display());
    println!("catalog CID:     {}", cid);
    println!("signer:          {}", attestation["signer"]);
    println!("signature:       {}", attestation["signature"]);
    println!("attestation:     {}", out_path.display());
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sign-catalog: {}", e);
            ExitCode::FAILURE
        }
    }
}
