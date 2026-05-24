// SPDX-License-Identifier: Apache-2.0
//
// rfc8785-jcs-shim-demo-client: a CONSUMER that needs RFC 8785 JSON
// Canonicalization Scheme encoding via the
// concept:family:json-canonicalization CONTRACT, not a specific library. The
// three carrier sites below cite concept:rfc8785-jcs-encode,
// concept:rfc8785-jcs-encode-value, and concept:rfc8785-jcs-encode-string.
// `provekit materialize --library provekit-rfc8785-jcs-java` realizes them
// against the rfc8785-jcs shim (provekit-shim-rfc8785-jcs-java).
//
// The encode_jcs body recurses into encode_value, which recurses into
// encode_string; the carrier function names (encode_jcs / encode_value /
// encode_string) match the shim's so those recursive calls resolve. The body
// between each carrier and the next site is what the kit's assemble RPC fills
// from the shim's signed .proof (NOT a disk JSON cache).

package org.provekit.demo.jcsclient;

import com.fasterxml.jackson.databind.JsonNode;

public final class Canonicalizer {

    private Canonicalizer() {
    }

    // provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:rfc8785-jcs-encode","family":"concept:family:json-canonicalization","function":"encode_jcs","params":["v"],"param_types":["JsonNode"],"return_type":"String","named_term_tree":{"conceptName":"concept:rfc8785-jcs-encode","args":[{"sort":"JsonValue","source":"v"}]}}
    // provekit-concept-payload-cid: blake3-512:cff19d72b4b2216fef151fde3f77996959ec323bd75774a10e096d2fb955ed40b29f2c97791b3ec4d3bdc50eda4367cdd4a80351cfeefc5db6dc7d840e4cd275

    // provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:rfc8785-jcs-encode-value","family":"concept:family:json-canonicalization","function":"encode_value","params":["v","out"],"param_types":["JsonNode","StringBuilder"],"return_type":"void","named_term_tree":{"conceptName":"concept:rfc8785-jcs-encode-value","args":[{"sort":"JsonValue","source":"v"},{"sort":"StringBuffer","source":"out"}]}}
    // provekit-concept-payload-cid: blake3-512:8bcec81ba210f38e22b96c8fbaaf3524c1ba208c6cee138604ed0a07ba77b62a98d17270583c5d1b6814054da42586a476696c875af5f2f7c04157ccb2a4a013

    // provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:rfc8785-jcs-encode-string","family":"concept:family:json-canonicalization","function":"encode_string","params":["s","out"],"param_types":["String","StringBuilder"],"return_type":"void","named_term_tree":{"conceptName":"concept:rfc8785-jcs-encode-string","args":[{"sort":"JsonText","source":"s"},{"sort":"StringBuffer","source":"out"}]}}
    // provekit-concept-payload-cid: blake3-512:b990e7c5e760fd47bbc9d0b0fe87d637df018b9227a3b647e4aea2787cd9fed07db31e3dd871ffae1213edcf997e1eb8ad91a5b5b5648b3ca580c46f9b0113a0
}
