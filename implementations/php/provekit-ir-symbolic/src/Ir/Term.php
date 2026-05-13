<?php
/** ProvekIt IR: Term types. Mirrors the Go `ir` package. */

namespace ProvekIt\Ir;

enum SortKind: string { case Primitive = 'primitive'; case Function = 'function'; case Dependent = 'dependent'; case Region = 'region'; }

abstract class Sort implements \JsonSerializable {
    abstract public function jsonSerialize(): array;

    public static function Primitive(string $name): self    { return new PrimitiveSort($name); }
    public static function FuncOf(array $args, Sort $ret): self { return new FunctionSort($args, $ret); }
    public static function Dependent(string $name, string $indexVar, Sort $indexSort): self { return new DependentSort($name, $indexVar, $indexSort); }
    public static function Region(string $name): self { return new RegionSort($name); }

    public static function Bool():   self { return new PrimitiveSort('Bool'); }
    public static function Int():    self { return new PrimitiveSort('Int'); }
    public static function Real():   self { return new PrimitiveSort('Real'); }
    public static function String(): self { return new PrimitiveSort('String'); }
    public static function Ref():    self { return new PrimitiveSort('Ref'); }
}

class PrimitiveSort extends Sort {
    public function __construct(public readonly string $name) {}
    public function jsonSerialize(): array { return ['kind' => 'primitive', 'name' => $this->name]; }
}

class FunctionSort extends Sort {
    public function __construct(
        /** @var Sort[] */
        public readonly array $args,
        public readonly Sort $return_,
    ) {}
    public function jsonSerialize(): array {
        return ['kind' => 'function', 'args' => array_map(fn($a) => $a->jsonSerialize(), $this->args), 'return' => $this->return_->jsonSerialize()];
    }
}

class DependentSort extends Sort {
    public function __construct(
        public readonly string $name,
        public readonly string $indexVar,
        public readonly Sort $indexSort,
    ) {}
    public function jsonSerialize(): array {
        return ['kind' => 'dependent', 'name' => $this->name, 'indexVar' => $this->indexVar, 'indexSort' => $this->indexSort->jsonSerialize()];
    }
}

class RegionSort extends Sort {
    public function __construct(
        public readonly string $name,
    ) {}
    public function jsonSerialize(): array {
        return ['kind' => 'region', 'name' => $this->name];
    }
}

abstract class IrTerm implements \JsonSerializable {
    abstract public function jsonSerialize(): array;
}

class VarTerm extends IrTerm {
    public function __construct(public readonly string $name) {}
    public function jsonSerialize(): array {
        return ['kind' => 'var', 'name' => $this->name];
    }
}

class ConstTerm extends IrTerm {
    public function __construct(
        public readonly mixed $value,
        public readonly Sort $sort,
    ) {}
    public function jsonSerialize(): array {
        return ['kind' => 'const', 'value' => $this->value, 'sort' => $this->sort];
    }
}

class CtorTerm extends IrTerm {
    public function __construct(
        public readonly string $name,
        public readonly array $args, // IrTerm[]
    ) {}
    public function jsonSerialize(): array {
        return ['kind' => 'ctor', 'name' => $this->name, 'args' => array_map(fn($a) => $a->jsonSerialize(), $this->args)];
    }
}
