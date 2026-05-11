<?php

declare(strict_types=1);

namespace ProvekIt\LiftPhpSource;

final class PhpSourceCompiler
{
    /**
     * @param array<int, array> $ir
     */
    public function compileIrDocument(array $ir): string
    {
        $source = $this->sourceUnitBytes($ir);
        if ($source !== null) {
            return $source;
        }

        $parts = [];
        foreach ($ir as $contract) {
            if (($contract['kind'] ?? null) === 'function-contract') {
                $parts[] = $this->compileFunctionContract($contract, includeOpenTag: false);
            }
        }
        return "<?php\n" . implode("\n", array_filter($parts)) . (empty($parts) ? '' : "\n");
    }

    /**
     * @param array<int, string> $formals
     */
    public function compileBodyTerm(array $bodyTerm, string $fnName = 'f', array $formals = []): string
    {
        $contract = [
            'kind' => 'function-contract',
            'fnName' => $fnName,
            'formals' => $formals,
            'post' => ['args' => [null, $bodyTerm]],
        ];
        return $this->compileFunctionContract($contract, includeOpenTag: true);
    }

    private function compileFunctionContract(array $contract, bool $includeOpenTag): string
    {
        if (class_exists(\PhpParser\PrettyPrinter\Standard::class)) {
            return $this->compileFunctionContractWithPhpParser($contract, $includeOpenTag);
        }
        return $this->compileFunctionContractFallback($contract, $includeOpenTag);
    }

    private function compileFunctionContractWithPhpParser(array $contract, bool $includeOpenTag): string
    {
        $fn = new \PhpParser\Node\Stmt\Function_(
            $this->sourceFunctionName((string)$contract['fnName']),
            [
                'params' => array_map(
                    static fn(string $name): object => new \PhpParser\Node\Param(new \PhpParser\Node\Expr\Variable($name)),
                    $contract['formals'] ?? []
                ),
                'stmts' => $this->stmtNodes(contract_rhs($contract)),
            ]
        );
        $printer = new \PhpParser\PrettyPrinter\Standard();
        return $includeOpenTag ? $printer->prettyPrintFile([$fn]) . "\n" : $printer->prettyPrint([$fn]) . "\n";
    }

    /**
     * @return array<int, object>
     */
    private function stmtNodes(array $term): array
    {
        if (($term['kind'] ?? null) === 'ctor' && ($term['name'] ?? null) === 'php:seq') {
            return array_merge($this->stmtNodes($term['args'][0]), $this->stmtNodes($term['args'][1]));
        }
        if (($term['kind'] ?? null) === 'const' && ($term['value'] ?? null) === null) {
            return [];
        }
        return [$this->stmtNode($term)];
    }

    private function stmtNode(array $term): object
    {
        $name = $term['name'] ?? '';
        $args = $term['args'] ?? [];
        return match ($name) {
            'php:assign' => new \PhpParser\Node\Stmt\Expression(new \PhpParser\Node\Expr\Assign($this->targetNode($args[0]), $this->exprNode($args[1]))),
            'php:return' => new \PhpParser\Node\Stmt\Return_($this->isUnit($args[0]) ? null : $this->exprNode($args[0])),
            'php:if' => new \PhpParser\Node\Stmt\If_($this->exprNode($args[0]), [
                'stmts' => $this->stmtNodes($args[1]),
                'else' => $this->isUnit($args[2]) ? null : new \PhpParser\Node\Stmt\Else_($this->stmtNodes($args[2])),
            ]),
            'php:while' => new \PhpParser\Node\Stmt\While_($this->exprNode($args[0]), $this->stmtNodes($args[1])),
            'php:echo' => new \PhpParser\Node\Stmt\Echo_([$this->exprNode($args[0])]),
            'php:throw' => new \PhpParser\Node\Stmt\Throw_($this->exprNode($args[0])),
            default => new \PhpParser\Node\Stmt\Expression($this->exprNode($term)),
        };
    }

    private function exprNode(array $term): object
    {
        $kind = $term['kind'] ?? '';
        if ($kind === 'var') {
            return new \PhpParser\Node\Expr\Variable((string)$term['name']);
        }
        if ($kind === 'const') {
            $value = $term['value'] ?? null;
            return match (true) {
                is_int($value) => new \PhpParser\Node\Scalar\LNumber($value),
                is_float($value) => new \PhpParser\Node\Scalar\DNumber($value),
                is_bool($value) => new \PhpParser\Node\Expr\ConstFetch(new \PhpParser\Node\Name($value ? 'true' : 'false')),
                $value === null => new \PhpParser\Node\Expr\ConstFetch(new \PhpParser\Node\Name('null')),
                default => new \PhpParser\Node\Scalar\String_((string)$value),
            };
        }
        if ($kind !== 'ctor') {
            throw new \InvalidArgumentException('unsupported term kind: ' . $kind);
        }

        $name = $term['name'];
        $args = $term['args'] ?? [];
        return match ($name) {
            'php:add' => new \PhpParser\Node\Expr\BinaryOp\Plus($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:sub' => new \PhpParser\Node\Expr\BinaryOp\Minus($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:mul' => new \PhpParser\Node\Expr\BinaryOp\Mul($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:div' => new \PhpParser\Node\Expr\BinaryOp\Div($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:mod' => new \PhpParser\Node\Expr\BinaryOp\Mod($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:concat' => new \PhpParser\Node\Expr\BinaryOp\Concat($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:and' => new \PhpParser\Node\Expr\BinaryOp\BooleanAnd($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:or' => new \PhpParser\Node\Expr\BinaryOp\BooleanOr($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:eq' => new \PhpParser\Node\Expr\BinaryOp\Equal($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:ne' => new \PhpParser\Node\Expr\BinaryOp\NotEqual($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:identical' => new \PhpParser\Node\Expr\BinaryOp\Identical($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:not_identical' => new \PhpParser\Node\Expr\BinaryOp\NotIdentical($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:lt' => new \PhpParser\Node\Expr\BinaryOp\Smaller($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:le' => new \PhpParser\Node\Expr\BinaryOp\SmallerOrEqual($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:gt' => new \PhpParser\Node\Expr\BinaryOp\Greater($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:ge' => new \PhpParser\Node\Expr\BinaryOp\GreaterOrEqual($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:bitand' => new \PhpParser\Node\Expr\BinaryOp\BitwiseAnd($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:bitor' => new \PhpParser\Node\Expr\BinaryOp\BitwiseOr($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:bitxor' => new \PhpParser\Node\Expr\BinaryOp\BitwiseXor($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:shl' => new \PhpParser\Node\Expr\BinaryOp\ShiftLeft($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:shr' => new \PhpParser\Node\Expr\BinaryOp\ShiftRight($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:nullcoalesce' => new \PhpParser\Node\Expr\BinaryOp\Coalesce($this->exprNode($args[0]), $this->exprNode($args[1])),
            'php:not' => new \PhpParser\Node\Expr\BooleanNot($this->exprNode($args[0])),
            'php:neg' => new \PhpParser\Node\Expr\UnaryMinus($this->exprNode($args[0])),
            'php:pos' => new \PhpParser\Node\Expr\UnaryPlus($this->exprNode($args[0])),
            'php:bitnot' => new \PhpParser\Node\Expr\BitwiseNot($this->exprNode($args[0])),
            'php:index' => new \PhpParser\Node\Expr\ArrayDimFetch($this->exprNode($args[0]), $this->isUnit($args[1]) ? null : $this->exprNode($args[1])),
            'php:propfetch' => new \PhpParser\Node\Expr\PropertyFetch($this->exprNode($args[0]), $this->constString($args[1])),
            'php:call' => new \PhpParser\Node\Expr\FuncCall(new \PhpParser\Node\Name($this->constString($args[0])), array_map(fn(array $arg): object => new \PhpParser\Node\Arg($this->exprNode($arg)), array_slice($args, 1))),
            default => throw new \InvalidArgumentException('unsupported php operation in expression position: ' . $name),
        };
    }

    private function targetNode(array $term): object
    {
        $node = $this->exprNode($term);
        return $node;
    }

    private function compileFunctionContractFallback(array $contract, bool $includeOpenTag): string
    {
        $name = $this->sourceFunctionName((string)$contract['fnName']);
        $params = implode(', ', array_map(static fn(string $formal): string => '$' . $formal, $contract['formals'] ?? []));
        $lines = $this->stmtLines(contract_rhs($contract), 1);
        $body = $lines === [] ? "    return null;\n" : implode('', $lines);
        return ($includeOpenTag ? "<?php\n" : '') . "function {$name}({$params}) {\n" . $body . "}\n";
    }

    /**
     * @return array<int, string>
     */
    private function stmtLines(array $term, int $indent): array
    {
        if (($term['kind'] ?? null) === 'ctor' && ($term['name'] ?? null) === 'php:seq') {
            return array_merge($this->stmtLines($term['args'][0], $indent), $this->stmtLines($term['args'][1], $indent));
        }
        if ($this->isUnit($term)) {
            return [];
        }
        $pad = str_repeat('    ', $indent);
        $name = $term['name'] ?? '';
        $args = $term['args'] ?? [];
        return match ($name) {
            'php:assign' => [$pad . $this->exprString($args[0]) . ' = ' . $this->exprString($args[1]) . ";\n"],
            'php:return' => [$pad . 'return ' . ($this->isUnit($args[0]) ? 'null' : $this->exprString($args[0])) . ";\n"],
            'php:echo' => [$pad . 'echo ' . $this->exprString($args[0]) . ";\n"],
            'php:throw' => [$pad . 'throw ' . $this->exprString($args[0]) . ";\n"],
            default => [$pad . $this->exprString($term) . ";\n"],
        };
    }

    private function exprString(array $term): string
    {
        $kind = $term['kind'] ?? '';
        if ($kind === 'var') {
            return '$' . $term['name'];
        }
        if ($kind === 'const') {
            $value = $term['value'] ?? null;
            return match (true) {
                is_int($value), is_float($value) => (string)$value,
                is_bool($value) => $value ? 'true' : 'false',
                $value === null => 'null',
                default => '"' . addcslashes((string)$value, "\\\"\n\r\t") . '"',
            };
        }
        $name = $term['name'] ?? '';
        $args = $term['args'] ?? [];
        $binary = [
            'php:add' => '+',
            'php:sub' => '-',
            'php:mul' => '*',
            'php:div' => '/',
            'php:mod' => '%',
            'php:concat' => '.',
            'php:eq' => '==',
            'php:ne' => '!=',
            'php:identical' => '===',
            'php:not_identical' => '!==',
            'php:lt' => '<',
            'php:le' => '<=',
            'php:gt' => '>',
            'php:ge' => '>=',
            'php:and' => '&&',
            'php:or' => '||',
            'php:nullcoalesce' => '??',
        ];
        if (isset($binary[$name])) {
            return $this->exprString($args[0]) . ' ' . $binary[$name] . ' ' . $this->exprString($args[1]);
        }
        return match ($name) {
            'php:index' => $this->exprString($args[0]) . '[' . ($this->isUnit($args[1]) ? '' : $this->exprString($args[1])) . ']',
            'php:call' => $this->constString($args[0]) . '(' . implode(', ', array_map(fn(array $arg): string => $this->exprString($arg), array_slice($args, 1))) . ')',
            'php:not' => '!' . $this->exprString($args[0]),
            'php:neg' => '-' . $this->exprString($args[0]),
            default => throw new \InvalidArgumentException('unsupported php operation in fallback compiler: ' . $name),
        };
    }

    private function sourceUnitBytes(array $ir): ?string
    {
        foreach ($ir as $contract) {
            if (($contract['kind'] ?? null) !== 'function-contract') {
                continue;
            }
            $term = contract_rhs($contract);
            if (($term['kind'] ?? null) === 'ctor' && ($term['name'] ?? null) === 'php:source-unit') {
                $bytes = $term['args'][0]['value'] ?? null;
                return is_string($bytes) ? $bytes : null;
            }
        }
        return null;
    }

    private function sourceFunctionName(string $fnName): string
    {
        if (str_contains($fnName, '::')) {
            return substr($fnName, (int)strrpos($fnName, '::') + 2);
        }
        if (str_contains($fnName, '\\')) {
            return substr($fnName, (int)strrpos($fnName, '\\') + 1);
        }
        return $fnName;
    }

    private function constString(array $term): string
    {
        if (($term['kind'] ?? null) !== 'const' || !is_string($term['value'] ?? null)) {
            throw new \InvalidArgumentException('expected string constant');
        }
        return $term['value'];
    }

    private function isUnit(array $term): bool
    {
        return ($term['kind'] ?? null) === 'const' && !array_key_exists('value', $term) || (($term['kind'] ?? null) === 'const' && ($term['value'] ?? null) === null);
    }
}
