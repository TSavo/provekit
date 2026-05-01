// SPDX-License-Identifier: Apache-2.0
//
// Deterministic CBOR encoder. RFC 8949 §4.2.1 "Core Deterministic
// Encoding" rules:
//
//   - shortest-form integer head encoding (smallest of immediate /
//     u8 / u16 / u32 / u64)
//   - definite-length items only
//   - map keys sorted by bytewise lex order of their CBOR-encoded form
//
// We emit only the major types we need: unsigned int, byte string,
// text string, array, map. Mirrors
// implementations/rust/provekit-proof-envelope/src/cbor.rs and
// implementations/cpp/provekit/proof-envelope/cbor.cpp 1:1.

namespace Provekit.ProofEnvelope;

public enum CborMajor : byte
{
    UnsignedInt = 0,
    ByteString = 2,
    TextString = 3,
    Array = 4,
    Map = 5,
}

public static class Cbor
{
    /// <summary>
    /// Append a CBOR head: 1 byte (immediate), 2 bytes (uint8), 3 bytes
    /// (uint16), 5 bytes (uint32), or 9 bytes (uint64). Always shortest
    /// form per §4.2.1.
    /// </summary>
    public static void AppendHead(List<byte> output, CborMajor major, ulong arg)
    {
        var mt = (byte)((byte)major << 5);
        if (arg < 24)
        {
            output.Add((byte)(mt | (byte)arg));
            return;
        }
        if (arg <= 0xFF)
        {
            output.Add((byte)(mt | 24));
            output.Add((byte)arg);
            return;
        }
        if (arg <= 0xFFFF)
        {
            output.Add((byte)(mt | 25));
            output.Add((byte)(arg >> 8));
            output.Add((byte)arg);
            return;
        }
        if (arg <= 0xFFFFFFFF)
        {
            output.Add((byte)(mt | 26));
            output.Add((byte)(arg >> 24));
            output.Add((byte)(arg >> 16));
            output.Add((byte)(arg >> 8));
            output.Add((byte)arg);
            return;
        }
        output.Add((byte)(mt | 27));
        for (var i = 7; i >= 0; i--)
        {
            output.Add((byte)(arg >> (i * 8)));
        }
    }

    public static void EncodeUint(List<byte> output, ulong value) =>
        AppendHead(output, CborMajor.UnsignedInt, value);

    public static void EncodeBstr(List<byte> output, ReadOnlySpan<byte> bytes)
    {
        AppendHead(output, CborMajor.ByteString, (ulong)bytes.Length);
        for (var i = 0; i < bytes.Length; i++) output.Add(bytes[i]);
    }

    public static void EncodeTstr(List<byte> output, string utf8)
    {
        var bytes = System.Text.Encoding.UTF8.GetBytes(utf8);
        AppendHead(output, CborMajor.TextString, (ulong)bytes.Length);
        output.AddRange(bytes);
    }

    public static void EncodeArrayHead(List<byte> output, ulong count) =>
        AppendHead(output, CborMajor.Array, count);

    public static void EncodeMapHead(List<byte> output, ulong count) =>
        AppendHead(output, CborMajor.Map, count);
}
