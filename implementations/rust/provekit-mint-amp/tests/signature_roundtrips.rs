mod common;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use ed25519_dalek::{Signature, Verifier};

use provekit_mint_amp::{canonical_jcs, mint_sort, SortSpec};

#[test]
fn signature_verifies_against_canonical_payload_bytes() {
    let (_dir, catalog) = common::test_catalog("signature").expect("catalog");
    let signer = common::signer();
    let minted = mint_sort(
        SortSpec::from_path(&common::fixture("sort_c_bool.spec.json")).expect("sort spec"),
        &signer,
        &catalog,
    )
    .expect("mint sort");

    let envelope: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&minted.path).expect("read envelope"))
            .expect("parse envelope");
    let sig_b64 = envelope["signature"]["sig_b64"]
        .as_str()
        .expect("signature string");
    let sig_bytes = B64.decode(sig_b64).expect("decode signature");
    let sig_array: [u8; 64] = sig_bytes.try_into().expect("signature length");
    let signature = Signature::from_bytes(&sig_array);
    let canonical = canonical_jcs(&minted.payload).expect("canonical payload");

    signer
        .verifying_key()
        .verify(canonical.as_bytes(), &signature)
        .expect("signature verifies");
}
