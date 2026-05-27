// SPDX-License-Identifier: Apache-2.0

namespace Provekit.Canonicalizer;

internal static class ValueContracts
{
    internal static int csharp_value_boolean_true_is_singleton()
    {
        if (!ReferenceEquals(Value.Boolean(true), Value.True)) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_value_boolean_false_is_singleton()
    {
        if (!ReferenceEquals(Value.Boolean(false), Value.False)) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_value_integer_round_trips(long value)
    {
        if (AsIntOfInteger(value) != value) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_value_string_round_trips(string value)
    {
        if (AsStringOfString(value) != value) throw new InvalidOperationException("contract");
        return 1;
    }

    private static long AsIntOfInteger(long value) => Value.Integer(value).AsInt();

    private static string AsStringOfString(string value) => Value.String(value).AsString();
}
