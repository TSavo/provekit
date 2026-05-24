// SPDX-License-Identifier: Apache-2.0
//
// json-shim-demo-client: a CONSUMER that needs JSON parse + serialize via the
// concept:family:json CONTRACT, not a specific library. The two carrier sites
// below cite concept:json-parse and concept:json-serialize. `provekit
// materialize --library jackson` realizes them against the Jackson shim;
// `--library gson` realizes the SAME contract against the Gson shim.
//
// The body between the carrier and the next site is what materialize replaces.

package org.provekit.demo.jsonclient;

public final class ConfigCodec {

    private ConfigCodec() {
    }

    // provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:json-parse","family":"concept:family:json","function":"parseConfig","params":["s"],"param_types":["String"],"return_type":"JsonNode","named_term_tree":{"conceptName":"concept:json-parse","args":[{"sort":"JsonText","source":"s"}]}}
    // provekit-concept-payload-cid: blake3-512:c79c205687559a241aa4acb56c231b97d5a2ad4543aa0220ac5e25892ac609fcc55f4fa39081ebfc850002a21f7c7d55b9daf895420a9bbdedf5c23800822610

    // provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:json-serialize","family":"concept:family:json","function":"renderConfig","params":["v"],"param_types":["JsonNode"],"return_type":"String","named_term_tree":{"conceptName":"concept:json-serialize","args":[{"sort":"JsonValue","source":"v"}]}}
    // provekit-concept-payload-cid: blake3-512:7dc07514eb7c8154591b7aa9f988ed92f16ae0bca11d47b7327c504049dfa3bc5a3776a2627a675b4247642df22b75cd36b169cbf9e4637870a9dc496e6f8169
}
