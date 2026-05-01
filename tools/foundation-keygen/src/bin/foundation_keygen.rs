// SPDX-License-Identifier: Apache-2.0
//
// foundation-keygen
//
// Generate the v0 ProvekIt Foundation Root Key from the publicly-known
// deterministic seed `[0x42; 32]`. Writes:
//
//   .provekit/keys/foundation-v0.priv  (raw 32-byte seed, hex-encoded; gitignored)
//   .provekit/keys/foundation-v0.pub   (ed25519:<base64>; committed)
//
// This is the v0 "test foundation key": every byte is reproducible from
// public inputs. v1 will be HSM-generated; key rotation is a future
// spec. See `protocol/specs/2026-04-30-protocol-versioning.md` and
// `.provekit/keys/README.md`.

use std::fs;
use std::process::ExitCode;

use foundation_keygen::{privkey_path, pubkey_path, FOUNDATION_V0_SEED};
use provekit_proof_envelope::ed25519_pubkey_string;

fn run() -> Result<(), String> {
    let priv_path = privkey_path();
    let pub_path = pubkey_path();

    if let Some(dir) = priv_path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {}", dir.display(), e))?;
    }

    // Private key on disk: hex-encoded 32-byte seed plus a trailing
    // newline. Since the seed is public for v0, this file's value is
    // pedagogical (it shows the shape) rather than secret.
    let priv_hex = FOUNDATION_V0_SEED
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    let priv_body = format!(
        "# ProvekIt Foundation v0 Ed25519 SEED (test only).\n\
         # Publicly known: `[0x42; 32]`. Anyone can forge a valid v0\n\
         # signature. v1 will be HSM-generated; do NOT trust v0 in\n\
         # production. This file is gitignored to mirror the procedure\n\
         # we will follow in v1, where the seed must never be committed.\n\
         {}\n",
        priv_hex
    );
    fs::write(&priv_path, priv_body)
        .map_err(|e| format!("write {}: {}", priv_path.display(), e))?;

    let pub_string = ed25519_pubkey_string(&FOUNDATION_V0_SEED);
    let pub_body = format!("{}\n", pub_string);
    fs::write(&pub_path, pub_body)
        .map_err(|e| format!("write {}: {}", pub_path.display(), e))?;

    println!("# ProvekIt Foundation v0 keypair");
    println!();
    println!("seed (TEST, public):  [0x42; 32]");
    println!("private file:         {}", priv_path.display());
    println!("public file:          {}", pub_path.display());
    println!("public key:           {}", pub_string);
    println!();
    println!("v0 uses a deterministic test seed; v1 should be HSM-generated.");
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("foundation-keygen: {}", e);
            ExitCode::FAILURE
        }
    }
}
