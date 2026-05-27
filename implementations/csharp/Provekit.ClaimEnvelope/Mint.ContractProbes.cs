// SPDX-License-Identifier: Apache-2.0

namespace Provekit.ClaimEnvelope;

internal static class MintContractProbes
{
    internal static int ThrowsOnEmptyContract()
    {
        try
        {
            _ = Mint.MintContract(EmptyContractArgs());
            return 0;
        }
        catch (InvalidOperationException)
        {
            return 1;
        }
    }

    private static MintContractArgs EmptyContractArgs() => new()
    {
        ContractName = "empty",
        ProducedBy = "csharp-contracts",
        ProducedAt = "2026-04-30T12:00:00.000Z",
        Authoring = new Authoring.KitAuthor("csharp-contracts"),
        SignerSeed = FoundationSeed,
    };

    private static readonly byte[] FoundationSeed = Enumerable.Repeat((byte)0x42, 32).ToArray();
}
