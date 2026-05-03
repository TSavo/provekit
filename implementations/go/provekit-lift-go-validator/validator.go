// Package validator lifts go-playground/validator struct tags to canonical IR.
//
// Maps struct field validate tags to ProvekIt ContractDeclarations with
// byte-for-byte identical IR to sister kits for equivalent constraints.
//
// Example:
//
//	type User struct {
//	    Name  string `validate:"required,min=1,max=100"`
//	    Age   int    `validate:"gte=0,lte=150"`
//	    Email string `validate:"email"`
//	}
//
//	decls := validator.LiftStruct(User{})
//	// -> 3 ContractDeclarations, one per field
package validator

import (
	"fmt"
	"reflect"
	"strconv"
	"strings"

	ir "github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"
)

// LiftStruct walks a struct value's fields, parses validate tags, and
// returns one ContractDeclaration per field that has recognizable constraints.
// The contract name is "<TypeName>.<FieldName>".
func LiftStruct(v interface{}) []ir.Declaration {
	t := reflect.TypeOf(v)
	if t.Kind() == reflect.Ptr {
		t = t.Elem()
	}
	if t.Kind() != reflect.Struct {
		return nil
	}

	var decls []ir.Declaration
	typeName := t.Name()

	for i := 0; i < t.NumField(); i++ {
		field := t.Field(i)
		tag := field.Tag.Get("validate")
		if tag == "" {
			continue
		}
		f := liftField(field, tag)
		if f != nil {
			decls = append(decls, ir.ContractDeclaration{
				Name:       fmt.Sprintf("%s.%s", typeName, field.Name),
				OutBinding: ir.DefaultOutBinding,
				Pre:        f,
			})
		}
	}
	return decls
}

// liftField parses a validate tag and returns the conjunctive IR formula
// for all constraints on the field, or nil if nothing was recognized.
func liftField(field reflect.StructField, tag string) ir.IrFormula {
	var constraints []ir.IrFormula
	fieldName := field.Name
	sort := goSort(field.Type)
	v := ir.MakeVar(fieldName, sort)

	parts := strings.Split(tag, ",")
	for _, part := range parts {
		part = strings.TrimSpace(part)
		if part == "" {
			continue
		}

		f := liftTag(v, sort, part)
		if f != nil {
			constraints = append(constraints, f)
		}
	}

	switch len(constraints) {
	case 0:
		return nil
	case 1:
		return constraints[0]
	default:
		return ir.And(constraints...)
	}
}

// liftTag maps a single validate tag fragment to an IR formula.
func liftTag(v ir.IrTerm, sort ir.Sort, tag string) ir.IrFormula {
	// required
	if tag == "required" {
		return requiredConstraint(v, sort)
	}

	// gte=N, lte=N, gt=N, lt=N, eq=N, ne=N — direct numeric comparisons
	for _, op := range []string{"gte=", "lte=", "gt=", "lt=", "eq=", "ne="} {
		if !strings.HasPrefix(tag, op) {
			continue
		}
		numStr := tag[len(op):]
		n, err := strconv.ParseInt(numStr, 10, 64)
		if err != nil {
			return nil
		}
		rhs := ir.Num(n)
		switch op[:len(op)-1] {
		case "gte":
			return ir.Gte(v, rhs)
		case "lte":
			return ir.Lte(v, rhs)
		case "gt":
			return ir.Gt(v, rhs)
		case "lt":
			return ir.Lt(v, rhs)
		case "eq":
			return ir.Eq(v, rhs)
		case "ne":
			return ir.Neq(v, rhs)
		}
	}

	// min=N, max=N — context-sensitive: numeric bounds or string length
	if strings.HasPrefix(tag, "min=") {
		numStr := tag[4:]
		n, err := strconv.ParseInt(numStr, 10, 64)
		if err != nil {
			return nil
		}
		if isNumericSort(sort) {
			return ir.Gte(v, ir.Num(n))
		}
		return ir.Gte(ir.StringLength(v), ir.Num(n))
	}
	if strings.HasPrefix(tag, "max=") {
		numStr := tag[4:]
		n, err := strconv.ParseInt(numStr, 10, 64)
		if err != nil {
			return nil
		}
		if isNumericSort(sort) {
			return ir.Lte(v, ir.Num(n))
		}
		return ir.Lte(ir.StringLength(v), ir.Num(n))
	}

	// len=N — exact string length
	if strings.HasPrefix(tag, "len=") {
		numStr := tag[4:]
		n, err := strconv.ParseInt(numStr, 10, 64)
		if err != nil {
			return nil
		}
		return ir.Eq(ir.StringLength(v), ir.Num(n))
	}

	// Vacuous-true runtime validators: tags whose semantics live in the
	// runtime validator (go-playground/validator) but have no provable
	// content in IR theory. Each emits a distinct kit-predicate Atomic
	// (`kit:<tag>`) so consumers can content-address the position via
	// the OpacityManifest. See VacuousTrueTags / opacity_manifest.go.
	if pred, ok := vacuousKitPredicate(tag); ok {
		return ir.Atomic(pred, v)
	}

	// oneof=A B C
	if strings.HasPrefix(tag, "oneof=") {
		values := strings.Fields(tag[6:])
		var eqs []ir.IrFormula
		for _, val := range values {
			eqs = append(eqs, ir.Eq(v, ir.StrConst(val)))
		}
		return ir.Or(eqs...)
	}

	return nil
}

// requiredConstraint emits the canonical non-null/non-zero constraint.
//
// For strings: neq(var, "") since Go zero-value for string is "".
// For numerics: neq(var, 0) since Go zero-value is 0.
func requiredConstraint(v ir.IrTerm, sort ir.Sort) ir.IrFormula {
	if sort == ir.String {
		return ir.Neq(v, ir.StrConst(""))
	}
	return ir.Neq(v, ir.Num(0))
}

// goSort maps a reflect.Type to a ProvekIt Sort.
func goSort(t reflect.Type) ir.Sort {
	switch t.Kind() {
	case reflect.String:
		return ir.String
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64,
		reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64,
		reflect.Float32, reflect.Float64:
		return ir.Int
	case reflect.Bool:
		return ir.Bool
	case reflect.Ptr, reflect.Interface, reflect.Map, reflect.Slice:
		return ir.Ref
	default:
		return ir.Ref
	}
}

func isNumericSort(sort ir.Sort) bool {
	return sort == ir.Int || sort == ir.Real
}
