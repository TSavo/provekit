// SPDX-License-Identifier: Apache-2.0

namespace Provekit.IR;

/// <summary>
/// IR-JSON v1.1.0 sort. Currently primitive-only ("Int", "Real",
/// "String", "Bool"). Mirrors the Rust/C++ peers.
/// </summary>
public sealed record Sort(string Name)
{
    public static Sort Int => new("Int");
    public static Sort Real => new("Real");
    public static Sort String => new("String");
    public static Sort Bool => new("Bool");
}
