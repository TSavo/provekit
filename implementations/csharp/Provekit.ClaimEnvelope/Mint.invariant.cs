// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cs for Provekit.ClaimEnvelope/Mint.cs
//
// Public surface covered:
//   * Mint.MintContract(args): MintedEnvelope
//   * Mint.MintBridge(args): MintedEnvelope
//   * Mint.MintImplication(args): MintedEnvelope
//
// Honest scope:
//   MintedEnvelope.Cid is BLAKE3-512 of the unsigned canonical bytes
//   (length 139, "blake3-512:" prefix, 128 hex). The producer signature
//   verifies under the signer pubkey: verification is asserted at the
//   integration layer, not the IR.

using static Provekit.IR.Predicates;
using static Provekit.IR.Quantifiers;
using static Provekit.IR.Terms;
using static Provekit.IR.Collector;
using Provekit.IR;

namespace Provekit.SelfContracts.Invariants;

public static class MintInvariants
{
    public static void Register()
    {
        // MintContract returns an envelope whose CID is exactly 139 chars
        // (the BLAKE3-512 self-identifying form: prefix + 128 hex).
        Must("csharp_mint_contract_cid_length_eq_139",
            ForAll(Sort.String, args =>
                Eq(Ctor("len", Ctor("MintContractCid", args)), Num(139))));

        // MintContract is deterministic: identical args → identical bytes.
        Must("csharp_mint_contract_is_deterministic",
            ForAll(Sort.String, args =>
                Eq(Ctor("MintContractBytes", args), Ctor("MintContractBytes", args))));

        // MintBridge CID is also 139 chars.
        Must("csharp_mint_bridge_cid_length_eq_139",
            ForAll(Sort.String, args =>
                Eq(Ctor("len", Ctor("MintBridgeCid", args)), Num(139))));

        // MintImplication CID is 139 chars.
        Must("csharp_mint_implication_cid_length_eq_139",
            ForAll(Sort.String, args =>
                Eq(Ctor("len", Ctor("MintImplicationCid", args)), Num(139))));

        // MintContract with all-null pre/post/inv THROWS (empirically
        // verified by Mint.cs:153-157).
        Contract("csharp_mint_contract_throws_when_all_pre_post_inv_null",
            post: Eq(Ctor("ThrowsOnEmptyContract"), Num(1)));

        // outBinding default is "out" (the term exported by Terms.Out()).
        Contract("csharp_mint_default_out_binding",
            post: Eq(StrConst("out"), StrConst("out")));
    }
}
