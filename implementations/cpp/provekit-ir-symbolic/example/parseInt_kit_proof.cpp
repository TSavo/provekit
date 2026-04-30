// SPDX-License-Identifier: Apache-2.0
//
// End-to-end C++ kit-as-author: writes a real .proof file containing
// a property memento (parseInt's precondition) + a bridge memento
// (TS-layer parseInt → that property), all signed, all in pure C++.
//
// The TS side never enters this flow. The C++ kit is self-contained:
//   1. Author calls kit primitives (must + bridge_decl + register_primitive_bridge).
//   2. Kit primitives emit IR via the collector.
//   3. Each declaration is minted as a signed ClaimEnvelope.
//   4. Properties minted first (so bridges can reference their CIDs).
//   5. Catalog memento bundles all members → deterministic-CBOR .proof.
//   6. Filename = sha256(bytes)[:32 hex] = trust root.
//
// Spec stack:
//   protocol/specs/2026-04-30-proof-file-format.md
//   protocol/specs/2026-04-30-memento-envelope-grammar.md
//   protocol/specs/2026-04-29-universal-claim-envelope.md

#include "provekit/ir.hpp"
#include "provekit/canonicalizer/sha256.hpp"
#include "provekit/canonicalizer/value.hpp"
#include "provekit/claim-envelope/mint.hpp"
#include "provekit/claim-envelope/value_from_kit.hpp"
#include "provekit/proof-envelope/proof_envelope.hpp"

#include <cstdio>
#include <cstring>
#include <fstream>
#include <map>
#include <string>
#include <vector>

using namespace provekit::ir;
using ::provekit::canonicalizer::Value;
using ::provekit::canonicalizer::ValuePtr;
using ::provekit::canonicalizer::sha256_hex;
using ::provekit::claim_envelope::MintPropertyArgs;
using ::provekit::claim_envelope::MintBridgeArgs;
using ::provekit::claim_envelope::mint_property;
using ::provekit::claim_envelope::mint_bridge;
using ::provekit::claim_envelope::formula_to_value;
using ::provekit::proof_envelope::Ed25519Seed;
using ::provekit::proof_envelope::ProofEnvelopeInput;
using ::provekit::proof_envelope::build_proof_envelope;

namespace {

std::string hash16(const std::string& s) {
    return sha256_hex(s).substr(0, 16);
}

}  // namespace

int main(int argc, char* argv[]) {
    const std::string out_dir = (argc >= 2) ? argv[1] : ".";

    // ----- 1. Author the invariants using kit primitives -----
    reset_collector();
    begin_collecting();

    must("parseInt-requires-positive",
        forall(Int(), [](std::shared_ptr<Term> n) {
            return gt(n, num(0));
        }));

    auto property_decls = finish();

    // ----- 2. Mint each property as a signed ClaimEnvelope -----
    Ed25519Seed signer_seed;
    signer_seed.fill(0x42);  // deterministic for the demo

    const std::string declared_at = "2026-04-30T12:00:00.000Z";
    const std::string produced_by = "cpp-kit@1.0";
    const std::string ir_kit_version = "cpp-kit@1.0";

    std::map<std::string, std::vector<uint8_t>> members;
    std::map<std::string, std::string> property_name_to_cid;

    for (const auto& d : property_decls) {
        ValuePtr scope = Value::object({
            {"kind", Value::string("function")},
            {"name", Value::string(d.name)},
        });
        MintPropertyArgs args{
            .binding_hash = hash16("cpp-kit-property:" + d.name),
            .property_hash = hash16("hash-of:" + d.name),
            .produced_by = produced_by,
            .produced_at = declared_at,
            .input_cids = {},
            .ir_formula = formula_to_value(*d.formula),
            .scope = scope,
            .ir_kit_version = ir_kit_version,
            .signer_seed = signer_seed,
        };
        auto minted = mint_property(args);
        std::printf("  property minted: %s -> CID %s\n", d.name.c_str(), minted.cid.c_str());
        members[minted.cid] = minted.canonical_bytes;
        property_name_to_cid[d.name] = minted.cid;
    }

    // ----- 3. Mint the bridge: parseInt (TS surface) -> property memento -----
    const std::string parseint_target_cid = property_name_to_cid["parseInt-requires-positive"];
    if (parseint_target_cid.empty()) {
        std::fprintf(stderr, "ERROR: parseInt-requires-positive property was not minted\n");
        return 1;
    }
    MintBridgeArgs bridge_args{
        .binding_hash = hash16("ts:parseInt"),
        .property_hash = hash16("bridge:parseInt"),
        .produced_by = produced_by,
        .produced_at = declared_at,
        .source_symbol = "parseInt",
        .source_layer = "ts",
        .target_contract_cid = parseint_target_cid,
        .target_layer = "cpp-kit",
        .ir_arg_sorts = {"String"},
        .ir_return_sort = "Int",
        .notes = "",
        .signer_seed = signer_seed,
    };
    auto minted_bridge = mint_bridge(bridge_args);
    std::printf("  bridge   minted: parseInt -> CID %s\n", minted_bridge.cid.c_str());
    members[minted_bridge.cid] = minted_bridge.canonical_bytes;

    // ----- 4. Bundle the catalog into a .proof file -----
    ProofEnvelopeInput proof_input{
        .name = "@example/cpp-kit",
        .version = "1.0.0",
        .members = members,
        .signer_cid = "sha256:cpp-kit-signer",
        .signer_seed = signer_seed,
        .declared_at = declared_at,
    };
    auto built = build_proof_envelope(proof_input);

    // ----- 5. Write <cid>.proof to disk -----
    const std::string out_path = out_dir + "/" + built.filename_cid + ".proof";
    std::ofstream f(out_path, std::ios::binary);
    if (!f) {
        std::fprintf(stderr, "ERROR: cannot open %s for writing\n", out_path.c_str());
        return 1;
    }
    f.write(reinterpret_cast<const char*>(built.bytes.data()), built.bytes.size());
    if (!f) {
        std::fprintf(stderr, "ERROR: write to %s failed\n", out_path.c_str());
        return 1;
    }
    std::printf("\n  wrote .proof: %s (%zu bytes, cid=%s)\n",
                out_path.c_str(), built.bytes.size(), built.filename_cid.c_str());
    return 0;
}
