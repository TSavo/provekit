use std::path::Path;

use serde_json::Value;

use crate::ir_builder;
use crate::types::{BridgeDecl, ContractDecl, Declaration, Diagnostics};

pub fn lift_spec(path: &Path, diagnostics: &mut Diagnostics) -> Result<Vec<Declaration>, String> {
    let contents = std::fs::read_to_string(path).map_err(|e| format!("read: {e}"))?;
    let doc: Value = if path
        .extension()
        .map(|e| e == "yaml" || e == "yml")
        .unwrap_or(false)
    {
        let yaml_doc: serde_yaml::Value =
            serde_yaml::from_str(&contents).map_err(|e| format!("yaml parse: {e}"))?;
        let json_str =
            serde_json::to_string(&yaml_doc).map_err(|e| format!("yaml->json: {e}"))?;
        serde_json::from_str(&json_str).map_err(|e| format!("json parse: {e}"))?
    } else {
        serde_json::from_str(&contents).map_err(|e| format!("json parse: {e}"))?
    };

    let mut spec = SpecResolver { doc, diagnostics };
    spec.resolve_all()
}

struct SpecResolver<'a> {
    doc: Value,
    diagnostics: &'a mut Diagnostics,
}

impl<'a> SpecResolver<'a> {
    fn resolve_all(&mut self) -> Result<Vec<Declaration>, String> {
        let mut decls: Vec<Declaration> = Vec::new();

        let title = self
            .doc
            .pointer("/info/title")
            .and_then(|v| v.as_str())
            .unwrap_or("api");
        let version = self
            .doc
            .pointer("/info/version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.0.0");

        let spec_prefix = slugify(&format!("{title}-{version}"));

        let paths = match self.doc.get("paths") {
            Some(Value::Object(map)) => map.clone(),
            _ => {
                self.diagnostics.push("no paths found in spec");
                return Ok(decls);
            }
        };

        let schemas = self
            .doc
            .pointer("/components/schemas")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        for (_path_url, path_item) in &paths {
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

                let route_slug =
                    slugify(&format!("{spec_prefix}-{method}-{op_id}"));

                if let Some(request_body) = operation.get("requestBody") {
                    if let Some((contract, bridge)) = self.lift_body(
                        request_body,
                        &format!("{route_slug}-request"),
                        &schemas,
                    ) {
                        decls.push(Declaration::Contract(contract));
                        if let Some(b) = bridge {
                            decls.push(Declaration::Bridge(b));
                        }
                    }
                }

                if let Some(responses) = operation.get("responses") {
                    if let Value::Object(resp_map) = responses {
                        for (status_code, response) in resp_map {
                            let sc = status_code
                                .chars()
                                .take(3)
                                .collect::<String>();
                            let status_slug =
                                slugify(&format!("{route_slug}-{sc}"));

                            if let Some((contract, bridge)) = self.lift_body(
                                response,
                                &status_slug,
                                &schemas,
                            ) {
                                decls.push(Declaration::Contract(contract));
                                if let Some(b) = bridge {
                                    decls.push(Declaration::Bridge(b));
                                }
                            }
                        }
                    }
                }

                if let Some(parameters) = operation.get("parameters") {
                    if let Value::Array(params) = parameters {
                        let param_constraints =
                            self.lift_parameters(params);
                        if let Some(formula) = param_constraints {
                            let name = format!("{route_slug}-params");
                            decls.push(Declaration::Contract(ContractDecl {
                                name,
                                out_binding: "out".to_string(),
                                pre: Some(formula),
                                post: None,
                                inv: None,
                            }));
                        }
                    }
                }
            }
        }

        for (schema_name, schema_def) in &schemas {
            let name = slugify(&format!("{spec_prefix}-schema-{schema_name}"));
            if let Some((contract, _bridge)) =
                self.lift_schema_to_contract(name.clone(), schema_def, &schemas)
            {
                decls.push(Declaration::Contract(contract));
            }
        }

        Ok(decls)
    }

    fn lift_body(
        &self,
        body: &Value,
        base_name: impl Into<String>,
        schemas: &serde_json::Map<String, Value>,
    ) -> Option<(ContractDecl, Option<BridgeDecl>)> {
        let base_name = base_name.into();

        let schema = resolve_schema(body, schemas);

        let content = body
            .get("content")
            .or_else(|| body.get("application/json").and_then(|_| Some(body)));

        if let Some(Value::Object(content_map)) = content {
            for (content_type, media_type) in content_map {
                let ct_slug = slugify(&format!("{base_name}-{content_type}"));
                if let Some(schema_val) = media_type.get("schema") {
                    let resolved = resolve_schema(schema_val, schemas);
                    return self.lift_schema_to_contract(ct_slug, &resolved, schemas);
                }
            }

            if let Some(schema_val) = content_map.get("application/json").or(content_map.values().next()) {
                let resolved = resolve_schema(schema_val, schemas);
                return self.lift_schema_to_contract(base_name, &resolved, schemas);
            }
            return None;
        }

        let ct_slug = slugify(&format!("{base_name}-json"));
        self.lift_schema_to_contract(ct_slug, &schema, schemas)
    }

    fn lift_schema_to_contract(
        &self,
        name: String,
        schema: &Value,
        schemas: &serde_json::Map<String, Value>,
    ) -> Option<(ContractDecl, Option<BridgeDecl>)> {
        let schema = resolve_schema(schema, schemas);
        let constraints = schema_to_constraints(&schema, schemas, "x");

        let post = ir_builder::forall_ref("x", constraints);

        let contract = ContractDecl {
            name,
            out_binding: "out".to_string(),
            pre: None,
            post: Some(post),
            inv: None,
        };

        let bridge: Option<BridgeDecl> = None;
        Some((contract, bridge))
    }

    fn lift_parameters(
        &self,
        params: &Vec<Value>,
    ) -> Option<Value> {
        let mut operands = Vec::new();

        for param in params {
            let name = param.get("name").and_then(|v| v.as_str())?;
            let required = param
                .get("required")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let _param_schema = param.get("schema").unwrap_or(param);

            if required {
                let field = ir_builder::field_access(name, "x");
                operands.push(ir_builder::not_null(field));
            }

            if let Some(schema) = param.get("schema") {
                let resolved = resolve_schema_ref(schema);
                let constraints =
                    schema_to_constraints_on_field(&resolved, name, "x");
                if !is_true(&constraints) {
                    operands.push(constraints);
                }
            }
        }

        if operands.is_empty() {
            return None;
        }
        Some(ir_builder::forall_ref("x", ir_builder::and(operands)))
    }
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
    matches!(s, "get" | "post" | "put" | "delete" | "patch" | "head" | "options")
}

fn resolve_schema_ref(schema: &Value) -> Value {
    if let Some(ref_path) = schema.get("$ref").and_then(|v| v.as_str()) {
        return Value::String(ref_path.to_string());
    }
    schema.clone()
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

fn schema_to_constraints(
    schema: &Value,
    schemas: &serde_json::Map<String, Value>,
    var_name: &str,
) -> Value {
    let schema = resolve_schema(schema, schemas);
    let base = schema_to_constraints_inner(&schema, schemas, var_name);
    let extras = extras_from_schema(&schema, schemas, var_name);
    merge_constraints(base, extras)
}

fn schema_to_constraints_on_field(
    schema: &Value,
    field_name: &str,
    var_name: &str,
) -> Value {
    let field = ir_builder::field_access(field_name, var_name);
    let inner = schema_to_constraints_on_value(schema, field);
    inner
}

fn schema_to_constraints_on_value(schema: &Value, value: Value) -> Value {
    let mut operands = Vec::new();

    let type_name = schema
        .get("type")
        .and_then(|v| v.as_str());

    let nullable = schema
        .get("nullable")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !nullable {
        if type_name.is_some() {
            operands.push(ir_builder::not_null(value.clone()));
        }
    }

    match type_name {
        Some("integer") | Some("number") => {
            if let Some(min) = schema.get("minimum").and_then(|v| v.as_f64()) {
                if schema
                    .get("exclusiveMinimum")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    operands.push(ir_builder::gt(
                        value.clone(),
                        ir_builder::const_real(min),
                    ));
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
                    operands.push(ir_builder::lt(
                        value.clone(),
                        ir_builder::const_real(max),
                    ));
                } else {
                    operands.push(ir_builder::lte(
                        value.clone(),
                        ir_builder::const_int(max as i64),
                    ));
                }
            }
            if let Some(multiple) =
                schema.get("multipleOf").and_then(|v| v.as_f64())
            {
                if type_name == Some("integer") {
                    operands.push(ir_builder::eq(
                        ir_builder::ctor("mod", vec![
                            value.clone(),
                            ir_builder::const_int(multiple as i64),
                        ]),
                        ir_builder::const_int(0),
                    ));
                }
            }
        }
        Some("string") => {
            if let Some(min_len) =
                schema.get("minLength").and_then(|v| v.as_i64())
            {
                operands
                    .push(ir_builder::len_gte(value.clone(), min_len));
            }
            if let Some(max_len) =
                schema.get("maxLength").and_then(|v| v.as_i64())
            {
                operands
                    .push(ir_builder::len_lte(value.clone(), max_len));
            }
            if let Some(pattern) =
                schema.get("pattern").and_then(|v| v.as_str())
            {
                operands
                    .push(ir_builder::matches(value.clone(), pattern));
            }
            if let Some(format) =
                schema.get("format").and_then(|v| v.as_str())
            {
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
                let items_schema = resolve_schema_ref(items);
                operands.push(ir_builder::forall(
                    "_elem",
                    ir_builder::ref_sort(),
                    schema_to_constraints_on_value(
                        &items_schema,
                        ir_builder::var("_elem"),
                    ),
                ));
            }
            if let Some(min_items) =
                schema.get("minItems").and_then(|v| v.as_i64())
            {
                operands.push(ir_builder::len_gte(
                    value.clone(),
                    min_items,
                ));
            }
            if let Some(max_items) =
                schema.get("maxItems").and_then(|v| v.as_i64())
            {
                operands.push(ir_builder::len_lte(
                    value.clone(),
                    max_items,
                ));
            }
        }
        Some("object") => {
            if let Some(properties) =
                schema.get("properties").and_then(|v| v.as_object())
            {
                let required: Vec<&str> = schema
                    .get("required")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .collect()
                    })
                    .unwrap_or_default();

                for (prop_name, prop_schema) in properties {
                    let prop_schema = resolve_schema_ref(prop_schema);
                    let field =
                        ir_builder::field_access_val(prop_name, &value);

                    if required.contains(&prop_name.as_str()) {
                        operands
                            .push(ir_builder::not_null(field.clone()));
                    }

                    let prop_constraints =
                        schema_to_constraints_on_value(&prop_schema, field);
                    if !is_true(&prop_constraints) {
                        operands.push(prop_constraints);
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
                    Some(ir_builder::const_int(
                        v.as_i64().unwrap_or(0),
                    ))
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

    if let Some(all_of) = schema.get("allOf").and_then(|v| v.as_array()) {
        for sub in all_of {
            let sub = resolve_schema_ref(sub);
            let constraints =
                schema_to_constraints_on_value(&sub, value.clone());
            if !is_true(&constraints) {
                operands.push(constraints);
            }
        }
    }

    if let Some(one_of) = schema.get("oneOf").and_then(|v| v.as_array()) {
        let branches: Vec<Value> = one_of
            .iter()
            .map(|sub| {
                let sub = resolve_schema_ref(sub);
                schema_to_constraints_on_value(&sub, value.clone())
            })
            .filter(|c| !is_true(c))
            .collect();
        if !branches.is_empty() {
            operands.push(ir_builder::atomic(
                "one_of",
                vec![value.clone(), ir_builder::ctor("Variants", branches)],
            ));
        }
    }

    if let Some(any_of) = schema.get("anyOf").and_then(|v| v.as_array()) {
        let branches: Vec<Value> = any_of
            .iter()
            .map(|sub| {
                let sub = resolve_schema_ref(sub);
                schema_to_constraints_on_value(&sub, value.clone())
            })
            .filter(|c| !is_true(c))
            .collect();
        if !branches.is_empty() {
            operands.push(ir_builder::or(branches));
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

fn schema_to_constraints_inner(
    schema: &Value,
    schemas: &serde_json::Map<String, Value>,
    var_name: &str,
) -> Value {
    let schema = resolve_schema(schema, schemas);
    schema_to_constraints_on_value(&schema, ir_builder::var(var_name))
}

fn extras_from_schema(
    schema: &Value,
    _schemas: &serde_json::Map<String, Value>,
    var_name: &str,
) -> Value {
    let mut operands = Vec::new();

    let required: Vec<&str> = schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    for field_name in &required {
        let field = ir_builder::field_access(field_name, var_name);
        operands.push(ir_builder::not_null(field));
    }

    if operands.is_empty() {
        return ir_builder::atomic("true", vec![]);
    }
    if operands.len() == 1 {
        return operands.into_iter().next().unwrap();
    }
    ir_builder::and(operands)
}

fn merge_constraints(a: Value, b: Value) -> Value {
    if is_true(&a) {
        return b;
    }
    if is_true(&b) {
        return a;
    }
    ir_builder::and(vec![a, b])
}

fn is_true(f: &Value) -> bool {
    f.get("name")
        .and_then(|v| v.as_str())
        .map(|n| n == "true")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PETSTORE_YAML: &str = r#"
openapi: "3.0.0"
info:
  title: Petstore
  version: "1.0.0"
paths:
  /pets:
    get:
      operationId: listPets
      parameters:
        - name: limit
          in: query
          required: false
          schema:
            type: integer
            minimum: 1
            maximum: 100
      responses:
        '200':
          description: A list of pets
          content:
            application/json:
              schema:
                type: array
                items:
                  $ref: '#/components/schemas/Pet'
    post:
      operationId: createPet
      requestBody:
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/NewPet'
      responses:
        '201':
          description: Created
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Pet'
components:
  schemas:
    Pet:
      type: object
      required: [id, name]
      properties:
        id:
          type: integer
          minimum: 1
        name:
          type: string
          minLength: 1
        tag:
          type: string
    NewPet:
      type: object
      required: [name]
      properties:
        name:
          type: string
          minLength: 1
        tag:
          type: string
"#;

    #[test]
    fn lifts_petstore_yaml() {
        let tmp = std::env::temp_dir().join("petstore-test.yaml");
        std::fs::write(&tmp, PETSTORE_YAML).unwrap();

        let mut diag = Diagnostics::new();
        let decls = lift_spec(&tmp, &mut diag).unwrap();

        assert!(!decls.is_empty());

        let contract_count = decls
            .iter()
            .filter(|d| matches!(d, Declaration::Contract(_)))
            .count();
        assert!(contract_count >= 3);
    }

    #[test]
    fn extracts_integer_constraints() {
        let schema: Value = serde_json::json!({
            "type": "object",
            "required": ["age"],
            "properties": {
                "age": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": 150
                }
            }
        });

        let formula = schema_to_constraints(&schema, &Default::default(), "x");
        let json_str = serde_json::to_string(&formula).unwrap();

        assert!(json_str.contains("\"name\":\"\u{2265}\""));
        assert!(json_str.contains("\"name\":\"\u{2264}\""));
        assert!(json_str.contains("\"name\":\"age\""));
        assert!(json_str.contains("age"));
    }

    #[test]
    fn extracts_string_constraints() {
        let schema: Value = serde_json::json!({
            "type": "object",
            "properties": {
                "email": {
                    "type": "string",
                    "format": "email",
                    "minLength": 5
                }
            }
        });

        let formula = schema_to_constraints(&schema, &Default::default(), "x");
        let json_str = serde_json::to_string(&formula).unwrap();

        assert!(json_str.contains("email"));
    }

    #[test]
    fn extracts_enum_constraints() {
        let schema: Value = serde_json::json!({
            "type": "object",
            "properties": {
                "role": {
                    "type": "string",
                    "enum": ["admin", "user", "guest"]
                }
            }
        });

        let formula = schema_to_constraints(&schema, &Default::default(), "x");
        let json_str = serde_json::to_string(&formula).unwrap();

        assert!(json_str.contains("member"));
        assert!(json_str.contains("admin"));
    }
}
