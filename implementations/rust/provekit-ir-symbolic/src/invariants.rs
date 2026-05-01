// SPDX-License-Identifier: Apache-2.0
//
// invariants.rs - Formal Invariants from IR Spec
//
// This module documents all formal invariants from protocol/specs/2026-04-30-ir-formal-grammar.md.
// Each invariant is expressed in FOL with English explanation.
// This serves as the machine-readable spec for the Rust kit.
//
// ============================================================================
// INVARIANTS INDEX (27 total)
// ============================================================================
//
// ## VarTerm (Terms Section)
//
// INVARIANT VarTerm.NoSortField:
//   ∀t: VarTerm → ¬HasKey(t, "sort")
//   Every VarTerm MUST NOT contain a `sort` field.
//
// INVARIANT VarTerm.SortFromQuantifier:
//   ∀v: VarTerm, q: QuantifierFormula → Sort(v) = q.sort
//   A variable inherits its sort from the enclosing quantifier.
//
// INVARIANT VarTerm.SortFromSubstitution:
//   ∀v: VarTerm → Sort(v) = Sort(substituting expression)
//   A free variable derives its sort from substitution.
//
// ## ConstTerm (Terms Section)
//
// INVARIANT ConstTerm.HasSort:
//   ∀t: ConstTerm → HasKey(t, "sort") ∧ IsSort(t.sort)
//   Every ConstTerm MUST have a `sort` field.
//
// ## CtorTerm (Terms Section)
//
// INVARIANT CtorTerm.NoSortField:
//   ∀t: CtorTerm → ¬HasKey(t, "sort")
//   A CtorTerm MUST NOT contain a `sort` field.
//
// ## QuantifierFormula (Formulas Section)
//
// INVARIANT QuantifierFormula.HasSort:
//   ∀q: QuantifierFormula → HasKey(q, "sort") ∧ IsSort(q.sort)
//
// INVARIANT QuantifierFormula.HasBody:
//   ∀q: QuantifierFormula → HasKey(q, "body") ∧ IsIrFormula(q.body)
//
// ## ConnectiveFormula (Formulas Section)
//
// INVARIANT ConnectiveFormula.NotArity:
//   ∀c: ConnectiveFormula (c.kind = "not") → Len(c.operands) = 1
//
// INVARIANT ConnectiveFormula.ImpliesArity:
//   ∀c: ConnectiveFormula (c.kind = "implies") → Len(c.operands) = 2
//
// INVARIANT ConnectiveFormula.AndOrArity:
//   ∀c: ConnectiveFormula (c.kind = "and" ∨ c.kind = "or") → Len(c.operands) ≥ 2
//
// ## AtomicFormula (Formulas Section)
//
// INVARIANT AtomicFormula.HasName:
//   ∀a: AtomicFormula → HasKey(a, "name") ∧ IsString(a.name)
//
// INVARIANT AtomicFormula.HasArgs:
//   ∀a: AtomicFormula → HasKey(a, "args") ∧ IsArray(a.args)
//
// INVARIANT AtomicFormula.KnownPredicate:
//   ∀a: AtomicFormula → IsBuiltInPredicate(a.name) ∨ IsKitDefinedPredicate(a.name)
//
// ## ContractDeclaration (Declarations Section)
//
// INVARIANT ContractDeclaration.HasOutBinding:
//   ∀c: ContractDeclaration → HasKey(c, "outBinding") ∧ c.outBinding ≠ ""
//
// INVARIANT ContractDeclaration.HasAtLeastOneFormula:
//   ∀c: ContractDeclaration → (pre ∨ post ∨ inv) is present
//
// INVARIANT ContractDeclaration.ValidFreeVariables:
//   Free variables must be outBinding or function parameter.
//
// ## BridgeDeclaration (Declarations Section)
//
// INVARIANT BridgeDeclaration.RequiredFields:
//   All required fields must be present.
//
// INVARIANT BridgeDeclaration.ValidCidFormat:
//   targetContractCid must be valid CID format.
//
// ## PrimitiveSort, BitvecSort, SetSort, TupleSort, FunctionSort
// ## Strict Mode, Round-trip Property, Test Plan
// (see protocol/specs/2026-04-30-ir-formal-grammar.md)
//
// ============================================================================
// IMPLEMENTATION VERIFICATION
// ============================================================================

#[cfg(test)]
mod tests {
    use crate::{atomic_, and_, forall, make_var, num, not_, implies, Int, Sort, Term};

    /// INVARIANT VarTerm.NoSortField: VarTerm has no sort field.
    #[test]
    fn varterm_has_no_sort_field() {
        let v = make_var("x");
        match &*v {
            Term::Var { name } => {
                assert_eq!(name, "x");
            }
            _ => panic!("Expected Var term"),
        }
    }

    /// INVARIANT ConstTerm.HasSort: ConstTerm has sort field.
    #[test]
    fn constterm_has_sort_field() {
        let n = num(42);
        match &*n {
            Term::Const { value, sort } => {
                assert!(matches!(value, crate::ConstValue::Int(42)));
                assert_eq!(sort, &Sort::int());
            }
            _ => panic!("Expected Const term"),
        }
    }

    /// INVARIANT CtorTerm.NoSortField: CtorTerm has no sort field.
    #[test]
    fn ctorterm_has_no_sort_field() {
        let ctor = crate::parse_int(make_var("s"));
        match &*ctor {
            Term::Ctor { name, args } => {
                assert_eq!(name, "parseInt");
                assert_eq!(args.len(), 1);
            }
            _ => panic!("Expected Ctor term"),
        }
    }

    /// INVARIANT QuantifierFormula.HasSort: forall has sort field.
    #[test]
    fn forall_has_sort() {
        let f = forall(Int(), |v| atomic_(">", vec![v.clone(), num(0)]));
        match &*f {
            crate::Formula::Quantifier { sort, .. } => {
                assert_eq!(sort, &Int());
            }
            _ => panic!("Expected Quantifier"),
        }
    }

    /// INVARIANT ConnectiveFormula.NotArity: not has 1 operand.
    #[test]
    fn not_has_one_operand() {
        let f = not_(atomic_("=", vec![num(1), num(1)]));
        match &*f {
            crate::Formula::Connective { kind, operands } => {
                assert_eq!(kind, "not");
                assert_eq!(operands.len(), 1);
            }
            _ => panic!("Expected Connective"),
        }
    }

    /// INVARIANT ConnectiveFormula.ImpliesArity: implies has 2 operands.
    #[test]
    fn implies_has_two_operands() {
        let f = implies(
            atomic_("=", vec![num(1), num(1)]),
            atomic_(">", vec![num(0), num(0)]),
        );
        match &*f {
            crate::Formula::Connective { kind, operands } => {
                assert_eq!(kind, "implies");
                assert_eq!(operands.len(), 2);
            }
            _ => panic!("Expected Connective"),
        }
    }

    /// INVARIANT ConnectiveFormula.AndOrArity: and has >= 2 operands.
    #[test]
    fn and_has_two_operands() {
        let f = and_(vec![
            atomic_("=", vec![num(1), num(1)]),
            atomic_(">", vec![num(0), num(0)]),
        ]);
        match &*f {
            crate::Formula::Connective { kind, operands } => {
                assert_eq!(kind, "and");
                assert!(operands.len() >= 2);
            }
            _ => panic!("Expected Connective"),
        }
    }

    /// INVARIANT AtomicFormula.HasName: atomic has name field.
    #[test]
    fn atomic_has_name() {
        let f = atomic_("=", vec![num(1), num(1)]);
        match &*f {
            crate::Formula::Atomic { name, args } => {
                assert_eq!(name, "=");
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected Atomic"),
        }
    }

    /// INVARIANT PrimitiveSort.ValidName: Sort::int() returns valid sort.
    #[test]
    fn primitive_sort_valid() {
        let s = Sort::int();
        assert_eq!(s.name, "Int");
    }
}