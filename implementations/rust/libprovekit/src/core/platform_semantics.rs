// SPDX-License-Identifier: Apache-2.0

use crate::core::types::PlatformSemanticsDeclaration;

/// Returns the PlatformSemanticsDeclaration for the given lower-target language,
/// or None when no kit has declared semantics for that target.
///
/// Per the open-keyed schema ruling at
/// `docs/plans/2026-05-16-platform-semantic-tag-schema-ruling.md`, kits mint
/// their own dimension names and value mementos. This dispatcher is the
/// CLI-side entry point that picks the right declaration to register with
/// ConformanceDeclaration::Carrier at kit-registration time.
///
/// Stage 2.5 ships this dispatcher returning None for ALL targets. Stage 3.1
/// per-kit dispatches will populate the match arms as each kit's
/// PlatformSemanticsDeclaration lands.
pub fn platform_semantics_for_lower_target(target: &str) -> Option<PlatformSemanticsDeclaration> {
    let _ = target;
    None
}
