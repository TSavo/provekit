// SPDX-License-Identifier: Apache-2.0

namespace Provekit.Canonicalizer;

internal static class HashContracts
{
    internal static int csharp_blake3_512_output_length_eq_139(byte[] input)
    {
        if (Hash.Blake3_512(input).Length != 139) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_blake3_512_is_deterministic(byte[] input)
    {
        if (Hash.Blake3_512(input) != Hash.Blake3_512(input)) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_blake3_512_utf8_output_length_eq_139(string input)
    {
        if (Hash.Blake3_512Utf8(input).Length != 139) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_blake3_512_utf8_is_deterministic(string input)
    {
        if (Hash.Blake3_512Utf8(input) != Hash.Blake3_512Utf8(input)) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_blake3_512_prefix_length_eq_11()
    {
        if (Hash.Blake3_512_Prefix.Length != 11) throw new InvalidOperationException("contract");
        return 1;
    }
}
