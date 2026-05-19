// SPDX-License-Identifier: Apache-2.0
//
// Dispatcher for per-target PlatformSemanticsDeclarations.
//
// Per #1270: the kit IS the authority on its platform semantics. This
// dispatcher MUST NOT carry a hardcoded Rust mirror of any kit's declaration.
// All declarations are loaded from each kit's binary via JSON-RPC
// (provekit.plugin.platform_semantics, PEP 1.7.0). See
// `core::platform_semantics_loader` for the loader.
//
// The previous version of this module carried hardcoded Rust modules for
// typescript/java/python/python_sqlite3/python_aiosqlite/better_sqlite3/pg
// and #[include!]'d provekit-realize-rust-core + provekit-realize-c-core's
// platform_semantics.rs directly. That was the substrate violation #1270
// fixed: kit knowledge was duplicated into libprovekit Rust source, drifting
// from the kit binary's actual behavior. The loader makes the wire the
// single source of truth.

use crate::core::platform_semantics_loader::load_platform_semantics_cached;
use crate::core::types::PlatformSemanticsDeclaration;
use provekit_ir_types::{DimensionValueMemento, PlatformSemanticTag};
use std::collections::BTreeMap;

/// Returns the PlatformSemanticsDeclaration for the given lower-target language,
/// or None when no kit binary served `provekit.plugin.platform_semantics` or
/// the kit binary was not findable on PATH.
///
/// The declaration is loaded from the kit binary via JSON-RPC. The kit must
/// implement `provekit.plugin.platform_semantics` per the PEP 1.7.0 plugin
/// protocol. See `core::platform_semantics_loader`.
///
/// libprovekit does NOT carry hardcoded mirrors of any kit's declaration.
/// Per #1270, that data lives only in the kit, served via the wire.
pub fn platform_semantics_for_lower_target(target: &str) -> Option<PlatformSemanticsDeclaration> {
    load_platform_semantics_cached(target).ok()
}

/// Convenience alias retained for legacy callers. Now identical to
/// `platform_semantics_for_lower_target("python")`.
pub fn python_kit_declaration() -> PlatformSemanticsDeclaration {
    platform_semantics_for_lower_target("python")
        .expect("python kit declaration must be loadable via RPC")
}

/// Compose the language-kit per-op platform semantics with the binding-kit
/// per-op library semantics. Returns the merged declaration, or None if
/// neither layer has any declaration for the given pair.
///
/// Conflict resolution: if both kits declare a tag for the same op-CID,
/// the binding-kit tag overrides the language-kit tag for that op.
/// (Binding-kit semantics are more specific than pure-language semantics.)
///
/// op_aliases merge uses the same binding-wins policy.
pub fn platform_semantics_for_binding(
    lang: &str,
    binding_tag: &str,
) -> Option<PlatformSemanticsDeclaration> {
    let lang_decl = platform_semantics_for_lower_target(lang);
    let binding_decl = platform_semantics_for_lower_target(&format!("{lang}-{binding_tag}"))
        .or_else(|| platform_semantics_for_lower_target(binding_tag));

    match (lang_decl, binding_decl) {
        (None, None) => None,
        (Some(l), None) => Some(l),
        (None, Some(b)) => Some(b),
        (Some(lang_d), Some(binding_d)) => Some(merge_declarations(lang_d, binding_d)),
    }
}

/// Merge language-kit and binding-kit declarations. Binding-kit wins on
/// op-CID conflicts for both tags and op_aliases. dimension_values are
/// unioned with deduplication by CID.
fn merge_declarations(
    lang: PlatformSemanticsDeclaration,
    binding: PlatformSemanticsDeclaration,
) -> PlatformSemanticsDeclaration {
    let mut tags_by_op: BTreeMap<String, PlatformSemanticTag> = lang
        .tags
        .into_iter()
        .map(|tag| (tag.op_cid.clone(), tag))
        .collect();
    for tag in binding.tags {
        tags_by_op.insert(tag.op_cid.clone(), tag);
    }
    let merged_tags: Vec<PlatformSemanticTag> = tags_by_op.into_values().collect();

    let mut seen_cids: BTreeMap<String, ()> = BTreeMap::new();
    let mut merged_values: Vec<DimensionValueMemento> = Vec::new();
    for value in lang
        .dimension_values
        .into_iter()
        .chain(binding.dimension_values)
    {
        if seen_cids.insert(value.cid.clone(), ()).is_none() {
            merged_values.push(value);
        }
    }

    let mut merged_aliases = lang.op_aliases;
    merged_aliases.extend(binding.op_aliases);

    PlatformSemanticsDeclaration {
        tags: merged_tags,
        dimension_values: merged_values,
        op_aliases: merged_aliases,
    }
}

#[cfg(test)]
mod binding_compose_tests {
    use super::*;
    use provekit_ir_types::{DimensionValueMemento, IrFormula, PlatformSemanticTag};

    const TEST_KIT_CID: &str = "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111";
    const TEST_OP_A: &str = "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const TEST_OP_B: &str = "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn dim_value(dim: &str, name: &str) -> DimensionValueMemento {
        DimensionValueMemento::new(
            TEST_KIT_CID.to_string(),
            dim.to_string(),
            name.to_string(),
            IrFormula::Atomic {
                name: format!("test:{name}"),
                args: vec![],
            },
        )
    }

    fn one_tag(op_cid: &str, dim: &str, value_cid: &str) -> PlatformSemanticTag {
        let mut dimensions = BTreeMap::new();
        dimensions.insert(dim.to_string(), value_cid.to_string());
        PlatformSemanticTag::new(TEST_KIT_CID.to_string(), op_cid.to_string(), dimensions)
    }

    fn decl_with(op_cid: &str, dim: &str, value_name: &str) -> PlatformSemanticsDeclaration {
        let val = dim_value(dim, value_name);
        let tag = one_tag(op_cid, dim, &val.cid.clone());
        PlatformSemanticsDeclaration {
            tags: vec![tag],
            dimension_values: vec![val],
            op_aliases: BTreeMap::new(),
        }
    }

    #[test]
    fn merge_declarations_combines_non_overlapping_ops() {
        let lang = decl_with(TEST_OP_A, "LangDim", "LangValue");
        let binding = decl_with(TEST_OP_B, "BindDim", "BindValue");
        let merged = merge_declarations(lang, binding);
        let op_cids: Vec<&str> = merged.tags.iter().map(|t| t.op_cid.as_str()).collect();
        assert!(
            op_cids.contains(&TEST_OP_A),
            "language op must be in merged"
        );
        assert!(op_cids.contains(&TEST_OP_B), "binding op must be in merged");
        assert_eq!(merged.dimension_values.len(), 2);
    }

    #[test]
    fn merge_declarations_binding_overrides_language_for_same_op() {
        let lang = decl_with(TEST_OP_A, "Dim", "LangValue");
        let binding = decl_with(TEST_OP_A, "Dim", "BindValue");

        let lang_value_cid = lang.dimension_values[0].cid.clone();
        let binding_value_cid = binding.dimension_values[0].cid.clone();
        assert_ne!(lang_value_cid, binding_value_cid, "fixture must differ");

        let merged = merge_declarations(lang, binding);
        assert_eq!(merged.tags.len(), 1, "conflict deduplicated to one tag");
        let winning_value_cid = merged.tags[0].dimensions.get("Dim").expect("Dim present");
        assert_eq!(
            winning_value_cid, &binding_value_cid,
            "binding-kit must override language-kit on same op-CID"
        );
    }

    #[test]
    fn platform_semantics_for_binding_returns_none_when_neither_layer_declared() {
        // Both targets unknown: no kit binary exists, loader returns None
        // for each, dispatcher returns None.
        let result = platform_semantics_for_binding("__unknown_lang__", "__unknown_binding__");
        assert!(
            result.is_none(),
            "unknown lang + unknown binding must return None"
        );
    }
}
