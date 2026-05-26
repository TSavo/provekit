// SPDX-License-Identifier: Apache-2.0

pub mod canonical;
pub mod compose;
pub mod core;
pub mod desugar;
pub mod effect_propagation;
pub mod exam_manifest;
pub mod ffi;
pub mod policy_profile_registry;
pub mod promotion_decision_registry;
pub mod proofir_bridge;
pub mod substrate_default_cids;
pub mod transport;
pub mod witness_registry;
pub mod wp;

pub use exam_manifest::ExamManifestKit;
pub use proofir_bridge::{proofir_resolve, proofir_unresolve};

#[derive(Debug, thiserror::Error)]
pub enum ProvekitError {
    #[error("{0}")]
    Message(String),
}

pub type Result<T> = std::result::Result<T, ProvekitError>;
