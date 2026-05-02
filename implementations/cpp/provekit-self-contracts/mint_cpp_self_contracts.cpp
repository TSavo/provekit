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
#include "provekit/canonicalizer/hash.hpp"
#include "provekit/canonicalizer/value.hpp"
#include "provekit/claim-envelope/mint.hpp"
#include "provekit/claim-envelope/value_from_kit.hpp"
#include "provekit/proof-envelope/proof_envelope.hpp"
#include "provekit/proof-envelope/sign_ed25519.hpp"

#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <map>
#include <sstream>
#include <string>
#include <vector>
#include <unistd.h>

using namespace provekit::ir;
using ::provekit::claim_envelope::MintContractArgs;
using ::provekit::claim_envelope::mint_contract;
using ::provekit::claim_envelope::formula_to_value;
using ::provekit::claim_envelope::AuthoringKind;
using ::provekit::claim_envelope::AuthoringKitAuthor;
using ::provekit::proof_envelope::Ed25519Seed;
using ::provekit::proof_envelope::ProofEnvelopeInput;
using ::provekit::proof_envelope::build_proof_envelope;
using ::provekit::proof_envelope::ed25519_pubkey_string_from_seed;
using ::provekit::canonicalizer::compute_cid;

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
    //
    // signer_cid = BLAKE3-512(JCS-canonical bytes of the signer's
    // self-identifying pubkey string `ed25519:<base64>`). This matches
    // the protocol-correct pattern Rust/Go/TS/C# peers use, NOT the
    // hardcoded placeholder the parseInt_kit_proof example carried.
    // The .proof's signer_cid now actually corresponds to the
    // foundation v0 key bytes; a verifier walking signer_cid -> key
    // memento gets the right answer.
    const std::string signer_pubkey = ed25519_pubkey_string_from_seed(signer_seed);
    const std::string signer_cid = compute_cid(signer_pubkey);

    ProofEnvelopeInput proof_input{
        .name = "@example/cpp-self-contracts",
        .version = "1.0.0",
        .members = members,
        .signer_cid = signer_cid,
        .signer_seed = signer_seed,
        .declared_at = declared_at,
        // Self-contracts bundle does not back-pin a binary; emit absent.
        .binary_cid = "",
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

// Base64 (standard) encoder for binary -> JSON-safe string. Sufficient
// for proof-envelope payload over JSON-RPC. No padding tricks, no URL-safe
// variant; matches Rust/Go base64::engine::general_purpose::STANDARD.
static std::string base64_encode(const std::vector<uint8_t>& data) {
    static const char alphabet[] =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    std::string out;
    out.reserve(((data.size() + 2) / 3) * 4);
    size_t i = 0;
    for (; i + 3 <= data.size(); i += 3) {
        uint32_t triple = (uint32_t(data[i]) << 16) | (uint32_t(data[i+1]) << 8) | uint32_t(data[i+2]);
        out.push_back(alphabet[(triple >> 18) & 0x3F]);
        out.push_back(alphabet[(triple >> 12) & 0x3F]);
        out.push_back(alphabet[(triple >> 6) & 0x3F]);
        out.push_back(alphabet[triple & 0x3F]);
    }
    if (i < data.size()) {
        uint32_t triple = uint32_t(data[i]) << 16;
        if (i + 1 < data.size()) triple |= uint32_t(data[i+1]) << 8;
        out.push_back(alphabet[(triple >> 18) & 0x3F]);
        out.push_back(alphabet[(triple >> 12) & 0x3F]);
        out.push_back((i + 1 < data.size()) ? alphabet[(triple >> 6) & 0x3F] : '=');
        out.push_back('=');
    }
    return out;
}

// JSON string escape for the small set of control chars + quote/backslash.
// Sufficient for our protocol fields (CIDs are hex; base64 has no specials).
static std::string json_escape(const std::string& s) {
    std::string out;
    out.reserve(s.size() + 2);
    out.push_back('"');
    for (char c : s) {
        switch (c) {
            case '"':  out += "\\\""; break;
            case '\\': out += "\\\\"; break;
            case '\n': out += "\\n";  break;
            case '\r': out += "\\r";  break;
            case '\t': out += "\\t";  break;
            default:
                if (static_cast<unsigned char>(c) < 0x20) {
                    char buf[8];
                    std::snprintf(buf, sizeof(buf), "\\u%04x", c);
                    out += buf;
                } else {
                    out.push_back(c);
                }
        }
    }
    out.push_back('"');
    return out;
}

// Extremely thin JSON-RPC scan. We don't pull in nlohmann/json for the
// orchestrator binary; the protocol's request shape is small and fixed:
//   { "jsonrpc":"2.0", "id":<id>, "method":"<name>", "params":<obj> }
// We only need `id` (passthrough) and `method`.
struct ParsedReq {
    std::string id_raw;     // verbatim JSON value (number, string, or null)
    std::string method;     // unquoted method name
    bool valid = false;
};

static std::string find_field(const std::string& body, const std::string& key) {
    // Returns the verbatim field value (with surrounding quotes for strings,
    // bare digits for numbers, "null" for null). Naive: assumes top-level
    // shape and no escaped quotes inside the value.
    size_t kpos = body.find("\"" + key + "\"");
    if (kpos == std::string::npos) return "";
    size_t colon = body.find(':', kpos);
    if (colon == std::string::npos) return "";
    size_t i = colon + 1;
    while (i < body.size() && (body[i] == ' ' || body[i] == '\t')) i++;
    if (i >= body.size()) return "";
    if (body[i] == '"') {
        size_t end = body.find('"', i + 1);
        if (end == std::string::npos) return "";
        return body.substr(i, end - i + 1);
    }
    // number / null / bool: scan until comma/brace/whitespace
    size_t end = i;
    while (end < body.size() && body[end] != ',' && body[end] != '}' && body[end] != ' ' && body[end] != '\n' && body[end] != '\r') end++;
    return body.substr(i, end - i);
}

static ParsedReq parse_req(const std::string& line) {
    ParsedReq r;
    std::string id = find_field(line, "id");
    std::string method_raw = find_field(line, "method");
    if (method_raw.size() < 2 || method_raw.front() != '"') return r;
    r.id_raw = id.empty() ? std::string("null") : id;
    r.method = method_raw.substr(1, method_raw.size() - 2);
    r.valid = true;
    return r;
}

static void run_rpc_mode() {
    std::ios::sync_with_stdio(false);
    std::string line;
    while (std::getline(std::cin, line)) {
        if (line.empty()) continue;
        ParsedReq req = parse_req(line);
        if (!req.valid) continue;

        if (req.method == "initialize") {
            std::ostringstream out;
            out << "{\"jsonrpc\":\"2.0\",\"id\":" << req.id_raw << ",\"result\":{"
                << "\"name\":\"cpp-self-contracts\","
                << "\"version\":\"1.0.0\","
                << "\"protocol_version\":\"provekit-lift/1\","
                << "\"capabilities\":{"
                << "\"authoring_surfaces\":[\"cpp-self-contracts\"],"
                << "\"ir_version\":\"v1.1.0\","
                << "\"emits_signed_mementos\":true}}}";
            std::cout << out.str() << "\n" << std::flush;
        } else if (req.method == "lift") {
            // Mint into a unique temp dir; capture CID + bytes.
            char tmpl[] = "/tmp/provekit-cpp-rpc-XXXXXX";
            char* tmp = mkdtemp(tmpl);
            if (!tmp) {
                std::ostringstream err;
                err << "{\"jsonrpc\":\"2.0\",\"id\":" << req.id_raw
                    << ",\"error\":{\"code\":-32603,\"message\":\"mkdtemp failed\"}}";
                std::cout << err.str() << "\n" << std::flush;
                continue;
            }
            const std::string tmp_dir(tmp);
            const std::string cid = mint_one_run(tmp_dir, /*verbose=*/false);
            const std::string proof_path = tmp_dir + "/" + cid + ".proof";
            std::ifstream f(proof_path, std::ios::binary);
            if (!f) {
                std::ostringstream err;
                err << "{\"jsonrpc\":\"2.0\",\"id\":" << req.id_raw
                    << ",\"error\":{\"code\":-32603,\"message\":\"read .proof failed\"}}";
                std::cout << err.str() << "\n" << std::flush;
                std::system(("rm -rf '" + tmp_dir + "'").c_str());
                continue;
            }
            std::vector<uint8_t> bytes((std::istreambuf_iterator<char>(f)),
                                        std::istreambuf_iterator<char>());
            f.close();
            std::system(("rm -rf '" + tmp_dir + "'").c_str());

            const std::string b64 = base64_encode(bytes);
            std::ostringstream out;
            out << "{\"jsonrpc\":\"2.0\",\"id\":" << req.id_raw << ",\"result\":{"
                << "\"kind\":\"proof-envelope\","
                << "\"filename_cid\":" << json_escape(cid) << ","
                << "\"bytes_base64\":" << json_escape(b64) << ","
                << "\"diagnostics\":[]}}";
            std::cout << out.str() << "\n" << std::flush;
        } else if (req.method == "shutdown") {
            std::ostringstream out;
            out << "{\"jsonrpc\":\"2.0\",\"id\":" << req.id_raw << ",\"result\":null}";
            std::cout << out.str() << "\n" << std::flush;
            return;
        } else {
            std::ostringstream err;
            err << "{\"jsonrpc\":\"2.0\",\"id\":" << req.id_raw
                << ",\"error\":{\"code\":-32601,\"message\":\"METHOD_NOT_FOUND: "
                << req.method << "\"}}";
            std::cout << err.str() << "\n" << std::flush;
        }
    }
}

int main(int argc, char* argv[]) {
    // --rpc takes over stdin/stdout for the lift-plugin protocol.
    for (int i = 1; i < argc; i++) {
        if (std::string(argv[i]) == "--rpc") {
            run_rpc_mode();
            return 0;
        }
    }

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
