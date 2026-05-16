// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};

use provekit_ir_types::{
    composition_refusal_compose_input_cid, composition_refusal_header_cid,
    composition_refusal_signature, CompositionRefusalEnvelope, CompositionRefusalHeader,
    CompositionRefusalMemento, CompositionRefusalMetadata, MissingRequirement,
};
use thiserror::Error;

use crate::compose::CCP_VERSION;

use super::traits::{InputCatalog, Kit, KitError};
use super::types::{Cid, DomainClaim, Input, PathAlgebra, PathError, Term};

/// Registry of executable kits keyed by the `PathAlgebra.kit` selector.
#[derive(Default)]
pub struct KitRegistry {
    kits: HashMap<String, Box<dyn Kit>>,
}

impl KitRegistry {
    /// Register a kit under the exact path selector.
    pub fn register(&mut self, name: impl Into<String>, kit: impl Kit + 'static) {
        self.kits.insert(name.into(), Box::new(kit));
    }

    /// Borrow a registered kit by selector.
    pub fn get(&self, name: &str) -> Option<&dyn Kit> {
        self.kits.get(name).map(Box::as_ref)
    }
}

/// Execute a composed path by materializing step inputs and dispatching registered kits.
pub fn execute_path(
    input: &Input,
    registry: &KitRegistry,
    inputs: &dyn InputCatalog,
) -> Result<DomainClaim, PathExecutionError> {
    let Input::Path(path) = input else {
        return Err(PathExecutionError::UnsupportedInput(
            "execute_path expects Input::Path".to_string(),
        ));
    };
    let ordered = path.ordered_steps().map_err(PathExecutionError::Path)?;
    let mut claims_by_step: BTreeMap<String, DomainClaim> = BTreeMap::new();
    let mut materialized_terms: HashMap<Cid, Term> = HashMap::new();

    for step in ordered {
        let kit = registry
            .get(&step.kit)
            .ok_or_else(|| PathExecutionError::Refused(Box::new(missing_kit_refusal(step))))?;
        let step_input = step_input(step, inputs, &materialized_terms)?;
        let claim = kit
            .transform(&step_input)
            .map_err(PathExecutionError::Kit)?;
        if let Some(term) = claim.payload.clone() {
            materialized_terms.insert(claim.to.clone(), term);
        }
        claims_by_step.insert(step.name.clone(), claim);
    }

    let terminals = path.terminal_steps();
    let [terminal] = terminals.as_slice() else {
        return Err(PathExecutionError::UnsupportedInput(format!(
            "execute_path expects exactly one terminal step, got {}",
            terminals.len()
        )));
    };
    claims_by_step
        .remove(&terminal.name)
        .ok_or_else(|| PathExecutionError::UnsupportedInput("terminal step did not execute".into()))
}

fn step_input(
    step: &PathAlgebra,
    inputs: &dyn InputCatalog,
    materialized_terms: &HashMap<Cid, Term>,
) -> Result<Input, PathExecutionError> {
    let [cid] = step.inputs.as_slice() else {
        return Err(PathExecutionError::UnsupportedInput(format!(
            "path step `{}` expects exactly one input CID, got {}",
            step.name,
            step.inputs.len()
        )));
    };
    if let Some(input) = inputs.get_input(cid) {
        return Ok(input);
    }
    if let Some(term) = materialized_terms.get(cid) {
        return Ok(Input::Term(term.clone()));
    }
    Err(PathExecutionError::Refused(Box::new(
        missing_input_refusal(step, cid),
    )))
}

/// Errors from path execution.
#[derive(Debug, Error)]
pub enum PathExecutionError {
    /// The path or step shape is outside the current executor contract.
    #[error("{0}")]
    UnsupportedInput(String),
    /// The path algebra itself is invalid.
    #[error(transparent)]
    Path(#[from] PathError),
    /// The target kit failed.
    #[error(transparent)]
    Kit(#[from] KitError),
    /// Execution refused with a durable composition-refusal memento.
    #[error("path execution refused")]
    Refused(Box<CompositionRefusalMemento>),
}

impl PathExecutionError {
    /// Borrow the durable refusal memento when this error is a refusal.
    pub fn composition_refusal(&self) -> Option<&CompositionRefusalMemento> {
        match self {
            Self::Refused(refusal) => Some(refusal),
            _ => None,
        }
    }
}

fn missing_kit_refusal(step: &PathAlgebra) -> CompositionRefusalMemento {
    composition_refusal_for_missing_requirement(
        step,
        "kit-registry",
        "plugin-registry",
        format!(
            "path step `{}` references unregistered kit `{}`",
            step.name, step.kit
        ),
    )
}

fn missing_input_refusal(step: &PathAlgebra, cid: &Cid) -> CompositionRefusalMemento {
    composition_refusal_for_missing_requirement(
        step,
        "input-catalog",
        "path-input",
        format!(
            "path step `{}` input `{cid}` is not materialized and no prior term payload produced it",
            step.name
        ),
    )
}

fn composition_refusal_for_missing_requirement(
    step: &PathAlgebra,
    role: &str,
    memento_kind: &str,
    reason: String,
) -> CompositionRefusalMemento {
    let atoms_cids: Vec<String> = step.inputs.iter().map(ToString::to_string).collect();
    let effect_set_cids = Vec::new();
    let compose_input_cid =
        composition_refusal_compose_input_cid(&atoms_cids, &effect_set_cids, CCP_VERSION);
    let mut header = CompositionRefusalHeader {
        atoms_cids,
        blocking_effects: None,
        ccp_version: CCP_VERSION.to_string(),
        cid: String::new(),
        compose_input_cid,
        effect_occurrences: Some(vec![]),
        effect_set_cids,
        failure_detail: reason.clone(),
        failure_kind: "memento-required-missing".to_string(),
        incompatible_pair: None,
        kind: "composition-refusal".to_string(),
        missing_memento_requirements: Some(vec![MissingRequirement {
            expected_cid: None,
            memento_kind: Some(memento_kind.to_string()),
            reason,
            role: Some(role.to_string()),
        }]),
        schema_version: "1".to_string(),
    };
    header.cid = composition_refusal_header_cid(&header);
    let metadata = CompositionRefusalMetadata::default();
    let signature = composition_refusal_signature(&header, &metadata);
    CompositionRefusalMemento {
        envelope: CompositionRefusalEnvelope {
            declared_at: "1970-01-01T00:00:00Z".to_string(),
            signature,
            signer: "substrate:libprovekit".to_string(),
        },
        header,
        metadata,
    }
}
