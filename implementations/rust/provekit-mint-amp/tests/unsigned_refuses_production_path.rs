use provekit_mint_amp::{mint_sort, Catalog, Signer, SortSpec};

#[test]
fn unsigned_signer_refuses_non_test_catalog_root() {
    let dir = tempfile::Builder::new()
        .prefix("provekit-production-catalog-")
        .tempdir()
        .expect("tempdir");
    let catalog = Catalog::new(dir.path().join("catalog")).expect("catalog");
    let signer = Signer::from_unsigned_test_only();
    let spec = SortSpec::from_json_str(
        r#"{
          "kind": "sort",
          "fn_name": "production-check",
          "formals": [],
          "return_sort": {"kind": "kind", "name": "*"},
          "post": {"kind": "sort-description", "name": "production check"}
        }"#,
    )
    .expect("spec");

    let err = mint_sort(spec, &signer, &catalog).expect_err("unsigned should refuse");

    assert!(
        err.to_string().contains("unsigned test signer"),
        "unexpected error: {err}"
    );
}
