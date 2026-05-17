// SPDX-License-Identifier: Apache-2.0

use crate::core::types::PlatformSemanticsDeclaration;

mod python_common;
pub mod java;
pub mod python_lift_source;
pub mod python_realize_core;

mod c_realize_core {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../c/provekit-realize-c-core/platform_semantics.rs"
    ));
}

/// Returns the PlatformSemanticsDeclaration for the given lower-target language,
/// or None when no kit has declared semantics for that target.
///
/// Per the open-keyed schema ruling at
/// `docs/plans/2026-05-16-platform-semantic-tag-schema-ruling.md`, kits mint
/// their own dimension names and value mementos. This dispatcher is the
/// CLI-side entry point that picks the right declaration to register with
/// ConformanceDeclaration::Carrier at kit-registration time.
///
/// Stage 2.5 shipped this dispatcher returning None for all targets. Stage 3.1
/// per-kit dispatches populate match arms as each kit's
/// PlatformSemanticsDeclaration lands.
pub fn platform_semantics_for_lower_target(target: &str) -> Option<PlatformSemanticsDeclaration> {
    match target {
        "python" => Some(python_kit_declaration()),
        "rust" => {
            let declaration = provekit_realize_rust_core::platform_semantics::declaration();
            Some(PlatformSemanticsDeclaration {
                tags: declaration.tags,
            })
        }
        "java" => Some(java::declaration()),
        "c" => Some(c_realize_core::declaration()),
        _ => None,
    }
}

pub fn python_kit_declaration() -> PlatformSemanticsDeclaration {
    python_realize_core::declaration()
}
