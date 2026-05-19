// SPDX-License-Identifier: Apache-2.0
//
// Binding-kit declaration for the Python `sqlite3` library.
//
// This is a BINDING-kit declaration (not a language-kit). It characterizes
// how the `sqlite3` library exposes the result of an INSERT that returns the
// newly created row id.
//
// Binding tag: "sqlite3"  (the tag component after splitting
// "python-sqlite3" via split_library_surface).
//
// RowIdMechanism = CursorLastRowid: the library exposes the inserted row id
// as `cursor.lastrowid` after executing an INSERT through that cursor. This is
// structurally different from the better-sqlite3 mechanism, which reads
// connection-global SQLite state via `lastInsertRowid`.
//
// Concept-op CID for concept:insert-and-get-id:
//   blake3-512:0a4f0a8d36d8dee96b8d5b32a18bb390f35877ecef611771048c6e10cfc3d25ad8f59de89b00c7794f62cabaf91dbd779244338393a8bb6ef5e8309b0929b3ca
// Minted from the AlgorithmMemento JSON in:
//   menagerie/concept-shapes/catalog/algorithms/concept:insert-and-get-id.<cid>.json

use std::collections::BTreeMap;

use provekit_canonicalizer::blake3_512_of;
use provekit_ir_types::{DimensionValueMemento, IrFormula, IrTerm, PlatformSemanticTag};

use crate::core::types::PlatformSemanticsDeclaration;

const KIT_ID: &str = "provekit-binding-python-sqlite3@0.1.0";

/// CID for concept:insert-and-get-id, minted from its AlgorithmMemento via JCS+blake3-512.
pub const CONCEPT_INSERT_AND_GET_ID_CID: &str =
    "blake3-512:0a4f0a8d36d8dee96b8d5b32a18bb390f35877ecef611771048c6e10cfc3d25ad8f59de89b00c7794f62cabaf91dbd779244338393a8bb6ef5e8309b0929b3ca";

pub fn declaration() -> PlatformSemanticsDeclaration {
    let kit_cid = blake3_512_of(KIT_ID.as_bytes());
    let values = dimension_values_for_kit(&kit_cid);
    let value_cids = values
        .iter()
        .map(|v| (v.value_name.clone(), v.cid.clone()))
        .collect::<BTreeMap<_, _>>();

    PlatformSemanticsDeclaration {
        tags: vec![tag(
            &kit_cid,
            CONCEPT_INSERT_AND_GET_ID_CID,
            &[(
                "RowIdMechanism",
                value_cids["CursorLastRowid"].as_str(),
            )],
        )],
        dimension_values: values,
        op_aliases: BTreeMap::new(),
    }
}

pub fn dimension_values() -> Vec<DimensionValueMemento> {
    let kit_cid = blake3_512_of(KIT_ID.as_bytes());
    dimension_values_for_kit(&kit_cid)
}

fn dimension_values_for_kit(kit_cid: &str) -> Vec<DimensionValueMemento> {
    vec![
        // CursorLastRowid: row id is sourced from the cursor state after
        // executing the INSERT. The compare_to formula is an atomic predicate
        // whose `source` arg names the access mechanism: a Ctor term
        // "cursor_lastrowid" applied to "cursor_state_after_execute". This
        // structurally distinguishes it from LastInsertRowid, which reads
        // connection state at call return.
        DimensionValueMemento::new(
            kit_cid.to_string(),
            "RowIdMechanism".to_string(),
            "CursorLastRowid".to_string(),
            IrFormula::Atomic {
                name: "row_id_source".to_string(),
                args: vec![IrTerm::Ctor {
                    name: "cursor_lastrowid".to_string(),
                    args: vec![IrTerm::Ctor {
                        name: "cursor_state_after_execute".to_string(),
                        args: vec![],
                    }],
                }],
            },
        ),
    ]
}

fn tag(kit_cid: &str, op_cid: &str, pairs: &[(&str, &str)]) -> PlatformSemanticTag {
    let mut dimensions = BTreeMap::new();
    for (dimension, cid) in pairs {
        dimensions.insert((*dimension).to_string(), (*cid).to_string());
    }
    PlatformSemanticTag::new(kit_cid.to_string(), op_cid.to_string(), dimensions)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Positive: declaration is non-empty and has insert-and-get-id semantics.
    #[test]
    fn python_sqlite3_declaration_is_non_empty() {
        let decl = declaration();
        assert!(
            !decl.tags.is_empty(),
            "python-sqlite3 binding kit must declare at least one op tag"
        );
        assert!(
            decl.tags.iter().any(|t| t.op_cid == CONCEPT_INSERT_AND_GET_ID_CID),
            "python-sqlite3 kit must declare concept:insert-and-get-id"
        );
        assert!(
            !decl.dimension_values.is_empty(),
            "python-sqlite3 kit must declare dimension values"
        );
    }

    // Discrimination: python-sqlite3 uses CursorLastRowid; better-sqlite3 uses LastInsertRowid.
    #[test]
    fn python_sqlite3_row_id_mechanism_differs_from_better_sqlite3() {
        use super::super::better_sqlite3;

        let python_decl = declaration();
        let sqlite_decl = better_sqlite3::declaration();

        let python_tag = python_decl
            .tags
            .iter()
            .find(|t| t.op_cid == CONCEPT_INSERT_AND_GET_ID_CID)
            .expect("python-sqlite3 must have insert-and-get-id tag");
        let sqlite_tag = sqlite_decl
            .tags
            .iter()
            .find(|t| t.op_cid == CONCEPT_INSERT_AND_GET_ID_CID)
            .expect("better-sqlite3 must have insert-and-get-id tag");

        let python_mechanism = python_tag
            .dimensions
            .get("RowIdMechanism")
            .expect("python-sqlite3 tag must have RowIdMechanism dimension");
        let sqlite_mechanism = sqlite_tag
            .dimensions
            .get("RowIdMechanism")
            .expect("better-sqlite3 tag must have RowIdMechanism dimension");

        assert_ne!(
            python_mechanism, sqlite_mechanism,
            "CursorLastRowid and LastInsertRowid must hash to different dimension value CIDs"
        );
    }

    // Structural: the CursorLastRowid dimension value carries a valid IrFormula compare_to.
    #[test]
    fn python_sqlite3_dimension_value_has_valid_compare_to() {
        use provekit_ir_types::IrFormula;

        let values = dimension_values();
        let row_id_value = values
            .iter()
            .find(|v| v.dimension_name == "RowIdMechanism" && v.value_name == "CursorLastRowid")
            .expect("must have CursorLastRowid dimension value");

        assert!(
            matches!(&row_id_value.compare_to, IrFormula::Atomic { args, .. } if !args.is_empty()),
            "CursorLastRowid compare_to must be an IrFormula::Atomic with IrTerm args"
        );
        assert!(
            row_id_value.cid.starts_with("blake3-512:"),
            "dimension value CID must start with blake3-512:"
        );
        assert_eq!(
            row_id_value.cid.len(),
            139,
            "dimension value CID must be 139 chars"
        );
    }
}
