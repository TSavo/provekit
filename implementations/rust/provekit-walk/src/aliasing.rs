// SPDX-License-Identifier: Apache-2.0
//
// C.8 aliasing analysis: walk type definitions to detect interior mutability.

use serde_json::Value;
use std::collections::HashSet;

/// Known types that wrap UnsafeCell. Detection of any of these in a
/// transitive type walk means the type has interior mutability.
const INTERIOR_MUT_TYPE_NAMES: &[&str] = &[
    "UnsafeCell", "Cell", "RefCell", "Mutex", "RwLock", "OnceCell", "LazyLock",
];

/// Walk the type graph starting from `ty` and return true if any
/// transitively-reachable ADT is an interior-mutability wrapper.
/// Uses `visited` for cycle detection keyed on Charon ADT def_id.
pub fn has_unsafecell_transitive(
    ty: &Value,
    type_decls: Option<&Value>,
    visited: &mut HashSet<u64>,
) -> bool {
    let inner = match ty.get("Untagged") {
        Some(v) => v,
        None => return false,
    };

    // Adt: recurse into fields
    if let Some(adt) = inner.get("Adt") {
        let id = adt.get("id");
        let adt_id = id.and_then(|i| i.as_u64()).or_else(|| {
            id.and_then(|i| i.get("Adt")).and_then(|a| a.as_u64())
        });

        // Check the ADT's own name against interior-mut set
        if let Some(type_name) = adt_name_for_id(type_decls, adt_id) {
            if INTERIOR_MUT_TYPE_NAMES.contains(&type_name.as_str()) {
                return true;
            }
        }

        // Cycle detection
        if let Some(aid) = adt_id {
            if !visited.insert(aid) {
                return false; // already visited, no interior mut found
            }
        }

        // Recurse into generic type params
        if let Some(types_arr) = adt
            .get("generics")
            .and_then(|g| g.get("types"))
            .and_then(|t| t.as_array())
        {
            for t in types_arr {
                // Wrap in Untagged for the recursive call
                let mut map = serde_json::Map::new();
                map.insert("Untagged".to_string(), t.clone());
                let wrapped = Value::Object(map);
                if has_unsafecell_transitive(&wrapped, type_decls, visited) {
                    return true;
                }
            }
        }
        return false;
    }

    // Reference: recurse into referent type
    if let Some(arr) = inner.get("Ref").and_then(|v| v.as_array()) {
        if arr.len() >= 2 {
            return has_unsafecell_transitive(&arr[1], type_decls, visited);
        }
    }

    // Slice: recurse into element type
    if let Some(elem) = inner.get("Slice") {
        return has_unsafecell_transitive(elem, type_decls, visited);
    }

    false
}

/// Look up an ADT's source name from the type_decls table.
fn adt_name_for_id(type_decls: Option<&Value>, adt_id: Option<u64>) -> Option<String> {
    let tds = type_decls?.as_array()?;
    let target = adt_id?;
    for td in tds {
        let idx = td.get("def_id")?.get("index")?.as_u64()?;
        if idx == target {
            let name_arr = td.get("item_meta")?.get("name")?.as_array()?;
            for seg in name_arr.iter().rev() {
                if let Some(ident_arr) = seg.get("Ident").and_then(|v| v.as_array()) {
                    if let Some(s) = ident_arr.first().and_then(|v| v.as_str()) {
                        return Some(s.to_string());
                    }
                }
            }
        }
    }
    None
}

/// True when a Charon Ty JSON value is a shared reference (`&T`).
pub fn is_shared_ref_charon_ty(ty: &Value) -> bool {
    ty.get("Untagged")
        .and_then(|i| i.get("Ref"))
        .and_then(|r| r.as_array())
        .and_then(|arr| arr.get(2))
        .and_then(|m| m.as_str())
        .map(|m| m == "Shared")
        .unwrap_or(false)
}

/// True when a Charon Ty JSON value is a mutable reference (`&mut T`).
pub fn is_mut_ref_charon_ty(ty: &Value) -> bool {
    ty.get("Untagged")
        .and_then(|i| i.get("Ref"))
        .and_then(|r| r.as_array())
        .and_then(|arr| arr.get(2))
        .and_then(|m| m.as_str())
        .map(|m| m == "Mut")
        .unwrap_or(false)
}
