// SPDX-License-Identifier: Apache-2.0
//
// IR-JSON v1.1.0 Formula. Three node kinds:
//   - AtomicFormula: { kind: "atomic", name, args }
//   - ConnectiveFormula: { kind: "and"|"or"|"not"|"implies", operands }
//   - QuantifierFormula: { kind: "forall"|"exists", name, sort, body }
//
// Five formula kinds total when you flatten the four connectives. This
// IS the "maximal-uniformity" v1.1.0 IR — every node has `kind`, every
// applicable node has `name`, the operands array unifies the four
// boolean connectives. Mirrors Rust/C++ peers exactly.

namespace Provekit.IR;

public abstract record Formula;

public sealed record AtomicFormula(string Name, IReadOnlyList<Term> Args) : Formula;

public sealed record ConnectiveFormula(string Kind, IReadOnlyList<Formula> Operands) : Formula;

public sealed record QuantifierFormula(string Kind, string Name, Sort Sort, Formula Body) : Formula;
