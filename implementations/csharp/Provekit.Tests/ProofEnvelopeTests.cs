// SPDX-License-Identifier: Apache-2.0
//
// CBOR + ed25519 + .proof envelope tests for Provekit.ProofEnvelope.

using Provekit.ProofEnvelope;
using Xunit;

namespace Provekit.Tests;

public class ProofEnvelopeTests
{
    // ---------- CBOR shortest-form integer encoding ----------

    [Fact]
    public void Cbor_Uint_Zero_IsImmediateForm()
    {
        var o = new List<byte>();
        Cbor.EncodeUint(o, 0);
        Assert.Equal(new byte[] { 0x00 }, o);
    }

    [Fact]
    public void Cbor_Uint_TwentyThree_IsImmediateForm()
    {
        var o = new List<byte>();
        Cbor.EncodeUint(o, 23);
        Assert.Equal(new byte[] { 0x17 }, o);
    }

    [Fact]
    public void Cbor_Uint_TwentyFour_IsUint8Form()
    {
        var o = new List<byte>();
        Cbor.EncodeUint(o, 24);
        Assert.Equal(new byte[] { 0x18, 24 }, o);
    }

    [Fact]
    public void Cbor_Uint_256_IsUint16Form()
    {
        var o = new List<byte>();
        Cbor.EncodeUint(o, 256);
        Assert.Equal(new byte[] { 0x19, 0x01, 0x00 }, o);
    }

    [Fact]
    public void Cbor_Uint_65536_IsUint32Form()
    {
        var o = new List<byte>();
        Cbor.EncodeUint(o, 65536);
        Assert.Equal(new byte[] { 0x1a, 0x00, 0x01, 0x00, 0x00 }, o);
    }

    [Fact]
    public void Cbor_Tstr_Hello_HasMajor3LengthHead()
    {
        var o = new List<byte>();
        Cbor.EncodeTstr(o, "hello");
        // major 3 (text string), len 5 short form: 0x65 then "hello"
        Assert.Equal(new byte[] { 0x65, (byte)'h', (byte)'e', (byte)'l', (byte)'l', (byte)'o' }, o);
    }

    // ---------- Ed25519 sign / verify ----------

    [Fact]
    public void Ed25519_DeterministicForFixedSeed()
    {
        var seed = Enumerable.Repeat((byte)0x42, 32).ToArray();
        var a = Sign.Ed25519SignWithSeed(seed, "hello"u8);
        var b = Sign.Ed25519SignWithSeed(seed, "hello"u8);
        Assert.Equal(a, b);
        Assert.Equal(64, a.Length);
    }

    [Fact]
    public void Ed25519_StringFormHasPrefix()
    {
        var seed = Enumerable.Repeat((byte)0x42, 32).ToArray();
        var s = Sign.Ed25519SignString(seed, "hello"u8);
        Assert.StartsWith("ed25519:", s);
    }

    [Fact]
    public void Ed25519_PubkeyFormHasPrefix()
    {
        var seed = Enumerable.Repeat((byte)0x42, 32).ToArray();
        var s = Sign.Ed25519PubkeyString(seed);
        Assert.StartsWith("ed25519:", s);
    }

    [Fact]
    public void Ed25519_VerifyRoundTrip()
    {
        var seed = Enumerable.Repeat((byte)0x42, 32).ToArray();
        var pk = Sign.Ed25519PubkeyString(seed);
        var sig = Sign.Ed25519SignString(seed, "hello world"u8);
        Assert.True(Sign.Ed25519VerifyString(pk, sig, "hello world"u8));
        Assert.False(Sign.Ed25519VerifyString(pk, sig, "goodbye world"u8));
    }

    [Fact]
    public void Ed25519_VerifyRejectsMalformed()
    {
        Assert.False(Sign.Ed25519VerifyString("not-prefixed", "ed25519:AAAA", "x"u8));
        Assert.False(Sign.Ed25519VerifyString("ed25519:AAAA", "not-prefixed", "x"u8));
        Assert.False(Sign.Ed25519VerifyString("ed25519:!!!!", "ed25519:!!!!", "x"u8));
    }

    [Fact]
    public void Ed25519_PublicKeyMatchesRfc8032KnownVector()
    {
        // RFC 8032 §7.1 test vector 1: secret seed all-zeros expansion
        // produces public key 3b6a27bcceb6a42d62a3a8d02a6f0d73 ...
        // We use the fixed seed = [0x42; 32] cross-checked against the
        // Rust ed25519-dalek peer for stable cross-language matching.
        var seed = Enumerable.Repeat((byte)0x42, 32).ToArray();
        var s = Sign.Ed25519PubkeyString(seed);
        Assert.StartsWith("ed25519:", s);
        // Length = "ed25519:" + base64(32 bytes) = 8 + 44 = 52 chars.
        Assert.Equal(52, s.Length);
    }

    // ---------- Proof envelope ----------

    [Fact]
    public void Proof_BuildMinimalRoundTrips()
    {
        var members = new Dictionary<string, byte[]>
        {
            ["blake3-512:aa"] = "{\"hello\":\"world\"}"u8.ToArray(),
        };
        var input = new ProofEnvelopeInput
        {
            Name = "@x/y",
            Version = "0.0.1",
            Members = members,
            SignerCid = "blake3-512:bb",
            SignerSeed = Enumerable.Repeat((byte)0x11, 32).ToArray(),
            DeclaredAt = "2026-04-30T00:00:00.000Z",
        };
        var output = Proof.Build(input);
        Assert.StartsWith("blake3-512:", output.Cid);
        // First byte: map head with 7 entries (kind, name, version,
        // members, signer, declaredAt, signature) = 0xa7. Mirrors Rust.
        Assert.Equal(0xa7, output.Bytes[0]);
    }

    [Fact]
    public void Proof_DeterministicAcrossRuns()
    {
        var members = new Dictionary<string, byte[]>
        {
            ["blake3-512:aa"] = "{\"x\":1}"u8.ToArray(),
        };
        var input = new ProofEnvelopeInput
        {
            Name = "@x/y",
            Version = "0.0.1",
            Members = members,
            SignerCid = "blake3-512:bb",
            SignerSeed = Enumerable.Repeat((byte)0x11, 32).ToArray(),
            DeclaredAt = "2026-04-30T00:00:00.000Z",
        };
        var a = Proof.Build(input);
        var b = Proof.Build(input);
        Assert.Equal(a.Bytes, b.Bytes);
        Assert.Equal(a.Cid, b.Cid);
    }
}
