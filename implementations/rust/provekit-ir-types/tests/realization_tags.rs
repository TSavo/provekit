// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use provekit_ir_types::realization_tags::{
    tag_boundary, tag_composition, tag_first_class, tag_sugar_carrier,
};
use provekit_ir_types::{
    BoundaryRealization, CompositionRealization, FirstClassRealization, RealizationMemento,
    SugarCarrierRealization,
};

const COMPOSITION_TREE_CID: &str = "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111";
const BOUNDARY_CONTRACT_CID: &str = "blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222";

#[test]
fn tag_first_class_builds_first_class_realization() {
    let realization = tag_first_class("concept:add", "${x} + ${y}", "binary-operator");

    assert_eq!(
        realization,
        RealizationMemento::FirstClass(FirstClassRealization {
            syntactic_pattern: "${x} + ${y}".to_string(),
            surface_locator: "binary-operator".to_string(),
        })
    );
}

#[test]
fn tag_composition_builds_composition_realization() {
    let realization = tag_composition("concept:list", COMPOSITION_TREE_CID);

    assert_eq!(
        realization,
        RealizationMemento::Composition(CompositionRealization {
            composition_tree_cid: COMPOSITION_TREE_CID.to_string(),
        })
    );
}

#[test]
fn tag_boundary_builds_boundary_realization() {
    let realization = tag_boundary(
        "concept:http-request",
        "python-requests",
        "requests.get",
        BOUNDARY_CONTRACT_CID,
    );

    assert_eq!(
        realization,
        RealizationMemento::Boundary(BoundaryRealization {
            library: "python-requests".to_string(),
            api: "requests.get".to_string(),
            boundary_contract_cid: BOUNDARY_CONTRACT_CID.to_string(),
        })
    );
}

#[test]
fn tag_sugar_carrier_builds_sugar_carrier_realization() {
    let realization = tag_sugar_carrier("concept:free");

    assert_eq!(
        realization,
        RealizationMemento::SugarCarrier(SugarCarrierRealization {})
    );
}

#[test]
fn tagging_primitives_have_distinct_cids() {
    let realizations = [
        tag_first_class("concept:add", "${x} + ${y}", "binary-operator"),
        tag_composition("concept:add", COMPOSITION_TREE_CID),
        tag_boundary(
            "concept:add",
            "python-requests",
            "requests.get",
            BOUNDARY_CONTRACT_CID,
        ),
        tag_sugar_carrier("concept:add"),
    ];
    let cids = realizations
        .iter()
        .map(|realization| realization.recompute_cid().expect("cid"))
        .collect::<HashSet<_>>();

    assert_eq!(cids.len(), realizations.len());
}
