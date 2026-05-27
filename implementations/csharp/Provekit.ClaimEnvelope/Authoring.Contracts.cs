// SPDX-License-Identifier: Apache-2.0

namespace Provekit.ClaimEnvelope;

internal static class AuthoringContracts
{
    internal static int csharp_authoring_kit_author_kind_length_eq_10()
    {
        if ("kit-author".Length != 10) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_authoring_lift_kind_length_eq_4()
    {
        if ("lift".Length != 4) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_authoring_llm_kind_length_eq_3()
    {
        if ("llm".Length != 3) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_authoring_kit_author_omits_empty_note(string author)
    {
        if (ContainsNoteField(author, "") != 0) throw new InvalidOperationException("contract");
        return 1;
    }

    private static int ContainsNoteField(string author, string note) =>
        new Authoring.KitAuthor(author, note).ToValue().AsObject().Any(entry => entry.Key == "note") ? 1 : 0;
}
