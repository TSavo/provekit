// SPDX-License-Identifier: Apache-2.0
//
// blake3-shim-demo-client: a CONSUMER that needs a BLAKE3-512 digest via the
// concept:family:hash CONTRACT, not a specific library. The carrier site below
// cites concept:blake3-512-of. `provekit materialize --library bouncycastle`
// realizes it against the Bouncy Castle shim (provekit-shim-blake3-java).
//
// The signature is library-NEUTRAL: hashConfig takes byte[] and returns byte[].
// The body between the carrier and the next site is what the kit's assemble
// RPC fills from the shim's signed .proof (NOT a disk JSON cache).

package org.provekit.demo.blake3client;

public final class Hasher {

    private Hasher() {
    }

    // provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:blake3-512-of","family":"concept:family:hash","function":"hashConfig","params":["bytes"],"param_types":["byte[]"],"return_type":"byte[]","named_term_tree":{"conceptName":"concept:blake3-512-of","args":[{"sort":"Bytes","source":"bytes"}]}}
    // provekit-concept-payload-cid: blake3-512:c8f9643534534708d4a7da56763d9a1a3cc98dbfe66e3d5ca2605e801b21f698716f90822b63c5fd6a2e897e3ffc426c890cd957760433fe2b1a788905eb95a4
}
