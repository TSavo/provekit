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
    pub term_shape: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operand_bindings: Vec<Value>,
    #[serde(
        default,
        rename = "procMacroInvocations",
        alias = "proc_macro_invocations",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub proc_macro_invocations: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_function_name: Option<String>,
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
    /// #1359 / #1355: realization-tuple pins propagated through the
    /// spec. `family` and `library_version` flow from the @sugar /
    /// @boundary annotation (via the carrier payload) AND from the
    /// shim's library-sugar-binding-entry (via augment_spec_with_shim_term_shape).
    /// dispatch_realize reads them to perform family-aware constraint-
    /// satisfaction over the realize-manifest registry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library_version: Option<String>,
    /// Substrate-honest cross-language signature pins. Each entry is a
    /// concept-hub sort CID (blake3-512:...) — the kit-internal sort
    /// labels stay inside the source kit. The target kit's realize
    /// reads these to look up its OWN type syntax via its catalog
    /// morphism (concept-hub → kit-internal sort → kit-target-syntax).
    /// Empty strings in the vec signal "kit has no morphism for this
    /// param type" — substrate-honest gap signal.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_sort_cids: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub return_sort_cid: String,
    /// The dispatcher-resolved `library_tag` (from the realize manifest
    /// dispatched by kit_dispatch). The realize plugin uses this to
    /// disambiguate body-template entries when multiple libraries ship
    /// templates for the same concept. Without this field, multi-library
    /// body-template caches degrade to load-order-dependent selection.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target_library_tag: String,
    /// #1369: parametric sort identities via compositional content-addressing.
    ///
    /// When a `param_sort_cid` or `return_sort_cid` refers to a parametric
    /// sort (concept:Ref<T>, concept:List<T>, concept:Map<K,V>), its identity
    /// is computed from (constructor_cid, arg_cids) per the substrate's
    /// canonical form. The realize plugin needs the (cid → canonical_form)
    /// expansion to dispatch its parameterized morphism. Lift side computes
    /// the composite CID + populates this map; realize side reads it.
    ///
    /// Each entry: (composite_cid → ParametricSortExpansion). Primitive sort
    /// CIDs (non-parametric) are absent from this map.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parametric_sort_expansions: Vec<ParametricSortExpansion>,
}

/// Canonical form of a parametric sort application — the structure whose
/// JCS encoding's blake3-512 IS the composite CID. Carried in
/// RealizeRequest.parametric_sort_expansions so the realize plugin can
/// decompose composite CIDs into (constructor, args) for parameterized
/// morphism dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParametricSortExpansion {
    /// The composite CID this expansion identifies. cid == blake3-512(JCS({
    /// "kind": "parametric-sort-application",
    /// "constructor_cid": <constructor_cid>,
    /// "arg_cids": <arg_cids>
    /// }))
    pub cid: String,
    pub constructor_cid: String,
    pub arg_cids: Vec<String>,
}

impl ParametricSortExpansion {
    /// Compute the composite CID for a parametric sort application.
    /// Same (constructor, args) always produces the same CID (substrate
    /// content-addressing invariant); different args produce different CIDs.
    pub fn compose_cid(constructor_cid: &str, arg_cids: &[String]) -> String {
        let canonical = serde_json::json!({
            "kind": "parametric-sort-application",
            "constructor_cid": constructor_cid,
            "arg_cids": arg_cids,
        });
        let jcs = crate::canonical::serializable_jcs(&canonical)
            .expect("parametric expansion canonicalizes");
        provekit_canonicalizer::blake3_512_of(jcs.as_bytes())
    }

    /// Build an expansion + its composite CID from inputs.
    pub fn build(constructor_cid: &str, arg_cids: Vec<String>) -> Self {
        let cid = Self::compose_cid(constructor_cid, &arg_cids);
        Self {
            cid,
            constructor_cid: constructor_cid.to_string(),
            arg_cids,
        }
    }
}

/// Catalog-driven source-token alias entry for a kit (#1370).
///
/// The substrate's KitSourceAliasMemento files declare which source-text
/// tokens denote which kit-sort. Lifters read these via catalog query
/// instead of hardcoded switches. Three flavors:
///   - Primitive: token denotes a primitive sort directly (no args)
///   - Constructor: token denotes a parametric constructor (takes args at use-site)
///   - Shorthand: token denotes a fixed parametric application
#[derive(Debug, Clone)]
pub enum KitSourceAliasEntry {
    Primitive { target_cid: String },
    Constructor { constructor_cid: String, arity: usize },
    Shorthand { composite_cid: String, constructor_cid: String, arg_cids: Vec<String> },
}

/// Load all KitSourceAliasMemento files for a given kit from the catalog.
/// Returns a map: source-text token → KitSourceAliasEntry.
///
/// Walks up from CWD to find menagerie/, then reads
/// concept-shapes/catalog/kit-source-aliases/<kit>-*.json.
pub fn load_kit_source_aliases(kit: &str) -> std::collections::BTreeMap<String, KitSourceAliasEntry> {
    let mut map = std::collections::BTreeMap::new();
    let Some(root) = find_menagerie_root() else { return map; };
    let aliases_dir = root.join("menagerie")
        .join("concept-shapes").join("catalog").join("kit-source-aliases");
    if !aliases_dir.is_dir() { return map; }
    let algorithms_dir = root.join("menagerie")
        .join("concept-shapes").join("catalog").join("algorithms");
    let prefix = format!("{}-", kit);
    let Ok(entries) = std::fs::read_dir(&aliases_dir) else { return map; };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !name.starts_with(&prefix) || !name.ends_with(".json") { continue; }
        let Ok(raw) = std::fs::read_to_string(&path) else { continue };
        let Ok(doc): Result<serde_json::Value, _> = serde_json::from_str(&raw) else { continue };
        let Some(memento) = doc.get("memento") else { continue };
        let Some(morphism_cid) = memento.get("sort_morphism_cid").and_then(|v| v.as_str()) else { continue };
        let Some(target_cid) = resolve_morphism_target_cid(&algorithms_dir, morphism_cid) else { continue };
        let Some(aliases) = memento.get("source_aliases").and_then(|v| v.as_array()) else { continue };
        let shorthand = memento.get("denotes_parametric_application");
        let arity = memento.get("parametric_arity").and_then(|v| v.as_u64());
        for alias_v in aliases {
            let Some(token) = alias_v.as_str() else { continue };
            let entry = if let Some(sh) = shorthand.and_then(|v| v.as_object()) {
                let ctor = sh.get("constructor_cid").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let arg_cids: Vec<String> = sh.get("arg_cids")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                let composite = ParametricSortExpansion::compose_cid(&ctor, &arg_cids);
                KitSourceAliasEntry::Shorthand { composite_cid: composite, constructor_cid: ctor, arg_cids }
            } else if let Some(a) = arity {
                KitSourceAliasEntry::Constructor { constructor_cid: target_cid.clone(), arity: a as usize }
            } else {
                KitSourceAliasEntry::Primitive { target_cid: target_cid.clone() }
            };
            map.entry(token.to_string()).or_insert(entry);
        }
    }
    map
}

fn find_menagerie_root() -> Option<std::path::PathBuf> {
    let mut p = std::env::current_dir().ok()?;
    loop {
        if p.join("menagerie").is_dir() { return Some(p); }
        p = p.parent()?.to_path_buf();
    }
}

fn resolve_morphism_target_cid(algorithms_dir: &std::path::Path, morphism_cid: &str) -> Option<String> {
    let entries = std::fs::read_dir(algorithms_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name()?.to_str()?;
        if !name.contains(morphism_cid) || !name.ends_with(".json") { continue; }
        let raw = std::fs::read_to_string(&path).ok()?;
        let doc: serde_json::Value = serde_json::from_str(&raw).ok()?;
        let target = doc.get("header")?.get("target_sort_cid")?.as_str()?;
        return Some(target.to_string());
    }
    None
}

/// Resolve a rust type-string to a concept-hub sort CID via the kit-source-alias
/// catalog (#1370). Parametric types produce COMPOSITE CIDs computed via
/// blake3-512(JCS(constructor + args)). The `expansions` accumulator captures
/// each composite CID's canonical form so the realize plugin can decompose them.
///
/// NO hardcoded source-token names. The map is loaded once via
/// load_kit_source_aliases("rust") and queried recursively for parametric types.
pub fn rust_type_to_concept_hub_sort_cid(
    rust_type: &str,
    aliases: &std::collections::BTreeMap<String, KitSourceAliasEntry>,
    expansions: &mut Vec<ParametricSortExpansion>,
) -> Option<String> {
    let trimmed = rust_type.trim();

    // &mut T: prefix syntax for parametric Ref constructor.
    // walk_rpc emits `&mutT` (no space, sugar_type_surface strips spaces);
    // source_transform keeps `&mut T`. Handle both.
    if trimmed.starts_with("&mut ") || (trimmed.starts_with("&mut") && !trimmed.starts_with("&mute")) {
        let inner_src = if trimmed.starts_with("&mut ") {
            &trimmed[5..]
        } else {
            &trimmed[4..]
        };
        let inner_cid = rust_type_to_concept_hub_sort_cid(inner_src, aliases, expansions)?;
        let mut_alias = aliases.get("&mut")?;
        if let KitSourceAliasEntry::Constructor { constructor_cid, .. } = mut_alias {
            let exp = ParametricSortExpansion::build(constructor_cid, vec![inner_cid]);
            let cid = exp.cid.clone();
            if !expansions.iter().any(|e| e.cid == cid) {
                expansions.push(exp);
            }
            return Some(cid);
        }
        return None;
    }

    // Strip leading & (immutable borrow: same value identity at substrate level).
    let t = trimmed.trim_start_matches('&').trim();

    // Strip Option<T> and Result<T, E> wrappers (success arm only).
    let t = if let Some(stripped) = t.strip_prefix("Option<").and_then(|s| s.strip_suffix('>')) {
        stripped.trim()
    } else if let Some(stripped) = t.strip_prefix("Result<") {
        let mut depth = 0i32;
        let mut end = stripped.len();
        for (i, ch) in stripped.chars().enumerate() {
            match ch {
                '<' => depth += 1,
                '>' => depth -= 1,
                ',' if depth == 0 => { end = i; break; }
                _ => {}
            }
        }
        stripped[..end].trim()
    } else {
        t
    };

    // Direct lookup first — handles shorthand tokens like "Vec<u8>", "[u8]", "()".
    if let Some(entry) = aliases.get(t) {
        return resolve_alias_entry(entry, &[], aliases, expansions);
    }

    // Parse outer<args>: Foo<A, B> → outer="Foo", args=["A", "B"]
    if let Some(open) = t.find('<') {
        if t.ends_with('>') {
            let outer = t[..open].trim();
            let inside = &t[open+1..t.len()-1];
            let arg_srcs = split_top_level_commas(inside);
            if let Some(entry) = aliases.get(outer) {
                return resolve_alias_entry(entry, &arg_srcs, aliases, expansions);
            }
        }
    }

    // [T; N] / [T] → composite via the Vec/List constructor when not byte
    if let Some(rest) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let inner_src = rest.split(';').next().unwrap_or(rest).trim();
        // Inner u8 → bytes shorthand
        if inner_src == "u8" {
            if let Some(entry) = aliases.get("[u8]") {
                return resolve_alias_entry(entry, &[], aliases, expansions);
            }
        }
        if let Some(entry) = aliases.get("Vec") {
            if let KitSourceAliasEntry::Constructor { .. } = entry {
                return resolve_alias_entry(entry, &[inner_src.to_string()], aliases, expansions);
            }
        }
    }

    None
}

fn resolve_alias_entry(
    entry: &KitSourceAliasEntry,
    arg_srcs: &[String],
    aliases: &std::collections::BTreeMap<String, KitSourceAliasEntry>,
    expansions: &mut Vec<ParametricSortExpansion>,
) -> Option<String> {
    match entry {
        KitSourceAliasEntry::Primitive { target_cid } => Some(target_cid.clone()),
        KitSourceAliasEntry::Shorthand { composite_cid, constructor_cid, arg_cids } => {
            let exp = ParametricSortExpansion {
                cid: composite_cid.clone(),
                constructor_cid: constructor_cid.clone(),
                arg_cids: arg_cids.clone(),
            };
            if !expansions.iter().any(|e| e.cid == exp.cid) {
                expansions.push(exp);
            }
            Some(composite_cid.clone())
        }
        KitSourceAliasEntry::Constructor { constructor_cid, arity } => {
            if arg_srcs.len() != *arity { return None; }
            let mut arg_cids = Vec::with_capacity(arg_srcs.len());
            for a in arg_srcs {
                arg_cids.push(rust_type_to_concept_hub_sort_cid(a, aliases, expansions)?);
            }
            let exp = ParametricSortExpansion::build(constructor_cid, arg_cids);
            let cid = exp.cid.clone();
            if !expansions.iter().any(|e| e.cid == cid) {
                expansions.push(exp);
            }
            Some(cid)
        }
    }
}

fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in s.chars() {
        if c == '<' { depth += 1; }
        else if c == '>' { depth -= 1; }
        if c == ',' && depth == 0 {
            out.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    if !cur.trim().is_empty() { out.push(cur.trim().to_string()); }
    out
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

/// #1374: structured realize-plugin response shape.
///
/// Realize plugins return a FRAGMENT — a per-site emission that says
/// what the fragment IS plus what CONTEXT it needs (imports, helper
/// declarations, dependencies, diagnostics, compile-unit requirements).
/// Assembly (Milestone C, #1375) composes fragments into compilation
/// units; the substrate doesn't bake file semantics into the CLI.
///
/// Backwards compatibility: legacy plugins return only `source` + `is_stub`
/// (the other fields default to empty/None). New plugins populate them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RealizedSource {
    pub extension: String,
    pub source: String,
    pub is_stub: bool,
    pub emitted_artifact_cid: Option<String>,
    pub observed_loss_record: Value,
    pub used_sugars: Vec<Value>,
    pub observation_wrapper_emission_record: Option<Value>,

    /// #1374: realization-fragment context.
    ///
    /// Symbols the fragment's emitted source uses from outside its own
    /// body. Java: fully-qualified class names ("java.util.List",
    /// "com.fasterxml.jackson.databind.JsonNode"). Rust: use-path strings
    /// ("std::io::BufRead", "serde_json::Value"). Python: import names.
    /// Assembly deduplicates these across fragments and emits the
    /// import block(s) idiomatically for the target language.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<String>,

    /// Helper declarations the fragment needs in the surrounding
    /// compilation unit. Each entry is a kit-specific shape with at
    /// least a `name`, `kind` (static-field / static-init / function /
    /// type-alias / module-private / etc.), and `source` (the helper's
    /// declaration text). Assembly hoists these into the appropriate
    /// scope (java: static fields in the class, static init blocks;
    /// rust: const/lazy_static items; python: module-level).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub helpers: Vec<Value>,

    /// Build-time dependencies the fragment requires. Each entry
    /// declares (kind, coords): kind = "maven" / "cargo" / "pip" /
    /// "npm" / "system"; coords = the kit's package-coordinate string
    /// ("com.fasterxml.jackson.core:jackson-databind:2.17.0",
    /// "serde_json = 1", ...). Assembly surfaces these to the user
    /// (or auto-populates project files in future work).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<Value>,

    /// Realize-side diagnostics — informational, warning, or error
    /// notes the plugin wants to attach to the fragment. Each entry
    /// is at least {severity: info/warn/error, message: string}.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Value>,

    /// Requirements the compilation unit must satisfy for this
    /// fragment to compile cleanly. Kit-specific keys:
    /// - java: `package`, `language_version`, `top_level` (class/interface/record)
    /// - rust: `edition`, `module_path`
    /// - python: `version`, `module`
    /// Assembly reconciles requirements across fragments + materializes
    /// the appropriate compilation-unit shell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compile_unit_requirements: Option<Value>,
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
        let mut invocation =
            RealizeInvocation::from_input(input, &self.target_lang, &self.library_tag)
                .map_err(KitError::Transformation)?;
        let recursive_library_tag = invocation.effective_library_tag.clone();
        let child_loss_records = invocation
            .realize_recursive_children(
                &self.workspace_root,
                &self.target_lang,
                recursive_library_tag.as_deref(),
                &self.transport,
            )
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
        let mut realized = realized;
        realized.observed_loss_record =
            merge_observed_loss_records(child_loss_records, realized.observed_loss_record);
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

    fn realize_recursive_children<T: RealizeTransport>(
        &mut self,
        workspace_root: &Path,
        target_lang: &str,
        library_tag: Option<&str>,
        transport: &T,
    ) -> Result<Vec<Value>, String> {
        let Some(tree) = self.request.named_term_tree.clone() else {
            return Ok(Vec::new());
        };
        let mut stack = vec![format!(
            "{}#{}",
            self.request.concept_name,
            self.request.mode.as_deref().unwrap_or("")
        )];
        collect_recursive_child_bindings(
            &tree,
            self,
            workspace_root,
            target_lang,
            library_tag,
            transport,
            &mut stack,
        )
    }
}

fn collect_recursive_child_bindings<T: RealizeTransport>(
    tree: &Value,
    invocation: &mut RealizeInvocation,
    workspace_root: &Path,
    target_lang: &str,
    library_tag: Option<&str>,
    transport: &T,
    stack: &mut Vec<String>,
) -> Result<Vec<Value>, String> {
    let Some(composition_point) = optional_composition_point(tree)? else {
        return Ok(Vec::new());
    };
    let children = recursive_children(tree)?;
    let mut loss_records = Vec::new();
    for (index, child) in children {
        let (claim, realized, binding) = realize_child_node(
            child,
            &invocation.request,
            workspace_root,
            target_lang,
            library_tag,
            transport,
            vec![index],
            &composition_point,
            stack,
        )?;
        invocation.premises.push(claim.cid());
        invocation.premises.extend(explicit_cid_array(
            child,
            &["claimCid", "claim_cid", "pathClaimCid", "path_claim_cid"],
        )?);
        dedup_cids(&mut invocation.premises);
        invocation.request.operand_bindings.push(binding);
        loss_records.push(realized.observed_loss_record);
    }
    Ok(loss_records)
}

fn realize_child_node<T: RealizeTransport>(
    node: &Value,
    parent_request: &RealizeRequest,
    workspace_root: &Path,
    target_lang: &str,
    library_tag: Option<&str>,
    transport: &T,
    position: Vec<usize>,
    composition_point: &str,
    stack: &mut Vec<String>,
) -> Result<(DomainClaim, RealizedSource, Value), String> {
    let concept_name = required_node_string(node, &["conceptName", "concept_name"])?;
    let mode = node_string(node, &["mode"]).or_else(|| parent_request.mode.clone());
    let stack_key = format!("{}#{}", concept_name, mode.as_deref().unwrap_or(""));
    if stack.iter().any(|seen| seen == &stack_key) {
        return Err(format!("recursive realization cycle at `{}`", concept_name));
    }
    if stack.len() >= 8 {
        return Err(format!(
            "recursive realization depth limit reached at `{}`",
            concept_name
        ));
    }
    stack.push(stack_key);

    let mut invocation = invocation_from_tree_node(node, parent_request, target_lang, library_tag)?;
    let child_loss_records = collect_recursive_child_bindings(
        node,
        &mut invocation,
        workspace_root,
        target_lang,
        library_tag,
        transport,
        stack,
    )?;
    let realized = transport
        .dispatch_realize(
            workspace_root,
            target_lang,
            invocation.effective_library_tag.as_deref(),
            &invocation.request,
        )
        .map_err(|error| {
            format!(
                "recursive child `{}` realization refused: {}",
                concept_name, error
            )
        })?;
    let mut realized = realized;
    realized.observed_loss_record =
        merge_observed_loss_records(child_loss_records, realized.observed_loss_record);
    let claim = claim_from_realized(invocation, realized.clone());
    let binding = recursive_child_binding(
        node,
        position,
        composition_point,
        &concept_name,
        &claim,
        &realized,
    );
    stack.pop();
    Ok((claim, realized, binding))
}

fn invocation_from_tree_node(
    node: &Value,
    parent_request: &RealizeRequest,
    target_lang: &str,
    library_tag: Option<&str>,
) -> Result<RealizeInvocation, String> {
    let concept_name = required_node_string(node, &["conceptName", "concept_name"])?;
    let function = node_string(node, &["function"])
        .unwrap_or_else(|| child_function_name(&parent_request.function, &concept_name));
    let params =
        node_string_array(node, &["params"])?.unwrap_or_else(|| parent_request.params.clone());
    let param_types = node_string_array(node, &["paramTypes", "param_types"])?
        .unwrap_or_else(|| parent_request.param_types.clone());
    let return_type = node_string(node, &["returnType", "return_type"])
        .unwrap_or_else(|| parent_request.return_type.clone());
    let mode = node_string(node, &["mode"]).or_else(|| parent_request.mode.clone());
    let modes =
        node_string_array(node, &["modes"])?.unwrap_or_else(|| parent_request.modes.clone());
    let operand_bindings = value_array_field(node, &["operandBindings", "operand_bindings"]);
    // Resolve effective library_tag FIRST so child requests payload matches
    // the library they're actually routed to (transport + body-template
    // selection both rely on this — previously they could disagree).
    let effective_library_tag = node_string(node, &["libraryTag", "library_tag", "library"])
        .or_else(|| library_tag.map(str::to_string));
    let request = RealizeRequest {
        function,
        params,
        param_types,
        return_type,
        concept_name,
        named_term_tree: Some(node.clone()),
        term_shape: node
            .get("termShape")
            .or_else(|| node.get("term_shape"))
            .cloned(),
        operand_bindings,
        source_function_name: node_string(node, &["sourceFunctionName", "source_function_name"])
            .or_else(|| parent_request.source_function_name.clone()),
        mode,
        modes,
        contract: parent_request.contract.clone(),
        sugar_cids: parent_request.sugar_cids.clone(),
        sugar_plugins: parent_request.sugar_plugins.clone(),
        family: parent_request.family.clone(),
        library_version: parent_request.library_version.clone(),
        param_sort_cids: node_string_array(node, &["paramSortCids", "param_sort_cids"])?
            .unwrap_or_else(|| parent_request.param_sort_cids.clone()),
        return_sort_cid: node_string(node, &["returnSortCid", "return_sort_cid"])
            .unwrap_or_else(|| parent_request.return_sort_cid.clone()),
        // Child target_library_tag = effective_library_tag (child-level
        // resolution) falling back to parent's. Previously always copied
        // parent, which mismatched the transport route when the child had
        // a libraryTag override.
        target_library_tag: effective_library_tag
            .clone()
            .unwrap_or_else(|| parent_request.target_library_tag.clone()),
        parametric_sort_expansions: parent_request.parametric_sort_expansions.clone(),
        proc_macro_invocations: value_array_field(
            node,
            &["procMacroInvocations", "proc_macro_invocations"],
        ),
    };
    let mut from = Vec::new();
    if let Some(shape_cid) = optional_cid_field(node, &["shapeCid", "shape_cid"])? {
        from.push(shape_cid);
    }
    if from.is_empty() {
        from.push(request_address(&request)?);
    }
    let mut premises = explicit_cid_array(node, &["premises"])?;
    premises.extend(explicit_cid_array(
        node,
        &["claimCid", "claim_cid", "pathClaimCid", "path_claim_cid"],
    )?);
    dedup_cids(&mut premises);
    let target_library_cid = target_library_cid(
        target_lang,
        effective_library_tag.as_deref().unwrap_or("default"),
    );
    Ok(RealizeInvocation {
        request,
        from,
        premises,
        target_library_cid,
        effective_library_tag,
        policy_cid: None,
        body_template_cids: Vec::new(),
    })
}

fn request_address(request: &RealizeRequest) -> Result<Cid, String> {
    let value = serde_json::to_value(request)
        .map_err(|error| format!("serialize child request: {error}"))?;
    Ok(address(&Term::Const {
        value,
        sort: Sort::Primitive {
            name: "RealizeRequest".to_string(),
        },
    }))
}

fn recursive_children(tree: &Value) -> Result<Vec<(usize, &Value)>, String> {
    let Some(args) = tree.get("args") else {
        return Ok(Vec::new());
    };
    let Some(items) = args.as_array() else {
        return Err("recursive namedTermTree args must be an array".to_string());
    };
    let mut children = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if !item.is_object() {
            return Err(format!(
                "recursive child at position {index} must be an object"
            ));
        }
        children.push((index, item));
    }
    Ok(children)
}

fn optional_composition_point(tree: &Value) -> Result<Option<String>, String> {
    let Some(point) = node_string(tree, &["compositionPoint", "composition_point"]) else {
        return Ok(None);
    };
    if matches!(
        point.as_str(),
        "before" | "after-return" | "after-throw" | "around"
    ) {
        Ok(Some(point))
    } else {
        Err(format!("unknown recursive composition point `{point}`"))
    }
}

fn required_node_string(node: &Value, names: &[&str]) -> Result<String, String> {
    node_string(node, names)
        .ok_or_else(|| format!("recursive concept node missing string field `{}`", names[0]))
}

fn node_string(node: &Value, names: &[&str]) -> Option<String> {
    field(node, names)
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn node_string_array(node: &Value, names: &[&str]) -> Result<Option<Vec<String>>, String> {
    let Some(value) = field(node, names) else {
        return Ok(None);
    };
    let Some(items) = value.as_array() else {
        return Err(format!(
            "recursive concept node field `{}` must be an array",
            names[0]
        ));
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let Some(text) = item.as_str() else {
            return Err(format!(
                "recursive concept node field `{}` must contain strings",
                names[0]
            ));
        };
        out.push(text.to_string());
    }
    Ok(Some(out))
}

fn child_function_name(parent_function: &str, concept_name: &str) -> String {
    let suffix = concept_name
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    if suffix.is_empty() {
        format!("{parent_function}__child")
    } else {
        format!("{parent_function}__{suffix}")
    }
}

fn recursive_child_binding(
    node: &Value,
    position: Vec<usize>,
    composition_point: &str,
    concept_name: &str,
    claim: &DomainClaim,
    realized: &RealizedSource,
) -> Value {
    json!({
        "kind": "recursive-child-realization",
        "position": position,
        "composition_point": composition_point,
        "concept_name": concept_name,
        "operation_kind": node_string(node, &["operationKind", "operation_kind"]).unwrap_or_default(),
        "shape_cid": node_string(node, &["shapeCid", "shape_cid"]).unwrap_or_default(),
        "child_claim_cid": claim.cid(),
        "child_output_cid": claim.to,
        "extension": realized.extension,
        "source": realized.source,
        "is_stub": realized.is_stub,
        "emitted_artifact_cid": realized.emitted_artifact_cid,
        "observed_loss_record": realized.observed_loss_record,
        "used_sugars": realized.used_sugars,
        "observation_wrapper_emission_record": realized.observation_wrapper_emission_record
    })
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

fn decompose_bind_result(args: &[Term], claim: &DomainClaim) -> Result<Value, String> {
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
    let mut spec = realize_spec_from_named_term(term)?;
    merge_realize_sidecar(&mut spec, claim, term)?;
    Ok(spec)
}

pub fn realize_spec_from_named_term(term: &NamedTerm) -> Result<Value, String> {
    let function = realize_function_name(term).to_string();
    let named_term_tree = term
        .named_term_tree
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|error| format!("serialize namedTermTree for `{function}`: {error}"))?;
    let (param_types, return_type) = realize_signature_from_named_term(term);
    Ok(json!({
        "kind": "RealizeRequest",
        "function": function,
        "params": term.params,
        "paramTypes": param_types,
        "returnType": return_type,
        "conceptName": term.concept_name,
        "namedTermTree": named_term_tree,
        "termShape": term.term_shape,
        "termShapeCid": term.term_shape_cid,
    }))
}

fn realize_function_name(term: &NamedTerm) -> &str {
    if term.function.trim().is_empty() {
        term.name.as_str()
    } else {
        term.function.as_str()
    }
}

/// Prefer order: function (if set) > fn_name_sugar (CLI pipe sugar) > name (concept fallback)
pub fn realize_function_name_with_sugar(term: &NamedTerm) -> &str {
    if !term.function.trim().is_empty() {
        return term.function.as_str();
    }
    if let Some(sugar) = term.fn_name_sugar.as_deref() {
        if !sugar.trim().is_empty() {
            return sugar;
        }
    }
    term.name.as_str()
}

fn merge_realize_sidecar(
    spec: &mut Value,
    claim: &DomainClaim,
    term: &NamedTerm,
) -> Result<(), String> {
    let Some(sidecar) = realize_sidecar_from_claim(claim)? else {
        return Ok(());
    };
    let Some(entry) = sidecar_entry_for_term(&sidecar, term) else {
        return Ok(());
    };
    if let Some(bindings) = entry.get("operand_bindings").and_then(Value::as_array) {
        spec["operandBindings"] = Value::Array(bindings.clone());
    }
    if let Some(invocations) = entry
        .get("proc_macro_invocations")
        .or_else(|| entry.get("procMacroInvocations"))
        .and_then(Value::as_array)
    {
        spec["procMacroInvocations"] = Value::Array(invocations.clone());
    }
    if let Some(function_name) = entry.get("source_function_name").and_then(Value::as_str) {
        if !function_name.is_empty() {
            spec["sourceFunctionName"] = Value::String(function_name.to_string());
        }
    }
    Ok(())
}

fn realize_sidecar_from_claim(claim: &DomainClaim) -> Result<Option<Value>, String> {
    let Some(hint) = claim.contract.concept_hint.as_deref() else {
        return Ok(None);
    };
    let Some(json_text) = hint.strip_prefix("provekit-realize-sidecar:") else {
        return Ok(None);
    };
    serde_json::from_str(json_text)
        .map(Some)
        .map_err(|error| format!("parse realize sidecar metadata: {error}"))
}

fn sidecar_entry_for_term<'a>(sidecar: &'a Value, term: &NamedTerm) -> Option<&'a Value> {
    let terms = sidecar.get("terms").and_then(Value::as_array)?;
    if terms.len() == 1 {
        return terms.first();
    }
    terms
        .iter()
        .find(|entry| entry.get("function").and_then(Value::as_str) == Some(term.function.as_str()))
}

fn realize_signature_from_named_term(term: &NamedTerm) -> (Vec<String>, String) {
    // Erasure heuristic: legacy bind payloads sometimes omit type info
    // entirely. Distinguish "no types declared" from "types declared as
    // empty/unit". `return_type == ""` is absence; `return_type == "()"`
    // is explicit unit. Functions with declared parameters or explicit
    // return type are NOT erased.
    let erased = term.param_types.is_empty()
        && term.params.is_empty()
        && term.return_type.trim().is_empty();
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

pub fn request_from_spec(spec: &Value) -> Result<RealizeRequest, String> {
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
        term_shape: non_null_field(spec, &["termShape", "term_shape"]).cloned(),
        operand_bindings: value_array_field(spec, &["operandBindings", "operand_bindings"]),
        proc_macro_invocations: value_array_field(
            spec,
            &["procMacroInvocations", "proc_macro_invocations"],
        ),
        source_function_name: string_field_optional(
            spec,
            &["sourceFunctionName", "source_function_name"],
        ),
        mode: string_field_optional(spec, &["mode"]),
        modes: string_array_field(spec, &["modes"]).unwrap_or_default(),
        contract,
        sugar_cids: string_array_field(spec, &["sugarCids", "sugar_cids"]).unwrap_or_default(),
        sugar_plugins: value_array_field(spec, &["sugarPlugins", "sugar_plugins"]),
        family: string_field_optional(spec, &["family"]),
        library_version: string_field_optional(spec, &["library_version", "libraryVersion"]),
        param_sort_cids: string_array_field(spec, &["param_sort_cids", "paramSortCids"])
            .unwrap_or_default(),
        return_sort_cid: string_field_optional(spec, &["return_sort_cid", "returnSortCid"])
            .unwrap_or_default(),
        target_library_tag: string_field_optional(
            spec,
            &["target_library_tag", "targetLibraryTag", "library_tag", "libraryTag"],
        )
        .unwrap_or_default(),
        parametric_sort_expansions: spec
            .get("parametric_sort_expansions")
            .or_else(|| spec.get("parametricSortExpansions"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
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

fn merge_observed_loss_records(child_records: Vec<Value>, parent_record: Value) -> Value {
    if child_records.is_empty() {
        return parent_record;
    }
    let mut merged = serde_json::Map::new();
    for record in child_records
        .into_iter()
        .chain(std::iter::once(parent_record))
    {
        merge_loss_record_value(&mut merged, record);
    }
    Value::Object(merged)
}

fn merge_loss_record_value(merged: &mut serde_json::Map<String, Value>, record: Value) {
    match record {
        Value::Object(entries) => {
            for (key, value) in entries {
                match merged.remove(&key) {
                    Some(existing) if existing == value => {
                        merged.insert(key, existing);
                    }
                    Some(existing) => {
                        merged.insert(
                            key,
                            json!({
                                "kind": "and",
                                "operands": [existing, value]
                            }),
                        );
                    }
                    None => {
                        merged.insert(key, value);
                    }
                }
            }
        }
        Value::Null => {}
        other => {
            merged.insert("non_object_loss_record".to_string(), other);
        }
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
