// SPDX-License-Identifier: Apache-2.0

use crate::core::types::PlatformSemanticsDeclaration;
use provekit_ir_types::{DimensionValueMemento, PlatformSemanticTag};
use std::collections::BTreeMap;

pub mod better_sqlite3;
pub mod java;
pub mod pg;
pub mod python_aiosqlite;
mod python_common;
pub mod python_lift_source;
pub mod python_realize_core;
pub mod python_sqlite3;
pub mod typescript;

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
                dimension_values: provekit_realize_rust_core::platform_semantics::dimension_values(
                ),
                op_aliases: rust_concept_op_aliases(),
            })
        }
        "java" => Some(java::declaration()),
        "c" => Some(c_realize_core::declaration()),
        "typescript" => Some(typescript::declaration()),
        _ => None,
    }
}

pub fn python_kit_declaration() -> PlatformSemanticsDeclaration {
    python_realize_core::declaration()
}

fn rust_concept_op_aliases() -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "blake3-512:95fc70e63a5550fd2e25142f13932919c59d085654ab387789c798886b0111c61d28fe533fc98b50df70eea9428a9af8aa75372c8b1c1deb3acc1a4094790468".to_string(),
            "blake3-512:398980644a46039b0c2875ab36ccb61f52f284ccad5481593305ed3f10efe91e7863c00a3f2d673644430f691e6b5354f5d65f9da4fa23acdb13dc58f5b438f9".to_string(),
        ),
        (
            "blake3-512:b7c54558573348bb3a9297732547a8e6e9d152403d292df7426b6bb8a248f705b4b030bf2a22ba547a17d6f1bfaf8e75a6843e02e8f23a8226ebc09e2a8622af".to_string(),
            "blake3-512:b6c62a64669ff12d0af45d9932c1ab5e08576f1cac97b4abe60392a9f02393dac9765514b024b1481ddc829d4b7fb97950ad648a9944dceafa194b8423923533".to_string(),
        ),
        (
            "blake3-512:46cd627de058c8d4f7d087ea33f4904af65ad4b2e3cfd3aff8f44bf27db96b33c2dae39cd30f53898c233c9465ba8d2701c69e5903d48935113103b4db00fd03".to_string(),
            "blake3-512:1df457dceb0ec7a6dc4596eb70be001be09180afc69fa3ff8121cd78a0daff5dd9606dbfd4fb9fcdc5d834939a6f19c52b80aace16dea6df5ffdce62d86bbfa2".to_string(),
        ),
        (
            "blake3-512:c6a13abbcafdf83edcff49d883a7c7440faadd8af896da0ad46e2bcb177ed0649d005b4ddecd4689cf565b10679219a07c784399bafe5c6174642e1b808d7839".to_string(),
            "blake3-512:d7403da8d2a8921b71170b5fc34c12022118d0c545f25c7ff89fe77bbed02419e3528479ded0e746535ee92d0e1801bce46608c15c3d6d2a5567bec811cbc75a".to_string(),
        ),
        (
            "blake3-512:92340897b43965e01454b00a6a43ec54b2bf0e01213a45fa2311f730dde18adf8da97a22458c1a2a0fb23ce85ef3ad9b22e704804c74f41997aba3ba02cefe0d".to_string(),
            "blake3-512:235c6177611c2753a1c0d07d44391f5465ab50dc585372df52220118cb103ef19502192a07148bd2969d7f6f7ed0d134714d7745825f486768d0b0de8ac0b6dc".to_string(),
        ),
        (
            "blake3-512:f9cdfcba8d0e223803126504a2a6ed10005fa61acb5c55b74b270bc66d963eb7648ab6763f0510760df93145c0f6670087a403417e8b3100c7142e121111807a".to_string(),
            "blake3-512:37af5330572cf08650e3b6d5fdfc2649d56c0bb2e019f9be3861082c9d1961c1808beca6f9dfc39742ade25f06bfb499da74c89d33f64decd0c70f0972d021e1".to_string(),
        ),
        (
            "blake3-512:c90e3c159b25e4c4c7f9c899da5aa3ee048a548719ced7360f3e514450811096b21cd5473f22d7a05df088f92210bbc916e65970b9fa1e1511c193ed969f112b".to_string(),
            "blake3-512:cb23fbc9d05a19b353e1fe85c77e241fdc8c58cde5a7c5cad008b721a51eaf682284d8bfe3b383d751cb58833e94beb6bd0dd4d330f9619f095c8b4daa8298da".to_string(),
        ),
        (
            "blake3-512:9e96c2445bad6bb1e5a6f902ad7f733e3f4619829b9c0e232361fbf50b978c8332029212ed895762e604d1df009fce58848cda33524a697df798233eae30a14b".to_string(),
            "blake3-512:fcc41d285a20dae6c2deb2a854665d5d43bc829a09a76107d929898b3b169d1abf53ed71f302b00ec2146bcec3b5fe732ca7ecd4354e7739e67feea3db9fd6a2".to_string(),
        ),
        (
            "blake3-512:d57b54bffe698ed804a4a49486b73a1a8a3e7bd84fb12babaad01ce22d8b7bcb5a35f3476324063f8de9f8090846d0d4fbeb48d78475d07e16f7925b4f264de3".to_string(),
            "blake3-512:5c455355a13fd97a872848613b34b2b56f9738c832f900558710af1cd053976157513f31a8feb123202557dc0a369b88bc7c946179fe817d6c2f80d4f318f824".to_string(),
        ),
        (
            "blake3-512:343b1f9faa98218467d810e0a2bb1b1eebeaf921c71a1bc52141f885220afff482c631c52e2157a6067640f4830f928add53ef7aa0386c6a27ee3c8bab6dc353".to_string(),
            "blake3-512:16ba612da4883e853dd18b08c8e7b1803e1e2b0a42ab83c261048a49cdfd9b20bc54e809b8f4e8e5c9af63cc7447dee039cb826c611dfec137855a11a502adb9".to_string(),
        ),
        (
            "blake3-512:5e788f0d551081f4e709e4418e01017fa9ae1c04963e7be2862fadad8a8434fafa204629fbec53e2e44624c195ac2e32c0410df25cf8ff3a4be672582f89109f".to_string(),
            "blake3-512:eeaaf14737f661b6bce03f23d281974502182fea83909eeaade25e510887b26e80dac1b10af3b1f2f496b53898051d63e8d250e78cfa8e88380c84809e5eabe0".to_string(),
        ),
        (
            "blake3-512:ad958847b50cf07ddbb92d85ae488a5f983d5619e108476b42e519174cfcce883ecd637544a372b946bb45a1c22893c710bc9b08ea0569ad0e035b3babb6a409".to_string(),
            "blake3-512:e0c3e13fd7e0d11fa3b78f4e083ab60b1166bdd905bc04e533e6dcc97d79330bd6a403caaf1265d8134ea3ccd5fe8cfd5a3e18f349ea7edcb6310c098e845c0f".to_string(),
        ),
    ])
}

/// Compose the language-kit per-op platform semantics with the binding-kit
/// per-op library semantics. Returns the merged declaration, or None if
/// neither layer has any declaration for the given pair.
///
/// Conflict resolution: if both kits declare a tag for the same op-CID,
/// the binding-kit tag overrides the language-kit tag for that op.
/// (Binding-kit semantics are more specific than pure-language semantics.)
///
/// NOTE: op_aliases merge uses the same binding-wins policy. If both kits
/// map the same source op-CID, the binding alias takes precedence.
/// Agent C can revise this comment if a different alias-merge policy is
/// needed when wiring binding-kit arms.
pub fn platform_semantics_for_binding(
    lang: &str,
    binding_tag: &str,
) -> Option<PlatformSemanticsDeclaration> {
    let lang_decl = platform_semantics_for_lower_target(lang);
    let binding_decl = binding_semantics_for_tag(binding_tag);

    match (lang_decl, binding_decl) {
        (None, None) => None,
        (Some(l), None) => Some(l),
        (None, Some(b)) => Some(b),
        (Some(lang_d), Some(binding_d)) => Some(merge_declarations(lang_d, binding_d)),
    }
}

/// Extension point for binding-specific platform-semantics declarations.
///
/// Binding tags are the second component produced by split_library_surface,
/// e.g. "better-sqlite3" from "typescript-better-sqlite3", "pg" from
/// "typescript-pg". This stub is populated with arms for the two binding kits
/// minted in this branch (Agent B); Agent C wires the end-to-end loss-record
/// path once the op-CID path is verified.
fn binding_semantics_for_tag(binding_tag: &str) -> Option<PlatformSemanticsDeclaration> {
    match binding_tag {
        "aiosqlite" => Some(python_aiosqlite::declaration()),
        "better-sqlite3" => Some(better_sqlite3::declaration()),
        "pg" => Some(pg::declaration()),
        "sqlite3" => Some(python_sqlite3::declaration()),
        _ => None,
    }
}

/// Merge language-kit and binding-kit declarations. Binding-kit wins on
/// op-CID conflicts for both tags and op_aliases. dimension_values are
/// unioned with deduplication by CID.
fn merge_declarations(
    lang: PlatformSemanticsDeclaration,
    binding: PlatformSemanticsDeclaration,
) -> PlatformSemanticsDeclaration {
    // Build a map of lang tags by op-CID for easy override lookup.
    let mut tags_by_op: BTreeMap<String, PlatformSemanticTag> = lang
        .tags
        .into_iter()
        .map(|tag| (tag.op_cid.clone(), tag))
        .collect();
    // Binding-kit tags override language-kit tags for the same op-CID.
    for tag in binding.tags {
        tags_by_op.insert(tag.op_cid.clone(), tag);
    }
    let merged_tags: Vec<PlatformSemanticTag> = tags_by_op.into_values().collect();

    // Union dimension_values, deduplicating by CID (binding appended last so
    // its entries survive if a CID collision occurs; the dedup keeps the first
    // occurrence, so lang values that share a CID with binding values are kept).
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

    // Binding-kit aliases override language-kit aliases for the same source op-CID.
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

    /// Test 1: Both layers contribute distinct op-CIDs -- merged has both.
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

    /// Test 2: Conflict resolution -- same op-CID in both, binding-kit wins.
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

    /// Test 3: Returns None when both lang and binding_tag are unknown.
    #[test]
    fn platform_semantics_for_binding_returns_none_when_neither_layer_declared() {
        let result = platform_semantics_for_binding("__unknown_lang__", "__unknown_binding__");
        assert!(
            result.is_none(),
            "unknown lang + unknown binding must return None"
        );
    }
}

#[cfg(test)]
mod sort_admission_tests {
    use super::*;
    use crate::core::types::OpCoverageVerdict;
    use crate::effect_propagation::ChangedCallsite;
    use provekit_ir_types::{DimensionValueMemento, IrFormula, IrTerm};

    const CONCEPT_LITERAL_CID: &str = "blake3-512:02804a0bdbd2d5d541544451f41ee8d0d340baf28f70bd5abf5844e87a96aedd7b5ab3453962754a020679cc8c6b3d1f4cf0336a7ad8118128d42ac667abf2d6";
    const NULL_SORT_CID: &str = "blake3-512:62f6040bd3f414c1e6c2b7bdf276669cd5613b33cb508a81170170064ca3ffba771a4b0002dc52e059fce5f9f63a1874ef71bd4ec89ae06e89c87a3e91aac3b5";

    fn sort_admission_value(declaration: &PlatformSemanticsDeclaration) -> &DimensionValueMemento {
        let tag = declaration
            .tags
            .iter()
            .find(|tag| tag.op_cid == CONCEPT_LITERAL_CID)
            .expect("kit must declare concept:literal");
        let sort_admission_cid = tag
            .dimensions
            .get("SortAdmission")
            .expect("concept:literal tag must carry SortAdmission");
        declaration
            .dimension_values
            .iter()
            .find(|value| &value.cid == sort_admission_cid)
            .expect("SortAdmission CID must have a dimension value memento")
    }

    fn formula_admits_sort(formula: &IrFormula, name: &str, cid: &str) -> bool {
        let IrFormula::Atomic { name: atom, args } = formula else {
            return false;
        };
        atom == "admits_sorts"
            && args.iter().any(|term| {
                matches!(
                    term,
                    IrTerm::Ctor { name: sort_name, args }
                        if sort_name == name
                            && matches!(
                                args.as_slice(),
                                [IrTerm::Ctor { name: sort_cid, args }] if sort_cid == cid && args.is_empty()
                            )
                )
            })
    }

    #[test]
    fn sort_admission_identical_java_python_sets_share_cid() {
        let java = java::declaration();
        let python = python_kit_declaration();
        let java_value = sort_admission_value(&java);
        let python_value = sort_admission_value(&python);

        assert_eq!(
            java_value.cid, python_value.cid,
            "identical admitted sort sets must share SortAdmission CID"
        );
        assert_eq!(
            java_value.value_name, python_value.value_name,
            "identical admitted sort sets must use the same derived value name"
        );
        assert_eq!(
            java_value.compare_to, python_value.compare_to,
            "identical admitted sort sets must have identical compare_to formula"
        );
    }

    #[test]
    fn sort_admission_null_literal_diverges_python_to_rust_changed_callsite() {
        let python = python_kit_declaration();
        let rust = platform_semantics_for_lower_target("rust").expect("rust kit declaration");
        let python_value = sort_admission_value(&python);
        let rust_value = sort_admission_value(&rust);

        assert!(
            formula_admits_sort(&python_value.compare_to, "Null", NULL_SORT_CID),
            "python SortAdmission must admit Null"
        );
        assert!(
            !formula_admits_sort(&rust_value.compare_to, "Null", NULL_SORT_CID),
            "rust SortAdmission must not admit Null"
        );

        let verdict = python
            .compare_op_with(CONCEPT_LITERAL_CID, &rust)
            .expect("declared concept:literal comparison must not error");

        let divergence = match verdict {
            OpCoverageVerdict::Divergent(divergence) => divergence,
            other => panic!("expected SortAdmission divergence, got {other:?}"),
        };
        assert_eq!(divergence.dimension_name, "SortAdmission");

        let changed = ChangedCallsite {
            callsite_cid: "callsite:null-literal".to_string(),
            dimension_name: Some(divergence.dimension_name),
            effect: format!(
                "platform-semantic-divergence:{}->{}",
                divergence.source_value_cid, divergence.target_value_cid
            ),
        };
        assert_eq!(changed.dimension_name.as_deref(), Some("SortAdmission"));
        assert_eq!(changed.callsite_cid, "callsite:null-literal");
    }
}
