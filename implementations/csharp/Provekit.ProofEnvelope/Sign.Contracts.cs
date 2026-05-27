// SPDX-License-Identifier: Apache-2.0

namespace Provekit.ProofEnvelope;

internal static class SignContracts
{
    internal static int csharp_sign_prefix_length_eq_8()
    {
        if (Sign.Ed25519SigPrefix.Length != 8) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_sign_raw_signature_length_eq_64(byte[] seed, byte[] message)
    {
        if (seed.Length != 32) throw new InvalidOperationException("contract");
        if (Sign.Ed25519SignWithSeed(seed, message).Length != 64) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_sign_string_is_deterministic(byte[] seed, byte[] message)
    {
        if (seed.Length != 32) throw new InvalidOperationException("contract");
        if (Sign.Ed25519SignString(seed, message) != Sign.Ed25519SignString(seed, message))
        {
            throw new InvalidOperationException("contract");
        }
        return 1;
    }

    internal static int csharp_sign_pubkey_string_length_eq_52(byte[] seed)
    {
        if (seed.Length != 32) throw new InvalidOperationException("contract");
        if (Sign.Ed25519PubkeyString(seed).Length != 52) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_sign_string_length_eq_96(byte[] seed, byte[] message)
    {
        if (seed.Length != 32) throw new InvalidOperationException("contract");
        if (Sign.Ed25519SignString(seed, message).Length != 96) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_sign_verify_round_trip(byte[] seed, byte[] message)
    {
        if (seed.Length != 32) throw new InvalidOperationException("contract");
        if (!Sign.Ed25519VerifyString(
                Sign.Ed25519PubkeyString(seed),
                Sign.Ed25519SignString(seed, message),
                message))
        {
            throw new InvalidOperationException("contract");
        }
        return 1;
    }

    internal static int csharp_sign_verify_rejects_malformed_prefix(byte[] message)
    {
        if (Sign.Ed25519VerifyString("malformed", "malformed", message)) throw new InvalidOperationException("contract");
        return 1;
    }
}
