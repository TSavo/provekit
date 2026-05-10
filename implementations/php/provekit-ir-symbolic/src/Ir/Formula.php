<?php
/** ProvekIt IR: Formula types. */

namespace ProvekIt\Ir;

abstract class IrFormula implements \JsonSerializable {
    abstract public function jsonSerialize(): array;
}

class AtomicFormula extends IrFormula {
    public function __construct(
        public readonly string $name,
        public readonly array $args, // IrTerm[]
    ) {}
    public function jsonSerialize(): array {
        return ['kind' => 'atomic', 'name' => $this->name, 'args' => array_map(fn($a) => $a->jsonSerialize(), $this->args)];
    }
}

class ConnectiveFormula extends IrFormula {
    public function __construct(
        public readonly string $kind, // "and" | "or" | "not" | "implies"
        public readonly array $operands, // IrFormula[]
    ) {}
    public function jsonSerialize(): array {
        return ['kind' => $this->kind, 'operands' => array_map(fn($o) => $o->jsonSerialize(), $this->operands)];
    }
}

class QuantifierFormula extends IrFormula {
    public function __construct(
        public readonly string $kind, // "forall" | "exists"
        public readonly string $name,
        public readonly Sort $sort,
        public readonly IrFormula $body,
    ) {}
    public function jsonSerialize(): array {
        return ['kind' => $this->kind, 'name' => $this->name, 'sort' => $this->sort, 'body' => $this->body];
    }
}

// ---------- Builder helpers (kit primitives) ----------

function V(string $name, ?Sort $sort = null): IrTerm {
    return new VarTerm($name);
}

function Num(int $value): IrTerm {
    return new ConstTerm($value, Sort::Int());
}

function Str(string $value): IrTerm {
    return new ConstTerm($value, Sort::String());
}

function Ctor(string $name, IrTerm ...$args): IrTerm {
    return new CtorTerm($name, $args);
}

function Ctor1(string $name, IrTerm $arg): IrTerm {
    return new CtorTerm($name, [$arg]);
}

function Ctor2(string $name, IrTerm $a, IrTerm $b): IrTerm {
    return new CtorTerm($name, [$a, $b]);
}

function Eq(IrTerm $a, IrTerm $b): IrFormula {
    return new AtomicFormula('=', [$a, $b]);
}

function Neq(IrTerm $a, IrTerm $b): IrFormula {
    return new AtomicFormula("\u{2260}", [$a, $b]);
}

function Gt(IrTerm $a, IrTerm $b): IrFormula {
    return new AtomicFormula('>', [$a, $b]);
}

function Gte(IrTerm $a, IrTerm $b): IrFormula {
    return new AtomicFormula("\u{2265}", [$a, $b]);
}

function Lt(IrTerm $a, IrTerm $b): IrFormula {
    return new AtomicFormula('<', [$a, $b]);
}

function Lte(IrTerm $a, IrTerm $b): IrFormula {
    return new AtomicFormula("\u{2264}", [$a, $b]);
}

function NotNull(IrTerm $a): IrFormula {
    return new AtomicFormula('not_null', [$a]);
}

function And_(IrFormula ...$operands): IrFormula {
    $ops = array_values($operands);
    if (count($ops) === 1) return $ops[0];
    return new ConnectiveFormula('and', $ops);
}

function Or_(IrFormula ...$operands): IrFormula {
    $ops = array_values($operands);
    if (count($ops) === 1) return $ops[0];
    return new ConnectiveFormula('or', $ops);
}

function Implies(IrFormula $ante, IrFormula $cons): IrFormula {
    return new ConnectiveFormula('implies', [$ante, $cons]);
}

function ForAll(string $name, Sort $sort, IrFormula $body): IrFormula {
    return new QuantifierFormula('forall', $name, $sort, $body);
}

function ForAllRef(string $name, IrFormula $body): IrFormula {
    return ForAll($name, Sort::Ref(), $body);
}

function TrueAtom(): IrFormula {
    return new AtomicFormula('true', []);
}

function StringLength(IrTerm $t): IrTerm {
    return Ctor1('strlen', $t);
}

function LenGte(IrTerm $t, int $n): IrFormula {
    return Gte(StringLength($t), Num($n));
}

function LenLte(IrTerm $t, int $n): IrFormula {
    return Lte(StringLength($t), Num($n));
}

function StdSort(IrTerm $t, Sort $sort): IrFormula {
    return Eq(Ctor1('kind_of', $t), new ConstTerm($sort->name, Sort::String()));
}
