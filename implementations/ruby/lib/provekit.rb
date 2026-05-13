# SPDX-License-Identifier: Apache-2.0
#
# ProvekIt Ruby kit. Native crypto substrate (Side B per issue #176):
# BLAKE3 + JCS + Ed25519 + CBOR + envelope construction, no shell-out.
#
# Public surface:
#   Provekit::Blake3        - BLAKE3-512 hash, "blake3-512:<hex>" form.
#   Provekit::IR            - IR types + JCS canonical JSON emitter.
#   Provekit::Jcs           - alias for Provekit::IR::Jcs (one canonicalizer).
#   Provekit::Cbor          - deterministic CBOR encoder (RFC 8949 §4.2.1).
#   Provekit::Signing       - Ed25519 sign/verify, self-identifying string form.
#   Provekit::ProofEnvelope - .proof envelope build + verify.
#   Provekit::ClaimEnvelope - v1.2 layered claim envelope minter.
#   Provekit::ContractSet   - contractSetCid computation.

require_relative "provekit/blake3"
require_relative "provekit/version"
require_relative "provekit/ir"
require_relative "provekit/cbor"
require_relative "provekit/signing"
require_relative "provekit/proof_envelope"
require_relative "provekit/claim_envelope"
require_relative "provekit/contract_set"

module Provekit
  # Single canonical JCS encoder for the kit. Aliases the implementation
  # already in IR (which the IR conformance fixtures pin against), so
  # there is exactly one JCS encoder in the gem.
  Jcs = IR::Jcs unless defined?(Jcs)
end
