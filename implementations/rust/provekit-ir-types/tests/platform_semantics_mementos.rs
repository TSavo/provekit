// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use provekit_ir_types::{DimensionValueMemento, IrFormula, PlatformSemanticTag};

const KIT_CID: &str = "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111";
const OP_CID: &str = "blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222";
const WRAPPING_CID: &str = "blake3-512:db0ba7a27a32c677aa59b79b8f9ba261f6568995ded6c4f68fc30539d6dedc7c1744f14acdb31c87f9ae75ef38bf519d7d0b7074f714b96a78ed359ac906cb2e";
const TRUNCATE_CID: &str = "blake3-512:03d9358f650200c8c8682e0535a391013fc7750b33ca7f55b22f31714193a493d61e96a135a6dac7e522dd2b13715597fd3300d1532b65a091e07bdaa7953f13";
const TAG_CID: &str = "blake3-512:6740c2388258930b896950b5cffd0c542a3f2ef4ba288fa422d57655f3488fd651b73de384485ea2f3dd08f64fe7d5c7572f29ca6f213b0118cc8da039afe992";

fn true_formula() -> IrFormula {
    IrFormula::Atomic {
        name: "true".to_string(),
        args: vec![],
    }
}

#[test]
fn dimension_value_memento_round_trips_and_recomputes_cid() {
    let value = DimensionValueMemento::new(
        KIT_CID.to_string(),
        "ArithmeticOverflowMode".to_string(),
        "Wrapping".to_string(),
        true_formula(),
    );

    let encoded = value.to_jcs_string();
    let decoded: DimensionValueMemento =
        serde_json::from_str(&encoded).expect("dimension value decodes");

    assert_eq!(decoded, value);
    assert_eq!(decoded.cid, decoded.recompute_cid());
    assert_eq!(decoded.cid, WRAPPING_CID);
}

#[test]
fn platform_semantic_tag_round_trips_and_recomputes_cid() {
    let wrapping = DimensionValueMemento::new(
        KIT_CID.to_string(),
        "ArithmeticOverflowMode".to_string(),
        "Wrapping".to_string(),
        true_formula(),
    );
    let truncate = DimensionValueMemento::new(
        KIT_CID.to_string(),
        "IntegerDivisionRoundingMode".to_string(),
        "Truncate".to_string(),
        true_formula(),
    );
    let mut dimensions = BTreeMap::new();
    dimensions.insert(
        "IntegerDivisionRoundingMode".to_string(),
        truncate.cid.clone(),
    );
    dimensions.insert("ArithmeticOverflowMode".to_string(), wrapping.cid.clone());

    let tag = PlatformSemanticTag::new(KIT_CID.to_string(), OP_CID.to_string(), dimensions);

    let encoded = tag.to_jcs_string();
    let decoded: PlatformSemanticTag = serde_json::from_str(&encoded).expect("tag decodes");

    assert_eq!(wrapping.cid, WRAPPING_CID);
    assert_eq!(truncate.cid, TRUNCATE_CID);
    assert_eq!(decoded, tag);
    assert_eq!(decoded.cid, decoded.recompute_cid());
    assert_eq!(decoded.cid, TAG_CID);
    assert!(
        encoded
            .find("ArithmeticOverflowMode")
            .expect("overflow key")
            < encoded
                .find("IntegerDivisionRoundingMode")
                .expect("division key")
    );
}
