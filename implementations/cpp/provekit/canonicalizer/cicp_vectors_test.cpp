// SPDX-License-Identifier: Apache-2.0
//
// CICP golden-vector conformance. The expected CIDs live in
// protocol/conformance/cicp/vectors.json; this test re-derives them via
// the native C++ JCS + BLAKE3-512 path.

#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <map>
#include <set>
#include <sstream>
#include <string>
#include <vector>

#include <nlohmann/json.hpp>

#include "hash.hpp"
#include "jcs.hpp"
#include "value.hpp"

using Json = nlohmann::json;
using namespace provekit::canonicalizer;

namespace {

constexpr const char* kVectorDir = "protocol/conformance/cicp";
constexpr const char* kCidPrefix = "blake3-512:";

// Embedded fallback copies of protocol/conformance/cicp/*.json. Bazel declares
// the shared corpus as test data; the fallback keeps direct standalone runs
// useful when they are launched away from the repo root or runfiles tree.
static constexpr const char* k_vectors_json = R"JSON({"catalogVersion":"v1.6.2-2026-05-07","catalogCid":"blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f","protocol":"content-addressed-ci-protocol","vectors":[{"name":"blast-radius-rust-kit","capability":"cicp.blast-radius.v1","body":"blast-radius-rust-kit.json","expectedCid":"blake3-512:b46ed4acaa333e1c67d34435914543235529eee7beb8e70ca5075fd5d4417a3a5685625532e3631f81c65d748e6d0b354c158b5f8043dc70aa1eb654a4ee9550","shouldPass":true},{"name":"blast-radius-rust-kit-next-catalog","capability":"cicp.blast-radius.v1.protocol-catalog-invalidates","body":"blast-radius-rust-kit-next-catalog.json","expectedCid":"blake3-512:add810e8496aa2b72de4db0e15f789a76e092b13dd0233d7e0e78c4155d17916f5a6104cfa011af734eec760f1c627bd13357cd64d1670407686d9136ac1f7b8","shouldPass":true},{"name":"job-result-pass","capability":"cicp.job-result.v1","body":"job-result-pass.json","expectedCid":"blake3-512:1c426f1cc560a02623931abd9349b855150f64507d1f5a231312fb0c017fe38a9224f60dc611087aaffb225411cbc5dbe936a2b2a469546387a5f304052cc141","shouldPass":true},{"name":"reuse-identical","capability":"cicp.reuse.v1.identical-input-closure","body":"reuse-identical.json","expectedCid":"blake3-512:4236da5414741b5c24e2347e5308ee60adf764ccf741f97865f0e149f2869547bde3e3e5d8d5b43e7be0389fc97ec2e4bcee37bfe5797f907db84337e921c961","shouldPass":true},{"name":"reuse-bridged-by-evolution","capability":"cicp.reuse.v1.bridged-by-evolution","body":"reuse-bridged-by-evolution.json","expectedCid":"blake3-512:1c83f9e79533e2c0254ec66e76e1478e332fc6c72ea751566f3698976c7050269a45cb149af0f22c931edb3d5724b7df112fe89df93c47c750b0718cc0b16dbd","shouldPass":true},{"name":"impact-protocol-extension-only","capability":"cicp.impact.v1","body":"impact-protocol-extension-only.json","expectedCid":"blake3-512:53f2eed2f4b6b87f62ae3348d5686ec9177c140965110abacaa5a68330fc27dbc6bca42673798bc0ab7f1e9a665b74aa4fa39900fd10ae0258a5a0efedcd817e","shouldPass":true},{"name":"invalid-blast-radius-open-input-closure","capability":"cicp.blast-radius.v1.fail-closed","body":"invalid-blast-radius-open-input-closure.json","shouldPass":false,"errorContains":"inputCids missing required CID"}]})JSON";
static constexpr const char* k_blast_radius_rust_kit_json = R"JSON({"kind":"CIBlastRadius","schemaVersion":"1","jobKey":"provekit/conformance/rust","subjectKind":"kit","subject":"rust","protocolCatalogCid":"blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f","jobDefinitionCid":"blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111","commandCid":"blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222","runnerIdentityCid":"blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333","toolchainCids":["blake3-512:44444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444"],"sourceClosureCid":"blake3-512:55555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555","lockfileCids":["blake3-512:66666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666"],"generatedInputCids":["blake3-512:77777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777"],"fixtureCids":["blake3-512:88888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888"],"relevantSpecCids":["blake3-512:4b63e8c58d59b54272407b624b67578b7e1a8fdeb71d41c7d5e18d3bd6d668e7f77c8e2b9a68a10d3732dda40baf66db27f87ab10cbdb1d52e857bcbb7d3ec47"],"policyCid":"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","nondeterminism":{"network":"forbidden","clock":"forbidden","secrets":"forbidden","randomness":"forbidden"},"inputCids":["blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111","blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222","blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333","blake3-512:44444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444","blake3-512:4b63e8c58d59b54272407b624b67578b7e1a8fdeb71d41c7d5e18d3bd6d668e7f77c8e2b9a68a10d3732dda40baf66db27f87ab10cbdb1d52e857bcbb7d3ec47","blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f","blake3-512:55555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555","blake3-512:66666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666","blake3-512:77777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777","blake3-512:88888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888","blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]})JSON";
static constexpr const char* k_blast_radius_rust_kit_next_catalog_json = R"JSON({"kind":"CIBlastRadius","schemaVersion":"1","jobKey":"provekit/conformance/rust","subjectKind":"kit","subject":"rust","protocolCatalogCid":"blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","jobDefinitionCid":"blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111","commandCid":"blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222","runnerIdentityCid":"blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333","toolchainCids":["blake3-512:44444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444"],"sourceClosureCid":"blake3-512:55555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555","lockfileCids":["blake3-512:66666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666"],"generatedInputCids":["blake3-512:77777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777"],"fixtureCids":["blake3-512:88888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888"],"relevantSpecCids":["blake3-512:4b63e8c58d59b54272407b624b67578b7e1a8fdeb71d41c7d5e18d3bd6d668e7f77c8e2b9a68a10d3732dda40baf66db27f87ab10cbdb1d52e857bcbb7d3ec47"],"policyCid":"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","nondeterminism":{"network":"forbidden","clock":"forbidden","secrets":"forbidden","randomness":"forbidden"},"inputCids":["blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111","blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222","blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333","blake3-512:44444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444","blake3-512:4b63e8c58d59b54272407b624b67578b7e1a8fdeb71d41c7d5e18d3bd6d668e7f77c8e2b9a68a10d3732dda40baf66db27f87ab10cbdb1d52e857bcbb7d3ec47","blake3-512:55555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555","blake3-512:66666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666","blake3-512:77777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777","blake3-512:88888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888","blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"]})JSON";
static constexpr const char* k_job_result_pass_json = R"JSON({"kind":"CIJobResultBodyClaim","schemaVersion":"1","jobKey":"provekit/conformance/rust","blastRadiusCid":"blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","result":"pass","outputCid":"blake3-512:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd","logCid":"blake3-512:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee","startedAt":"2026-05-07T00:00:00Z","finishedAt":"2026-05-07T00:01:00Z","runnerIdentityCid":"blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333","policyCid":"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","inputCids":["blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333","blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","blake3-512:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd","blake3-512:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"],"producer":{"kind":"ci-runner","name":"github-actions","version":"cicp-vector"}})JSON";
static constexpr const char* k_reuse_identical_json = R"JSON({"kind":"CIReuseBodyClaim","schemaVersion":"1","jobKey":"provekit/conformance/rust","currentBlastRadiusCid":"blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","previousBlastRadiusCid":"blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","previousResultWitnessCid":"blake3-512:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff","reuseReason":"identical-input-closure","bridgeWitnessCids":[],"policyCid":"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","inputCids":["blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","blake3-512:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"]})JSON";
static constexpr const char* k_reuse_bridged_by_evolution_json = R"JSON({"kind":"CIReuseBodyClaim","schemaVersion":"1","jobKey":"provekit/conformance/java","currentBlastRadiusCid":"blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","previousBlastRadiusCid":"blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","previousResultWitnessCid":"blake3-512:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff","reuseReason":"bridged-by-evolution","bridgeWitnessCids":["blake3-512:284f10c5a1572caa149701c45a82c439c10532fd4b59d076fdf1c42c7ff666387219c08b25457b14b8e642a9f97faabb007c4e4338ebac03eb8a679899f340b5"],"policyCid":"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","inputCids":["blake3-512:284f10c5a1572caa149701c45a82c439c10532fd4b59d076fdf1c42c7ff666387219c08b25457b14b8e642a9f97faabb007c4e4338ebac03eb8a679899f340b5","blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","blake3-512:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"]})JSON";
static constexpr const char* k_impact_protocol_extension_only_json = R"JSON({"kind":"CIImpactBodyClaim","schemaVersion":"1","baseStateCid":"blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","candidateStateCid":"blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","protocolEvolutionWitnessCids":["blake3-512:284f10c5a1572caa149701c45a82c439c10532fd4b59d076fdf1c42c7ff666387219c08b25457b14b8e642a9f97faabb007c4e4338ebac03eb8a679899f340b5"],"changedBlastRadiusCids":["blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"],"unchangedBlastRadiusCids":["blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"],"requiredJobKeys":["provekit/conformance/rust"],"reusableWitnessCids":["blake3-512:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"],"refusalCids":[],"policyCid":"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","inputCids":["blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","blake3-512:284f10c5a1572caa149701c45a82c439c10532fd4b59d076fdf1c42c7ff666387219c08b25457b14b8e642a9f97faabb007c4e4338ebac03eb8a679899f340b5","blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","blake3-512:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"]})JSON";
static constexpr const char* k_invalid_blast_radius_open_input_closure_json = R"JSON({"kind":"CIBlastRadius","schemaVersion":"1","jobKey":"provekit/conformance/rust","subjectKind":"kit","subject":"rust","protocolCatalogCid":"blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f","jobDefinitionCid":"blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111","commandCid":"blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222","runnerIdentityCid":"blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333","toolchainCids":[],"sourceClosureCid":"blake3-512:55555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555","lockfileCids":[],"generatedInputCids":[],"fixtureCids":[],"relevantSpecCids":[],"policyCid":"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","nondeterminism":{"network":"forbidden","clock":"forbidden","secrets":"forbidden","randomness":"forbidden"},"inputCids":["blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f"]})JSON";

const char* embedded_json_for(const std::string& filename) {
    static const std::map<std::string, const char*> embedded = {
        {"vectors.json", k_vectors_json},
        {"blast-radius-rust-kit.json", k_blast_radius_rust_kit_json},
        {"blast-radius-rust-kit-next-catalog.json", k_blast_radius_rust_kit_next_catalog_json},
        {"job-result-pass.json", k_job_result_pass_json},
        {"reuse-identical.json", k_reuse_identical_json},
        {"reuse-bridged-by-evolution.json", k_reuse_bridged_by_evolution_json},
        {"impact-protocol-extension-only.json", k_impact_protocol_extension_only_json},
        {"invalid-blast-radius-open-input-closure.json", k_invalid_blast_radius_open_input_closure_json},
    };
    auto it = embedded.find(filename);
    if (it == embedded.end()) {
        throw std::runtime_error("no embedded CICP vector JSON for " + filename);
    }
    return it->second;
}

std::string read_vector_json(const std::string& filename) {
    const std::string path = std::string(kVectorDir) + "/" + filename;
    std::ifstream f(path, std::ios::binary);
    if (!f) return embedded_json_for(filename);
    std::ostringstream ss;
    ss << f.rdbuf();
    return ss.str();
}

ValuePtr json_to_value(const Json& j) {
    if (j.is_null()) return Value::null_value();
    if (j.is_boolean()) return Value::boolean(j.get<bool>());
    if (j.is_number_integer()) return Value::integer(j.get<int64_t>());
    if (j.is_number_unsigned()) return Value::integer(static_cast<int64_t>(j.get<uint64_t>()));
    if (j.is_string()) return Value::string(j.get<std::string>());
    if (j.is_array()) {
        std::vector<ValuePtr> elems;
        for (const auto& e : j) {
            elems.push_back(json_to_value(e));
        }
        return Value::array(elems);
    }
    if (j.is_object()) {
        std::vector<std::pair<std::string, ValuePtr>> kvs;
        for (auto it = j.begin(); it != j.end(); ++it) {
            kvs.emplace_back(it.key(), json_to_value(it.value()));
        }
        return Value::object(kvs);
    }
    throw std::runtime_error("unsupported JSON value");
}

bool is_cid(const std::string& s) {
    return s.rfind(kCidPrefix, 0) == 0;
}

void add_cids_from_array(const Json& body, const char* field, std::set<std::string>* out) {
    if (!body.contains(field)) return;
    if (!body[field].is_array()) {
        throw std::runtime_error(std::string(field) + " must be an array");
    }
    for (const auto& entry : body[field]) {
        if (!entry.is_string()) {
            throw std::runtime_error(std::string(field) + " entry must be a string");
        }
        const std::string cid = entry.get<std::string>();
        if (is_cid(cid)) out->insert(cid);
    }
}

void add_cid_field(const Json& body, const char* field, std::set<std::string>* out) {
    if (!body.contains(field)) return;
    if (!body[field].is_string()) {
        throw std::runtime_error(std::string(field) + " must be a string");
    }
    const std::string cid = body[field].get<std::string>();
    if (is_cid(cid)) out->insert(cid);
}

std::set<std::string> required_input_cids(const Json& body) {
    std::set<std::string> required;
    add_cid_field(body, "baseStateCid", &required);
    add_cid_field(body, "blastRadiusCid", &required);
    add_cid_field(body, "candidateStateCid", &required);
    add_cid_field(body, "commandCid", &required);
    add_cid_field(body, "currentBlastRadiusCid", &required);
    add_cid_field(body, "jobDefinitionCid", &required);
    add_cid_field(body, "logCid", &required);
    add_cid_field(body, "outputCid", &required);
    add_cid_field(body, "policyCid", &required);
    add_cid_field(body, "previousBlastRadiusCid", &required);
    add_cid_field(body, "previousResultWitnessCid", &required);
    add_cid_field(body, "protocolCatalogCid", &required);
    add_cid_field(body, "runnerIdentityCid", &required);
    add_cid_field(body, "sourceClosureCid", &required);

    add_cids_from_array(body, "bridgeWitnessCids", &required);
    add_cids_from_array(body, "changedBlastRadiusCids", &required);
    add_cids_from_array(body, "fixtureCids", &required);
    add_cids_from_array(body, "generatedInputCids", &required);
    add_cids_from_array(body, "lockfileCids", &required);
    add_cids_from_array(body, "protocolEvolutionWitnessCids", &required);
    add_cids_from_array(body, "refusalCids", &required);
    add_cids_from_array(body, "relevantSpecCids", &required);
    add_cids_from_array(body, "reusableWitnessCids", &required);
    add_cids_from_array(body, "toolchainCids", &required);
    add_cids_from_array(body, "unchangedBlastRadiusCids", &required);
    return required;
}

std::string validate_input_closure(const Json& body) {
    if (!body.contains("inputCids") || !body["inputCids"].is_array()) {
        return "inputCids missing or not an array";
    }
    std::set<std::string> input_cids;
    for (const auto& entry : body["inputCids"]) {
        if (!entry.is_string()) return "inputCids entry is not a string";
        input_cids.insert(entry.get<std::string>());
    }
    for (const auto& cid : required_input_cids(body)) {
        if (input_cids.find(cid) == input_cids.end()) {
            return "inputCids missing required CID " + cid;
        }
    }
    return "";
}

bool check(bool ok, const std::string& label, const std::string& got, const std::string& want) {
    if (ok) {
        std::printf("  [PASS] %s\n", label.c_str());
        return true;
    }
    std::printf("  [FAIL] %s\n", label.c_str());
    std::printf("    got:  %s\n", got.c_str());
    std::printf("    want: %s\n", want.c_str());
    return false;
}

}  // namespace

int main() {
    std::printf("CICP golden-vector conformance test:\n\n");

    int failures = 0;
    const Json catalog = Json::parse(read_vector_json("vectors.json"));
    for (const auto& vector : catalog.at("vectors")) {
        const std::string name = vector.at("name").get<std::string>();
        const std::string body_file = vector.at("body").get<std::string>();
        const bool should_pass = vector.at("shouldPass").get<bool>();
        const Json body = Json::parse(read_vector_json(body_file));

        if (should_pass) {
            const std::string canonical = encode_jcs(*json_to_value(body));
            const std::string actual = compute_cid(canonical);
            const std::string expected = vector.at("expectedCid").get<std::string>();
            if (!check(actual == expected, name + " CID", actual, expected)) {
                failures++;
            }

            const std::string validation_error = validate_input_closure(body);
            if (!check(validation_error.empty(), name + " inputCids closed", validation_error, "")) {
                failures++;
            }
        } else {
            const std::string validation_error = validate_input_closure(body);
            const std::string expected = vector.at("errorContains").get<std::string>();
            if (!check(validation_error.find(expected) != std::string::npos,
                       name + " fails closed",
                       validation_error,
                       expected)) {
                failures++;
            }
        }
    }

    std::printf("\n");
    if (failures == 0) {
        std::printf("CICP CONFORMANCE OK\n");
        return 0;
    }
    std::printf("CICP CONFORMANCE FAILED: %d check(s)\n", failures);
    return 1;
}
