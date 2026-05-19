// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use provekit_ir_types::{DimensionValueMemento, IrFormula, IrTerm, PlatformSemanticTag, Sort};
use serde::{Deserialize, Serialize};

const RUST_KIT_CID: &str = "blake3-512:e3c223b8b6f39382e43cb06c5b04059987e661d96311decd5003d4ec79c7d6f9969de39ae16dd6509cb5236185260d59c63288db7ff772aae00f8123ea826cbd";

// Canonical sort CIDs (from #1282)
const SORT_INT_CID: &str = "blake3-512:30ffc51350121a7172f3e4064a33c45bbd345756979fccff6875cd2ab33e4964d098a99df80cfbdf1ec1a0738c5ac3476f0ff8f75589ea511d1acd82c74ecd58";
const SORT_FLOAT_CID: &str = "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57";
const SORT_STRING_CID: &str = "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10";
const SORT_BOOL_CID: &str = "blake3-512:0ee13bf3fd6b7ecfbee72dfbfc18a7c0ea7f1663de6cca43cefb36f5b4c03665452646094a7c296e819e75d683c6ce4821f3d7db3c3c78ae97f2d4e3451d2074";
const SORT_BYTES_CID: &str = "blake3-512:7116ef6e62e6739b213a8394f975a53c771b89f08c36d27143827acfcfebc0e39e5b82c530be668c3cfd5ec6966ccaa42930b37fdb1f4ac25652a970be10fb6b";
// Rust does not admit Null (no native null at the value layer)

// concept:literal CID (from #1282)
const CONCEPT_LITERAL_CID: &str = "blake3-512:02804a0bdbd2d5d541544451f41ee8d0d340baf28f70bd5abf5844e87a96aedd7b5ab3453962754a020679cc8c6b3d1f4cf0336a7ad8118128d42ac667abf2d6";

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformSemanticsDeclaration {
    pub tags: Vec<PlatformSemanticTag>,
}

pub fn declaration() -> PlatformSemanticsDeclaration {
    let values = dimension_value_cids();
    PlatformSemanticsDeclaration {
        tags: vec![
            tag(OP_ADD, &[("ArithmeticOverflow", &values.wrapping)]),
            tag(OP_SUB, &[("ArithmeticOverflow", &values.wrapping)]),
            tag(OP_MUL, &[("ArithmeticOverflow", &values.wrapping)]),
            tag(
                OP_DIV,
                &[
                    ("IntegerDivisionRounding", &values.truncate),
                    ("NullSemantics", &values.panic_on_div_by_zero),
                ],
            ),
            tag(
                OP_REM,
                &[
                    ("IntegerDivisionRounding", &values.truncate),
                    ("NullSemantics", &values.panic_on_div_by_zero),
                ],
            ),
            tag(OP_EQ, &[]),
            tag(OP_NE, &[]),
            tag(OP_LT, &[]),
            tag(OP_LE, &[]),
            tag(OP_GT, &[]),
            tag(OP_GE, &[]),
            tag(OP_AND, &[]),
            tag(OP_OR, &[]),
            tag(OP_NOT, &[]),
            tag(OP_SHL, &[("ShiftMode", &values.arithmetic)]),
            tag(OP_SHR, &[("ShiftMode", &values.arithmetic)]),
            tag(OP_BITAND, &[("BitwiseSemantics", &values.twos_complement)]),
            tag(OP_BITOR, &[("BitwiseSemantics", &values.twos_complement)]),
            tag(OP_BITXOR, &[("BitwiseSemantics", &values.twos_complement)]),
            tag(OP_NEG, &[("ArithmeticOverflow", &values.wrapping)]),
            tag(OP_BITNOT, &[("BitwiseSemantics", &values.twos_complement)]),
            // concept:literal: which canonical sorts this kit admits at the value layer
            tag(
                CONCEPT_LITERAL_CID,
                &[("SortAdmission", &values.sort_admission)],
            ),
        ],
    }
}

struct DimensionValueCids {
    wrapping: String,
    truncate: String,
    arithmetic: String,
    panic_on_div_by_zero: String,
    twos_complement: String,
    sort_admission: String,
}

fn dimension_value_cids() -> DimensionValueCids {
    DimensionValueCids {
        wrapping: dimension_value_cid("ArithmeticOverflow", "Wrapping", atom("rust:Wrapping")),
        truncate: dimension_value_cid("IntegerDivisionRounding", "Truncate", atom("rust:Truncate")),
        arithmetic: dimension_value_cid("ShiftMode", "Arithmetic", atom("rust:Arithmetic")),
        panic_on_div_by_zero: dimension_value_cid(
            "NullSemantics",
            "PanicOnDivByZero",
            atom("rust:PanicOnDivByZero"),
        ),
        twos_complement: dimension_value_cid(
            "BitwiseSemantics",
            "TwosComplement",
            atom("rust:TwosComplement"),
        ),
        // Rust admits: Int, Float, String, Bool, Bytes (no Null)
        // Args must be sorted alphabetically by CID string value:
        //   BOOL  (0ee1...)
        //   INT   (30ff...)
        //   BYTES (7116...)
        //   FLOAT (b979...)
        //   STRING(be87...)
        sort_admission: dimension_value_cid(
            "SortAdmission",
            "RustValueTier",
            admits_sorts_formula(&[
                SORT_BOOL_CID,
                SORT_INT_CID,
                SORT_BYTES_CID,
                SORT_FLOAT_CID,
                SORT_STRING_CID,
            ]),
        ),
    }
}

pub fn dimension_values() -> Vec<DimensionValueMemento> {
    vec![
        dimension_value("ArithmeticOverflow", "Wrapping", atom("rust:Wrapping")),
        dimension_value("IntegerDivisionRounding", "Truncate", atom("rust:Truncate")),
        dimension_value("ShiftMode", "Arithmetic", atom("rust:Arithmetic")),
        dimension_value(
            "NullSemantics",
            "PanicOnDivByZero",
            atom("rust:PanicOnDivByZero"),
        ),
        dimension_value(
            "BitwiseSemantics",
            "TwosComplement",
            atom("rust:TwosComplement"),
        ),
        // concept:literal SortAdmission: Rust admits Int, Float, String, Bool, Bytes (no Null)
        dimension_value(
            "SortAdmission",
            "RustValueTier",
            admits_sorts_formula(&[
                SORT_BOOL_CID,
                SORT_INT_CID,
                SORT_BYTES_CID,
                SORT_FLOAT_CID,
                SORT_STRING_CID,
            ]),
        ),
    ]
}

fn dimension_value(
    dimension_name: &str,
    value_name: &str,
    compare_to: IrFormula,
) -> DimensionValueMemento {
    DimensionValueMemento::new(
        RUST_KIT_CID.to_string(),
        dimension_name.to_string(),
        value_name.to_string(),
        compare_to,
    )
}

fn dimension_value_cid(dimension_name: &str, value_name: &str, compare_to: IrFormula) -> String {
    dimension_value(dimension_name, value_name, compare_to).cid
}

fn tag(op_cid: &str, dimensions: &[(&str, &str)]) -> PlatformSemanticTag {
    let dimensions = dimensions
        .iter()
        .map(|(dimension, cid)| ((*dimension).to_string(), (*cid).to_string()))
        .collect::<BTreeMap<_, _>>();
    PlatformSemanticTag::new(RUST_KIT_CID.to_string(), op_cid.to_string(), dimensions)
}

fn atom(name: &str) -> IrFormula {
    IrFormula::Atomic {
        name: name.to_string(),
        args: vec![],
    }
}

/// Build an `admits_sorts` formula whose args are CID constants in the given order.
/// Callers must pass CIDs already sorted alphabetically by string value.
fn admits_sorts_formula(sorted_cids: &[&str]) -> IrFormula {
    let args = sorted_cids
        .iter()
        .map(|cid| IrTerm::Const {
            value: serde_json::Value::String((*cid).to_string()),
            sort: Sort::Primitive {
                name: "cid".to_string(),
            },
        })
        .collect();
    IrFormula::Atomic {
        name: "admits_sorts".to_string(),
        args,
    }
}
