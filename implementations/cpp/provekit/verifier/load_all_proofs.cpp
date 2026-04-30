// SPDX-License-Identifier: Apache-2.0

#include "load_all_proofs.hpp"

#include "../canonicalizer/jcs.hpp"
#include "../canonicalizer/sha256.hpp"
#include "../canonicalizer/value.hpp"
#include "../proof-envelope/cbor_decoder.hpp"

#include <filesystem>
#include <fstream>
#include <regex>
#include <sstream>

namespace fs = std::filesystem;

namespace provekit::verifier {

namespace {

// Read entire file as bytes.
std::vector<uint8_t> read_file(const std::string& path) {
    std::ifstream f(path, std::ios::binary);
    if (!f) throw std::runtime_error("cannot open " + path);
    std::ostringstream ss;
    ss << f.rdbuf();
    const std::string s = ss.str();
    return std::vector<uint8_t>(s.begin(), s.end());
}

// Convert nlohmann::json → canonicalizer::Value tree (so we can JCS-
// encode it for CID re-derivation).
::provekit::canonicalizer::ValuePtr json_to_value(const Json& j) {
    using V = ::provekit::canonicalizer::Value;
    if (j.is_null()) return V::null_value();
    if (j.is_boolean()) return V::boolean(j.get<bool>());
    if (j.is_number_integer()) return V::integer(j.get<int64_t>());
    if (j.is_number_unsigned()) return V::integer(static_cast<int64_t>(j.get<uint64_t>()));
    if (j.is_number_float()) return V::integer(static_cast<int64_t>(j.get<double>()));
    if (j.is_string()) return V::string(j.get<std::string>());
    if (j.is_array()) {
        std::vector<::provekit::canonicalizer::ValuePtr> elems;
        for (const auto& e : j) elems.push_back(json_to_value(e));
        return V::array(elems);
    }
    if (j.is_object()) {
        std::vector<std::pair<std::string, ::provekit::canonicalizer::ValuePtr>> kvs;
        for (auto it = j.begin(); it != j.end(); ++it) {
            kvs.emplace_back(it.key(), json_to_value(it.value()));
        }
        return V::object(kvs);
    }
    return V::null_value();
}

// Recompute envelope CID from JSON envelope (minus cid + producerSignature).
std::string compute_envelope_cid(const Json& env) {
    Json stripped = env;
    stripped.erase("cid");
    stripped.erase("producerSignature");
    auto value_tree = json_to_value(stripped);
    const std::string canonical = ::provekit::canonicalizer::encode_jcs(*value_tree);
    return ::provekit::canonicalizer::sha256_hex(canonical).substr(0, 32);
}

}  // namespace

MementoPool LoadAllProofsStage::Run(const std::string& project_root) {
    MementoPool pool;
    for (const auto& p : enumerate_proof_files(project_root)) {
        try {
            load_one(p, pool);
        } catch (const std::exception& e) {
            pool.load_errors.push_back({p, std::string("read/decode: ") + e.what()});
        }
    }
    return pool;
}

void LoadAllProofsStage::load_one(const std::string& proof_path, MementoPool& pool) {
    auto bytes = read_file(proof_path);

    // Rule 1: filename CID matches content (trust root).
    fs::path p(proof_path);
    const std::string filename = p.filename().string();
    static const std::regex re(R"(^([0-9a-f]+)\.proof$)");
    std::smatch m;
    if (std::regex_match(filename, m, re)) {
        const std::string filenameCid = m[1].str();
        const std::string s(reinterpret_cast<const char*>(bytes.data()), bytes.size());
        const std::string derived = ::provekit::canonicalizer::sha256_hex(s).substr(0, 32);
        if (derived != filenameCid) {
            pool.load_errors.push_back({proof_path,
                "rule 1 (trust root): filename CID " + filenameCid +
                    " != content hash " + derived});
            return;
        }
    }

    ::provekit::proof_envelope::CBORDecoder dec(bytes);
    auto catalog = dec.decode();
    if (!catalog->is_map()) {
        pool.load_errors.push_back({proof_path, "catalog is not a map"});
        return;
    }
    const auto& m_root = catalog->as_map();
    auto it = m_root.find("members");
    if (it == m_root.end() || !it->second->is_map()) {
        pool.load_errors.push_back({proof_path, "catalog has no `members` map"});
        return;
    }
    for (const auto& [cid, val] : it->second->as_map()) {
        if (!val->is_bstr()) {
            pool.load_errors.push_back({proof_path, "member " + cid + ": value is not bstr"});
            continue;
        }
        const auto& env_bytes = val->as_bstr();
        const std::string env_text(reinterpret_cast<const char*>(env_bytes.data()), env_bytes.size());
        Json env;
        try {
            env = Json::parse(env_text);
        } catch (const std::exception& e) {
            pool.load_errors.push_back({proof_path,
                "member " + cid + ": JSON parse: " + e.what()});
            continue;
        }
        // Rule 2: re-derive envelope CID.
        const std::string derived = compute_envelope_cid(env);
        if (derived != cid) {
            pool.load_errors.push_back({proof_path,
                "rule 2: member " + cid + " derives to " + derived});
            continue;
        }
        pool.mementos[cid] = env;
        if (env.contains("evidence") && env["evidence"].is_object()) {
            const auto& ev = env["evidence"];
            if (ev.value("kind", "") == "bridge" && ev.contains("body") && ev["body"].is_object()) {
                const auto sym = ev["body"].value("sourceSymbol", "");
                if (!sym.empty()) pool.bridges_by_symbol[sym] = env;
            }
        }
    }
}

std::vector<std::string> LoadAllProofsStage::enumerate_proof_files(const std::string& project_root) {
    std::vector<std::string> out;
    auto push_proofs = [&](const fs::path& dir) {
        if (!fs::exists(dir) || !fs::is_directory(dir)) return;
        for (const auto& e : fs::directory_iterator(dir)) {
            if (!e.is_regular_file()) continue;
            if (e.path().extension() == ".proof") {
                out.push_back(e.path().string());
            }
        }
    };
    push_proofs(project_root);

    fs::path nm = fs::path(project_root) / "node_modules";
    if (!fs::exists(nm)) return out;

    for (const auto& e : fs::directory_iterator(nm)) {
        if (!e.is_directory()) continue;
        const auto name = e.path().filename().string();
        if (name.empty() || name[0] == '.') continue;
        if (name[0] == '@') {
            for (const auto& sub : fs::directory_iterator(e.path())) {
                if (sub.is_directory()) push_proofs(sub.path());
            }
        } else {
            push_proofs(e.path());
        }
    }
    return out;
}

}  // namespace provekit::verifier
