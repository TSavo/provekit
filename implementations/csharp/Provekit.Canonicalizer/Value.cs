// SPDX-License-Identifier: Apache-2.0
//
// Tiny JSON value tree used by the JCS encoder. Mirrors
// implementations/rust/provekit-canonicalizer/src/value.rs and
// implementations/cpp/provekit/canonicalizer/value.hpp.
//
// Insertion-order is preserved for objects; the JCS encoder re-sorts
// keys at emit time per RFC 8785 §3.2.3 (Unicode code point order; for
// ASCII keys this collapses to byte-order).

namespace Provekit.Canonicalizer;

public enum ValueKind
{
    Null,
    Bool,
    Integer,
    String,
    Array,
    Object,
}

/// <summary>
/// Immutable JSON value used by the canonicalizer. Construct via
/// <see cref="Value.Null"/>, <see cref="Value.Boolean"/>,
/// <see cref="Value.Integer"/>, <see cref="Value.String"/>,
/// <see cref="Value.Array"/>, <see cref="Value.Object"/>.
/// </summary>
public sealed class Value
{
    public ValueKind Kind { get; }
    private readonly bool _bool;
    private readonly long _int;
    private readonly string? _str;
    private readonly IReadOnlyList<Value>? _arr;
    private readonly IReadOnlyList<KeyValuePair<string, Value>>? _obj;

    private Value(ValueKind kind, bool b = false, long i = 0, string? s = null,
                  IReadOnlyList<Value>? arr = null,
                  IReadOnlyList<KeyValuePair<string, Value>>? obj = null)
    {
        Kind = kind;
        _bool = b;
        _int = i;
        _str = s;
        _arr = arr;
        _obj = obj;
    }

    public static Value Null { get; } = new(ValueKind.Null);
    public static Value True { get; } = new(ValueKind.Bool, b: true);
    public static Value False { get; } = new(ValueKind.Bool, b: false);

    public static Value Boolean(bool b) => b ? True : False;
    public static Value Integer(long n) => new(ValueKind.Integer, i: n);
    public static Value String(string s) => new(ValueKind.String, s: s ?? throw new ArgumentNullException(nameof(s)));

    public static Value Array(IEnumerable<Value> items) =>
        new(ValueKind.Array, arr: items.ToArray());

    public static Value Array(params Value[] items) =>
        new(ValueKind.Array, arr: items);

    public static Value Object(IEnumerable<KeyValuePair<string, Value>> entries) =>
        new(ValueKind.Object, obj: entries.ToArray());

    public static Value Object(params (string Key, Value Val)[] entries)
    {
        var arr = new KeyValuePair<string, Value>[entries.Length];
        for (var i = 0; i < entries.Length; i++)
        {
            arr[i] = new KeyValuePair<string, Value>(entries[i].Key, entries[i].Val);
        }
        return new Value(ValueKind.Object, obj: arr);
    }

    public bool AsBool() => Kind == ValueKind.Bool ? _bool : throw new InvalidOperationException("not bool");
    public long AsInt() => Kind == ValueKind.Integer ? _int : throw new InvalidOperationException("not integer");
    public string AsString() => Kind == ValueKind.String ? _str! : throw new InvalidOperationException("not string");
    public IReadOnlyList<Value> AsArray() => Kind == ValueKind.Array ? _arr! : throw new InvalidOperationException("not array");
    public IReadOnlyList<KeyValuePair<string, Value>> AsObject() => Kind == ValueKind.Object ? _obj! : throw new InvalidOperationException("not object");
}
