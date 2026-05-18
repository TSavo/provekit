// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use provekit_ir_types::realization_tags::tag_sugar_carrier;
use serde_json::{json, Value};

pub mod platform_semantics;

const BODY_TEMPLATE_REL: &str =
    "menagerie/rust-language-signature/specs/body-templates/rust-canonical-bodies.json";
const RETURN_OP_CID: &str = "blake3-512:776d417c66325df1d40e3e0fd7331195e2b1d14f9c30b5984030f21aa8b6b38b3eb81ee3dddd46716003275c9960022e2273dd8efb0110bacc5719811ee18dc6";
const CALL_NEW_OP_CID: &str = "blake3-512:e6576534d74eee6b309fa55457620d4903472dcd331f0cb9c2be2a95994655ad64ef1fa56f778534f6ba5c04c055069bb109de3dae4dc45bde7dd689671b24b8";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Realization {
    pub source: String,
    pub is_stub: bool,
    pub extension: String,
    pub emitted_artifact_cid: String,
    pub observed_loss_record: Value,
    pub used_sugars: Vec<Value>,
}

#[derive(Debug, Clone)]
struct BodyTemplateEntry {
    concept_name: String,
    mode: Option<String>,
    realization_kind: Option<String>,
    template_kind: String,
    template: String,
    loss_record_contribution: Option<Value>,
    min_params: Option<usize>,
    max_params: Option<usize>,
    requires_param_types: Option<Vec<String>>,
    requires_return_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderedBody {
    body: String,
    observed_loss_record: Value,
}

pub fn emit_stub(
    function: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    concept_name: &str,
) -> Realization {
    emit_stub_with_mode(
        function,
        params,
        param_types,
        return_type,
        concept_name,
        None,
    )
}

pub fn emit_stub_with_mode(
    function: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    concept_name: &str,
    mode: Option<&str>,
) -> Realization {
    let rendered = operator_body_template_for(concept_name, params, param_types, return_type, mode)
        .or_else(|| body_template_for(concept_name, params, param_types, return_type, mode));
    if rendered.is_none() {
        if let Some(realized) =
            emit_sugar_carrier(function, params, param_types, return_type, concept_name)
        {
            return realized;
        }
    }
    match rendered {
        Some(rendered) => emit_function_with_evidence(
            function,
            params,
            param_types,
            return_type,
            &rendered.body,
            rendered.observed_loss_record,
            Vec::new(),
        ),
        None => {
            let body = stub_body_for(concept_name);
            emit_function(function, params, param_types, return_type, &body, true)
        }
    }
}

/// Emits Rust for the D7 resolved Value::null body shape:
/// `return(call:new(literal("new" | "*::new"), literal(["Null"])))`.
///
/// The trailing `return` node is lowered as a Rust tail expression. The nested
/// `call:new` shape lowers to `<callee>(Value::Null)`. D7-v2 fixtures used a
/// bare `new`; D7-v3 fixtures carry the receiver-prefixed `Arc::new` spelling.
/// Unsupported resolved concepts or literal shapes fall back to the existing
/// `panic!("provekit-bind canonical: <concept>")` stub body.
pub fn emit_from_resolved(
    resolved_term_json: &str,
    function_name: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
) -> Realization {
    let resolved = match serde_json::from_str::<Value>(resolved_term_json) {
        Ok(resolved) => resolved,
        Err(_) => {
            return emit_stub(
                function_name,
                params,
                param_types,
                return_type,
                "malformed-resolved-term",
            );
        }
    };

    match lower_resolved_body(&resolved, params) {
        Ok(body) => emit_function(
            function_name,
            params,
            param_types,
            return_type,
            &body,
            false,
        ),
        Err(concept_name) => emit_stub(
            function_name,
            params,
            param_types,
            return_type,
            &concept_name,
        ),
    }
}

fn emit_function(
    function: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    body: &str,
    is_stub: bool,
) -> Realization {
    let source = function_source(function, params, param_types, return_type, body);
    let emitted_artifact_cid = blake3_512_of(source.as_bytes());
    Realization {
        source,
        is_stub,
        extension: "rs".to_string(),
        emitted_artifact_cid,
        observed_loss_record: json!({}),
        used_sugars: Vec::new(),
    }
}

fn emit_function_with_evidence(
    function: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    body: &str,
    observed_loss_record: Value,
    used_sugars: Vec<Value>,
) -> Realization {
    let source = function_source(function, params, param_types, return_type, body);
    let emitted_artifact_cid = blake3_512_of(source.as_bytes());
    Realization {
        source,
        is_stub: false,
        extension: "rs".to_string(),
        emitted_artifact_cid,
        observed_loss_record,
        used_sugars,
    }
}

#[derive(Debug, Clone, Copy)]
struct SugarCarrierSpec {
    concept_name: &'static str,
    concept_cid: &'static str,
    operation_kind: &'static str,
    loss_name: &'static str,
}

const SUGAR_CARRIER_SPECS: &[SugarCarrierSpec] = &[
    SugarCarrierSpec {
        concept_name: "concept:postdec",
        concept_cid: "blake3-512:cac33b2bef01e38d327440e7bfecebf3e7540d463a02e68dd047e47d0c9cca45f94181ce773fb389671a960cc957760b540b2927afd6d2c624cf9ddaca225f1a",
        operation_kind: "postdec",
        loss_name: "rust-no-postfix-decrement",
    },
    SugarCarrierSpec {
        concept_name: "concept:postinc",
        concept_cid: "blake3-512:be615743882f980a2fde0ca6ec3250305c28e2fac1fe4d17accd1790d62af7992ff80282f6507335b959ccceaa32a047f1845b8a9e96a54d20b3766d46589aee",
        operation_kind: "postinc",
        loss_name: "rust-no-postfix-increment",
    },
    SugarCarrierSpec {
        concept_name: "concept:predec",
        concept_cid: "blake3-512:fa83fc84643e03f1e60aa66848412e0cdc25ad6ede0cf216643fb8d4dbe52c4d8df28283f754040cc0f53a62ec22e73a2db623e6507055ab1076df8394024995",
        operation_kind: "predec",
        loss_name: "rust-no-prefix-decrement",
    },
    SugarCarrierSpec {
        concept_name: "concept:preinc",
        concept_cid: "blake3-512:8c8383c221eaca3b95d30437d768065d5117091415afb04e92f541af6fb26d37af79d423e25a59ffaf3f6e2d654d0bd64cfe8e071ee5483ed6bca2614442001f",
        operation_kind: "preinc",
        loss_name: "rust-no-prefix-increment",
    },
    SugarCarrierSpec {
        concept_name: "concept:throw",
        concept_cid: "blake3-512:bfca9b128ea5128d15236ebbe44150ff60355b9bbcd664ae4abbc34f2e4e658f7441089449956bfdb333d1f2eb1bff828c74a5d2f3df7fec723abe883bb81a12",
        operation_kind: "throw",
        loss_name: "rust-result-not-throw",
    },
    SugarCarrierSpec {
        concept_name: "concept:new",
        concept_cid: "blake3-512:26eb0a9484d68fff3fafe1ee82f09e3c3f49e1e2d1e8d01c733362b39473590e61f5903080ffdf69f2532e57047d0fbd4439a11ff778936e27a61f0c4c8c35b8",
        operation_kind: "new",
        loss_name: "rust-no-new-keyword",
    },
    SugarCarrierSpec {
        concept_name: "concept:ushr",
        concept_cid: "blake3-512:5746cb4f8bb8d713624731661de51e851e7ca65dae10a88bae4727d1e0070525be77e9919d90939264acaf4c093b00808862e6d0d2c24ac05262ce95cd67c8ad",
        operation_kind: "ushr",
        loss_name: "rust-unified-shr",
    },
    SugarCarrierSpec {
        concept_name: "concept:source-unit",
        concept_cid: "blake3-512:377bec17d4c9ea2e44216e244685c282b3ac83c19191699eab94a47ff0b123bf4899d6b5691ce88fa7bc70d4dd9f8d2566631bd02895f891ec67ca6d32a87285",
        operation_kind: "source-unit",
        loss_name: "rust-crate-source-unit",
    },
];

fn emit_sugar_carrier(
    function: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    concept_name: &str,
) -> Option<Realization> {
    let spec = sugar_carrier_spec(concept_name)?;
    let loss_record = loss_record_for(spec.loss_name);
    let loss_record_cid = cid_for_json(&loss_record);
    let sugar_dict = sugar_dict_for(spec, &loss_record);
    let sugar_dict_cid = cid_for_json(&sugar_dict);
    let used_sugar = used_sugar_for(&sugar_dict, &sugar_dict_cid);
    let args_jcs = json!([]);
    let payload = json!({
        "args_jcs": args_jcs,
        "args_jcs_cid": cid_for_json(&args_jcs),
        "artifact_kind": "provekit-concept-citation-comment-sugar",
        "concept_cid": spec.concept_cid,
        "concept_name": spec.concept_name,
        "concept_site_cid": spec.concept_cid,
        "emitted_by": {
            "kit_cid": sugar_dict_cid,
            "kit_id": format!("provekit-realize-rust-core@{}", env!("CARGO_PKG_VERSION")),
            "kit_kind": "realize",
            "target_language": "rust"
        },
        "loss_record_cid": loss_record_cid,
        "operation_kind": spec.operation_kind,
        "schema_version": "1",
        "shape_cid": spec.concept_cid,
        "sugar_dict_cid": sugar_dict_cid,
        "term_position": [0]
    });
    let payload_jcs = jcs_for_json(&payload);
    let payload_cid = blake3_512_of(payload_jcs.as_bytes());
    let message = format!("provekit concept carrier: {}", spec.concept_name);
    let body = format!(
        "// provekit-concept: {payload_jcs}\n// provekit-concept-payload-cid: {payload_cid}\npanic!({})",
        rust_string_literal(&message)
    );
    Some(emit_function_with_evidence(
        function,
        params,
        param_types,
        return_type,
        &body,
        loss_record,
        vec![used_sugar],
    ))
}

fn sugar_carrier_spec(concept_name: &str) -> Option<SugarCarrierSpec> {
    let normalized = normalize_concept_name(concept_name);
    SUGAR_CARRIER_SPECS
        .iter()
        .copied()
        .find(|spec| spec.concept_name == normalized)
}

fn normalize_concept_name(concept_name: &str) -> String {
    if concept_name.starts_with("concept:") {
        concept_name.to_string()
    } else {
        format!("concept:{concept_name}")
    }
}

fn loss_record_for(loss_name: &str) -> Value {
    let mut value = serde_json::Map::new();
    value.insert(
        loss_name.to_string(),
        json!({
            "args": [],
            "head": "atomic",
            "name": loss_name
        }),
    );
    json!({
        "loss_record_contribution": {
            "form": "literal",
            "value": Value::Object(value)
        }
    })
}

fn sugar_dict_for(spec: SugarCarrierSpec, loss_record: &Value) -> Value {
    let realization = serde_json::to_value(tag_sugar_carrier(spec.concept_name))
        .unwrap_or_else(|_| json!({"kind": "sugar-carrier"}));
    json!({
        "concept_name": spec.concept_name,
        "kind": "concept-citation-sugar-carrier",
        "loss_record_contribution": loss_record.get("loss_record_contribution").cloned().unwrap_or_else(|| json!({})),
        "realization": realization,
        "target_language": "rust"
    })
}

fn used_sugar_for(sugar_dict: &Value, sugar_dict_cid: &str) -> Value {
    let mut value = sugar_dict.clone();
    if let Value::Object(map) = &mut value {
        map.insert(
            "header".to_string(),
            json!({
                "cid": sugar_dict_cid,
                "kind": "sugar"
            }),
        );
    }
    value
}

fn cid_for_json(value: &Value) -> String {
    let jcs = jcs_for_json(value);
    blake3_512_of(jcs.as_bytes())
}

fn jcs_for_json(value: &Value) -> String {
    let canonical = cvalue_from_json(value);
    encode_jcs(&canonical)
}

fn cvalue_from_json(value: &Value) -> CValue {
    match value {
        Value::Null => CValue::Null,
        Value::Bool(value) => CValue::Bool(*value),
        Value::Number(value) => CValue::Integer(value.as_i64().unwrap_or(0)),
        Value::String(value) => CValue::String(value.clone()),
        Value::Array(items) => CValue::Array(
            items
                .iter()
                .map(|item| std::sync::Arc::new(cvalue_from_json(item)))
                .collect(),
        ),
        Value::Object(map) => CValue::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), std::sync::Arc::new(cvalue_from_json(value))))
                .collect(),
        ),
    }
}

fn lower_resolved_body(term: &Value, params: &[String]) -> Result<String, String> {
    let concept_name = resolved_concept_name(term);
    if concept_name != "return" {
        return Err(concept_name);
    }

    let args = resolved_args(term).ok_or(concept_name)?;
    if args.len() != 1 {
        return Err("return".to_string());
    }
    lower_resolved_expr(&args[0], params)
}

fn lower_resolved_expr(term: &Value, params: &[String]) -> Result<String, String> {
    match resolved_concept_name(term).as_str() {
        "call:new" => lower_call_new(term, params),
        "literal" => lower_literal(term),
        concept_name => Err(concept_name.to_string()),
    }
}

fn lower_call_new(term: &Value, params: &[String]) -> Result<String, String> {
    let args = resolved_args(term).ok_or_else(|| "call:new".to_string())?;
    if args.len() != 2 {
        return Err("call:new".to_string());
    }

    let name = lower_literal(&args[0])?;
    let lowered_args = lower_call_new_args_literal(&args[1], params)?;
    if name != "new" && !name.ends_with("::new") {
        return Err("call:new".to_string());
    }

    Ok(format!("{name}({lowered_args})"))
}

fn lower_call_new_args_literal(term: &Value, params: &[String]) -> Result<String, String> {
    let node = resolved_node(term).ok_or_else(|| "literal".to_string())?;
    if node.get("kind").and_then(Value::as_str) != Some("literal") {
        return Err(resolved_concept_name(term));
    }

    match node.get("value") {
        Some(Value::Array(items)) if is_value_null_literal(items) => Ok("Value::Null".to_string()),
        Some(Value::Array(items)) => lower_single_value_variant_application(items, params)
            .ok_or_else(|| "literal".to_string()),
        _ => Err("literal".to_string()),
    }
}

fn lower_single_value_variant_application(items: &[Value], params: &[String]) -> Option<String> {
    if items.len() != 1 {
        return None;
    }

    let surface = items[0].as_str()?;
    let call = surface.strip_prefix("call:")?;
    let (ctor_name, rest) = call.split_once('(')?;
    let inner = rest.strip_suffix(')')?;
    let (variant_path, arg_list) = inner.split_once(", [")?;
    let arg = arg_list.strip_suffix(']')?;
    let first_param = params.first()?;

    let variant_name = variant_path.rsplit("::").next()?;
    if ctor_name != variant_name {
        return None;
    }

    let lowered_arg = lower_value_variant_arg(arg, first_param)?;
    match variant_path {
        "Value::Bool" | "Value::Integer" | "Value::String" => {
            Some(format!("{variant_path}({lowered_arg})"))
        }
        _ => None,
    }
}

fn lower_value_variant_arg(arg: &str, first_param: &str) -> Option<String> {
    if arg == first_param {
        return Some(arg.to_string());
    }

    let method_call = arg.strip_prefix("method:")?;
    let (method_name, rest) = method_call.split_once('(')?;
    if method_name.is_empty() {
        return None;
    }
    let inner = rest.strip_suffix(')')?;
    let (receiver, method_args) = inner.split_once(", [")?;
    if receiver != first_param {
        return None;
    }
    if !method_args.strip_suffix(']')?.is_empty() {
        return None;
    }

    Some(format!("{first_param}.{method_name}()"))
}

fn lower_literal(term: &Value) -> Result<String, String> {
    let node = resolved_node(term).ok_or_else(|| "literal".to_string())?;
    if node.get("kind").and_then(Value::as_str) != Some("literal") {
        return Err(resolved_concept_name(term));
    }

    match node.get("value") {
        Some(Value::String(value)) if value == "new" || value.ends_with("::new") => {
            Ok(value.to_string())
        }
        Some(Value::Array(items)) if is_value_null_literal(items) => Ok("Value::Null".to_string()),
        _ => Err("literal".to_string()),
    }
}

fn is_value_null_literal(items: &[Value]) -> bool {
    items.len() == 1 && items[0].as_str() == Some("Null")
}

fn resolved_args(term: &Value) -> Option<&[Value]> {
    resolved_node(term)?
        .get("args")?
        .as_array()
        .map(Vec::as_slice)
}

fn resolved_node(term: &Value) -> Option<&serde_json::Map<String, Value>> {
    term.get("node")?.as_object()
}

fn resolved_concept_name(term: &Value) -> String {
    let Some(node) = resolved_node(term) else {
        return "malformed-resolved-term".to_string();
    };
    match node.get("kind").and_then(Value::as_str) {
        Some("literal") => "literal".to_string(),
        Some("concept:op-application") => node
            .get("op_definition_cid")
            .and_then(Value::as_str)
            .map(op_concept_name)
            .unwrap_or_else(|| "malformed-resolved-term".to_string()),
        _ => "malformed-resolved-term".to_string(),
    }
}

fn op_concept_name(op_definition_cid: &str) -> String {
    match op_definition_cid {
        RETURN_OP_CID => "return".to_string(),
        CALL_NEW_OP_CID => "call:new".to_string(),
        other => other.to_string(),
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
            let mode = params.get("mode").and_then(Value::as_str);
            let param_names = string_array(params.get("params"));
            let param_types = string_array(params.get("param_types"));
            let realized = emit_stub_with_mode(
                function,
                &param_names,
                &param_types,
                return_type,
                concept_name,
                mode,
            );
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "source": realized.source,
                    "emitted_artifact_cid": realized.emitted_artifact_cid,
                    "is_stub": realized.is_stub,
                    "extension": realized.extension,
                    "observed_loss_record": realized.observed_loss_record,
                    "used_sugars": realized.used_sugars
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
    mode: Option<&str>,
) -> Option<RenderedBody> {
    body_template_for_entries(
        entries(),
        concept_name,
        params,
        param_types,
        return_type,
        mode,
    )
}

fn body_template_for_entries(
    entries: &[BodyTemplateEntry],
    concept_name: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    mode: Option<&str>,
) -> Option<RenderedBody> {
    let mapped_param_types: Vec<String> =
        param_types.iter().map(|ty| map_source_type(ty)).collect();
    let mapped_return_type = map_source_type(return_type);
    for entry in entries {
        if !concept_matches(&entry.concept_name, concept_name) {
            continue;
        }
        if !mode_matches(entry.mode.as_deref(), mode) {
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
            return Some(RenderedBody {
                body: rendered,
                observed_loss_record: observed_loss_record_for_entry(entry),
            });
        }
    }
    None
}

fn observed_loss_record_for_entry(entry: &BodyTemplateEntry) -> Value {
    if entry.realization_kind.as_deref() != Some("boundary-realization") {
        return json!({});
    }
    let Some(contribution) = entry.loss_record_contribution.clone() else {
        return json!({});
    };
    json!({
        "loss_record_contribution": contribution
    })
}

fn operator_body_template_for(
    concept_name: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    mode: Option<&str>,
) -> Option<RenderedBody> {
    let root = operator_root()?;
    let (language, library_tag) = operator_binding_surface(&root, concept_name)?;
    if language != "rust" {
        return None;
    }
    let template = load_library_body_template(&language, &library_tag)?;
    body_template_for_entries(
        &template,
        concept_name,
        params,
        param_types,
        return_type,
        mode,
    )
}

fn operator_root() -> Option<PathBuf> {
    if let Some(root) = std::env::var_os("PROVEKIT_OPERATOR_ROOT") {
        let root = PathBuf::from(root);
        if root
            .join(".provekit")
            .join("library-bindings.json")
            .is_file()
        {
            return Some(root);
        }
    }
    std::env::current_dir().ok().and_then(|cwd| {
        cwd.ancestors()
            .find(|base| {
                base.join(".provekit")
                    .join("library-bindings.json")
                    .is_file()
            })
            .map(Path::to_path_buf)
    })
}

fn operator_binding_surface(root: &Path, concept_name: &str) -> Option<(String, String)> {
    let path = root.join(".provekit").join("library-bindings.json");
    let raw = std::fs::read_to_string(path).ok()?;
    let doc: Value = serde_json::from_str(&raw).ok()?;
    let config_language = doc.get("language")?.as_str()?;
    let bindings = doc.get("bindings")?.as_object()?;
    let surface = bindings
        .iter()
        .find(|(candidate, _)| concept_matches(candidate, concept_name))?
        .1
        .as_str()?;
    let (language, tag) = split_library_surface(surface)?;
    if language != config_language {
        return None;
    }
    Some((language, tag))
}

fn split_library_surface(surface: &str) -> Option<(String, String)> {
    let (language, tag) = surface.split_once('-')?;
    if language.is_empty() || tag.is_empty() {
        return None;
    }
    Some((language.to_string(), tag.to_string()))
}

fn load_library_body_template(language: &str, library_tag: &str) -> Option<Vec<BodyTemplateEntry>> {
    let rel = PathBuf::from("menagerie")
        .join(format!("{language}-language-signature"))
        .join("specs")
        .join("body-templates")
        .join(format!("{language}-canonical-bodies-{library_tag}.json"));
    find_repo_file(&rel)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .map(|root| parse_entries(&root))
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

fn mode_matches(entry_mode: Option<&str>, request_mode: Option<&str>) -> bool {
    match entry_mode {
        Some(entry) => request_mode.is_some_and(|request| request == entry),
        None => true,
    }
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
        mode: item.get("mode").and_then(Value::as_str).map(str::to_string),
        realization_kind: item
            .get("realization_kind")
            .and_then(Value::as_str)
            .map(str::to_string),
        template_kind: template.get("kind")?.as_str()?.to_string(),
        template: template.get("template")?.as_str()?.to_string(),
        loss_record_contribution: item.get("loss_record_contribution").cloned(),
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
    use std::path::PathBuf;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    fn resolved_return_call_new_with_literal_args(args: Vec<Value>) -> Value {
        serde_json::json!({
            "node": {
                "args": [
                    {
                        "node": {
                            "args": [
                                {
                                    "node": {
                                        "kind": "literal",
                                        "value": "Arc::new",
                                    },
                                    "sort": {"args": [], "kind": "ctor", "name": "FnContract"},
                                },
                                {
                                    "node": {
                                        "kind": "literal",
                                        "value": args,
                                    },
                                    "sort": {"args": [], "kind": "ctor", "name": "ListOfExpr"},
                                },
                            ],
                            "kind": "concept:op-application",
                            "op_definition_cid": CALL_NEW_OP_CID,
                        },
                        "sort": {"args": [], "kind": "ctor", "name": "Expr"},
                    },
                ],
                "kind": "concept:op-application",
                "op_definition_cid": RETURN_OP_CID,
            },
            "sort": {"args": [], "kind": "ctor", "name": "Stmt"},
        })
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
    fn renders_contract_observation_witness_body_template() {
        let rendered = emit_stub_with_mode(
            "observe_contract",
            &strings(&["callsite_cid", "contract_cid", "mode"]),
            &strings(&["String", "String", "String"]),
            "ContractObservationResult",
            "concept:contract-observation",
            Some("witness"),
        );

        assert!(!rendered.is_stub);
        assert!(rendered.source.contains("provekit_witness::observe"));
        assert!(rendered.source.contains("callsite_cid"));
        assert!(rendered.source.contains("contract_cid"));
        assert!(rendered.source.contains("mode"));
    }

    #[test]
    fn refuses_contract_observation_witness_template_for_gate_mode() {
        let rendered = emit_stub_with_mode(
            "observe_contract",
            &strings(&["callsite_cid", "contract_cid", "mode"]),
            &strings(&["String", "String", "String"]),
            "ContractObservationResult",
            "concept:contract-observation",
            Some("gate"),
        );

        assert!(rendered.is_stub);
        assert!(!rendered.source.contains("provekit_witness::observe"));
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
    fn emits_value_null_from_resolved_return_call_new_literal_shape() {
        let resolved = serde_json::json!({
            "node": {
                "args": [
                    {
                        "node": {
                            "args": [
                                {
                                    "node": {
                                        "kind": "literal",
                                        "value": "new",
                                    },
                                    "sort": {"args": [], "kind": "ctor", "name": "FnContract"},
                                },
                                {
                                    "node": {
                                        "kind": "literal",
                                        "value": ["Null"],
                                    },
                                    "sort": {"args": [], "kind": "ctor", "name": "ListOfExpr"},
                                },
                            ],
                            "kind": "concept:op-application",
                            "op_definition_cid": CALL_NEW_OP_CID,
                        },
                        "sort": {"args": [], "kind": "ctor", "name": "Expr"},
                    },
                ],
                "kind": "concept:op-application",
                "op_definition_cid": RETURN_OP_CID,
            },
            "sort": {"args": [], "kind": "ctor", "name": "Stmt"},
        });
        let rendered = emit_from_resolved(
            &serde_json::to_string(&resolved).expect("resolved json"),
            "null",
            &[],
            &[],
            "Arc < Value >",
        );

        assert!(!rendered.is_stub);
        assert_eq!(
            rendered.source,
            "pub fn null() -> Arc < Value > {\n    new(Value::Null)\n}\n"
        );
    }

    #[test]
    fn emits_value_null_from_resolved_call_new_with_receiver_prefix() {
        let resolved = serde_json::json!({
            "node": {
                "args": [
                    {
                        "node": {
                            "args": [
                                {
                                    "node": {
                                        "kind": "literal",
                                        "value": "Arc::new",
                                    },
                                    "sort": {"args": [], "kind": "ctor", "name": "FnContract"},
                                },
                                {
                                    "node": {
                                        "kind": "literal",
                                        "value": ["Null"],
                                    },
                                    "sort": {"args": [], "kind": "ctor", "name": "ListOfExpr"},
                                },
                            ],
                            "kind": "concept:op-application",
                            "op_definition_cid": CALL_NEW_OP_CID,
                        },
                        "sort": {"args": [], "kind": "ctor", "name": "Expr"},
                    },
                ],
                "kind": "concept:op-application",
                "op_definition_cid": RETURN_OP_CID,
            },
            "sort": {"args": [], "kind": "ctor", "name": "Stmt"},
        });
        let rendered = emit_from_resolved(
            &serde_json::to_string(&resolved).expect("resolved json"),
            "null",
            &[],
            &[],
            "Arc < Value >",
        );

        assert!(!rendered.is_stub);
        assert_eq!(
            rendered.source,
            "pub fn null() -> Arc < Value > {\n    Arc::new(Value::Null)\n}\n"
        );
    }

    #[test]
    fn emits_value_bool_from_resolved_call_new_variant_arg_shape() {
        let resolved = resolved_return_call_new_with_literal_args(vec![Value::String(
            "call:Bool(Value::Bool, [b])".to_string(),
        )]);
        let rendered = emit_from_resolved(
            &serde_json::to_string(&resolved).expect("resolved json"),
            "boolean",
            &strings(&["b"]),
            &strings(&["bool"]),
            "Arc < Value >",
        );

        assert!(!rendered.is_stub);
        assert_eq!(
            rendered.source,
            "pub fn boolean(b: bool) -> Arc < Value > {\n    Arc::new(Value::Bool(b))\n}\n"
        );
    }

    #[test]
    fn emits_value_integer_from_resolved_call_new_variant_arg_shape() {
        let resolved = resolved_return_call_new_with_literal_args(vec![Value::String(
            "call:Integer(Value::Integer, [n])".to_string(),
        )]);
        let rendered = emit_from_resolved(
            &serde_json::to_string(&resolved).expect("resolved json"),
            "integer",
            &strings(&["n"]),
            &strings(&["i64"]),
            "Arc < Value >",
        );

        assert!(!rendered.is_stub);
        assert_eq!(
            rendered.source,
            "pub fn integer(n: i64) -> Arc < Value > {\n    Arc::new(Value::Integer(n))\n}\n"
        );
    }

    #[test]
    fn emits_value_string_from_resolved_call_new_variant_arg_shape() {
        let resolved = resolved_return_call_new_with_literal_args(vec![Value::String(
            "call:String(Value::String, [s])".to_string(),
        )]);
        let rendered = emit_from_resolved(
            &serde_json::to_string(&resolved).expect("resolved json"),
            "string",
            &strings(&["s"]),
            &strings(&["String"]),
            "Arc < Value >",
        );

        assert!(!rendered.is_stub);
        assert_eq!(
            rendered.source,
            "pub fn string(s: String) -> Arc < Value > {\n    Arc::new(Value::String(s))\n}\n"
        );
    }

    #[test]
    fn emits_value_string_from_resolved_call_new_variant_method_arg_shape() {
        let resolved = resolved_return_call_new_with_literal_args(vec![Value::String(
            "call:String(Value::String, [method:into(s, [])])".to_string(),
        )]);
        let rendered = emit_from_resolved(
            &serde_json::to_string(&resolved).expect("resolved json"),
            "string<S: Into<String>>",
            &strings(&["s"]),
            &strings(&["S"]),
            "Arc < Value >",
        );

        assert!(!rendered.is_stub);
        assert_eq!(
            rendered.source,
            "pub fn string<S: Into<String>>(s: S) -> Arc < Value > {\n    Arc::new(Value::String(s.into()))\n}\n"
        );
    }

    #[test]
    fn unsupported_resolved_shape_falls_back_to_stub() {
        let resolved = serde_json::json!({
            "node": {
                "args": [],
                "kind": "concept:op-application",
                "op_definition_cid": "unsupported-concept",
            },
            "sort": {"args": [], "kind": "ctor", "name": "Stmt"},
        });
        let rendered = emit_from_resolved(
            &serde_json::to_string(&resolved).expect("resolved json"),
            "unknown_cell",
            &strings(&["x"]),
            &strings(&["i64"]),
            "i64",
        );

        assert!(rendered.is_stub);
        assert_eq!(
            rendered.source,
            "pub fn unknown_cell(x: i64) -> i64 {\n    panic!(\"provekit-bind canonical: unsupported-concept\")\n}\n"
        );
    }

    #[test]
    fn emit_from_resolved_uses_operator_library_binding_before_stub_fallback() {
        let _guard = env_lock().lock().expect("env lock");
        let root = temp_operator_root("realize_library_binding");
        std::fs::create_dir_all(root.join(".provekit")).expect("create .provekit");
        std::fs::write(
            root.join(".provekit").join("library-bindings.json"),
            r#"{
  "language": "rust",
  "bindings": {
    "concept:custom-echo": "rust-demo"
  }
}
"#,
        )
        .expect("write library bindings");
        let template_dir = root
            .join("menagerie")
            .join("rust-language-signature")
            .join("specs")
            .join("body-templates");
        std::fs::create_dir_all(&template_dir).expect("create body template dir");
        std::fs::write(
            template_dir.join("rust-canonical-bodies-demo.json"),
            r#"{
  "header": {
    "content": {
      "entries": [
        {
          "concept_name": "concept:custom-echo",
          "emission_template": {
            "kind": "verbatim",
            "template": "${param0}.clone()"
          },
          "signature_guard": {
            "min_params": 1,
            "max_params": 1,
            "requires_return_type": "String"
          }
        }
      ]
    }
  }
}
"#,
        )
        .expect("write body template");
        std::env::set_var("PROVEKIT_OPERATOR_ROOT", &root);
        std::env::set_var("PROVEKIT_REPO_ROOT", &root);

        let resolved = serde_json::json!({
            "node": {
                "args": [],
                "kind": "concept:op-application",
                "op_definition_cid": "concept:custom-echo",
            },
            "sort": {"args": [], "kind": "ctor", "name": "Stmt"},
        });
        let rendered = emit_from_resolved(
            &serde_json::to_string(&resolved).expect("resolved json"),
            "echo",
            &strings(&["value"]),
            &strings(&["String"]),
            "String",
        );

        std::env::remove_var("PROVEKIT_OPERATOR_ROOT");
        std::env::remove_var("PROVEKIT_REPO_ROOT");
        let _ = std::fs::remove_dir_all(root);

        assert!(!rendered.is_stub);
        assert_eq!(
            rendered.source,
            "pub fn echo(value: String) -> String {\n    value.clone()\n}\n"
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

    #[test]
    fn renders_dynamic_dispatch_boundary_realization() {
        let rendered = dynamic_dispatch_render();

        assert!(!rendered.is_stub);
        assert!(rendered.source.contains("&dyn Fn(i64) -> i64"));
        assert!(rendered.source.contains("dispatched(arg)"));
    }

    #[test]
    fn records_dynamic_dispatch_boundary_loss_without_sugar() {
        let rendered = dynamic_dispatch_render();

        assert_boundary_loss(
            &rendered,
            "rust-dyn-trait-preserves-call-contract-loses-concrete-receiver-type",
        );
    }

    #[test]
    fn discriminates_dynamic_dispatch_boundary_from_missing_concept() {
        let rendered = dynamic_dispatch_render();
        let missing = emit_stub(
            "call_dyn",
            &strings(&["target", "arg"]),
            &strings(&["&dyn Fn(i64) -> i64", "i64"]),
            "i64",
            "concept:dynamic-dispatch-missing",
        );

        assert!(!rendered.is_stub);
        assert!(missing.is_stub);
        assert!(rendered.source.contains("&dyn Fn(i64) -> i64"));
        assert!(!missing.source.contains("dispatched(arg)"));
    }

    #[test]
    fn renders_closure_boundary_realization() {
        let rendered = boundary_render("call_closure", "concept:closure");

        assert!(!rendered.is_stub);
        assert!(rendered.source.contains("let f = move ||"));
        assert!(rendered.source.contains("f()"));
    }

    #[test]
    fn records_closure_boundary_loss_without_sugar() {
        let rendered = boundary_render("call_closure", "concept:closure");

        assert_boundary_loss(
            &rendered,
            "rust-closure-preserves-capture-and-call-loses-explicit-environment-record",
        );
    }

    #[test]
    fn discriminates_closure_boundary_from_missing_concept() {
        assert_boundary_discriminates("concept:closure", "let f = move ||");
    }

    #[test]
    fn renders_iterator_boundary_realization() {
        let rendered = emit_stub(
            "first_item",
            &strings(&["items"]),
            &strings(&["list"]),
            "i64",
            "concept:iterator",
        );

        assert!(!rendered.is_stub);
        assert!(rendered.source.contains("let mut iter = items.iter();"));
        assert!(rendered.source.contains("iter.next().copied().unwrap_or_default()"));
    }

    #[test]
    fn records_iterator_boundary_loss_without_sugar() {
        let rendered = emit_stub(
            "first_item",
            &strings(&["items"]),
            &strings(&["list"]),
            "i64",
            "concept:iterator",
        );

        assert_boundary_loss(
            &rendered,
            "rust-iterator-preserves-next-protocol-loses-container-identity",
        );
    }

    #[test]
    fn discriminates_iterator_boundary_from_missing_concept() {
        let rendered = emit_stub(
            "first_item",
            &strings(&["items"]),
            &strings(&["list"]),
            "i64",
            "concept:iterator",
        );
        let missing = emit_stub(
            "first_item",
            &strings(&["items"]),
            &strings(&["list"]),
            "i64",
            "concept:iterator-missing",
        );

        assert!(!rendered.is_stub);
        assert!(missing.is_stub);
        assert!(rendered.source.contains("items.iter()"));
        assert!(!missing.source.contains("items.iter()"));
    }

    #[test]
    fn renders_generic_instantiation_boundary_realization() {
        let rendered = boundary_render("generic_id", "concept:generic-instantiation");

        assert!(!rendered.is_stub);
        assert!(rendered.source.contains("std::convert::identity::<i64>(arg)"));
    }

    #[test]
    fn records_generic_instantiation_boundary_loss_without_sugar() {
        let rendered = boundary_render("generic_id", "concept:generic-instantiation");

        assert_boundary_loss(
            &rendered,
            "rust-monomorphization-preserves-type-application-loses-runtime-genericity",
        );
    }

    #[test]
    fn discriminates_generic_instantiation_boundary_from_missing_concept() {
        assert_boundary_discriminates("concept:generic-instantiation", "identity::<i64>");
    }

    #[test]
    fn renders_reference_boundary_realization() {
        let rendered = boundary_render("borrow_value", "concept:reference");

        assert!(!rendered.is_stub);
        assert!(rendered.source.contains("let r: &i64 = &arg;"));
        assert!(rendered.source.contains("*r"));
    }

    #[test]
    fn records_reference_boundary_loss_without_sugar() {
        let rendered = boundary_render("borrow_value", "concept:reference");

        assert_boundary_loss(
            &rendered,
            "rust-reference-preserves-aliasing-and-borrow-loses-cross-language-ownership",
        );
    }

    #[test]
    fn discriminates_reference_boundary_from_missing_concept() {
        assert_boundary_discriminates("concept:reference", "let r: &i64 = &arg;");
    }

    fn boundary_render(function: &str, concept_name: &str) -> Realization {
        emit_stub(
            function,
            &strings(&["arg"]),
            &strings(&["i64"]),
            "i64",
            concept_name,
        )
    }

    fn dynamic_dispatch_render() -> Realization {
        emit_stub(
            "call_dyn",
            &strings(&["target", "arg"]),
            &strings(&["&dyn Fn(i64) -> i64", "i64"]),
            "i64",
            "concept:dynamic-dispatch",
        )
    }

    fn assert_boundary_loss(rendered: &Realization, loss_name: &str) {
        assert!(!rendered.is_stub);
        assert!(rendered.used_sugars.is_empty());
        assert!(!rendered.source.contains("provekit-concept:"));
        let names = rendered
            .observed_loss_record
            .pointer("/loss_record_contribution/value")
            .and_then(Value::as_object)
            .map(|value| {
                value
                    .values()
                    .filter_map(|item| item.get("name").and_then(Value::as_str))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        assert!(names.contains(&loss_name));
    }

    fn assert_boundary_discriminates(concept_name: &str, expected_source: &str) {
        let rendered = boundary_render("boundary", concept_name);
        let missing = boundary_render("boundary", &format!("{concept_name}-missing"));

        assert!(!rendered.is_stub);
        assert!(missing.is_stub);
        assert!(rendered.source.contains(expected_source));
        assert!(!missing.source.contains(expected_source));
    }

    fn env_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    fn temp_operator_root(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("{name}_{nanos}"));
        std::fs::create_dir_all(&root).expect("create temp operator root");
        root
    }
}
