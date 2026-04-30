// SPDX-License-Identifier: Apache-2.0
//
// Cross-language conformance test. Asserts the C++ proof-envelope
// builder produces byte-identical output to the TS reference for a
// fixed deterministic input.
//
// Fixture (frozen so both impls have the same target):
//   name        = "test"
//   version     = "1.0.0"
//   declaredAt  = "2026-04-30T12:00:00.000Z"
//   members     = {} (empty)
//   signerCid   = "sha256:000000000000abcd"
//   signerSeed  = 32 bytes of 0x42
//
// Expected values computed by running TS buildProofEnvelope on these
// inputs (see scripts/cross-lang-equivalence/proof-envelope.fixture.json).
// Both impls derive from RFC 8949 §4.2.1 (deterministic CBOR) +
// RFC 8032 (ed25519); byte-identical output proves both conform.

#include <array>
#include <cstdio>
#include <cstdint>
#include <map>
#include <string>
#include <vector>

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

constexpr const char* EXPECTED_HEX =
    "a7"  // map of 7
    "646b696e64"               // tstr "kind"
    "67636174616c6f67"         // tstr "catalog"
    "646e616d65"               // tstr "name"
    "6474657374"               // tstr "test"
    "667369676e6572"           // tstr "signer"
    "777368613235363a30303030303030303030303061626364"  // tstr "sha256:000...abcd"
    "676d656d62657273"         // tstr "members"
    "a0"                       // map of 0
    "6776657273696f6e"         // tstr "version"
    "65312e302e30"             // tstr "1.0.0"
    "697369676e6174757265"     // tstr "signature"
    "5840"                     // bstr length 64
    "bfe9e1301bea39c4662a695dedd4a2249ad33ce458d2dc7aa3a698f46f2e345a"
    "d6470f54271016d9fd9034fa4932d0f947240263054eea8fc468a7ac03ec5b0b"
    "6a6465636c6172656441 74"    // tstr "declaredAt"   <-- whitespace fixed below
    "7818"                       // tstr length 24
    "323032362d30342d33305431323a30303a30302e3030305a";

constexpr const char* EXPECTED_FILENAME_CID =
    "eaad3b93b9e8c0635dcde31aac893565";

}  // namespace

int main() {
    std::printf("Cross-language proof-envelope conformance test:\n");

    Ed25519Seed seed{};
    seed.fill(0x42);

    ProofEnvelopeInput input{
        .name = "test",
        .version = "1.0.0",
        .members = std::map<std::string, std::vector<uint8_t>>{},
        .signer_cid = "sha256:000000000000abcd",
        .signer_seed = seed,
        .declared_at = "2026-04-30T12:00:00.000Z",
    };

    ProofEnvelopeOutput out = build_proof_envelope(input);

    // Strip whitespace from expected hex (we used spaces above for readability).
    std::string want;
    want.reserve(400);
    for (char c : std::string(EXPECTED_HEX)) {
        if (c == ' ') continue;
        want.push_back(c);
    }

    const std::string got = hex(out.bytes);

    int failures = 0;
    if (got == want) {
        std::printf("  [PASS] envelope bytes match TS reference (%zu bytes)\n", out.bytes.size());
    } else {
        std::printf("  [FAIL] envelope bytes diverge\n");
        std::printf("    got:  %s\n", got.c_str());
        std::printf("    want: %s\n", want.c_str());
        failures++;
    }

    if (out.filename_cid == EXPECTED_FILENAME_CID) {
        std::printf("  [PASS] filename CID matches: %s\n", out.filename_cid.c_str());
    } else {
        std::printf("  [FAIL] filename CID diverges\n");
        std::printf("    got:  %s\n", out.filename_cid.c_str());
        std::printf("    want: %s\n", EXPECTED_FILENAME_CID);
        failures++;
    }

    std::printf("\n");
    if (failures == 0) {
        std::printf("PROOF-ENVELOPE CONFORMANCE OK — C++ matches TS byte-for-byte.\n");
        return 0;
    }
    std::printf("PROOF-ENVELOPE CONFORMANCE FAILED — %d divergence(s).\n", failures);
    return 1;
}
