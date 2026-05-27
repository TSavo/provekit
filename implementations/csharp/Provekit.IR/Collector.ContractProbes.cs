// SPDX-License-Identifier: Apache-2.0

namespace Provekit.IR;

internal static class CollectorContractProbes
{
    internal static int EmptyFinishLength()
    {
        Collector.BeginCollecting();
        return Collector.Finish().Count;
    }

    internal static int MustAppendCount()
    {
        Collector.BeginCollecting();
        Collector.Must("collector_probe", Predicates.And());
        return Collector.Finish().Count;
    }

    internal static int ThrowsOnAllNull()
    {
        try
        {
            Collector.Contract("collector_empty");
            return 0;
        }
        catch (InvalidOperationException)
        {
            return 1;
        }
    }
}
