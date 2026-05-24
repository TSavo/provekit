// SPDX-License-Identifier: Apache-2.0
//
// json-shim-demo-client: a CONSUMER that needs JSON parse + serialize via the
// concept:family:json CONTRACT, not a specific library. The two carrier sites
// below cite concept:json-parse and concept:json-serialize. `provekit
// materialize --library jackson` realizes them against the Jackson shim;
// `--library gson` realizes the SAME contract against the Gson shim.
//
// The signature is library-NEUTRAL: parse returns `Object` and serialize takes
// `Object` — the common supertype of Jackson's JsonNode and Gson's JsonElement.
// That is what lets ONE consumer materialize against EITHER library. The body
// between the carrier and the next site is what the kit's assemble RPC fills.

package org.provekit.demo.jsonclient;

public final class ConfigCodec {

    private ConfigCodec() {
    }

    // provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:json-parse","family":"concept:family:json","function":"parseConfig","params":["s"],"param_types":["String"],"return_type":"Object","named_term_tree":{"conceptName":"concept:json-parse","args":[{"sort":"JsonText","source":"s"}]}}
    // provekit-concept-payload-cid: blake3-512:9e05076563fd91c477f9541a47f13c88ea9319335938a4edbf14959c9b5bfcb35dc50355b3bef3c21ec3ad16668b199a267c87cf4fd4e45d2ee34df2e03eb40d

    // provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:json-serialize","family":"concept:family:json","function":"renderConfig","params":["v"],"param_types":["Object"],"return_type":"String","named_term_tree":{"conceptName":"concept:json-serialize","args":[{"sort":"JsonValue","source":"v"}]}}
    // provekit-concept-payload-cid: blake3-512:4c19aa95cbd826f50fac08159ffd8812bc47b4dfd80609ed8cec47aa75eb9abf6a233f46b581f74bbbdf0208f96eb1d954b734254be870d8287c0de955b78e8b
}
