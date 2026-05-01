// SPDX-License-Identifier: Apache-2.0
//
// BLAKE3-512 helper. v1.1.0 of the protocol mandates self-identifying
// hashes of the form:
//
//   "blake3-512:" + lowercase-hex(64-byte-digest)
//
// Uses the Blake3 NuGet package's XOF interface to produce 64 bytes
// (128 hex chars). NO truncation. v1.1.0 is scorched earth: BLAKE3-512
// is the only hash function permitted, always 512 bits wide.

using System.Globalization;
using System.Text;
using Blake3;

namespace Provekit.Canonicalizer;

public static class Hash
{
    public const string Blake3_512_Prefix = "blake3-512:";

    /// <summary>
    /// Returns the spec's self-identifying string form:
    /// <c>"blake3-512:" + lowercase-hex(64-byte digest)</c>.
    /// </summary>
    public static string Blake3_512(ReadOnlySpan<byte> input)
    {
        // Blake3.Hasher exposes Finalize(Span<byte>) (XOF). The default
        // Hash() returns 32 bytes; the spec requires 64. We finalize
        // directly into a 64-byte buffer.
        Span<byte> digest = stackalloc byte[64];
        using var hasher = Hasher.New();
        // Blake3.Hasher's UpdateWithJoin rejects empty spans; only call
        // Update when there's data. Empty input is still a valid hash:
        // the XOF finalizer will produce the BLAKE3-of-empty digest.
        if (!input.IsEmpty)
        {
            hasher.UpdateWithJoin(input);
        }
        hasher.Finalize(digest);

        var sb = new StringBuilder(Blake3_512_Prefix.Length + 128);
        sb.Append(Blake3_512_Prefix);
        AppendLowerHex(sb, digest);
        return sb.ToString();
    }

    /// <summary>Convenience: hash a UTF-8 string.</summary>
    public static string Blake3_512Utf8(string s) =>
        Blake3_512(Encoding.UTF8.GetBytes(s));

    private static void AppendLowerHex(StringBuilder sb, ReadOnlySpan<byte> bytes)
    {
        const string Hex = "0123456789abcdef";
        for (var i = 0; i < bytes.Length; i++)
        {
            sb.Append(Hex[bytes[i] >> 4]);
            sb.Append(Hex[bytes[i] & 0xF]);
        }
    }
}
