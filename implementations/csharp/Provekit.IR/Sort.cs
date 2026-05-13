// SPDX-License-Identifier: Apache-2.0

namespace Provekit.IR;

/// <summary>
/// IR-JSON v1.4.0 sort. Primitive / Function / Dependent tagged unions.
/// </summary>
public abstract record Sort
{
    public sealed record Primitive(string Name) : Sort;
    public sealed record Function(Sort[] Args, Sort Return) : Sort;
    public sealed record Dependent(string Name, string IndexVar, Sort IndexSort) : Sort;
    public sealed record RegionSort(string Name) : Sort;

    public static Sort Int => new Primitive("Int");
    public static Sort Real => new Primitive("Real");
    public static Sort String => new Primitive("String");
    public static Sort Bool => new Primitive("Bool");
}
