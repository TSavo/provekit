// SPDX-License-Identifier: Apache-2.0

use crate::{
    BoundaryRealization, CompositionRealization, FirstClassRealization, RealizationMemento,
    SugarCarrierRealization,
};

/// Tag a concept op with a language-native syntactic form.
///
/// # Example
///
/// ```
/// use sugar_ir_types::realization_tags::tag_first_class;
///
/// let realization = tag_first_class(
///     "concept:add",
///     "${x} + ${y}",
///     "binary-operator",
/// );
/// // Returns RealizationMemento::FirstClass(...)
/// ```
pub fn tag_first_class(
    op_name: &str,
    syntactic_pattern: &str,
    surface_locator: &str,
) -> RealizationMemento {
    let _ = op_name;
    RealizationMemento::FirstClass(FirstClassRealization {
        syntactic_pattern: syntactic_pattern.to_string(),
        surface_locator: surface_locator.to_string(),
    })
}

/// Tag a concept op with a content-addressed composition tree.
///
/// # Example
///
/// ```
/// use sugar_ir_types::realization_tags::tag_composition;
///
/// let realization = tag_composition(
///     "concept:list",
///     "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
/// );
/// // Returns RealizationMemento::Composition(...)
/// ```
pub fn tag_composition(op_name: &str, composition_tree: &str) -> RealizationMemento {
    let _ = op_name;
    RealizationMemento::Composition(CompositionRealization {
        composition_tree_cid: composition_tree.to_string(),
    })
}

/// Tag a concept op with a library or API boundary contract.
///
/// # Example
///
/// ```
/// use sugar_ir_types::realization_tags::tag_boundary;
///
/// let realization = tag_boundary(
///     "concept:http-request",
///     "python-requests",
///     "requests.get",
///     "blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222",
/// );
/// // Returns RealizationMemento::Boundary(...)
/// ```
pub fn tag_boundary(
    op_name: &str,
    library: &str,
    api: &str,
    boundary_contract_cid: &str,
) -> RealizationMemento {
    let _ = op_name;
    RealizationMemento::Boundary(BoundaryRealization {
        library: library.to_string(),
        api: api.to_string(),
        boundary_contract_cid: boundary_contract_cid.to_string(),
    })
}

/// Tag a concept op as a concept-citation sugar carrier.
///
/// # Example
///
/// ```
/// use sugar_ir_types::realization_tags::tag_sugar_carrier;
///
/// let realization = tag_sugar_carrier("concept:free");
/// // Returns RealizationMemento::SugarCarrier(...)
/// ```
pub fn tag_sugar_carrier(op_name: &str) -> RealizationMemento {
    let _ = op_name;
    RealizationMemento::SugarCarrier(SugarCarrierRealization {})
}
