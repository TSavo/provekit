use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use crate::ir_builder;
use crate::types::{ContractDecl, Declaration, Diagnostics};

pub fn lift_proto(path: &Path, diagnostics: &mut Diagnostics) -> Result<Vec<Declaration>, String> {
    let contents = std::fs::read_to_string(path).map_err(|e| format!("read: {e}"))?;

    let mut parser = ProtoParser {
        messages: Vec::new(),
        enums: HashMap::new(),
        rpcs: Vec::new(),
    };

    parser.parse(&contents)?;

    let mut decls: Vec<Declaration> = Vec::new();

    for msg in &parser.messages {
        let name = format!("proto-{}", slugify(&msg.name));
        if let Some(contract) = message_to_contract(&name, msg) {
            decls.push(Declaration::Contract(contract));
            diagnostics.push(format!("lifted message {}", msg.name));
        }
    }

    for _enum_def in parser.messages.iter().filter_map(|m| {
        if m.fields.is_empty() && !m.nested.is_empty() {
            Some(m)
        } else {
            None
        }
    }) {}

    for rpc in &parser.rpcs {
        let name = format!("proto-rpc-{}", slugify(&rpc.name));

        if !rpc.request_type.is_empty() {
            let req_name = format!("{name}-request");
            let has_type = ir_builder::atomic(
                "has_type",
                vec![
                    ir_builder::var("x"),
                    ir_builder::const_string(&rpc.request_type),
                ],
            );
            let post = ir_builder::forall_ref("x", has_type);
            decls.push(Declaration::Contract(ContractDecl {
                name: req_name,
                out_binding: "out".to_string(),
                pre: None,
                post: Some(post),
                inv: None,
            }));
        }

        if !rpc.response_type.is_empty() {
            let resp_name = format!("{name}-response");
            let has_type = ir_builder::atomic(
                "has_type",
                vec![
                    ir_builder::var("x"),
                    ir_builder::const_string(&rpc.response_type),
                ],
            );
            let post = ir_builder::forall_ref("x", has_type);
            decls.push(Declaration::Contract(ContractDecl {
                name: resp_name,
                out_binding: "out".to_string(),
                pre: None,
                post: Some(post),
                inv: None,
            }));
        }
    }

    Ok(decls)
}

fn slugify(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .to_lowercase()
}

fn message_to_contract(name: &str, msg: &ProtoMessage) -> Option<ContractDecl> {
    let mut operands = Vec::new();

    for field in &msg.fields {
        let field_var = ir_builder::field_access(&field.name, "x");
        let _typ = proto_to_ir_sort(&field.field_type);

        match field.label {
            FieldLabel::Required => {
                operands.push(ir_builder::not_null(field_var.clone()));
            }
            FieldLabel::Optional => {}
            FieldLabel::Repeated => {}

            FieldLabel::None => {}
        }

        let type_constraint = match field.field_type.as_str() {
            "int32" | "int64" | "sint32" | "sint64" | "sfixed32" | "sfixed64" => {
                ir_builder::atomic("is_int", vec![field_var.clone()])
            }
            "uint32" | "uint64" | "fixed32" | "fixed64" => {
                ir_builder::gte(field_var.clone(), ir_builder::const_int(0))
            }
            "float" | "double" => ir_builder::atomic("is_real", vec![field_var.clone()]),
            "bool" => ir_builder::atomic("is_bool", vec![field_var.clone()]),
            "string" => ir_builder::atomic("is_string", vec![field_var.clone()]),
            "bytes" => ir_builder::atomic("is_string", vec![field_var.clone()]),
            name if name.starts_with("map<") => {
                ir_builder::atomic("is_map", vec![field_var.clone()])
            }
            _ => ir_builder::atomic("true", vec![]),
        };

        if !is_true(&type_constraint) {
            operands.push(type_constraint);
        }
    }

    if operands.is_empty() {
        return Some(ContractDecl {
            name: name.to_string(),
            out_binding: "out".to_string(),
            pre: None,
            post: Some(ir_builder::atomic("true", vec![])),
            inv: None,
        });
    }

    let post = ir_builder::forall_ref("x", ir_builder::and(operands));

    Some(ContractDecl {
        name: name.to_string(),
        out_binding: "out".to_string(),
        pre: None,
        post: Some(post),
        inv: None,
    })
}

fn proto_to_ir_sort(proto_type: &str) -> Value {
    match proto_type {
        "int32" | "int64" | "uint32" | "uint64" | "sint32" | "sint64" | "fixed32" | "fixed64"
        | "sfixed32" | "sfixed64" => ir_builder::int_sort(),
        "float" | "double" => ir_builder::real_sort(),
        "bool" => ir_builder::bool_sort(),
        "string" | "bytes" => ir_builder::string_sort(),
        _ => ir_builder::ref_sort(),
    }
}

fn is_true(f: &Value) -> bool {
    f.get("name")
        .and_then(|v| v.as_str())
        .map(|n| n == "true")
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
enum FieldLabel {
    Required,
    Optional,
    Repeated,
    None,
}

#[derive(Debug, Clone)]
struct ProtoField {
    name: String,
    field_type: String,
    label: FieldLabel,
}

#[derive(Debug, Clone)]
struct ProtoMessage {
    name: String,
    fields: Vec<ProtoField>,
    nested: Vec<ProtoMessage>,
}

#[derive(Debug, Clone)]
struct ProtoRpc {
    name: String,
    request_type: String,
    response_type: String,
}

struct ProtoParser {
    messages: Vec<ProtoMessage>,
    enums: HashMap<String, Vec<String>>,
    rpcs: Vec<ProtoRpc>,
}

impl ProtoParser {
    fn parse(&mut self, input: &str) -> Result<(), String> {
        let lines: Vec<&str> = input.lines().collect();
        let mut pos = 0;
        while pos < lines.len() {
            let line = lines[pos].trim();

            if line.is_empty()
                || line.starts_with("//")
                || line.starts_with("syntax")
                || line.starts_with("package")
                || line.starts_with("import")
                || line.starts_with("option")
            {
                pos += 1;
                continue;
            }

            if line.starts_with("message ") {
                let (msg, consumed) = self.parse_message(&lines, pos)?;
                self.messages.push(msg);
                pos += consumed;
            } else if line.starts_with("service ") {
                let (svc_rpcs, consumed) = self.parse_service(&lines, pos)?;
                self.rpcs.extend(svc_rpcs);
                pos += consumed;
            } else if line.starts_with("enum ") {
                let (enum_name, values, consumed) = self.parse_enum(&lines, pos)?;
                self.enums.insert(enum_name, values);
                pos += consumed;
            } else if line.starts_with("/*") {
                pos += self.skip_block_comment(&lines, pos);
            } else {
                pos += 1;
            }
        }
        Ok(())
    }

    fn parse_message(&self, lines: &[&str], start: usize) -> Result<(ProtoMessage, usize), String> {
        let header = lines[start].trim();
        let name = header
            .trim_start_matches("message ")
            .trim_end_matches(" {")
            .trim()
            .to_string();

        let mut pos = start + 1;
        let mut fields: Vec<ProtoField> = Vec::new();
        let mut nested: Vec<ProtoMessage> = Vec::new();

        while pos < lines.len() {
            let line = lines[pos].trim();

            if line.is_empty() || line.starts_with("//") {
                pos += 1;
                continue;
            }
            if line.starts_with("/*") {
                pos += self.skip_block_comment(lines, pos);
                continue;
            }
            if line == "}" {
                pos += 1;
                break;
            }

            if line.starts_with("message ") {
                let (nested_msg, consumed) = self.parse_message(lines, pos)?;
                nested.push(nested_msg);
                pos += consumed;
                continue;
            }

            if line.starts_with("enum ") {
                let (_, _, consumed) = self.parse_enum(lines, pos)?;
                pos += consumed;
                continue;
            }

            if line.starts_with("oneof ") {
                let consumed = self.skip_block(lines, pos)?;
                pos += consumed;
                continue;
            }

            if line.starts_with("map<") {
                if let Some((field_name, _number)) = parse_field_tail(line) {
                    let map_type = line
                        .trim_end_matches(&format!(" {} = {};", field_name, _number))
                        .trim()
                        .to_string();
                    fields.push(ProtoField {
                        name: field_name,
                        field_type: map_type,
                        label: FieldLabel::None,
                    });
                }
                pos += 1;
                continue;
            }

            if let Some((label, field_type, field_name, _number)) = parse_field_line(line) {
                fields.push(ProtoField {
                    name: field_name,
                    field_type,
                    label,
                });
            }

            pos += 1;
        }

        Ok((
            ProtoMessage {
                name,
                fields,
                nested,
            },
            pos - start,
        ))
    }

    fn parse_service(
        &self,
        lines: &[&str],
        start: usize,
    ) -> Result<(Vec<ProtoRpc>, usize), String> {
        let mut pos = start + 1;
        let mut rpcs: Vec<ProtoRpc> = Vec::new();

        while pos < lines.len() {
            let line = lines[pos].trim();

            if line.is_empty() || line.starts_with("//") {
                pos += 1;
                continue;
            }
            if line == "}" {
                pos += 1;
                break;
            }

            if line.starts_with("rpc ") {
                if let Some(rpc) = parse_rpc_line(line, lines, pos) {
                    rpcs.push(rpc);
                }
            }

            pos += 1;
        }
        Ok((rpcs, pos - start))
    }

    fn parse_enum(
        &self,
        lines: &[&str],
        start: usize,
    ) -> Result<(String, Vec<String>, usize), String> {
        let header = lines[start].trim();
        let name = header
            .trim_start_matches("enum ")
            .trim_end_matches(" {")
            .trim()
            .to_string();

        let mut pos = start + 1;
        let mut values: Vec<String> = Vec::new();

        while pos < lines.len() {
            let line = lines[pos].trim();
            if line.is_empty() || line.starts_with("//") {
                pos += 1;
                continue;
            }
            if line == "}" {
                pos += 1;
                break;
            }
            if let Some(val_name) = parse_enum_value(line) {
                values.push(val_name);
            }
            pos += 1;
        }
        Ok((name, values, pos - start))
    }

    fn skip_block(&self, lines: &[&str], start: usize) -> Result<usize, String> {
        let mut depth = 0;
        let mut pos = start;
        while pos < lines.len() {
            let line = lines[pos];
            for c in line.chars() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(pos - start + 1);
                        }
                    }
                    _ => {}
                }
            }
            pos += 1;
        }
        Err("unclosed block".to_string())
    }

    fn skip_block_comment(&self, lines: &[&str], start: usize) -> usize {
        let mut pos = start;
        while pos < lines.len() {
            if lines[pos].contains("*/") {
                return pos - start + 1;
            }
            pos += 1;
        }
        pos - start
    }
}

fn parse_field_line(line: &str) -> Option<(FieldLabel, String, String, i64)> {
    let line = line.trim_end_matches(';');
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }

    let (label, type_start, name_idx) = match parts[0] {
        "required" => (FieldLabel::Required, 1, 2),
        "optional" => (FieldLabel::Optional, 1, 2),
        "repeated" => (FieldLabel::Repeated, 1, 2),
        name if name.ends_with('}') => return None,
        "oneof" => return None,
        "map" => (FieldLabel::None, 1, 2),
        _ => (FieldLabel::None, 0, 1),
    };

    if parts.len() <= name_idx + 2 {
        return None;
    }

    let field_type = parts[type_start].to_string();
    let field_name = parts[name_idx].to_string();

    if parts[name_idx + 1] != "=" {
        return None;
    }

    let number: i64 = parts[name_idx + 2].parse().ok()?;

    Some((label, field_type, field_name, number))
}

fn parse_field_tail(line: &str) -> Option<(String, i64)> {
    let line = line.trim_end_matches(';');
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }
    let name_pos = parts.len() - 3;
    let name = parts[name_pos].to_string();
    if parts[name_pos + 1] != "=" {
        return None;
    }
    let number: i64 = parts[name_pos + 2].parse().ok()?;
    Some((name, number))
}

fn parse_rpc_line(line: &str, lines: &[&str], pos: usize) -> Option<ProtoRpc> {
    let line = line.trim();

    let name = line.strip_prefix("rpc ")?.split_whitespace().next()?;

    let single_line = format!("{}", line.trim_end_matches(';'));

    if let Some(rest) = single_line.strip_prefix(&format!("rpc {} (", name)) {
        if let Some(idx) = rest.find(") returns (") {
            let request_type = rest[..idx].trim_start_matches("stream ").to_string();
            let response_type = rest[idx + 11..]
                .trim_start_matches("stream ")
                .trim_end_matches(')')
                .trim()
                .to_string();
            if !request_type.is_empty() || !response_type.is_empty() {
                return Some(ProtoRpc {
                    name: name.to_string(),
                    request_type,
                    response_type,
                });
            }
        }
    }

    let mut request_type = String::new();
    let mut response_type = String::new();
    let mut buffer = line.to_string();

    for offset in (pos + 1)..lines.len() {
        let l = lines[offset].trim();
        if l.is_empty() {
            continue;
        }

        buffer.push(' ');
        buffer.push_str(l);

        let trimmed = buffer.trim();
        let rest = trimmed.strip_prefix(&format!("rpc {} (", name))?;
        if let Some(idx) = rest.find(") returns (") {
            request_type = rest[..idx].trim_start_matches("stream ").to_string();
            response_type = rest[idx + 11..]
                .trim_start_matches("stream ")
                .trim()
                .trim_end_matches(';')
                .trim_end_matches(')')
                .trim()
                .to_string();
            break;
        }
    }

    Some(ProtoRpc {
        name: name.to_string(),
        request_type,
        response_type,
    })
}

fn parse_enum_value(line: &str) -> Option<String> {
    let line = line.trim().trim_end_matches(';');
    let name = line.split('=').next()?.trim().to_string();
    if name.is_empty()
        || name == "}"
        || name.starts_with("//")
        || name.starts_with("reserved")
        || name.starts_with("option")
    {
        return None;
    }
    Some(name.to_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER_PROTO: &str = r#"
syntax = "proto3";

package example;

message User {
  int32 id = 1;
  string email = 2;
  string name = 3;
  int32 age = 4;
  repeated string tags = 5;
  bool active = 6;
  uint64 created_at = 7;
}

message CreateUserRequest {
  string email = 1;
  string name = 2;
  int32 age = 3;
}

message CreateUserResponse {
  int32 id = 1;
}

service UserService {
  rpc CreateUser (CreateUserRequest) returns (CreateUserResponse);
  rpc GetUser (GetUserRequest) returns (User);
}

message GetUserRequest {
  int32 id = 1;
}
"#;

    #[test]
    fn lifts_user_proto() {
        let tmp = std::env::temp_dir().join("user-test.proto");
        std::fs::write(&tmp, USER_PROTO).unwrap();

        let mut diag = Diagnostics::new();
        let decls = lift_proto(&tmp, &mut diag).unwrap();

        assert!(!decls.is_empty());
        let contract_count = decls
            .iter()
            .filter(|d| matches!(d, Declaration::Contract(_)))
            .count();
        assert!(contract_count >= 4);
    }

    #[test]
    fn parses_user_message_fields() {
        let mut parser = ProtoParser {
            messages: vec![],
            enums: HashMap::new(),
            rpcs: vec![],
        };
        parser.parse(USER_PROTO).unwrap();

        let user = parser.messages.iter().find(|m| m.name == "User").unwrap();
        assert_eq!(user.fields.len(), 7);
        assert_eq!(user.fields[0].name, "id");
        assert_eq!(user.fields[0].field_type, "int32");
        assert_eq!(user.fields[4].name, "tags");
        assert_eq!(user.fields[4].field_type, "string");
    }

    #[test]
    fn parses_rpc_definitions() {
        let mut parser = ProtoParser {
            messages: vec![],
            enums: HashMap::new(),
            rpcs: vec![],
        };
        parser.parse(USER_PROTO).unwrap();

        assert_eq!(parser.rpcs.len(), 2);
        assert!(parser.rpcs.iter().any(|r| r.name == "CreateUser"));
        assert!(parser.rpcs.iter().any(|r| r.name == "GetUser"));
    }
}
