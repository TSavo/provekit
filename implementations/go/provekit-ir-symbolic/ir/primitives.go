package ir

func Num(value int64) IrTerm {
	return constTerm{Value: value, Sort: Int}
}

func RealConst(value float64) IrTerm {
	return constTerm{Value: value, Sort: Real}
}

func StrConst(value string) IrTerm {
	return constTerm{Value: value, Sort: String}
}

func BoolConst(value bool) IrTerm {
	return constTerm{Value: value, Sort: Bool}
}

func ctor(name string, args []IrTerm, sort Sort) IrTerm {
	return ctorTerm{Name: name, Args: args, Sort: sort}
}

func ParseInt(s IrTerm) IrTerm {
	ensureKitBridgesRegistered()
	return ctor("parseInt", []IrTerm{s}, Int)
}
func ParseFloat(s IrTerm) IrTerm {
	ensureKitBridgesRegistered()
	return ctor("parseFloat", []IrTerm{s}, Real)
}

func IsNaN(n IrTerm) IrTerm {
	ensureKitBridgesRegistered()
	return ctor("isNaN", []IrTerm{n}, Bool)
}
func IsFinite(n IrTerm) IrTerm {
	ensureKitBridgesRegistered()
	return ctor("isFinite", []IrTerm{n}, Bool)
}
func IsInteger(n IrTerm) IrTerm {
	ensureKitBridgesRegistered()
	return ctor("isInteger", []IrTerm{n}, Bool)
}

func Abs(n IrTerm) IrTerm {
	s := n.TermSort()
	if s == nil {
		s = Real
	}
	return ctor("Math.abs", []IrTerm{n}, s)
}

func Max(a, b IrTerm) IrTerm {
	s := a.TermSort()
	if s == nil {
		s = Real
	}
	return ctor("Math.max", []IrTerm{a, b}, s)
}

func Min(a, b IrTerm) IrTerm {
	s := a.TermSort()
	if s == nil {
		s = Real
	}
	return ctor("Math.min", []IrTerm{a, b}, s)
}

func Floor(n IrTerm) IrTerm {
	ensureKitBridgesRegistered()
	return ctor("Math.floor", []IrTerm{n}, Int)
}
func Ceil(n IrTerm) IrTerm {
	ensureKitBridgesRegistered()
	return ctor("Math.ceil", []IrTerm{n}, Int)
}
func Sqrt(n IrTerm) IrTerm {
	ensureKitBridgesRegistered()
	return ctor("Math.sqrt", []IrTerm{n}, Real)
}
func Sign(n IrTerm) IrTerm {
	ensureKitBridgesRegistered()
	return ctor("Math.sign", []IrTerm{n}, Int)
}

func StringLength(s IrTerm) IrTerm {
	ensureKitBridgesRegistered()
	return ctor("String.prototype.length", []IrTerm{s}, Int)
}
func StringIncludes(s, sub IrTerm) IrTerm {
	ensureKitBridgesRegistered()
	return ctor("String.prototype.includes", []IrTerm{s, sub}, Bool)
}
func ArrayLength(arr IrTerm) IrTerm {
	ensureKitBridgesRegistered()
	return ctor("Array.prototype.length", []IrTerm{arr}, Int)
}
func ArrayIncludes(arr, item IrTerm) IrTerm {
	ensureKitBridgesRegistered()
	return ctor("Array.prototype.includes", []IrTerm{arr, item}, Bool)
}

func Add(a, b IrTerm) IrTerm { return ctor("+", []IrTerm{a, b}, Int) }
func Sub(a, b IrTerm) IrTerm { return ctor("-", []IrTerm{a, b}, Int) }
func Mul(a, b IrTerm) IrTerm { return ctor("*", []IrTerm{a, b}, Int) }
func Div(a, b IrTerm) IrTerm { return ctor("/", []IrTerm{a, b}, Real) }
func Neg(a IrTerm) IrTerm    { return ctor("-", []IrTerm{a}, Int) }

func atom(name string, args []IrTerm) IrFormula {
	return atomicFormula{Name: name, Args: args}
}

// Atomic is the exported constructor for an atomic predicate
// application. Lift adapters use it to emit kit-predicate placeholders
// (e.g. `kit:email`) for runtime validators that have no IR theory.
// Compilers that lack semantics for these names list them in the
// OpacityManifest with reasonCode = "kit_predicate_no_semantics".
func Atomic(name string, args ...IrTerm) IrFormula {
	return atom(name, args)
}

func Eq(a, b IrTerm) IrFormula  { return atom("=", []IrTerm{a, b}) }
func Neq(a, b IrTerm) IrFormula { return atom("≠", []IrTerm{a, b}) }
func Lt(a, b IrTerm) IrFormula  { return atom("<", []IrTerm{a, b}) }
func Lte(a, b IrTerm) IrFormula { return atom("≤", []IrTerm{a, b}) }
func Gt(a, b IrTerm) IrFormula  { return atom(">", []IrTerm{a, b}) }
func Gte(a, b IrTerm) IrFormula { return atom("≥", []IrTerm{a, b}) }

func IsTrue(b IrTerm) IrFormula  { return atom("true", []IrTerm{b}) }
func IsFalse(b IrTerm) IrFormula { return atom("false", []IrTerm{b}) }

// Lambda creates a first-class function term: λx: τ. body
func Lambda(paramName string, paramSort Sort, body IrTerm) IrTerm {
	return lambdaTerm{ParamName: paramName, ParamSort: paramSort, Body: body, Sort: body.TermSort()}
}

// LetBinding creates a name-term pair for let expressions
func LetBinding(name string, boundTerm IrTerm) letBinding {
	return letBinding{Name: name, BoundTerm: boundTerm}
}

// Let creates a let term: let bindings in body
func Let(bindings []letBinding, body IrTerm) IrTerm {
	return letTerm{Bindings: bindings, Body: body, Sort: body.TermSort()}
}

// Choice creates a definite description formula: εx. P(x)
func Choice(varName string, sort Sort, body func(x IrTerm) IrFormula) IrFormula {
	x := varTerm{Name: varName, Sort: sort}
	return choiceFormula{VarName: varName, Sort: sort, Body: body(x)}
}
