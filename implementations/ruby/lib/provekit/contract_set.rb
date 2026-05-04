# SPDX-License-Identifier: Apache-2.0
#
# Provekit::ContractSet -- contractSetCid computation.
#
# Per `protocol/specs/2026-05-03-contract-set-extension.md` §1:
#
#   contractSetCid := "blake3-512:" || hex(BLAKE3-512(JCS(<sorted contractCids>)))
#
# Where `<sorted contractCids>` is the array of `contractCid` values
# (per `2026-05-03-contract-cid-vs-attestation-cid.md`) sorted
# lexicographically by their hex representation. The sort makes the
# set order-independent: two implementations enumerating contracts in
# different orders agree on the set's CID.
#
# Reference (authoritative):
#   implementations/rust/provekit-claim-envelope/src/lib.rs (compute_contract_set_cid)
#   implementations/python/provekit-lift-py-tests/src/provekit_lift_py_tests/claim_envelope.py
#
# Cross-kit byte-equivalence:
#   Output is byte-identical to Rust + Python kits for the same inputs.
#   See test/test_claim_envelope.rb cross-kit tests.

require_relative "blake3"
require_relative "ir"

module Provekit
  module ContractSet
    # Compute the contractSetCid from a sequence of contractCid strings.
    # The sequence is sorted lexicographically before JCS-encoding, so
    # callers may enumerate contracts in any order.
    #
    # NOTE: the spec treats the input as a SEQUENCE-of-CIDs, not a
    # deduplicated set; duplicates in the input affect the output. If
    # callers want set semantics (uniqueness), they must dedupe before
    # calling.
    def self.compute_cid(contract_cids)
      sorted = contract_cids.map(&:to_s).sort
      jcs = Provekit::IR::Jcs.encode(sorted)
      Provekit::Blake3.hex(jcs)
    end
  end
end
