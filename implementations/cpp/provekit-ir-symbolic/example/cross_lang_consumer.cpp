// SPDX-License-Identifier: Apache-2.0
//
// THE C++ END-TO-END DEMO (REVERSE DIRECTION).
//
//   C++ signs C++.
//   C++ calls Go (via the bridged kit primitive parseInt).
//   C++ detects parseInt(num(0)).
//
// Architecture mirror:
//   1. Go kit shipped a .proof file with parseInt's precondition
//      (forall n: Int. n > 0): produced by cmd/go-kit-publish.
//   2. C++ consumer authors invariants via kit primitives parse_int(num(...))
//: every call emits a Ctor("parseInt", [arg]) IrTerm.
//   3. C++ consumer mints + signs its property mementos in pure C++.
//   4. C++ consumer bundles them into its own .proof file in pure C++.
//   5. C++ bridge enforcement runner walks both .proofs:
//        - load-all-proofs builds a unified CID pool.
//        - enumerate-callsites finds Ctor("parseInt", ...) inside C++'s properties.
//        - resolve-bridge-target hash-looks-up the bridge → Go's property memento.
//        - instantiate-obligation substitutes the call's arg into `forall n. n > 0`.
//        - solve-obligation invokes z3 (parallel via std::async).
//        - report aggregates.
//
//   parse_int(num(5)) → instantiate `5 > 0` → unsat(¬(5 > 0)) → DISCHARGED
//   parse_int(num(0)) → instantiate `0 > 0` → sat(¬(0 > 0))   → UNSATISFIED
//
// C++ imports zero lines of Go. The connection is the protocol: bytes
// the Go kit produced, walked by the C++ verifier, closed by Z3.

#include "provekit/ir.hpp"
#include "provekit/canonicalizer/hash.hpp"
#include "provekit/canonicalizer/value.hpp"
#include "provekit/claim-envelope/mint.hpp"
#include "provekit/claim-envelope/value_from_kit.hpp"
#include "provekit/proof-envelope/proof_envelope.hpp"
#include "provekit/verifier/runner.hpp"
#include "provekit/verifier/types.hpp"

#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <map>
#include <string>
#include <vector>
#include <unistd.h>

namespace fs = std::filesystem;
using namespace provekit::ir;
using ::provekit::canonicalizer::Value;
using ::provekit::canonicalizer::ValuePtr;
using ::provekit::canonicalizer::compute_cid;
using ::provekit::claim_envelope::MintContractArgs;
using ::provekit::claim_envelope::mint_contract;
using ::provekit::claim_envelope::formula_to_value;
using ::provekit::claim_envelope::AuthoringKind;
using ::provekit::claim_envelope::AuthoringKitAuthor;
using ::provekit::proof_envelope::Ed25519Seed;
using ::provekit::proof_envelope::ProofEnvelopeInput;
using ::provekit::proof_envelope::build_proof_envelope;

namespace {

bool copy_file(const std::string& src, const std::string& dst) {
    std::ifstream in(src, std::ios::binary);
    if (!in) return false;
    fs::create_directories(fs::path(dst).parent_path());
    std::ofstream out(dst, std::ios::binary);
    if (!out) return false;
    out << in.rdbuf();
    return out.good();
}

}  // namespace

int main(int argc, char* argv[]) {
    if (argc < 2) {
        std::fprintf(stderr, "Usage: %s <go-proof-path>\n", argv[0]);
        return 2;
    }
    const std::string go_proof_path = argv[1];

    if (!fs::exists(go_proof_path)) {
        std::fprintf(stderr,
                     "ERROR: Go .proof not found at %s: run go-kit-publish first.\n",
                     go_proof_path.c_str());
        return 2;
    }

    // ---- 1. Lay out a project_root with the Go .proof in node_modules ----
    char tmpl[] = "/tmp/cpp-cross-lang-XXXXXX";
    if (!mkdtemp(tmpl)) {
        std::fprintf(stderr, "ERROR: mkdtemp failed\n");
        return 1;
    }
    const std::string project_root = tmpl;
    const std::string go_dest =
        project_root + "/node_modules/@example/go-kit/" + fs::path(go_proof_path).filename().string();
    if (!copy_file(go_proof_path, go_dest)) {
        std::fprintf(stderr, "ERROR: failed to copy Go .proof to %s\n", go_dest.c_str());
        return 1;
    }
    std::printf("  installed Go .proof at: %s\n", go_dest.c_str());

    // ---- 2. Author C++-side invariants ----
    reset_collector();
    begin_collecting();
    ensure_kit_bridges_registered();

    // parse_int(num(5)): should DISCHARGE
    must("calls-parseInt-with-positive-5",
         eq(parse_int(num(5)), num(5)));
    // parse_int(num(0)): should be UNSATISFIED (caught by Go's precondition)
    must("calls-parseInt-with-zero",
         eq(parse_int(num(0)), num(0)));

    auto decls = finish();
    if (decls.size() != 2) {
        std::fprintf(stderr, "ERROR: expected 2 declarations, got %zu\n", decls.size());
        return 1;
    }

    // ---- 3. Mint each contract memento (C++ signs C++) ----
    Ed25519Seed signer_seed;
    signer_seed.fill(0x37);
    const std::string declared_at = "2026-04-30T15:00:00.000Z";
    const std::string produced_by = "cpp-consumer@1";

    std::map<std::string, std::vector<uint8_t>> members;
    for (const auto& d : decls) {
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
    }

    // ---- 4. Bundle the consumer's .proof file ----
    Ed25519Seed catalog_seed;
    catalog_seed.fill(0x73);
    ProofEnvelopeInput proof_input{
        .name = "@example/cpp-consumer",
        .version = "1.0.0",
        .members = members,
        .signer_cid =
            "blake3-512:"
            "63707020636f6e73756d65720000000000000000000000000000000000000000"
            "0000000000000000000000000000000000000000000000000000000000000000",
        .signer_seed = catalog_seed,
        .declared_at = declared_at,
    };
    auto built = build_proof_envelope(proof_input);
    const std::string consumer_path = project_root + "/" + built.filename_cid + ".proof";
    {
        std::ofstream f(consumer_path, std::ios::binary);
        if (!f) {
            std::fprintf(stderr, "ERROR: cannot write %s\n", consumer_path.c_str());
            return 1;
        }
        f.write(reinterpret_cast<const char*>(built.bytes.data()), built.bytes.size());
    }
    std::printf("  C++ consumer .proof: %s (%zu bytes)\n", consumer_path.c_str(), built.bytes.size());

    // ---- 5. Run the C++ bridge enforcement runner ----
    provekit::verifier::Runner runner({project_root, "z3"});
    auto report = runner.Run();

    for (const auto& le : report.load_errors) {
        std::printf("  load error: %s: %s\n", le.proof_path.c_str(), le.reason.c_str());
    }

    int rc = 0;
    if (report.total_callsites != 2) {
        std::fprintf(stderr, "FAIL: expected 2 callsites, got %d\n", report.total_callsites);
        rc = 1;
    }

    const provekit::verifier::ReportRow* passing = nullptr;
    const provekit::verifier::ReportRow* failing = nullptr;
    for (const auto& row : report.rows) {
        std::printf("    %s: %s%s%s\n",
                    row.callsite.property_name.c_str(),
                    row.status.c_str(),
                    row.reason.empty() ? "" : ": ",
                    row.reason.c_str());
        if (row.callsite.property_name == "calls-parseInt-with-positive-5") passing = &row;
        if (row.callsite.property_name == "calls-parseInt-with-zero") failing = &row;
    }

    if (!passing) { std::fprintf(stderr, "FAIL: missing positive-5 row\n"); rc = 1; }
    else if (passing->status != "discharged") {
        std::fprintf(stderr, "FAIL: parse_int(num(5)) status = %s, want discharged\n",
                     passing->status.c_str());
        rc = 1;
    }
    if (!failing) { std::fprintf(stderr, "FAIL: missing zero row\n"); rc = 1; }
    else if (failing->status != "unsatisfied") {
        std::fprintf(stderr, "FAIL: parse_int(num(0)) status = %s, want unsatisfied\n",
                     failing->status.c_str());
        rc = 1;
    }

    if (rc == 0) {
        std::printf("\n  ✓ DEMO: C++ verifier caught parse_int(num(0)) using the Go-authored precondition.\n"
                    "    Discharged calls:  %d\n"
                    "    Caught violations: %d\n",
                    report.discharged, report.violations);
    }

    fs::remove_all(project_root);
    return rc;
}
