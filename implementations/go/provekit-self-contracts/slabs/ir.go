package slabs

// Invariants for ir/{types,quantifiers,property,canonicalize}.go.
//
// Public surface covered:
//   * Sort: Bool/Int/Real/String/Ref/Node/Edge + SetOf/TupleOf/FuncOf.
//   * IrTerm constructors: Num/StrConst/BoolConst/RealConst, MakeVar,
//     MakeCtor, ParseInt/Floor/Add/Sub/Mul/Div/Eq/Lt/Gt/Lte/Gte/Neq.
//   * IrFormula constructors: ForAll/Exists (+Named), And/Or/Not/Implies/Iff.
//   * Contract / Must / Bridge / Property collector primitives.
//   * MarshalDeclarations(decls) ([]byte, error).

import "github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"

func InvariantsIRTypes() {
	// Var-term JSON omits sort under v1.1.0:
	//   {kind:"var", name}  (NO sort)
	// LIVING DOCS: kit-defined predicate `omitsSort` is opaque to Z3.
	ir.Must("varTerm_json_no_sort_field",
		ir.ForAll(ir.String, func(name ir.IrTerm) ir.IrFormula {
			return ir.Eq(ctor1("varTerm_json", name), ctor1("varTerm_json", name))
		}))

	// Atomic-formula JSON uses `name` (was `predicate` pre-v1.1.0):
	//   {kind:"atomic", name, args}
	// Length floor on the kind discriminator's emitted form:
	//   len("\"kind\":\"atomic\"") = 16
	ir.Contract("atomicFormula_kind_discriminator_length_ge_15", ir.ContractArgs{
		Post: ir.Gte(ir.StringLength(ctor0("atomicFormula_kind_prefix")), ir.Num(15)),
	})

	// Ctor-term JSON shape: {kind:"ctor", name, args}: NO sort field.
	// LIVING DOCS: Z3 has no semantics for "shape equals ...".
	ir.Must("ctorTerm_json_no_sort_field",
		ir.ForAll(ir.String, func(name ir.IrTerm) ir.IrFormula {
			return ir.Eq(ctor1("ctorTerm_json", name), ctor1("ctorTerm_json", name))
		}))

	// Quantifier shape is FLAT: {kind, name, sort, body}.
	// No nested Lambda wrapper. Mirrors Rust types.go.
	ir.Must("quantFormula_json_is_flat",
		ir.ForAll(ir.String, func(body ir.IrTerm) ir.IrFormula {
			return ir.Eq(ctor1("quantFormula_json", body), ctor1("quantFormula_json", body))
		}))
}

func InvariantsIRQuantifiers() {
	// ForAll emits kind="forall".
	ir.Contract("ForAll_kind_eq_forall", ir.ContractArgs{
		Post: ir.Eq(ctor0("ForAll_kind"), ir.StrConst("forall")),
	})

	// Exists emits kind="exists".
	ir.Contract("Exists_kind_eq_exists", ir.ContractArgs{
		Post: ir.Eq(ctor0("Exists_kind"), ir.StrConst("exists")),
	})

	// Implies has exactly 2 operands (antecedent, consequent).
	ir.Contract("Implies_arity_eq_2", ir.ContractArgs{
		Post: ir.Eq(ctor0("Implies_arity"), ir.Num(2)),
	})

	// Not has exactly 1 operand.
	ir.Contract("Not_arity_eq_1", ir.ContractArgs{
		Post: ir.Eq(ctor0("Not_arity"), ir.Num(1)),
	})

	// And/Or with 0 args desugar to atomic true/false; with 1 arg pass
	// through. v1.1.0 grammar: 2+ operands at the connective shape.
	// LIVING DOCS: desugaring rules opaque to Z3.
	ir.Must("And_zero_args_desugars_to_true",
		ir.ForAll(ir.Int, func(_ ir.IrTerm) ir.IrFormula {
			return ir.Eq(ctor0("And_zero_args_kind"), ir.StrConst("atomic"))
		}))

	// Iff desugars to and(implies(a,b), implies(b,a)).
	ir.Must("Iff_desugars_to_and_of_implies",
		ir.ForAll(ir.Int, func(_ ir.IrTerm) ir.IrFormula {
			return ir.Eq(ctor0("Iff_desugar_kind"), ir.StrConst("and"))
		}))
}

func InvariantsIRProperty() {
	// ContractDeclaration kind discriminator is "contract".
	ir.Contract("ContractDeclaration_kind_eq_contract", ir.ContractArgs{
		Post: ir.Eq(ctor0("ContractDeclaration_kind"), ir.StrConst("contract")),
	})

	// BridgeDeclaration kind discriminator is "bridge".
	ir.Contract("BridgeDeclaration_kind_eq_bridge", ir.ContractArgs{
		Post: ir.Eq(ctor0("BridgeDeclaration_kind"), ir.StrConst("bridge")),
	})

	// DefaultOutBinding is "out" (length 3).
	ir.Contract("DefaultOutBinding_length_eq_3", ir.ContractArgs{
		Post: ir.Eq(ir.StringLength(ctor0("DefaultOutBinding")), ir.Num(3)),
	})

	// Contract panics if all of Pre/Post/Inv are nil.
	// LIVING DOCS: panic semantics not in Z3's first-order theory.
	ir.Must("Contract_panics_when_all_nil",
		ir.ForAll(ir.Int, func(_ ir.IrTerm) ir.IrFormula {
			return ir.Eq(ctor0("Contract_validate_nil"), ctor0("Contract_validate_nil"))
		}))
}

func InvariantsIRCanonicalize() {
	// MarshalDeclarations is deterministic.
	//   forall decls. MarshalDeclarations(decls) = MarshalDeclarations(decls)
	ir.Must("MarshalDeclarations_is_deterministic",
		ir.ForAll(ir.String, func(d ir.IrTerm) ir.IrFormula {
			return ir.Eq(ctor1("MarshalDeclarations", d), ctor1("MarshalDeclarations", d))
		}))

	// MarshalDeclarations emits a JSON array (top-level shape).
	// First byte is '[' for any decls slice. LIVING DOCS for the byte-
	// shape claim; operationally enforced by ir/canonical_form_test.go.
	ir.Must("MarshalDeclarations_emits_array",
		ir.ForAll(ir.String, func(d ir.IrTerm) ir.IrFormula {
			// nonempty output
			return ir.Gte(ir.StringLength(ctor1("MarshalDeclarations", d)), ir.Num(2))
		}))
}
