// SPDX-License-Identifier: Apache-2.0

namespace Provekit.IR;

internal static class QuantifiersContracts
{
    internal static int csharp_quantifiers_first_var_name_is_x0()
    {
        if (QuantifierContractProbes.FirstForAllName() != "_x0") throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_quantifiers_first_var_name_length_eq_3()
    {
        if (QuantifierContractProbes.FirstForAllName().Length != 3) throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_quantifiers_forall_kind()
    {
        if (QuantifierContractProbes.FirstForAllKind() != "forall") throw new InvalidOperationException("contract");
        return 1;
    }

    internal static int csharp_quantifiers_exists_kind()
    {
        if (QuantifierContractProbes.FirstExistsKind() != "exists") throw new InvalidOperationException("contract");
        return 1;
    }
}
