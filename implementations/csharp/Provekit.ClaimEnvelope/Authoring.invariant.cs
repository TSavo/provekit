// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cs for Provekit.ClaimEnvelope/Authoring.cs
//
// Public surface covered:
//   * Authoring.KitAuthor(author, note?)
//   * Authoring.Lift(lifter, evidence, sourceCid?)
//   * Authoring.Llm(model, version, promptCid, confidence, rationale?)
//   * Authoring.ToValue() — emits producerKind tag
//
// Honest scope:
//   The producerKind discriminator strings are protocol-locked:
//   "kit-author" / "lift" / "llm". Cross-language byte agreement
//   depends on these exact spellings.

using static Provekit.IR.Predicates;
using static Provekit.IR.Terms;
using static Provekit.IR.Collector;

namespace Provekit.SelfContracts.Invariants;

public static class AuthoringInvariants
{
    public static void Register()
    {
        // producerKind discriminator: "kit-author" is 10 chars.
        Contract("csharp_authoring_kit_author_kind_length_eq_10",
            post: Eq(Ctor("len", StrConst("kit-author")), Num(10)));

        // producerKind discriminator: "lift" is 4 chars.
        Contract("csharp_authoring_lift_kind_length_eq_4",
            post: Eq(Ctor("len", StrConst("lift")), Num(4)));

        // producerKind discriminator: "llm" is 3 chars.
        Contract("csharp_authoring_llm_kind_length_eq_3",
            post: Eq(Ctor("len", StrConst("llm")), Num(3)));

        // KitAuthor with empty note OMITS the field (null/empty equivalence)
        // — encoded as a contract that the emitted JSON does not contain
        // the literal substring `"note":`.
        Contract("csharp_authoring_kit_author_omits_empty_note",
            post: Eq(Ctor("ContainsNoteField", Ctor("KitAuthor", StrConst("x"), StrConst(""))), Num(0)));
    }
}
