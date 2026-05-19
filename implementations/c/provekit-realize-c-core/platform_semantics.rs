// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::core::types::PlatformSemanticsDeclaration;
use provekit_ir_types::{DimensionValueMemento, IrFormula, IrTerm, PlatformSemanticTag};

const KIT_CID: &str = "blake3-512:dff15254b714e03acf6f72eb8a65465ffc0140a69e538a610201bfa4ae39456bf4d18a5b87a77c64db12b9e9af53f8538ce73468e44367d1a06006a79d6e9830";

const ARITHMETIC_OVERFLOW: &str = "ArithmeticOverflow";
const INTEGER_DIVISION_ROUNDING: &str = "IntegerDivisionRounding";
const SHIFT_MODE: &str = "ShiftMode";
const NULL_SEMANTICS: &str = "NullSemantics";
const BITWISE_SEMANTICS: &str = "BitwiseSemantics";
const SORT_ADMISSION: &str = "SortAdmission";

const UNDEFINED_BEHAVIOR: &str = "UndefinedBehavior";
const TRUNCATE: &str = "Truncate";
const IMPLEMENTATION_DEFINED: &str = "ImplementationDefined";
const TWOS_COMPLEMENT: &str = "TwosComplement";
const CONCEPT_LITERAL_CID: &str = "blake3-512:02804a0bdbd2d5d541544451f41ee8d0d340baf28f70bd5abf5844e87a96aedd7b5ab3453962754a020679cc8c6b3d1f4cf0336a7ad8118128d42ac667abf2d6";

const SORT_BYTES_CID: &str = "blake3-512:7116ef6e62e6739b213a8394f975a53c771b89f08c36d27143827acfcfebc0e39e5b82c530be668c3cfd5ec6966ccaa42930b37fdb1f4ac25652a970be10fb6b";
const SORT_FLOAT_CID: &str = "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57";
const SORT_INT_CID: &str = "blake3-512:30ffc51350121a7172f3e4064a33c45bbd345756979fccff6875cd2ab33e4964d098a99df80cfbdf1ec1a0738c5ac3476f0ff8f75589ea511d1acd82c74ecd58";
const SORT_NULL_CID: &str = "blake3-512:62f6040bd3f414c1e6c2b7bdf276669cd5613b33cb508a81170170064ca3ffba771a4b0002dc52e059fce5f9f63a1874ef71bd4ec89ae06e89c87a3e91aac3b5";
const SORT_STRING_CID: &str = "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10";

#[derive(Clone, Copy)]
struct AdmittedSort {
    name: &'static str,
    cid: &'static str,
}

const C_PLATFORM_SEMANTIC_OP_CIDS: &[&str] = &[
    "blake3-512:95fc70e63a5550fd2e25142f13932919c59d085654ab387789c798886b0111c61d28fe533fc98b50df70eea9428a9af8aa75372c8b1c1deb3acc1a4094790468", // concept:add
    "blake3-512:b7c54558573348bb3a9297732547a8e6e9d152403d292df7426b6bb8a248f705b4b030bf2a22ba547a17d6f1bfaf8e75a6843e02e8f23a8226ebc09e2a8622af", // concept:sub
    "blake3-512:46cd627de058c8d4f7d087ea33f4904af65ad4b2e3cfd3aff8f44bf27db96b33c2dae39cd30f53898c233c9465ba8d2701c69e5903d48935113103b4db00fd03", // concept:mul
    "blake3-512:ad958847b50cf07ddbb92d85ae488a5f983d5619e108476b42e519174cfcce883ecd637544a372b946bb45a1c22893c710bc9b08ea0569ad0e035b3babb6a409", // concept:neg
    "blake3-512:c6a13abbcafdf83edcff49d883a7c7440faadd8af896da0ad46e2bcb177ed0649d005b4ddecd4689cf565b10679219a07c784399bafe5c6174642e1b808d7839", // concept:div
    "blake3-512:92340897b43965e01454b00a6a43ec54b2bf0e01213a45fa2311f730dde18adf8da97a22458c1a2a0fb23ce85ef3ad9b22e704804c74f41997aba3ba02cefe0d", // concept:mod
    "blake3-512:f9cdfcba8d0e223803126504a2a6ed10005fa61acb5c55b74b270bc66d963eb7648ab6763f0510760df93145c0f6670087a403417e8b3100c7142e121111807a", // concept:shl
    "blake3-512:c90e3c159b25e4c4c7f9c899da5aa3ee048a548719ced7360f3e514450811096b21cd5473f22d7a05df088f92210bbc916e65970b9fa1e1511c193ed969f112b", // concept:shr
    "blake3-512:9e96c2445bad6bb1e5a6f902ad7f733e3f4619829b9c0e232361fbf50b978c8332029212ed895762e604d1df009fce58848cda33524a697df798233eae30a14b", // concept:bitand
    "blake3-512:d57b54bffe698ed804a4a49486b73a1a8a3e7bd84fb12babaad01ce22d8b7bcb5a35f3476324063f8de9f8090846d0d4fbeb48d78475d07e16f7925b4f264de3", // concept:bitor
    "blake3-512:343b1f9faa98218467d810e0a2bb1b1eebeaf921c71a1bc52141f885220afff482c631c52e2157a6067640f4830f928add53ef7aa0386c6a27ee3c8bab6dc353", // concept:bitxor
    "blake3-512:5e788f0d551081f4e709e4418e01017fa9ae1c04963e7be2862fadad8a8434fafa204629fbec53e2e44624c195ac2e32c0410df25cf8ff3a4be672582f89109f", // concept:bitnot
    "blake3-512:93ff252a879bc061949fecdb9710a0a927b47f5104f5e628c7e0bd2477e3ea3515ebb2bc2794d9cc7c11c6ea16db511ff20a18c699bb94f7854e79b5e195f717", // concept:deref
    "blake3-512:8c8383c221eaca3b95d30437d768065d5117091415afb04e92f541af6fb26d37af79d423e25a59ffaf3f6e2d654d0bd64cfe8e071ee5483ed6bca2614442001f", // concept:preinc
    "blake3-512:be615743882f980a2fde0ca6ec3250305c28e2fac1fe4d17accd1790d62af7992ff80282f6507335b959ccceaa32a047f1845b8a9e96a54d20b3766d46589aee", // concept:postinc
    "blake3-512:fa83fc84643e03f1e60aa66848412e0cdc25ad6ede0cf216643fb8d4dbe52c4d8df28283f754040cc0f53a62ec22e73a2db623e6507055ab1076df8394024995", // concept:predec
    "blake3-512:cac33b2bef01e38d327440e7bfecebf3e7540d463a02e68dd047e47d0c9cca45f94181ce773fb389671a960cc957760b540b2927afd6d2c624cf9ddaca225f1a", // concept:postdec
];

struct DimensionCids {
    arithmetic_overflow: String,
    integer_division_rounding: String,
    shift_mode: String,
    null_semantics: String,
    bitwise_semantics: String,
    sort_admission: String,
}

impl DimensionCids {
    fn new() -> Self {
        Self {
            arithmetic_overflow: dimension_value(ARITHMETIC_OVERFLOW, UNDEFINED_BEHAVIOR).cid,
            integer_division_rounding: dimension_value(INTEGER_DIVISION_ROUNDING, TRUNCATE).cid,
            shift_mode: dimension_value(SHIFT_MODE, IMPLEMENTATION_DEFINED).cid,
            null_semantics: dimension_value(NULL_SEMANTICS, UNDEFINED_BEHAVIOR).cid,
            bitwise_semantics: dimension_value(BITWISE_SEMANTICS, TWOS_COMPLEMENT).cid,
            sort_admission: sort_admission_value(&[
                AdmittedSort {
                    name: "Int",
                    cid: SORT_INT_CID,
                },
                AdmittedSort {
                    name: "Float",
                    cid: SORT_FLOAT_CID,
                },
                AdmittedSort {
                    name: "String",
                    cid: SORT_STRING_CID,
                },
                AdmittedSort {
                    name: "Bytes",
                    cid: SORT_BYTES_CID,
                },
                AdmittedSort {
                    name: "Null",
                    cid: SORT_NULL_CID,
                },
            ])
            .cid,
        }
    }

    fn dimensions(&self) -> BTreeMap<String, String> {
        BTreeMap::from([
            (
                ARITHMETIC_OVERFLOW.to_string(),
                self.arithmetic_overflow.clone(),
            ),
            (
                INTEGER_DIVISION_ROUNDING.to_string(),
                self.integer_division_rounding.clone(),
            ),
            (SHIFT_MODE.to_string(), self.shift_mode.clone()),
            (NULL_SEMANTICS.to_string(), self.null_semantics.clone()),
            (
                BITWISE_SEMANTICS.to_string(),
                self.bitwise_semantics.clone(),
            ),
        ])
    }
}

pub fn declaration() -> PlatformSemanticsDeclaration {
    let dimensions = DimensionCids::new();
    let mut tags = C_PLATFORM_SEMANTIC_OP_CIDS
        .iter()
        .map(|op_cid| {
            PlatformSemanticTag::new(
                KIT_CID.to_string(),
                (*op_cid).to_string(),
                dimensions.dimensions(),
            )
        })
        .collect::<Vec<_>>();
    tags.push(PlatformSemanticTag::new(
        KIT_CID.to_string(),
        CONCEPT_LITERAL_CID.to_string(),
        BTreeMap::from([(SORT_ADMISSION.to_string(), dimensions.sort_admission)]),
    ));
    PlatformSemanticsDeclaration {
        tags,
        dimension_values: dimension_values(),
        op_aliases: BTreeMap::new(),
    }
}

pub fn dimension_values() -> Vec<DimensionValueMemento> {
    vec![
        dimension_value(ARITHMETIC_OVERFLOW, UNDEFINED_BEHAVIOR),
        dimension_value(INTEGER_DIVISION_ROUNDING, TRUNCATE),
        dimension_value(SHIFT_MODE, IMPLEMENTATION_DEFINED),
        dimension_value(NULL_SEMANTICS, UNDEFINED_BEHAVIOR),
        dimension_value(BITWISE_SEMANTICS, TWOS_COMPLEMENT),
        sort_admission_value(&[
            AdmittedSort {
                name: "Int",
                cid: SORT_INT_CID,
            },
            AdmittedSort {
                name: "Float",
                cid: SORT_FLOAT_CID,
            },
            AdmittedSort {
                name: "String",
                cid: SORT_STRING_CID,
            },
            AdmittedSort {
                name: "Bytes",
                cid: SORT_BYTES_CID,
            },
            AdmittedSort {
                name: "Null",
                cid: SORT_NULL_CID,
            },
        ]),
    ]
}

fn dimension_value(dimension_name: &str, value_name: &str) -> DimensionValueMemento {
    DimensionValueMemento::new(
        KIT_CID.to_string(),
        dimension_name.to_string(),
        value_name.to_string(),
        IrFormula::Atomic {
            name: format!("c:{value_name}"),
            args: vec![],
        },
    )
}

fn sort_admission_value(admitted: &[AdmittedSort]) -> DimensionValueMemento {
    let mut admitted = admitted.to_vec();
    admitted.sort_by(|left, right| left.cid.cmp(right.cid));
    let value_name = admitted
        .iter()
        .map(|sort| sort.name)
        .collect::<Vec<_>>()
        .join("");
    DimensionValueMemento::new(
        KIT_CID.to_string(),
        SORT_ADMISSION.to_string(),
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
