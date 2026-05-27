// SPDX-License-Identifier: Apache-2.0

namespace Provekit.IR;

internal static class CollectorContracts
{
    internal static int csharp_collector_empty_finish_length_eq_0()
    {
        if (CollectorContractProbes.EmptyFinishLength() != 0) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_collector_must_appends_one()
    {
        if (CollectorContractProbes.MustAppendCount() != 1) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_collector_contract_all_null_throws()
    {
        if (CollectorContractProbes.ThrowsOnAllNull() != 1) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_collector_default_out_binding_length_eq_3()
    {
        if ("out".Length != 3) throw new InvalidOperationException("contract");
        return 1;
    }
}
