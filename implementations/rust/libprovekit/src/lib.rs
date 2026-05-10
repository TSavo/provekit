// SPDX-License-Identifier: Apache-2.0

pub mod canonical;
pub mod ci;
pub mod compose;
pub mod core;
pub mod ffi;

#[derive(Debug, thiserror::Error)]
pub enum ProvekitError {
    #[error("{0}")]
    Message(String),
}

pub type Result<T> = std::result::Result<T, ProvekitError>;
