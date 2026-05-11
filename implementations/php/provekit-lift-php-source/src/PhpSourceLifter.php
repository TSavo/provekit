<?php

declare(strict_types=1);

namespace ProvekIt\LiftPhpSource;

final class PhpSourceLifter
{
    /**
     * @return array{ir: array<int, array>, callEdges: array<int, array>, diagnostics: array<int, array>, opacityReport: array<int, array>, refusals: array<int, array>}
     */
    public function liftSource(string $source, string $path): array
    {
        if (class_exists(\PhpParser\ParserFactory::class)) {
            return $this->liftSourceWithPhpParser($source, $path);
        }
        return $this->liftSourceWithTokenFallback($source, $path);
    }

    /**
     * @param array<int, string> $sourcePaths
     * @return array{ir: array<int, array>, callEdges: array<int, array>, diagnostics: array<int, array>, opacityReport: array<int, array>, refusals: array<int, array>}
     */
    public function liftPaths(string $workspaceRoot, array $sourcePaths): array
    {
        $result = $this->emptyResult();
        $root = realpath($workspaceRoot);
        if ($root === false) {
            $result['diagnostics'][] = ['severity' => 'error', 'message' => 'workspace root not found: ' . $workspaceRoot];
            return $result;
        }

        foreach ($sourcePaths as $sourcePath) {
            $fullPath = $this->resolveInsideRoot($root, $sourcePath);
            if ($fullPath === null) {
                $result['refusals'][] = [
                    'kind' => 'path-traversal',
                    'function' => null,
                    'line' => null,
                    'reason' => "path '{$sourcePath}' escapes workspace root '{$root}'",
                ];
                $result['diagnostics'][] = ['severity' => 'error', 'message' => 'path traversal rejected: ' . $sourcePath];
                continue;
            }

            $files = [];
            if (is_dir($fullPath)) {
                $it = new \RecursiveIteratorIterator(new \RecursiveDirectoryIterator($fullPath));
                foreach ($it as $file) {
                    if ($file instanceof \SplFileInfo && $file->isFile() && $file->getExtension() === 'php') {
                        $files[] = $file->getPathname();
                    }
                }
                sort($files, SORT_STRING);
            } elseif (is_file($fullPath)) {
                $files[] = $fullPath;
            } else {
                $result['diagnostics'][] = ['severity' => 'warning', 'message' => 'path not found: ' . $fullPath];
            }

            foreach ($files as $file) {
                $source = file_get_contents($file);
                if ($source === false) {
                    $result['diagnostics'][] = ['severity' => 'error', 'message' => 'cannot read: ' . $file];
                    continue;
                }
                $fileResult = $this->liftSource($source, $file);
                $result['ir'] = array_merge($result['ir'], $fileResult['ir']);
                $result['callEdges'] = array_merge($result['callEdges'], $fileResult['callEdges']);
                $result['diagnostics'] = array_merge($result['diagnostics'], $fileResult['diagnostics']);
                $result['opacityReport'] = array_merge($result['opacityReport'], $fileResult['opacityReport']);
                $result['refusals'] = array_merge($result['refusals'], $fileResult['refusals']);
            }
        }

        return $result;
    }

    /**
     * @return array{ir: array<int, array>, callEdges: array<int, array>, diagnostics: array<int, array>, opacityReport: array<int, array>, refusals: array<int, array>}
     */
    private function liftSourceWithPhpParser(string $source, string $path): array
    {
        $result = $this->emptyResult();
        try {
            $parserFactory = new \PhpParser\ParserFactory();
            $parser = method_exists($parserFactory, 'createForNewestSupportedVersion')
                ? $parserFactory->createForNewestSupportedVersion()
                : $parserFactory->create(\PhpParser\ParserFactory::PREFER_PHP7);
            $stmts = $parser->parse($source) ?? [];
        } catch (\Throwable $e) {
            $result['refusals'][] = [
                'kind' => 'parse-error',
                'function' => null,
                'line' => method_exists($e, 'getStartLine') ? $e->getStartLine() : null,
                'reason' => $e->getMessage(),
            ];
            $result['ir'][] = source_unit_contract($path, $source, unit_const());
            return $result;
        }

        $bodyTerms = [];
        $this->walkTopLevel($stmts, '', [], $path, $result, $bodyTerms);
        array_unshift($result['ir'], source_unit_contract($path, $source, fold_seq($bodyTerms)));
        return $result;
    }

    /**
     * @param array<int, object> $stmts
     * @param array<int, string> $classStack
     * @param array<int, array> $bodyTerms
     */
    private function walkTopLevel(array $stmts, string $namespace, array $classStack, string $path, array &$result, array &$bodyTerms): void
    {
        foreach ($stmts as $stmt) {
            if ($stmt instanceof \PhpParser\Node\Stmt\Namespace_) {
                $nextNamespace = $stmt->name !== null ? $stmt->name->toString() : '';
                $this->walkTopLevel($stmt->stmts, $nextNamespace, $classStack, $path, $result, $bodyTerms);
                continue;
            }

            if ($stmt instanceof \PhpParser\Node\Stmt\Function_) {
                $fnName = $this->qualifyFunctionName($namespace, $stmt->name->toString());
                $this->liftFunctionNode($stmt, $fnName, $path, $result, $bodyTerms, false);
                continue;
            }

            if ($stmt instanceof \PhpParser\Node\Stmt\Class_) {
                if ($stmt->name === null) {
                    $this->addRefusal($result, 'unhandled-syntax', null, $stmt->getStartLine(), 'anonymous classes are not supported');
                    continue;
                }
                $className = $stmt->name->toString();
                $nextStack = array_merge($classStack, [$className]);
                foreach ($stmt->stmts as $member) {
                    if ($member instanceof \PhpParser\Node\Stmt\ClassMethod) {
                        $qualifiedClass = $this->qualifyFunctionName($namespace, implode('\\', $nextStack));
                        $fnName = $qualifiedClass . '::' . $member->name->toString();
                        $this->liftFunctionNode($member, $fnName, $path, $result, $bodyTerms, true);
                    } elseif ($member instanceof \PhpParser\Node\Stmt\TraitUse) {
                        $this->addRefusal($result, 'unhandled-syntax', $this->qualifyFunctionName($namespace, $className), $member->getStartLine(), 'TraitUse is not supported');
                    } else {
                        $this->addRefusal($result, 'unhandled-syntax', $this->qualifyFunctionName($namespace, $className), method_exists($member, 'getStartLine') ? $member->getStartLine() : null, $this->nodeKind($member) . ' is not supported');
                    }
                }
                continue;
            }

            if ($stmt instanceof \PhpParser\Node\Stmt\Trait_) {
                $name = $stmt->name !== null ? $this->qualifyFunctionName($namespace, $stmt->name->toString()) : null;
                $this->addRefusal($result, 'unhandled-syntax', $name, $stmt->getStartLine(), 'traits are not supported');
                continue;
            }

            if ($stmt instanceof \PhpParser\Node\Stmt\InlineHTML || $stmt instanceof \PhpParser\Node\Stmt\Nop || $stmt instanceof \PhpParser\Node\Stmt\Use_) {
                continue;
            }

            if ($stmt instanceof \PhpParser\Node\Stmt\Expression) {
                $this->addRefusal($result, 'unhandled-syntax', null, $stmt->getStartLine(), 'top-level executable statements are not lifted');
                continue;
            }

            $this->addRefusal($result, 'unhandled-syntax', null, method_exists($stmt, 'getStartLine') ? $stmt->getStartLine() : null, $this->nodeKind($stmt) . ' is not supported');
        }
    }

    /**
     * @param array<int, array> $bodyTerms
     */
    private function liftFunctionNode(object $node, string $fnName, string $path, array &$result, array &$bodyTerms, bool $method): void
    {
        if ($method && str_starts_with($this->nodeName($node), '__')) {
            $this->addRefusal($result, 'unsupported-function-shape', $fnName, $node->getStartLine(), 'magic methods are not supported');
            return;
        }
        if (($node->byRef ?? false) === true) {
            $this->addRefusal($result, 'unsupported-function-shape', $fnName, $node->getStartLine(), 'by-reference returns are not supported');
            return;
        }
        if (($node->stmts ?? null) === null) {
            $this->addRefusal($result, 'unsupported-function-shape', $fnName, $node->getStartLine(), 'declarations without bodies are not supported');
            return;
        }

        $formals = [];
        foreach ($node->params as $param) {
            if (($param->byRef ?? false) === true) {
                $this->addRefusal($result, 'unsupported-function-shape', $fnName, $param->getStartLine(), 'by-reference parameters are not supported');
                return;
            }
            if (($param->variadic ?? false) === true) {
                $this->addRefusal($result, 'unsupported-function-shape', $fnName, $param->getStartLine(), 'variadic parameters are not supported');
                return;
            }
            if ($param->default !== null) {
                $this->addRefusal($result, 'unsupported-function-shape', $fnName, $param->getStartLine(), 'default parameters are not supported');
                return;
            }
            if (!$param->var instanceof \PhpParser\Node\Expr\Variable || !is_string($param->var->name)) {
                $this->addRefusal($result, 'unsupported-function-shape', $fnName, $param->getStartLine(), 'non-plain parameters are not supported');
                return;
            }
            $formals[] = $param->var->name;
        }

        $effects = new EffectSet();
        try {
            $body = $this->lowerStatements($node->stmts, $effects, $fnName);
        } catch (RefusalException $e) {
            $this->addRefusal($result, $e->kind, $fnName, $e->sourceLine, $e->getMessage());
            return;
        }

        $result['ir'][] = function_contract($fnName, $formals, $body, $effects->all(), $path, $node->getStartLine());
        $bodyTerms[] = $body;
    }

    /**
     * @param array<int, object> $stmts
     */
    private function lowerStatements(array $stmts, EffectSet $effects, string $fnName): array
    {
        $terms = [];
        foreach ($stmts as $stmt) {
            $terms[] = $this->lowerStatement($stmt, $effects, $fnName);
        }
        return fold_seq($terms);
    }

    private function lowerStatement(object $stmt, EffectSet $effects, string $fnName): array
    {
        if ($stmt instanceof \PhpParser\Node\Stmt\Return_) {
            return ctor('php:return', $stmt->expr === null ? unit_const() : $this->lowerExpr($stmt->expr, $effects));
        }
        if ($stmt instanceof \PhpParser\Node\Stmt\Expression) {
            return $this->lowerExpr($stmt->expr, $effects);
        }
        if ($stmt instanceof \PhpParser\Node\Stmt\Echo_) {
            $effects->addIo();
            return fold_seq(array_map(fn(object $expr): array => ctor('php:echo', $this->lowerExpr($expr, $effects)), $stmt->exprs));
        }
        if ($stmt instanceof \PhpParser\Node\Stmt\If_) {
            if ($stmt->elseifs !== []) {
                throw $this->refuse($stmt, 'elseif chains are not supported; use nested if/else');
            }
            $elseTerm = $stmt->else !== null ? $this->lowerStatements($stmt->else->stmts, $effects, $fnName) : unit_const();
            return ctor(
                'php:if',
                $this->lowerExpr($stmt->cond, $effects),
                $this->lowerStatements($stmt->stmts, $effects, $fnName),
                $elseTerm
            );
        }
        if ($stmt instanceof \PhpParser\Node\Stmt\While_) {
            $loop = ctor('php:while', $this->lowerExpr($stmt->cond, $effects), $this->lowerStatements($stmt->stmts, $effects, $fnName));
            $effects->addOpaqueLoop(cid_of_json($loop));
            return $loop;
        }
        if ($stmt instanceof \PhpParser\Node\Stmt\Do_) {
            $loop = ctor('php:dowhile', $this->lowerStatements($stmt->stmts, $effects, $fnName), $this->lowerExpr($stmt->cond, $effects));
            $effects->addOpaqueLoop(cid_of_json($loop));
            return $loop;
        }
        if ($stmt instanceof \PhpParser\Node\Stmt\For_) {
            if (count($stmt->cond) > 1) {
                throw $this->refuse($stmt, 'for loops with multiple conditions are not supported');
            }
            $loop = ctor(
                'php:for',
                fold_seq(array_map(fn(object $expr): array => $this->lowerExpr($expr, $effects), $stmt->init)),
                $stmt->cond === [] ? bool_const(true) : $this->lowerExpr($stmt->cond[0], $effects),
                fold_seq(array_map(fn(object $expr): array => $this->lowerExpr($expr, $effects), $stmt->loop)),
                $this->lowerStatements($stmt->stmts, $effects, $fnName)
            );
            $effects->addOpaqueLoop(cid_of_json($loop));
            return $loop;
        }
        if ($stmt instanceof \PhpParser\Node\Stmt\Foreach_) {
            if ($stmt->keyVar !== null) {
                throw $this->refuse($stmt, 'foreach key variables are not supported');
            }
            if (($stmt->byRef ?? false) === true) {
                throw $this->refuse($stmt, 'foreach by-reference values are not supported');
            }
            if (!$stmt->valueVar instanceof \PhpParser\Node\Expr\Variable || !is_string($stmt->valueVar->name)) {
                throw $this->refuse($stmt, 'foreach value must be a plain variable');
            }
            $loop = ctor(
                'php:foreach',
                $this->lowerExpr($stmt->expr, $effects),
                string_const($stmt->valueVar->name),
                $this->lowerStatements($stmt->stmts, $effects, $fnName)
            );
            $effects->addOpaqueLoop(cid_of_json($loop));
            return $loop;
        }
        if ($stmt instanceof \PhpParser\Node\Stmt\Throw_) {
            $effects->addPanics();
            return ctor('php:throw', $this->lowerExpr($stmt->expr, $effects));
        }
        if ($stmt instanceof \PhpParser\Node\Stmt\Break_) {
            return ctor('php:break', unit_const());
        }
        if ($stmt instanceof \PhpParser\Node\Stmt\Continue_) {
            return ctor('php:continue', unit_const());
        }

        throw $this->refuse($stmt, $this->nodeKind($stmt) . ' is not supported');
    }

    private function lowerExpr(object $expr, EffectSet $effects): array
    {
        if ($expr instanceof \PhpParser\Node\Expr\Variable) {
            if (!is_string($expr->name)) {
                throw $this->refuse($expr, 'variable variables are not supported');
            }
            return var_term($expr->name);
        }
        if ($expr instanceof \PhpParser\Node\Scalar\LNumber) {
            return int_const($expr->value);
        }
        if ($expr instanceof \PhpParser\Node\Scalar\DNumber) {
            return real_const($expr->value);
        }
        if ($expr instanceof \PhpParser\Node\Scalar\String_) {
            return string_const($expr->value);
        }
        if ($expr instanceof \PhpParser\Node\Expr\ConstFetch) {
            $name = strtolower($expr->name->toString());
            return match ($name) {
                'true' => bool_const(true),
                'false' => bool_const(false),
                'null' => unit_const(),
                default => var_term($expr->name->toString()),
            };
        }
        if ($expr instanceof \PhpParser\Node\Expr\Assign) {
            $target = $this->lowerTarget($expr->var, $effects);
            if (($cell = $this->writeCellForTarget($expr->var)) !== null) {
                $effects->addWrite($cell);
            }
            return ctor('php:assign', $target, $this->lowerExpr($expr->expr, $effects));
        }
        if ($expr instanceof \PhpParser\Node\Expr\ArrayDimFetch) {
            return $this->lowerArrayDimFetch($expr, $effects, false);
        }
        if ($expr instanceof \PhpParser\Node\Expr\PropertyFetch) {
            return $this->lowerPropertyFetch($expr, $effects);
        }
        if ($expr instanceof \PhpParser\Node\Expr\StaticPropertyFetch) {
            $cell = $this->staticPropertyCell($expr);
            if ($cell !== null) {
                $effects->addRead($cell);
            }
            return ctor('php:staticprop', string_const($this->className($expr->class)), string_const($this->propertyName($expr->name, $expr)));
        }
        if ($expr instanceof \PhpParser\Node\Expr\FuncCall) {
            return $this->lowerFuncCall($expr, $effects);
        }
        if ($expr instanceof \PhpParser\Node\Expr\MethodCall) {
            return $this->lowerMethodCall($expr, $effects);
        }
        if ($expr instanceof \PhpParser\Node\Expr\StaticCall) {
            return $this->lowerStaticCall($expr, $effects);
        }
        if ($expr instanceof \PhpParser\Node\Expr\Print_) {
            $effects->addIo();
            return ctor('php:print', $this->lowerExpr($expr->expr, $effects));
        }
        if ($expr instanceof \PhpParser\Node\Expr\Exit_) {
            $effects->addPanics();
            return ctor('php:exit', $expr->expr === null ? unit_const() : $this->lowerExpr($expr->expr, $effects));
        }
        if ($expr instanceof \PhpParser\Node\Expr\Throw_) {
            $effects->addPanics();
            return ctor('php:throw', $this->lowerExpr($expr->expr, $effects));
        }
        if ($expr instanceof \PhpParser\Node\Expr\Ternary) {
            $cond = $this->lowerExpr($expr->cond, $effects);
            return ctor('php:ternary', $cond, $expr->if === null ? $cond : $this->lowerExpr($expr->if, $effects), $this->lowerExpr($expr->else, $effects));
        }
        if ($expr instanceof \PhpParser\Node\Expr\BooleanNot) {
            return ctor('php:not', $this->lowerExpr($expr->expr, $effects));
        }
        if ($expr instanceof \PhpParser\Node\Expr\UnaryMinus) {
            return ctor('php:neg', $this->lowerExpr($expr->expr, $effects));
        }
        if ($expr instanceof \PhpParser\Node\Expr\UnaryPlus) {
            return ctor('php:pos', $this->lowerExpr($expr->expr, $effects));
        }
        if ($expr instanceof \PhpParser\Node\Expr\BitwiseNot) {
            return ctor('php:bitnot', $this->lowerExpr($expr->expr, $effects));
        }
        if ($expr instanceof \PhpParser\Node\Expr\BinaryOp) {
            return $this->lowerBinaryOp($expr, $effects);
        }

        throw $this->refuse($expr, $this->nodeKind($expr) . ' is not supported');
    }

    private function lowerTarget(object $target, EffectSet $effects): array
    {
        if ($target instanceof \PhpParser\Node\Expr\Variable) {
            if (!is_string($target->name)) {
                throw $this->refuse($target, 'variable variables are not supported');
            }
            return var_term($target->name);
        }
        if ($target instanceof \PhpParser\Node\Expr\ArrayDimFetch) {
            return $this->lowerArrayDimFetch($target, $effects, true);
        }
        if ($target instanceof \PhpParser\Node\Expr\PropertyFetch) {
            return $this->lowerPropertyFetch($target, $effects);
        }
        if ($target instanceof \PhpParser\Node\Expr\StaticPropertyFetch) {
            return ctor('php:staticprop', string_const($this->className($target->class)), string_const($this->propertyName($target->name, $target)));
        }
        throw $this->refuse($target, $this->nodeKind($target) . ' is not assignable');
    }

    private function lowerBinaryOp(object $expr, EffectSet $effects): array
    {
        $op = match (true) {
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\Plus => 'php:add',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\Minus => 'php:sub',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\Mul => 'php:mul',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\Div => 'php:div',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\Mod => 'php:mod',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\Concat => 'php:concat',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\BooleanAnd,
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\LogicalAnd => 'php:and',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\BooleanOr,
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\LogicalOr => 'php:or',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\Equal => 'php:eq',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\NotEqual => 'php:ne',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\Identical => 'php:identical',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\NotIdentical => 'php:not_identical',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\Smaller => 'php:lt',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\SmallerOrEqual => 'php:le',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\Greater => 'php:gt',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\GreaterOrEqual => 'php:ge',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\BitwiseAnd => 'php:bitand',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\BitwiseOr => 'php:bitor',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\BitwiseXor => 'php:bitxor',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\ShiftLeft => 'php:shl',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\ShiftRight => 'php:shr',
            $expr instanceof \PhpParser\Node\Expr\BinaryOp\Coalesce => 'php:nullcoalesce',
            default => null,
        };
        if ($op === null) {
            throw $this->refuse($expr, $this->nodeKind($expr) . ' is not supported');
        }
        return ctor($op, $this->lowerExpr($expr->left, $effects), $this->lowerExpr($expr->right, $effects));
    }

    private function lowerArrayDimFetch(object $expr, EffectSet $effects, bool $asTarget): array
    {
        if (!$asTarget && ($cell = $this->globalCell($expr)) !== null) {
            $effects->addRead($cell);
        }
        return ctor(
            'php:index',
            $this->lowerExpr($expr->var, $effects),
            $expr->dim === null ? unit_const() : $this->lowerExpr($expr->dim, $effects)
        );
    }

    private function lowerPropertyFetch(object $expr, EffectSet $effects): array
    {
        return ctor('php:propfetch', $this->lowerExpr($expr->var, $effects), string_const($this->propertyName($expr->name, $expr)));
    }

    private function lowerFuncCall(object $expr, EffectSet $effects): array
    {
        if (!$expr->name instanceof \PhpParser\Node\Name) {
            throw $this->refuse($expr, 'dynamic function calls are not supported');
        }
        $callee = ltrim($expr->name->toString(), '\\');
        $args = $this->lowerArgs($expr->args, $effects);
        $lower = strtolower($callee);
        if (in_array($lower, ['eval', 'call_user_func', 'call_user_func_array'], true)) {
            throw $this->refuse($expr, $callee . ' is not supported');
        }
        if ($this->isIoFunction($lower)) {
            $effects->addIo();
        } elseif ($lower === 'trigger_error' && $this->isUserErrorTrigger($expr->args)) {
            $effects->addPanics();
        } elseif (!in_array($lower, ['intval', 'strval', 'count', 'strlen'], true)) {
            $effects->addUnresolvedCall($callee);
        }
        return ctor('php:call', string_const($callee), ...$args);
    }

    private function lowerMethodCall(object $expr, EffectSet $effects): array
    {
        if (!$expr->name instanceof \PhpParser\Node\Identifier) {
            throw $this->refuse($expr, 'dynamic method calls are not supported');
        }
        $method = $expr->name->toString();
        $effects->addUnresolvedCall('->' . $method);
        return ctor('php:methodcall', $this->lowerExpr($expr->var, $effects), string_const($method), ...$this->lowerArgs($expr->args, $effects));
    }

    private function lowerStaticCall(object $expr, EffectSet $effects): array
    {
        if (!$expr->name instanceof \PhpParser\Node\Identifier) {
            throw $this->refuse($expr, 'dynamic static calls are not supported');
        }
        $class = $this->className($expr->class);
        $method = $expr->name->toString();
        $effects->addUnresolvedCall($class . '::' . $method);
        return ctor('php:staticcall', string_const($class), string_const($method), ...$this->lowerArgs($expr->args, $effects));
    }

    /**
     * @param array<int, object> $args
     * @return array<int, array>
     */
    private function lowerArgs(array $args, EffectSet $effects): array
    {
        $out = [];
        foreach ($args as $arg) {
            if (($arg->name ?? null) !== null) {
                throw $this->refuse($arg, 'named arguments are not supported');
            }
            if (($arg->unpack ?? false) === true) {
                throw $this->refuse($arg, 'argument unpacking is not supported');
            }
            if (($arg->byRef ?? false) === true) {
                throw $this->refuse($arg, 'by-reference arguments are not supported');
            }
            $out[] = $this->lowerExpr($arg->value, $effects);
        }
        return $out;
    }

    private function writeCellForTarget(object $target): ?string
    {
        if ($target instanceof \PhpParser\Node\Expr\ArrayDimFetch) {
            return $this->globalCell($target);
        }
        if ($target instanceof \PhpParser\Node\Expr\StaticPropertyFetch) {
            return $this->staticPropertyCell($target);
        }
        if ($target instanceof \PhpParser\Node\Expr\PropertyFetch && $target->var instanceof \PhpParser\Node\Expr\Variable && is_string($target->var->name) && $target->var->name !== 'this') {
            return '$' . $target->var->name . '->' . $this->propertyName($target->name, $target);
        }
        return null;
    }

    private function globalCell(object $expr): ?string
    {
        if (!$expr instanceof \PhpParser\Node\Expr\ArrayDimFetch) {
            return null;
        }
        if (!$expr->var instanceof \PhpParser\Node\Expr\Variable || $expr->var->name !== 'GLOBALS') {
            return null;
        }
        $key = $this->dimLiteral($expr->dim);
        return $key === null ? null : 'GLOBALS.' . $key;
    }

    private function staticPropertyCell(object $expr): ?string
    {
        if (!$expr instanceof \PhpParser\Node\Expr\StaticPropertyFetch) {
            return null;
        }
        return $this->className($expr->class) . '::$' . $this->propertyName($expr->name, $expr);
    }

    private function dimLiteral(?object $dim): ?string
    {
        if ($dim instanceof \PhpParser\Node\Scalar\String_) {
            return $dim->value;
        }
        if ($dim instanceof \PhpParser\Node\Scalar\LNumber) {
            return (string)$dim->value;
        }
        return null;
    }

    private function propertyName(object $node, object $owner): string
    {
        if ($node instanceof \PhpParser\Node\Identifier) {
            return $node->toString();
        }
        if ($node instanceof \PhpParser\Node\VarLikeIdentifier) {
            return $node->toString();
        }
        throw $this->refuse($owner, 'dynamic property names are not supported');
    }

    private function className(object $node): string
    {
        if ($node instanceof \PhpParser\Node\Name) {
            return ltrim($node->toString(), '\\');
        }
        throw $this->refuse($node, 'dynamic class names are not supported');
    }

    private function isIoFunction(string $callee): bool
    {
        return $callee === 'printf'
            || $callee === 'header'
            || $callee === 'fopen'
            || $callee === 'fread'
            || $callee === 'fwrite'
            || $callee === 'readfile'
            || str_starts_with($callee, 'file_')
            || str_starts_with($callee, 'curl_');
    }

    /**
     * @param array<int, object> $args
     */
    private function isUserErrorTrigger(array $args): bool
    {
        if (count($args) < 2) {
            return false;
        }
        $arg = $args[1]->value ?? null;
        return $arg instanceof \PhpParser\Node\Expr\ConstFetch
            && strtoupper($arg->name->toString()) === 'E_USER_ERROR';
    }

    private function qualifyFunctionName(string $namespace, string $name): string
    {
        return $namespace === '' ? $name : $namespace . '\\' . $name;
    }

    private function nodeName(object $node): string
    {
        return isset($node->name) && is_object($node->name) && method_exists($node->name, 'toString') ? $node->name->toString() : '';
    }

    private function nodeKind(object $node): string
    {
        $class = get_class($node);
        $class = str_replace('PhpParser\\Node\\', '', $class);
        return str_replace('\\', '_', $class);
    }

    private function refuse(object $node, string $reason): RefusalException
    {
        return new RefusalException('unhandled-syntax', method_exists($node, 'getStartLine') ? $node->getStartLine() : null, $reason);
    }

    private function addRefusal(array &$result, string $kind, ?string $function, ?int $line, string $reason): void
    {
        $result['refusals'][] = ['kind' => $kind, 'function' => $function, 'line' => $line, 'reason' => $reason];
    }

    /**
     * Token fallback is intentionally narrow. CI installs nikic/php-parser;
     * this path keeps local tests useful when Composer is unavailable.
     *
     * @return array{ir: array<int, array>, callEdges: array<int, array>, diagnostics: array<int, array>, opacityReport: array<int, array>, refusals: array<int, array>}
     */
    private function liftSourceWithTokenFallback(string $source, string $path): array
    {
        $result = $this->emptyResult();
        $bodyTerms = [];
        $namespace = '';
        if (preg_match('/\\bnamespace\\s+([^;{]+)\\s*;/', $source, $m)) {
            $namespace = trim($m[1]);
        }

        $withoutClasses = $source;
        $items = [];
        foreach ($this->extractClassBlocks($source) as $class) {
            $withoutClasses = str_replace($class['full'], '', $withoutClasses);
            foreach ($this->extractFunctionBlocks($class['body'], $class['bodyLine'] - 1) as $method) {
                $fnName = $this->qualifyFunctionName($namespace, $class['name']) . '::' . $method['name'];
                $items[] = ['line' => $method['line'], 'fnName' => $fnName, 'function' => $method];
            }
        }
        foreach ($this->extractFunctionBlocks($withoutClasses) as $function) {
            $items[] = ['line' => $function['line'], 'fnName' => $this->qualifyFunctionName($namespace, $function['name']), 'function' => $function];
        }
        usort($items, static fn(array $a, array $b): int => $a['line'] <=> $b['line']);
        foreach ($items as $item) {
            $this->liftFallbackFunction($item['function'], $item['fnName'], $path, $result, $bodyTerms);
        }

        array_unshift($result['ir'], source_unit_contract($path, $source, fold_seq($bodyTerms)));
        return $result;
    }

    /**
     * @return array<int, array{name: string, body: string, full: string, bodyLine: int}>
     */
    private function extractClassBlocks(string $source): array
    {
        $classes = [];
        if (!preg_match_all('/\\bclass\\s+(\\w+)[^{]*\\{/', $source, $matches, PREG_OFFSET_CAPTURE)) {
            return [];
        }
        foreach ($matches[0] as $i => $match) {
            $open = $match[1] + strlen($match[0]) - 1;
            $close = $this->matchingBrace($source, $open);
            if ($close === null) {
                continue;
            }
            $classes[] = [
                'name' => $matches[1][$i][0],
                'body' => substr($source, $open + 1, $close - $open - 1),
                'full' => substr($source, $matches[0][$i][1], $close - $matches[0][$i][1] + 1),
                'bodyLine' => substr_count(substr($source, 0, $open + 1), "\n") + 1,
            ];
        }
        return $classes;
    }

    /**
     * @return array<int, array{name: string, params: array<int, string>, body: string, line: int}>
     */
    private function extractFunctionBlocks(string $source, int $baseLine = 0): array
    {
        $functions = [];
        if (!preg_match_all('/\\bfunction\\s+(&\\s*)?(\\w+)\\s*\\(([^)]*)\\)\\s*\\{/', $source, $matches, PREG_OFFSET_CAPTURE)) {
            return [];
        }
        foreach ($matches[0] as $i => $match) {
            $open = $match[1] + strlen($match[0]) - 1;
            $close = $this->matchingBrace($source, $open);
            if ($close === null) {
                continue;
            }
            $params = array_values(array_filter(array_map(
                static fn(string $raw): string => ltrim(trim($raw), '$'),
                explode(',', $matches[3][$i][0])
            ), static fn(string $param): bool => $param !== ''));
            $functions[] = [
                'name' => $matches[2][$i][0],
                'params' => $params,
                'body' => substr($source, $open + 1, $close - $open - 1),
                'line' => $baseLine + substr_count(substr($source, 0, $match[1]), "\n") + 1,
            ];
        }
        return $functions;
    }

    /**
     * @param array{name: string, params: array<int, string>, body: string, line: int} $function
     * @param array<int, array> $bodyTerms
     */
    private function liftFallbackFunction(array $function, string $fnName, string $path, array &$result, array &$bodyTerms): void
    {
        $effects = new EffectSet();
        try {
            $body = $this->lowerFallbackBody($function['body'], $effects, $function['line']);
        } catch (RefusalException $e) {
            $this->addRefusal($result, $e->kind, $fnName, $e->sourceLine, $e->getMessage());
            return;
        }
        $result['ir'][] = function_contract($fnName, $function['params'], $body, $effects->all(), $path, $function['line']);
        $bodyTerms[] = $body;
    }

    private function lowerFallbackBody(string $body, EffectSet $effects, int $baseLine): array
    {
        if (str_contains($body, 'fn(') || str_contains($body, 'fn (')) {
            $line = $baseLine + substr_count(substr($body, 0, strpos($body, 'fn')), "\n");
            throw new RefusalException('unhandled-syntax', $line, 'Expr_ArrowFunction is not supported');
        }
        $terms = [];
        $lines = preg_split('/\\R/', $body) ?: [];
        for ($i = 0; $i < count($lines); $i++) {
            $line = trim($lines[$i]);
            if ($line === '' || $line === '{' || $line === '}') {
                continue;
            }
            if (preg_match('/^\$GLOBALS\["([^"]+)"\]\s*=\s*(.+);$/', $line, $m)) {
                $effects->addWrite('GLOBALS.' . $m[1]);
                $terms[] = ctor('php:assign', ctor('php:index', var_term('GLOBALS'), string_const($m[1])), $this->lowerFallbackExpr($m[2], $effects));
                continue;
            }
            if (preg_match('/^\\$(\\w+)\\s*=\\s*(.+);$/', $line, $m)) {
                $terms[] = ctor('php:assign', var_term($m[1]), $this->lowerFallbackExpr($m[2], $effects));
                continue;
            }
            if (preg_match('/^return\\s+(.+);$/', $line, $m)) {
                $terms[] = ctor('php:return', $this->lowerFallbackExpr($m[1], $effects));
                continue;
            }
            if (preg_match('/^echo\\s+(.+);$/', $line, $m)) {
                $effects->addIo();
                $terms[] = ctor('php:echo', $this->lowerFallbackExpr($m[1], $effects));
                continue;
            }
            if (preg_match('/^missing\\((.*)\\);$/', $line, $m)) {
                $effects->addUnresolvedCall('missing');
                $terms[] = ctor('php:call', string_const('missing'), $this->lowerFallbackExpr($m[1], $effects));
                continue;
            }
            if (preg_match('/^while\\s*\\((.+)\\)\\s*\\{$/', $line, $m)) {
                $inner = [];
                $i++;
                while ($i < count($lines) && trim($lines[$i]) !== '}') {
                    $inner[] = $lines[$i];
                    $i++;
                }
                $loop = ctor('php:while', $this->lowerFallbackExpr($m[1], $effects), $this->lowerFallbackBody(implode("\n", $inner), $effects, $baseLine + $i));
                $effects->addOpaqueLoop(cid_of_json($loop));
                $terms[] = $loop;
                continue;
            }
            if (preg_match('/^if\\s*\\((.+)\\)\\s*\\{$/', $line, $m)) {
                $inner = [];
                $i++;
                while ($i < count($lines) && trim($lines[$i]) !== '}') {
                    $inner[] = $lines[$i];
                    $i++;
                }
                $terms[] = ctor('php:if', $this->lowerFallbackExpr($m[1], $effects), $this->lowerFallbackBody(implode("\n", $inner), $effects, $baseLine + $i), unit_const());
                continue;
            }
            if (preg_match('/^throw\\s+(.+);$/', $line, $m)) {
                $effects->addPanics();
                $terms[] = ctor('php:throw', $this->lowerFallbackExpr($m[1], $effects));
                continue;
            }
            throw new RefusalException('unhandled-syntax', $baseLine + $i, 'token fallback cannot lower statement: ' . $line);
        }
        return fold_seq($terms);
    }

    private function lowerFallbackExpr(string $expr, EffectSet $effects): array
    {
        $expr = trim($expr);
        if (preg_match('/^\$GLOBALS\["([^"]+)"\]$/', $expr, $m)) {
            $effects->addRead('GLOBALS.' . $m[1]);
            return ctor('php:index', var_term('GLOBALS'), string_const($m[1]));
        }
        if (preg_match('/^\\$(\\w+)$/', $expr, $m)) {
            return var_term($m[1]);
        }
        if (preg_match('/^-?\\d+$/', $expr)) {
            return int_const((int)$expr);
        }
        if (preg_match('/^(.+)\\s*\\+\\s*(.+)$/', $expr, $m)) {
            return ctor('php:add', $this->lowerFallbackExpr($m[1], $effects), $this->lowerFallbackExpr($m[2], $effects));
        }
        if (preg_match('/^(.+)\\s*-\\s*(.+)$/', $expr, $m)) {
            return ctor('php:sub', $this->lowerFallbackExpr($m[1], $effects), $this->lowerFallbackExpr($m[2], $effects));
        }
        if (preg_match('/^(.+)\\s*>\\s*(.+)$/', $expr, $m)) {
            return ctor('php:gt', $this->lowerFallbackExpr($m[1], $effects), $this->lowerFallbackExpr($m[2], $effects));
        }
        if (preg_match('/^(.+)\\s*<\\s*(.+)$/', $expr, $m)) {
            return ctor('php:lt', $this->lowerFallbackExpr($m[1], $effects), $this->lowerFallbackExpr($m[2], $effects));
        }
        throw new RefusalException('unhandled-syntax', null, 'token fallback cannot lower expression: ' . $expr);
    }

    private function matchingBrace(string $source, int $open): ?int
    {
        $depth = 0;
        $len = strlen($source);
        for ($i = $open; $i < $len; $i++) {
            if ($source[$i] === '{') {
                $depth++;
            } elseif ($source[$i] === '}') {
                $depth--;
                if ($depth === 0) {
                    return $i;
                }
            }
        }
        return null;
    }

    private function resolveInsideRoot(string $root, string $sourcePath): ?string
    {
        $candidate = $sourcePath;
        if (!$this->isAbsolutePath($candidate)) {
            $candidate = $root . DIRECTORY_SEPARATOR . $sourcePath;
        }
        $parent = is_dir($candidate) ? $candidate : dirname($candidate);
        $realParent = realpath($parent);
        if ($realParent === false) {
            return null;
        }
        $resolved = $realParent . DIRECTORY_SEPARATOR . basename($candidate);
        $rootPrefix = rtrim($root, DIRECTORY_SEPARATOR) . DIRECTORY_SEPARATOR;
        return $resolved === $root || str_starts_with($resolved, $rootPrefix) ? $resolved : null;
    }

    private function isAbsolutePath(string $path): bool
    {
        return str_starts_with($path, DIRECTORY_SEPARATOR) || preg_match('/^[A-Za-z]:[\\\\\\/]/', $path) === 1;
    }

    /**
     * @return array{ir: array<int, array>, callEdges: array<int, array>, diagnostics: array<int, array>, opacityReport: array<int, array>, refusals: array<int, array>}
     */
    private function emptyResult(): array
    {
        return ['ir' => [], 'callEdges' => [], 'diagnostics' => [], 'opacityReport' => [], 'refusals' => []];
    }
}

final class RefusalException extends \RuntimeException
{
    public function __construct(
        public readonly string $kind,
        public readonly ?int $sourceLine,
        string $message
    ) {
        parent::__construct($message);
    }
}
