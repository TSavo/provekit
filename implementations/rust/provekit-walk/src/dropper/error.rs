// SPDX-License-Identifier: Apache-2.0

use super::template::{DropTemplate, NotRenderable};
use super::emit::EmitResult;

/// Reasons `drop_gap` may fail to produce a verified, closing emission.
///
/// All variants are recoverable and inspectable. The `ClosureVerificationFailed`
/// variant carries the failed `EmitResult` so callers can see the proposed
/// emission even when re-lift didn't confirm DAG closure (useful for
/// debugging the dropper or extending the verifier).
#[derive(Debug, Clone)]
pub enum DropFailure {
    /// The source could not be parsed as a Rust file.
    SourceParseFailed,
    /// The named caller function does not appear in the source.
    CallerNotFound { caller_name: String },
    /// No gap matching the predicate was detected in any walk from the caller.
    NoGapDetected { predicate: String },
    /// The predicate descriptor has no verified template family in this kit.
    UnknownPredicate { predicate: String },
    /// The requested template is not currently a verified candidate for this
    /// predicate. For the MVP, only `DropTemplate::Defensive` is verified.
    TemplateNotCandidate {
        predicate: String,
        requested: DropTemplate,
    },
    /// The template's render path is scaffolding-only (see `NotRenderable`).
    NotRenderable(NotRenderable),
    /// `emit_drop` could not splice the source (parse failure, missing caller,
    /// or out-of-range stmt_index).
    EmitFailed,
    /// The emission was produced but `verify_closure` could not confirm that
    /// the gap is structurally discharged after re-lift. The proposed
    /// `EmitResult` is included for inspection.
    ClosureVerificationFailed { emit: EmitResult },
}

impl std::fmt::Display for DropFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DropFailure::SourceParseFailed => write!(f, "source did not parse as Rust"),
            DropFailure::CallerNotFound { caller_name } => {
                write!(f, "caller `{caller_name}` not found in source")
            }
            DropFailure::NoGapDetected { predicate } => {
                write!(f, "no gap for predicate `{predicate}` detected in any walk")
            }
            DropFailure::UnknownPredicate { predicate } => {
                write!(f, "predicate `{predicate}` has no template family in this kit")
            }
            DropFailure::TemplateNotCandidate { predicate, requested } => {
                write!(
                    f,
                    "template {:?} is not a verified candidate for predicate `{predicate}`",
                    requested
                )
            }
            DropFailure::NotRenderable(e) => write!(f, "{e}"),
            DropFailure::EmitFailed => write!(f, "emit_drop could not splice the source"),
            DropFailure::ClosureVerificationFailed { .. } => {
                write!(f, "emission produced but re-lift did not confirm DAG closure")
            }
        }
    }
}

impl std::error::Error for DropFailure {}
