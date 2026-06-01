// SPDX-License-Identifier: Apache-2.0

//! Substrate identifiers for panic-freedom concepts.
//!
//! These constants intentionally keep the existing Rust v1 wire tokens. The
//! Phase 4 substrate shape audit marks the corresponding concepts as
//! alias-read first, so introducing this module must not change proof bytes.

/// Substrate panic-freedom result-ok predicate; see `SUBSTRATE-SHAPE-AUDIT.md`.
pub const IS_OK: &str = "is_ok";

/// Substrate panic-freedom result-err predicate; see `SUBSTRATE-SHAPE-AUDIT.md`.
pub const IS_ERR: &str = "is_err";

/// Substrate panic-freedom option-some predicate; see `SUBSTRATE-SHAPE-AUDIT.md`.
pub const IS_SOME: &str = "is_some";

/// Substrate panic-freedom option-none predicate; see `SUBSTRATE-SHAPE-AUDIT.md`.
pub const IS_NONE: &str = "is_none";

/// Substrate panic-freedom guarded-branch carrier; see `SUBSTRATE-SHAPE-AUDIT.md`.
pub const CF_GUARDED: &str = "cf_guarded";

/// Substrate panic-freedom guarded-value carrier alias; see `SUBSTRATE-SHAPE-AUDIT.md`.
pub const CF_GUARDED_CONCEPT: &str = "concept:panic-freedom.guard";

/// Substrate panic-freedom control-flow choice carrier; see `SUBSTRATE-SHAPE-AUDIT.md`.
pub const CF_ITE: &str = "cf_ite";

/// Substrate panic-freedom control-flow choice carrier alias; see `SUBSTRATE-SHAPE-AUDIT.md`.
pub const CF_ITE_CONCEPT: &str = "concept:panic-freedom.choice";

/// Substrate panic-freedom unwrap leaf; see `SUBSTRATE-SHAPE-AUDIT.md`.
pub const METHOD_UNWRAP: &str = "method:unwrap";

/// Substrate panic-freedom expect leaf; see `SUBSTRATE-SHAPE-AUDIT.md`.
pub const METHOD_EXPECT: &str = "method:expect";

/// Substrate panic-freedom unwrap-err leaf; see `SUBSTRATE-SHAPE-AUDIT.md`.
pub const METHOD_UNWRAP_ERR: &str = "method:unwrap_err";
