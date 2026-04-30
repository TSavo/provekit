// SPDX-License-Identifier: Apache-2.0
//
// .proof envelope builder.
//
// Spec: protocol/specs/2026-04-30-proof-file-format.md
//
// Inputs:
//   name, version, members map, signer CID, signer seed, declaredAt
// Outputs:
//   final CBOR bytes + filename CID (sha256(bytes)[:32 hex chars])
//
// Determinism: same inputs → byte-identical output. Conformance with
// other implementations is proved by both producing the same bytes for
// the same input.

#pragma once

#include <array>
#include <cstdint>
#include <map>
#include <string>
#include <vector>

#include "sign_ed25519.hpp"

namespace provekit::proof_envelope {

struct ProofEnvelopeInput {
    std::string name;
    std::string version;
    /** member CID (tstr) → canonical envelope bytes (bstr). */
    std::map<std::string, std::vector<uint8_t>> members;
    std::string signer_cid;
    Ed25519Seed signer_seed;
    std::string declared_at;  // RFC 3339, e.g. "2026-04-30T12:00:00.000Z"
};

struct ProofEnvelopeOutput {
    std::vector<uint8_t> bytes;
    std::string filename_cid;  // 32 lowercase hex chars
};

ProofEnvelopeOutput build_proof_envelope(const ProofEnvelopeInput& input);

}  // namespace provekit::proof_envelope
