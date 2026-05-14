// SPDX-License-Identifier: Apache-2.0

pub mod canonical;
pub mod ci;
pub mod compose;
pub mod core;
pub mod desugar;
pub mod effect_propagation;
pub mod ffi;
pub mod promotion_decision_registry;
pub mod substrate_default_cids;
pub mod transport;
pub mod witness_registry;
pub mod wp;

#[derive(Debug, thiserror::Error)]
pub enum ProvekitError {
    #[error("{0}")]
    Message(String),
}

pub type Result<T> = std::result::Result<T, ProvekitError>;
