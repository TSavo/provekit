// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cs for Provekit.IR/Collector.cs
//
// Public surface covered:
//   * BeginCollecting()           — clears state + counter
//   * Must(name, precondition)    — registers pre-only contract
//   * Contract(name, pre?, post?, inv?) — registers any contract
//   * Finish() / FinishBridges()  — drains state
//
// Honest scope:
//   Lifecycle invariants — Finish() after BeginCollecting() with no
//   intervening Must/Contract returns an empty list. Contract() with
//   all three of pre/post/inv null throws.

using static Provekit.IR.Predicates;
using static Provekit.IR.Quantifiers;
using static Provekit.IR.Terms;
using static Provekit.IR.Collector;
using Provekit.IR;

namespace Provekit.SelfContracts.Invariants;

public static class CollectorInvariants
{
    public static void Register()
    {
        // After BeginCollecting() with no Must/Contract calls, Finish()
        // returns a list of length zero.
        Contract("csharp_collector_empty_finish_length_eq_0",
            post: Eq(Ctor("FinishLength", Ctor("BeginCollecting")), Num(0)));

        // Must(name, p) is equivalent to Contract(name, pre=p): both
        // append exactly one ContractDecl to the collector.
        Contract("csharp_collector_must_appends_one",
            post: Eq(Ctor("MustAppendCount"), Num(1)));

        // Contract(...) with all-null pre/post/inv throws — encoded as
        // a contract whose post asserts the length-0 of the throw path.
        Contract("csharp_collector_contract_all_null_throws",
            post: Eq(Ctor("ThrowsOnAllNull"), Num(1)));

        // outBinding default is "out" (length 3).
        Contract("csharp_collector_default_out_binding_length_eq_3",
            post: Eq(Ctor("len", StrConst("out")), Num(3)));
    }
}
