// SPDX-License-Identifier: Apache-2.0

namespace Provekit.IR;

internal static class PredicatesContracts
{
    internal static int csharp_pred_eq_name_length_eq_1()
    {
        if ("=".Length != 1) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_pred_gt_name_length_eq_1()
    {
        if (">".Length != 1) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_pred_lt_name_length_eq_1()
    {
        if ("<".Length != 1) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_pred_ne_unicode_length_eq_1_chars()
    {
        if ("≠".Length != 1) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_pred_gte_unicode_length_eq_1_chars()
    {
        if ("≥".Length != 1) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_pred_lte_unicode_length_eq_1_chars()
    {
        if ("≤".Length != 1) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_pred_eq_reflexive_construction(long value)
    {
        long copy = value;
        if (value != copy) throw new InvalidOperationException("contract");
        return 1;
    }
}
