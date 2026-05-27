// SPDX-License-Identifier: Apache-2.0
//
// mint_cpp_self_contracts: the C++ peer self-contracts orchestrator.
//
// Runs the existing C++ native lifter over the kit's native self-contract
// assertion surface, mints the lifted contracts as signed mementos, bundles
// them into a `.proof` whose filename IS its catalog CID, and asserts
// byte-determinism by minting twice into separate output dirs.
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
#include "provekit/canonicalizer/jcs.hpp"
#include "provekit/canonicalizer/value.hpp"
#include "provekit/claim-envelope/mint.hpp"
#include "provekit/claim-envelope/value_from_kit.hpp"
#include "provekit/proof-envelope/proof_envelope.hpp"
#include "provekit/proof-envelope/sign_ed25519.hpp"

#include <algorithm>
#include <cerrno>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <map>
#include <stdexcept>
#include <sstream>
#include <string>
#include <vector>
#include <unistd.h>

#include <nlohmann/json.hpp>

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
using ::provekit::canonicalizer::encode_jcs;
using ::provekit::canonicalizer::Value;
using ::provekit::canonicalizer::ValuePtr;
using Json = nlohmann::json;

// Compute the signer-independent contractCid for a contract.
// Per spec 2026-05-03-contract-cid-vs-attestation-cid.md §1:
//   contractCid = blake3-512(JCS({name, outBinding, pre?, post?, inv?}))
static std::string contract_cid_from_args(const MintContractArgs& args) {
    std::vector<std::pair<std::string, ValuePtr>> kvs;
    kvs.push_back({"name", Value::string(args.contract_name)});
    kvs.push_back({"outBinding", Value::string(args.out_binding)});
    if (args.pre)  kvs.push_back({"pre",  args.pre});
    if (args.post) kvs.push_back({"post", args.post});
    if (args.inv)  kvs.push_back({"inv",  args.inv});
    auto v = Value::object(std::move(kvs));
    return compute_cid(encode_jcs(v));
}

// Compute the contractSetCid from a sorted list of signer-independent
// contractCid strings. Per spec 2026-05-03-contract-set-extension.md §1:
//   contractSetCid = blake3-512(JCS(<sorted contractCIDs>))
static std::string compute_contract_set_cid(std::vector<std::string> cids) {
    std::sort(cids.begin(), cids.end());
    std::vector<ValuePtr> elems;
    elems.reserve(cids.size());
    for (const auto& c : cids) {
        elems.push_back(Value::string(c));
    }
    auto arr = Value::array(std::move(elems));
    return compute_cid(encode_jcs(arr));
}

struct LiftedContractDecl {
    std::string name;
    ValuePtr pre;
    ValuePtr post;
    ValuePtr inv;
    std::string out_binding;
};

static ValuePtr json_to_value(const Json& j) {
    if (j.is_null()) return Value::null_value();
    if (j.is_boolean()) return Value::boolean(j.get<bool>());
    if (j.is_number_integer()) return Value::integer(j.get<int64_t>());
    if (j.is_number_unsigned()) return Value::integer(static_cast<int64_t>(j.get<uint64_t>()));
    if (j.is_string()) return Value::string(j.get<std::string>());
    if (j.is_array()) {
        std::vector<ValuePtr> elems;
        elems.reserve(j.size());
        for (const auto& item : j) elems.push_back(json_to_value(item));
        return Value::array(std::move(elems));
    }
    if (j.is_object()) {
        std::vector<std::pair<std::string, ValuePtr>> kvs;
        kvs.reserve(j.size());
        for (auto it = j.begin(); it != j.end(); ++it) {
            kvs.emplace_back(it.key(), json_to_value(it.value()));
        }
        return Value::object(std::move(kvs));
    }
    throw std::runtime_error("unsupported JSON value in lifted contract");
}

static std::string shell_quote(const std::string& s) {
    std::string out = "'";
    for (char c : s) {
        if (c == '\'') {
            out += "'\\''";
        } else {
            out.push_back(c);
        }
    }
    out.push_back('\'');
    return out;
}

static std::string current_working_dir() {
    char buf[4096];
    if (!getcwd(buf, sizeof(buf))) {
        throw std::runtime_error("getcwd failed");
    }
    return std::string(buf);
}

static std::string workspace_root() {
    const char* from_env = std::getenv("PROVEKIT_WORKSPACE_ROOT");
    if (from_env && *from_env) return std::string(from_env);
    return current_working_dir();
}

static std::string read_text_file_or_die(const std::string& path) {
    std::ifstream in(path, std::ios::binary);
    if (!in) {
        throw std::runtime_error("cannot read " + path);
    }
    std::ostringstream buf;
    buf << in.rdbuf();
    return buf.str();
}

static std::vector<LiftedContractDecl> load_native_lifted_contracts(const std::string& root) {
    char tmpl[] = "/tmp/provekit-cpp-native-lift-XXXXXX";
    char* tmp = mkdtemp(tmpl);
    if (!tmp) {
        throw std::runtime_error("mkdtemp failed while lifting native C++ self-contracts");
    }
    const std::string tmp_dir(tmp);
    const std::string lifter = root + "/implementations/cpp/target/provekit-lift-cpp";
    const std::string native_dir =
        root + "/implementations/cpp/provekit-self-contracts/native";
    const std::string cmd =
        shell_quote(lifter) + " --workspace " + shell_quote(native_dir) +
        " -o " + shell_quote(tmp_dir) + " >/dev/null";
    if (std::system(cmd.c_str()) != 0) {
        std::system(("rm -rf " + shell_quote(tmp_dir)).c_str());
        throw std::runtime_error("provekit-lift-cpp failed while lifting native self-contracts");
    }

    const std::string json_text = read_text_file_or_die(tmp_dir + "/lifted.json");
    std::system(("rm -rf " + shell_quote(tmp_dir)).c_str());

    Json parsed = Json::parse(json_text);
    if (!parsed.is_array()) {
        throw std::runtime_error("provekit-lift-cpp lifted.json is not an array");
    }

    std::vector<LiftedContractDecl> out;
    for (const auto& item : parsed) {
        if (!item.is_object()) continue;
        if (item.value("kind", "") != "contract") continue;
        LiftedContractDecl d;
        d.name = item.value("name", "");
        d.out_binding = item.value("outBinding", "out");
        if (d.name.empty()) {
            throw std::runtime_error("lifted contract missing name");
        }
        if (item.contains("pre")) d.pre = json_to_value(item.at("pre"));
        if (item.contains("post")) d.post = json_to_value(item.at("post"));
        if (item.contains("inv")) d.inv = json_to_value(item.at("inv"));
        if (!d.pre && !d.post && !d.inv) {
            throw std::runtime_error("lifted contract `" + d.name + "` has no formula");
        }
        out.push_back(std::move(d));
    }
    if (out.empty()) {
        throw std::runtime_error("native C++ self-contract lift produced zero contracts");
    }
    return out;
}

// Cross-kit bridge counterparts still use the kit collector until the bridge
// bundle has a native lifted surface. The C++ kit self-contract obligations
// themselves are loaded from provekit-lift-cpp output below.
extern "C" {
    void cross_kit_bridges_invariants();
}

struct MintOneRunResult {
    std::string bundle_cid;
    std::string contract_set_cid;
};

static MintOneRunResult mint_one_run(const std::string& out_dir,
                                     bool verbose,
                                     const std::string& root) {
    // 1. Lift the native C++ self-contract surface, then add bridge
    // counterparts still owned by the collector-backed bridge module.
    const auto lifted_contracts = load_native_lifted_contracts(root);

    reset_collector();
    begin_collecting();
    cross_kit_bridges_invariants();
    auto bridge_counterpart_decls = finish();

    if (verbose) {
        std::printf("lifted %zu native contracts from C++ assertion surfaces\n",
                    lifted_contracts.size());
        std::printf("authored %zu bridge counterpart contracts\n",
                    bridge_counterpart_decls.size());
    }

    // 2. Mint each as a signed ClaimEnvelope under the foundation key.
    Ed25519Seed signer_seed;
    signer_seed.fill(0x42);

    const std::string declared_at = "2026-04-30T12:00:00.000Z";
    const std::string produced_by = "cpp-kit@1.0";

    std::map<std::string, std::vector<uint8_t>> members;
    std::vector<std::string> content_cids;

    auto mint_one_contract = [&](const MintContractArgs& args) {
        content_cids.push_back(contract_cid_from_args(args));
        auto minted = mint_contract(args);
        members[minted.cid] = minted.canonical_bytes;
    };

    for (const auto& d : lifted_contracts) {
        MintContractArgs args{};
        args.contract_name = d.name;
        args.pre = d.pre;
        args.post = d.post;
        args.inv = d.inv;
        args.out_binding = d.out_binding;
        args.produced_by = produced_by;
        args.produced_at = declared_at;
        args.input_cids = {};
        args.authoring_kind = AuthoringKind::Lift;
        args.authoring_lift = {"provekit-lift-cpp@0.1.0", "tests", ""};
        args.signer_seed = signer_seed;
        mint_one_contract(args);
    }

    for (const auto& d : bridge_counterpart_decls) {
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
        mint_one_contract(args);
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

    const std::string cset_cid = compute_contract_set_cid(content_cids);

    if (verbose) {
        std::printf("  catalog CID:        %s\n", built.filename_cid.c_str());
        std::printf("  contractSetCid:     %s\n", cset_cid.c_str());
        std::printf("  proof bytes:        %zu\n", built.bytes.size());
        std::printf("  .proof file:        %s\n", out_path.c_str());
    }

    return MintOneRunResult{built.filename_cid, cset_cid};
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
                << "\"protocol_version\":\"pep/1.7.0\","
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
            const auto run_result = mint_one_run(tmp_dir, /*verbose=*/false, workspace_root());
            const std::string& cid = run_result.bundle_cid;
            const std::string& cset_cid = run_result.contract_set_cid;
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
                << "\"contract_set_cid\":" << json_escape(cset_cid) << ","
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
    const std::string root = workspace_root();

    std::printf("== ProvekIt C++ self-contracts orchestrator ==\n\n");
    std::printf("output dir: %s\n\n", out_dir.c_str());

    std::printf("== mint #1 ==\n");
    const auto run1 = mint_one_run(out_dir, /*verbose=*/true, root);

    // Determinism check: mint twice into separate output dirs and
    // assert byte-equality. The .proof filename IS the catalog CID;
    // matching filenames means matching bytes.
    const std::string out_dir2 = out_dir + "/_determinism_check";
    if (std::system(("mkdir -p '" + out_dir2 + "'").c_str()) != 0) {
        std::fprintf(stderr, "ERROR: cannot create %s\n", out_dir2.c_str());
        return 1;
    }
    std::printf("\n== mint #2 (determinism check) ==\n");
    const auto run2 = mint_one_run(out_dir2, /*verbose=*/false, root);
    if (run1.bundle_cid != run2.bundle_cid || run1.contract_set_cid != run2.contract_set_cid) {
        std::fprintf(stderr,
                     "DETERMINISM FAILURE:\n  run 1 cid:            %s\n  run 2 cid:            %s\n"
                     "  run 1 contractSetCid: %s\n  run 2 contractSetCid: %s\n",
                     run1.bundle_cid.c_str(), run2.bundle_cid.c_str(),
                     run1.contract_set_cid.c_str(), run2.contract_set_cid.c_str());
        return 1;
    }
    std::printf("  determinism check:  OK (two runs produced identical CIDs)\n");

    std::printf("\n== done. C++ self-application: live. ==\n");
    return 0;
}
