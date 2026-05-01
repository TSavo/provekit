// SPDX-License-Identifier: Apache-2.0
//
// `provekit verify-protocol [--catalog CID]`.
//
// Recompute the embedded catalog's CID and compare against either the
// `--catalog` arg or the compiled-in `EXPECTED_CATALOG_CID`. Surfaces
// drift between what the binary IS and what it CLAIMS.

use owo_colors::OwoColorize;
use serde_json::json;

use crate::protocol::{compute_embedded_catalog_cid, EXPECTED_CATALOG_CID};
use crate::VerifyProtocolArgs;

pub fn run(args: VerifyProtocolArgs) -> u8 {
    let expected = args.catalog.unwrap_or_else(|| EXPECTED_CATALOG_CID.to_string());
    match compute_embedded_catalog_cid() {
        Ok(actual) => {
            let ok = actual == expected;
            if args.out.json {
                let payload = json!({
                    "expected": expected,
                    "actual": actual,
                    "ok": ok,
                });
                println!("{}", serde_json::to_string_pretty(&payload).unwrap());
            } else if !args.out.quiet {
                println!("{}", "ProvekIt protocol conformance".bold());
                println!("  expected : {}", expected);
                println!("  actual   : {}", actual);
                if ok {
                    println!("  status   : {}", "match".green().bold());
                } else {
                    println!("  status   : {}", "drift".red().bold());
                }
            }
            if ok {
                crate::EXIT_OK
            } else {
                crate::EXIT_VERIFY_FAIL
            }
        }
        Err(e) => {
            eprintln!("{}: {e:#}", "error".red().bold());
            crate::EXIT_USER_ERROR
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputFlags;

    #[test]
    fn verify_protocol_default_matches() {
        let args = VerifyProtocolArgs {
            catalog: None,
            out: OutputFlags {
                json: false,
                quiet: true,
            },
        };
        assert_eq!(run(args), crate::EXIT_OK);
    }

    #[test]
    fn verify_protocol_bad_cid_fails() {
        let args = VerifyProtocolArgs {
            catalog: Some("blake3-512:dead".into()),
            out: OutputFlags {
                json: false,
                quiet: true,
            },
        };
        assert_eq!(run(args), crate::EXIT_VERIFY_FAIL);
    }
}
