// SPDX-License-Identifier: Apache-2.0
//
// stdio-shim-demo-client: a CONSUMER that needs stdin/stdout/stderr line I/O
// via the concept:family:stdio-stream CONTRACT, not a specific library. The
// three carrier sites below cite concept:stdio-read-line,
// concept:stdio-write-line, and concept:stderr-write-line. `sugar
// materialize --library java-io` realizes them against the java.io shim
// (sugar-shim-stdio-java).
//
// The signatures are library-NEUTRAL (String / void). The body between each
// carrier and the next site is what the kit's assemble RPC fills from the
// shim's signed .proof (NOT a disk JSON cache).

package org.sugar.demo.stdioclient;

public final class LineEcho {

    private LineEcho() {
    }

    // sugar-concept: {"artifact_kind":"sugar-concept-citation-comment-sugar","concept_name":"concept:stdio-read-line","family":"concept:family:stdio-stream","function":"readLine","params":[],"param_types":[],"return_type":"String","named_term_tree":{"conceptName":"concept:stdio-read-line","args":[]}}
    // sugar-concept-payload-cid: blake3-512:476fc3025ad9197be403df3b440a1d7c2451600d4ac478701a05e01af2e4e23df5d31009a7ffeb9f52f296835da8828d539ba5da2b2da9ce204a377cbb7ccc8a

    // sugar-concept: {"artifact_kind":"sugar-concept-citation-comment-sugar","concept_name":"concept:stdio-write-line","family":"concept:family:stdio-stream","function":"writeLine","params":["line"],"param_types":["String"],"return_type":"void","named_term_tree":{"conceptName":"concept:stdio-write-line","args":[{"sort":"Text","source":"line"}]}}
    // sugar-concept-payload-cid: blake3-512:9fae0c5d99eb1aed0de5473f251d014ee50f292146371b18a10e0b89608a8180dfd406ca9631363b31ef6b4e6342f7e20d92b27e38236f482b528cbe9b761dc0

    // sugar-concept: {"artifact_kind":"sugar-concept-citation-comment-sugar","concept_name":"concept:stderr-write-line","family":"concept:family:stdio-stream","function":"errLine","params":["line"],"param_types":["String"],"return_type":"void","named_term_tree":{"conceptName":"concept:stderr-write-line","args":[{"sort":"Text","source":"line"}]}}
    // sugar-concept-payload-cid: blake3-512:19b9cd63ad8d32483d0c326cd01e6d408e3101dd9f204e413e577d828a965e1c9a860e69feea258a9afccb4bf9741132d7fb70652a09d3d3f8866af00f6fa328
}
