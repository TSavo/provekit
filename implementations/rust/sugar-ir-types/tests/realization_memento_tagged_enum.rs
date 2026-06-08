// SPDX-License-Identifier: Apache-2.0
//
// Tagged enum tests for RealizationMemento.

use sugar_ir_types::{
    BoundaryRealization, CompositionRealization, FirstClassRealization, RealizationMemento,
    RealizationValidationError, SugarCarrierRealization,
};

const CID_1: &str = "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111";
const CID_2: &str = "blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222";

const FIRST_CLASS_JSON: &str =
    r#"{"kind":"first-class","surface_locator":"expression","syntactic_pattern":"${x} + ${y}"}"#;
const FIRST_CLASS_CID: &str = "blake3-512:acdf3eb35ee4650940aeacc8a98007a2d28c797e7ff2da361b74e31d0da23b34d0e2d285013f41890ff3b1a6e8f756b98832cb96e4331548988b32149a401205";

const COMPOSITION_JSON: &str = r#"{"composition_tree_cid":"blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111","kind":"composition"}"#;
const COMPOSITION_CID: &str = "blake3-512:0a20ad99956b4c18f0995dcd18f55b182838c1743708f95ec386b3dce25ee1b9e9c5b9feeb7bd472633b873b590b2fd27cd4d84573b5af15a5f7ef1bb376806e";

const BOUNDARY_JSON: &str = r#"{"api":"requests.get","boundary_contract_cid":"blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222","kind":"boundary","library":"python-requests"}"#;
const BOUNDARY_CID: &str = "blake3-512:f19a3b0940968d46c9db8d608534c5e66513e3d3c854bd48bbbbe7456f564630ab6adc5c42e88a2a310f361589b91ee538363aea581f8a8536132440860268e4";

const SUGAR_CARRIER_JSON: &str = r#"{"kind":"sugar-carrier"}"#;
const SUGAR_CARRIER_CID: &str = "blake3-512:9cad6b2857c8c51495787c2c91bdbf890af3c99656d7dac721dc5267445872ed369407e8ca36c05cc0b2ae93821ea2978ea04d18cb9fd7cb89190dafc85530ae";

fn first_class() -> RealizationMemento {
    RealizationMemento::FirstClass(FirstClassRealization {
        syntactic_pattern: "${x} + ${y}".to_string(),
        surface_locator: "expression".to_string(),
    })
}

fn composition() -> RealizationMemento {
    RealizationMemento::Composition(CompositionRealization {
        composition_tree_cid: CID_1.to_string(),
    })
}

fn boundary() -> RealizationMemento {
    RealizationMemento::Boundary(BoundaryRealization {
        library: "python-requests".to_string(),
        api: "requests.get".to_string(),
        boundary_contract_cid: CID_2.to_string(),
    })
}

fn sugar_carrier() -> RealizationMemento {
    RealizationMemento::SugarCarrier(SugarCarrierRealization {})
}

#[test]
fn round_trips_first_class_realization() {
    let memento = first_class();

    let serialized = serde_json::to_string(&memento).expect("serialize");

    assert_eq!(serialized, FIRST_CLASS_JSON);
    assert_eq!(
        serde_json::from_str::<RealizationMemento>(&serialized).expect("parse"),
        memento
    );
}

#[test]
fn round_trips_composition_realization() {
    let memento = composition();

    let serialized = serde_json::to_string(&memento).expect("serialize");

    assert_eq!(serialized, COMPOSITION_JSON);
    assert_eq!(
        serde_json::from_str::<RealizationMemento>(&serialized).expect("parse"),
        memento
    );
}

#[test]
fn round_trips_boundary_realization() {
    let memento = boundary();

    let serialized = serde_json::to_string(&memento).expect("serialize");

    assert_eq!(serialized, BOUNDARY_JSON);
    assert_eq!(
        serde_json::from_str::<RealizationMemento>(&serialized).expect("parse"),
        memento
    );
}

#[test]
fn round_trips_sugar_carrier_realization() {
    let memento = sugar_carrier();

    let serialized = serde_json::to_string(&memento).expect("serialize");

    assert_eq!(serialized, SUGAR_CARRIER_JSON);
    assert_eq!(
        serde_json::from_str::<RealizationMemento>(&serialized).expect("parse"),
        memento
    );
}

#[test]
fn emits_jcs_canonical_bytes_per_variant() {
    assert_eq!(
        first_class().to_jcs_string().expect("first class JCS"),
        FIRST_CLASS_JSON
    );
    assert_eq!(
        composition().to_jcs_string().expect("composition JCS"),
        COMPOSITION_JSON
    );
    assert_eq!(
        boundary().to_jcs_string().expect("boundary JCS"),
        BOUNDARY_JSON
    );
    assert_eq!(
        sugar_carrier().to_jcs_string().expect("sugar carrier JCS"),
        SUGAR_CARRIER_JSON
    );
}

#[test]
fn recomputes_pinned_cids_per_variant() {
    assert_eq!(
        first_class().recompute_cid().expect("first class cid"),
        FIRST_CLASS_CID
    );
    assert_eq!(
        composition().recompute_cid().expect("composition cid"),
        COMPOSITION_CID
    );
    assert_eq!(
        boundary().recompute_cid().expect("boundary cid"),
        BOUNDARY_CID
    );
    assert_eq!(
        sugar_carrier().recompute_cid().expect("sugar carrier cid"),
        SUGAR_CARRIER_CID
    );
}

#[test]
fn kind_field_discriminates_overlapping_data() {
    let first_class = RealizationMemento::FirstClass(FirstClassRealization {
        syntactic_pattern: "requests.get".to_string(),
        surface_locator: "python-requests".to_string(),
    });
    let boundary = RealizationMemento::Boundary(BoundaryRealization {
        library: "python-requests".to_string(),
        api: "requests.get".to_string(),
        boundary_contract_cid: CID_2.to_string(),
    });

    assert_ne!(
        first_class.recompute_cid().expect("first class cid"),
        boundary.recompute_cid().expect("boundary cid")
    );
}

#[test]
fn legacy_catalog_realization_parses_as_boundary() {
    let text = format!(
        r#"{{
            "cid": "{CID_2}",
            "memento": {{
                "source_lang": "c11",
                "target_form": "concept:double-dispatch->c11:2d-fn-ptr-table"
            }}
        }}"#
    );

    let parsed: RealizationMemento = serde_json::from_str(&text).expect("parse legacy fixture");

    let RealizationMemento::Boundary(boundary) = parsed else {
        panic!("legacy fixture should classify as Boundary");
    };
    assert_eq!(boundary.library, "c11");
    assert_eq!(boundary.api, "concept:double-dispatch->c11:2d-fn-ptr-table");
    assert_eq!(boundary.boundary_contract_cid, CID_2);
}

#[test]
fn validates_per_variant_rules() {
    assert_eq!(
        RealizationMemento::FirstClass(FirstClassRealization {
            syntactic_pattern: String::new(),
            surface_locator: "expression".to_string(),
        })
        .validate(),
        Err(RealizationValidationError::EmptySyntacticPattern)
    );
    assert_eq!(
        RealizationMemento::Composition(CompositionRealization {
            composition_tree_cid: "not-a-cid".to_string(),
        })
        .validate(),
        Err(RealizationValidationError::InvalidCompositionTreeCid {
            composition_tree_cid: "not-a-cid".to_string(),
        })
    );
    assert_eq!(
        RealizationMemento::Boundary(BoundaryRealization {
            library: String::new(),
            api: "requests.get".to_string(),
            boundary_contract_cid: CID_2.to_string(),
        })
        .validate(),
        Err(RealizationValidationError::EmptyBoundaryLibrary)
    );
    assert_eq!(
        RealizationMemento::Boundary(BoundaryRealization {
            library: "python-requests".to_string(),
            api: String::new(),
            boundary_contract_cid: CID_2.to_string(),
        })
        .validate(),
        Err(RealizationValidationError::EmptyBoundaryApi)
    );
    assert_eq!(
        RealizationMemento::Boundary(BoundaryRealization {
            library: "python-requests".to_string(),
            api: "requests.get".to_string(),
            boundary_contract_cid: String::new(),
        })
        .validate(),
        Err(RealizationValidationError::EmptyBoundaryContractCid)
    );
    assert_eq!(
        RealizationMemento::Boundary(BoundaryRealization {
            library: "python-requests".to_string(),
            api: "requests.get".to_string(),
            boundary_contract_cid: "not-a-cid".to_string(),
        })
        .validate(),
        Err(RealizationValidationError::InvalidBoundaryContractCid {
            boundary_contract_cid: "not-a-cid".to_string(),
        })
    );

    first_class().validate().expect("first class valid");
    composition().validate().expect("composition valid");
    boundary().validate().expect("boundary valid");
    sugar_carrier().validate().expect("sugar carrier valid");
}
