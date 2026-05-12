// SPDX-License-Identifier: Apache-2.0
//
// lift_plugin_protocol.rs
//
// Encodes the rules of `protocol/specs/2026-04-30-lift-plugin-protocol.md`
// (the "lift-plugin-protocol spec", v1.2.0 normative, LEGACY-RETAINED for
// `content`-payload shape under `kind = "lift"`) as machine-enforceable
// contract mementos. These contracts are the source-of-truth that each
// kit's lift-plugin implementation will bridge to: the Rust CLI dispatches
// to per-language lift plugins via JSON-RPC over stdio (LSP shape), and
// every peer plugin must satisfy these protocol-level invariants.
//
// Wire protocol-version token: as of `pep/1.7.0` (the Plugin Extension
// Protocol rename, `protocol/specs/2026-05-12-plugin-protocol.md` §0.4),
// the canonical EMIT value for the `protocol_version` field is `pep/1.7.0`.
// During the migration minor version, the legacy token `provekit-lift/1`
// is ACCEPTED on read with a loud deprecation notice; the NEXT minor
// version will refuse it. The verifier below implements the accept-set
// {"provekit-lift/1", "pep/1.7.0"} per plugin-protocol §0.4.
//
// Two layers, mirroring the catalog_format.rs convention:
//
//   1. `pub fn invariants()` authors one IR contract per rule via
//      the kit's `must` / `contract` collector. Each contract's formula
//      is a declarative published claim that names the rule. Most are
//      trivially-true-under-Z3 because the rule lives at the JSON-RPC
//      message layer (a string-equality, an array shape, a method name)
//      which the IR can only gesture at; operational enforcement is the
//      sibling verifier below.
//
//   2. Per-contract `verify_*` functions take a `serde_json::Value`
//      message and return `Result<(), String>`. The `#[test]` cases at
//      the bottom of the file construct sample requests/responses and
//      assert each verifier holds on conformant input and fires on
//      drifted input. Lighter than catalog_format.rs's CatalogReport
//      apparatus  the lift-plugin domain has fewer cross-rule
//      dependencies, so per-contract verifiers read cleaner.
//
// SCOPE  contracts encoded (each cites a spec line range from
// `2026-04-30-lift-plugin-protocol.md`):
//
//   C1 lift_plugin_initialize_protocol_version_match
//        request.protocol_version is in the accept-set
//        {"provekit-lift/1", "pep/1.7.0"} and response confirms the same
//        value, OR responds PROTOCOL_VERSION_MISMATCH. The canonical EMIT
//        value under pep/1.7.0 is "pep/1.7.0"; the legacy token is
//        accepted on read during the migration minor version per
//        2026-05-12-plugin-protocol.md §0.4. Spec lines 64-88.
//
//   C2 lift_plugin_initialize_capabilities_populated
//        response.capabilities.authoring_surfaces is a non-empty array of
//        strings; ir_version is a string starting with "v".
//        Spec lines 73-86, 44-48 (manifest mirror).
//
//   C3 lift_plugin_lift_request_well_formed
//        request.surface is a string; request.source_paths is a non-empty
//        array; each path is a non-empty string; request.options.layer is
//        either "all" or "identify-only"; options.identifyOnly, when
//        present, mirrors the layer.
//        Spec formal message grammar: lift-params/lift-options.
//
//   C4 lift_plugin_lift_request_surface_in_capabilities
//        request.surface  initialize-time capabilities.authoring_surfaces.
//        Spec lines 160-166.
//
//   C5 lift_plugin_lift_response_kind_matches_layer
//        when request.options.layer == "all", response.kind is one of
//        {"ir-document", "signed-mementos", "proof-envelope"}; when
//        request.options.layer == "identify-only", response.kind is one of
//        {"identity-document", "package-inspection-document"}.
//        Spec formal message grammar: all-layer-result/identify-only-result.
//
//   C6 lift_plugin_lift_response_ir_document_array
//        when kind == "ir-document", response.ir is an array (of IR-JSON
//        declarations conforming to `2026-04-30-ir-formal-grammar.md`).
//        Spec lines 104-117.
//
//   C7 lift_plugin_diagnostic_field_well_formed
//        response.diagnostics, when present, is an array. Spec line 208
//        ("Reports any diagnostics under the `diagnostics` field ...,
//        never silently dropping warnings") only mandates the field
//        shape; does not specify per-entry structure.
//
// Out of scope for THIS module:
//
//   * JCS / canonicalization-layer rules (no `<` `>` `&` escapes etc.)
//     belong to `2026-04-30-canonicalization-grammar.md`; discharge lives
//     in `provekit-canonicalizer/src/jcs.invariant.rs`. We do not
//     re-encode them here; encoding them in this module would force a
//     cross-spec citation that doesn't appear in the lift-plugin-protocol
//     spec text.
//
//   * Diagnostic per-entry schema (level / message fields). Spec line 208
//     says "diagnostics under the `diagnostics` field" but does not
//     mandate per-entry shape. C7 only checks the array container.
//
//   * `shutdown` method (lines 150-158): trivial response shape; no
//     normative MUST beyond "responds with `{result: null}` and exits".
//     Encoding is mechanical and would not catch real drift.

use std::rc::Rc;

use provekit_ir_symbolic::{
    contract, eq, forall, gte, must, num, str_const, ContractArgs, String_, Term,
};
use serde_json::Value as JsonValue;

// ---------------------------------------------------------------------------
// Layer 1: IR-contract authoring  `invariants()`
// ---------------------------------------------------------------------------

/// Wrap a single-arg ctor so the formula reads like a function call. The
/// IR carries the name through verbatim; Z3 has no axioms for these.
fn ctor1(name: &str, arg: Rc<Term>) -> Rc<Term> {
    Rc::new(Term::Ctor {
        name: name.into(),
        args: vec![arg],
    })
}

/// Wrap a two-arg ctor for predicates that pair the message subject with
/// a path or expected literal.
fn ctor2(name: &str, a: Rc<Term>, b: Rc<Term>) -> Rc<Term> {
    Rc::new(Term::Ctor {
        name: name.into(),
        args: vec![a, b],
    })
}

/// Mint contract mementos for the lift-plugin-protocol spec rules.
///
/// Each contract is named `lift_plugin_<rule>`. The IR body is a
/// declarative statement of the rule; operational discharge is the
/// per-rule `verify_*` function below.
pub fn invariants() {
    // -- C1: initialize protocol_version match (spec lines 64-88). ---------
    //
    // forall req. response_protocol_version(req) =
    //             request_protocol_version(req)
    //         OR  response_error_code(req) = "PROTOCOL_VERSION_MISMATCH"
    //
    // Encoded as a post: the response either confirms the version or
    // emits the named error code. We use a paired-equality ctor to
    // express the disjunction in a way the IR's atomic-only predicate
    // language can carry through (the verifier discharges the actual
    // case-split).
    contract(
        "lift_plugin_initialize_protocol_version_match",
        ContractArgs {
            pre: Some(eq(
                ctor1("request_protocol_version", str_const("req")),
                str_const("pep/1.7.0"),
            )),
            post: Some(eq(
                ctor1(
                    "response_confirms_protocol_or_errors_mismatch",
                    str_const("req"),
                ),
                ctor1("true_const", str_const("")),
            )),
            ..Default::default()
        },
    );

    // -- C2: initialize capabilities populated (spec lines 73-86). ---------
    //
    // forall resp. len(authoring_surfaces_of(resp)) >= 1
    //              AND ir_version_starts_with_v(resp) = true.
    //
    // Two facets, two atomic predicates. The IR encodes the "at least
    // one surface" lower bound directly via gte/len; the "ir_version
    // starts with v" goes through a named ctor since string-prefix is
    // not in the kit's atomic predicate set.
    must(
        "lift_plugin_initialize_capabilities_authoring_surfaces_nonempty",
        forall(String_(), |resp| {
            gte(ctor1("len", ctor1("authoring_surfaces_of", resp)), num(1))
        }),
    );

    contract(
        "lift_plugin_initialize_capabilities_ir_version_starts_with_v",
        ContractArgs {
            post: Some(eq(
                ctor1("ir_version_starts_with_v", str_const("resp")),
                ctor1("true_const", str_const("")),
            )),
            ..Default::default()
        },
    );

    // -- C3: lift request well-formed. -------------------------------------
    //
    // forall req. is_string(surface_of(req)) = true
    //          AND len(source_paths_of(req)) >= 1
    //          AND every path is a non-empty string.
    //
    // The "every path non-empty" facet uses a named ctor because the
    // IR's quantifier-over-array-elements lives a layer up from the
    // sort system here. Operational verifier discharges all three.
    contract(
        "lift_plugin_lift_request_surface_is_string",
        ContractArgs {
            post: Some(eq(
                ctor1("is_string", ctor1("surface_of", str_const("req"))),
                ctor1("true_const", str_const("")),
            )),
            ..Default::default()
        },
    );

    must(
        "lift_plugin_lift_request_source_paths_nonempty",
        forall(String_(), |req| {
            gte(ctor1("len", ctor1("source_paths_of", req)), num(1))
        }),
    );

    contract(
        "lift_plugin_lift_request_source_paths_each_nonempty",
        ContractArgs {
            post: Some(eq(
                ctor1("all_source_paths_nonempty", str_const("req")),
                ctor1("true_const", str_const("")),
            )),
            ..Default::default()
        },
    );

    contract(
        "lift_plugin_lift_request_options_layer_well_formed",
        ContractArgs {
            post: Some(eq(
                ctor1("lift_options_layer_well_formed", str_const("req")),
                ctor1("true_const", str_const("")),
            )),
            ..Default::default()
        },
    );

    // -- C4: lift surface in capabilities (spec lines 160-166). ------------
    //
    // forall req, caps. surface_of(req)  authoring_surfaces_of(caps).
    //
    // Encoded via a named membership ctor; the verifier carries the
    // pair (request, capabilities-from-init) and runs the lookup.
    contract(
        "lift_plugin_lift_request_surface_in_capabilities",
        ContractArgs {
            post: Some(eq(
                ctor2(
                    "surface_in_capabilities",
                    str_const("req"),
                    str_const("caps"),
                ),
                ctor1("true_const", str_const("")),
            )),
            ..Default::default()
        },
    );

    // -- C5: lift response kind matches request layer. ---------------------
    //
    // forall req, resp. response_kind_matches_requested_layer(req, resp) = true.
    //
    // The set is a finite literal; the IR ctor name encodes the
    // discharge target.
    contract(
        "lift_plugin_lift_response_kind_matches_layer",
        ContractArgs {
            post: Some(eq(
                ctor2(
                    "response_kind_matches_requested_layer",
                    str_const("req"),
                    str_const("resp"),
                ),
                ctor1("true_const", str_const("")),
            )),
            ..Default::default()
        },
    );

    // -- C6: when kind == "ir-document", response.ir is an array. ----------
    //         (Spec lines 104-117.)
    //
    // forall resp. (kind_of(resp) = "ir-document") implies
    //              is_array(ir_field_of(resp)) = true.
    //
    // Implication is in the IR; we use a named guarded ctor that
    // collapses to vacuous-true when kind is not "ir-document".
    contract(
        "lift_plugin_lift_response_ir_document_array",
        ContractArgs {
            post: Some(eq(
                ctor1("ir_field_is_array_when_kind_ir_document", str_const("resp")),
                ctor1("true_const", str_const("")),
            )),
            ..Default::default()
        },
    );

    // -- C7: diagnostics field, when present, is an array. -----------------
    //         (Spec line 208.) Per-entry schema NOT encoded; spec only
    //         mandates the field container shape.
    contract(
        "lift_plugin_diagnostic_field_is_array",
        ContractArgs {
            post: Some(eq(
                ctor1("diagnostics_field_is_array_or_absent", str_const("resp")),
                ctor1("true_const", str_const("")),
            )),
            ..Default::default()
        },
    );

    // -- C8: lifter emits call-edge stream alongside contracts (spec #114 §1
    //        R1: "A lifter that emits contracts but no call edges is
    //        non-conformant under this spec").
    //
    // forall resp. when kind == "proof-envelope" OR kind == "signed-mementos",
    //              call_edge_stream_present(resp) = true.
    //
    // Encoded as a post-condition: the call_edge_stream_present ctor
    // discharges vacuously when the response carries no lifted contracts
    // (empty compilation unit) and fires when a non-empty contract set is
    // emitted with no accompanying call-edge stream.
    contract(
        "lift_emits_call_edge_stream",
        ContractArgs {
            post: Some(eq(
                ctor1("call_edge_stream_present_or_unit_empty", str_const("resp")),
                ctor1("true_const", str_const("")),
            )),
            ..Default::default()
        },
    );
}

// ---------------------------------------------------------------------------
// Layer 2: operational verifiers  per-contract `verify_*`
// ---------------------------------------------------------------------------

/// PROTOCOL_VERSION_MISMATCH is the named error code in the spec error
/// table (line 179). The numeric code is 1001 but the spec also uses
/// the symbolic name; both are accepted.
pub const PROTOCOL_VERSION_MISMATCH_NAME: &str = "PROTOCOL_VERSION_MISMATCH";
pub const PROTOCOL_VERSION_MISMATCH_CODE: i64 = 1001;

/// The canonical EMIT value for the `protocol_version` field under
/// `pep/1.7.0` (Plugin Extension Protocol, `2026-05-12-plugin-protocol.md`
/// §0.4). Producers MUST emit this token; consumers accept this token
/// PLUS the legacy tokens in `LEGACY_ACCEPTED_PROTOCOL_VERSIONS` for the
/// migration minor version.
pub const PROTOCOL_VERSION_LITERAL: &str = "pep/1.7.0";

/// Legacy protocol-version tokens accepted on READ during the migration
/// minor version per `2026-05-12-plugin-protocol.md` §0.4. The NEXT
/// minor version of the runtime MUST drop these and accept only
/// `PROTOCOL_VERSION_LITERAL`.
pub const LEGACY_ACCEPTED_PROTOCOL_VERSIONS: &[&str] = &["provekit-lift/1"];

/// The full accept-set under the current migration minor version: the
/// canonical token plus the legacy tokens.
pub fn is_accepted_protocol_version(v: &str) -> bool {
    v == PROTOCOL_VERSION_LITERAL || LEGACY_ACCEPTED_PROTOCOL_VERSIONS.contains(&v)
}

/// Allowed proof-producing `lift` response `kind` values when
/// `options.layer == "all"`.
pub const ALL_LAYER_RESPONSE_KINDS: &[&str] = &["ir-document", "signed-mementos", "proof-envelope"];

/// Allowed side-effect-free identity `lift` response `kind` values when
/// `options.layer == "identify-only"`.
pub const IDENTIFY_ONLY_RESPONSE_KINDS: &[&str] =
    &["identity-document", "package-inspection-document"];

/// Full response-kind vocabulary across every layer.
pub const ALLOWED_LIFT_RESPONSE_KINDS: &[&str] = &[
    "ir-document",
    "signed-mementos",
    "proof-envelope",
    "identity-document",
    "package-inspection-document",
];

// --- C1 ---------------------------------------------------------------------

/// Verify C1: the initialize request carries the canonical protocol
/// version and the response either confirms the version or returns the
/// PROTOCOL_VERSION_MISMATCH error.
///
/// `request` is the JSON-RPC request `params` object; `response` is the
/// full JSON-RPC response (either `{"result": ...}` or `{"error": {...}}`).
pub fn verify_c1_initialize_protocol_version_match(
    request: &JsonValue,
    response: &JsonValue,
) -> Result<(), String> {
    // Pre: request must declare the canonical version.
    let req_v = request
        .get("protocol_version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "request.protocol_version missing or not a string".to_string())?;
    if !is_accepted_protocol_version(req_v) {
        return Err(format!(
            "C1 precondition: request.protocol_version is `{}`, expected one of {{`{}`}} \
             (canonical) or {:?} (legacy, accepted during migration window per \
             2026-05-12-plugin-protocol.md §0.4)",
            req_v, PROTOCOL_VERSION_LITERAL, LEGACY_ACCEPTED_PROTOCOL_VERSIONS
        ));
    }

    // Post: response either has a result whose protocol_version matches,
    // OR an error with code/name == PROTOCOL_VERSION_MISMATCH.
    if let Some(result) = response.get("result") {
        let resp_v = result
            .get("protocol_version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                "C1: response.result.protocol_version missing or not a string".to_string()
            })?;
        if resp_v != req_v {
            return Err(format!(
                "C1 postcondition: response.protocol_version `{}` != request `{}`, \
                 and response is not PROTOCOL_VERSION_MISMATCH error",
                resp_v, req_v
            ));
        }
        return Ok(());
    }
    if let Some(error) = response.get("error") {
        let code = error.get("code").and_then(|v| v.as_i64());
        let name = error
            .get("data")
            .and_then(|d| d.get("name"))
            .and_then(|v| v.as_str())
            .or_else(|| error.get("message").and_then(|v| v.as_str()));
        let is_mismatch = code == Some(PROTOCOL_VERSION_MISMATCH_CODE)
            || name == Some(PROTOCOL_VERSION_MISMATCH_NAME)
            || name
                .map(|s| s.contains(PROTOCOL_VERSION_MISMATCH_NAME))
                .unwrap_or(false);
        if !is_mismatch {
            return Err(format!(
                "C1 postcondition: response.error neither matches code {} nor name `{}`; \
                 got code={:?} name={:?}",
                PROTOCOL_VERSION_MISMATCH_CODE, PROTOCOL_VERSION_MISMATCH_NAME, code, name,
            ));
        }
        return Ok(());
    }
    Err("C1: response has neither `result` nor `error` field".to_string())
}

// --- C2 ---------------------------------------------------------------------

/// Verify C2: the initialize response declares a non-empty
/// authoring_surfaces array of strings AND an ir_version string starting
/// with "v".
///
/// `response` is the JSON-RPC response object; capabilities are read from
/// `response.result.capabilities` per spec lines 76-86.
pub fn verify_c2_initialize_capabilities_populated(response: &JsonValue) -> Result<(), String> {
    let result = response
        .get("result")
        .ok_or_else(|| "C2: response has no `result` field".to_string())?;
    let caps = result
        .get("capabilities")
        .ok_or_else(|| "C2: response.result.capabilities missing".to_string())?;

    let surfaces = caps
        .get("authoring_surfaces")
        .ok_or_else(|| "C2: capabilities.authoring_surfaces missing".to_string())?;
    let arr = surfaces.as_array().ok_or_else(|| {
        format!(
            "C2: capabilities.authoring_surfaces must be an array, got {}",
            type_label(surfaces)
        )
    })?;
    if arr.is_empty() {
        return Err("C2: capabilities.authoring_surfaces is empty".to_string());
    }
    for (i, item) in arr.iter().enumerate() {
        if !item.is_string() {
            return Err(format!(
                "C2: capabilities.authoring_surfaces[{}] is {}, expected string",
                i,
                type_label(item)
            ));
        }
    }

    let ir_v = caps
        .get("ir_version")
        .ok_or_else(|| "C2: capabilities.ir_version missing".to_string())?;
    let ir_s = ir_v.as_str().ok_or_else(|| {
        format!(
            "C2: capabilities.ir_version must be a string, got {}",
            type_label(ir_v)
        )
    })?;
    if !ir_s.starts_with('v') {
        return Err(format!(
            "C2: capabilities.ir_version `{}` does not start with `v`",
            ir_s
        ));
    }
    Ok(())
}

// --- C3 ---------------------------------------------------------------------

/// Verify C3: the lift request is well-formed.
/// `request` is the JSON-RPC request `params` object.
pub fn verify_c3_lift_request_well_formed(request: &JsonValue) -> Result<(), String> {
    let surface = request
        .get("surface")
        .ok_or_else(|| "C3: request.surface missing".to_string())?;
    if !surface.is_string() {
        return Err(format!(
            "C3: request.surface must be a string, got {}",
            type_label(surface)
        ));
    }

    let paths = request
        .get("source_paths")
        .ok_or_else(|| "C3: request.source_paths missing".to_string())?;
    let arr = paths.as_array().ok_or_else(|| {
        format!(
            "C3: request.source_paths must be an array, got {}",
            type_label(paths)
        )
    })?;
    if arr.is_empty() {
        return Err("C3: request.source_paths is empty".to_string());
    }
    for (i, item) in arr.iter().enumerate() {
        let s = item.as_str().ok_or_else(|| {
            format!(
                "C3: request.source_paths[{}] is {}, expected string",
                i,
                type_label(item)
            )
        })?;
        if s.is_empty() {
            return Err(format!("C3: request.source_paths[{}] is empty string", i));
        }
    }
    request_layer(request, "C3")?;
    Ok(())
}

// --- C4 ---------------------------------------------------------------------

/// Verify C4: the lift request's `surface` field is a member of the
/// initialize-time `capabilities.authoring_surfaces` set.
///
/// `request` is the lift request `params`; `init_response` is the
/// initialize response (we read capabilities from `result.capabilities`).
pub fn verify_c4_surface_in_capabilities(
    request: &JsonValue,
    init_response: &JsonValue,
) -> Result<(), String> {
    let surface = request
        .get("surface")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "C4: request.surface missing or not a string".to_string())?;

    let surfaces = init_response
        .get("result")
        .and_then(|r| r.get("capabilities"))
        .and_then(|c| c.get("authoring_surfaces"))
        .and_then(|s| s.as_array())
        .ok_or_else(|| {
            "C4: init_response.result.capabilities.authoring_surfaces missing or not array"
                .to_string()
        })?;

    let found = surfaces.iter().any(|v| v.as_str() == Some(surface));
    if !found {
        let listed: Vec<String> = surfaces
            .iter()
            .map(|v| match v.as_str() {
                Some(s) => format!("`{s}`"),
                None => format!("{}", v),
            })
            .collect();
        return Err(format!(
            "C4: surface `{}` not in capabilities.authoring_surfaces [{}]",
            surface,
            listed.join(", ")
        ));
    }
    Ok(())
}

// --- C5 ---------------------------------------------------------------------

/// Verify C5: the lift response's `kind` field matches the requested
/// `options.layer`.
pub fn verify_c5_response_kind_matches_layer(
    request: &JsonValue,
    response: &JsonValue,
) -> Result<(), String> {
    let layer = request_layer(request, "C5")?;
    let result = response
        .get("result")
        .ok_or_else(|| "C5: response has no `result` field".to_string())?;
    let kind = result
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "C5: response.result.kind missing or not a string".to_string())?;
    let allowed = match layer {
        "all" => ALL_LAYER_RESPONSE_KINDS,
        "identify-only" => IDENTIFY_ONLY_RESPONSE_KINDS,
        _ => unreachable!("request_layer only returns known layer values"),
    };
    if !allowed.contains(&kind) {
        return Err(format!(
            "C5: options.layer `{}` response.result.kind `{}` not in {{{}}}",
            layer,
            kind,
            allowed.join(", ")
        ));
    }
    Ok(())
}

// --- C6 ---------------------------------------------------------------------

/// Verify C6: when the lift response's `kind` is `"ir-document"`, the
/// `ir` field is an array (of IR-JSON declarations conforming to
/// `2026-04-30-ir-formal-grammar.md`).
///
/// Vacuously holds when kind is not `"ir-document"`.
pub fn verify_c6_ir_document_array(response: &JsonValue) -> Result<(), String> {
    let result = response
        .get("result")
        .ok_or_else(|| "C6: response has no `result` field".to_string())?;
    let kind = result.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    if kind != "ir-document" {
        return Ok(());
    }
    let ir = result
        .get("ir")
        .ok_or_else(|| "C6: kind=ir-document but `ir` field missing".to_string())?;
    if !ir.is_array() {
        return Err(format!(
            "C6: kind=ir-document but `ir` is {}, expected array",
            type_label(ir)
        ));
    }
    Ok(())
}

// --- C7 ---------------------------------------------------------------------

/// Verify C7: the `diagnostics` field, when present, is an array. (Per
/// spec line 208 the field SHOULD be present whenever there are
/// warnings; this verifier does not enforce presence, only that the
/// shape is correct when the field appears.)
pub fn verify_c7_diagnostics_field_is_array(response: &JsonValue) -> Result<(), String> {
    let result = match response.get("result") {
        Some(r) => r,
        None => return Ok(()), // error responses don't carry diagnostics
    };
    let diags = match result.get("diagnostics") {
        Some(d) => d,
        None => return Ok(()),
    };
    if !diags.is_array() {
        return Err(format!(
            "C7: response.result.diagnostics is {}, expected array",
            type_label(diags)
        ));
    }
    Ok(())
}

// --- C8 ---------------------------------------------------------------------

/// Verify C8: the lift response includes a call-edge stream when the contract
/// set is non-empty. Conforms to spec #114 §1 R1.
///
/// This verifier checks the `signed-mementos` and `proof-envelope` shapes,
/// where the call-edge stream appears as `call_edges` (an array). For the
/// `ir-document` shape, the verifier is vacuous (IR documents are an
/// intermediate representation; call edges travel in a separate pass).
///
/// The verifier is vacuously OK when the contract set is empty (no contracts
/// lifted → no call edges required).
pub fn verify_c8_call_edge_stream_present(response: &JsonValue) -> Result<(), String> {
    let result = match response.get("result") {
        Some(r) => r,
        None => return Ok(()), // error responses don't carry call edges
    };
    let kind = result.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    // Only check the signed-mementos and proof-envelope shapes; ir-document
    // is vacuous (call edges are not part of the IR document shape).
    if kind != "signed-mementos" && kind != "proof-envelope" {
        return Ok(());
    }
    // If there are no contract members, call edges are vacuously satisfied.
    let members_empty = result
        .get("members")
        .and_then(|m| m.as_object())
        .map(|m| m.is_empty())
        .unwrap_or(true);
    if members_empty {
        return Ok(());
    }
    // Non-empty contract set: call_edges must be present and an array.
    let call_edges = match result.get("call_edges") {
        Some(v) => v,
        None => {
            return Err(
                "C8: response has non-empty contract set but no `call_edges` field".to_string(),
            )
        }
    };
    if !call_edges.is_array() {
        return Err(format!(
            "C8: response.result.call_edges must be an array, got {}",
            type_label(call_edges)
        ));
    }
    Ok(())
}

// --- helpers ---------------------------------------------------------------

fn type_label(v: &JsonValue) -> &'static str {
    match v {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "bool",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
}

fn request_layer<'a>(request: &'a JsonValue, contract: &str) -> Result<&'a str, String> {
    let options = request
        .get("options")
        .ok_or_else(|| format!("{contract}: request.options missing"))?;
    let layer = options
        .get("layer")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("{contract}: request.options.layer missing or not a string"))?;
    match layer {
        "all" | "identify-only" => {}
        other => {
            return Err(format!(
                "{contract}: request.options.layer `{other}` is not one of {{all, identify-only}}"
            ))
        }
    }
    if let Some(identify_only) = options.get("identifyOnly") {
        let Some(flag) = identify_only.as_bool() else {
            return Err(format!(
                "{contract}: request.options.identifyOnly is {}, expected bool",
                type_label(identify_only)
            ));
        };
        let expected = layer == "identify-only";
        if flag != expected {
            return Err(format!(
                "{contract}: request.options.identifyOnly={flag} disagrees with layer `{layer}`"
            ));
        }
    }
    Ok(layer)
}

// ---------------------------------------------------------------------------
// Tests  per-contract positive + negative coverage
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- shared sample messages -------------------------------------------

    fn good_init_request() -> JsonValue {
        json!({
            "client": {"name": "provekit-cli", "version": "v1.1.0"},
            "protocol_version": "pep/1.7.0",
            "workspace_root": "/abs/path/to/workspace",
            "config_path": ".provekit/config.toml",
        })
    }

    fn good_init_response() -> JsonValue {
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "name": "rust-self-contracts",
                "version": "1.0.0",
                "protocol_version": "pep/1.7.0",
                "capabilities": {
                    "authoring_surfaces": ["rust-self-contracts", "kani"],
                    "ir_version": "v1.1.0",
                    "emits_signed_mementos": true,
                },
            },
        })
    }

    fn good_lift_request() -> JsonValue {
        json!({
            "surface": "rust-self-contracts",
            "source_paths": ["implementations/rust/provekit-canonicalizer/src/jcs.rs"],
            "options": {"layer": "all"},
        })
    }

    fn good_identify_only_lift_request() -> JsonValue {
        json!({
            "surface": "supply-chain-npm",
            "source_paths": ["."],
            "options": {"layer": "identify-only", "identifyOnly": true},
        })
    }

    fn good_lift_response_ir_document() -> JsonValue {
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "kind": "ir-document",
                "ir": [
                    {"kind": "contract", "name": "encode_jcs_is_deterministic", "outBinding": "out"},
                ],
                "diagnostics": [],
            },
        })
    }

    fn good_lift_response_package_inspection() -> JsonValue {
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "kind": "package-inspection-document",
                "ecosystem": "npm",
                "package": {"name": "safe-json", "version": "1.4.2"},
                "artifact": {
                    "path": "package.tgz",
                    "binaryCid": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "bytes": 64
                },
                "diagnostics": [],
            },
        })
    }

    // --- C1 ---------------------------------------------------------------

    #[test]
    fn c1_holds_on_matching_protocol_versions() {
        let req = good_init_request();
        let resp = good_init_response();
        verify_c1_initialize_protocol_version_match(&req, &resp).unwrap();
    }

    #[test]
    fn c1_holds_on_protocol_version_mismatch_error() {
        let req = good_init_request();
        let resp = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": PROTOCOL_VERSION_MISMATCH_CODE,
                "message": "PROTOCOL_VERSION_MISMATCH: client wants provekit-lift/1, plugin has provekit-lift/2",
            },
        });
        verify_c1_initialize_protocol_version_match(&req, &resp).unwrap();
    }

    #[test]
    fn c1_holds_on_legacy_protocol_version_in_migration_window() {
        // The migration minor version accepts both `provekit-lift/1` (legacy)
        // and `pep/1.7.0` (canonical). A request bearing the legacy token
        // paired with a response confirming the same legacy token MUST
        // satisfy C1; the deprecation notice is a sibling memento per
        // 2026-05-12-plugin-protocol.md §0.4, not a verifier-level reject.
        let req = json!({
            "client": {"name": "provekit-cli", "version": "v1.1.0"},
            "protocol_version": "provekit-lift/1",
            "workspace_root": "/abs/path/to/workspace",
            "config_path": ".provekit/config.toml",
        });
        let resp = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "name": "rust-self-contracts",
                "version": "1.0.0",
                "protocol_version": "provekit-lift/1",
                "capabilities": {
                    "authoring_surfaces": ["rust-self-contracts", "kani"],
                    "ir_version": "v1.1.0",
                    "emits_signed_mementos": true,
                },
            },
        });
        verify_c1_initialize_protocol_version_match(&req, &resp).unwrap();
    }

    #[test]
    fn c1_violation_drift_without_error_is_caught() {
        let req = good_init_request();
        // Plugin returns a different protocol version with no error code.
        let resp = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "name": "rogue-plugin",
                "version": "0.0.1",
                "protocol_version": "provekit-lift/2",
                "capabilities": {
                    "authoring_surfaces": ["rogue-surface"],
                    "ir_version": "v1.1.0",
                },
            },
        });
        let err = verify_c1_initialize_protocol_version_match(&req, &resp)
            .expect_err("C1 should fire on silent version drift");
        assert!(
            err.contains("postcondition"),
            "expected postcondition violation, got: {err}"
        );
    }

    #[test]
    fn c1_violation_request_wrong_protocol_is_caught() {
        let req = json!({
            "client": {"name": "x", "version": "v0"},
            "protocol_version": "provekit-lift/0",
            "workspace_root": "/x",
            "config_path": "y",
        });
        let resp = good_init_response();
        let err = verify_c1_initialize_protocol_version_match(&req, &resp)
            .expect_err("C1 precondition should fire on bad request version");
        assert!(
            err.contains("precondition"),
            "expected precondition violation, got: {err}"
        );
    }

    // --- C2 ---------------------------------------------------------------

    #[test]
    fn c2_holds_on_real_init_response() {
        verify_c2_initialize_capabilities_populated(&good_init_response()).unwrap();
    }

    #[test]
    fn c2_violation_empty_authoring_surfaces_is_caught() {
        let resp = json!({
            "result": {
                "capabilities": {
                    "authoring_surfaces": [],
                    "ir_version": "v1.1.0",
                },
            },
        });
        let err = verify_c2_initialize_capabilities_populated(&resp)
            .expect_err("C2 should fire on empty surfaces");
        assert!(err.contains("empty"), "got: {err}");
    }

    #[test]
    fn c2_violation_non_string_surface_is_caught() {
        let resp = json!({
            "result": {
                "capabilities": {
                    "authoring_surfaces": ["ok", 42],
                    "ir_version": "v1.1.0",
                },
            },
        });
        let err = verify_c2_initialize_capabilities_populated(&resp)
            .expect_err("C2 should fire on non-string surface");
        assert!(err.contains("expected string"), "got: {err}");
    }

    #[test]
    fn c2_violation_ir_version_no_v_prefix_is_caught() {
        let resp = json!({
            "result": {
                "capabilities": {
                    "authoring_surfaces": ["x"],
                    "ir_version": "1.1.0",
                },
            },
        });
        let err = verify_c2_initialize_capabilities_populated(&resp)
            .expect_err("C2 should fire on missing v prefix");
        assert!(err.contains("does not start with `v`"), "got: {err}");
    }

    // --- C3 ---------------------------------------------------------------

    #[test]
    fn c3_holds_on_real_lift_request() {
        verify_c3_lift_request_well_formed(&good_lift_request()).unwrap();
    }

    #[test]
    fn c3_holds_on_identify_only_request_with_legacy_mirror() {
        verify_c3_lift_request_well_formed(&good_identify_only_lift_request()).unwrap();
    }

    #[test]
    fn c3_violation_empty_source_paths_is_caught() {
        let req = json!({"surface": "x", "source_paths": []});
        let err = verify_c3_lift_request_well_formed(&req)
            .expect_err("C3 should fire on empty source_paths");
        assert!(err.contains("empty"), "got: {err}");
    }

    #[test]
    fn c3_violation_non_string_path_is_caught() {
        let req = json!({"surface": "x", "source_paths": ["a", 5]});
        let err = verify_c3_lift_request_well_formed(&req)
            .expect_err("C3 should fire on non-string path");
        assert!(err.contains("expected string"), "got: {err}");
    }

    #[test]
    fn c3_violation_empty_string_path_is_caught() {
        let req = json!({"surface": "x", "source_paths": ["a", ""]});
        let err = verify_c3_lift_request_well_formed(&req)
            .expect_err("C3 should fire on empty string path");
        assert!(err.contains("empty string"), "got: {err}");
    }

    #[test]
    fn c3_violation_surface_missing_is_caught() {
        let req = json!({"source_paths": ["a"]});
        let err = verify_c3_lift_request_well_formed(&req)
            .expect_err("C3 should fire on missing surface");
        assert!(err.contains("surface"), "got: {err}");
    }

    #[test]
    fn c3_violation_identify_only_mirror_drift_is_caught() {
        let req = json!({
            "surface": "supply-chain-npm",
            "source_paths": ["."],
            "options": {"layer": "identify-only", "identifyOnly": false},
        });
        let err = verify_c3_lift_request_well_formed(&req)
            .expect_err("C3 should fire when identifyOnly disagrees with layer");
        assert!(err.contains("identifyOnly"), "got: {err}");
    }

    // --- C4 ---------------------------------------------------------------

    #[test]
    fn c4_holds_when_surface_listed() {
        verify_c4_surface_in_capabilities(&good_lift_request(), &good_init_response()).unwrap();
    }

    #[test]
    fn c4_violation_surface_not_in_caps_is_caught() {
        let req = json!({"surface": "go-self-contracts", "source_paths": ["a"]});
        let err = verify_c4_surface_in_capabilities(&req, &good_init_response())
            .expect_err("C4 should fire when surface absent");
        assert!(err.contains("not in capabilities"), "got: {err}");
    }

    // --- C5 ---------------------------------------------------------------

    #[test]
    fn c5_holds_for_ir_document_kind() {
        verify_c5_response_kind_matches_layer(
            &good_lift_request(),
            &good_lift_response_ir_document(),
        )
        .unwrap();
    }

    #[test]
    fn c5_holds_for_signed_mementos_kind() {
        let resp = json!({
            "result": {
                "kind": "signed-mementos",
                "members": {},
                "signer_cid": "blake3-512:00",
            },
        });
        verify_c5_response_kind_matches_layer(&good_lift_request(), &resp).unwrap();
    }

    #[test]
    fn c5_holds_for_proof_envelope_kind() {
        let resp = json!({
            "result": {
                "kind": "proof-envelope",
                "filename_cid": "blake3-512:00",
                "bytes_base64": "",
            },
        });
        verify_c5_response_kind_matches_layer(&good_lift_request(), &resp).unwrap();
    }

    #[test]
    fn c5_holds_for_package_inspection_kind_in_identify_only_layer() {
        verify_c5_response_kind_matches_layer(
            &good_identify_only_lift_request(),
            &good_lift_response_package_inspection(),
        )
        .unwrap();
    }

    #[test]
    fn c5_violation_ir_document_in_identify_only_layer_is_caught() {
        let err = verify_c5_response_kind_matches_layer(
            &good_identify_only_lift_request(),
            &good_lift_response_ir_document(),
        )
        .expect_err("C5 should fire when identify-only returns a proof-producing shape");
        assert!(err.contains("identify-only"), "got: {err}");
    }

    #[test]
    fn c5_violation_unknown_kind_is_caught() {
        let resp = json!({"result": {"kind": "raw-bytes"}});
        let err = verify_c5_response_kind_matches_layer(&good_lift_request(), &resp)
            .expect_err("C5 should fire on unknown kind");
        assert!(err.contains("not in"), "got: {err}");
    }

    // --- C6 ---------------------------------------------------------------

    #[test]
    fn c6_holds_when_ir_is_array() {
        verify_c6_ir_document_array(&good_lift_response_ir_document()).unwrap();
    }

    #[test]
    fn c6_vacuous_when_kind_is_proof_envelope() {
        let resp = json!({
            "result": {
                "kind": "proof-envelope",
                "filename_cid": "blake3-512:00",
                "bytes_base64": "",
            },
        });
        verify_c6_ir_document_array(&resp).unwrap();
    }

    #[test]
    fn c6_violation_ir_not_array_is_caught() {
        let resp = json!({
            "result": {
                "kind": "ir-document",
                "ir": "not-an-array",
            },
        });
        let err =
            verify_c6_ir_document_array(&resp).expect_err("C6 should fire when ir is not an array");
        assert!(err.contains("expected array"), "got: {err}");
    }

    #[test]
    fn c6_violation_ir_field_missing_is_caught() {
        let resp = json!({"result": {"kind": "ir-document"}});
        let err =
            verify_c6_ir_document_array(&resp).expect_err("C6 should fire when ir is missing");
        assert!(err.contains("missing"), "got: {err}");
    }

    // --- C7 ---------------------------------------------------------------

    #[test]
    fn c7_holds_when_diagnostics_array() {
        verify_c7_diagnostics_field_is_array(&good_lift_response_ir_document()).unwrap();
    }

    #[test]
    fn c7_holds_when_diagnostics_absent() {
        let resp = json!({"result": {"kind": "ir-document", "ir": []}});
        verify_c7_diagnostics_field_is_array(&resp).unwrap();
    }

    #[test]
    fn c7_violation_diagnostics_not_array_is_caught() {
        let resp = json!({
            "result": {
                "kind": "ir-document",
                "ir": [],
                "diagnostics": "uh oh",
            },
        });
        let err = verify_c7_diagnostics_field_is_array(&resp)
            .expect_err("C7 should fire when diagnostics is not an array");
        assert!(err.contains("expected array"), "got: {err}");
    }

    // --- collector sanity: invariants() pushes the expected number. ------

    #[test]
    fn invariants_authors_expected_contract_count() {
        use provekit_ir_symbolic::{begin_collecting, finish, reset_collector};
        reset_collector();
        begin_collecting();
        invariants();
        let decls = finish();
        // C1, C2 (two facets), C3 (four facets), C4, C5, C6, C7, C8
        // = 1 + 2 + 4 + 1 + 1 + 1 + 1 + 1 = 12 ContractDecls.
        assert_eq!(
            decls.len(),
            12,
            "lift_plugin_protocol::invariants should author 12 contracts; got {}",
            decls.len()
        );
        // Spot-check a few names.
        let names: Vec<&str> = decls.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"lift_plugin_initialize_protocol_version_match"));
        assert!(names.contains(&"lift_plugin_lift_response_kind_matches_layer"));
        assert!(names.contains(&"lift_plugin_lift_response_ir_document_array"));
        assert!(names.contains(&"lift_plugin_diagnostic_field_is_array"));
        assert!(
            names.contains(&"lift_emits_call_edge_stream"),
            "expected lift_emits_call_edge_stream in invariants; got {names:?}"
        );
    }

    // --- C8 ---------------------------------------------------------------

    #[test]
    fn c8_holds_when_call_edges_present_with_contracts() {
        let resp = json!({
            "result": {
                "kind": "signed-mementos",
                "members": {"blake3-512:aaa": "..."},
                "call_edges": [
                    {
                        "schemaVersion": "1",
                        "kind": "call-edge",
                        "sourceContractCid": "blake3-512:bbb",
                        "targetContractCid": "blake3-512:aaa",
                        "callSiteLocus": {"file": "foo.rs", "line": null, "col": null},
                        "targetSymbol": "a",
                        "evidenceTerm": {"kind": "obligation", "source": "blake3-512:bbb", "target": "blake3-512:aaa"},
                    }
                ],
            },
        });
        verify_c8_call_edge_stream_present(&resp).unwrap();
    }

    #[test]
    fn c8_holds_vacuously_when_no_contract_members() {
        let resp = json!({
            "result": {
                "kind": "signed-mementos",
                "members": {},
            },
        });
        verify_c8_call_edge_stream_present(&resp).unwrap();
    }

    #[test]
    fn c8_holds_vacuously_for_ir_document_kind() {
        let resp = json!({
            "result": {
                "kind": "ir-document",
                "ir": [{"kind": "contract", "name": "foo", "outBinding": "out"}],
            },
        });
        verify_c8_call_edge_stream_present(&resp).unwrap();
    }

    #[test]
    fn c8_violation_missing_call_edges_field_is_caught() {
        let resp = json!({
            "result": {
                "kind": "signed-mementos",
                "members": {"blake3-512:aaa": "..."},
                // call_edges field is absent
            },
        });
        let err = verify_c8_call_edge_stream_present(&resp)
            .expect_err("C8 should fire when call_edges absent and contracts non-empty");
        assert!(
            err.contains("call_edges"),
            "expected call_edges mention; got: {err}"
        );
    }

    #[test]
    fn c8_violation_call_edges_not_array_is_caught() {
        let resp = json!({
            "result": {
                "kind": "signed-mementos",
                "members": {"blake3-512:aaa": "..."},
                "call_edges": "not-an-array",
            },
        });
        let err = verify_c8_call_edge_stream_present(&resp)
            .expect_err("C8 should fire when call_edges is not an array");
        assert!(err.contains("array"), "expected array mention; got: {err}");
    }
}
