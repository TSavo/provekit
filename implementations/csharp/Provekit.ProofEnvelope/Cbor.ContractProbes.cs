// SPDX-License-Identifier: Apache-2.0

namespace Provekit.ProofEnvelope;

internal static class CborContractProbes
{
    internal static int HeadByteCount(ulong arg)
    {
        var output = new List<byte>();
        Cbor.AppendHead(output, CborMajor.UnsignedInt, arg);
        return output.Count;
    }

    internal static string EncodedTstr(string text)
    {
        var output = new List<byte>();
        Cbor.EncodeTstr(output, text);
        return Convert.ToBase64String(output.ToArray());
    }

    internal static int EncodedTstrLength(string text)
    {
        var output = new List<byte>();
        Cbor.EncodeTstr(output, text);
        return output.Count;
    }

    internal static string EncodedBstr(byte[] bytes)
    {
        var output = new List<byte>();
        Cbor.EncodeBstr(output, bytes);
        return Convert.ToBase64String(output.ToArray());
    }
}
