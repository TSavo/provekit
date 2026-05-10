// SPDX-License-Identifier: Apache-2.0

pub mod catalog;
pub mod signer;
mod spec;

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

pub use catalog::{Catalog, CatalogEntry, CatalogIndex, Kind};
pub use signer::Signer;
pub use spec::{
    AlgorithmSpec, BindingSpec, EffectSignatureSpec, EquationSpec, LanguageMorphismSpec,
    LanguageSignatureSpec, SortSpec,
};

pub type Result<T> = std::result::Result<T, MintError>;

#[derive(Debug, thiserror::Error)]
pub enum MintError {
    #[error("validation error: {0}")]
    Validation(String),
    #[error("catalog error: {0}")]
    Catalog(String),
    #[error("signer error: {0}")]
    Signer(String),
    #[error("canonicalization error: {0}")]
    Canonical(String),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("JSON error at {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
}

#[derive(Debug, Clone)]
pub struct MintedMemento {
    pub cid: String,
    pub path: PathBuf,
    pub payload: Value,
}

pub fn canonical_jcs(value: &Value) -> Result<String> {
    libprovekit::canonical::json_jcs(value).map_err(|e| MintError::Canonical(e.to_string()))
}

pub fn canonical_cid(value: &Value) -> Result<String> {
    libprovekit::canonical::json_cid(value).map_err(|e| MintError::Canonical(e.to_string()))
}

pub fn mint_algorithm(
    spec: AlgorithmSpec,
    signer: &Signer,
    catalog: &Catalog,
) -> Result<MintedMemento> {
    let payload = algorithm_payload(&spec)?;
    write_signed(
        Kind::Algorithm,
        &required_string(&spec.raw, "fn_name")?,
        payload,
        signer,
        catalog,
    )
}

pub fn mint_binding(
    spec: BindingSpec,
    signer: &Signer,
    catalog: &Catalog,
) -> Result<MintedMemento> {
    let algorithm_cid = match optional_reference(&spec.raw, "algorithm", &spec, signer, catalog)? {
        Some(cid) => cid,
        None => first_input_cid(&spec.raw, &spec, signer, catalog).ok_or_else(|| {
            MintError::Validation(
                "BindingMemento requires `algorithm` or a non-empty `input_cids` array".into(),
            )
        })??,
    };
    let payload = binding_payload(&spec, algorithm_cid, signer, catalog)?;
    write_signed(
        Kind::Binding,
        &required_string(&spec.raw, "fn_name")?,
        payload,
        signer,
        catalog,
    )
}

pub fn mint_sort(spec: SortSpec, signer: &Signer, catalog: &Catalog) -> Result<MintedMemento> {
    let payload = sort_payload(&spec)?;
    write_signed(
        Kind::Sort,
        &required_string(&spec.raw, "fn_name")?,
        payload,
        signer,
        catalog,
    )
}

pub fn mint_equation(
    spec: EquationSpec,
    signer: &Signer,
    catalog: &Catalog,
) -> Result<MintedMemento> {
    let payload = equation_payload(&spec, signer, catalog)?;
    write_signed(
        Kind::Equation,
        &required_string(&spec.raw, "fn_name")?,
        payload,
        signer,
        catalog,
    )
}

pub fn mint_effect_signature(
    spec: EffectSignatureSpec,
    signer: &Signer,
    catalog: &Catalog,
) -> Result<MintedMemento> {
    let payload = language_signature_payload(
        &spec,
        "EffectSignatureMemento",
        "LSP",
        "EffectSignature",
        true,
        signer,
        catalog,
    )?;
    write_signed(
        Kind::EffectSignature,
        &required_string(&spec.raw, "fn_name")?,
        payload,
        signer,
        catalog,
    )
}

pub fn mint_language_signature(
    spec: LanguageSignatureSpec,
    signer: &Signer,
    catalog: &Catalog,
) -> Result<MintedMemento> {
    let payload = language_signature_payload(
        &spec,
        "LanguageSignatureMemento",
        "LSP",
        "LanguageSignature",
        false,
        signer,
        catalog,
    )?;
    write_signed(
        Kind::LanguageSignature,
        &required_string(&spec.raw, "fn_name")?,
        payload,
        signer,
        catalog,
    )
}

pub fn mint_language_morphism(
    spec: LanguageMorphismSpec,
    signer: &Signer,
    catalog: &Catalog,
) -> Result<MintedMemento> {
    let payload = language_morphism_payload(&spec, signer, catalog)?;
    write_signed(
        Kind::LanguageMorphism,
        &required_string(&spec.raw, "fn_name")?,
        payload,
        signer,
        catalog,
    )
}

fn write_signed(
    kind: Kind,
    name: &str,
    payload: Value,
    signer: &Signer,
    catalog: &Catalog,
) -> Result<MintedMemento> {
    signer.ensure_catalog_allowed(catalog.root())?;
    let cid = canonical_cid(&payload)?;
    let path = catalog.write_signed_memento(kind, name, &payload, &cid, signer)?;
    Ok(MintedMemento { cid, path, payload })
}

fn algorithm_payload(spec: &AlgorithmSpec) -> Result<Value> {
    validate_kind(spec.raw(), &["algorithm", "AlgorithmMemento"])?;
    validate_contract_fields(spec.raw(), true, true, true, true)?;
    validate_formal_lengths(spec.raw())?;
    let mut payload = base_contract_payload("AMP", "AlgorithmMemento", spec.raw())?;
    payload["auto_minted_mementos"] = json!([]);
    Ok(payload)
}

fn binding_payload(
    spec: &BindingSpec,
    algorithm_cid: String,
    signer: &Signer,
    catalog: &Catalog,
) -> Result<Value> {
    validate_kind(
        spec.raw(),
        &["binding", "BindingMemento", "BindingClaimMemento"],
    )?;
    validate_contract_fields(spec.raw(), true, true, true, true)?;
    validate_formal_lengths(spec.raw())?;
    require_cid_field(spec.raw(), "body_cid")?;

    let mut payload = base_contract_payload("AMP", "BindingMemento", spec.raw())?;
    payload["language"] = optional_string(spec.raw(), "language")
        .map(Value::String)
        .unwrap_or(Value::Null);
    payload["algorithm_cid"] = Value::String(algorithm_cid.clone());
    payload["input_cids"] = Value::Array(input_cids_with(spec, algorithm_cid, signer, catalog)?);
    payload["post"] = resolve_embedded_refs(
        required_value(spec.raw(), "post")?,
        spec.base_dir(),
        signer,
        catalog,
    )?;
    payload["discharge_status"] = Value::String("pending".into());
    payload["auto_minted_mementos"] = json!([]);
    Ok(payload)
}

fn sort_payload(spec: &SortSpec) -> Result<Value> {
    validate_kind(spec.raw(), &["sort", "SortMemento"])?;
    require_string(spec.raw(), "fn_name")?;
    required_value(spec.raw(), "return_sort")?;
    required_value(spec.raw(), "post")?;
    let mut payload = base_contract_payload("LSP", "SortMemento", spec.raw())?;
    if !payload_has_key(&payload, "formal_sorts") {
        payload["formal_sorts"] = json!([]);
    }
    Ok(payload)
}

fn equation_payload(spec: &EquationSpec, signer: &Signer, catalog: &Catalog) -> Result<Value> {
    validate_kind(spec.raw(), &["equation", "EquationMemento"])?;
    require_string(spec.raw(), "fn_name")?;
    required_array(spec.raw(), "formals")?;
    required_array(spec.raw(), "formal_sorts")?;
    required_value(spec.raw(), "post")?;
    validate_formal_lengths(spec.raw())?;

    let mut payload = base_contract_payload("LSP", "EquationMemento", spec.raw())?;
    payload["formal_sorts"] = Value::Array(resolve_reference_array(
        spec,
        "formal_sorts",
        signer,
        catalog,
        Some(Kind::Sort),
    )?);
    payload["return_sort"] = spec
        .raw()
        .get("return_sort")
        .cloned()
        .unwrap_or_else(|| json!({"kind": "primitive", "name": "Bool"}));
    payload["effects"] = json!({"effects": []});
    Ok(payload)
}

fn language_signature_payload<S: JsonSpec>(
    spec: &S,
    kind: &str,
    protocol: &str,
    return_name: &str,
    require_empty_effects: bool,
    signer: &Signer,
    catalog: &Catalog,
) -> Result<Value> {
    let accepted = if require_empty_effects {
        vec!["effect_signature", "EffectSignatureMemento"]
    } else {
        vec!["language_signature", "LanguageSignatureMemento"]
    };
    validate_kind(spec.raw(), &accepted)?;
    require_string(spec.raw(), "fn_name")?;
    for field in ["sorts", "operations", "equations", "effect_signatures"] {
        required_array(spec.raw(), field)?;
    }
    if require_empty_effects && !required_array(spec.raw(), "effect_signatures")?.is_empty() {
        return Err(MintError::Validation(
            "EffectSignatureMemento requires an empty `effect_signatures` array".into(),
        ));
    }

    let sorts = resolve_reference_array(spec, "sorts", signer, catalog, Some(Kind::Sort))?;
    let operations =
        resolve_reference_array(spec, "operations", signer, catalog, Some(Kind::Algorithm))?;
    let equations =
        resolve_reference_array(spec, "equations", signer, catalog, Some(Kind::Equation))?;
    let effect_signatures = resolve_reference_array(
        spec,
        "effect_signatures",
        signer,
        catalog,
        Some(Kind::EffectSignature),
    )?;

    let mut payload = base_static_payload(protocol, kind, spec.raw());
    payload["return_sort"] = spec
        .raw()
        .get("return_sort")
        .cloned()
        .unwrap_or_else(|| json!({"kind": "ctor", "name": return_name, "args": []}));
    payload["post"] = json!({
        "sorts": sorts,
        "operations": operations,
        "equations": equations,
        "effect_signatures": effect_signatures,
    });
    if let Some(body_cid) = optional_string(spec.raw(), "body_cid") {
        validate_cid(&body_cid, "body_cid")?;
        payload["body_cid"] = Value::String(body_cid);
    }
    Ok(payload)
}

fn language_morphism_payload(
    spec: &LanguageMorphismSpec,
    signer: &Signer,
    catalog: &Catalog,
) -> Result<Value> {
    validate_kind(
        spec.raw(),
        &["language_morphism", "LanguageMorphismMemento"],
    )?;
    require_string(spec.raw(), "fn_name")?;
    require_string(spec.raw(), "source_signature")?;
    require_string(spec.raw(), "target_signature")?;
    required_value(spec.raw(), "post")?;
    require_cid_field(spec.raw(), "body_cid")?;

    let source_cid = required_reference(
        spec.raw(),
        "source_signature",
        spec,
        signer,
        catalog,
        Some(Kind::LanguageSignature),
    )?;
    let target_cid = required_reference(
        spec.raw(),
        "target_signature",
        spec,
        signer,
        catalog,
        Some(Kind::LanguageSignature),
    )?;

    let mut payload = base_contract_payload("LSP", "LanguageMorphismMemento", spec.raw())?;
    payload["formals"] = spec
        .raw()
        .get("formals")
        .cloned()
        .unwrap_or_else(|| json!(["source_term"]));
    payload["formal_sorts"] = spec
        .raw()
        .get("formal_sorts")
        .cloned()
        .unwrap_or_else(|| json!([{"kind": "TermInLanguage", "signature_cid": source_cid}]));
    payload["return_sort"] = spec
        .raw()
        .get("return_sort")
        .cloned()
        .unwrap_or_else(|| json!({"kind": "TermInLanguage", "signature_cid": target_cid}));
    payload["source_signature_cid"] = Value::String(source_cid.clone());
    payload["target_signature_cid"] = Value::String(target_cid.clone());
    payload["input_cids"] = json!([source_cid, target_cid]);
    payload["post"] = resolve_embedded_refs(
        required_value(spec.raw(), "post")?,
        spec.base_dir(),
        signer,
        catalog,
    )?;
    payload["discharge_status"] = Value::String("pending".into());
    Ok(payload)
}

fn base_contract_payload(protocol: &str, kind: &str, raw: &Value) -> Result<Value> {
    let mut payload = base_static_payload(protocol, kind, raw);
    payload["formals"] = raw.get("formals").cloned().unwrap_or_else(|| json!([]));
    payload["formal_sorts"] = raw
        .get("formal_sorts")
        .cloned()
        .unwrap_or_else(|| json!([]));
    payload["return_sort"] = raw
        .get("return_sort")
        .cloned()
        .unwrap_or_else(|| json!({"kind": "primitive", "name": "Bool"}));
    payload["pre"] = raw.get("pre").cloned().unwrap_or_else(true_formula);
    payload["post"] = raw
        .get("post")
        .cloned()
        .ok_or_else(|| MintError::Validation("missing required field `post`".into()))?;
    payload["effects"] = raw.get("effects").cloned().unwrap_or_else(empty_effects);
    if let Some(locus) = raw.get("locus") {
        payload["locus"] = locus.clone();
    }
    if let Some(body_cid) = optional_string(raw, "body_cid") {
        validate_cid(&body_cid, "body_cid")?;
        payload["body_cid"] = Value::String(body_cid);
    }
    if let Some(input_cids) = raw.get("input_cids") {
        payload["input_cids"] = input_cids.clone();
    }
    if let Some(refines) = raw.get("refines") {
        payload["refines"] = refines.clone();
    }
    Ok(payload)
}

fn base_static_payload(protocol: &str, kind: &str, raw: &Value) -> Value {
    json!({
        "schema_version": "1",
        "protocol": protocol,
        "kind": kind,
        "fn_name": raw.get("fn_name").cloned().unwrap_or(Value::Null),
        "formals": [],
        "formal_sorts": [],
        "pre": true_formula(),
        "post": Value::Null,
        "effects": empty_effects(),
        "auto_minted_mementos": [],
    })
}

fn validate_contract_fields(
    raw: &Value,
    formals: bool,
    formal_sorts: bool,
    return_sort: bool,
    pre_post: bool,
) -> Result<()> {
    require_string(raw, "fn_name")?;
    if formals {
        required_array(raw, "formals")?;
    }
    if formal_sorts {
        required_array(raw, "formal_sorts")?;
    }
    if return_sort {
        required_value(raw, "return_sort")?;
    }
    if pre_post {
        required_value(raw, "pre")?;
        required_value(raw, "post")?;
    }
    Ok(())
}

fn validate_formal_lengths(raw: &Value) -> Result<()> {
    let formals = raw.get("formals").and_then(Value::as_array);
    let formal_sorts = raw.get("formal_sorts").and_then(Value::as_array);
    if let (Some(formals), Some(formal_sorts)) = (formals, formal_sorts) {
        if formals.len() != formal_sorts.len() {
            return Err(MintError::Validation(format!(
                "`formals` length {} does not match `formal_sorts` length {}",
                formals.len(),
                formal_sorts.len()
            )));
        }
    }
    Ok(())
}

fn validate_kind(raw: &Value, accepted: &[&str]) -> Result<()> {
    let Some(kind) = raw.get("kind").and_then(Value::as_str) else {
        return Ok(());
    };
    if accepted.contains(&kind) {
        Ok(())
    } else {
        Err(MintError::Validation(format!(
            "spec kind `{kind}` is not one of {}",
            accepted.join(", ")
        )))
    }
}

fn require_string(raw: &Value, field: &str) -> Result<()> {
    required_string(raw, field).map(|_| ())
}

fn required_string(raw: &Value, field: &str) -> Result<String> {
    raw.get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| MintError::Validation(format!("missing required string field `{field}`")))
}

fn optional_string(raw: &Value, field: &str) -> Option<String> {
    raw.get(field).and_then(Value::as_str).map(str::to_string)
}

fn required_array<'a>(raw: &'a Value, field: &str) -> Result<&'a Vec<Value>> {
    raw.get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| MintError::Validation(format!("missing required array field `{field}`")))
}

fn required_value<'a>(raw: &'a Value, field: &str) -> Result<&'a Value> {
    raw.get(field)
        .ok_or_else(|| MintError::Validation(format!("missing required field `{field}`")))
}

fn payload_has_key(payload: &Value, key: &str) -> bool {
    payload
        .as_object()
        .map(|obj| obj.contains_key(key))
        .unwrap_or(false)
}

fn require_cid_field(raw: &Value, field: &str) -> Result<()> {
    let value = required_string(raw, field)?;
    validate_cid(&value, field)
}

fn validate_cid(value: &str, field: &str) -> Result<()> {
    if libprovekit::canonical::is_blake3_512_cid(value) {
        Ok(())
    } else {
        Err(MintError::Validation(format!(
            "`{field}` must be a blake3-512 CID"
        )))
    }
}

fn first_input_cid<S: JsonSpec>(
    raw: &Value,
    spec: &S,
    signer: &Signer,
    catalog: &Catalog,
) -> Option<Result<String>> {
    raw.get("input_cids")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .map(|item| resolve_reference_value(item, spec.base_dir(), signer, catalog, None))
}

fn input_cids_with<S: JsonSpec>(
    spec: &S,
    required_cid: String,
    signer: &Signer,
    catalog: &Catalog,
) -> Result<Vec<Value>> {
    let mut out = vec![Value::String(required_cid.clone())];
    if let Some(items) = spec.raw().get("input_cids").and_then(Value::as_array) {
        for item in items {
            let cid = resolve_reference_value(item, spec.base_dir(), signer, catalog, None)?;
            if cid != required_cid && !out.iter().any(|v| v.as_str() == Some(cid.as_str())) {
                out.push(Value::String(cid));
            }
        }
    }
    Ok(out)
}

fn optional_reference<S: JsonSpec>(
    raw: &Value,
    field: &str,
    spec: &S,
    signer: &Signer,
    catalog: &Catalog,
) -> Result<Option<String>> {
    raw.get(field)
        .map(|value| resolve_reference_value(value, spec.base_dir(), signer, catalog, None))
        .transpose()
}

fn required_reference<S: JsonSpec>(
    raw: &Value,
    field: &str,
    spec: &S,
    signer: &Signer,
    catalog: &Catalog,
    expected: Option<Kind>,
) -> Result<String> {
    resolve_reference_value(
        required_value(raw, field)?,
        spec.base_dir(),
        signer,
        catalog,
        expected,
    )
}

fn resolve_reference_array<S: JsonSpec>(
    spec: &S,
    field: &str,
    signer: &Signer,
    catalog: &Catalog,
    expected: Option<Kind>,
) -> Result<Vec<Value>> {
    let items = required_array(spec.raw(), field)?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(Value::String(resolve_reference_value(
            item,
            spec.base_dir(),
            signer,
            catalog,
            expected,
        )?));
    }
    Ok(out)
}

fn resolve_embedded_refs(
    value: &Value,
    base_dir: Option<&Path>,
    signer: &Signer,
    catalog: &Catalog,
) -> Result<Value> {
    match value {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(resolve_embedded_refs(item, base_dir, signer, catalog)?);
            }
            Ok(Value::Array(out))
        }
        Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (key, item) in map {
                out.insert(
                    key.clone(),
                    resolve_embedded_refs(item, base_dir, signer, catalog)?,
                );
            }
            Ok(Value::Object(out))
        }
        Value::String(s) if looks_like_json_path(s) => {
            let path = resolve_path(base_dir, s);
            if path.exists() {
                Ok(Value::String(mint_spec_path(&path, signer, catalog, None)?))
            } else {
                Ok(value.clone())
            }
        }
        _ => Ok(value.clone()),
    }
}

fn resolve_reference_value(
    value: &Value,
    base_dir: Option<&Path>,
    signer: &Signer,
    catalog: &Catalog,
    expected: Option<Kind>,
) -> Result<String> {
    let s = value
        .as_str()
        .ok_or_else(|| MintError::Validation("CID reference must be a string".into()))?;
    if libprovekit::canonical::is_blake3_512_cid(s) {
        return Ok(s.to_string());
    }
    let path = resolve_path(base_dir, s);
    if !path.exists() {
        return Err(MintError::Validation(format!(
            "reference `{s}` is neither a CID nor an existing spec path"
        )));
    }
    mint_spec_path(&path, signer, catalog, expected)
}

fn mint_spec_path(
    path: &Path,
    signer: &Signer,
    catalog: &Catalog,
    expected: Option<Kind>,
) -> Result<String> {
    let raw = read_json(path)?;
    let kind = kind_from_spec(&raw).ok_or_else(|| {
        MintError::Validation(format!(
            "referenced spec {} is missing `kind`",
            path.display()
        ))
    })?;
    if let Some(expected) = expected {
        if kind != expected {
            return Err(MintError::Validation(format!(
                "referenced spec {} has kind `{kind}`, expected `{expected}`",
                path.display()
            )));
        }
    }
    let cid = match kind {
        Kind::Algorithm => mint_algorithm(AlgorithmSpec::from_path(path)?, signer, catalog)?.cid,
        Kind::Binding => mint_binding(BindingSpec::from_path(path)?, signer, catalog)?.cid,
        Kind::Sort => mint_sort(SortSpec::from_path(path)?, signer, catalog)?.cid,
        Kind::Equation => mint_equation(EquationSpec::from_path(path)?, signer, catalog)?.cid,
        Kind::EffectSignature => {
            mint_effect_signature(EffectSignatureSpec::from_path(path)?, signer, catalog)?.cid
        }
        Kind::LanguageSignature => {
            mint_language_signature(LanguageSignatureSpec::from_path(path)?, signer, catalog)?.cid
        }
        Kind::LanguageMorphism => {
            mint_language_morphism(LanguageMorphismSpec::from_path(path)?, signer, catalog)?.cid
        }
    };
    Ok(cid)
}

fn kind_from_spec(raw: &Value) -> Option<Kind> {
    match raw.get("kind").and_then(Value::as_str)? {
        "algorithm" | "AlgorithmMemento" => Some(Kind::Algorithm),
        "binding" | "BindingMemento" | "BindingClaimMemento" => Some(Kind::Binding),
        "sort" | "SortMemento" => Some(Kind::Sort),
        "equation" | "EquationMemento" => Some(Kind::Equation),
        "effect_signature" | "EffectSignatureMemento" => Some(Kind::EffectSignature),
        "language_signature" | "LanguageSignatureMemento" => Some(Kind::LanguageSignature),
        "language_morphism" | "LanguageMorphismMemento" => Some(Kind::LanguageMorphism),
        _ => None,
    }
}

fn resolve_path(base_dir: Option<&Path>, raw: &str) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        base_dir.unwrap_or_else(|| Path::new(".")).join(path)
    }
}

fn looks_like_json_path(value: &str) -> bool {
    value.ends_with(".json") || value.ends_with(".spec.json") || value.contains('/')
}

fn read_json(path: &Path) -> Result<Value> {
    let bytes = std::fs::read(path).map_err(|source| MintError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| MintError::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn true_formula() -> Value {
    json!({"kind": "atomic", "name": "true", "args": []})
}

fn empty_effects() -> Value {
    json!({"effects": []})
}

pub trait JsonSpec {
    fn raw(&self) -> &Value;
    fn base_dir(&self) -> Option<&Path>;
}
