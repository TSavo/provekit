// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use provekit_ir_types::{DimensionValueMemento, IrFormula, PlatformSemanticTag};

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

const VALUE_WRAPPING_CID: &str = "blake3-512:a734f797c61984b7c52d3c88f87f108849391db0a7bba5f69921df50495f7bb7e09aa93dd3dd4dcecb1c7e271ec3f180dd2128f674f5f8b036c913c13ce81076";
const VALUE_TRUNCATE_CID: &str = "blake3-512:58417eadb3b2c138b4fb5aa48e9d87f433cb68aea8896aedbd5d6643465dd82d1928d4160401c6942fa802b6e8de6e48f3fb76f77aad84be595f9d337df56204";
const VALUE_ARITHMETIC_CID: &str = "blake3-512:b81b47fe4825ce7de2b555a57c7d25f01fe35fdf77591c2a3ad26204e3ea9d0aecbb978e1081247b8ce2d61ec2f01d2b3059ec9c467e527231a3bb459dd4fb63";
const VALUE_PANIC_ON_DIV_BY_ZERO_CID: &str = "blake3-512:c4c96f8725547a04b9a39e6b711a21e6eb342cd78b1c0607e5a94784288bd2e84332748d3196c8f41301e593f3b3b7441babfca8441de03fa1c78c8992679271";
const VALUE_TWOS_COMPLEMENT_CID: &str = "blake3-512:cd6a316263b716899aa4743e84e19d10ba1dbce0f6294ad608f8e6430f69b132b1e40e7e58069f59323d8cddb3ff31c4744e6726fa7b0e922611d093a6899adc";

fn atom(name: &str) -> IrFormula {
    IrFormula::Atomic {
        name: name.to_string(),
        args: vec![],
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
            OP_BITNOT,
        ])
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
                "blake3-512:91708fd03693a204f945bf36fe35b186945f090fbf1f2300f6d6d67e09d7f456bbf597178608cd8b45401733c4b1d3ecd34a3fb0b5dea83e7934643e947d698a",
            ),
            (
                OP_SUB,
                "blake3-512:1f046b09b69ab9e3f0dd07c81f02276d464b1a338268eef7dfed9a2c94bf82d33c2b94691de7040975e821855cb35af4345191cda6b633d496f89de599cfee8e",
            ),
            (
                OP_MUL,
                "blake3-512:8a91cbf2dfadb3cc159239e5e0401d53c394e9be0f8e3718cca3391099b121f351f8e36f2bddb32ddbf5c9adbb338c2ec2b70cb14d4b6fc82bfe4e0745d3ec86",
            ),
            (
                OP_DIV,
                "blake3-512:da815dc5ae6d52ab84104794eaf05c768ce3ef8a63fe17ddf52e8bf00770f227ae85995548f33cafadacaf9b58a7192ed0102b6c0b9cd74a637d78a4ee08f37f",
            ),
            (
                OP_REM,
                "blake3-512:670323ccf5872e7f1d397117b219ff7c4141c72533cca6001b34d9b39135f5f2c06a644e978fe2e57ccb04f9519f7d025a9d7b852932f55ad5aca34c8b924188",
            ),
            (
                OP_EQ,
                "blake3-512:e7d7b4c9edad872d5344d59f1c22d176a1400d39e3b066babc62ec3dba23e16d341cb9e9fa7ffa050099f696a6ac965b2f1995563039024985d52720ca1b2f7d",
            ),
            (
                OP_NE,
                "blake3-512:54b42e8945004432cf237eb5cb1999d0d0c2b2b913bb750bb3e3b20248ed8862c3972b592eb58b90f091eef6204a3237621adbe9e9ad8bd29fe6859895b00977",
            ),
            (
                OP_LT,
                "blake3-512:49a4701d7524eec47ed3ff84b72a0989037f04791e38b656fc69ad85e9d7b2c1f77a06609b252b97e9a61c16ae4aef0b82d25ee771cdbf92f825a1b6b3acceaf",
            ),
            (
                OP_LE,
                "blake3-512:c0e3860af4a9af559a93d7350573faee1fa067a0c5ee6c78a9755645dce4b36fb94f44621a308660943f3110584faf3bfa131a2f7824b4295a8ff0ec4581ac2a",
            ),
            (
                OP_GT,
                "blake3-512:286d3deac20f6036713504885d97c48088ebde08b0a22278112e88b0b431e58996c93f17902a9311c3463ddb06ebd4bf8cd8ac14a6e1781e0ae9e3c7d472ed46",
            ),
            (
                OP_GE,
                "blake3-512:7a2193f803d72cb4be7840d13193fd1b6482abe317f5bff7ecf93379aa0b54d1ead6eaa5c1cf6c6091159820b4b07e844d3191ca06b317f1816d3cd1d950abc7",
            ),
            (
                OP_AND,
                "blake3-512:1e52320e20651381c8abdcba363cdf15ec4c3358eb7be7a6a19173503f733fd819676db9bd64e6041482355fe13f41ff9f49db02e7e64d82a65b45640761f6d1",
            ),
            (
                OP_OR,
                "blake3-512:adf379019920a343ae7a2dd18195d623a5bb6f4a08baa32efde39f738ef557b39d54114ae2a17136faf8b37a404cd0cd918d43fda9d8dc8635ffa302f4d4ba73",
            ),
            (
                OP_NOT,
                "blake3-512:111f28a09497322ec0ea0d1f05fd0be8fac27d062123dea71e3d6a879a2f69b7f5cf33258daa314e366b45b4a0c6dc2a707fa60e7776dd37cf1846fcdab3e9a0",
            ),
            (
                OP_SHL,
                "blake3-512:1276e3dac697a7791e0f597cb546f78380168f710ff0f1a87c167f3bb8fbc6f2b81fcbbdae2e5dd688d805331bee4175dba4237166d43d18fe71180daf7de159",
            ),
            (
                OP_SHR,
                "blake3-512:cefd601e271ea246145e5e97c17c49012bac77daef02991c224717ea385ac324732e1940804fdb1c07915671f55e1b02ab06d84f438dbde73e3a296be6c27331",
            ),
            (
                OP_BITAND,
                "blake3-512:df02004cdedcf2bf174ee0a1afbc8f05d0eddc6a87b0fb451fb511900c22662f013be6c0f2eca3ee7e96b94c47dbeb9854a9d770510bfad85dce8f1cb8cf9f47",
            ),
            (
                OP_BITOR,
                "blake3-512:f46f10314e1f6d4844d8893e0aa44fa4473faef8510fc2075be7abdcb2d711d93362a7bbd27f9791e1312c65581a7b43cbaf9b62f9abd975405801b8e68b4994",
            ),
            (
                OP_BITXOR,
                "blake3-512:6bf02ec3c189d8628d947b0097bc4c338c99c0d37da0cee552fa015a0f9a5957ec982b6d779b3da2d12df12e502f8b3b41fe3bfa7808d4a6abf912c5c8695bd5",
            ),
            (
                OP_NEG,
                "blake3-512:badbc6dd4d338d3a680ea0e1c4a9afe8ca4fafaf63abcff9b416ecb54b6aa62f58006d60c3c97487a59ad9f49d3ca54f2ac83c2e8c6e8b281e0356e64601c928",
            ),
            (
                OP_BITNOT,
                "blake3-512:4839862d09aecad3b41362c53080b50cf938af2003cb8bd81e5ada5f363476ddfdb67d8b7ae08ecf9b2152ce1c4aa9d03da7752885583d8a08d4432c93022d4b",
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
