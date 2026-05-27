// SPDX-License-Identifier: Apache-2.0

namespace Provekit.ProofEnvelope;

internal static class CborContracts
{
    internal static int csharp_cbor_head_immediate_length_eq_1(ulong arg)
    {
        if (arg < 24 && CborContractProbes.HeadByteCount(arg) != 1) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_cbor_encode_tstr_is_deterministic(string text)
    {
        if (CborContractProbes.EncodedTstr(text) != CborContractProbes.EncodedTstr(text))
        {
            throw new InvalidOperationException("contract");
        }
        return 1;
    }

    internal static int csharp_cbor_encode_bstr_is_deterministic(byte[] bytes)
    {
        if (CborContractProbes.EncodedBstr(bytes) != CborContractProbes.EncodedBstr(bytes))
        {
            throw new InvalidOperationException("contract");
        }
        return 1;
    }

    internal static int csharp_cbor_empty_tstr_length_eq_1()
    {
        if (CborContractProbes.EncodedTstrLength("") != 1) throw new InvalidOperationException("contract");
        return 1;
    }
}
