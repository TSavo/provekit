use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::ir_builder;
use crate::types::Diagnostics;

const TS_EXT: &[&str] = &["ts", "tsx"];
const GO_EXT: &[&str] = &["go"];
const RS_EXT: &[&str] = &["rs"];

#[derive(Debug, Clone)]
struct OpInfo {
    operation_id: String,
    method: String,
    path_url: String,
    contract_name: String,
    response_constraints: Option<Value>,
}

pub struct AnnotateInput {
    pub spec_path: PathBuf,
    pub code_dir: PathBuf,
}

pub struct AnnotateOutput {
    pub files_written: usize,
    pub annotations_injected: usize,
    pub diagnostics: Diagnostics,
}

pub fn run_annotate(input: &AnnotateInput) -> Result<AnnotateOutput, String> {
    let contents =
        std::fs::read_to_string(&input.spec_path).map_err(|e| format!("read spec: {e}"))?;
    let doc: Value = if input
        .spec_path
        .extension()
        .map(|e| e == "yaml" || e == "yml")
        .unwrap_or(false)
    {
        let yaml_doc: serde_yaml::Value =
            serde_yaml::from_str(&contents).map_err(|e| format!("yaml parse: {e}"))?;
        let json_str = serde_json::to_string(&yaml_doc).map_err(|e| format!("yaml->json: {e}"))?;
        serde_json::from_str(&json_str).map_err(|e| format!("json parse: {e}"))?
    } else {
        serde_json::from_str(&contents).map_err(|e| format!("json parse: {e}"))?
    };

    let title = doc
        .pointer("/info/title")
        .and_then(|v| v.as_str())
        .unwrap_or("api");
    let version = doc
        .pointer("/info/version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0");
    let spec_prefix = slugify(&format!("{title}-{version}"));

    let mut ops: Vec<OpInfo> = Vec::new();

    let paths = match doc.get("paths") {
        Some(Value::Object(map)) => map,
        _ => return Err("no paths in spec".to_string()),
    };

    let schemas = doc
        .pointer("/components/schemas")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    for (path_url, path_item) in paths {
        let path_item = match path_item {
            Value::Object(m) => m,
            _ => continue,
        };
        for (method, operation) in path_item {
            if !is_http_method(method) {
                continue;
            }
            let operation = match operation {
                Value::Object(m) => m,
                _ => continue,
            };
            let op_id = operation
                .get("operationId")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| method)
                .to_string();
            let route_slug = slugify(&format!("{spec_prefix}-{method}-{op_id}"));

            let resp_constraints = operation
                .get("responses")
                .and_then(|v| v.as_object())
                .and_then(|responses| responses.get("200").or_else(|| responses.values().next()))
                .and_then(|resp| lift_response_body(resp, &schemas));

            let _req_constraints = operation
                .get("requestBody")
                .and_then(|body| lift_response_body(body, &schemas));

            let ct_slug = format!("{route_slug}-200-application-json");

            ops.push(OpInfo {
                operation_id: op_id.clone(),
                method: method.clone(),
                path_url: path_url.clone(),
                contract_name: ct_slug,
                response_constraints: resp_constraints,
            });
        }
    }

    let mut output = AnnotateOutput {
        files_written: 0,
        annotations_injected: 0,
        diagnostics: Diagnostics::new(),
    };

    for entry in walkdir::WalkDir::new(&input.code_dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let ext = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let lang = if TS_EXT.contains(&ext) {
            Lang::Ts
        } else if GO_EXT.contains(&ext) {
            Lang::Go
        } else if RS_EXT.contains(&ext) {
            Lang::Rs
        } else {
            continue;
        };

        let file_contents =
            std::fs::read_to_string(entry.path()).map_err(|e| format!("read: {e}"))?;

        let mut modified = file_contents.clone();
        let mut injected = 0usize;

        for op in &ops {
            let func_patterns = function_name_patterns(&op.operation_id);
            let mut found = false;

            for pattern in &func_patterns {
                if let Some((line_start, _line_end, indent)) =
                    find_function(&modified, pattern, lang)
                {
                    let annotation = build_annotation(op, lang);
                    let annotated_line =
                        build_annotation_line(lang, &annotation, &op.contract_name, indent);

                    let insertion_point = find_insertion_point(&modified, line_start);
                    modified.insert_str(insertion_point, &annotated_line);

                    found = true;
                    injected += 1;
                    break;
                }
            }

            if !found {
                output.diagnostics.push(format!(
                    "no match for operation {} in {}",
                    op.operation_id,
                    entry.path().display()
                ));
            }
        }

        if injected > 0 {
            std::fs::write(entry.path(), &modified).map_err(|e| format!("write: {e}"))?;
            output.files_written += 1;
            output.annotations_injected += injected;
        }
    }

    let annotation_file =
        build_annotation_defs(&ops, spec_prefix, lang_preference(&input.code_dir));
    let annot_path = input.code_dir.join("provekit-contracts.ts");
    std::fs::write(&annot_path, &annotation_file).map_err(|e| format!("write: {e}"))?;
    output.files_written += 1;
    output.annotations_injected += ops.len();

    Ok(output)
}

#[derive(Debug, Clone, Copy)]
enum Lang {
    Ts,
    Go,
    Rs,
}

fn lang_preference(code_dir: &Path) -> Lang {
    for entry in walkdir::WalkDir::new(code_dir)
        .max_depth(2)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
            if TS_EXT.contains(&ext) {
                return Lang::Ts;
            }
            if GO_EXT.contains(&ext) {
                return Lang::Go;
            }
            if RS_EXT.contains(&ext) {
                return Lang::Rs;
            }
        }
    }
    Lang::Ts
}

fn function_name_patterns(operation_id: &str) -> Vec<String> {
    use std::collections::HashSet;
    let mut patterns = vec![
        operation_id.to_string(),
        operation_id.to_lowercase(),
        slugify(operation_id).replace('-', "_"),
        to_camel_case(operation_id),
        to_pascal_case(operation_id),
    ];
    let mut seen = HashSet::new();
    patterns.retain(|p| seen.insert(p.clone()));
    patterns
}

fn find_function(content: &str, name: &str, lang: Lang) -> Option<(usize, usize, usize)> {
    let lines: Vec<&str> = content.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        let match_idx = match lang {
            Lang::Ts | Lang::Go => {
                if let Some(idx) = trimmed.find(&format!("function {name}")) {
                    Some(idx)
                } else if let Some(idx) = trimmed.find(&format!("async function {name}")) {
                    Some(idx)
                } else if let Some(idx) = trimmed.find(&format!("const {name} =")) {
                    Some(idx)
                } else if let Some(idx) = trimmed.find(&format!("export function {name}")) {
                    Some(idx)
                } else if let Some(idx) = trimmed.find(&format!("export async function {name}")) {
                    Some(idx)
                } else {
                    trimmed.find(&format!("fn {name}"))
                }
            }
            Lang::Rs => {
                if let Some(idx) = trimmed.find(&format!("fn {name}")) {
                    Some(idx)
                } else if let Some(idx) = trimmed.find(&format!("pub fn {name}")) {
                    Some(idx)
                } else if let Some(idx) = trimmed.find(&format!("pub async fn {name}")) {
                    Some(idx)
                } else {
                    trimmed.find(&format!("async fn {name}"))
                }
            }
        };

        if let Some(_idx) = match_idx {
            let indent = line.len() - line.trim_start().len();
            let start = content.lines().take(i).map(|l| l.len() + 1).sum::<usize>();
            let end = start + line.len();
            return Some((start, end, indent));
        }

        if trimmed.contains(name) {
            let lower = trimmed.to_lowercase();
            if lower.contains("export")
                || lower.contains("function")
                || lower.contains("fn ")
                || lower.contains("const ")
            {
                let indent = line.len() - line.trim_start().len();
                let start = content.lines().take(i).map(|l| l.len() + 1).sum::<usize>();
                let end = start + line.len();
                return Some((start, end, indent));
            }
        }
    }
    None
}

fn find_insertion_point(content: &str, line_start: usize) -> usize {
    let before = &content[..line_start];
    if let Some(last_newline) = before.rfind('\n') {
        last_newline + 1
    } else {
        0
    }
}

fn build_annotation(op: &OpInfo, _lang: Lang) -> String {
    let mut parts = vec![format!(
        "@provekit.target {}.{}",
        "openapi", op.contract_name
    )];

    if let Some(rc) = &op.response_constraints {
        parts.push(format!(
            "@provekit.post {}",
            serde_json::to_string(rc).unwrap_or_default()
        ));
    }

    parts.join("\n")
}

fn build_annotation_line(
    lang: Lang,
    annotation: &str,
    _contract_name: &str,
    indent: usize,
) -> String {
    let pad = " ".repeat(indent);
    let comment = match lang {
        Lang::Ts | Lang::Go | Lang::Rs => "//",
    };

    let mut result = String::new();
    for line in annotation.lines() {
        result.push_str(&format!("{pad}{comment} {line}\n"));
    }
    result
}

fn build_annotation_defs(ops: &[OpInfo], spec_prefix: String, lang: Lang) -> String {
    let comment = match lang {
        Lang::Ts | Lang::Go | Lang::Rs => "//",
    };

    let mut out = String::new();
    out.push_str(&format!(
        "{comment} Generated by provekit-lift-openapi --annotate\n"
    ));
    out.push_str(&format!("{comment} Spec: {spec_prefix}\n"));
    out.push_str(&format!(
        "{comment} These constraint annotations connect generated client code\n"
    ));
    out.push_str(&format!(
        "{comment} to proven OpenAPI endpoint contracts.\n"
    ));
    out.push_str(&format!(
        "{comment} When lifted by your language's provekit lifter,\n"
    ));
    out.push_str(&format!(
        "{comment} the verifier will flag any violation at build time.\n\n"
    ));

    for op in ops {
        out.push_str(&format!(
            "{comment} {method} {path}\n",
            method = op.method.to_uppercase(),
            path = op.path_url
        ));
        out.push_str(&format!(
            "{comment} @provekit.target openapi.{contract}\n",
            contract = op.contract_name
        ));
        if let Some(rc) = &op.response_constraints {
            out.push_str(&format!(
                "{comment} @provekit.post {post}\n",
                post = serde_json::to_string(rc).unwrap_or_default()
            ));
        }
        out.push('\n');
    }

    out
}

fn lift_response_body(body: &Value, schemas: &serde_json::Map<String, Value>) -> Option<Value> {
    let schema = if let Some(s) = body.get("schema") {
        s.clone()
    } else if let Some(content) = body.get("content") {
        if let Value::Object(ct) = content {
            let first = ct.values().next()?;
            first.get("schema")?.clone()
        } else {
            return None;
        }
    } else {
        return None;
    };

    let schema = resolve_schema(&schema, schemas);
    let constraints =
        schema_to_constraints_on_value_resolved(&schema, schemas, ir_builder::var("x"));
    Some(ir_builder::forall_ref("x", constraints))
}

fn schema_to_constraints_on_value_resolved(
    schema: &Value,
    schemas: &serde_json::Map<String, Value>,
    value: Value,
) -> Value {
    let schema = resolve_schema(schema, schemas);
    schema_to_constraints_on_value_inner(&schema, schemas, value)
}

fn schema_to_constraints_on_value_inner(
    schema: &Value,
    schemas: &serde_json::Map<String, Value>,
    value: Value,
) -> Value {
    let mut operands = Vec::new();

    let type_name = schema.get("type").and_then(|v| v.as_str());
    let nullable = schema
        .get("nullable")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !nullable && type_name.is_some() {
        operands.push(ir_builder::not_null(value.clone()));
    }

    match type_name {
        Some("integer") | Some("number") => {
            if let Some(min) = schema.get("minimum").and_then(|v| v.as_f64()) {
                if schema
                    .get("exclusiveMinimum")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    operands.push(ir_builder::gt(value.clone(), ir_builder::const_real(min)));
                } else {
                    operands.push(ir_builder::gte(
                        value.clone(),
                        ir_builder::const_int(min as i64),
                    ));
                }
            }
            if let Some(max) = schema.get("maximum").and_then(|v| v.as_f64()) {
                if schema
                    .get("exclusiveMaximum")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    operands.push(ir_builder::lt(value.clone(), ir_builder::const_real(max)));
                } else {
                    operands.push(ir_builder::lte(
                        value.clone(),
                        ir_builder::const_int(max as i64),
                    ));
                }
            }
        }
        Some("string") => {
            if let Some(min_len) = schema.get("minLength").and_then(|v| v.as_i64()) {
                operands.push(ir_builder::len_gte(value.clone(), min_len));
            }
            if let Some(max_len) = schema.get("maxLength").and_then(|v| v.as_i64()) {
                operands.push(ir_builder::len_lte(value.clone(), max_len));
            }
            if let Some(pattern) = schema.get("pattern").and_then(|v| v.as_str()) {
                operands.push(ir_builder::matches(value.clone(), pattern));
            }
            if let Some(format) = schema.get("format").and_then(|v| v.as_str()) {
                operands.push(ir_builder::atomic(format, vec![value.clone()]));
            }
        }
        Some("boolean") => {
            operands.push(ir_builder::eq(
                ir_builder::ctor("kind_of", vec![value.clone()]),
                ir_builder::const_string("boolean"),
            ));
        }
        Some("array") => {
            if let Some(items) = schema.get("items") {
                let items_schema = resolve_schema(items, schemas);
                operands.push(ir_builder::forall(
                    "_elem",
                    ir_builder::ref_sort(),
                    schema_to_constraints_on_value_resolved(
                        &items_schema,
                        schemas,
                        ir_builder::var("_elem"),
                    ),
                ));
            }
            if let Some(min_items) = schema.get("minItems").and_then(|v| v.as_i64()) {
                operands.push(ir_builder::len_gte(value.clone(), min_items));
            }
            if let Some(max_items) = schema.get("maxItems").and_then(|v| v.as_i64()) {
                operands.push(ir_builder::len_lte(value.clone(), max_items));
            }
        }
        Some("object") => {
            if let Some(properties) = schema.get("properties").and_then(|v| v.as_object()) {
                let required: Vec<&str> = schema
                    .get("required")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();

                for (prop_name, prop_schema) in properties {
                    let prop_schema = resolve_schema(prop_schema, schemas);
                    let field = ir_builder::field_access_val(prop_name, &value);
                    if required.contains(&prop_name.as_str()) {
                        operands.push(ir_builder::not_null(field.clone()));
                    }
                    let pc = schema_to_constraints_on_value_inner(&prop_schema, schemas, field);
                    if !is_true(&pc) {
                        operands.push(pc);
                    }
                }
            }
        }
        _ => {}
    }

    if let Some(enum_vals) = schema.get("enum").and_then(|v| v.as_array()) {
        let choices: Vec<Value> = enum_vals
            .iter()
            .filter_map(|v| {
                if v.is_number() {
                    Some(ir_builder::const_int(v.as_i64().unwrap_or(0)))
                } else if v.is_string() {
                    Some(ir_builder::const_string(v.as_str().unwrap_or("")))
                } else {
                    Some(v.clone())
                }
            })
            .collect();
        if !choices.is_empty() {
            operands.push(ir_builder::member_of(value.clone(), choices));
        }
    }

    if operands.is_empty() {
        return ir_builder::atomic("true", vec![]);
    }
    if operands.len() == 1 {
        return operands.into_iter().next().unwrap();
    }
    ir_builder::and(operands)
}

fn resolve_schema(schema: &Value, schemas: &serde_json::Map<String, Value>) -> Value {
    if let Some(ref_str) = schema.get("$ref").and_then(|v| v.as_str()) {
        if ref_str.starts_with("#/components/schemas/") {
            let name = ref_str.trim_start_matches("#/components/schemas/");
            if let Some(resolved) = schemas.get(name) {
                return resolved.clone();
            }
        }
    }
    schema.clone()
}

fn is_true(f: &Value) -> bool {
    f.get("name")
        .and_then(|v| v.as_str())
        .map(|n| n == "true")
        .unwrap_or(false)
}

fn slugify(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase()
}

fn is_http_method(s: &str) -> bool {
    matches!(
        s,
        "get" | "post" | "put" | "delete" | "patch" | "head" | "options"
    )
}

fn to_camel_case(s: &str) -> String {
    let parts: Vec<&str> = s
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .collect::<Vec<_>>()
        .into_iter()
        .flat_map(|p| p.split('_'))
        .filter(|p| !p.is_empty())
        .collect();

    let mut result = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            result.push_str(&part.to_lowercase());
        } else {
            let mut chars = part.chars();
            if let Some(first) = chars.next() {
                result.push(first.to_ascii_uppercase());
                result.push_str(&chars.as_str().to_lowercase());
            }
        }
    }
    result
}

fn to_pascal_case(s: &str) -> String {
    let mut camel = to_camel_case(s);
    if let Some(first) = camel.chars().next() {
        let rest: String = camel.chars().skip(1).collect();
        camel = format!("{}{}", first.to_ascii_uppercase(), rest);
    }
    camel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_pascal_case_function() {
        let result = find_function(
            "export async function getUsers(): Promise<User[]> {",
            "getUsers",
            Lang::Ts,
        );
        assert!(result.is_some());
    }

    #[test]
    fn pattern_variants_cover_kebab_to_camel() {
        let patterns = function_name_patterns("get-users");
        assert!(patterns.contains(&"getUsers".to_string()));
    }

    #[test]
    fn annotate_produces_file() {
        let spec = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1.0"
paths:
  /users:
    get:
      operationId: getUsers
      responses:
        '200':
          description: OK
          content:
            application/json:
              schema:
                type: array
                items:
                  $ref: '#/components/schemas/User'
components:
  schemas:
    User:
      type: object
      required: [id]
      properties:
        id:
          type: integer
          minimum: 1
"#;
        let dir = std::env::temp_dir().join("provekit-annotate-test");
        let spec_path = dir.join("spec.yaml");
        let code_path = dir.join("generated");

        std::fs::create_dir_all(&code_path).unwrap();
        std::fs::write(&spec_path, spec).unwrap();
        std::fs::write(
            code_path.join("client.ts"),
            "export async function getUsers(): Promise<User[]> { return client.get('/users'); }\n",
        )
        .unwrap();

        let result = run_annotate(&AnnotateInput {
            spec_path,
            code_dir: code_path.clone(),
        })
        .unwrap();

        assert_eq!(result.files_written, 2);
        assert!(result.annotations_injected >= 2);
    }

    #[test]
    fn resolves_ref_schema_constraints() {
        let schemas: serde_json::Map<String, Value> = serde_json::json!({
            "User": {
                "type": "object",
                "required": ["id"],
                "properties": {
                    "id": {
                        "type": "integer",
                        "minimum": 1
                    }
                }
            }
        })
        .as_object()
        .unwrap()
        .clone();

        let array_schema = serde_json::json!({
            "type": "array",
            "items": {
                "$ref": "#/components/schemas/User"
            }
        });

        let result =
            schema_to_constraints_on_value_resolved(&array_schema, &schemas, ir_builder::var("x"));

        let json = serde_json::to_string(&result).unwrap();
        assert!(
            json.contains("\"name\":\"id\""),
            "should contain id field: {json}"
        );
    }
}
