// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use provekit_ir_types::Sort;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::bind::{concept_bind_result_cid, named_term_document_from_bind_payload, NamedTerm};
use super::primitives::address;
use super::traits::{Kit, KitError};
use super::types::{
    formula_true, memento_from_parts, Cid, Dialect, DomainClaim, DomainKind, Input, Term, Verdict,
};

/// Request shape for the PEP 1.7.0 realize surface.
///
/// The transport serializes this directly as plugin params. Callers should not
/// construct it while migrating to path execution; `LowerKit::transform`
/// materializes this payload from `Input` and invokes the configured transport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealizeRequest {
    pub function: String,
    pub params: Vec<String>,
    pub param_types: Vec<String>,
    pub return_type: String,
    pub concept_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub named_term_tree: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub modes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract: Option<RealizeContractPayload>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sugar_cids: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sugar_plugins: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealizeContractPayload {
    pub concept_site_cid: String,
    pub object_fcm_cid: String,
    pub local_contract_cid: String,
    pub origin: String,
    pub discharge_verdict: String,
    pub witnesses: Vec<RealizeContractWitness>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealizeContractWitness {
    pub role: String,
    pub predicate: Value,
    pub predicate_text: String,
    pub source_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealizedSource {
    pub extension: String,
    pub source: String,
    pub is_stub: bool,
    pub emitted_artifact_cid: Option<String>,
    pub observed_loss_record: Value,
    pub used_sugars: Vec<Value>,
    pub observation_wrapper_emission_record: Option<Value>,
}

/// Transport boundary used by `LowerKit` to call the existing realize layer.
pub trait RealizeTransport {
    fn dispatch_realize(
        &self,
        workspace_root: &Path,
        target_lang: &str,
        library_tag: Option<&str>,
        request: &RealizeRequest,
    ) -> Result<RealizedSource, String>;
}

/// Core Kit adapter over the existing realize transport.
#[derive(Debug, Clone)]
pub struct LowerKit<T> {
    workspace_root: PathBuf,
    target_lang: String,
    library_tag: Option<String>,
    transport: T,
}

impl<T> LowerKit<T> {
    pub fn new(
        workspace_root: impl Into<PathBuf>,
        target_lang: impl Into<String>,
        library_tag: Option<String>,
        transport: T,
    ) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            target_lang: target_lang.into(),
            library_tag,
            transport,
        }
    }
}

impl<T: RealizeTransport> LowerKit<T> {
    /// Recover the legacy transport response from a lower claim payload.
    pub fn realized_source_from_claim(claim: &DomainClaim) -> Result<RealizedSource, String> {
        let Some(Term::Const { value, .. }) = &claim.payload else {
            return Err("lower claim missing realize response payload".to_string());
        };
        serde_json::from_value(value.clone())
            .map_err(|error| format!("decode lower claim payload: {error}"))
    }
}

impl<T: RealizeTransport + 'static> Kit for LowerKit<T> {
    fn dialect(&self) -> Dialect {
        Dialect::Other(self.target_lang.clone())
    }

    fn transform(&self, input: &Input) -> Result<DomainClaim, KitError> {
        let invocation = RealizeInvocation::from_input(input, &self.target_lang, &self.library_tag)
            .map_err(KitError::Transformation)?;
        let realized = self
            .transport
            .dispatch_realize(
                &self.workspace_root,
                &self.target_lang,
                invocation.effective_library_tag.as_deref(),
                &invocation.request,
            )
            .map_err(|error| {
                KitError::Transformation(format!("realize plugin transport: {error}"))
            })?;
        Ok(claim_from_realized(invocation, realized))
    }

    fn prove(&self, claim: DomainClaim) -> Result<DomainClaim, KitError> {
        Ok(claim)
    }

    fn parse(&self, input: &Input) -> Result<Term, KitError> {
        self.transform(input)?
            .payload
            .ok_or_else(|| KitError::Serialization("lower claim missing term payload".to_string()))
    }

    fn serialize(&self, term: &Term) -> Result<Input, KitError> {
        Ok(Input::Term(term.clone()))
    }
}

struct RealizeInvocation {
    request: RealizeRequest,
    from: Vec<Cid>,
    premises: Vec<Cid>,
    target_library_cid: Cid,
    effective_library_tag: Option<String>,
    policy_cid: Option<Cid>,
    body_template_cids: Vec<Cid>,
}

impl RealizeInvocation {
    fn from_input(
        input: &Input,
        target_lang: &str,
        configured_library_tag: &Option<String>,
    ) -> Result<Self, String> {
        let spec = spec_value_from_input(input)?;
        let fallback_from = fallback_from(input);
        let request = request_from_spec(&spec)?;
        let effective_library_tag =
            string_field_optional(&spec, &["libraryTag", "library_tag", "library"])
                .or_else(|| configured_library_tag.clone());
        let target_library_cid = target_library_cid(
            target_lang,
            effective_library_tag.as_deref().unwrap_or("default"),
        );
        let from = from_cids(&spec, fallback_from)?;
        let mut premises = explicit_cid_array(&spec, &["premises"])?;
        if let Input::Claim(claim) = input {
            premises.push(claim.cid());
        }
        dedup_cids(&mut premises);
        let policy_cid = optional_cid_field(&spec, &["policyCid", "policy_cid"])?;
        let body_template_cids =
            explicit_cid_array(&spec, &["bodyTemplateCids", "body_template_cids"])?;
        Ok(Self {
            request,
            from,
            premises,
            target_library_cid,
            effective_library_tag,
            policy_cid,
            body_template_cids,
        })
    }
}

fn spec_value_from_input(input: &Input) -> Result<Value, String> {
    match input {
        Input::Spec(value) => Ok(value.clone()),
        Input::Term(Term::Const { value, .. }) => Ok(value.clone()),
        Input::Claim(claim) => claim_spec_value(claim),
        _ => Err("lower kit expects Input::Spec, Input::Term, or Input::Claim".to_string()),
    }
}

fn claim_spec_value(claim: &DomainClaim) -> Result<Value, String> {
    if let Some(Term::Op { op_cid, args, .. }) = &claim.payload {
        if op_cid == &concept_bind_result_cid() {
            return decompose_bind_result(args, claim);
        }
    }
    if let Some(Term::Const { value, .. }) = &claim.payload {
        return Ok(value.clone());
    }
    let param_types = claim
        .contract
        .formal_sorts
        .iter()
        .map(sort_name)
        .collect::<Vec<_>>();
    Ok(json!({
        "function": claim.contract.fn_name,
        "params": claim.contract.formals,
        "paramTypes": param_types,
        "returnType": sort_name(&claim.contract.return_sort),
        "conceptName": claim.contract.concept_hint.clone().unwrap_or_else(|| claim.contract.fn_name.clone()),
        "termShapeCid": claim.to,
    }))
}

fn decompose_bind_result(args: &[Term], _claim: &DomainClaim) -> Result<Value, String> {
    if args.len() != 2 {
        return Err(format!(
            "bind-result wrapper expected 2 args, got {}",
            args.len()
        ));
    }
    let wrapper = Term::Op {
        op_cid: concept_bind_result_cid(),
        name: "concept:bind-result".to_string(),
        args: args.to_vec(),
    };
    let named = named_term_document_from_bind_payload(&wrapper)
        .map_err(|error| format!("decompose bind-result wrapper named form: {error}"))?;
    let term_count = named.terms.len();
    let [term] = named.terms.as_slice() else {
        return Err(format!(
            "bind-result wrapper expected exactly one named term, got {term_count}"
        ));
    };
    realize_spec_from_named_term(term)
}

pub fn realize_spec_from_named_term(term: &NamedTerm) -> Result<Value, String> {
    let named_term_tree = term
        .named_term_tree
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|error| format!("serialize namedTermTree for `{}`: {error}", term.function))?;
    let (param_types, return_type) = realize_signature_from_named_term(term);
    Ok(json!({
        "kind": "RealizeRequest",
        "function": term.function,
        "params": term.params,
        "paramTypes": param_types,
        "returnType": return_type,
        "conceptName": term.concept_name,
        "namedTermTree": named_term_tree,
        "termShapeCid": term.term_shape_cid,
    }))
}

fn realize_signature_from_named_term(term: &NamedTerm) -> (Vec<String>, String) {
    let erased = term.param_types.is_empty() && matches!(term.return_type.trim(), "" | "()");
    if !erased {
        return (term.param_types.clone(), term.return_type.clone());
    }

    let param_types = term
        .params
        .iter()
        .map(|_| "int".to_string())
        .collect::<Vec<_>>();
    let return_type = if is_unit_concept(&term.concept_name) {
        "()".to_string()
    } else {
        "int".to_string()
    };
    (param_types, return_type)
}

fn is_unit_concept(concept_name: &str) -> bool {
    let trimmed = concept_name.trim();
    trimmed == "unit" || trimmed == "concept:unit"
}

fn fallback_from(input: &Input) -> Vec<Cid> {
    match input {
        Input::Claim(claim) if !claim.from.is_empty() => claim.from.clone(),
        Input::Claim(claim) => vec![claim.to.clone()],
        _ => vec![address(input)],
    }
}

fn request_from_spec(spec: &Value) -> Result<RealizeRequest, String> {
    let contract = match non_null_field(spec, &["contract"]) {
        Some(value) => Some(
            serde_json::from_value(value.clone())
                .map_err(|error| format!("decode realize contract payload: {error}"))?,
        ),
        None => None,
    };
    Ok(RealizeRequest {
        function: required_string_field(spec, &["function"])?,
        params: string_array_field(spec, &["params"])?,
        param_types: string_array_field(spec, &["paramTypes", "param_types"])?,
        return_type: required_string_field(spec, &["returnType", "return_type"])?,
        concept_name: required_string_field(spec, &["conceptName", "concept_name"])?,
        named_term_tree: non_null_field(spec, &["namedTermTree", "named_term_tree"]).cloned(),
        mode: string_field_optional(spec, &["mode"]),
        modes: string_array_field(spec, &["modes"]).unwrap_or_default(),
        contract,
        sugar_cids: string_array_field(spec, &["sugarCids", "sugar_cids"]).unwrap_or_default(),
        sugar_plugins: value_array_field(spec, &["sugarPlugins", "sugar_plugins"]),
    })
}

fn claim_from_realized(invocation: RealizeInvocation, realized: RealizedSource) -> DomainClaim {
    let response_term = realized_source_term(&realized);
    let response_cid = address(&response_term);
    let to = realized
        .emitted_artifact_cid
        .as_deref()
        .and_then(|cid| Cid::parse(cid).ok())
        .unwrap_or_else(|| response_cid.clone());
    let mut artifacts = vec![
        invocation.target_library_cid.clone(),
        to.clone(),
        response_cid,
    ];
    if let Some(policy_cid) = invocation.policy_cid {
        artifacts.push(policy_cid);
    }
    artifacts.extend(invocation.body_template_cids);
    artifacts.extend(
        invocation
            .request
            .sugar_cids
            .iter()
            .filter_map(|cid| Cid::parse(cid.as_str()).ok()),
    );
    artifacts.extend(realized.used_sugars.iter().filter_map(cid_from_used_sugar));
    if let Some(loss_cid) = loss_record_cid(&realized.observed_loss_record) {
        artifacts.push(loss_cid);
    }
    dedup_cids(&mut artifacts);

    let formal_sorts = invocation
        .request
        .param_types
        .iter()
        .map(|name| Sort::Primitive { name: name.clone() })
        .collect();
    let return_sort = Sort::Primitive {
        name: invocation.request.return_type.clone(),
    };
    let contract = memento_from_parts(
        invocation.request.function,
        invocation.request.params,
        formal_sorts,
        return_sort,
        formula_true(),
        formula_true(),
        Some(to.to_string()),
    );

    DomainClaim {
        domain: DomainKind::Other("lower-realize".to_string()),
        contract,
        artifacts,
        from: invocation.from,
        premises: invocation.premises,
        to,
        witness: None,
        payload: Some(response_term),
        verdict: Verdict::Unresolved,
        attestation: None,
    }
}

fn realized_source_term(realized: &RealizedSource) -> Term {
    Term::Const {
        value: serde_json::to_value(realized).expect("realized source serializes"),
        sort: Sort::Primitive {
            name: "RealizePluginResponse".to_string(),
        },
    }
}

fn target_library_cid(target_lang: &str, library_tag: &str) -> Cid {
    address(&Input::Spec(json!({
        "targetLanguage": target_lang,
        "libraryTag": library_tag,
    })))
}

fn from_cids(spec: &Value, fallback: Vec<Cid>) -> Result<Vec<Cid>, String> {
    let explicit = explicit_cid_array(spec, &["from"])?;
    if !explicit.is_empty() {
        return Ok(explicit);
    }
    if let Some(cid) = optional_cid_field(
        spec,
        &[
            "termShapeCid",
            "term_shape_cid",
            "termTreeCid",
            "term_tree_cid",
            "namedTermTreeCid",
            "named_term_tree_cid",
            "inputCid",
            "input_cid",
        ],
    )? {
        return Ok(vec![cid]);
    }
    Ok(fallback)
}

fn field<'a>(spec: &'a Value, names: &[&str]) -> Option<&'a Value> {
    names.iter().find_map(|name| spec.get(*name))
}

fn non_null_field<'a>(spec: &'a Value, names: &[&str]) -> Option<&'a Value> {
    field(spec, names).filter(|value| !value.is_null())
}

fn required_string_field(spec: &Value, names: &[&str]) -> Result<String, String> {
    string_field_optional(spec, names)
        .ok_or_else(|| format!("realize spec missing string field `{}`", names[0]))
}

fn string_field_optional(spec: &Value, names: &[&str]) -> Option<String> {
    field(spec, names)
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn string_array_field(spec: &Value, names: &[&str]) -> Result<Vec<String>, String> {
    let Some(value) = field(spec, names) else {
        return Err(format!("realize spec missing array field `{}`", names[0]));
    };
    let items = value
        .as_array()
        .ok_or_else(|| format!("realize spec field `{}` must be an array", names[0]))?;
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("realize spec field `{}` must contain strings", names[0]))
        })
        .collect()
}

fn value_array_field(spec: &Value, names: &[&str]) -> Vec<Value> {
    field(spec, names)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn optional_cid_field(spec: &Value, names: &[&str]) -> Result<Option<Cid>, String> {
    let Some(value) = field(spec, names) else {
        return Ok(None);
    };
    let Some(text) = value.as_str() else {
        return Err(format!(
            "realize spec field `{}` must be a CID string",
            names[0]
        ));
    };
    Cid::parse(text)
        .map(Some)
        .map_err(|error| format!("realize spec field `{}`: {error}", names[0]))
}

fn explicit_cid_array(spec: &Value, names: &[&str]) -> Result<Vec<Cid>, String> {
    let Some(value) = field(spec, names) else {
        return Ok(Vec::new());
    };
    match value {
        Value::Array(items) => items
            .iter()
            .map(|item| {
                let Some(text) = item.as_str() else {
                    return Err(format!(
                        "realize spec field `{}` must contain CID strings",
                        names[0]
                    ));
                };
                Cid::parse(text)
                    .map_err(|error| format!("realize spec field `{}`: {error}", names[0]))
            })
            .collect(),
        Value::String(text) => Cid::parse(text)
            .map(|cid| vec![cid])
            .map_err(|error| format!("realize spec field `{}`: {error}", names[0])),
        _ => Err(format!(
            "realize spec field `{}` must be a CID string or array",
            names[0]
        )),
    }
}

fn loss_record_cid(record: &Value) -> Option<Cid> {
    match record {
        Value::Null => None,
        Value::Object(map) if map.is_empty() => None,
        _ => cid_field_from_value(record).or_else(|| {
            Some(address(&Term::Const {
                value: record.clone(),
                sort: Sort::Primitive {
                    name: "ObservedLossRecord".to_string(),
                },
            }))
        }),
    }
}

fn cid_from_used_sugar(value: &Value) -> Option<Cid> {
    value
        .get("header")
        .and_then(|header| header.get("cid"))
        .and_then(Value::as_str)
        .or_else(|| value.as_str())
        .and_then(|cid| Cid::parse(cid).ok())
}

fn cid_field_from_value(value: &Value) -> Option<Cid> {
    ["cid", "lossRecordCid", "loss_record_cid", "recordCid"]
        .iter()
        .filter_map(|name| value.get(*name))
        .find_map(|value| value.as_str())
        .and_then(|cid| Cid::parse(cid).ok())
}

fn dedup_cids(cids: &mut Vec<Cid>) {
    cids.sort();
    cids.dedup();
}

fn sort_name(sort: &Sort) -> String {
    match sort {
        Sort::Primitive { name } => name.clone(),
        other => serde_json::to_string(other).expect("sort serializes"),
    }
}
