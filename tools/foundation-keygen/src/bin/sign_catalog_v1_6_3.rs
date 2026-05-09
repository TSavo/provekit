// SPDX-License-Identifier: Apache-2.0
//
// sign-catalog-v1-6-3
//
// Compute the protocol catalog's CID, build the canonical attestation
// JSON for protocolVersion `v1.6.3`, sign it under the v0 foundation
// seed, and write the signed attestation to
// `.provekit/catalog-signatures/v1.6.3.json`.
//
// Patch bump over v1.6.2: re-bakes lift-plugin-protocol to formalize
// identify-only package inspection. No core verifier behavior or
// all-layer lift semantic obligation changes.

use std::fs;
use std::process::ExitCode;

use foundation_keygen::{
    build_signed_attestation_for, catalog_path, compute_catalog_cid_from_path, signature_path_for,
    FOUNDATION_V0_SEED, V1_6_3_DECLARED_AT,
};

const PROTOCOL_VERSION: &str = "v1.6.3";

fn run() -> Result<(), String> {
    let catalog = catalog_path();
    let cid = compute_catalog_cid_from_path(&catalog)?;
    let attestation = build_signed_attestation_for(
        PROTOCOL_VERSION,
        &FOUNDATION_V0_SEED,
        &cid,
        V1_6_3_DECLARED_AT,
    )?;

    let out_path = signature_path_for(PROTOCOL_VERSION);
    if let Some(dir) = out_path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {}", dir.display(), e))?;
    }
    let mut out = serde_json::to_string_pretty(&attestation)
        .map_err(|e| format!("serialize attestation: {}", e))?;
    out.push('\n');
    fs::write(&out_path, out).map_err(|e| format!("write {}: {}", out_path.display(), e))?;

    println!("# ProvekIt v1.6.3 catalog attestation");
    println!();
    println!("catalog file:    {}", catalog.display());
    println!("catalog CID:     {}", cid);
    println!("signer:          {}", attestation["signer"]);
    println!("signature:       {}", attestation["signature"]);
    println!();
    println!("wrote signed attestation: {}", out_path.display());
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sign-catalog-v1-6-3: {e}");
            ExitCode::FAILURE
        }
    }
}
