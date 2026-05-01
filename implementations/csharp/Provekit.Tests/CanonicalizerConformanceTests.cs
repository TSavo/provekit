// SPDX-License-Identifier: Apache-2.0
//
// Cross-language conformance test for Provekit.Canonicalizer. Mirrors
// implementations/cpp/provekit/canonicalizer/canonicalizer_test.cpp 1:1.
//
// EXPECTED VALUES DERIVED FROM THE SPEC, NOT FROM A REFERENCE IMPL:
//
// The fixture is the canonical AST for the formula `x > 0` where x is
// de Bruijn index 0 of sort Int:
//
//   {"args":[{"index":0,"kind":"var","sort":{"kind":"primitive","name":"Int"}},
//            {"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":0}],
//    "kind":"atomic","predicate":">"}
//
// (rendered with all keys sorted per §7.3, no whitespace per §7.4,
//  numbers per §7.6, integer 0 emits digit "0").
//
// BLAKE3-512 of those bytes (v1.1.0 protocol hash, full 64-byte digest):
//   c592f83501c1cfbb9ae69fe89b7738896d0309f1493e3b3f89dbbe78ebbcdb5d
//   6a519307b558b89e37a68d0443a564719d57f30e6a53f4d014b48e9d7fba23a5
//
// Any conformant implementation in any language must produce these
// exact bytes and this exact hash. If C# doesn't match, EITHER the C#
// impl is wrong OR the spec has a hole. Both are bugs we surface.

using System.Text;
using Provekit.Canonicalizer;
using Xunit;

namespace Provekit.Tests;

public class CanonicalizerConformanceTests
{
    private const string ExpectedBytes =
        @"{""args"":[{""index"":0,""kind"":""var"",""sort"":{""kind"":""primitive"",""name"":""Int""}}," +
        @"{""kind"":""const"",""sort"":{""kind"":""primitive"",""name"":""Int""},""value"":0}]," +
        @"""kind"":""atomic"",""predicate"":"">""}";

    private const string ExpectedPropertyHash =
        "blake3-512:" +
        "c592f83501c1cfbb9ae69fe89b7738896d0309f1493e3b3f89dbbe78ebbcdb5d" +
        "6a519307b558b89e37a68d0443a564719d57f30e6a53f4d014b48e9d7fba23a5";

    /// <summary>
    /// Build the canonical AST for `x &gt; 0` (x = de Bruijn 0, sort Int).
    /// Construction order doesn't matter — the encoder sorts keys per §7.3.
    /// </summary>
    private static Value MakeFixture()
    {
        var intSort = Value.Object(
            ("kind", Value.String("primitive")),
            ("name", Value.String("Int"))
        );
        var varX = Value.Object(
            ("kind", Value.String("var")),
            ("index", Value.Integer(0)),
            ("sort", intSort)
        );
        var constZero = Value.Object(
            ("kind", Value.String("const")),
            ("value", Value.Integer(0)),
            ("sort", intSort)
        );
        return Value.Object(
            ("kind", Value.String("atomic")),
            ("predicate", Value.String(">")),
            ("args", Value.Array(varX, constZero))
        );
    }

    [Fact]
    public void Section7_JcsBytes_AreByteIdenticalToSpec()
    {
        var actual = Jcs.Encode(MakeFixture());
        Assert.Equal(ExpectedBytes, actual);

        // And byte-for-byte at the UTF-8 layer (the layer the protocol locks).
        var actualBytes = Encoding.UTF8.GetBytes(actual);
        var expectedBytes = Encoding.UTF8.GetBytes(ExpectedBytes);
        Assert.Equal(expectedBytes, actualBytes);
    }

    [Fact]
    public void Section11_PropertyHash_MatchesSpecDerivedExpected()
    {
        var bytes = Jcs.EncodeUtf8(MakeFixture());
        var actual = Hash.Blake3_512(bytes);
        Assert.Equal(ExpectedPropertyHash, actual);
    }

    [Theory]
    [InlineData("≥")] // ≥
    [InlineData("≤")] // ≤
    [InlineData("≠")] // ≠
    public void UnicodePredicateRoundTripsVerbatim(string sym)
    {
        // Per protocol-catalog-format §5: atomic predicate names use exactly
        // these UTF-8 sequences. Cross-language hash agreement depends on
        // the encoder NOT re-encoding them per byte.
        var v = Value.String(sym);
        var encoded = Jcs.Encode(v);
        var expected = "\"" + sym + "\"";
        Assert.Equal(expected, encoded);

        // Inner UTF-8 bytes must match the input's UTF-8 bytes.
        var encodedUtf8 = Encoding.UTF8.GetBytes(encoded);
        var symUtf8 = Encoding.UTF8.GetBytes(sym);
        var inner = encodedUtf8[1..^1];
        Assert.Equal(symUtf8, inner);
    }

    [Fact]
    public void MixedAsciiAndUnicodePreserved()
    {
        var v = Value.String("x ≥ 0");
        var encoded = Jcs.Encode(v);
        Assert.Equal("\"x ≥ 0\"", encoded);
    }

    [Fact]
    public void UnicodeInObjectNameField_MatchesPeerByteForByte()
    {
        // {"name":"≥"} canonicalizes to literally {"name":"\xe2\x89\xa5"}.
        var v = Value.Object(("name", Value.String("≥")));
        var encoded = Jcs.EncodeUtf8(v);
        var expected = new byte[]
        {
            (byte)'{', (byte)'"', (byte)'n', (byte)'a', (byte)'m', (byte)'e', (byte)'"', (byte)':',
            (byte)'"', 0xe2, 0x89, 0xa5, (byte)'"', (byte)'}',
        };
        Assert.Equal(expected, encoded);
    }

    [Fact]
    public void EmptyInput_BlakeHash_HasPrefixAnd128HexChars()
    {
        var h = Hash.Blake3_512(ReadOnlySpan<byte>.Empty);
        Assert.StartsWith("blake3-512:", h);
        Assert.Equal("blake3-512:".Length + 128, h.Length);
    }

    [Fact]
    public void Blake3_512_OfHello_MatchesRustPeerVector()
    {
        // BLAKE3 of "hello" with 64-byte XOF output. Computed against the
        // Rust peer's `blake3_512_of(b"hello")` and the official `blake3`
        // crate's XOF interface; pinned here as a cross-language sanity
        // anchor so a future hash-package swap can't silently truncate.
        const string expected =
            "blake3-512:" +
            "ea8f163db38682925e4491c5e58d4bb3506ef8c14eb78a86e908c5624a67200f" +
            "e992405f0d785b599a2e3387f6d34d01faccfeb22fb697ef3fd53541241a338c";
        Assert.Equal(expected, Hash.Blake3_512Utf8("hello"));
    }

    [Fact]
    public void Blake3_512_OfEmpty_MatchesRustPeerVector()
    {
        // BLAKE3 of empty input with 64-byte XOF output, cross-checked
        // against the Rust peer.
        const string expected =
            "blake3-512:" +
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262" +
            "e00f03e7b69af26b7faaf09fcd333050338ddfe085b8cc869ca98b206c08243a";
        Assert.Equal(expected, Hash.Blake3_512(ReadOnlySpan<byte>.Empty));
    }

    [Fact]
    public void Object_KeysSortedByOrdinal()
    {
        var v = Value.Object(
            ("b", Value.Integer(1)),
            ("a", Value.String("x"))
        );
        Assert.Equal("{\"a\":\"x\",\"b\":1}", Jcs.Encode(v));
    }

    [Fact]
    public void EmptyObjectAndArray_RenderEmpty()
    {
        Assert.Equal("{}", Jcs.Encode(Value.Object()));
        Assert.Equal("[]", Jcs.Encode(Value.Array()));
    }

    [Fact]
    public void StringEscapes_QuoteAndBackslash()
    {
        var v = Value.String("a\"b\\c");
        Assert.Equal("\"a\\\"b\\\\c\"", Jcs.Encode(v));
    }
}
