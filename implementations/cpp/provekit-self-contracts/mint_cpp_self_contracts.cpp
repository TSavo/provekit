// SPDX-License-Identifier: Apache-2.0
//
// mint_cpp_self_contracts — the C++ peer self-contracts orchestrator.
//
// Walks every .invariant.cpp file in the C++ workspace by calling its
// extern "C" registrar, mints all collected contracts as signed
// mementos, bundles into a `.proof` whose filename IS its catalog CID,
// asserts byte-determinism by minting twice into separate output dirs.
//
// Mirrors implementations/rust/provekit-self-contracts/src/bin/mint-self-contracts.rs.
//
// Run:
//   tools/build-cpp-self-contracts.sh
//   ./mint_cpp_self_contracts /tmp/cpp-self-out
//
// The protocol is the bytes. The minted .proof is conformant with the
// catalog (v1.1.0) and verifies under the same foundation key as the
// Rust and Go peers.

#include "provekit/ir.hpp"
#include "provekit/canonicalizer/value.hpp"
#include "provekit/claim-envelope/mint.hpp"
#include "provekit/claim-envelope/value_from_kit.hpp"
#include "provekit/proof-envelope/proof_envelope.hpp"

#include <cstdio>
#include <fstream>
#include <map>
#include <string>
#include <vector>

using namespace provekit::ir;
using ::provekit::claim_envelope::MintContractArgs;
using ::provekit::claim_envelope::mint_contract;
using ::provekit::claim_envelope::formula_to_value;
using ::provekit::claim_envelope::AuthoringKind;
using ::provekit::claim_envelope::AuthoringKitAuthor;
using ::provekit::proof_envelope::Ed25519Seed;
using ::provekit::proof_envelope::ProofEnvelopeInput;
using ::provekit::proof_envelope::build_proof_envelope;

// Each .invariant.cpp file defines an extern "C" registrar that calls
// must()/contract() to push ContractDecls into the kit-side collector.
// Declare them here; link order is per the build script.
extern "C" {
    void jcs_invariants();
    void hash_invariants();
    void property_hash_invariants();
    void cbor_invariants();
    void sign_invariants();
    void proof_envelope_invariants();
    void mint_invariants();
    void load_all_proofs_invariants();
    void enumerate_callsites_invariants();
    void resolve_target_invariants();
    void instantiate_invariants();
}

static std::string mint_one_run(const std::string& out_dir, bool verbose) {
    // 1. Author every .invariant.cpp module via its registrar.
    reset_collector();
    begin_collecting();
    jcs_invariants();
    hash_invariants();
    property_hash_invariants();
    cbor_invariants();
    sign_invariants();
    proof_envelope_invariants();
    mint_invariants();
    load_all_proofs_invariants();
    enumerate_callsites_invariants();
    resolve_target_invariants();
    instantiate_invariants();
    auto contract_decls = finish();

    if (verbose) {
        std::printf("authored %zu contracts across 11 .invariant.cpp slabs\n",
                    contract_decls.size());
    }

    // 2. Mint each as a signed ClaimEnvelope under the foundation key.
    Ed25519Seed signer_seed;
    signer_seed.fill(0x42);

    const std::string declared_at = "2026-04-30T12:00:00.000Z";
    const std::string produced_by = "cpp-kit@1.0";

    std::map<std::string, std::vector<uint8_t>> members;
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
        members[minted.cid] = minted.canonical_bytes;
    }

    // 3. Bundle the catalog into a deterministic-CBOR .proof.
    ProofEnvelopeInput proof_input{
        .name = "@example/cpp-self-contracts",
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

    // 4. Write <cid>.proof to disk.
    const std::string out_path = out_dir + "/" + built.filename_cid + ".proof";
    std::ofstream f(out_path, std::ios::binary);
    if (!f) {
        std::fprintf(stderr, "ERROR: cannot open %s for writing\n", out_path.c_str());
        std::exit(1);
    }
    f.write(reinterpret_cast<const char*>(built.bytes.data()),
            static_cast<std::streamsize>(built.bytes.size()));
    if (!f) {
        std::fprintf(stderr, "ERROR: write to %s failed\n", out_path.c_str());
        std::exit(1);
    }

    if (verbose) {
        std::printf("  catalog CID:        %s\n", built.filename_cid.c_str());
        std::printf("  proof bytes:        %zu\n", built.bytes.size());
        std::printf("  .proof file:        %s\n", out_path.c_str());
    }

    return built.filename_cid;
}

int main(int argc, char* argv[]) {
    const std::string out_dir = (argc >= 2) ? argv[1] : ".";

    std::printf("== ProvekIt C++ self-contracts orchestrator ==\n\n");
    std::printf("output dir: %s\n\n", out_dir.c_str());

    std::printf("== mint #1 ==\n");
    const std::string cid1 = mint_one_run(out_dir, /*verbose=*/true);

    // Determinism check: mint twice into separate output dirs and
    // assert byte-equality. The .proof filename IS the catalog CID;
    // matching filenames means matching bytes.
    const std::string out_dir2 = out_dir + "/_determinism_check";
    if (std::system(("mkdir -p '" + out_dir2 + "'").c_str()) != 0) {
        std::fprintf(stderr, "ERROR: cannot create %s\n", out_dir2.c_str());
        return 1;
    }
    std::printf("\n== mint #2 (determinism check) ==\n");
    const std::string cid2 = mint_one_run(out_dir2, /*verbose=*/false);
    if (cid1 != cid2) {
        std::fprintf(stderr,
                     "DETERMINISM FAILURE:\n  run 1 cid: %s\n  run 2 cid: %s\n",
                     cid1.c_str(), cid2.c_str());
        return 1;
    }
    std::printf("  determinism check:  OK (two runs produced identical CIDs)\n");

    std::printf("\n== done. C++ self-application: live. ==\n");
    return 0;
}
