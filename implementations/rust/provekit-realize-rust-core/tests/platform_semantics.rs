// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use provekit_ir_types::{DimensionValueMemento, IrFormula, IrTerm, PlatformSemanticTag, Sort};

const RUST_KIT_CID: &str = "blake3-512:e3c223b8b6f39382e43cb06c5b04059987e661d96311decd5003d4ec79c7d6f9969de39ae16dd6509cb5236185260d59c63288db7ff772aae00f8123ea826cbd";

const OP_ADD: &str = "blake3-512:398980644a46039b0c2875ab36ccb61f52f284ccad5481593305ed3f10efe91e7863c00a3f2d673644430f691e6b5354f5d65f9da4fa23acdb13dc58f5b438f9";
const OP_SUB: &str = "blake3-512:b6c62a64669ff12d0af45d9932c1ab5e08576f1cac97b4abe60392a9f02393dac9765514b024b1481ddc829d4b7fb97950ad648a9944dceafa194b8423923533";
const OP_MUL: &str = "blake3-512:1df457dceb0ec7a6dc4596eb70be001be09180afc69fa3ff8121cd78a0daff5dd9606dbfd4fb9fcdc5d834939a6f19c52b80aace16dea6df5ffdce62d86bbfa2";
const OP_DIV: &str = "blake3-512:d7403da8d2a8921b71170b5fc34c12022118d0c545f25c7ff89fe77bbed02419e3528479ded0e746535ee92d0e1801bce46608c15c3d6d2a5567bec811cbc75a";
// The Rust language signature names `%` as `rem`; this is the A10 mod primitive.
const OP_REM: &str = "blake3-512:235c6177611c2753a1c0d07d44391f5465ab50dc585372df52220118cb103ef19502192a07148bd2969d7f6f7ed0d134714d7745825f486768d0b0de8ac0b6dc";
const OP_EQ: &str = "blake3-512:b9d9027b698b8dfd4dd405df747cb891594bcb78d5c8529259bc5026eade633e962003c2b340caf0194296e893953c8eaa805de5146e2ff229e7adc1ac1c540f";
const OP_NE: &str = "blake3-512:737811bbbf4501951fa8e2ead801f9e0b825a6a74d7ff2d7ebda6bfd8142e6371e8f8d8a5fa0c2946bd441a65b2a58e0f95a97f09db0e93d59f4d31e4b3efc7a";
const OP_LT: &str = "blake3-512:1f601089c47ff4e0388fba2f087beeeb63fba12ddf3327e1b22223d458b3a05bbd6e19c2d68a23657b4bab09981a5d4c8c36428ad0eb1a999e7695af0b4ceaba";
const OP_LE: &str = "blake3-512:a1e6c93d90bfc818a725fa64db396c73f6756999ea39c9e662e2093db5740347ddad60381e3ed2acd1cea78690efd631a9f15179769eb51ae196deebd4b651de";
const OP_GT: &str = "blake3-512:7593a5d7d8afb9d4e6531c384ac97e1309ace179c2d68daaa23112d1196371922872d60e9fa16c4079e8163f922789987b72acf3460795eb6bc2c286c8a53a28";
const OP_GE: &str = "blake3-512:ca43840037f6d8be92a358b55e4ab08224b9d22593f7d1b8682ee22fa5d7fcc72533c8592f8ddf35ae85e40cc8ff237ab128d56e14256542084942097c820c3c";
const OP_AND: &str = "blake3-512:65e21fc8e86a0dc11cc960162da8b5023ad20d87d265a713aff021787e1af9bcc86d793dd3d10f68e7474703b28e73b881d20b3a188aa8e2da1cca8c334282b3";
const OP_OR: &str = "blake3-512:0af3f74b822cff96c71915c137b110baa0e812ba2f8b6ad42cc9d0980be5877c66d506a71d31af19e53f1e9ca99791816fe8651cb5d42450dc258dbe4ee913e9";
const OP_NOT: &str = "blake3-512:b1918406c64c20d960881b79daadd495f6e7f6900f0f72c884bfb1acc886b8ca3c096e91c3e30ed911fd995e71549f8484458b6aa7d3182c329d6df5075dff79";
const OP_SHL: &str = "blake3-512:37af5330572cf08650e3b6d5fdfc2649d56c0bb2e019f9be3861082c9d1961c1808beca6f9dfc39742ade25f06bfb499da74c89d33f64decd0c70f0972d021e1";
const OP_SHR: &str = "blake3-512:cb23fbc9d05a19b353e1fe85c77e241fdc8c58cde5a7c5cad008b721a51eaf682284d8bfe3b383d751cb58833e94beb6bd0dd4d330f9619f095c8b4daa8298da";
const OP_BITAND: &str = "blake3-512:fcc41d285a20dae6c2deb2a854665d5d43bc829a09a76107d929898b3b169d1abf53ed71f302b00ec2146bcec3b5fe732ca7ecd4354e7739e67feea3db9fd6a2";
const OP_BITOR: &str = "blake3-512:5c455355a13fd97a872848613b34b2b56f9738c832f900558710af1cd053976157513f31a8feb123202557dc0a369b88bc7c946179fe817d6c2f80d4f318f824";
const OP_BITXOR: &str = "blake3-512:16ba612da4883e853dd18b08c8e7b1803e1e2b0a42ab83c261048a49cdfd9b20bc54e809b8f4e8e5c9af63cc7447dee039cb826c611dfec137855a11a502adb9";
const OP_NEG: &str = "blake3-512:e0c3e13fd7e0d11fa3b78f4e083ab60b1166bdd905bc04e533e6dcc97d79330bd6a403caaf1265d8134ea3ccd5fe8cfd5a3e18f349ea7edcb6310c098e845c0f";
const OP_BITNOT: &str = "blake3-512:eeaaf14737f661b6bce03f23d281974502182fea83909eeaade25e510887b26e80dac1b10af3b1f2f496b53898051d63e8d250e78cfa8e88380c84809e5eabe0";
// concept:literal: which canonical sorts this kit admits at the value layer
const CONCEPT_LITERAL_CID: &str = "blake3-512:02804a0bdbd2d5d541544451f41ee8d0d340baf28f70bd5abf5844e87a96aedd7b5ab3453962754a020679cc8c6b3d1f4cf0336a7ad8118128d42ac667abf2d6";

// Canonical sort CIDs (from #1282)
const SORT_INT_CID: &str = "blake3-512:30ffc51350121a7172f3e4064a33c45bbd345756979fccff6875cd2ab33e4964d098a99df80cfbdf1ec1a0738c5ac3476f0ff8f75589ea511d1acd82c74ecd58";
const SORT_FLOAT_CID: &str = "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57";
const SORT_STRING_CID: &str = "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10";
const SORT_BOOL_CID: &str = "blake3-512:0ee13bf3fd6b7ecfbee72dfbfc18a7c0ea7f1663de6cca43cefb36f5b4c03665452646094a7c296e819e75d683c6ce4821f3d7db3c3c78ae97f2d4e3451d2074";
const SORT_BYTES_CID: &str = "blake3-512:7116ef6e62e6739b213a8394f975a53c771b89f08c36d27143827acfcfebc0e39e5b82c530be668c3cfd5ec6966ccaa42930b37fdb1f4ac25652a970be10fb6b";
// Rust does not admit Null

// Golden DimensionValueMemento CIDs (computed with #1263 elision: kit_cid elided)
const VALUE_WRAPPING_CID: &str = "blake3-512:b41427a2dac6053f60e401d86cba437687101d9702d7331021003c209e019083146c7882f4cfcf52177f8e9bc9cd9f29857c35d4ff921001d4ab364e7e599113";
const VALUE_TRUNCATE_CID: &str = "blake3-512:d9c1599bf81d67c5151cce547ccc24f590b376d5494d47d71d4ebf3a88c6087478160dbb072e721ff6e1e8eaf62c9f7f1b68c4f94494bf203ebae6a746244852";
const VALUE_ARITHMETIC_CID: &str = "blake3-512:fded608e2f3bac31845a272c8c377205f15bada070cc33e4362b6bb5cac733c981c8ffff7fe8e01e2005c01205bf83e570b793c892ac6aad39b4ccbf96d500bb";
const VALUE_PANIC_ON_DIV_BY_ZERO_CID: &str = "blake3-512:1dcdd89dfc40a84fe3e99c6abf7acbcee7e11d711552fa96688575dda99ea958324ec5e910765e943955aea7f166b5d0aa8458f3de6072ef4853dfb2c6673d96";
const VALUE_TWOS_COMPLEMENT_CID: &str = "blake3-512:a8e44e30e136a0a6ea02cbcc38df869c33461ee95c61d9731f1e7cbec8cdf5fd251bc6a3ef994aaffc7b9975eb4624e7c6031673967cde6c4308d79eedf3462e";
// concept:literal SortAdmission: Rust admits Int, Float, String, Bool, Bytes (no Null)
const VALUE_SORT_ADMISSION_CID: &str = "blake3-512:4aeaff60222a421d94a76359148c4f88bd3f555c7d7cf33250688391becce9bc4e954fc3bdf84044301d5a0e3006187848b71c9d9e5c9726a11b0d7737607820";

fn atom(name: &str) -> IrFormula {
    IrFormula::Atomic {
        name: name.to_string(),
        args: vec![],
    }
}

fn cid_const(cid: &str) -> IrTerm {
    IrTerm::Const {
        value: serde_json::Value::String(cid.to_string()),
        sort: Sort::Primitive {
            name: "cid".to_string(),
        },
    }
}

fn admits_sorts_formula(sorted_cids: &[&str]) -> IrFormula {
    IrFormula::Atomic {
        name: "admits_sorts".to_string(),
        args: sorted_cids.iter().map(|c| cid_const(c)).collect(),
    }
}

fn dimension_value_cids() -> BTreeMap<&'static str, String> {
    BTreeMap::from([
        (
            "Wrapping",
            DimensionValueMemento::new(
                RUST_KIT_CID.to_string(),
                "ArithmeticOverflow".to_string(),
                "Wrapping".to_string(),
                atom("rust:Wrapping"),
            )
            .cid,
        ),
        (
            "Truncate",
            DimensionValueMemento::new(
                RUST_KIT_CID.to_string(),
                "IntegerDivisionRounding".to_string(),
                "Truncate".to_string(),
                atom("rust:Truncate"),
            )
            .cid,
        ),
        (
            "Arithmetic",
            DimensionValueMemento::new(
                RUST_KIT_CID.to_string(),
                "ShiftMode".to_string(),
                "Arithmetic".to_string(),
                atom("rust:Arithmetic"),
            )
            .cid,
        ),
        (
            "PanicOnDivByZero",
            DimensionValueMemento::new(
                RUST_KIT_CID.to_string(),
                "NullSemantics".to_string(),
                "PanicOnDivByZero".to_string(),
                atom("rust:PanicOnDivByZero"),
            )
            .cid,
        ),
        (
            "TwosComplement",
            DimensionValueMemento::new(
                RUST_KIT_CID.to_string(),
                "BitwiseSemantics".to_string(),
                "TwosComplement".to_string(),
                atom("rust:TwosComplement"),
            )
            .cid,
        ),
        (
            "RustValueTier",
            DimensionValueMemento::new(
                RUST_KIT_CID.to_string(),
                "SortAdmission".to_string(),
                "RustValueTier".to_string(),
                // Args sorted alphabetically by CID string: BOOL, INT, BYTES, FLOAT, STRING
                admits_sorts_formula(&[
                    SORT_BOOL_CID,
                    SORT_INT_CID,
                    SORT_BYTES_CID,
                    SORT_FLOAT_CID,
                    SORT_STRING_CID,
                ]),
            )
            .cid,
        ),
    ])
}

#[test]
fn rust_platform_semantics_covers_a10_operator_surface() {
    let declaration = provekit_realize_rust_core::platform_semantics::declaration();
    let op_cids = declaration
        .tags
        .iter()
        .map(|tag| tag.op_cid.as_str())
        .collect::<BTreeSet<_>>();

    assert_eq!(
        op_cids,
        BTreeSet::from([
            OP_ADD, OP_SUB, OP_MUL, OP_DIV, OP_REM, OP_EQ, OP_NE, OP_LT, OP_LE, OP_GT, OP_GE,
            OP_AND, OP_OR, OP_NOT, OP_SHL, OP_SHR, OP_BITAND, OP_BITOR, OP_BITXOR, OP_NEG,
            OP_BITNOT, CONCEPT_LITERAL_CID,
        ])
    );
}

#[test]
fn rust_platform_semantics_concept_literal_has_sort_admission() {
    let declaration = provekit_realize_rust_core::platform_semantics::declaration();
    let cids = dimension_value_cids();
    let tags = tags_by_op(&declaration.tags);

    // concept:literal tag must carry a SortAdmission dimension
    assert_dimension(
        tags[CONCEPT_LITERAL_CID],
        "SortAdmission",
        &cids["RustValueTier"],
    );

    // Rust does not admit Null: dimension value CID must equal VALUE_SORT_ADMISSION_CID
    assert_eq!(
        tags[CONCEPT_LITERAL_CID]
            .dimensions
            .get("SortAdmission")
            .map(String::as_str),
        Some(VALUE_SORT_ADMISSION_CID),
    );
}

#[test]
fn rust_platform_semantics_uses_stage_31_dimension_names_and_values() {
    let declaration = provekit_realize_rust_core::platform_semantics::declaration();
    let cids = dimension_value_cids();
    let tags = tags_by_op(&declaration.tags);

    for op in [OP_ADD, OP_SUB, OP_MUL, OP_NEG] {
        assert_dimension(tags[op], "ArithmeticOverflow", &cids["Wrapping"]);
    }
    for op in [OP_DIV, OP_REM] {
        assert_dimension(tags[op], "IntegerDivisionRounding", &cids["Truncate"]);
        assert_dimension(tags[op], "NullSemantics", &cids["PanicOnDivByZero"]);
    }
    for op in [OP_SHL, OP_SHR] {
        assert_dimension(tags[op], "ShiftMode", &cids["Arithmetic"]);
    }
    for op in [OP_BITAND, OP_BITOR, OP_BITXOR, OP_BITNOT] {
        assert_dimension(tags[op], "BitwiseSemantics", &cids["TwosComplement"]);
    }
}

#[test]
fn rust_platform_semantics_round_trips_and_pins_cids() {
    let declaration = provekit_realize_rust_core::platform_semantics::declaration();
    let encoded_declaration = serde_json::to_string(&declaration).expect("declaration serializes");
    let decoded_declaration: provekit_realize_rust_core::platform_semantics::PlatformSemanticsDeclaration =
        serde_json::from_str(&encoded_declaration).expect("declaration decodes");
    assert_eq!(decoded_declaration, declaration);

    let expected_values = BTreeMap::from([
        ("Wrapping", VALUE_WRAPPING_CID.to_string()),
        ("Truncate", VALUE_TRUNCATE_CID.to_string()),
        ("Arithmetic", VALUE_ARITHMETIC_CID.to_string()),
        (
            "PanicOnDivByZero",
            VALUE_PANIC_ON_DIV_BY_ZERO_CID.to_string(),
        ),
        ("TwosComplement", VALUE_TWOS_COMPLEMENT_CID.to_string()),
        ("RustValueTier", VALUE_SORT_ADMISSION_CID.to_string()),
    ]);
    let actual_values = dimension_value_cids();
    assert_eq!(actual_values, expected_values);

    let actual_tags = declaration
        .tags
        .iter()
        .map(|tag| {
            let encoded = tag.to_jcs_string();
            let decoded: PlatformSemanticTag =
                serde_json::from_str(&encoded).expect("platform semantic tag decodes");
            assert_eq!(&decoded, tag);
            assert_eq!(decoded.cid, decoded.recompute_cid());
            (tag.op_cid.as_str(), tag.cid.as_str())
        })
        .collect::<BTreeMap<_, _>>();

    assert_eq!(
        actual_tags,
        BTreeMap::from([
            (
                OP_ADD,
                "blake3-512:0dfacd845ef36d7b171abef7b5928744e138c4e0f434eea71bd512608349517c77edaeb380400c62c4d49aa80cb485e33c5ed0d51b1d96405a79307671f15a47",
            ),
            (
                OP_SUB,
                "blake3-512:e1a90c7b6589922548b8ef54d6e7d09f99eaa39dbcc1ce65fd22a835b7180602445bc7a66e69d01e1d864191b2291fbc53b60ecd437dcf102b9443ef9485023f",
            ),
            (
                OP_MUL,
                "blake3-512:8efb198db60a95f0fe86d752fa4ffa9833da0158ccb68d94b6f121f2981cd7472c1ea5bf05b336b3783d131d4f8f5b0ddcca9116589bf5922fc8efb256749dc6",
            ),
            (
                OP_DIV,
                "blake3-512:287f3c237657505dfb2460e08858035fca52b98a797f3cddba470d844dd8465bb41cfb69298d2df41416a8e67424f27270d4af48b36b7250a7a41b99d9940a0e",
            ),
            (
                OP_REM,
                "blake3-512:ed74fc4d815f5d61fc011af50f8336ec190355f6fbe1ddcfa51b852516d7e08b66dc08fb448abd7e9fb9b76292bccf9aa3d320ac0c073ab44372f6028d2ae138",
            ),
            (
                OP_EQ,
                "blake3-512:1348654ce481289f31abd325d74c5a2bae6194eb69bd23ecb4868d4baa2898410d529caa24f638ea70f8832e01c83763cd27fdc809feeeebd694ebb65f487a03",
            ),
            (
                OP_NE,
                "blake3-512:270cfe8acfa144badcd1d61181269d50416c73ae36480f423378bf14764cd3106adb9154d108f8106d9c27b4af7ae781d3e70927aa71546cd6f9e41f08fd0aa7",
            ),
            (
                OP_LT,
                "blake3-512:f8e2d9cff1dc5792dd70ac501ee90ee6ccf2a5388753789343f0520b1ca84df2f927db370f40ba9d06eccfdfaaef4c064bb72762fe265991cfd7c340e5ac6681",
            ),
            (
                OP_LE,
                "blake3-512:e1d589c5b8eebd9abc30b82fdba497b9a22a2dffe492c69a5ee28d73fc8637c223920cbeb54930aede156ab149eda5e31f09d29501443d1272ab84592e55366d",
            ),
            (
                OP_GT,
                "blake3-512:12003ccc8d5f09b340e07efd9d77d8ccc66b7732e37d97ab56a1109da2638e0bad19af75ec4adc5c4dc3114d26a6ed81ed11ba5c88813b739e5d48152f9f63b9",
            ),
            (
                OP_GE,
                "blake3-512:81c44e63350e91801b3fa407d057f5a44af4203a1e1249d8fa3f6628d237f1ee64a0d4571857e230b2f8b673df1beb4b578229a42b08e2dd8186b167144caaae",
            ),
            (
                OP_AND,
                "blake3-512:1ddbc79263a17940ee397804060cd0aa9d09e7f7bb2c55a91b944c7470951699f40bd0b509abcf888f18598ea5ab66e93a2b5c17bbdb865e8f43fff49ffac70c",
            ),
            (
                OP_OR,
                "blake3-512:aef7b2034dd62c70f7fca1d60082394aacb71b9bbd14ff56881658cabf21b84968564e4bc171113afe76f51454a8e4ed59aa0cdbf83cea6c307da392cb1377a0",
            ),
            (
                OP_NOT,
                "blake3-512:39683c76522dced541058e39fa6aba7b4272125be4e26892b49b85738f621875fa1ffa52c0186f7ff2b5fb48c17306f5fa81dc19c4b08ec82c0d4c1592cff3c0",
            ),
            (
                OP_SHL,
                "blake3-512:ae7073d3ae82c97419e3ea07080a3baee50f95cfc2689b15c9c45248e8b8590c6efbdab6660d446ddb5310635878dd02a6e19952e7bf7bbe83290e7ab4e2e8fc",
            ),
            (
                OP_SHR,
                "blake3-512:5ad37cf51db1fa8eae34d4f0ed355f840b2902ee76e9bfe973e681f21b86215eadaeaa13dbaf7970b3bb771549f62c969d612187ffc2dc03413a0d8eee0a7981",
            ),
            (
                OP_BITAND,
                "blake3-512:023f60645813ba899792d80fd0c5e387d195631d260b09aa20ac73169a18fbf753e86f7a32fddec7b68f0de06c4457820c15036fe4d475187ce05f3701265e3e",
            ),
            (
                OP_BITOR,
                "blake3-512:ed9fe854214c745cdc24e4aa65fd70a146376425e50c61885e9bfb110b020036d189d42ceeaeff725c5362f8f38ac22c5eaa76d1cf2935f60d920f2e6aac8ad3",
            ),
            (
                OP_BITXOR,
                "blake3-512:649d32e3fafcb4ef31c7732e9ed72eac4186842927f513386dd0e1dbf23202f50618d651122da0dc9ee73eae39144a6cce58c63f864932f93c35ea00ad809b21",
            ),
            (
                OP_NEG,
                "blake3-512:79bda3526bd3486522d973afce2f3f9afac7e053f4442c36aa06cc8d817fe191bea55e685f6d63d4771cc524c764dfeabe922e580dcda3a949877f50c50e164d",
            ),
            (
                OP_BITNOT,
                "blake3-512:a5c8877837a1be42d679199d3a8064725f857a86cc658ff2a9fe46b8f1cd41d1dcb78bc50e13d25583c46639c98f44415005a55dd8b8cc9185b0b619739c2b9b",
            ),
            (
                CONCEPT_LITERAL_CID,
                "blake3-512:117d51ef4c61ba1e73c6e0a607c75f8c3078ea19caffb4f8d2040ccf056b0bdde11ba9bf2e817cabc026a00c6fb8ca432c9efbef56d6e3610b4acc1a7e9c6613",
            ),
        ])
    );
}

fn tags_by_op(tags: &[PlatformSemanticTag]) -> BTreeMap<&str, &PlatformSemanticTag> {
    tags.iter().map(|tag| (tag.op_cid.as_str(), tag)).collect()
}

fn assert_dimension(tag: &PlatformSemanticTag, dimension: &str, expected_cid: &str) {
    assert_eq!(
        tag.dimensions.get(dimension).map(String::as_str),
        Some(expected_cid),
        "dimension {dimension} on op {}",
        tag.op_cid
    );
}
