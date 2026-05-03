// SPDX-License-Identifier: Apache-2.0
//
// Phase-2 cross-kit bridge test: cpp kit attestation that the cpp lift
// adapter satisfies the Rust kit's `lift_plugin_protocol` contracts.
//
// For each of the 10 contracts in the Rust self-contracts bundle (see
// `implementations/rust/provekit-self-contracts/src/lift_plugin_protocol.rs`),
// this test:
//
//   1. Builds the 10 BridgeDecls via build_lift_plugin_protocol_bridges()
//      (each links rust source CID -> cpp counterpart name placeholder,
//      with target_proof_cid = `deferred:phase-3-proof-bundle`).
//   2. Marshals the array via JCS-alphabetical write_bridge_decl emitter.
//   3. Asserts the BLAKE3-512 of those bytes matches the pinned golden.
//
// On rust drift (rare but possible if the rust lift_plugin_protocol slab
// is re-touched), the pinned `rust_contract_cid()` map will diverge from
// the new mint output; the bridge bytes change, the pinned bridge-array
// CID changes, this test fails. That's the desired behavior: drift must
// be visible, not silent.
//
// The pinned hash is independent of the cpp bundle CID. The bundle CID
// drifts because the slab adds 10 counterpart contracts to the mint;
// the attestation re-sign is a follow-up tracked in the PR body.
//
// Spec sources:
//   protocol/specs/2026-04-30-lift-plugin-protocol.md (the 10 rules)
//   protocol/specs/2026-04-30-ir-formal-grammar.md §BridgeDeclaration
//   protocol/specs/2026-05-02-binary-attestation-protocol.md (phase-3)

#include "provekit/ir.hpp"
#include "provekit/canonicalizer/hash.hpp"

#include "cross_kit_bridges.hpp"

#include <cstdio>
#include <string>
#include <vector>

using ::provekit::canonicalizer::compute_cid;
using ::provekit::ir::BridgeDecl;
namespace ckb = provekit::cross_kit_bridges;

namespace {

bool check_eq_str(const char* label, const std::string& got, const std::string& want) {
    if (got == want) {
        std::printf("  [PASS] %s\n", label);
        return true;
    }
    std::printf("  [FAIL] %s\n", label);
    std::printf("         got:  %s\n", got.c_str());
    std::printf("         want: %s\n", want.c_str());
    return false;
}

bool starts_with(const std::string& s, const std::string& prefix) {
    return s.size() >= prefix.size() &&
           s.compare(0, prefix.size(), prefix) == 0;
}

bool is_lowercase_hex(const std::string& s) {
    for (char c : s) {
        if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f'))) return false;
    }
    return true;
}

}  // namespace

// Pinned BLAKE3-512 over the JCS-canonical bytes of the 10 phase-2
// BridgeDecls returned by ckb::build_lift_plugin_protocol_bridges().
//
// Drift in any of the following invalidates this hash:
//   - rust contract memento CID for any of the 10 lift-plugin-protocol
//     contracts (rust_contract_cid() in cross_kit_bridges.cpp)
//   - bridge name spelling (bridge_to_<rust_name>)
//   - cpp counterpart name spelling (cpp_<rust_name>)
//   - source_layer / target_layer / notes literals
//   - kDeferredCppLiftBinaryCid ("deferred:phase-3-proof-bundle")
//   - declaration order
//   - JCS write_bridge_decl emitter
//
// Computed at PR-authoring time over the BridgeDecl array returned by
// build_lift_plugin_protocol_bridges() with target_contract_cid carrying
// the `pending-cpp-counterpart:<name>` placeholder. The cpp orchestrator
// has no bridge-resolution pass yet (that's phase-3 work), so the
// placeholder shape IS what's frozen here: the bridge-list is content-
// addressable independent of the transient cpp bundle's internal CIDs.
static const char* kPinnedBridgeArrayCid =
    "blake3-512:5815db0b992594bf120317c5e4a3f25132574ba3615106fd20a81f2dc8c1a4cf"
    "1d37bae9753a6056ef8e9b53d4405aa1c36de24236437ba9baefc296949c91a6";

int main() {
    int failures = 0;
    std::printf("phase-2 cross-kit bridges test (cpp peer):\n\n");

    // --- 1. Bridge count ---------------------------------------------------
    auto bridges = ckb::build_lift_plugin_protocol_bridges();
    if (bridges.size() != 10) {
        std::printf("  [FAIL] expected 10 bridges, got %zu\n", bridges.size());
        failures++;
        return 1;  // bail; subsequent checks assume 10
    }
    std::printf("  [PASS] bridge count == 10\n");

    // --- 2. Bridge name + counterpart prefix invariants --------------------
    bool prefix_ok = true;
    for (const auto& b : bridges) {
        if (!starts_with(b.name, "bridge_to_lift_plugin_")) {
            std::printf("  [FAIL] bridge name must start with "
                        "'bridge_to_lift_plugin_'; got %s\n",
                        b.name.c_str());
            prefix_ok = false;
        }
        if (!starts_with(b.source_symbol, "lift_plugin_")) {
            std::printf("  [FAIL] bridge %s: source_symbol must start with "
                        "'lift_plugin_'; got %s\n",
                        b.name.c_str(), b.source_symbol.c_str());
            prefix_ok = false;
        }
    }
    if (prefix_ok) {
        std::printf("  [PASS] all bridge + source names use lift_plugin_ prefix\n");
    } else {
        failures++;
    }

    // --- 3. Source/target layers, notes, target_proof_cid ------------------
    bool fields_ok = true;
    for (const auto& b : bridges) {
        if (b.source_layer != "rust-kit") {
            std::printf("  [FAIL] bridge %s: source_layer = %s, want rust-kit\n",
                        b.name.c_str(), b.source_layer.c_str());
            fields_ok = false;
        }
        if (b.target_layer != "cpp-kit") {
            std::printf("  [FAIL] bridge %s: target_layer = %s, want cpp-kit\n",
                        b.name.c_str(), b.target_layer.c_str());
            fields_ok = false;
        }
        if (b.target_proof_cid != "deferred:phase-3-proof-bundle") {
            std::printf("  [FAIL] bridge %s: target_proof_cid = %s, "
                        "want deferred:phase-3-proof-bundle\n",
                        b.name.c_str(), b.target_proof_cid.c_str());
            fields_ok = false;
        }
        if (b.notes != "lift-plugin-protocol conformance bridge; phase 2") {
            std::printf("  [FAIL] bridge %s: notes = %s\n",
                        b.name.c_str(), b.notes.c_str());
            fields_ok = false;
        }
    }
    if (fields_ok) {
        std::printf("  [PASS] all bridges carry expected layers + notes + "
                    "deferred target_proof_cid\n");
    } else {
        failures++;
    }

    // --- 4. Source contract CIDs are well-formed BLAKE3-512 ----------------
    bool cids_ok = true;
    const std::string want_prefix = "blake3-512:";
    for (const auto& b : bridges) {
        if (!starts_with(b.source_contract_cid, want_prefix)) {
            std::printf("  [FAIL] bridge %s: source_contract_cid missing "
                        "blake3-512: prefix\n", b.name.c_str());
            cids_ok = false;
            continue;
        }
        const std::string hex = b.source_contract_cid.substr(want_prefix.size());
        if (hex.size() != 128) {
            std::printf("  [FAIL] bridge %s: source_contract_cid hex length "
                        "%zu, want 128\n", b.name.c_str(), hex.size());
            cids_ok = false;
            continue;
        }
        if (!is_lowercase_hex(hex)) {
            std::printf("  [FAIL] bridge %s: source_contract_cid hex not "
                        "lowercase\n", b.name.c_str());
            cids_ok = false;
        }
    }
    if (cids_ok) {
        std::printf("  [PASS] all 10 source_contract_cid values are "
                    "well-formed blake3-512:<128-hex>\n");
    } else {
        failures++;
    }

    // --- 5. Pinned BLAKE3-512 of the JCS-canonical bridges array -----------
    //
    // Load-bearing pin: any drift in rust source CIDs, bridge field
    // encoding, or declaration order surfaces here.
    const std::string jcs_bytes = ckb::marshal_bridges(bridges);
    const std::string got_cid = compute_cid(jcs_bytes);

    if (!check_eq_str("phase-2 cpp bridges JCS hash matches pin",
                       got_cid, std::string(kPinnedBridgeArrayCid))) {
        std::printf("\n         If this drift is intentional, update "
                    "kPinnedBridgeArrayCid in this file and re-sign\n"
                    "         .provekit/self-contracts-attestations/cpp.json "
                    "in a follow-up commit.\n");
        std::printf("\n         marshalled bytes (%zu chars):\n  %s\n",
                    jcs_bytes.size(), jcs_bytes.c_str());
        failures++;
    }

    std::printf("\n");
    if (failures == 0) {
        std::printf("PHASE-2 CROSS-KIT BRIDGES (cpp) OK.\n");
        return 0;
    }
    std::printf("PHASE-2 CROSS-KIT BRIDGES (cpp) FAILED: %d divergence(s).\n",
                failures);
    return 1;
}
