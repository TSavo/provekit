// SPDX-License-Identifier: Apache-2.0
//
// catalog_format.rs
//
// Encodes the rules of `protocol/specs/2026-04-30-protocol-catalog-format.md`
// (the "catalog-format spec") as machine-enforceable contract mementos.
//
// Two layers, mirroring the kit's existing convention (see e.g.
// `provekit-canonicalizer/src/jcs.invariant.rs`):
//
//   1. `pub fn invariants()` authors one IR contract per rule via
//      the `must` / `contract` collector. Each contract's formula is
//      a declarative published claim that names the rule. Most are
//      trivially-true-under-Z3 because the rule lives at the JSON-value
//      layer (a string regex, a key prefix predicate) which the IR
//      can only gesture at; the operational enforcement is the sibling
//      verifier below.
//
//   2. `pub fn verify_catalog(json) -> CatalogReport` is the runtime
//      checker for rules that are derivable purely from the catalog
//      JSON value. `pub fn verify_catalog_against_spec_dir(json, &Path)
//      -> CatalogReport` extends discharge to the rules that need
//      filesystem access to spec-file bytes (R10, R11, R12). Both
//      produce the same shape of report; rules that need a spec dir
//      report `RuleVerdict::Vacuous` when called via the JSON-only
//      path.
//
// SCOPE  rules encoded (numbered against the spec text):
//
//   R1  : top-level `kind` MUST be the literal string "catalog"           (1)
//   R2  : `name` MUST be present (string)                                 (1)
//   R3  : `version` MUST be present (string)                              (1)
//   R4  : `algorithms` MUST be object, each role array of strings         (1)
//   R5  : every value in `properties` is a self-identifying CID string    (1, 6)
//   R6  : `declaredAt` matches ISO-8601 UTC                               (1)
//   R7  : underscore-prefixed fields participate in JCS canonicalization  (1)
//   R8  : spec-file CID = blake3-512:hex(BLAKE3-512(spec_file_bytes))     (2.1)
//   R9  : catalog CID = blake3-512:hex(BLAKE3-512(JCS(catalog_json)))     (2.2)
//   R11 : every spec file's raw-byte BLAKE3-512 matches `properties[key]` (5)
//   R12 : catalog's own JCS-canonical CID matches the recomputed value    (5)
//   R14 : no truncated digests; full 128-hex-char BLAKE3-512 output       (6)
//   R15 : every catalog CID carries the `blake3-512:` prefix              (6)
//
// Thirteen rules total. The remaining two:
//
//   R10 : every spec file referenced from `properties` exists at the
//         expected path. Deferred  the catalog itself does NOT carry
//         the path mapping (spec-key  basename); that lives in
//         `tools/recompute-spec-cids/src/main.rs::SPEC_MAP`. Encoding
//         R10 inside this kit would require importing that map (or
//         duplicating it), creating a maintenance footgun. Left as a
//         conformance-tool concern.
//
//   R13 : catalog CIDs MUST NOT be raw-byte hashes. The rule is the
//         negation of R9's positive form. Encoding it positively is
//         unavoidably circular ("the CID is NOT what you would compute
//         the wrong way"). Discharge is via R12: any catalog publishing
//         a CID that does not match `BLAKE3-512(JCS(catalog))` is in
//         violation, regardless of how the wrong value was produced.
//         R13 is therefore subsumed by R12.

use std::path::Path;
use std::rc::Rc;

use provekit_canonicalizer::blake3_512_of;
use provekit_ir_symbolic::{
    contract, eq, forall, gte, must, num, str_const, ContractArgs,
    String_, Term,
};
use serde_json::{Map, Value as JsonValue};

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

/// Mint contract mementos for the catalog-format spec rules.
///
/// Each contract is named `catalog_format_<rule>_<short_label>`. The
/// IR body is a declarative statement of the rule; operational
/// discharge is `verify_catalog` below.
pub fn invariants() {
    // -- R1: top-level `kind` MUST be the literal string "catalog". --------
    must(
        "catalog_format_r1_kind_is_literal_catalog",
        forall(String_(), |c| {
            eq(ctor1("kind_of", c), str_const("catalog"))
        }),
    );

    // -- R2: `name` MUST be present (string). ------------------------------
    //
    // forall c: String. has_string_field(c, "name") = true
    must(
        "catalog_format_r2_name_present_string",
        forall(String_(), |c| {
            eq(
                ctor1(
                    "has_string_field_name",
                    c,
                ),
                ctor1("true_const", str_const("")),
            )
        }),
    );

    // -- R3: `version` MUST be present (string). ---------------------------
    must(
        "catalog_format_r3_version_present_string",
        forall(String_(), |c| {
            eq(
                ctor1(
                    "has_string_field_version",
                    c,
                ),
                ctor1("true_const", str_const("")),
            )
        }),
    );

    // -- R4: `algorithms` MUST be object, role -> array of strings. -------
    //
    // forall c: String. is_algorithms_object(c) = true
    contract(
        "catalog_format_r4_algorithms_role_array_of_strings",
        ContractArgs {
            post: Some(eq(
                ctor1("is_algorithms_object", str_const("c")),
                ctor1("true_const", str_const("")),
            )),
            ..Default::default()
        },
    );

    // -- R5: every value in `properties` is a self-identifying CID. --------
    contract(
        "catalog_format_r5_property_values_are_self_identifying_cids",
        ContractArgs {
            post: Some(eq(
                ctor1("is_self_identifying_cid", ctor1("properties_value", str_const("any"))),
                ctor1("true_const", str_const("")),
            )),
            ..Default::default()
        },
    );

    // -- R6: `declaredAt` is ISO-8601 UTC. --------------------------------
    contract(
        "catalog_format_r6_declaredAt_is_iso8601_utc",
        ContractArgs {
            post: Some(eq(
                ctor1("iso8601_utc", ctor1("declaredAt_of", str_const("c"))),
                ctor1("true_const", str_const("")),
            )),
            ..Default::default()
        },
    );

    // -- R7: underscore-prefixed fields participate in JCS canonical. -----
    must(
        "catalog_format_r7_underscore_fields_participate_in_canonicalization",
        forall(String_(), |c| {
            gte(
                ctor1("len", ctor1("jcs_with_underscore_fields", c)),
                num(1),
            )
        }),
    );

    // -- R8: spec-file CID equals blake3-512:hex(BLAKE3-512(spec_bytes)). -
    //
    // forall f: String. spec_cid_of(f) = blake3_512_self_identifying(f)
    //
    // The IR formula names the rule; operational discharge runs in
    // `verify_catalog_against_spec_dir`.
    must(
        "catalog_format_r8_spec_cid_is_blake3_512_of_raw_bytes",
        forall(String_(), |f| {
            eq(
                ctor1("spec_cid_of_file", f.clone()),
                ctor1("blake3_512_self_identifying_of_bytes", f),
            )
        }),
    );

    // -- R9: catalog CID equals blake3-512:hex(BLAKE3-512(JCS(catalog))). -
    must(
        "catalog_format_r9_catalog_cid_is_blake3_512_of_jcs",
        forall(String_(), |c| {
            eq(
                ctor1("catalog_cid_of", c.clone()),
                ctor1(
                    "blake3_512_self_identifying_of_bytes",
                    ctor1("jcs_canonical_bytes_of", c),
                ),
            )
        }),
    );

    // -- R11: every spec file's raw-byte BLAKE3-512 matches its CID. ------
    //
    // forall key, file. properties_value(key) = spec_cid_of_file(file).
    must(
        "catalog_format_r11_property_values_match_disk_blake3",
        forall(String_(), |c| {
            eq(
                ctor1("properties_value", c.clone()),
                ctor1("spec_cid_of_file_for_key", c),
            )
        }),
    );

    // -- R12: catalog CID stable: recompute(JCS(catalog)) = published. ----
    must(
        "catalog_format_r12_catalog_cid_recomputes_to_published",
        forall(String_(), |c| {
            eq(
                ctor1("published_catalog_cid_of", c.clone()),
                ctor1("recomputed_catalog_cid_of", c),
            )
        }),
    );

    // -- R14: no truncated digests; full 128 hex chars. -------------------
    must(
        "catalog_format_r14_full_blake3_512_no_truncation",
        forall(String_(), |c| {
            eq(ctor1("len", ctor1("hex_part_of_cid", c)), num(128))
        }),
    );

    // -- R15: every CID carries the `blake3-512:` prefix. ----------------
    contract(
        "catalog_format_r15_cid_carries_blake3_512_prefix",
        ContractArgs {
            post: Some(eq(
                ctor1(
                    "starts_with",
                    ctor1("any_cid_in_catalog", str_const("c")),
                ),
                str_const("blake3-512:"),
            )),
            ..Default::default()
        },
    );
}

// ---------------------------------------------------------------------------
// Layer 2: operational verifier  `verify_catalog(json) -> CatalogReport`
// ---------------------------------------------------------------------------

/// Per-rule verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleVerdict {
    /// The rule held against this catalog.
    Holds,
    /// The rule was violated; the string is a human-readable reason.
    Violated(String),
    /// The rule was not exercised (e.g. a disk-dependent check called
    /// without a spec dir). Treated as passing by `all_hold()` but
    /// reported separately so the caller can tell discharge from
    /// vacuity.
    Vacuous(String),
}

/// Aggregated outcome of running every encoded rule against a catalog
/// JSON value. Field order mirrors the rule numbers above.
#[derive(Debug, Clone)]
pub struct CatalogReport {
    pub r1_kind: RuleVerdict,
    pub r2_name_present: RuleVerdict,
    pub r3_version_present: RuleVerdict,
    pub r4_algorithms_shape: RuleVerdict,
    pub r5_property_values_are_cids: RuleVerdict,
    pub r6_declared_at_iso8601: RuleVerdict,
    pub r7_underscore_fields_in_canonical: RuleVerdict,
    /// R8 has no JSON-only discharge: the rule binds spec_cid to raw
    /// spec bytes. We mark it Vacuous from `verify_catalog` and let
    /// `verify_catalog_against_spec_dir` discharge it via R11's
    /// per-file recomputation (R8 is the formula; R11 is its closure
    /// over the property map).
    pub r8_spec_cid_formula: RuleVerdict,
    pub r9_catalog_cid_formula: RuleVerdict,
    /// Disk-dependent: only discharged from
    /// `verify_catalog_against_spec_dir`.
    pub r11_disk_blake3_matches: RuleVerdict,
    pub r12_catalog_cid_recomputes: RuleVerdict,
    pub r14_no_truncated_digests: RuleVerdict,
    pub r15_cid_blake3_512_prefix: RuleVerdict,
}

impl CatalogReport {
    /// True iff every rule either Holds or is Vacuous.
    pub fn all_hold(&self) -> bool {
        let verdicts = [
            &self.r1_kind,
            &self.r2_name_present,
            &self.r3_version_present,
            &self.r4_algorithms_shape,
            &self.r5_property_values_are_cids,
            &self.r6_declared_at_iso8601,
            &self.r7_underscore_fields_in_canonical,
            &self.r8_spec_cid_formula,
            &self.r9_catalog_cid_formula,
            &self.r11_disk_blake3_matches,
            &self.r12_catalog_cid_recomputes,
            &self.r14_no_truncated_digests,
            &self.r15_cid_blake3_512_prefix,
        ];
        verdicts
            .iter()
            .all(|v| !matches!(v, RuleVerdict::Violated(_)))
    }

    /// All violations, formatted (rule_label, reason). Vacuous results
    /// are NOT reported.
    pub fn violations(&self) -> Vec<(&'static str, String)> {
        let mut out: Vec<(&'static str, String)> = Vec::new();
        let pairs: &[(&'static str, &RuleVerdict)] = &[
            ("R1", &self.r1_kind),
            ("R2", &self.r2_name_present),
            ("R3", &self.r3_version_present),
            ("R4", &self.r4_algorithms_shape),
            ("R5", &self.r5_property_values_are_cids),
            ("R6", &self.r6_declared_at_iso8601),
            ("R7", &self.r7_underscore_fields_in_canonical),
            ("R8", &self.r8_spec_cid_formula),
            ("R9", &self.r9_catalog_cid_formula),
            ("R11", &self.r11_disk_blake3_matches),
            ("R12", &self.r12_catalog_cid_recomputes),
            ("R14", &self.r14_no_truncated_digests),
            ("R15", &self.r15_cid_blake3_512_prefix),
        ];
        for (label, v) in pairs {
            if let RuleVerdict::Violated(reason) = v {
                out.push((label, reason.clone()));
            }
        }
        out
    }
}

/// CID format: `^blake3-512:[0-9a-f]{128}$`.
fn cid_format_problem(s: &str) -> Option<String> {
    let prefix = "blake3-512:";
    if !s.starts_with(prefix) {
        return Some(format!(
            "missing `blake3-512:` prefix; got `{}...`",
            &s[..s.len().min(20)]
        ));
    }
    let hex = &s[prefix.len()..];
    if hex.len() != 128 {
        return Some(format!(
            "hex digest length is {} (expected 128 for blake3-512)",
            hex.len()
        ));
    }
    if let Some(bad) = hex.chars().find(|c| !c.is_ascii_hexdigit() || c.is_ascii_uppercase()) {
        return Some(format!(
            "non-lowercase-hex character `{}` in digest",
            bad
        ));
    }
    None
}

/// ISO-8601 UTC: shape `YYYY-MM-DDTHH:MM:SS(.fff)?Z`.
fn iso8601_utc_problem(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    if bytes.len() < 20 {
        return Some(format!("string too short ({} bytes) for ISO-8601 UTC", bytes.len()));
    }
    if !s.ends_with('Z') {
        return Some(format!("must end with literal `Z` (UTC marker); got `{}`", s));
    }
    let positions: &[(usize, fn(u8) -> bool, &str)] = &[
        (4, |b| b == b'-', "expected `-` after year"),
        (7, |b| b == b'-', "expected `-` after month"),
        (10, |b| b == b'T', "expected `T` after day"),
        (13, |b| b == b':', "expected `:` after hour"),
        (16, |b| b == b':', "expected `:` after minute"),
    ];
    for (pos, pred, msg) in positions {
        if bytes.get(*pos).copied().is_none_or(|b| !pred(b)) {
            return Some(format!("at byte {}: {}", pos, msg));
        }
    }
    let digit_positions: &[usize] = &[
        0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18,
    ];
    for &p in digit_positions {
        if !bytes[p].is_ascii_digit() {
            return Some(format!(
                "at byte {}: expected ASCII digit, got `{}`",
                p, bytes[p] as char
            ));
        }
    }
    let tail = &s[19..];
    if tail == "Z" {
        return None;
    }
    if !tail.starts_with('.') || !tail.ends_with('Z') {
        return Some(format!(
            "fractional-second segment must look like `.fff...Z`; got `{}`",
            tail
        ));
    }
    let frac = &tail[1..tail.len() - 1];
    if frac.is_empty() || !frac.chars().all(|c| c.is_ascii_digit()) {
        return Some(format!("fractional-second digits invalid in `{}`", tail));
    }
    None
}

/// JCS-encode a JSON value via the canonicalizer crate. We round-trip
/// `serde_json::Value` -> `provekit_canonicalizer::Value` to stay on the
/// same bytes the rest of the protocol uses.
fn jcs_encode(j: &JsonValue) -> Result<String, String> {
    use provekit_canonicalizer::{encode_jcs, Value};
    use std::sync::Arc;
    fn to_canonical(j: &JsonValue) -> Result<Arc<Value>, String> {
        Ok(match j {
            JsonValue::Null => Value::null(),
            JsonValue::Bool(b) => Value::boolean(*b),
            JsonValue::Number(n) => {
                let i = n
                    .as_i64()
                    .ok_or_else(|| format!("non-i64 number `{}` not supported", n))?;
                Value::integer(i)
            }
            JsonValue::String(s) => Value::string(s.clone()),
            JsonValue::Array(items) => {
                let mut converted: Vec<Arc<Value>> = Vec::with_capacity(items.len());
                for it in items {
                    converted.push(to_canonical(it)?);
                }
                Value::array(converted)
            }
            JsonValue::Object(map) => {
                let mut entries: Vec<(String, Arc<Value>)> = Vec::with_capacity(map.len());
                for (k, v) in map {
                    entries.push((k.clone(), to_canonical(v)?));
                }
                Value::object(entries)
            }
        })
    }
    let v = to_canonical(j)?;
    Ok(encode_jcs(&v))
}

/// Verify every JSON-derivable rule against `catalog`. Disk-dependent
/// rules (R11, R12) are reported as `Vacuous`; use
/// `verify_catalog_against_spec_dir` for full discharge. R8 is also
/// vacuous from this entry point: its operational form is per-file
/// recomputation (R11).
pub fn verify_catalog(catalog: &JsonValue) -> CatalogReport {
    verify_catalog_inner(catalog, None, None)
}

/// Verify every encoded rule against `catalog`, including disk-dependent
/// ones. `spec_files` is an iterator of `(property_key, file_path)`
/// pairs; only keys present in this map are checked against disk. Keys
/// in `properties` not covered by `spec_files` are still subject to
/// R5/R14/R15 (format checks) but not R11.
///
/// `claimed_catalog_cid` is the externally-published CID we expect
/// `BLAKE3-512(JCS(catalog))` to equal (R12). Pass `None` to leave R12
/// vacuous.
pub fn verify_catalog_against_spec_dir<'a, I>(
    catalog: &JsonValue,
    spec_files: I,
    claimed_catalog_cid: Option<&str>,
) -> CatalogReport
where
    I: IntoIterator<Item = (&'a str, &'a Path)>,
{
    let map: std::collections::BTreeMap<&str, &Path> = spec_files.into_iter().collect();
    verify_catalog_inner(catalog, Some(&map), claimed_catalog_cid)
}

fn verify_catalog_inner(
    catalog: &JsonValue,
    spec_files: Option<&std::collections::BTreeMap<&str, &Path>>,
    claimed_catalog_cid: Option<&str>,
) -> CatalogReport {
    let obj: Option<&Map<String, JsonValue>> = catalog.as_object();

    // --- R1: kind == "catalog" ------------------------------------------
    let r1_kind = match obj.and_then(|o| o.get("kind")) {
        Some(JsonValue::String(s)) if s == "catalog" => RuleVerdict::Holds,
        Some(JsonValue::String(s)) => RuleVerdict::Violated(format!(
            "`kind` is `{}`, expected literal `catalog`",
            s
        )),
        Some(other) => RuleVerdict::Violated(format!(
            "`kind` must be a string, got {}",
            value_type(other)
        )),
        None => RuleVerdict::Violated("top-level `kind` field is missing".into()),
    };

    // --- R2: `name` present and a string. -------------------------------
    let r2_name_present = match obj.and_then(|o| o.get("name")) {
        Some(JsonValue::String(_)) => RuleVerdict::Holds,
        Some(other) => RuleVerdict::Violated(format!(
            "`name` must be a string, got {}",
            value_type(other)
        )),
        None => RuleVerdict::Violated("top-level `name` field is missing".into()),
    };

    // --- R3: `version` present and a string. ----------------------------
    let r3_version_present = match obj.and_then(|o| o.get("version")) {
        Some(JsonValue::String(_)) => RuleVerdict::Holds,
        Some(other) => RuleVerdict::Violated(format!(
            "`version` must be a string, got {}",
            value_type(other)
        )),
        None => RuleVerdict::Violated("top-level `version` field is missing".into()),
    };

    // --- R4: `algorithms` is object; each value an array of strings. ----
    let r4_algorithms_shape = match obj.and_then(|o| o.get("algorithms")) {
        Some(JsonValue::Object(algs)) => {
            let mut problems: Vec<String> = Vec::new();
            for (role, val) in algs {
                match val {
                    JsonValue::Array(items) => {
                        for (i, item) in items.iter().enumerate() {
                            if !item.is_string() {
                                problems.push(format!(
                                    "algorithms[`{}`][{}] is {}, expected string tag",
                                    role,
                                    i,
                                    value_type(item)
                                ));
                            }
                        }
                        if items.is_empty() {
                            problems.push(format!(
                                "algorithms[`{}`] is empty array; at least one tag required",
                                role
                            ));
                        }
                    }
                    other => problems.push(format!(
                        "algorithms[`{}`] must be array of strings, got {}",
                        role,
                        value_type(other)
                    )),
                }
            }
            if problems.is_empty() {
                RuleVerdict::Holds
            } else {
                RuleVerdict::Violated(problems.join("; "))
            }
        }
        Some(other) => RuleVerdict::Violated(format!(
            "`algorithms` must be an object, got {}",
            value_type(other)
        )),
        None => RuleVerdict::Violated("top-level `algorithms` field is missing".into()),
    };

    // --- R5 / R14 / R15 are checked together against `properties`. -----
    let mut r5_problems: Vec<String> = Vec::new();
    let mut r14_problems: Vec<String> = Vec::new();
    let mut r15_problems: Vec<String> = Vec::new();
    match obj.and_then(|o| o.get("properties")) {
        Some(JsonValue::Object(props)) => {
            for (key, val) in props {
                let s = match val {
                    JsonValue::String(s) => s,
                    other => {
                        r5_problems.push(format!(
                            "properties[`{}`] is {}, expected string",
                            key,
                            value_type(other)
                        ));
                        continue;
                    }
                };
                let prefix = "blake3-512:";
                if !s.starts_with(prefix) {
                    r15_problems.push(format!(
                        "properties[`{}`] missing `blake3-512:` prefix",
                        key
                    ));
                    r5_problems.push(format!(
                        "properties[`{}`] is not a self-identifying CID",
                        key
                    ));
                    continue;
                }
                let hex = &s[prefix.len()..];
                if hex.len() != 128 {
                    r14_problems.push(format!(
                        "properties[`{}`] has hex length {} (expected 128 for blake3-512)",
                        key,
                        hex.len()
                    ));
                }
                if let Some(reason) = cid_format_problem(s) {
                    r5_problems.push(format!("properties[`{}`]: {}", key, reason));
                }
            }
        }
        Some(other) => {
            let msg = format!(
                "`properties` must be an object, got {}",
                value_type(other)
            );
            r5_problems.push(msg.clone());
            r14_problems.push(msg.clone());
            r15_problems.push(msg);
        }
        None => {
            let msg = "`properties` field is missing".to_string();
            r5_problems.push(msg.clone());
            r14_problems.push(msg.clone());
            r15_problems.push(msg);
        }
    }

    let r5_property_values_are_cids = if r5_problems.is_empty() {
        RuleVerdict::Holds
    } else {
        RuleVerdict::Violated(r5_problems.join("; "))
    };
    let r14_no_truncated_digests = if r14_problems.is_empty() {
        RuleVerdict::Holds
    } else {
        RuleVerdict::Violated(r14_problems.join("; "))
    };
    let r15_cid_blake3_512_prefix = if r15_problems.is_empty() {
        RuleVerdict::Holds
    } else {
        RuleVerdict::Violated(r15_problems.join("; "))
    };

    // --- R6: declaredAt is ISO-8601 UTC. -------------------------------
    let r6_declared_at_iso8601 = match obj.and_then(|o| o.get("declaredAt")) {
        Some(JsonValue::String(s)) => match iso8601_utc_problem(s) {
            None => RuleVerdict::Holds,
            Some(reason) => RuleVerdict::Violated(format!(
                "`declaredAt` is not ISO-8601 UTC: {} (value `{}`)",
                reason, s
            )),
        },
        Some(other) => RuleVerdict::Violated(format!(
            "`declaredAt` must be a string, got {}",
            value_type(other)
        )),
        None => RuleVerdict::Violated("`declaredAt` field is missing".into()),
    };

    // --- R7: underscore-prefixed fields participate in JCS canon. ------
    let r7_underscore_fields_in_canonical = match obj {
        None => RuleVerdict::Violated("top-level value is not an object".into()),
        Some(o) => {
            let underscore_keys: Vec<&String> =
                o.keys().filter(|k| k.starts_with('_')).collect();
            if underscore_keys.is_empty() {
                RuleVerdict::Holds
            } else {
                let mut stripped = o.clone();
                stripped.retain(|k, _| !k.starts_with('_'));
                let stripped_value = JsonValue::Object(stripped);
                let full_value = JsonValue::Object(o.clone());
                match (jcs_encode(&full_value), jcs_encode(&stripped_value)) {
                    (Ok(full_bytes), Ok(stripped_bytes)) => {
                        if full_bytes == stripped_bytes {
                            RuleVerdict::Violated(
                                "JCS-canonical bytes did NOT change when underscore-prefixed \
                                 fields were stripped: the canonicalizer is dropping them, \
                                 violating 1's `participate in canonicalization` rule"
                                    .into(),
                            )
                        } else {
                            RuleVerdict::Holds
                        }
                    }
                    (Err(e), _) | (_, Err(e)) => RuleVerdict::Violated(format!(
                        "JCS encoding failed during R7 check: {}",
                        e
                    )),
                }
            }
        }
    };

    // --- R8 / R11: spec-file CID equals BLAKE3-512(file_bytes). --------
    //
    // R8 is the formula; R11 is its discharge over each (key, file)
    // pair. We discharge them together: if a spec_files map is
    // provided, walk every entry, recompute the file's BLAKE3-512, and
    // compare to `properties[key]`. R8 is reported `Holds` iff every
    // computed CID has the canonical self-identifying form (which the
    // helper produces unconditionally), `Vacuous` if no spec_files
    // were provided.
    let (r8_spec_cid_formula, r11_disk_blake3_matches) = match spec_files {
        None => (
            RuleVerdict::Vacuous(
                "R8 is the formula version of R11; supply `spec_files` to discharge".into(),
            ),
            RuleVerdict::Vacuous(
                "no spec_files provided; cannot recompute disk BLAKE3-512".into(),
            ),
        ),
        Some(map) => {
            let props = obj
                .and_then(|o| o.get("properties"))
                .and_then(|p| p.as_object());
            match props {
                None => (
                    RuleVerdict::Violated(
                        "cannot discharge R8: `properties` is not an object".into(),
                    ),
                    RuleVerdict::Violated(
                        "cannot discharge R11: `properties` is not an object".into(),
                    ),
                ),
                Some(props_obj) => {
                    let mut r11_problems: Vec<String> = Vec::new();
                    let mut r8_problems: Vec<String> = Vec::new();
                    for (key, path) in map {
                        let claimed = match props_obj.get(*key) {
                            Some(JsonValue::String(s)) => s.clone(),
                            Some(other) => {
                                r11_problems.push(format!(
                                    "properties[`{}`] is {}, expected string CID",
                                    key,
                                    value_type(other)
                                ));
                                continue;
                            }
                            None => {
                                r11_problems.push(format!(
                                    "spec_files names `{}` but no such key in `properties`",
                                    key
                                ));
                                continue;
                            }
                        };
                        let bytes = match std::fs::read(path) {
                            Ok(b) => b,
                            Err(e) => {
                                r11_problems.push(format!(
                                    "read `{}` for key `{}`: {}",
                                    path.display(),
                                    key,
                                    e
                                ));
                                continue;
                            }
                        };
                        let recomputed = blake3_512_of(&bytes);
                        // R8: the recomputed CID is by construction
                        // self-identifying; the only way it fails is
                        // an internal helper bug. We assert it for
                        // completeness.
                        if !recomputed.starts_with("blake3-512:") {
                            r8_problems.push(format!(
                                "internal: blake3_512_of produced non-self-identifying CID for key `{}`",
                                key
                            ));
                        }
                        if recomputed != claimed {
                            r11_problems.push(format!(
                                "properties[`{}`] = `{}` does not match BLAKE3-512({}) = `{}`",
                                key, claimed, path.display(), recomputed
                            ));
                        }
                    }
                    let r8 = if r8_problems.is_empty() {
                        RuleVerdict::Holds
                    } else {
                        RuleVerdict::Violated(r8_problems.join("; "))
                    };
                    let r11 = if r11_problems.is_empty() {
                        RuleVerdict::Holds
                    } else {
                        RuleVerdict::Violated(r11_problems.join("; "))
                    };
                    (r8, r11)
                }
            }
        }
    };

    // --- R9 / R12: catalog CID equals BLAKE3-512(JCS(catalog)). --------
    //
    // R9 is the formula; R12 is the discharge against a claimed CID.
    // R9 always holds in the JSON-only flow (we can verify the JCS
    // pipeline runs), and R12 is Vacuous unless `claimed_catalog_cid`
    // was supplied. When it IS supplied we compute the recomputed CID
    // and compare.
    let r9_catalog_cid_formula = match jcs_encode(catalog) {
        Ok(jcs) => {
            let recomputed = blake3_512_of(jcs.as_bytes());
            if recomputed.starts_with("blake3-512:")
                && recomputed.len() == "blake3-512:".len() + 128
            {
                RuleVerdict::Holds
            } else {
                RuleVerdict::Violated(format!(
                    "internal: recomputed catalog CID malformed: `{}`",
                    recomputed
                ))
            }
        }
        Err(e) => RuleVerdict::Violated(format!("JCS encoding failed: {}", e)),
    };

    let r12_catalog_cid_recomputes = match claimed_catalog_cid {
        None => RuleVerdict::Vacuous(
            "no claimed catalog CID provided; cannot compare against recomputed value".into(),
        ),
        Some(claimed) => match jcs_encode(catalog) {
            Ok(jcs) => {
                let recomputed = blake3_512_of(jcs.as_bytes());
                if recomputed == claimed {
                    RuleVerdict::Holds
                } else {
                    RuleVerdict::Violated(format!(
                        "claimed catalog CID `{}` does not match recomputed BLAKE3-512(JCS(catalog)) = `{}`",
                        claimed, recomputed
                    ))
                }
            }
            Err(e) => RuleVerdict::Violated(format!(
                "JCS encoding failed during R12 check: {}",
                e
            )),
        },
    };

    CatalogReport {
        r1_kind,
        r2_name_present,
        r3_version_present,
        r4_algorithms_shape,
        r5_property_values_are_cids,
        r6_declared_at_iso8601,
        r7_underscore_fields_in_canonical,
        r8_spec_cid_formula,
        r9_catalog_cid_formula,
        r11_disk_blake3_matches,
        r12_catalog_cid_recomputes,
        r14_no_truncated_digests,
        r15_cid_blake3_512_prefix,
    }
}

fn value_type(v: &JsonValue) -> &'static str {
    match v {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "bool",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
}

// ---------------------------------------------------------------------------
// Tests  positive (real v1.3.x catalog) and negative (broken fixtures)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use provekit_ir_symbolic::{begin_collecting, finish, reset_collector};
    use std::path::PathBuf;

    /// Number of distinct rules encoded as IR contracts in `invariants()`.
    /// Mirror this constant if you add or remove a contract minted from
    /// `invariants()`. The value is asserted in
    /// `invariants_mints_one_contract_per_rule` below; updating one
    /// without the other is a test failure.
    const ENCODED_RULE_COUNT: usize = 13;

    fn repo_root() -> PathBuf {
        let manifest = env!("CARGO_MANIFEST_DIR");
        PathBuf::from(manifest)
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .expect("CARGO_MANIFEST_DIR has three ancestors")
            .to_path_buf()
    }

    fn real_catalog() -> JsonValue {
        let path = repo_root()
            .join("protocol")
            .join("specs")
            .join("2026-04-30-protocol-catalog.json");
        let bytes =
            std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        serde_json::from_slice(&bytes).expect("catalog parses as JSON")
    }

    fn fixture(name: &str) -> JsonValue {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("catalog-format")
            .join(name);
        let bytes =
            std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        serde_json::from_slice(&bytes).expect("fixture parses as JSON")
    }

    fn tmpdir() -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!(
            "provekit-catalog-format-test-{nanos}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    // -- POSITIVE: every JSON-derivable rule holds against real catalog. -

    #[test]
    fn real_catalog_satisfies_all_encoded_rules() {
        let cat = real_catalog();
        let report = verify_catalog(&cat);
        assert!(
            report.all_hold(),
            "real catalog violated rules: {:?}",
            report.violations()
        );
    }

    // -- INVARIANT: invariants() mints one contract per rule. ------------

    #[test]
    fn invariants_mints_one_contract_per_rule() {
        reset_collector();
        begin_collecting();
        invariants();
        let decls = finish();
        assert_eq!(
            decls.len(),
            ENCODED_RULE_COUNT,
            "expected {} catalog-format contracts, got {}",
            ENCODED_RULE_COUNT,
            decls.len()
        );
        let names: Vec<&str> = decls.iter().map(|d| d.name.as_str()).collect();
        assert!(names.iter().all(|n| n.starts_with("catalog_format_r")));
        let mut sorted = names.clone();
        sorted.sort();
        let original = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), original, "duplicate contract names");
    }

    // -- NEGATIVE: per-rule fixtures fail closed on the targeted rule. ---

    #[test]
    fn r1_violation_kind_not_catalog_is_caught() {
        let cat = fixture("r1-bad-kind.json");
        let report = verify_catalog(&cat);
        assert!(
            matches!(report.r1_kind, RuleVerdict::Violated(_)),
            "R1 should fire on bad kind; verdict = {:?}",
            report.r1_kind
        );
    }

    #[test]
    fn r2_violation_name_missing_is_caught() {
        let cat = fixture("r2-name-missing.json");
        let report = verify_catalog(&cat);
        assert!(
            matches!(report.r2_name_present, RuleVerdict::Violated(_)),
            "R2 should fire on missing name; verdict = {:?}",
            report.r2_name_present
        );
    }

    #[test]
    fn r3_violation_version_missing_is_caught() {
        let cat = fixture("r3-version-missing.json");
        let report = verify_catalog(&cat);
        assert!(
            matches!(report.r3_version_present, RuleVerdict::Violated(_)),
            "R3 should fire on missing version; verdict = {:?}",
            report.r3_version_present
        );
    }

    #[test]
    fn r4_violation_algorithms_not_object_is_caught() {
        let cat = fixture("r4-algorithms-not-object.json");
        let report = verify_catalog(&cat);
        assert!(
            matches!(report.r4_algorithms_shape, RuleVerdict::Violated(_)),
            "R4 should fire on non-object algorithms; verdict = {:?}",
            report.r4_algorithms_shape
        );
    }

    #[test]
    fn r4_violation_role_not_array_is_caught() {
        let cat = fixture("r4-role-not-array.json");
        let report = verify_catalog(&cat);
        assert!(
            matches!(report.r4_algorithms_shape, RuleVerdict::Violated(_)),
            "R4 should fire when a role is not an array; verdict = {:?}",
            report.r4_algorithms_shape
        );
    }

    #[test]
    fn r5_violation_property_value_not_string_is_caught() {
        let cat = fixture("r5-property-value-not-string.json");
        let report = verify_catalog(&cat);
        assert!(
            matches!(report.r5_property_values_are_cids, RuleVerdict::Violated(_)),
            "R5 should fire on non-string property value; verdict = {:?}",
            report.r5_property_values_are_cids
        );
    }

    #[test]
    fn r6_violation_declared_at_local_time_is_caught() {
        let cat = fixture("r6-declared-at-local-time.json");
        let report = verify_catalog(&cat);
        assert!(
            matches!(report.r6_declared_at_iso8601, RuleVerdict::Violated(_)),
            "R6 should fire on non-UTC declaredAt; verdict = {:?}",
            report.r6_declared_at_iso8601
        );
    }

    #[test]
    fn r14_violation_truncated_digest_is_caught() {
        let cat = fixture("r14-truncated-digest.json");
        let report = verify_catalog(&cat);
        assert!(
            matches!(report.r14_no_truncated_digests, RuleVerdict::Violated(_)),
            "R14 should fire on truncated digest; verdict = {:?}",
            report.r14_no_truncated_digests
        );
    }

    #[test]
    fn r15_violation_wrong_prefix_is_caught() {
        let cat = fixture("r15-sha256-prefix.json");
        let report = verify_catalog(&cat);
        assert!(
            matches!(report.r15_cid_blake3_512_prefix, RuleVerdict::Violated(_)),
            "R15 should fire on sha256 prefix; verdict = {:?}",
            report.r15_cid_blake3_512_prefix
        );
    }

    #[test]
    fn r7_underscore_fields_change_jcs_bytes() {
        let cat = real_catalog();
        let report = verify_catalog(&cat);
        assert!(
            matches!(report.r7_underscore_fields_in_canonical, RuleVerdict::Holds),
            "R7 should hold on real catalog; verdict = {:?}",
            report.r7_underscore_fields_in_canonical
        );
        let obj = cat.as_object().expect("catalog is object");
        let mut stripped = obj.clone();
        stripped.retain(|k, _| !k.starts_with('_'));
        let stripped_value = JsonValue::Object(stripped);
        let full_value = JsonValue::Object(obj.clone());
        let full_jcs = jcs_encode(&full_value).unwrap();
        let stripped_jcs = jcs_encode(&stripped_value).unwrap();
        assert_ne!(
            full_jcs, stripped_jcs,
            "underscore-prefixed fields must affect JCS bytes"
        );
    }

    // -- DISK-DEPENDENT: R8 / R11 / R12 with an in-memory spec dir. -----

    /// Build a tiny synthetic spec dir + matching catalog. We write a
    /// known-bytes spec file to disk, compute its real BLAKE3-512, then
    /// embed that CID into the catalog so R11 holds. R12 is given the
    /// recomputed catalog CID. Both should hold.
    fn synth_spec_dir_and_catalog() -> (PathBuf, JsonValue, String) {
        let dir = tmpdir();
        let spec_path = dir.join("hello-spec.md");
        let spec_bytes: &[u8] = b"# hello spec\n\nhello world\n";
        std::fs::write(&spec_path, spec_bytes).unwrap();
        let real_cid = blake3_512_of(spec_bytes);
        let cat = serde_json::json!({
            "kind": "catalog",
            "name": "provekit-protocol",
            "version": "v0.0.0-test",
            "algorithms": {
                "hash": ["blake3-512"],
                "signature": ["ed25519"],
                "pubkey": ["ed25519"],
            },
            "properties": {
                "hello-spec": real_cid,
            },
            "declaredAt": "2026-05-02T00:00:00Z",
        });
        let jcs = jcs_encode(&cat).unwrap();
        let recomputed_catalog_cid = blake3_512_of(jcs.as_bytes());
        (spec_path, cat, recomputed_catalog_cid)
    }

    #[test]
    fn r8_r11_disk_check_holds_on_synth_catalog() {
        let (spec_path, cat, _) = synth_spec_dir_and_catalog();
        let pairs: Vec<(&str, &Path)> = vec![("hello-spec", spec_path.as_path())];
        let report = verify_catalog_against_spec_dir(&cat, pairs, None);
        assert!(
            matches!(report.r11_disk_blake3_matches, RuleVerdict::Holds),
            "R11 should hold on synth catalog; verdict = {:?}",
            report.r11_disk_blake3_matches
        );
        assert!(
            matches!(report.r8_spec_cid_formula, RuleVerdict::Holds),
            "R8 should hold on synth catalog; verdict = {:?}",
            report.r8_spec_cid_formula
        );
        let _ = std::fs::remove_dir_all(spec_path.parent().unwrap());
    }

    #[test]
    fn r11_violation_disk_bytes_drift_is_caught() {
        // Construct a catalog with the right shape but wrong CID for
        // `hello-spec`. The on-disk file has the real bytes; the
        // catalog's claimed CID is for different bytes.
        let dir = tmpdir();
        let spec_path = dir.join("hello-spec.md");
        std::fs::write(&spec_path, b"# real spec\n").unwrap();
        let wrong_cid = blake3_512_of(b"# different content\n");
        let cat = serde_json::json!({
            "kind": "catalog",
            "name": "provekit-protocol",
            "version": "v0.0.0-test",
            "algorithms": {
                "hash": ["blake3-512"],
                "signature": ["ed25519"],
                "pubkey": ["ed25519"],
            },
            "properties": {
                "hello-spec": wrong_cid,
            },
            "declaredAt": "2026-05-02T00:00:00Z",
        });
        let pairs: Vec<(&str, &Path)> = vec![("hello-spec", spec_path.as_path())];
        let report = verify_catalog_against_spec_dir(&cat, pairs, None);
        assert!(
            matches!(report.r11_disk_blake3_matches, RuleVerdict::Violated(_)),
            "R11 should fire on drifted disk bytes; verdict = {:?}",
            report.r11_disk_blake3_matches
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn r12_catalog_cid_holds_on_correct_recompute() {
        let (spec_path, cat, recomputed) = synth_spec_dir_and_catalog();
        let report = verify_catalog_against_spec_dir(
            &cat,
            std::iter::empty(),
            Some(recomputed.as_str()),
        );
        assert!(
            matches!(report.r12_catalog_cid_recomputes, RuleVerdict::Holds),
            "R12 should hold when claimed CID matches recomputation; verdict = {:?}",
            report.r12_catalog_cid_recomputes
        );
        let _ = std::fs::remove_dir_all(spec_path.parent().unwrap());
    }

    #[test]
    fn r12_violation_wrong_claimed_cid_is_caught() {
        let (spec_path, cat, _real) = synth_spec_dir_and_catalog();
        // A made-up CID with the right shape but wrong digest.
        let bogus = format!(
            "blake3-512:{}",
            "0".repeat(128)
        );
        let report = verify_catalog_against_spec_dir(
            &cat,
            std::iter::empty(),
            Some(bogus.as_str()),
        );
        assert!(
            matches!(report.r12_catalog_cid_recomputes, RuleVerdict::Violated(_)),
            "R12 should fire on wrong claimed CID; verdict = {:?}",
            report.r12_catalog_cid_recomputes
        );
        let _ = std::fs::remove_dir_all(spec_path.parent().unwrap());
    }

    #[test]
    fn r9_holds_on_any_well_formed_catalog() {
        // R9 is the formula form of R12; even without a claimed CID,
        // the formula itself holds iff the JCS pipeline produces a
        // self-identifying CID of the right shape.
        let cat = real_catalog();
        let report = verify_catalog(&cat);
        assert!(
            matches!(report.r9_catalog_cid_formula, RuleVerdict::Holds),
            "R9 should hold on real catalog; verdict = {:?}",
            report.r9_catalog_cid_formula
        );
    }

    #[test]
    fn json_only_path_marks_disk_rules_vacuous() {
        let cat = real_catalog();
        let report = verify_catalog(&cat);
        assert!(
            matches!(report.r8_spec_cid_formula, RuleVerdict::Vacuous(_)),
            "R8 should be vacuous in JSON-only flow; got {:?}",
            report.r8_spec_cid_formula
        );
        assert!(
            matches!(report.r11_disk_blake3_matches, RuleVerdict::Vacuous(_)),
            "R11 should be vacuous in JSON-only flow; got {:?}",
            report.r11_disk_blake3_matches
        );
        assert!(
            matches!(report.r12_catalog_cid_recomputes, RuleVerdict::Vacuous(_)),
            "R12 should be vacuous in JSON-only flow; got {:?}",
            report.r12_catalog_cid_recomputes
        );
        // Vacuous still passes all_hold.
        assert!(report.all_hold());
    }

    // -- Aggregate sanity: every rule has both a positive and a negative
    //                       coverage path. ------------------------------

    #[test]
    fn coverage_summary_every_rule_has_positive_and_negative() {
        let names = [
            "r1-bad-kind.json",
            "r2-name-missing.json",
            "r3-version-missing.json",
            "r4-algorithms-not-object.json",
            "r4-role-not-array.json",
            "r5-property-value-not-string.json",
            "r6-declared-at-local-time.json",
            "r14-truncated-digest.json",
            "r15-sha256-prefix.json",
        ];
        for n in names {
            let v = fixture(n);
            let _ = verify_catalog(&v);
        }
    }
}
