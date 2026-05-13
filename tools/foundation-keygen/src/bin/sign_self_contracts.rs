// SPDX-License-Identifier: Apache-2.0
//
// sign-self-contracts
//
// Letter-envelope attestation signer for per-language self-contracts
// bundles. Per spec #94, the attestation now requires `contractSetCid`
// (the signer-independent trust anchor) in addition to the bundle CID.
//
// Usage:
//
//   sign-self-contracts <lang> <bundle-cid> <contract-set-cid>
//
// Example:
//
//   sign-self-contracts rust \
//     blake3-512:a0f58941758d7097...3838ab \
//     blake3-512:deadbeef...
//
// Effect: writes `.provekit/self-contracts-attestations/<lang>.json`
// containing the v0 foundation key's signature over the canonical
// (JCS) bytes of the signed message body
// `{schemaVersion, kind, lang, cid, contractSetCid, declaredAt, signer}`.
//
// Determinism: ed25519 is deterministic by spec, the timestamp is
// pinned to a per-protocol-version constant, and the seed is also a
// constant; re-running with the same arguments produces byte-identical output.

use std::fs;
use std::process::ExitCode;

use foundation_keygen::{
    build_signed_self_contracts_attestation, self_contracts_attestation_path_for,
    FOUNDATION_V0_SEED, SELF_CONTRACTS_DECLARED_AT_V1_6_0, SELF_CONTRACTS_LANGS,
};

fn validate_cid(cid: &str, name: &str) -> Result<(), String> {
    if !cid.starts_with("blake3-512:") {
        return Err(format!(
            "{name} must start with `blake3-512:`, got `{}`",
            cid
        ));
    }
    let hex = &cid["blake3-512:".len()..];
    if hex.len() != 128 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "{name} hex body must be 128 ascii hex chars, got `{}` ({} chars)",
            hex,
            hex.len()
        ));
    }
    Ok(())
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let lang = args.next().ok_or_else(|| {
        format!(
            "missing <lang> argument; expected one of {:?}",
            SELF_CONTRACTS_LANGS
        )
    })?;
    let cid = args
        .next()
        .ok_or_else(|| "missing <bundle-cid> argument (blake3-512:<128 hex>)".to_string())?;
    let contract_set_cid = args
        .next()
        .ok_or_else(|| "missing <contract-set-cid> argument (blake3-512:<128 hex>)".to_string())?;
    if args.next().is_some() {
        return Err(
            "unexpected extra arguments; usage: sign-self-contracts <lang> <bundle-cid> <contract-set-cid>"
                .to_string(),
        );
    }

    if !SELF_CONTRACTS_LANGS.contains(&lang.as_str()) {
        return Err(format!(
            "unknown lang `{}`; expected one of {:?}",
            lang, SELF_CONTRACTS_LANGS
        ));
    }
    validate_cid(&cid, "<bundle-cid>")?;
    validate_cid(&contract_set_cid, "<contract-set-cid>")?;

    let attestation = build_signed_self_contracts_attestation(
        &FOUNDATION_V0_SEED,
        &lang,
        &cid,
        &contract_set_cid,
        SELF_CONTRACTS_DECLARED_AT_V1_6_0,
    )?;

    let out_path = self_contracts_attestation_path_for(&lang);
    if let Some(dir) = out_path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {}", dir.display(), e))?;
    }
    let mut out = serde_json::to_string_pretty(&attestation)
        .map_err(|e| format!("serialize attestation: {}", e))?;
    out.push('\n');
    fs::write(&out_path, out).map_err(|e| format!("write {}: {}", out_path.display(), e))?;

    println!("# ProvekIt self-contracts attestation (spec #94)");
    println!();
    println!("lang:              {}", lang);
    println!("cid:               {}", cid);
    println!("contractSetCid:    {}", contract_set_cid);
    println!("declaredAt:        {}", SELF_CONTRACTS_DECLARED_AT_V1_6_0);
    println!("signer:            {}", attestation["signer"]);
    println!("signature:         {}", attestation["signature"]);
    println!();
    println!("wrote signed attestation: {}", out_path.display());
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sign-self-contracts: {e}");
            ExitCode::FAILURE
        }
    }
}
