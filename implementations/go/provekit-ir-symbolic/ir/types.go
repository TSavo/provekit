package ir

import (
	"bytes"
	"encoding/json"
)

// encodeJSON marshals a value without HTML escaping (`<`, `>`, `&`),
// matching JavaScript's default `JSON.stringify` behavior. Required for
// byte-equivalence with sister kits when predicates or string values
// contain these characters (e.g. the `>` and `<` atomic predicates).
func encodeJSON(v any) ([]byte, error) {
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	if err := enc.Encode(v); err != nil {
		return nil, err
	}
	out := buf.Bytes()
	if len(out) > 0 && out[len(out)-1] == '\n' {
		out = out[:len(out)-1]
	}
	return out, nil
}

// ----------------------------------------------------------------------
// Sort
// ----------------------------------------------------------------------

type Sort interface {
	sortMarker()
}

type primitiveSort struct {
	Name string
}

func (primitiveSort) sortMarker() {}

func (s primitiveSort) MarshalJSON() ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteString(`{"kind":"primitive","name":`)
	encoded, err := encodeJSON(s.Name)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteByte('}')
	return buf.Bytes(), nil
}

type setSort struct {
	Element Sort
}

func (setSort) sortMarker() {}

func (s setSort) MarshalJSON() ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteString(`{"kind":"set","element":`)
	encoded, err := encodeJSON(s.Element)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteByte('}')
	return buf.Bytes(), nil
}

type tupleSort struct {
	Elements []Sort
}

func (tupleSort) sortMarker() {}

func (s tupleSort) MarshalJSON() ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteString(`{"kind":"tuple","elements":`)
	encoded, err := encodeJSON(s.Elements)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteByte('}')
	return buf.Bytes(), nil
}

type funcSort struct {
	Domain []Sort
	Range  Sort
}

func (funcSort) sortMarker() {}

func (s funcSort) MarshalJSON() ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteString(`{"kind":"function","domain":`)
	encoded, err := encodeJSON(s.Domain)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"range":`)
	encoded, err = encodeJSON(s.Range)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteByte('}')
	return buf.Bytes(), nil
}

var (
	Bool   Sort = primitiveSort{Name: "Bool"}
	Int    Sort = primitiveSort{Name: "Int"}
	Real   Sort = primitiveSort{Name: "Real"}
	String Sort = primitiveSort{Name: "String"}
	Ref    Sort = primitiveSort{Name: "Ref"}
	Node   Sort = primitiveSort{Name: "Node"}
	Edge   Sort = primitiveSort{Name: "Edge"}
)

func SetOf(element Sort) Sort                { return setSort{Element: element} }
func TupleOf(elements ...Sort) Sort          { return tupleSort{Elements: elements} }
func FuncOf(domain []Sort, range_ Sort) Sort { return funcSort{Domain: domain, Range: range_} }

// ----------------------------------------------------------------------
// Term — VarTerm (no sort in JSON), ConstTerm (sort kept), CtorTerm (no
// sort in JSON). The Go struct keeps the Sort field for in-process
// reasoning (e.g. Abs/Max/Min return-sort inference); it just isn't
// emitted to JSON anymore. Locked key order per the v1.1.0 IR grammar:
//
//	var:   {kind, name}
//	const: {kind, value, sort}
//	ctor:  {kind, name, args}
// ----------------------------------------------------------------------

type IrTerm interface {
	termMarker()
	TermSort() Sort
}

type varTerm struct {
	Name string
	Sort Sort
}

func (varTerm) termMarker()      {}
func (t varTerm) TermSort() Sort { return t.Sort }

func (t varTerm) MarshalJSON() ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteString(`{"kind":"var","name":`)
	encoded, err := encodeJSON(t.Name)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteByte('}')
	return buf.Bytes(), nil
}

type constTerm struct {
	Value any
	Sort  Sort
}

func (constTerm) termMarker()      {}
func (t constTerm) TermSort() Sort { return t.Sort }

func (t constTerm) MarshalJSON() ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteString(`{"kind":"const","value":`)
	encoded, err := encodeJSON(t.Value)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"sort":`)
	encoded, err = encodeJSON(t.Sort)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteByte('}')
	return buf.Bytes(), nil
}

type ctorTerm struct {
	Name string
	Args []IrTerm
	Sort Sort
}

func (ctorTerm) termMarker()      {}
func (t ctorTerm) TermSort() Sort { return t.Sort }

func (t ctorTerm) MarshalJSON() ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteString(`{"kind":"ctor","name":`)
	encoded, err := encodeJSON(t.Name)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"args":`)
	encoded, err = marshalTerms(t.Args)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteByte('}')
	return buf.Bytes(), nil
}

func marshalTerms(terms []IrTerm) ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteByte('[')
	for i, t := range terms {
		if i > 0 {
			buf.WriteByte(',')
		}
		encoded, err := encodeJSON(t)
		if err != nil {
			return nil, err
		}
		buf.Write(encoded)
	}
	buf.WriteByte(']')
	return buf.Bytes(), nil
}

// ----------------------------------------------------------------------
// Formula — three uniform shapes per the v1.1.0 IR grammar:
//
//	atomic:     {kind:"atomic", name, args}
//	connective: {kind, operands}              -- and / or / not / implies
//	quantifier: {kind, name, sort, body}       -- forall / exists, FLAT
// ----------------------------------------------------------------------

type IrFormula interface {
	formulaMarker()
}

// quantFormula is the flat quantifier shape — no Lambda wrapper. The
// kind discriminator is "forall" or "exists".
type quantFormula struct {
	Kind string
	Name string
	Sort Sort
	Body IrFormula
}

func (quantFormula) formulaMarker() {}

func (f quantFormula) MarshalJSON() ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteString(`{"kind":`)
	encoded, err := encodeJSON(f.Kind)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"name":`)
	encoded, err = encodeJSON(f.Name)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"sort":`)
	encoded, err = encodeJSON(f.Sort)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"body":`)
	encoded, err = encodeJSON(f.Body)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteByte('}')
	return buf.Bytes(), nil
}

// connectiveFormula handles all four boolean connectives uniformly.
// not has 1 operand, implies has 2, and/or have 2+.
type connectiveFormula struct {
	Kind     string
	Operands []IrFormula
}

func (connectiveFormula) formulaMarker() {}

func (f connectiveFormula) MarshalJSON() ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteString(`{"kind":`)
	encoded, err := encodeJSON(f.Kind)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"operands":`)
	encoded, err = marshalFormulas(f.Operands)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteByte('}')
	return buf.Bytes(), nil
}

// atomicFormula uses `name` (was `predicate` pre-v1.1.0).
type atomicFormula struct {
	Name string
	Args []IrTerm
}

func (atomicFormula) formulaMarker() {}

func (f atomicFormula) MarshalJSON() ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteString(`{"kind":"atomic","name":`)
	encoded, err := encodeJSON(f.Name)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"args":`)
	encoded, err = marshalTerms(f.Args)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteByte('}')
	return buf.Bytes(), nil
}

func marshalFormulas(formulas []IrFormula) ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteByte('[')
	for i, f := range formulas {
		if i > 0 {
			buf.WriteByte(',')
		}
		encoded, err := encodeJSON(f)
		if err != nil {
			return nil, err
		}
		buf.Write(encoded)
	}
	buf.WriteByte(']')
	return buf.Bytes(), nil
}
