// SPDX-License-Identifier: Apache-2.0
//
// `provekit verify-protocol [--catalog CID] [--signed [--pubkey-file F] [--signature-file F]]`.
//
// Default mode: recompute the embedded catalog's CID and compare
// against the embedded `EXPECTED_CATALOG_CID`. Surfaces drift between
// what the binary IS and what it CLAIMS.
//
// `--signed` mode: also verify the Ed25519 signature in the embedded
// `.provekit/catalog-signatures/v1.1.0.json` against the embedded
// `.provekit/keys/foundation-v0.pub`. Override either via
// `--pubkey-file` / `--signature-file`.

use std::fs;

use owo_colors::OwoColorize;
use serde_json::json;

use crate::protocol::{
    compute_embedded_catalog_cid, parse_pubkey_bytes, verify_signed_attestation,
    EMBEDDED_CATALOG_SIGNATURE, EMBEDDED_FOUNDATION_PUBKEY, EXPECTED_CATALOG_CID,
};
use crate::VerifyProtocolArgs;

pub fn run(args: VerifyProtocolArgs) -> u8 {
    let expected = args
        .catalog
        .clone()
        .unwrap_or_else(|| EXPECTED_CATALOG_CID.to_string());
    let actual = match compute_embedded_catalog_cid() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{}: {e:#}", "error".red().bold());
            return crate::EXIT_USER_ERROR;
        }
    };
    let cid_ok = actual == expected;

    if !args.signed {
        // Pre-existing path: CID match only.
        if args.out.json {
            let payload = json!({
                "expected": expected,
                "actual": actual,
                "ok": cid_ok,
            });
            println!("{}", serde_json::to_string_pretty(&payload).unwrap());
        } else if !args.out.quiet {
            println!("{}", "ProvekIt protocol conformance".bold());
            println!("  expected : {}", expected);
            println!("  actual   : {}", actual);
            if cid_ok {
                println!("  status   : {}", "match".green().bold());
            } else {
                println!("  status   : {}", "drift".red().bold());
            }
        }
        return if cid_ok {
            crate::EXIT_OK
        } else {
            crate::EXIT_VERIFY_FAIL
        };
    }

    // Signed mode: load + verify the attestation.
    let pubkey_bytes_owned: Vec<u8>;
    let pubkey_bytes: &[u8] = match &args.pubkey_file {
        Some(p) => match fs::read(p) {
            Ok(b) => {
                pubkey_bytes_owned = b;
                &pubkey_bytes_owned
            }
            Err(e) => {
                eprintln!(
                    "{}: read pubkey file {}: {}",
                    "error".red().bold(),
                    p.display(),
                    e
                );
                return crate::EXIT_USER_ERROR;
            }
        },
        None => EMBEDDED_FOUNDATION_PUBKEY,
    };

    let sig_bytes_owned: Vec<u8>;
    let sig_bytes: &[u8] = match &args.signature_file {
        Some(p) => match fs::read(p) {
            Ok(b) => {
                sig_bytes_owned = b;
                &sig_bytes_owned
            }
            Err(e) => {
                eprintln!(
                    "{}: read signature file {}: {}",
                    "error".red().bold(),
                    p.display(),
                    e
                );
                return crate::EXIT_USER_ERROR;
            }
        },
        None => EMBEDDED_CATALOG_SIGNATURE,
    };

    let pubkey_string = match parse_pubkey_bytes(pubkey_bytes) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: {e:#}", "error".red().bold());
            return crate::EXIT_USER_ERROR;
        }
    };

    let verdict = match verify_signed_attestation(sig_bytes, &pubkey_string, &expected) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}: {e:#}", "error".red().bold());
            return crate::EXIT_USER_ERROR;
        }
    };

    let ok = cid_ok && verdict.ok();
    if args.out.json {
        let payload = json!({
            "expected": verdict.expected_cid,
            "actual": actual,
            "claimed": verdict.claimed_cid,
            "signer": verdict.signer_pubkey,
            "cidMatches": verdict.cid_matches,
            "signerMatches": verdict.signer_matches,
            "signatureOk": verdict.signature_ok,
            "ok": ok,
        });
        println!("{}", serde_json::to_string_pretty(&payload).unwrap());
    } else if !args.out.quiet {
        println!("{}", "ProvekIt protocol conformance (signed)".bold());
        println!("  expected         : {}", verdict.expected_cid);
        println!("  actual           : {}", actual);
        println!("  attested CID     : {}", verdict.claimed_cid);
        println!("  signer (pubkey)  : {}", verdict.signer_pubkey);
        let line = |label: &str, ok: bool| {
            let tag = if ok {
                "ok".green().bold().to_string()
            } else {
                "fail".red().bold().to_string()
            };
            println!("  {:<17}: {}", label, tag);
        };
        line("CID match", cid_ok);
        line("attested CID", verdict.cid_matches);
        line("signer match", verdict.signer_matches);
        line("signature", verdict.signature_ok);
        if ok {
            println!("  status           : {}", "match".green().bold());
        } else {
            println!("  status           : {}", "drift".red().bold());
        }
    }

    if ok {
        crate::EXIT_OK
    } else {
        crate::EXIT_VERIFY_FAIL
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputFlags;

    fn quiet_args() -> VerifyProtocolArgs {
        VerifyProtocolArgs {
            catalog: None,
            signed: false,
            pubkey_file: None,
            signature_file: None,
            out: OutputFlags {
                json: false,
                quiet: true,
            },
        }
    }

    #[test]
    fn verify_protocol_default_matches() {
        assert_eq!(run(quiet_args()), crate::EXIT_OK);
    }

    #[test]
    fn verify_protocol_bad_cid_fails() {
        let mut a = quiet_args();
        a.catalog = Some("blake3-512:dead".into());
        assert_eq!(run(a), crate::EXIT_VERIFY_FAIL);
    }

    #[test]
    fn verify_protocol_signed_passes_with_embedded_assets() {
        let mut a = quiet_args();
        a.signed = true;
        assert_eq!(run(a), crate::EXIT_OK);
    }

    #[test]
    fn verify_protocol_signed_fails_with_tampered_signature_file() {
        // Write a tampered signature file to a tempdir, point
        // --signature-file at it, expect failure.
        use std::env::temp_dir;
        let mut v: serde_json::Value =
            serde_json::from_slice(EMBEDDED_CATALOG_SIGNATURE).expect("parse");
        let obj = v.as_object_mut().unwrap();
        let sig = obj
            .get("signature")
            .and_then(|s| s.as_str())
            .unwrap()
            .to_string();
        let prefix = "ed25519:";
        let body = &sig[prefix.len()..];
        let first = body.chars().next().unwrap();
        let replacement = if first == 'A' { 'B' } else { 'A' };
        let mut tampered = String::from(prefix);
        tampered.push(replacement);
        tampered.push_str(&body[1..]);
        obj.insert("signature".into(), serde_json::Value::String(tampered));
        let bytes = serde_json::to_vec_pretty(&v).unwrap();

        let path = temp_dir().join(format!(
            "provekit-tamper-{}.json",
            std::process::id()
        ));
        std::fs::write(&path, bytes).unwrap();

        let mut a = quiet_args();
        a.signed = true;
        a.signature_file = Some(path.clone());
        assert_eq!(run(a), crate::EXIT_VERIFY_FAIL);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn verify_protocol_signed_fails_with_unrelated_pubkey_file() {
        // A pubkey that does not match the signer (random seed).
        // Procure by writing a different pubkey to a tempfile.
        use std::env::temp_dir;
        // Public key derived from seed [0x01; 32]: we just put any
        // valid Ed25519 pubkey that differs from the signer. We can
        // hardcode a known-different pubkey and rely on Ed25519
        // verification rejecting the signature.
        // Easiest: take the embedded pubkey, flip a base64 char.
        let pk = parse_pubkey_bytes(EMBEDDED_FOUNDATION_PUBKEY).unwrap();
        let prefix = "ed25519:";
        let body = &pk[prefix.len()..];
        let first = body.chars().next().unwrap();
        let replacement = if first == 'A' { 'B' } else { 'A' };
        let mut tampered = String::from(prefix);
        tampered.push(replacement);
        tampered.push_str(&body[1..]);

        let path = temp_dir().join(format!(
            "provekit-pubkey-{}.pub",
            std::process::id()
        ));
        std::fs::write(&path, format!("{}\n", tampered)).unwrap();

        let mut a = quiet_args();
        a.signed = true;
        a.pubkey_file = Some(path.clone());
        assert_eq!(run(a), crate::EXIT_VERIFY_FAIL);
        let _ = std::fs::remove_file(path);
    }
}
