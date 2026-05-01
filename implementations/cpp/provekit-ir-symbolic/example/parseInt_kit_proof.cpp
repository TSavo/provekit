// SPDX-License-Identifier: Apache-2.0
//
// End-to-end C++ kit-as-author: writes a real .proof file containing
// a contract memento (parseInt's pre/post) + a bridge memento
// (TS-layer parseInt → that contract), all signed, all in pure C++.
//
// The TS side never enters this flow. The C++ kit is self-contained:
//   1. Author calls kit primitives (contract + bridge_decl + register_primitive_bridge).
//   2. Kit primitives emit IR via the collector.
//   3. Each declaration is minted as a signed ClaimEnvelope.
//   4. Contracts minted first (so bridges can reference their CIDs).
//   5. Catalog memento bundles all members into a deterministic-CBOR .proof.
//   6. Filename = "blake3-512:" + 128 hex = full self-identifying CID = trust root.
//
// Spec stack (catalog v1.1.0):
//   protocol/specs/2026-04-30-proof-file-format.md
//   protocol/specs/2026-04-30-memento-envelope-grammar.md
//   protocol/specs/2026-04-29-universal-claim-envelope.md
//   protocol/specs/2026-04-30-handshake-algorithm.md

#include "provekit/ir.hpp"
#include "provekit/canonicalizer/hash.hpp"
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
using ::provekit::canonicalizer::compute_cid;
using ::provekit::claim_envelope::MintContractArgs;
using ::provekit::claim_envelope::MintBridgeArgs;
using ::provekit::claim_envelope::mint_contract;
using ::provekit::claim_envelope::mint_bridge;
using ::provekit::claim_envelope::formula_to_value;
using ::provekit::claim_envelope::AuthoringKind;
using ::provekit::claim_envelope::AuthoringKitAuthor;
using ::provekit::proof_envelope::Ed25519Seed;
using ::provekit::proof_envelope::ProofEnvelopeInput;
using ::provekit::proof_envelope::build_proof_envelope;

int main(int argc, char* argv[]) {
    const std::string out_dir = (argc >= 2) ? argv[1] : ".";

    // ----- 1. Author the contract via kit primitives -----
    reset_collector();
    begin_collecting();

    // parseInt's contract: pre says input non-empty would be the precondition,
    // but the v0 PoC uses pre = forall n. n > 0 over the (logical) Int input
    // to keep the cross-lang round-trip's existing arithmetic shape.
    must("parseInt",
         forall(Int(), [](std::shared_ptr<Term> n) {
             return gt(n, num(0));
         }));

    auto contract_decls = finish();

    // ----- 2. Mint each contract as a signed ClaimEnvelope -----
    Ed25519Seed signer_seed;
    signer_seed.fill(0x42);  // deterministic for the demo

    const std::string declared_at = "2026-04-30T12:00:00.000Z";
    const std::string produced_by = "cpp-kit@1.0";

    std::map<std::string, std::vector<uint8_t>> members;
    std::map<std::string, std::string> contract_name_to_cid;

    for (const auto& d : contract_decls) {
        MintContractArgs args{};
        args.contract_name = d.name;
        if (d.pre) args.pre = formula_to_value(*d.pre);
        if (d.post) args.post = formula_to_value(*d.post);
        if (d.inv) args.inv = formula_to_value(*d.inv);
        args.out_binding = d.outBinding;
        args.produced_by = produced_by;
        args.produced_at = declared_at;
        args.input_cids = {};
        args.authoring_kind = AuthoringKind::KitAuthor;
        args.authoring_kit_author = AuthoringKitAuthor{produced_by, ""};
        args.signer_seed = signer_seed;

        auto minted = mint_contract(args);
        std::printf("  contract minted: %s -> CID %s\n", d.name.c_str(), minted.cid.c_str());
        members[minted.cid] = minted.canonical_bytes;
        contract_name_to_cid[d.name] = minted.cid;
    }

    // ----- 3. Mint the bridge: parseInt (TS surface) -> contract memento -----
    const std::string parseint_target_cid = contract_name_to_cid["parseInt"];
    if (parseint_target_cid.empty()) {
        std::fprintf(stderr, "ERROR: parseInt contract was not minted\n");
        return 1;
    }
    MintBridgeArgs bridge_args{};
    bridge_args.produced_by = produced_by;
    bridge_args.produced_at = declared_at;
    bridge_args.source_symbol = "parseInt";
    bridge_args.source_layer = "ts";
    bridge_args.target_contract_cid = parseint_target_cid;
    bridge_args.target_layer = "cpp-kit";
    bridge_args.ir_arg_sorts = {"String"};
    bridge_args.ir_return_sort = "Int";
    bridge_args.notes = "";
    bridge_args.signer_seed = signer_seed;
    auto minted_bridge = mint_bridge(bridge_args);
    std::printf("  bridge   minted: parseInt -> CID %s\n", minted_bridge.cid.c_str());
    members[minted_bridge.cid] = minted_bridge.canonical_bytes;

    // ----- 4. Bundle the catalog into a .proof file -----
    // signer_cid is a placeholder for the v1.1.0 demo: a syntactically
    // valid self-identifying hash ("blake3-512:" + 128 hex chars). It
    // does not resolve to a real key memento yet.
    ProofEnvelopeInput proof_input{
        .name = "@example/cpp-kit",
        .version = "1.0.0",
        .members = members,
        .signer_cid =
            "blake3-512:"
            "63707020d09b8c5cab0fb7d6a1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f"
            "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
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
