// SPDX-License-Identifier: Apache-2.0
//
// .proof envelope builder. Per RFC 8949 §4.2.1 + the .proof spec
// (protocol/specs/2026-04-30-proof-file-format.md):
//
//   1. Build the unsigned body as a CBOR map with keys sorted by
//      bytewise lex order of their CBOR-encoded form.
//   2. Ed25519-sign the unsigned-body bytes.
//   3. Re-emit the body with the signature added; keys re-sort
//      automatically.
//   4. BLAKE3-512 the final bytes; the full self-identifying string
//      `"blake3-512:<128 hex>"` IS the catalog CID.
//
// The `members` map key is the embedded envelope's own CID; the value
// is its canonical bytes (JCS-JSON for memento envelopes per the
// memento envelope grammar) wrapped as a CBOR byte string.

using Provekit.Canonicalizer;

namespace Provekit.ProofEnvelope;

public sealed class ProofEnvelopeInput
{
    public string Name { get; init; } = "";
    public string Version { get; init; } = "";

    /// <summary>
    /// Map from member CID (full self-identifying string,
    /// e.g. "blake3-512:abc...") to that member's canonical bytes
    /// (JCS-JSON bytes for memento envelopes).
    /// </summary>
    public IReadOnlyDictionary<string, byte[]> Members { get; init; } =
        new Dictionary<string, byte[]>();

    /// <summary>CID of the signer's public-key memento (or any resolvable CID).</summary>
    public string SignerCid { get; init; } = "";

    /// <summary>Ed25519 seed bytes (32). Deterministic signing.</summary>
    public byte[] SignerSeed { get; init; } = Array.Empty<byte>();

    /// <summary>ISO-8601 string with millisecond precision, trailing 'Z'.</summary>
    public string DeclaredAt { get; init; } = "";
}

public sealed class ProofEnvelopeOutput
{
    /// <summary>CBOR bytes of the signed catalog. Hash of these bytes IS the CID.</summary>
    public required byte[] Bytes { get; init; }

    /// <summary>Full self-identifying CID, e.g. "blake3-512:&lt;128 hex&gt;".</summary>
    public required string Cid { get; init; }
}

internal sealed class CborPair
{
    public required byte[] KeyCbor { get; init; }
    public required byte[] ValueCbor { get; init; }
}

public static class Proof
{
    public static ProofEnvelopeOutput Build(ProofEnvelopeInput input)
    {
        // Step 1: encode unsigned body with sorted keys.
        var unsignedPairs = BodyPairsUnsigned(input);
        var unsignedBytes = new List<byte>();
        EmitSortedMap(unsignedBytes, unsignedPairs);
        var unsignedArr = unsignedBytes.ToArray();

        // Step 2: Ed25519-sign the unsigned bytes.
        var sig = Sign.Ed25519SignWithSeed(input.SignerSeed, unsignedArr);

        // Step 3: re-emit with signature added; keys re-sort automatically.
        var signedPairs = BodyPairsUnsigned(input);
        signedPairs.Add(MakeBytesPair("signature", sig));
        var finalBytes = new List<byte>();
        EmitSortedMap(finalBytes, signedPairs);
        var finalArr = finalBytes.ToArray();

        // Step 4: filename CID = full self-identifying BLAKE3-512.
        var cid = Hash.Blake3_512(finalArr);
        return new ProofEnvelopeOutput { Bytes = finalArr, Cid = cid };
    }

    private static List<CborPair> BodyPairsUnsigned(ProofEnvelopeInput input)
    {
        return new List<CborPair>
        {
            MakeStringPair("kind", "catalog"),
            MakeStringPair("name", input.Name),
            MakeStringPair("version", input.Version),
            MakeMembersPair("members", input.Members),
            MakeStringPair("signer", input.SignerCid),
            MakeStringPair("declaredAt", input.DeclaredAt),
        };
    }

    private static byte[] EncodeKey(string key)
    {
        var k = new List<byte>();
        Cbor.EncodeTstr(k, key);
        return k.ToArray();
    }

    private static CborPair MakeStringPair(string key, string value)
    {
        var v = new List<byte>();
        Cbor.EncodeTstr(v, value);
        return new CborPair { KeyCbor = EncodeKey(key), ValueCbor = v.ToArray() };
    }

    private static CborPair MakeBytesPair(string key, byte[] value)
    {
        var v = new List<byte>();
        Cbor.EncodeBstr(v, value);
        return new CborPair { KeyCbor = EncodeKey(key), ValueCbor = v.ToArray() };
    }

    private static CborPair MakeMembersPair(string key, IReadOnlyDictionary<string, byte[]> members)
    {
        var pairs = new List<CborPair>();
        foreach (var kv in members)
        {
            pairs.Add(MakeBytesPair(kv.Key, kv.Value));
        }
        var valueBytes = new List<byte>();
        EmitSortedMap(valueBytes, pairs);
        return new CborPair { KeyCbor = EncodeKey(key), ValueCbor = valueBytes.ToArray() };
    }

    private static void EmitSortedMap(List<byte> output, List<CborPair> pairs)
    {
        // Sort by bytewise lex order of the CBOR-encoded key form (RFC
        // 8949 §4.2.1).
        pairs.Sort((a, b) => CompareBytewise(a.KeyCbor, b.KeyCbor));
        Cbor.EncodeMapHead(output, (ulong)pairs.Count);
        foreach (var p in pairs)
        {
            output.AddRange(p.KeyCbor);
            output.AddRange(p.ValueCbor);
        }
    }

    private static int CompareBytewise(byte[] a, byte[] b)
    {
        var n = Math.Min(a.Length, b.Length);
        for (var i = 0; i < n; i++)
        {
            var d = a[i].CompareTo(b[i]);
            if (d != 0) return d;
        }
        return a.Length.CompareTo(b.Length);
    }
}
