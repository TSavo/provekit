//! Extract a simplified IR from the CDDL AST.

use std::collections::{HashMap, HashSet};

/// Our simplified IR, extracted from the CDDL rules.
pub struct IrSchema {
    pub types: HashMap<String, TypeDef>,
}

#[derive(Debug, Clone)]
pub struct TypeDef {
    pub name: String,
    pub kind: TypeKind,
}

#[derive(Debug, Clone)]
pub enum TypeKind {
    Enum { variants: Vec<Variant> },
    Struct { fields: Vec<Field> },
    Alias { target: String },
    StringEnum { values: Vec<String> },
}

#[derive(Debug, Clone)]
pub struct Variant {
    pub name: String,
    pub serde_rename: String,
    pub fields: Vec<Field>,
    pub is_unit: bool,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: String,
    pub optional: bool,
    pub serde_rename: Option<String>,
}

pub fn extract_ir(cddl: &cddl::ast::CDDL) -> IrSchema {
    let mut types = HashMap::new();
    for rule in &cddl.rules {
        if let Some(def) = process_rule(rule) {
            types.insert(def.name.clone(), def);
        }
    }

    // Second pass: resolve typename references in enums.
    // When an enum variant is a bare typename referring to a struct,
    // inline that struct's fields into the variant (excluding the
    // "kind" tag field).
    let mut inlined_structs = HashSet::new();
    let names: Vec<String> = types.keys().cloned().collect();
    for name in names {
        if let Some(def) = types.get(&name).cloned() {
            if let TypeKind::Enum { variants } = &def.kind {
                let mut new_variants = Vec::new();
                let mut changed = false;
                for v in variants {
                    if v.is_unit && v.fields.is_empty() {
                        if let Some(referenced) = types.get(&v.name) {
                            if let TypeKind::Struct { fields } = &referenced.kind {
                                let tag_values = find_tag_values(cddl, &v.name);
                                let inlined_fields: Vec<Field> = fields
                                    .iter()
                                    .filter(|f| f.name != "kind")
                                    .cloned()
                                    .collect();
                                if tag_values.is_empty() {
                                    // No tag found, keep original variant name.
                                    new_variants.push(v.clone());
                                } else {
                                    for tag_value in tag_values {
                                        let variant_name = tag_to_variant_name(&tag_value);
                                        new_variants.push(Variant {
                                            name: variant_name,
                                            serde_rename: tag_value,
                                            fields: inlined_fields.clone(),
                                            is_unit: false,
                                        });
                                    }
                                    inlined_structs.insert(v.name.clone());
                                    changed = true;
                                }
                                continue;
                            }
                        }
                    }
                    new_variants.push(v.clone());
                }
                if changed {
                    if let Some(def) = types.get_mut(&name) {
                        def.kind = TypeKind::Enum {
                            variants: new_variants,
                        };
                    }
                }
            }
        }
    }

    // Third pass: convert single-struct aliases into single-variant enums.
    // e.g. Sort = PrimitiveSort  ->  enum Sort { Primitive { name } }
    let alias_names: Vec<String> = types
        .iter()
        .filter(|(_, d)| matches!(d.kind, TypeKind::Alias { .. }))
        .map(|(n, _)| n.clone())
        .collect();
    for name in alias_names {
        if let Some(def) = types.get(&name).cloned() {
            if let TypeKind::Alias { target } = &def.kind {
                if let Some(tag_values) = find_tag_values_for_type(cddl, target) {
                    if let Some(referenced) = types.get(target) {
                        if let TypeKind::Struct { fields } = &referenced.kind {
                            let inlined_fields: Vec<Field> = fields
                                .iter()
                                .filter(|f| f.name != "kind")
                                .cloned()
                                .collect();
                            if let Some(first_tag) = tag_values.first() {
                                let variant_name = tag_to_variant_name(first_tag);
                                if let Some(def) = types.get_mut(&name) {
                                    def.kind = TypeKind::Enum {
                                        variants: vec![Variant {
                                            name: variant_name,
                                            serde_rename: first_tag.clone(),
                                            fields: inlined_fields,
                                            is_unit: false,
                                        }],
                                    };
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Remove struct types that were fully inlined into enum variants.
    // They are redundant and their unboxed recursive fields cause
    // infinite-size / drop-check errors.
    for name in &inlined_structs {
        types.remove(name);
    }

    IrSchema { types }
}

/// Convert a tag value (like "var", "atomic", "and") to a PascalCase variant name.
fn tag_to_variant_name(tag: &str) -> String {
    to_pascal_case(tag)
}

/// Find all possible tag values for a struct type.
/// Returns empty vec if the type is not a struct or has no recognizable kind field.
fn find_tag_values(cddl: &cddl::ast::CDDL, type_name: &str) -> Vec<String> {
    for rule in &cddl.rules {
        if rule.name() == type_name {
            if let cddl::ast::Rule::Type {
                rule: type_rule, ..
            } = rule
            {
                let choices: Vec<_> = type_rule.value.type_choices.iter().collect();
                if choices.len() == 1 {
                    if let cddl::ast::Type2::Map { group, .. } = &choices[0].type1.type2 {
                        return extract_kind_values(cddl, group);
                    }
                }
            }
        }
    }
    vec![]
}

/// Like find_tag_values but follows a type alias chain.
fn find_tag_values_for_type(cddl: &cddl::ast::CDDL, type_name: &str) -> Option<Vec<String>> {
    for rule in &cddl.rules {
        if rule.name() == type_name {
            match rule {
                cddl::ast::Rule::Type {
                    rule: type_rule, ..
                } => {
                    let choices: Vec<_> = type_rule.value.type_choices.iter().collect();
                    if choices.len() == 1 {
                        match &choices[0].type1.type2 {
                            cddl::ast::Type2::Map { group, .. } => {
                                return Some(extract_kind_values(cddl, group));
                            }
                            cddl::ast::Type2::Typename { ident, .. } => {
                                return find_tag_values_for_type(cddl, &ident.to_string());
                            }
                            _ => return None,
                        }
                    }
                }
                _ => return None,
            }
        }
    }
    None
}

/// Extract possible values from a `kind` field within a group.
/// Handles literal text values and references to string enums.
fn extract_kind_values(cddl: &cddl::ast::CDDL, group: &cddl::ast::Group) -> Vec<String> {
    for choice in &group.group_choices {
        for (entry, _) in &choice.group_entries {
            if let cddl::ast::GroupEntry::ValueMemberKey { ge, .. } = entry {
                if let Some(mk) = &ge.member_key {
                    let is_kind = match mk {
                        cddl::ast::MemberKey::Value { value, .. } => {
                            matches!(value, cddl::token::Value::TEXT(t) if t == "kind")
                        }
                        cddl::ast::MemberKey::Bareword { ident, .. } => ident.to_string() == "kind",
                        _ => false,
                    };
                    if is_kind {
                        let tc = match ge.entry_type.type_choices.first() {
                            Some(tc) => tc,
                            None => continue,
                        };
                        match &tc.type1.type2 {
                            cddl::ast::Type2::TextValue { value: tag, .. } => {
                                return vec![tag.to_string()];
                            }
                            cddl::ast::Type2::Typename { ident, .. } => {
                                let type_name = ident.to_string();
                                return resolve_string_enum(cddl, &type_name);
                            }
                            _ => continue,
                        }
                    }
                }
            }
        }
    }
    vec![]
}

/// Resolve a typename to its string enum values.
fn resolve_string_enum(cddl: &cddl::ast::CDDL, type_name: &str) -> Vec<String> {
    for rule in &cddl.rules {
        if rule.name() == type_name {
            if let cddl::ast::Rule::Type {
                rule: type_rule, ..
            } = rule
            {
                let mut values = Vec::new();
                for tc in &type_rule.value.type_choices {
                    if let cddl::ast::Type2::TextValue { value, .. } = &tc.type1.type2 {
                        values.push(value.to_string());
                    }
                }
                return values;
            }
        }
    }
    vec![]
}

fn process_rule(rule: &cddl::ast::Rule) -> Option<TypeDef> {
    let name = rule.name();
    match rule {
        cddl::ast::Rule::Type {
            rule: type_rule, ..
        } => {
            let kind = convert_type(&type_rule.value)?;
            Some(TypeDef { name, kind })
        }
        cddl::ast::Rule::Group { .. } => {
            // Our CDDL does not use standalone group rules.
            None
        }
    }
}

fn convert_type(ty: &cddl::ast::Type) -> Option<TypeKind> {
    let choices: Vec<_> = ty.type_choices.iter().collect();

    // Check if all choices are text values -> string enum
    let all_text_values: Option<Vec<String>> = choices
        .iter()
        .map(|tc| match &tc.type1.type2 {
            cddl::ast::Type2::TextValue { value, .. } => Some(value.to_string()),
            _ => None,
        })
        .collect();

    if let Some(values) = all_text_values {
        if !values.is_empty() {
            return Some(TypeKind::StringEnum { values });
        }
    }

    // Mixed text values + string-like typenames -> string enum
    let mut text_values = Vec::new();
    let mut all_string_like = true;
    for tc in &choices {
        match &tc.type1.type2 {
            cddl::ast::Type2::TextValue { value, .. } => {
                text_values.push(value.to_string());
            }
            cddl::ast::Type2::Typename { ident, .. } => {
                let n = ident.to_string();
                if n != "tstr" && n != "text" {
                    all_string_like = false;
                }
            }
            _ => {
                all_string_like = false;
            }
        }
    }
    if all_string_like && !text_values.is_empty() {
        return Some(TypeKind::StringEnum {
            values: text_values,
        });
    }

    // Check if all choices are Map types -> enum with struct variants
    let all_maps: Option<Vec<(String, Vec<Field>)>> = choices
        .iter()
        .map(|tc| match &tc.type1.type2 {
            cddl::ast::Type2::Map { group, .. } => {
                let tag_field = find_literal_tag(group)?;
                let fields = convert_group_entries(group, Some(&tag_field));
                Some((tag_field, fields))
            }
            cddl::ast::Type2::Typename { ident, .. } => {
                let variant_name = ident.to_string();
                Some((variant_name, vec![]))
            }
            _ => None,
        })
        .collect();

    if let Some(map_variants) = all_maps {
        if map_variants.len() > 1 {
            let variants = map_variants
                .into_iter()
                .map(|(tag, fields)| {
                    let variant_name = to_pascal_case(&tag);
                    let is_unit = fields.is_empty();
                    Variant {
                        name: variant_name,
                        serde_rename: tag,
                        fields,
                        is_unit,
                    }
                })
                .collect();
            return Some(TypeKind::Enum { variants });
        }
    }

    // Single type
    if choices.len() == 1 {
        let tc = &choices[0];
        match &tc.type1.type2 {
            cddl::ast::Type2::Map { group, .. } => {
                let fields = convert_group_entries(group, None);
                return Some(TypeKind::Struct { fields });
            }
            cddl::ast::Type2::Array { group, .. } => {
                let item_type = array_item_type(group);
                return Some(TypeKind::Alias {
                    target: format!("Vec<{}>", item_type),
                });
            }
            cddl::ast::Type2::Typename { ident, .. } => {
                return Some(TypeKind::Alias {
                    target: cddl_to_rust_type(&ident.to_string()),
                });
            }
            _ => {
                return Some(TypeKind::Alias {
                    target: type2_to_rust_type(&tc.type1.type2),
                });
            }
        }
    }

    // Multiple typename choices
    let variant_refs: Vec<String> = choices
        .iter()
        .map(|tc| match &tc.type1.type2 {
            cddl::ast::Type2::Typename { ident, .. } => ident.to_string(),
            _ => "Unknown".to_string(),
        })
        .collect();

    let variants = variant_refs
        .into_iter()
        .map(|name| Variant {
            name: name.clone(),
            serde_rename: name.clone(),
            fields: vec![],
            is_unit: true,
        })
        .collect();
    Some(TypeKind::Enum { variants })
}

/// Find a literal text tag value in a group (e.g. "var" from `kind: "var"`).
fn find_literal_tag(group: &cddl::ast::Group) -> Option<String> {
    for choice in &group.group_choices {
        for (entry, _) in &choice.group_entries {
            if let cddl::ast::GroupEntry::ValueMemberKey { ge, .. } = entry {
                if let Some(mk) = &ge.member_key {
                    let is_kind = match mk {
                        cddl::ast::MemberKey::Value { value, .. } => {
                            matches!(value, cddl::token::Value::TEXT(t) if t == "kind")
                        }
                        cddl::ast::MemberKey::Bareword { ident, .. } => ident.to_string() == "kind",
                        _ => false,
                    };
                    if is_kind {
                        let tc = ge.entry_type.type_choices.first()?;
                        if let cddl::ast::Type2::TextValue { value: tag, .. } = &tc.type1.type2 {
                            return Some(tag.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

fn convert_group_entries(group: &cddl::ast::Group, exclude_tag: Option<&str>) -> Vec<Field> {
    let mut fields = Vec::new();
    for choice in &group.group_choices {
        for (entry, _) in &choice.group_entries {
            if let cddl::ast::GroupEntry::ValueMemberKey { ge, .. } = entry {
                let optional = ge
                    .occur
                    .as_ref()
                    .map(|o| matches!(o.occur, cddl::ast::Occur::Optional { .. }))
                    .unwrap_or(false);

                let field_name = match &ge.member_key {
                    Some(cddl::ast::MemberKey::Bareword { ident, .. }) => ident.to_string(),
                    Some(cddl::ast::MemberKey::Value { value, .. }) => {
                        if let cddl::token::Value::TEXT(text) = value {
                            text.to_string()
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                };

                if exclude_tag == Some(&field_name) {
                    continue;
                }

                let ty = type_to_rust_type(&ge.entry_type);
                let serde_rename = if field_name == to_snake_case(&field_name) {
                    None
                } else {
                    Some(field_name.clone())
                };

                fields.push(Field {
                    name: to_snake_case(&field_name),
                    ty,
                    optional,
                    serde_rename,
                });
            }
        }
    }
    fields
}

fn type_to_rust_type(ty: &cddl::ast::Type) -> String {
    let choices: Vec<_> = ty.type_choices.iter().collect();
    if choices.len() == 1 {
        type1_to_rust_type(&choices[0].type1)
    } else {
        let refs: Vec<String> = choices
            .iter()
            .map(|tc| type1_to_rust_type(&tc.type1))
            .collect();
        refs.join(" | ")
    }
}

fn type1_to_rust_type(t1: &cddl::ast::Type1) -> String {
    type2_to_rust_type(&t1.type2)
}

fn type2_to_rust_type(t2: &cddl::ast::Type2) -> String {
    match t2 {
        cddl::ast::Type2::Typename { ident, .. } => cddl_to_rust_type(&ident.to_string()),
        cddl::ast::Type2::TextValue { .. } => "String".to_string(),
        cddl::ast::Type2::IntValue { .. } | cddl::ast::Type2::UintValue { .. } => "i64".to_string(),
        cddl::ast::Type2::FloatValue { .. } => "f64".to_string(),
        cddl::ast::Type2::Map { .. } => "serde_json::Map<String, serde_json::Value>".to_string(),
        cddl::ast::Type2::Array { group, .. } => {
            let item = array_item_type(group);
            format!("Vec<{}>", item)
        }
        cddl::ast::Type2::ParenthesizedType { pt, .. } => type_to_rust_type(pt),
        _ => "serde_json::Value".to_string(),
    }
}

fn array_item_type(group: &cddl::ast::Group) -> String {
    for choice in &group.group_choices {
        for (entry, _) in &choice.group_entries {
            match entry {
                cddl::ast::GroupEntry::TypeGroupname { ge, .. } => {
                    return cddl_to_rust_type(&ge.name.to_string());
                }
                cddl::ast::GroupEntry::ValueMemberKey { ge, .. } => {
                    return type_to_rust_type(&ge.entry_type);
                }
                _ => {}
            }
        }
    }
    "serde_json::Value".to_string()
}

fn cddl_to_rust_type(name: &str) -> String {
    match name {
        "tstr" | "text" => "String".to_string(),
        "number" => "serde_json::Number".to_string(),
        "bool" => "bool".to_string(),
        "null" => "()".to_string(),
        "Value" => "serde_json::Value".to_string(),
        other => other.to_string(),
    }
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_lowercase().next().unwrap());
        } else {
            result.push(ch);
        }
    }
    result
}

fn to_pascal_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize = true;
    for ch in s.chars() {
        if ch == '_' || ch == '-' {
            capitalize = true;
        } else if capitalize {
            result.push(ch.to_uppercase().next().unwrap());
            capitalize = false;
        } else {
            result.push(ch);
        }
    }
    result
}
