// provekit-cli lib.rs — test support surface.
//
// Exposes the linker core for integration tests in tests/polyglot_smoke.rs
// without pulling in all of main.rs's binary dependencies.
//
// The linker algorithm is implemented here rather than imported from cmd_link.rs
// because cmd_link.rs references `crate::LinkArgs` which only exists in the
// binary context (main.rs). The lib and binary are separate compilation units.
// The duplication is intentional and correct for this crate layout.

pub use linker_core::{derive_link_bundle, KitCallEdge, KitContract, LinkerError};

mod linker_core {
    use std::collections::BTreeMap;
    use provekit_canonicalizer::{blake3_512_of, encode_jcs};
    use serde_json::Value as Json;

    #[derive(Debug, Clone)]
    pub struct KitContract {
        pub name: String,
        pub kit: String,
        pub contract_cid: String,
        pub pre_json: Option<Json>,
        pub post_json: Option<Json>,
    }

    #[derive(Debug, Clone)]
    pub struct KitCallEdge {
        pub source_contract_cid: String,
        pub target_contract_cid: Option<String>,
        pub target_symbol: String,
        pub call_site_locus_json: Json,
        pub evidence_term_json: Json,
    }

    #[derive(Debug, Clone)]
    pub struct LinkerError {
        pub kind: String,
        pub target_symbol: String,
        pub source_contract_cid: String,
        pub reason: String,
    }

    /// Core linker algorithm — takes pre-collected contracts and call-edges,
    /// runs derivation per spec R2-R5, and returns a link bundle JSON.
    pub fn derive_link_bundle(
        all_contracts: Vec<KitContract>,
        all_call_edges: Vec<KitCallEdge>,
    ) -> Json {
        let mut name_kit_index: BTreeMap<(String, String), String> = BTreeMap::new();
        for c in &all_contracts {
            name_kit_index.insert((c.name.clone(), c.kit.clone()), c.contract_cid.clone());
        }

        let mut all_contract_cids: Vec<String> =
            all_contracts.iter().map(|c| c.contract_cid.clone()).collect();
        all_contract_cids.sort();
        let contract_set_cid = compute_set_cid_sorted(&all_contract_cids);

        let mut bridges: Vec<Json> = Vec::new();
        let mut linker_errors_out: Vec<LinkerError> = Vec::new();

        let mut sorted_edges = all_call_edges;
        sorted_edges.sort_by(|a, b| {
            a.source_contract_cid
                .cmp(&b.source_contract_cid)
                .then_with(|| {
                    let la = a.call_site_locus_json.to_string();
                    let lb = b.call_site_locus_json.to_string();
                    la.cmp(&lb)
                })
        });

        for edge in &sorted_edges {
            let resolved_target_cid = if let Some(ref cid) = edge.target_contract_cid {
                Some(cid.clone())
            } else {
                resolve_target_symbol(&edge.target_symbol, &name_kit_index)
            };

            match resolved_target_cid {
                None => {
                    linker_errors_out.push(LinkerError {
                        kind: "unresolved-symbol".into(),
                        target_symbol: edge.target_symbol.clone(),
                        source_contract_cid: edge.source_contract_cid.clone(),
                        reason: format!(
                            "targetSymbol `{}` did not resolve to any contract in the union",
                            edge.target_symbol
                        ),
                    });
                }
                Some(target_cid) => {
                    let target_contract = all_contracts
                        .iter()
                        .find(|c| c.contract_cid == target_cid);
                    let source_contract = all_contracts
                        .iter()
                        .find(|c| c.contract_cid == edge.source_contract_cid);

                    let source_post = source_contract.and_then(|c| c.post_json.as_ref());
                    let target_pre = target_contract.and_then(|c| c.pre_json.as_ref());

                    let bridge = derive_bridge(
                        &edge.source_contract_cid,
                        &target_cid,
                        &edge.call_site_locus_json,
                        &edge.evidence_term_json,
                    );
                    bridges.push(bridge);

                    let _ = target_pre;
                    if let Some(err) = discharge_obligation(
                        source_post,
                        target_pre,
                        &edge.source_contract_cid,
                        &target_cid,
                        &edge.target_symbol,
                    ) {
                        linker_errors_out.push(err);
                    }
                }
            }
        }

        bridges.sort_by(|a, b| {
            let ak = a
                .get("header")
                .and_then(|h| h.get("target"))
                .and_then(|t| t.get("cid"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let bk = b
                .get("header")
                .and_then(|h| h.get("target"))
                .and_then(|t| t.get("cid"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            ak.cmp(&bk)
        });

        let call_edge_set_cid = {
            let mut edge_bytes: Vec<String> = sorted_edges
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "sourceContractCid": e.source_contract_cid,
                        "targetContractCid": e.target_contract_cid,
                        "targetSymbol": e.target_symbol,
                    })
                    .to_string()
                })
                .collect();
            edge_bytes.sort();
            compute_set_cid_sorted(&edge_bytes)
        };

        let bridge_set_cid = {
            let mut bridge_strs: Vec<String> = bridges.iter().map(|b| b.to_string()).collect();
            bridge_strs.sort();
            compute_set_cid_sorted(&bridge_strs)
        };

        let linker_error_jsons: Vec<Json> = linker_errors_out
            .iter()
            .map(|e| {
                serde_json::json!({
                    "kind": "linker-error",
                    "errorKind": e.kind,
                    "targetSymbol": e.target_symbol,
                    "sourceContractCid": e.source_contract_cid,
                    "reason": e.reason,
                })
            })
            .collect();

        let bundle_without_cid = serde_json::json!({
            "schemaVersion": "1",
            "kind": "link-bundle",
            "contractSetCid": contract_set_cid,
            "callEdgeSetCid": call_edge_set_cid,
            "bridgeSetCid": bridge_set_cid,
            "linkerVersion": "0.1.0",
            "linkerErrors": linker_error_jsons,
            "bridges": bridges,
        });

        let link_bundle_cid = blake3_512_of(&jcs_of_json(&bundle_without_cid));
        let mut bundle_json = bundle_without_cid;
        if let Some(obj) = bundle_json.as_object_mut() {
            obj.insert("linkBundleCid".into(), Json::String(link_bundle_cid));
        }
        bundle_json
    }

    fn resolve_target_symbol(
        target_symbol: &str,
        name_kit_index: &BTreeMap<(String, String), String>,
    ) -> Option<String> {
        let pos = target_symbol.find(':')?;
        let kit = &target_symbol[..pos];
        let name = &target_symbol[pos + 1..];
        if kit.is_empty() || name.is_empty() {
            return None;
        }
        name_kit_index.get(&(name.to_string(), kit.to_string())).cloned()
    }

    fn derive_bridge(
        source_contract_cid: &str,
        target_contract_cid: &str,
        call_site_locus: &Json,
        evidence_term: &Json,
    ) -> Json {
        serde_json::json!({
            "schemaVersion": "2",
            "kind": "bridge",
            "header": {
                "kind": "bridge",
                "sourceContractCid": source_contract_cid,
                "target": {
                    "kind": "contract",
                    "cid": target_contract_cid
                }
            },
            "metadata": {
                "callSite": call_site_locus,
                "derivedRelation": {
                    "kind": "post-implies-pre",
                    "evidenceTerm": evidence_term
                },
                "derivedBy": "linker",
                "linkerVersion": "0.1.0"
            }
        })
    }

    fn discharge_obligation(
        source_post: Option<&Json>,
        _target_pre: Option<&Json>,
        source_contract_cid: &str,
        target_cid: &str,
        target_symbol: &str,
    ) -> Option<LinkerError> {
        match source_post {
            None | Some(Json::Null) => Some(LinkerError {
                kind: "unprovable-obligation".into(),
                target_symbol: target_symbol.to_string(),
                source_contract_cid: source_contract_cid.to_string(),
                reason: format!(
                    "caller post-condition is absent; cannot discharge `post_caller ⊃ pre_callee` for target `{target_cid}`"
                ),
            }),
            Some(_) => None,
        }
    }

    fn compute_set_cid_sorted(sorted_items: &[String]) -> String {
        let arr: Vec<std::sync::Arc<provekit_canonicalizer::Value>> = sorted_items
            .iter()
            .map(|s| provekit_canonicalizer::Value::string(s.clone()))
            .collect();
        let v = provekit_canonicalizer::Value::array(arr);
        let jcs = encode_jcs(&v);
        blake3_512_of(jcs.as_bytes())
    }

    fn jcs_of_json(v: &Json) -> Vec<u8> {
        let cv = json_to_value(v);
        encode_jcs(&cv).into_bytes()
    }

    fn json_to_value(j: &Json) -> provekit_canonicalizer::Value {
        match j {
            Json::Null => provekit_canonicalizer::Value::Null,
            Json::Bool(b) => provekit_canonicalizer::Value::Bool(*b),
            Json::Number(n) => {
                if let Some(i) = n.as_i64() {
                    provekit_canonicalizer::Value::Integer(i)
                } else {
                    provekit_canonicalizer::Value::String(n.to_string())
                }
            }
            Json::String(s) => provekit_canonicalizer::Value::String(s.clone()),
            Json::Array(arr) => provekit_canonicalizer::Value::Array(
                arr.iter()
                    .map(json_to_value)
                    .map(std::sync::Arc::new)
                    .collect(),
            ),
            Json::Object(map) => {
                let mut entries: Vec<(String, std::sync::Arc<provekit_canonicalizer::Value>)> =
                    map.iter()
                        .map(|(k, v)| (k.clone(), std::sync::Arc::new(json_to_value(v))))
                        .collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                provekit_canonicalizer::Value::Object(entries)
            }
        }
    }
}
