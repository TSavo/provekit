// SPDX-License-Identifier: Apache-2.0

using Provekit.Canonicalizer;

namespace Provekit.ProofEnvelope;

internal static class ProofContracts
{
    internal static int csharp_proof_build_cid_length_eq_139(string name)
    {
        if (BuildCid(name).Length != 139) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_proof_build_is_deterministic(string name)
    {
        if (BuildBytes(name) != BuildBytes(name)) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_proof_cid_matches_blake3_of_bytes(string name)
    {
        if (Hash.Blake3_512(BuildRawBytes(name)) != BuildCid(name)) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_proof_build_bytes_length_gte_1(string name)
    {
        if (BuildRawBytes(name).Length < 1) throw new InvalidOperationException("contract");
        return 1;
    }

    private static string BuildCid(string name) => Proof.Build(Input(name)).Cid;

    private static string BuildBytes(string name) => Convert.ToBase64String(BuildRawBytes(name));

    private static byte[] BuildRawBytes(string name) => Proof.Build(Input(name)).Bytes;

    private static ProofEnvelopeInput Input(string name) => new()
    {
        Name = name,
        Version = "1.0.0",
        Members = new Dictionary<string, byte[]>(),
        SignerCid = Hash.Blake3_512Utf8(Sign.Ed25519PubkeyString(FoundationSeed)),
        SignerSeed = FoundationSeed,
        DeclaredAt = "2026-04-30T12:00:00.000Z",
    };

    private static readonly byte[] FoundationSeed = Enumerable.Repeat((byte)0x42, 32).ToArray();
}
