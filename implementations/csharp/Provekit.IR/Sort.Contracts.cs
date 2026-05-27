// SPDX-License-Identifier: Apache-2.0

namespace Provekit.IR;

internal static class SortContracts
{
    internal static int csharp_sort_int_name_is_Int()
    {
        if (((Sort.Primitive)Sort.Int).Name != "Int") throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_sort_real_name_is_Real()
    {
        if (((Sort.Primitive)Sort.Real).Name != "Real") throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_sort_string_name_is_String()
    {
        if (((Sort.Primitive)Sort.String).Name != "String") throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_sort_bool_name_is_Bool()
    {
        if (((Sort.Primitive)Sort.Bool).Name != "Bool") throw new InvalidOperationException("contract");
        return 1;
    }
}
