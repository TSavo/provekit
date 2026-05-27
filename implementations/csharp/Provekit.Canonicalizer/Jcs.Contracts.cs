// SPDX-License-Identifier: Apache-2.0

namespace Provekit.Canonicalizer;

internal static class JcsContracts
{
    internal static int csharp_jcs_encode_is_deterministic(Value value)
    {
        if (Jcs.Encode(value) != Jcs.Encode(value)) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_jcs_encode_output_length_gte_2(Value value)
    {
        if (Jcs.Encode(value).Length < 2) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_jcs_encode_utf8_is_deterministic(Value value)
    {
        if (!BytesEqual(Jcs.EncodeUtf8(value), Jcs.EncodeUtf8(value))) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_jcs_encode_utf8_length_eq_encode_length_for_ascii(string ascii)
    {
        if (IsAscii(ascii) && Jcs.EncodeUtf8(Value.String(ascii)).Length != Jcs.Encode(Value.String(ascii)).Length)
        {
            throw new InvalidOperationException("contract");
        }
        return 1;
    }

    private static bool BytesEqual(byte[] left, byte[] right) => left.SequenceEqual(right);

    private static bool IsAscii(string value) => value.All(c => c <= 0x7f);
}
