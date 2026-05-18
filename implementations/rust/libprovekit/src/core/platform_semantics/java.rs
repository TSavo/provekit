// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use provekit_canonicalizer::blake3_512_of;
use provekit_ir_types::{DimensionValueMemento, IrFormula, PlatformSemanticTag};

use crate::core::types::PlatformSemanticsDeclaration;

const KIT_ID: &str = "provekit-realize-java-core@0.1.0";

const CONCEPT_ADD_CID: &str = "blake3-512:95fc70e63a5550fd2e25142f13932919c59d085654ab387789c798886b0111c61d28fe533fc98b50df70eea9428a9af8aa75372c8b1c1deb3acc1a4094790468";
const CONCEPT_SUB_CID: &str = "blake3-512:b7c54558573348bb3a9297732547a8e6e9d152403d292df7426b6bb8a248f705b4b030bf2a22ba547a17d6f1bfaf8e75a6843e02e8f23a8226ebc09e2a8622af";
const CONCEPT_MUL_CID: &str = "blake3-512:46cd627de058c8d4f7d087ea33f4904af65ad4b2e3cfd3aff8f44bf27db96b33c2dae39cd30f53898c233c9465ba8d2701c69e5903d48935113103b4db00fd03";
const CONCEPT_NEG_CID: &str = "blake3-512:ad958847b50cf07ddbb92d85ae488a5f983d5619e108476b42e519174cfcce883ecd637544a372b946bb45a1c22893c710bc9b08ea0569ad0e035b3babb6a409";
const CONCEPT_DIV_CID: &str = "blake3-512:c6a13abbcafdf83edcff49d883a7c7440faadd8af896da0ad46e2bcb177ed0649d005b4ddecd4689cf565b10679219a07c784399bafe5c6174642e1b808d7839";
const CONCEPT_MOD_CID: &str = "blake3-512:92340897b43965e01454b00a6a43ec54b2bf0e01213a45fa2311f730dde18adf8da97a22458c1a2a0fb23ce85ef3ad9b22e704804c74f41997aba3ba02cefe0d";
const CONCEPT_SHL_CID: &str = "blake3-512:f9cdfcba8d0e223803126504a2a6ed10005fa61acb5c55b74b270bc66d963eb7648ab6763f0510760df93145c0f6670087a403417e8b3100c7142e121111807a";
const CONCEPT_SHR_CID: &str = "blake3-512:c90e3c159b25e4c4c7f9c899da5aa3ee048a548719ced7360f3e514450811096b21cd5473f22d7a05df088f92210bbc916e65970b9fa1e1511c193ed969f112b";
const CONCEPT_USHR_CID: &str = "blake3-512:5746cb4f8bb8d713624731661de51e851e7ca65dae10a88bae4727d1e0070525be77e9919d90939264acaf4c093b00808862e6d0d2c24ac05262ce95cd67c8ad";
const CONCEPT_BITNOT_CID: &str = "blake3-512:5e788f0d551081f4e709e4418e01017fa9ae1c04963e7be2862fadad8a8434fafa204629fbec53e2e44624c195ac2e32c0410df25cf8ff3a4be672582f89109f";

pub fn declaration() -> PlatformSemanticsDeclaration {
    let kit_cid = blake3_512_of(KIT_ID.as_bytes());
    let values = dimension_values_for_kit(&kit_cid);
    let value_cids = values
        .iter()
        .map(|value| (value.value_name.clone(), value.cid.clone()))
        .collect::<BTreeMap<_, _>>();

    PlatformSemanticsDeclaration {
        tags: vec![
            tag(
                &kit_cid,
                CONCEPT_ADD_CID,
                &[("ArithmeticOverflow", value_cids["Wrapping"].as_str())],
            ),
            tag(
                &kit_cid,
                CONCEPT_SUB_CID,
                &[("ArithmeticOverflow", value_cids["Wrapping"].as_str())],
            ),
            tag(
                &kit_cid,
                CONCEPT_MUL_CID,
                &[("ArithmeticOverflow", value_cids["Wrapping"].as_str())],
            ),
            tag(
                &kit_cid,
                CONCEPT_NEG_CID,
                &[("ArithmeticOverflow", value_cids["Wrapping"].as_str())],
            ),
            tag(
                &kit_cid,
                CONCEPT_DIV_CID,
                &[
                    ("IntegerDivisionRounding", value_cids["Truncate"].as_str()),
                    (
                        "NullSemantics",
                        value_cids["ThrowArithmeticException"].as_str(),
                    ),
                ],
            ),
            tag(
                &kit_cid,
                CONCEPT_MOD_CID,
                &[
                    ("IntegerDivisionRounding", value_cids["Truncate"].as_str()),
                    (
                        "NullSemantics",
                        value_cids["ThrowArithmeticException"].as_str(),
                    ),
                ],
            ),
            tag(
                &kit_cid,
                CONCEPT_SHL_CID,
                &[("BitwiseSemantics", value_cids["TwosComplement"].as_str())],
            ),
            tag(
                &kit_cid,
                CONCEPT_SHR_CID,
                &[
                    ("BitwiseSemantics", value_cids["TwosComplement"].as_str()),
                    ("ShiftMode", value_cids["Arithmetic"].as_str()),
                ],
            ),
            tag(
                &kit_cid,
                CONCEPT_USHR_CID,
                &[
                    ("BitwiseSemantics", value_cids["TwosComplement"].as_str()),
                    ("ShiftMode", value_cids["Logical"].as_str()),
                ],
            ),
            tag(
                &kit_cid,
                CONCEPT_BITNOT_CID,
                &[("BitwiseSemantics", value_cids["TwosComplement"].as_str())],
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
        dimension_value(kit_cid, "ArithmeticOverflow", "Wrapping"),
        dimension_value(kit_cid, "IntegerDivisionRounding", "Truncate"),
        dimension_value(kit_cid, "ShiftMode", "Arithmetic"),
        dimension_value(kit_cid, "ShiftMode", "Logical"),
        dimension_value(kit_cid, "NullSemantics", "ThrowArithmeticException"),
        dimension_value(kit_cid, "BitwiseSemantics", "TwosComplement"),
    ]
}

fn dimension_value(kit_cid: &str, dimension_name: &str, value_name: &str) -> DimensionValueMemento {
    DimensionValueMemento::new(
        kit_cid.to_string(),
        dimension_name.to_string(),
        value_name.to_string(),
        IrFormula::Atomic {
            name: format!("java:{value_name}"),
            args: vec![],
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
