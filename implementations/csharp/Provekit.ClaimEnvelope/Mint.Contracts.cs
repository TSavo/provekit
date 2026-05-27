// SPDX-License-Identifier: Apache-2.0

namespace Provekit.ClaimEnvelope;

internal static class MintContracts
{
    internal static int csharp_mint_contract_cid_length_eq_139(string contractName)
    {
        if (MintContractCidLength(contractName) != 139) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_mint_contract_is_deterministic(string contractName)
    {
        if (MintContractBytes(contractName) != MintContractBytes(contractName)) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_mint_bridge_cid_length_eq_139(string sourceSymbol)
    {
        if (MintBridgeCidLength(sourceSymbol) != 139) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_mint_implication_cid_length_eq_139(string antecedentCid)
    {
        if (MintImplicationCidLength(antecedentCid) != 139) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_mint_contract_throws_when_all_pre_post_inv_null()
    {
        if (MintContractProbes.ThrowsOnEmptyContract() != 1) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_mint_default_out_binding()
    {
        if (DefaultOutBinding() != "out") throw new InvalidOperationException("contract");
        return 1;
    }

    private static int MintContractCidLength(string contractName) => Mint.ContractCid(MinimalContractArgs(contractName)).Length;

    private static string MintContractBytes(string contractName) =>
        Convert.ToBase64String(Mint.MintContract(MinimalContractArgs(contractName)).CanonicalBytes);

    private static int MintBridgeCidLength(string sourceSymbol) => Mint.MintBridge(MinimalBridgeArgs(sourceSymbol)).Cid.Length;

    private static int MintImplicationCidLength(string antecedentCid) =>
        Mint.MintImplication(MinimalImplicationArgs(antecedentCid)).Cid.Length;

    private static string DefaultOutBinding() => MinimalContractArgs("default").OutBinding;

    private static MintContractArgs MinimalContractArgs(string contractName) => new()
    {
        ContractName = contractName,
        Pre = Provekit.Canonicalizer.Value.Boolean(true),
        ProducedBy = "csharp-contracts",
        ProducedAt = "2026-04-30T12:00:00.000Z",
        Authoring = new Authoring.KitAuthor("csharp-contracts"),
        SignerSeed = FoundationSeed,
    };

    private static MintBridgeArgs MinimalBridgeArgs(string sourceSymbol) => new()
    {
        ProducedBy = "csharp-contracts",
        ProducedAt = "2026-04-30T12:00:00.000Z",
        SourceSymbol = sourceSymbol,
        SourceLayer = "source",
        TargetContractCid = SchemaCids.Contract,
        TargetLayer = "kit",
        IrReturnSort = "Bool",
        SignerSeed = FoundationSeed,
    };

    private static MintImplicationArgs MinimalImplicationArgs(string antecedentCid) => new()
    {
        ProducedBy = "csharp-contracts",
        ProducedAt = "2026-04-30T12:00:00.000Z",
        AntecedentHash = antecedentCid,
        ConsequentHash = SchemaCids.Contract,
        AntecedentCid = antecedentCid,
        ConsequentCid = SchemaCids.Contract,
        SignerSeed = FoundationSeed,
    };

    private static readonly byte[] FoundationSeed = Enumerable.Repeat((byte)0x42, 32).ToArray();
}
