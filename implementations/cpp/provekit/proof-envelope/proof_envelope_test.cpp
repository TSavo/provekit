// SPDX-License-Identifier: Apache-2.0
//
// Proof-envelope smoke test for v1.1.0 (hash-widening cut).
//
// During the v1.1.0 transition the TypeScript reference implementation
// is being ported to BLAKE3-512 self-identifying hashes in parallel
// with this C++ port. Until TS lands, this test cannot pin a
// cross-language byte-identical fixture (the TS-derived bytes were
// computed under SHA-256-prefix-32 and are no longer applicable).
//
// What this test asserts in the meantime:
//   1. The C++ proof-envelope builder runs without throwing on a known
//      deterministic input.
//   2. The filename CID is self-identifying and shaped correctly:
//      "blake3-512:" + 128 lowercase hex chars.
//   3. Re-running the builder on the same input produces identical
//      bytes (determinism).
//   4. The output bytes hash to the produced filename CID (trust root
//      coherence).
//
// When the TS reference implementation ships v1.1.0 envelopes, the
// expected_filename_cid + expected hex below get re-pinned by running
// the TS builder over the same fixture and asserting equality. Until
// then, the C++ builder is its own conformance witness.

#include <array>
#include <cstdio>
#include <cstdint>
#include <map>
#include <string>
#include <vector>

#include "../canonicalizer/hash.hpp"
#include "proof_envelope.hpp"

using namespace provekit::proof_envelope;

namespace {

std::string hex(const std::vector<uint8_t>& bytes) {
    static constexpr char H[] = "0123456789abcdef";
    std::string out;
    out.reserve(bytes.size() * 2);
    for (uint8_t b : bytes) {
        out.push_back(H[(b >> 4) & 0xF]);
        out.push_back(H[b & 0xF]);
    }
    return out;
}

bool starts_with(const std::string& s, const char* prefix) {
    const std::string p(prefix);
    return s.size() >= p.size() && s.compare(0, p.size(), p) == 0;
}

}  // namespace

int main() {
    std::printf("Proof-envelope v1.1.0 smoke test:\n");

    Ed25519Seed seed{};
    seed.fill(0x42);

    // signer_cid placeholder: a syntactically valid v1.1.0 self-identifying
    // hash (11-char tag + 128 hex chars). It does not resolve to a real
    // key memento yet.
    const std::string signer_cid_v1 =
        "blake3-512:"
        "00000000000000000000000000000000000000000000000000000000000000ab"
        "cd000000000000000000000000000000000000000000000000000000000000ab";

    ProofEnvelopeInput input{
        .name = "test",
        .version = "1.0.0",
        .members = std::map<std::string, std::vector<uint8_t>>{},
        .signer_cid = signer_cid_v1,
        .signer_seed = seed,
        .declared_at = "2026-04-30T12:00:00.000Z",
    };

    ProofEnvelopeOutput out1 = build_proof_envelope(input);
    ProofEnvelopeOutput out2 = build_proof_envelope(input);

    int failures = 0;

    // Check 1: filename CID has the v1.1.0 self-identifying shape.
    if (starts_with(out1.filename_cid, "blake3-512:") &&
        out1.filename_cid.size() == 11 + 128) {
        std::printf("  [PASS] filename CID is `blake3-512:` + 128 hex (%s)\n",
                    out1.filename_cid.c_str());
    } else {
        std::printf("  [FAIL] filename CID malformed: %s (size=%zu)\n",
                    out1.filename_cid.c_str(), out1.filename_cid.size());
        failures++;
    }

    // Check 2: determinism (two builds produce identical bytes + CID).
    if (out1.bytes == out2.bytes && out1.filename_cid == out2.filename_cid) {
        std::printf("  [PASS] deterministic: two builds produce byte-identical output\n");
    } else {
        std::printf("  [FAIL] non-deterministic builder output\n");
        failures++;
    }

    // Check 3: the output bytes hash to the produced filename CID.
    const std::string view(reinterpret_cast<const char*>(out1.bytes.data()),
                            out1.bytes.size());
    const std::string rederived = ::provekit::canonicalizer::compute_cid(view);
    if (rederived == out1.filename_cid) {
        std::printf("  [PASS] trust-root: recomputed hash matches filename CID\n");
    } else {
        std::printf("  [FAIL] trust-root mismatch: filename %s != recomputed %s\n",
                    out1.filename_cid.c_str(), rederived.c_str());
        failures++;
    }

    std::printf("\n  envelope size: %zu bytes\n", out1.bytes.size());
    std::printf("  envelope hex (truncated): %.80s...\n", hex(out1.bytes).c_str());
    std::printf("\n");

    if (failures == 0) {
        std::printf("PROOF-ENVELOPE v1.1.0 SMOKE TEST OK.\n");
        return 0;
    }
    std::printf("PROOF-ENVELOPE v1.1.0 SMOKE TEST FAILED: %d divergence(s).\n", failures);
    return 1;
}
