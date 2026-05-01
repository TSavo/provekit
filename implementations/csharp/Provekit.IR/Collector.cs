// SPDX-License-Identifier: Apache-2.0
//
// Contract collector + bridge collector. `BeginCollecting` clears
// state; `Must`/`Contract` register declarations; `Finish` returns
// the accumulated list and re-clears. Same shape as Rust/C++ kits.
//
// The collector is process-local (static); kit usage is single-shot
// per contract emission, so this is fine. No thread-safety beyond
// the internal lock.

namespace Provekit.IR;

public sealed record ContractDecl(
    string Name,
    Formula? Pre,
    Formula? Post,
    Formula? Inv,
    string OutBinding);

public sealed record BridgeDecl(
    string SourceSymbol,
    string SourceLayer,
    string TargetContractName,
    string TargetLayer,
    IReadOnlyList<string> IrArgSorts,
    string IrReturnSort,
    string Notes);

public static class Collector
{
    private static readonly object _lock = new();
    private static List<ContractDecl> _contracts = new();
    private static List<BridgeDecl> _bridges = new();

    /// <summary>Reset all collector state and the quantifier counter.</summary>
    public static void BeginCollecting()
    {
        lock (_lock)
        {
            _contracts = new List<ContractDecl>();
            _bridges = new List<BridgeDecl>();
        }
        Quantifiers.ResetCounter();
    }

    /// <summary>
    /// Register a contract. At least one of pre/post/inv must be
    /// non-null; outBinding defaults to "out". Mirrors the C++ kit's
    /// fail-fast check.
    /// </summary>
    public static void Contract(
        string name,
        Formula? pre = null,
        Formula? post = null,
        Formula? inv = null,
        string outBinding = "out")
    {
        if (pre is null && post is null && inv is null)
        {
            throw new InvalidOperationException(
                $"contract(\"{name}\"): at least one of pre/post/inv must be non-null");
        }
        lock (_lock)
        {
            _contracts.Add(new ContractDecl(name, pre, post, inv, outBinding));
        }
    }

    /// <summary>Precondition-only convenience alias for Contract.</summary>
    public static void Must(string name, Formula precondition) =>
        Contract(name, pre: precondition);

    /// <summary>Drain the collector and return all registered contracts.</summary>
    public static IReadOnlyList<ContractDecl> Finish()
    {
        lock (_lock)
        {
            var result = _contracts;
            _contracts = new List<ContractDecl>();
            return result;
        }
    }

    public static void RegisterBridge(BridgeDecl decl)
    {
        lock (_lock)
        {
            _bridges.Add(decl);
        }
    }

    public static IReadOnlyList<BridgeDecl> FinishBridges()
    {
        lock (_lock)
        {
            var result = _bridges;
            _bridges = new List<BridgeDecl>();
            return result;
        }
    }
}
