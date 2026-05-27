// SPDX-License-Identifier: Apache-2.0

namespace Provekit.IR;

internal static class QuantifierContractProbes
{
    internal static string FirstForAllName() => FirstQuantifier(
        formula => formula.Name,
        () => (QuantifierFormula)Quantifiers.ForAll(Sort.Int, value => Predicates.Eq(value, value)));

    internal static string FirstForAllKind() => FirstQuantifier(
        formula => formula.Kind,
        () => (QuantifierFormula)Quantifiers.ForAll(Sort.Int, value => Predicates.Eq(value, value)));

    internal static string FirstExistsKind() => FirstQuantifier(
        formula => formula.Kind,
        () => (QuantifierFormula)Quantifiers.Exists(Sort.Int, value => Predicates.Eq(value, value)));

    private static string FirstQuantifier(Func<QuantifierFormula, string> read, Func<QuantifierFormula> make)
    {
        Quantifiers.ResetCounter();
        return read(make());
    }
}
