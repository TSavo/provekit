<?php
/** ProvekIt IR — Term types. Mirrors the Go `ir` package. */

namespace ProvekIt\Ir;

enum SortKind: string { case Primitive = 'primitive'; }

class Sort implements \JsonSerializable {
    public function __construct(
        public readonly string $name,
        public readonly SortKind $kind = SortKind::Primitive,
    ) {}

    public function jsonSerialize(): array {
        return ['kind' => $this->kind->value, 'name' => $this->name];
    }

    // Well-known sorts
    public static function Bool():   self { return new self('Bool'); }
    public static function Int():    self { return new self('Int'); }
    public static function Real():   self { return new self('Real'); }
    public static function String(): self { return new self('String'); }
    public static function Ref():    self { return new self('Ref'); }
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
