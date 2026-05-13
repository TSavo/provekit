// SPDX-License-Identifier: Apache-2.0
//
// Cross-impl JCS conformance test for the v1.3.0 protocol additions:
//   1. `EvidenceTerm` IR-JSON shape , locked byte fixture from the
//      Rust peer's write_evidence/marshal_declarations in
//      implementations/rust/provekit-ir-symbolic/src/serialize.rs.
//   2. `binaryCid` field on the catalog memento , determinism +
//      shape check; absent vs present must be byte-distinct.
//
// THIS IS A GOLDEN TEST: the expected byte strings below are derived
// from the spec + Rust reference, not from the C++ impl. Any drift
// here is either a C++ bug or a Rust bug; both are reportable.
//
// Spec sources:
//   protocol/specs/2026-04-30-ir-formal-grammar.md (BridgeDeclaration)
//   protocol/specs/2026-04-30-proof-file-format.md §binaryCid
//   protocol/specs/2026-05-02-binary-attestation-protocol.md
//   implementations/rust/provekit-ir-symbolic/src/serialize.rs (peer)
//   implementations/rust/provekit-proof-envelope/src/proof.rs   (peer)

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <map>
#include <string>
#include <vector>

#include "provekit/ir.hpp"
#include "provekit/proof-envelope/proof_envelope.hpp"

using namespace provekit::ir;
using ::provekit::proof_envelope::Ed25519Seed;
using ::provekit::proof_envelope::ProofEnvelopeInput;
using ::provekit::proof_envelope::build_proof_envelope;

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

}  // namespace

int main() {
    int failures = 0;
    std::printf("v1.3.0 cross-impl conformance smoke test:\n\n");

    // --- 1. EvidenceTerm: locked IR-JSON shape ------------------------------
    //
    // Rust reference (serialize.rs `write_evidence`):
    //   {"kind":"evidence","proofType":"<x>","certificate":{"tool":"<x>",
    //    "version":"<x>","formulaHash":"<x>","proofData":"<x>"}}
    //
    // Wrapped inside a contract declaration via marshal_declarations, the
    // field appears AFTER `inv?` and BEFORE the closing `}` (insertion
    // order from the kit; canonicalizer's JCS pass re-sorts before hashing).

    reset_collector();
    begin_collecting();
    contract(
        "evidence-bearing-contract",
        /*pre=*/nullptr,
        /*post=*/nullptr,
        // A trivial inv: forall n. n > 0. Uses the same fresh-name counter
        // as parseInt-requires-positive for stability.
        /*inv=*/forall(Int(), [](std::shared_ptr<Term> n) {
            return gt(n, num(0));
        }),
        /*outBinding=*/"out",
        /*evidence=*/evidence(
            "smt-lib",
            EvidenceCertificate{
                /*tool=*/"z3",
                /*version=*/"4.13.0",
                /*formula_hash=*/"blake3-512:deadbeef",
                /*proof_data=*/"(unsat)"}));
    auto decls = finish();

    const std::string got = marshal_declarations(decls);
    const std::string want =
        "[{\"evidence\":{\"certificate\":{\"formulaHash\":\"blake3-512:deadbeef\","
        "\"proofData\":\"(unsat)\","
        "\"tool\":\"z3\","
        "\"version\":\"4.13.0\"},"
        "\"kind\":\"evidence\","
        "\"proofType\":\"smt-lib\"},"
        "\"inv\":{\"body\":{\"args\":[{\"kind\":\"var\",\"name\":\"_x0\"},"
        "{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},\"value\":0}],"
        "\"kind\":\"atomic\",\"name\":\">\"},"
        "\"kind\":\"forall\",\"name\":\"_x0\","
        "\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}},"
        "\"kind\":\"contract\","
        "\"name\":\"evidence-bearing-contract\","
        "\"outBinding\":\"out\"}]";

    if (!check_eq_str("EvidenceTerm IR-JSON shape", got, want)) {
        failures++;
    }

    // --- 2. EvidenceTerm absent: marshal_declarations omits the field -------
    //
    // The "omit absent" JCS rule means a contract with evidence == nullptr
    // produces identical bytes to the v1.2.0 shape. This guarantees the
    // pinned C++ self-contracts CID does not drift on this branch.

    reset_collector();
    begin_collecting();
    contract(
        "no-evidence-contract",
        /*pre=*/nullptr,
        /*post=*/nullptr,
        /*inv=*/forall(Int(), [](std::shared_ptr<Term> n) {
            return gt(n, num(0));
        }),
        /*outBinding=*/"out");  // evidence defaults to nullptr
    auto bare_decls = finish();

    const std::string bare_got = marshal_declarations(bare_decls);
    const std::string bare_want =
        "[{\"inv\":{\"body\":{\"args\":[{\"kind\":\"var\",\"name\":\"_x0\"},"
        "{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},\"value\":0}],"
        "\"kind\":\"atomic\",\"name\":\">\"},"
        "\"kind\":\"forall\",\"name\":\"_x0\","
        "\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}},"
        "\"kind\":\"contract\","
        "\"name\":\"no-evidence-contract\","
        "\"outBinding\":\"out\"}]";

    if (!check_eq_str("EvidenceTerm absent (omit-when-null)",
                      bare_got, bare_want)) {
        failures++;
    }

    // --- 3. PrimitiveBridgeDeclaration: v1.3.0 fields exist & default empty -
    //
    // The kit-default parseInt registration leaves source_contract_cid and
    // target_proof_cid empty. Concrete kits set them when minting a real
    // bridge envelope. This pass just confirms the slots are addressable
    // (compile-time check via .source_contract_cid / .target_proof_cid).

    reset_registry();
    ensure_kit_bridges_registered();
    auto bridges = list_bridges();
    if (bridges.size() != 1) {
        std::printf("  [FAIL] expected 1 default bridge, got %zu\n",
                    bridges.size());
        failures++;
    } else {
        const auto& b = bridges[0];
        if (b.ir_name != "parseInt") {
            std::printf("  [FAIL] bridge ir_name = %s, want parseInt\n",
                        b.ir_name.c_str());
            failures++;
        } else if (!b.source_contract_cid.empty()) {
            std::printf("  [FAIL] default bridge source_contract_cid not empty: %s\n",
                        b.source_contract_cid.c_str());
            failures++;
        } else if (!b.target_proof_cid.empty()) {
            std::printf("  [FAIL] default bridge target_proof_cid not empty: %s\n",
                        b.target_proof_cid.c_str());
            failures++;
        } else {
            std::printf("  [PASS] PrimitiveBridgeDeclaration v1.3.0 slots present\n");
        }
    }

    // --- 4. binaryCid: present vs absent yields byte-distinct envelopes -----
    //
    // The Rust peer (provekit-proof-envelope/src/proof.rs) emits the
    // CBOR pair only when `binary_cid` is Some(_). The C++ peer mirrors
    // by sentinel: empty string = absent. We assert two things:
    //   (a) Two builds with the SAME absent input produce identical bytes.
    //   (b) A build with binary_cid set produces DIFFERENT bytes than
    //       the absent build (proves the field actually changes the
    //       canonical output, vs. being silently dropped).

    Ed25519Seed seed{};
    seed.fill(0x42);

    const std::string signer_cid_v1 =
        "blake3-512:"
        "00000000000000000000000000000000000000000000000000000000000000ab"
        "cd000000000000000000000000000000000000000000000000000000000000ab";

    ProofEnvelopeInput absent_input{
        .name = "test-binary-cid",
        .version = "1.0.0",
        .members = std::map<std::string, std::vector<uint8_t>>{},
        .signer_cid = signer_cid_v1,
        .signer_seed = seed,
        .declared_at = "2026-05-02T00:00:00.000Z",
    };
    // binary_cid omitted (default = empty string).

    ProofEnvelopeInput present_input = absent_input;
    present_input.binary_cid =
        "blake3-512:"
        "1111111111111111111111111111111111111111111111111111111111111111"
        "1111111111111111111111111111111111111111111111111111111111111111";

    auto absent_a = build_proof_envelope(absent_input);
    auto absent_b = build_proof_envelope(absent_input);
    auto present  = build_proof_envelope(present_input);

    if (absent_a.bytes != absent_b.bytes) {
        std::printf("  [FAIL] binaryCid absent: build is non-deterministic\n");
        failures++;
    } else {
        std::printf("  [PASS] binaryCid absent: deterministic\n");
    }

    if (absent_a.bytes == present.bytes) {
        std::printf("  [FAIL] binaryCid present produced identical bytes "
                    "to absent , field was silently dropped\n");
        failures++;
    } else {
        std::printf("  [PASS] binaryCid present: byte-distinct from absent\n");
    }

    // The first byte of the catalog map is the CBOR map head. Adding one
    // optional pair shifts the map count by 1: 7 entries (without
    // binaryCid) → 0xa7; 8 entries → 0xa8. This is the cheap structural
    // check that the pair landed where the Rust peer puts it.
    if (absent_a.bytes.empty() || absent_a.bytes[0] != 0xa7) {
        std::printf("  [FAIL] absent catalog map head: got 0x%02x, want 0xa7\n",
                    absent_a.bytes.empty() ? 0 : absent_a.bytes[0]);
        failures++;
    } else {
        std::printf("  [PASS] absent catalog map head: 0xa7 (7 keys)\n");
    }
    if (present.bytes.empty() || present.bytes[0] != 0xa8) {
        std::printf("  [FAIL] present catalog map head: got 0x%02x, want 0xa8\n",
                    present.bytes.empty() ? 0 : present.bytes[0]);
        failures++;
    } else {
        std::printf("  [PASS] present catalog map head: 0xa8 (8 keys)\n");
    }

    // --- 5. BridgeDecl: locked IR-JSON shape (task #225) --------------------
    //
    // Cross-impl byte fixture from conformance/fixtures.toml `bridge_decl`:
    //   {"kind":"bridge","name":"myBridge","notes":"some notes",
    //    "sourceContractCid":"bafySource","sourceLayer":"c-kit",
    //    "sourceSymbol":"source","targetContractCid":"bafyTarget",
    //    "targetLayer":"coq","targetProofCid":"bafyProof"}
    //
    // JCS alphabetical key order: kind, name, [notes?], sourceContractCid,
    // sourceLayer, sourceSymbol, targetContractCid, targetLayer, targetProofCid.
    // The `notes` field is omitted entirely when empty (JCS "omit absent" rule);
    // see protocol/specs/2026-04-30-ir-formal-grammar.md §BridgeDeclaration.
    {
        BridgeDecl b{};
        b.name = "myBridge";
        b.source_symbol = "source";
        b.source_layer = "c-kit";
        b.source_contract_cid = "bafySource";
        b.target_contract_cid = "bafyTarget";
        b.target_proof_cid = "bafyProof";
        b.target_layer = "coq";
        b.notes = "some notes";

        std::ostringstream oss;
        write_bridge_decl(oss, b);
        const std::string bridge_got = oss.str();
        const std::string bridge_want =
            "{\"kind\":\"bridge\",\"name\":\"myBridge\","
            "\"notes\":\"some notes\","
            "\"sourceContractCid\":\"bafySource\","
            "\"sourceLayer\":\"c-kit\","
            "\"sourceSymbol\":\"source\","
            "\"targetContractCid\":\"bafyTarget\","
            "\"targetLayer\":\"coq\","
            "\"targetProofCid\":\"bafyProof\"}";

        if (!check_eq_str("BridgeDecl IR-JSON shape (all 9 fields)",
                          bridge_got, bridge_want)) {
            failures++;
        }

        // notes empty: field omitted entirely (not emitted as null/empty).
        BridgeDecl b_no_notes = b;
        b_no_notes.notes = "";
        std::ostringstream oss2;
        write_bridge_decl(oss2, b_no_notes);
        const std::string nn_got = oss2.str();
        const std::string nn_want =
            "{\"kind\":\"bridge\",\"name\":\"myBridge\","
            "\"sourceContractCid\":\"bafySource\","
            "\"sourceLayer\":\"c-kit\","
            "\"sourceSymbol\":\"source\","
            "\"targetContractCid\":\"bafyTarget\","
            "\"targetLayer\":\"coq\","
            "\"targetProofCid\":\"bafyProof\"}";

        if (!check_eq_str("BridgeDecl notes-absent (omit-when-empty)",
                          nn_got, nn_want)) {
            failures++;
        }
    }

    std::printf("\n");
    if (failures == 0) {
        std::printf("v1.3.0 CONFORMANCE SMOKE TEST OK.\n");
        return 0;
    }
    std::printf("v1.3.0 CONFORMANCE SMOKE TEST FAILED: %d divergence(s).\n",
                failures);
    return 1;
}
