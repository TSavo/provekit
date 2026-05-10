// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cs for Provekit.IR/Sort.cs
//
// Public surface covered:
//   * Sort.Int / Real / String / Bool: primitive sort constructors
//
// Honest scope:
//   Sort is a record carrying a Name; the four named primitives MUST
//   carry their canonical name strings. These contracts pin the names.

using static Provekit.IR.Predicates;
using static Provekit.IR.Terms;
using static Provekit.IR.Collector;

namespace Provekit.SelfContracts.Invariants;

public static class SortInvariants
{
    public static void Register()
    {
        // Sort.Int.Name = "Int": names are protocol-locked.
        Contract("csharp_sort_int_name_is_Int",
            post: Eq(Ctor("Name", Ctor("Sort_Int")), StrConst("Int")));

        Contract("csharp_sort_real_name_is_Real",
            post: Eq(Ctor("Name", Ctor("Sort_Real")), StrConst("Real")));

        Contract("csharp_sort_string_name_is_String",
            post: Eq(Ctor("Name", Ctor("Sort_String")), StrConst("String")));

        Contract("csharp_sort_bool_name_is_Bool",
            post: Eq(Ctor("Name", Ctor("Sort_Bool")), StrConst("Bool")));
    }
}
