// SPDX-License-Identifier: Apache-2.0
//
// verify-self-contracts
//
// Letter-envelope attestation verifier for per-language self-contracts
// bundles. Performs the four trust-path checks the conformance gate cares
// about in one invocation, no external tools (no `jq`, etc.) required:
//
//   1. The attestation file parses as JSON with the expected fields.
//   2. The attestation's `signer` matches the on-disk
//      `.provekit/keys/foundation-v0.pub`.
//   3. The Ed25519 signature verifies against the rebuilt JCS-encoded
//      message body (now covering contractSetCid per spec #94).
//   4. The attestation's `contractSetCid` field equals the observed
//      contractSetCid passed in by the caller. This is the trust-path
//      comparison per spec #94: content-only, signer-independent.
//
// Bundle CID drift (attestation.cid != observed bundle bytes) is reported
// as a SOFT WARNING (eprintln! + exit 0). It is diagnostic only: bundles
// may differ across machines/runs due to envelope timestamps while the
// contractSetCid remains byte-identical. See spec #94 §0.
//
// Usage:
//
//   verify-self-contracts <attestation.json> <observed-contract-set-cid>
//
// Exits 0 iff signer + signature + contractSetCid all match.
// Exits non-zero with a diagnostic on the first failed check.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use foundation_keygen::{pubkey_path, verify_signed_self_contracts_attestation};

fn parse_pubkey_bytes(bytes: &[u8]) -> Result<String, String> {
    let text =
        std::str::from_utf8(bytes).map_err(|e| format!("pubkey file is not UTF-8: {}", e))?;
    for line in text.lines() {
        let s = line.trim();
        if s.is_empty() || s.starts_with('#') {
            continue;
        }
        if !s.starts_with("ed25519:") {
            return Err(format!("pubkey line missing `ed25519:` prefix: {:?}", s));
        }
        return Ok(s.to_string());
    }
    Err("no pubkey line found in pubkey file".to_string())
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let attestation_path = args
        .next()
        .ok_or_else(|| "missing <attestation.json> argument".to_string())?;
    let observed_contract_set_cid = args
        .next()
        .ok_or_else(|| "missing <observed-contract-set-cid> argument".to_string())?;
    if args.next().is_some() {
        return Err(
            "unexpected extra arguments; usage: verify-self-contracts <attestation.json> <observed-contract-set-cid>"
                .to_string(),
        );
    }

    let attestation_bytes = fs::read(Path::new(&attestation_path))
        .map_err(|e| format!("read {}: {}", attestation_path, e))?;
    let pk_bytes = fs::read(pubkey_path())
        .map_err(|e| format!("read foundation pubkey {}: {}", pubkey_path().display(), e))?;
    let trusted_pubkey = parse_pubkey_bytes(&pk_bytes)?;

    let verdict = verify_signed_self_contracts_attestation(
        &attestation_bytes,
        &trusted_pubkey,
        &observed_contract_set_cid,
    )?;

    if !verdict.signer_matches {
        return Err(format!(
            "signer mismatch: attestation `{}` != trusted pubkey `{}`",
            verdict.signer_pubkey, trusted_pubkey
        ));
    }
    if !verdict.signature_ok {
        return Err(format!(
            "signature verification failed against trusted pubkey `{}`",
            trusted_pubkey
        ));
    }
    if !verdict.contract_set_cid_matches {
        return Err(format!(
            "contractSetCid drift: attestation claims `{}`, observed `{}`",
            verdict.claimed_contract_set_cid, verdict.observed_contract_set_cid
        ));
    }

    println!(
        "OK  {} (contractSetCid {})",
        attestation_path, verdict.claimed_contract_set_cid
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("verify-self-contracts: {e}");
            ExitCode::FAILURE
        }
    }
}
