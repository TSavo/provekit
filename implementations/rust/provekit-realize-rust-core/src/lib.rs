// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use provekit_canonicalizer::blake3_512_of;
use serde_json::Value;

const BODY_TEMPLATE_REL: &str =
    "menagerie/rust-language-signature/specs/body-templates/rust-canonical-bodies.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Realization {
    pub source: String,
    pub is_stub: bool,
    pub extension: String,
    pub emitted_artifact_cid: String,
}

#[derive(Debug, Clone)]
struct BodyTemplateEntry {
    concept_name: String,
    template_kind: String,
    template: String,
    min_params: Option<usize>,
    max_params: Option<usize>,
    requires_param_types: Option<Vec<String>>,
    requires_return_type: Option<String>,
}

pub fn emit_stub(
    function: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    concept_name: &str,
) -> Realization {
    let body = body_template_for(concept_name, params, param_types, return_type);
    let is_stub = body.is_none();
    let body = body.unwrap_or_else(|| stub_body_for(concept_name));
    let source = function_source(function, params, param_types, return_type, &body);
    let emitted_artifact_cid = blake3_512_of(source.as_bytes());
    Realization {
        source,
        is_stub,
        extension: "rs".to_string(),
        emitted_artifact_cid,
    }
}

pub fn dispatch(request: &Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    match method {
        "initialize" => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "name": "provekit-realize-rust",
                "version": "0.1.0",
                "protocol_version": "pep/1.7.0",
                "capabilities": {
                    "authoring_surfaces": ["rust"],
                    "emits_signed_mementos": false,
                    "ir_version": "v1.1.0"
                }
            }
        }),
        "provekit.plugin.invoke" => {
            let Some(params) = request.get("params").and_then(Value::as_object) else {
                return error(id, -32602, "INVALID_PARAMS: params must be an object");
            };
            let function = params.get("function").and_then(Value::as_str).unwrap_or("");
            let return_type = params
                .get("return_type")
                .and_then(Value::as_str)
                .unwrap_or("");
            let concept_name = params
                .get("concept_name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let param_names = string_array(params.get("params"));
            let param_types = string_array(params.get("param_types"));
            let realized = emit_stub(
                function,
                &param_names,
                &param_types,
                return_type,
                concept_name,
            );
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "source": realized.source,
                    "emitted_artifact_cid": realized.emitted_artifact_cid,
                    "is_stub": realized.is_stub,
                    "extension": realized.extension,
                    "observed_loss_record": {},
                    "used_sugars": []
                }
            })
        }
        "shutdown" | "provekit.plugin.shutdown" => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": null
        }),
        _ => error(id, -32601, &format!("METHOD_NOT_FOUND: {method}")),
    }
}

pub fn run_rpc() {
    use std::io::{self, BufRead, Write};

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let method = serde_json::from_str::<Value>(line)
            .ok()
            .and_then(|v| v.get("method").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_default();
        let response = match serde_json::from_str::<Value>(line) {
            Ok(request) => dispatch(&request),
            Err(err) => error(Value::Null, -32700, &format!("PARSE_ERROR: {err}")),
        };
        let _ = serde_json::to_writer(&mut stdout, &response);
        let _ = stdout.write_all(b"\n");
        let _ = stdout.flush();
        if method == "shutdown" || method == "provekit.plugin.shutdown" {
            break;
        }
    }
}

fn body_template_for(
    concept_name: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
) -> Option<String> {
    let mapped_param_types: Vec<String> =
        param_types.iter().map(|ty| map_source_type(ty)).collect();
    let mapped_return_type = map_source_type(return_type);
    for entry in entries() {
        if !concept_matches(&entry.concept_name, concept_name) {
            continue;
        }
        if entry.min_params.is_some_and(|min| params.len() < min) {
            continue;
        }
        if entry.max_params.is_some_and(|max| params.len() > max) {
            continue;
        }
        if entry.template_kind != "verbatim" {
            continue;
        }
        if let Some(required) = &entry.requires_return_type {
            if required != &mapped_return_type {
                continue;
            }
        }
        if let Some(required) = &entry.requires_param_types {
            if required != &mapped_param_types {
                continue;
            }
        }
        if let Some(rendered) = render_template(
            &entry.template,
            params,
            &mapped_param_types,
            &mapped_return_type,
        ) {
            return Some(rendered);
        }
    }
    None
}

fn render_template(
    template: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
) -> Option<String> {
    let mut rendered = template.to_string();
    for (index, name) in params.iter().enumerate() {
        rendered = rendered.replace(&format!("${{param{index}}}"), name);
    }
    for (index, ty) in param_types.iter().enumerate() {
        rendered = rendered.replace(&format!("${{param_type_{index}}}"), ty);
    }
    rendered = rendered.replace("${param_count}", &params.len().to_string());
    rendered = rendered.replace("${return_type}", return_type);
    if rendered.contains("${") {
        None
    } else {
        Some(rendered)
    }
}

fn function_source(
    function: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    body: &str,
) -> String {
    let typed_params = params
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let ty = param_types
                .get(index)
                .map(|s| map_source_type(s))
                .unwrap_or_else(|| "i64".to_string());
            format!("{name}: {ty}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    let mapped_return = map_source_type(return_type);
    let return_suffix = if mapped_return.is_empty() || mapped_return == "()" {
        String::new()
    } else {
        format!(" -> {mapped_return}")
    };
    let body_lines = body.lines().collect::<Vec<_>>();
    let indented = if body_lines.is_empty() {
        String::new()
    } else {
        body_lines
            .iter()
            .map(|line| {
                if line.is_empty() {
                    String::new()
                } else {
                    format!("    {line}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!("pub fn {function}({typed_params}){return_suffix} {{\n{indented}\n}}\n")
}

fn stub_body_for(concept_name: &str) -> String {
    let message = format!("provekit-bind canonical: {concept_name}");
    format!("panic!({})", rust_string_literal(&message))
}

fn rust_string_literal(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => out.push_str(&format!("\\u{{{:x}}}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn map_source_type(src: &str) -> String {
    match src.trim() {
        "" => "()".to_string(),
        "void" | "None" | "()" => "()".to_string(),
        "long" | "int" | "i64" | "u64" => "i64".to_string(),
        "i32" | "u32" => "i32".to_string(),
        "short" | "i16" | "u16" => "i16".to_string(),
        "byte" | "char" | "i8" | "u8" => "i8".to_string(),
        "boolean" | "bool" => "bool".to_string(),
        "double" | "float" | "f64" | "f32" => "f64".to_string(),
        "String" | "str" | "&str" | "&String" => "String".to_string(),
        "list" | "List" | "list[int]" | "list[i64]" => "&[i64]".to_string(),
        other => other.to_string(),
    }
}

fn concept_matches(entry_name: &str, request_name: &str) -> bool {
    entry_name == request_name
        || entry_name
            .strip_prefix("concept:")
            .is_some_and(|name| name == request_name)
        || request_name
            .strip_prefix("concept:")
            .is_some_and(|name| name == entry_name)
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn error(id: Value, code: i64, message: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

fn entries() -> &'static [BodyTemplateEntry] {
    static ENTRIES: OnceLock<Vec<BodyTemplateEntry>> = OnceLock::new();
    ENTRIES
        .get_or_init(|| {
            find_repo_file(Path::new(BODY_TEMPLATE_REL))
                .and_then(|path| std::fs::read_to_string(path).ok())
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                .map(|root| parse_entries(&root))
                .unwrap_or_default()
        })
        .as_slice()
}

fn parse_entries(root: &Value) -> Vec<BodyTemplateEntry> {
    root.pointer("/header/content/entries")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(parse_entry)
                .collect::<Vec<BodyTemplateEntry>>()
        })
        .unwrap_or_default()
}

fn parse_entry(item: &Value) -> Option<BodyTemplateEntry> {
    let template = item.get("emission_template")?;
    let guard = item.get("signature_guard");
    let requires_param_types = guard
        .and_then(|g| g.get("requires_param_types"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        });
    Some(BodyTemplateEntry {
        concept_name: item.get("concept_name")?.as_str()?.to_string(),
        template_kind: template.get("kind")?.as_str()?.to_string(),
        template: template.get("template")?.as_str()?.to_string(),
        min_params: guard
            .and_then(|g| g.get("min_params"))
            .and_then(Value::as_u64)
            .map(|n| n as usize),
        max_params: guard
            .and_then(|g| g.get("max_params"))
            .and_then(Value::as_u64)
            .map(|n| n as usize),
        requires_param_types,
        requires_return_type: guard
            .and_then(|g| g.get("requires_return_type"))
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn find_repo_file(relative: &Path) -> Option<PathBuf> {
    let mut bases = Vec::new();
    if let Some(root) = std::env::var_os("PROVEKIT_REPO_ROOT") {
        bases.push(PathBuf::from(root));
    }
    if let Ok(cwd) = std::env::current_dir() {
        bases.extend(cwd.ancestors().map(Path::to_path_buf));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            bases.extend(parent.ancestors().map(Path::to_path_buf));
        }
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    bases.extend(manifest_dir.ancestors().map(Path::to_path_buf));

    for base in bases {
        let candidate = base.join(relative);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn renders_identity_from_body_template() {
        let rendered = emit_stub(
            "wrap_identity",
            &strings(&["x"]),
            &strings(&["i64"]),
            "i64",
            "identity",
        );

        assert!(!rendered.is_stub);
        assert_eq!(rendered.extension, "rs");
        assert_eq!(
            rendered.source,
            "pub fn wrap_identity(x: i64) -> i64 {\n    x\n}\n"
        );
        assert!(rendered.emitted_artifact_cid.starts_with("blake3-512:"));
    }

    #[test]
    fn renders_unit_without_return_arrow() {
        let rendered = emit_stub("do_nothing", &[], &[], "()", "unit");

        assert!(!rendered.is_stub);
        assert_eq!(rendered.source, "pub fn do_nothing() {\n    ()\n}\n");
    }

    #[test]
    fn falls_back_to_deterministic_stub_for_missing_template() {
        let rendered = emit_stub(
            "unknown_cell",
            &strings(&["x"]),
            &strings(&["i64"]),
            "i64",
            "missing-cell",
        );

        assert!(rendered.is_stub);
        assert_eq!(
            rendered.source,
            "pub fn unknown_cell(x: i64) -> i64 {\n    panic!(\"provekit-bind canonical: missing-cell\")\n}\n"
        );
    }

    #[test]
    fn dispatch_invoke_returns_rpc_result_shape() {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "toggle",
                "params": ["flag"],
                "param_types": ["bool"],
                "return_type": "bool",
                "concept_name": "bool-cell"
            }
        });

        let response = dispatch(&request);
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 7);
        assert_eq!(response["result"]["extension"], "rs");
        assert_eq!(response["result"]["is_stub"], false);
        assert_eq!(
            response["result"]["source"],
            "pub fn toggle(flag: bool) -> bool {\n    !flag\n}\n"
        );
    }
}
