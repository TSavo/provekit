// SPDX-License-Identifier: Apache-2.0
//
// JCS-JSON encoder (RFC 8785). C# peer mirroring
// implementations/rust/provekit-canonicalizer/src/jcs.rs and
// implementations/cpp/provekit/canonicalizer/jcs.cpp 1:1.
//
// Rules (canonicalization grammar §7):
//   - Object keys sorted by Unicode code-point order. For ASCII-only
//     keys this collapses to byte-order; the protocol's keys are all
//     ASCII so byte-order suffices. We compare using Ordinal (code-unit
//     order over UTF-16); same result for the BMP non-surrogate range.
//   - Numbers: integers serialized as plain decimal digits (we only
//     carry int64; floats are not produced by the kit/mint flow).
//   - Strings: UTF-8 verbatim, escape `"` and `\\` and U+0000..U+001F
//     as `\u00XX` (lowercase hex). RFC 8785 also permits the named
//     short escapes (\n etc.); the C++/Rust peers chose `\u00XX` for
//     determinism: we match.
//   - true / false / null verbatim.
//   - No whitespace anywhere.
//
// Output is a UTF-8 byte sequence (returned as a String over UTF-16
// code units, but every non-ASCII char emitted is a single UTF-16 code
// unit whose UTF-8 byte expansion equals its corresponding input bytes;
// see the unicode round-trip test).

using System.Globalization;
using System.Text;

namespace Provekit.Canonicalizer;

public static class Jcs
{
    /// <summary>
    /// Encode <paramref name="v"/> as a JCS-JSON string per RFC 8785.
    /// The returned string's UTF-8 byte form IS the canonical wire form.
    /// </summary>
    public static string Encode(Value v)
    {
        var sb = new StringBuilder();
        EncodeValue(v, sb);
        return sb.ToString();
    }

    /// <summary>
    /// Encode and return the raw UTF-8 bytes directly.
    /// </summary>
    public static byte[] EncodeUtf8(Value v) => Encoding.UTF8.GetBytes(Encode(v));

    private static void EncodeValue(Value v, StringBuilder sb)
    {
        switch (v.Kind)
        {
            case ValueKind.Null:
                sb.Append("null");
                return;
            case ValueKind.Bool:
                sb.Append(v.AsBool() ? "true" : "false");
                return;
            case ValueKind.Integer:
                // §7.6: ECMA-262 ToString applied to a finite Number; for
                // an integer that's signed-decimal digits, no thousands
                // separator. CultureInfo.InvariantCulture defends against
                // a non-en locale ever sneaking a separator in.
                sb.Append(v.AsInt().ToString(CultureInfo.InvariantCulture));
                return;
            case ValueKind.String:
                EncodeString(v.AsString(), sb);
                return;
            case ValueKind.Array:
                EncodeArray(v.AsArray(), sb);
                return;
            case ValueKind.Object:
                EncodeObject(v.AsObject(), sb);
                return;
        }
        throw new InvalidOperationException("encode_value: unknown ValueKind");
    }

    private static void EncodeArray(IReadOnlyList<Value> items, StringBuilder sb)
    {
        sb.Append('[');
        for (var i = 0; i < items.Count; i++)
        {
            if (i > 0) sb.Append(',');
            EncodeValue(items[i], sb);
        }
        sb.Append(']');
    }

    private static void EncodeObject(IReadOnlyList<KeyValuePair<string, Value>> entries, StringBuilder sb)
    {
        // §7.3: keys sorted by Unicode code-point order. The protocol's
        // keys are all ASCII, so StringComparer.Ordinal (UTF-16 code-unit
        // order) produces the same ordering. Mirrors the Rust/C++ peers.
        var sorted = entries.ToArray();
        Array.Sort(sorted, (a, b) => string.CompareOrdinal(a.Key, b.Key));

        sb.Append('{');
        for (var i = 0; i < sorted.Length; i++)
        {
            if (i > 0) sb.Append(',');
            EncodeString(sorted[i].Key, sb);
            sb.Append(':');
            EncodeValue(sorted[i].Value, sb);
        }
        sb.Append('}');
    }

    // §7.5: encode a string with JSON-required escaping only:
    //   "  → \"
    //   \  → \\
    //   U+0000..U+001F → \u00XX (lowercase hex)
    //   all other code points: verbatim. Non-ASCII characters are kept
    //   as-is; the output's UTF-8 bytes equal the input's UTF-8 bytes
    //   for those characters.
    //
    // We iterate over UTF-16 code units. Surrogate pairs encode the same
    // supplementary code point identically in both UTF-16 and UTF-8, so
    // emitting them verbatim is correct (the StringBuilder preserves the
    // pair, and Encoding.UTF8 reassembles to the right 4-byte UTF-8 form).
    private static void EncodeString(string s, StringBuilder sb)
    {
        sb.Append('"');
        for (var i = 0; i < s.Length; i++)
        {
            var c = s[i];
            if (c == '"')
            {
                sb.Append("\\\"");
            }
            else if (c == '\\')
            {
                sb.Append("\\\\");
            }
            else if (c < 0x20)
            {
                // Lowercase hex \u00XX form, matching Rust/C++ peers.
                sb.Append("\\u00");
                sb.Append(HexLower((c >> 4) & 0xF));
                sb.Append(HexLower(c & 0xF));
            }
            else
            {
                sb.Append(c);
            }
        }
        sb.Append('"');
    }

    private static char HexLower(int n) => (char)(n < 10 ? ('0' + n) : ('a' + (n - 10)));
}
