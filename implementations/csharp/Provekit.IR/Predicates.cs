// SPDX-License-Identifier: Apache-2.0
//
// Atomic predicate constructors + connective constructors. The atomic
// predicate names are protocol-locked: ASCII "=", ">", "<" and the
// UTF-8 sequences for ≥, ≤, ≠ (U+2265, U+2264, U+2260). Cross-language
// hash agreement requires these exact byte sequences: see the
// canonicalizer's unicode round-trip tests.

namespace Provekit.IR;

public static class Predicates
{
    public static Formula Atomic(string name, params Term[] args) =>
        new AtomicFormula(name, args);

    public static Formula Eq(Term a, Term b) => Atomic("=", a, b);
    public static Formula Ne(Term a, Term b) => Atomic("≠", a, b); // ≠
    public static Formula Gt(Term a, Term b) => Atomic(">", a, b);
    public static Formula Gte(Term a, Term b) => Atomic("≥", a, b); // ≥
    public static Formula Lt(Term a, Term b) => Atomic("<", a, b);
    public static Formula Lte(Term a, Term b) => Atomic("≤", a, b); // ≤

    // Connectives: unified `operands` array. `not` takes one operand;
    // `implies` takes two (antecedent, consequent); `and`/`or` take any.
    private static Formula Conn(string kind, params Formula[] ops) =>
        new ConnectiveFormula(kind, ops);

    public static Formula Not(Formula a) => Conn("not", a);
    public static Formula Implies(Formula antecedent, Formula consequent) =>
        Conn("implies", antecedent, consequent);
    public static Formula And(params Formula[] operands) => Conn("and", operands);
    public static Formula Or(params Formula[] operands) => Conn("or", operands);
}
