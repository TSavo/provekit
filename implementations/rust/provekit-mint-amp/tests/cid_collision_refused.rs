mod common;

use serde_json::json;

use provekit_mint_amp::{Kind, Result};

#[test]
fn existing_cid_with_different_payload_is_refused() -> Result<()> {
    let (_dir, mut catalog) = common::test_catalog("collision").expect("catalog");
    let payload = json!({
        "schema_version": "1",
        "kind": "SortMemento",
        "fn_name": "collision"
    });

    let path = catalog.write_memento(Kind::Sort, "collision", &payload, "blake3-512:1234")?;
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&json!({
            "memento": {"tampered": true},
            "cid": "blake3-512:1234",
            "signature": null
        }))
        .expect("serialize tampered envelope"),
    )
    .expect("tamper file");

    let err = catalog
        .write_memento(Kind::Sort, "collision", &payload, "blake3-512:1234")
        .expect_err("tampered existing CID should fail");

    assert!(
        err.to_string().contains("CID collision"),
        "unexpected error: {err}"
    );
    Ok(())
}
