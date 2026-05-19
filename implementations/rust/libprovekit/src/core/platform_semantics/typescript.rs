// SPDX-License-Identifier: Apache-2.0
//
// TypeScript language-kit PlatformSemanticsDeclaration.
//
// JavaScript/TypeScript arithmetic uses IEEE 754 double-precision floating
// point for the Number type. There is no integer arithmetic type at the
// language level; all arithmetic ops produce doubles. Bitwise operators
// coerce their operands to signed 32-bit integers via the ToInt32 algorithm
// before operating, then return a double whose value equals the 32-bit result.
//
// Canonical value names chosen for cross-kit discrimination:
//   ArithmeticOverflow  -> Ieee754Saturate   (overflows saturate to +/-Infinity)
//   IntegerDivisionRounding -> FloatDivision (no integer-divide; always float)
//   NullSemantics       -> ReturnsNanOrInfinity (div/rem by zero gives NaN or Inf)
//   ShiftMode           -> Int32Wrapping     (bitwise shifts coerce to int32)
//   BitwiseSemantics    -> Int32             (bitwise ops coerce to int32)

use std::collections::BTreeMap;

use provekit_canonicalizer::blake3_512_of;
use provekit_ir_types::{DimensionValueMemento, IrFormula, IrTerm, PlatformSemanticTag};

use crate::core::types::PlatformSemanticsDeclaration;

const KIT_ID: &str = "provekit-realize-typescript-core@0.1.0";

// Concept-op CIDs shared across all language kits (same cross-kit hub CIDs
// as used by Rust, Java, Python). Source of truth: menagerie/concept-shapes/cids.tsv.
const OP_ADD: &str = "blake3-512:398980644a46039b0c2875ab36ccb61f52f284ccad5481593305ed3f10efe91e7863c00a3f2d673644430f691e6b5354f5d65f9da4fa23acdb13dc58f5b438f9";
const OP_SUB: &str = "blake3-512:b6c62a64669ff12d0af45d9932c1ab5e08576f1cac97b4abe60392a9f02393dac9765514b024b1481ddc829d4b7fb97950ad648a9944dceafa194b8423923533";
const OP_MUL: &str = "blake3-512:1df457dceb0ec7a6dc4596eb70be001be09180afc69fa3ff8121cd78a0daff5dd9606dbfd4fb9fcdc5d834939a6f19c52b80aace16dea6df5ffdce62d86bbfa2";
const OP_DIV: &str = "blake3-512:d7403da8d2a8921b71170b5fc34c12022118d0c545f25c7ff89fe77bbed02419e3528479ded0e746535ee92d0e1801bce46608c15c3d6d2a5567bec811cbc75a";
const OP_REM: &str = "blake3-512:235c6177611c2753a1c0d07d44391f5465ab50dc585372df52220118cb103ef19502192a07148bd2969d7f6f7ed0d134714d7745825f486768d0b0de8ac0b6dc";
const OP_SHL: &str = "blake3-512:37af5330572cf08650e3b6d5fdfc2649d56c0bb2e019f9be3861082c9d1961c1808beca6f9dfc39742ade25f06bfb499da74c89d33f64decd0c70f0972d021e1";
const OP_SHR: &str = "blake3-512:cb23fbc9d05a19b353e1fe85c77e241fdc8c58cde5a7c5cad008b721a51eaf682284d8bfe3b383d751cb58833e94beb6bd0dd4d330f9619f095c8b4daa8298da";
const OP_BITAND: &str = "blake3-512:fcc41d285a20dae6c2deb2a854665d5d43bc829a09a76107d929898b3b169d1abf53ed71f302b00ec2146bcec3b5fe732ca7ecd4354e7739e67feea3db9fd6a2";
const OP_BITOR: &str = "blake3-512:5c455355a13fd97a872848613b34b2b56f9738c832f900558710af1cd053976157513f31a8feb123202557dc0a369b88bc7c946179fe817d6c2f80d4f318f824";
const OP_BITXOR: &str = "blake3-512:16ba612da4883e853dd18b08c8e7b1803e1e2b0a42ab83c261048a49cdfd9b20bc54e809b8f4e8e5c9af63cc7447dee039cb826c611dfec137855a11a502adb9";
const OP_NEG: &str = "blake3-512:e0c3e13fd7e0d11fa3b78f4e083ab60b1166bdd905bc04e533e6dcc97d79330bd6a403caaf1265d8134ea3ccd5fe8cfd5a3e18f349ea7edcb6310c098e845c0f";
const OP_BITNOT: &str = "blake3-512:eeaaf14737f661b6bce03f23d281974502182fea83909eeaade25e510887b26e80dac1b10af3b1f2f496b53898051d63e8d250e78cfa8e88380c84809e5eabe0";
const OP_LITERAL: &str = "blake3-512:02804a0bdbd2d5d541544451f41ee8d0d340baf28f70bd5abf5844e87a96aedd7b5ab3453962754a020679cc8c6b3d1f4cf0336a7ad8118128d42ac667abf2d6";

const SORT_BOOL_CID: &str = "blake3-512:0ee13bf3fd6b7ecfbee72dfbfc18a7c0ea7f1663de6cca43cefb36f5b4c03665452646094a7c296e819e75d683c6ce4821f3d7db3c3c78ae97f2d4e3451d2074";
const SORT_FLOAT_CID: &str = "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57";
const SORT_NULL_CID: &str = "blake3-512:62f6040bd3f414c1e6c2b7bdf276669cd5613b33cb508a81170170064ca3ffba771a4b0002dc52e059fce5f9f63a1874ef71bd4ec89ae06e89c87a3e91aac3b5";
const SORT_STRING_CID: &str = "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10";

#[derive(Clone, Copy)]
struct AdmittedSort {
    name: &'static str,
    cid: &'static str,
}

pub fn declaration() -> PlatformSemanticsDeclaration {
    let kit_cid = blake3_512_of(KIT_ID.as_bytes());
    let values = dimension_values_for_kit(&kit_cid);
    let value_cids = values
        .iter()
        .map(|v| (v.value_name.clone(), v.cid.clone()))
        .collect::<BTreeMap<_, _>>();

    PlatformSemanticsDeclaration {
        tags: vec![
            // Arithmetic ops: IEEE 754 saturate on overflow (-> +/-Infinity, not wrap or panic)
            tag(
                &kit_cid,
                OP_ADD,
                &[("ArithmeticOverflow", value_cids["Ieee754Saturate"].as_str())],
            ),
            tag(
                &kit_cid,
                OP_SUB,
                &[("ArithmeticOverflow", value_cids["Ieee754Saturate"].as_str())],
            ),
            tag(
                &kit_cid,
                OP_MUL,
                &[("ArithmeticOverflow", value_cids["Ieee754Saturate"].as_str())],
            ),
            // Division: always float (no integer truncation), div-by-zero gives NaN/Infinity
            tag(
                &kit_cid,
                OP_DIV,
                &[
                    (
                        "IntegerDivisionRounding",
                        value_cids["FloatDivision"].as_str(),
                    ),
                    ("NullSemantics", value_cids["ReturnsNanOrInfinity"].as_str()),
                ],
            ),
            // Remainder: same float semantics, NaN/Infinity on zero
            tag(
                &kit_cid,
                OP_REM,
                &[
                    (
                        "IntegerDivisionRounding",
                        value_cids["FloatDivision"].as_str(),
                    ),
                    ("NullSemantics", value_cids["ReturnsNanOrInfinity"].as_str()),
                ],
            ),
            // Bitwise shifts: ToInt32 coercion then wrapping shift
            tag(
                &kit_cid,
                OP_SHL,
                &[("ShiftMode", value_cids["Int32Wrapping"].as_str())],
            ),
            tag(
                &kit_cid,
                OP_SHR,
                &[("ShiftMode", value_cids["Int32Wrapping"].as_str())],
            ),
            // Bitwise ops: ToInt32 coercion
            tag(
                &kit_cid,
                OP_BITAND,
                &[("BitwiseSemantics", value_cids["Int32"].as_str())],
            ),
            tag(
                &kit_cid,
                OP_BITOR,
                &[("BitwiseSemantics", value_cids["Int32"].as_str())],
            ),
            tag(
                &kit_cid,
                OP_BITXOR,
                &[("BitwiseSemantics", value_cids["Int32"].as_str())],
            ),
            tag(
                &kit_cid,
                OP_BITNOT,
                &[("BitwiseSemantics", value_cids["Int32"].as_str())],
            ),
            // Unary negation: IEEE 754 saturate (negating MIN_SAFE_INT gives float result)
            tag(
                &kit_cid,
                OP_NEG,
                &[("ArithmeticOverflow", value_cids["Ieee754Saturate"].as_str())],
            ),
            tag(
                &kit_cid,
                OP_LITERAL,
                &[("SortAdmission", value_cids["BoolNullFloatString"].as_str())],
            ),
        ],
        dimension_values: values,
        op_aliases: BTreeMap::new(),
    }
}

pub fn dimension_values() -> Vec<DimensionValueMemento> {
    let kit_cid = blake3_512_of(KIT_ID.as_bytes());
    dimension_values_for_kit(&kit_cid)
}

fn dimension_values_for_kit(kit_cid: &str) -> Vec<DimensionValueMemento> {
    vec![
        // ArithmeticOverflow: IEEE 754 double saturates to +/-Infinity rather than wrapping
        dimension_value(kit_cid, "ArithmeticOverflow", "Ieee754Saturate"),
        // IntegerDivisionRounding: JS `/` always performs float division, never truncates
        dimension_value(kit_cid, "IntegerDivisionRounding", "FloatDivision"),
        // NullSemantics: division or remainder by zero returns NaN or Infinity, no exception
        dimension_value(kit_cid, "NullSemantics", "ReturnsNanOrInfinity"),
        // ShiftMode: bitwise shift operands are coerced to Int32 with wrapping
        dimension_value(kit_cid, "ShiftMode", "Int32Wrapping"),
        // BitwiseSemantics: bitwise operands are coerced to Int32 via ToInt32 algorithm
        dimension_value(kit_cid, "BitwiseSemantics", "Int32"),
        sort_admission_value(
            kit_cid,
            &[
                AdmittedSort {
                    name: "Float",
                    cid: SORT_FLOAT_CID,
                },
                AdmittedSort {
                    name: "String",
                    cid: SORT_STRING_CID,
                },
                AdmittedSort {
                    name: "Bool",
                    cid: SORT_BOOL_CID,
                },
                AdmittedSort {
                    name: "Null",
                    cid: SORT_NULL_CID,
                },
            ],
        ),
    ]
}

fn dimension_value(kit_cid: &str, dimension_name: &str, value_name: &str) -> DimensionValueMemento {
    DimensionValueMemento::new(
        kit_cid.to_string(),
        dimension_name.to_string(),
        value_name.to_string(),
        IrFormula::Atomic {
            name: format!("typescript:{value_name}"),
            args: vec![],
        },
    )
}

fn sort_admission_value(kit_cid: &str, admitted: &[AdmittedSort]) -> DimensionValueMemento {
    let mut admitted = admitted.to_vec();
    admitted.sort_by(|left, right| left.cid.cmp(right.cid));
    let value_name = admitted
        .iter()
        .map(|sort| sort.name)
        .collect::<Vec<_>>()
        .join("");
    DimensionValueMemento::new(
        kit_cid.to_string(),
        "SortAdmission".to_string(),
        value_name,
        IrFormula::Atomic {
            name: "admits_sorts".to_string(),
            args: admitted
                .iter()
                .map(|sort| IrTerm::Ctor {
                    name: sort.name.to_string(),
                    args: vec![IrTerm::Ctor {
                        name: sort.cid.to_string(),
                        args: vec![],
                    }],
                })
                .collect(),
        },
    )
}

fn tag(kit_cid: &str, op_cid: &str, pairs: &[(&str, &str)]) -> PlatformSemanticTag {
    let mut dimensions = BTreeMap::new();
    for (dimension, cid) in pairs {
        dimensions.insert((*dimension).to_string(), (*cid).to_string());
    }
    PlatformSemanticTag::new(kit_cid.to_string(), op_cid.to_string(), dimensions)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Positive: declaration is non-empty (has tags and dimension_values)
    #[test]
    fn typescript_declaration_is_non_empty() {
        let decl = declaration();
        assert!(
            !decl.tags.is_empty(),
            "typescript kit must declare at least one op tag"
        );
        assert!(
            !decl.dimension_values.is_empty(),
            "typescript kit must declare dimension values"
        );
    }

    // Discrimination: TypeScript differs from Rust on ArithmeticOverflow for OP_ADD.
    // Rust uses Wrapping; TypeScript uses Ieee754Saturate. They must differ.
    #[test]
    fn typescript_arithmetic_overflow_differs_from_rust() {
        use provekit_canonicalizer::blake3_512_of as b3;
        let ts_decl = declaration();
        // Compute Rust kit's ArithmeticOverflow/Wrapping dimension value CID.
        // Rust kit CID is a known constant from provekit-realize-rust-core.
        let rust_kit_cid = "blake3-512:e3c223b8b6f39382e43cb06c5b04059987e661d96311decd5003d4ec79c7d6f9969de39ae16dd6509cb5236185260d59c63288db7ff772aae00f8123ea826cbd";
        let rust_overflow_cid = provekit_ir_types::DimensionValueMemento::new(
            rust_kit_cid.to_string(),
            "ArithmeticOverflow".to_string(),
            "Wrapping".to_string(),
            IrFormula::Atomic {
                name: "rust:Wrapping".to_string(),
                args: vec![],
            },
        )
        .cid;
        // TypeScript's kit CID is different (different KIT_ID -> different kit hash)
        let ts_kit_cid = b3(KIT_ID.as_bytes());
        let ts_overflow_cid = provekit_ir_types::DimensionValueMemento::new(
            ts_kit_cid.clone(),
            "ArithmeticOverflow".to_string(),
            "Ieee754Saturate".to_string(),
            IrFormula::Atomic {
                name: "typescript:Ieee754Saturate".to_string(),
                args: vec![],
            },
        )
        .cid;
        assert_ne!(
            rust_overflow_cid, ts_overflow_cid,
            "Rust Wrapping and TypeScript Ieee754Saturate must hash to different CIDs"
        );
        // Also verify the TS declaration actually contains the TS overflow CID and not Rust's
        let ts_add_tag = ts_decl.tags.iter().find(|t| t.op_cid == OP_ADD);
        let ts_add_overflow = ts_add_tag
            .and_then(|t| t.dimensions.get("ArithmeticOverflow"))
            .cloned();
        assert_eq!(
            ts_add_overflow.as_deref(),
            Some(ts_overflow_cid.as_str()),
            "TS OP_ADD tag must reference Ieee754Saturate CID"
        );
        assert_ne!(
            ts_add_overflow.as_deref(),
            Some(rust_overflow_cid.as_str()),
            "TS OP_ADD tag must NOT reference Rust Wrapping CID"
        );
    }

    // Structural: each tag's dimensions are a BTreeMap<String, String> (open-keyed CIDs).
    // Verifies the CID values in tag dimensions are valid blake3-512 prefixed strings.
    #[test]
    fn typescript_tag_dimensions_are_open_keyed_cids() {
        let decl = declaration();
        for tag in &decl.tags {
            for (dimension_name, dimension_cid) in &tag.dimensions {
                assert!(
                    dimension_cid.starts_with("blake3-512:"),
                    "tag {op_cid}: dimension {dimension_name} value CID must start with blake3-512:",
                    op_cid = &tag.op_cid,
                );
                // CID is 512-bit = 128 hex chars + "blake3-512:" prefix (11 chars) = 139 total
                assert_eq!(
                    dimension_cid.len(),
                    139,
                    "tag {op_cid}: dimension {dimension_name} CID has unexpected length {}",
                    dimension_cid.len(),
                    op_cid = &tag.op_cid,
                );
            }
        }
    }
}
