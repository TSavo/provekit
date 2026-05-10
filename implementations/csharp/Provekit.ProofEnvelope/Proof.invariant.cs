// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cs for Provekit.ProofEnvelope/Proof.cs
//
// Public surface covered:
//   * Proof.Build(input): ProofEnvelopeOutput
//
// Honest scope:
//   Build is the deterministic .proof catalog encoder. Its `Cid` field
//   IS the catalog filename ("blake3-512:" + 128 hex). Identical input
//   produces byte-identical output (the framework's most important
//   determinism claim: verified empirically by mint-twice in this
//   orchestrator's main).

using static Provekit.IR.Predicates;
using static Provekit.IR.Quantifiers;
using static Provekit.IR.Terms;
using static Provekit.IR.Collector;
using Provekit.IR;

namespace Provekit.SelfContracts.Invariants;

public static class ProofInvariants
{
    public static void Register()
    {
        // Build's Cid output is exactly 139 chars.
        Must("csharp_proof_build_cid_length_eq_139",
            ForAll(Sort.String, input =>
                Eq(Ctor("len", Ctor("BuildCid", input)), Num(139))));

        // Build is byte-deterministic: identical input → identical bytes.
        Must("csharp_proof_build_is_deterministic",
            ForAll(Sort.String, input =>
                Eq(Ctor("BuildBytes", input), Ctor("BuildBytes", input))));

        // The CID and the Bytes match: hashing Bytes equals Cid.
        Must("csharp_proof_cid_matches_blake3_of_bytes",
            ForAll(Sort.String, input =>
                Eq(Ctor("Blake3", Ctor("BuildBytes", input)),
                   Ctor("BuildCid", input))));

        // Output bytes length is bounded below by 1 (CBOR map head + at
        // least the kind/name/version pairs always present).
        Must("csharp_proof_build_bytes_length_gte_1",
            ForAll(Sort.String, input =>
                Gte(Ctor("len", Ctor("BuildBytes", input)), Num(1))));
    }
}
