mod common;

use provekit_mint_amp::{mint_sort, SortSpec};

#[test]
fn object_key_order_does_not_change_cid() {
    let (_dir, catalog) = common::test_catalog("determinism").expect("catalog");
    let signer = common::signer();

    let spec_a = SortSpec::from_path(&common::fixture("sort_c_int.spec.json")).expect("spec a");
    let spec_b = SortSpec::from_json_str(
        r#"{
          "post": {
            "semantics": "implementation-defined-overflow",
            "width": 32,
            "name": "C signed int",
            "kind": "sort-description"
          },
          "return_sort": {"name": "*", "kind": "kind"},
          "formals": [],
          "fn_name": "c:int",
          "kind": "sort"
        }"#,
    )
    .expect("spec b");

    let minted_a = mint_sort(spec_a, &signer, &catalog).expect("mint a");
    let minted_b = mint_sort(spec_b, &signer, &catalog).expect("mint b");

    assert_eq!(minted_a.cid, minted_b.cid);
}
