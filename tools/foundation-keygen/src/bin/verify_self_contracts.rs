// SPDX-License-Identifier: Apache-2.0
//
// verify-self-contracts
//
// Letter-envelope attestation verifier for per-language self-contracts
// bundles. Performs the four checks the conformance gate cares about
// in one invocation, no external tools (no `jq`, etc.) required:
//
//   1. The attestation file parses as JSON with the expected fields.
//   2. The attestation's `signer` matches the on-disk
//      `.provekit/keys/foundation-v0.pub`.
//   3. The Ed25519 signature verifies against the rebuilt JCS-encoded
//      six-field message body.
//   4. The attestation's `cid` field equals the observed CID passed in
//      by the caller (typically the freshly-minted output of
//      `provekit mint`).
//
// Usage:
//
//   verify-self-contracts <attestation.json> <observed-cid>
//
// Exits 0 iff all four checks pass; non-zero with a diagnostic on the
// first failed check otherwise.

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
    let observed_cid = args
        .next()
        .ok_or_else(|| "missing <observed-cid> argument".to_string())?;
    if args.next().is_some() {
        return Err(
            "unexpected extra arguments; usage: verify-self-contracts <attestation.json> <observed-cid>"
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
        &observed_cid,
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
    if !verdict.cid_matches {
        return Err(format!(
            "CID drift: attestation claims `{}`, observed `{}`",
            verdict.claimed_cid, verdict.observed_cid
        ));
    }

    println!("OK  {} (cid {})", attestation_path, verdict.claimed_cid);
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
