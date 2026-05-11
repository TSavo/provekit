<?php

declare(strict_types=1);

$root = __DIR__;
$specDir = $root . '/specs';
if (!is_dir($specDir)) {
    mkdir($specDir, 0777, true);
}

$ops = [
    op('source-unit', ['bytes', 'operational_term'], ['String', 'Stmt'], 'Stmt', named([
        ['name' => 'bytes', 'evaluation' => 'unevaluated', 'slot_sort' => 'literal'],
        ['name' => 'operational_term'],
    ]), 'lossless PHP source wrapper; source bytes are recoverable and operational_term is the lifted program'),
    op('seq', ['first', 'second'], ['Stmt', 'Stmt'], 'Stmt', positional(2), 'execute first, then second'),
    op('assign', ['target', 'value'], ['Value', 'Value'], 'Stmt', named([['name' => 'target'], ['name' => 'value']]), 'store value in target'),
    op('if', ['cond', 'then_branch', 'else_branch'], ['Value', 'Stmt', 'Stmt'], 'Stmt', named([['name' => 'cond'], ['name' => 'then_branch'], ['name' => 'else_branch']]), 'conditional branch'),
    op('while', ['cond', 'body'], ['Value', 'Stmt'], 'Stmt', named([['name' => 'cond'], ['name' => 'body']]), 'while loop'),
    op('dowhile', ['body', 'cond'], ['Stmt', 'Value'], 'Stmt', named([['name' => 'body'], ['name' => 'cond']]), 'do-while loop'),
    op('for', ['init', 'cond', 'step', 'body'], ['Stmt', 'Value', 'Stmt', 'Stmt'], 'Stmt', named([['name' => 'init'], ['name' => 'cond'], ['name' => 'step'], ['name' => 'body']]), 'for loop'),
    op('foreach', ['iterable', 'value_name', 'body'], ['Value', 'String', 'Stmt'], 'Stmt', named([['name' => 'iterable'], ['name' => 'value_name', 'slot_sort' => 'identifier'], ['name' => 'body']]), 'foreach loop over value variable'),
    op('return', ['value'], ['Value'], 'Stmt', named([['name' => 'value']]), 'return value'),
    op('break', ['unit'], ['Unit'], 'Stmt', named([['name' => 'unit']]), 'break out of loop'),
    op('continue', ['unit'], ['Unit'], 'Stmt', named([['name' => 'unit']]), 'continue loop'),
    op('throw', ['value'], ['Value'], 'Stmt', named([['name' => 'value']]), 'throw exception or value'),
    op('echo', ['value'], ['Value'], 'Stmt', named([['name' => 'value']]), 'echo value to output'),
    op('print', ['value'], ['Value'], 'Value', named([['name' => 'value']]), 'print value to output'),
    op('exit', ['value'], ['Value'], 'Stmt', named([['name' => 'value']]), 'terminate execution'),
    op('call', ['callee', 'args'], ['String', 'Value'], 'Value', named([['name' => 'callee'], ['name' => 'args', 'shape' => ['kind' => 'set']]]), 'function call'),
    op('methodcall', ['receiver', 'method', 'args'], ['Value', 'String', 'Value'], 'Value', named([['name' => 'receiver'], ['name' => 'method', 'slot_sort' => 'identifier'], ['name' => 'args', 'shape' => ['kind' => 'set']]]), 'instance method call'),
    op('staticcall', ['class', 'method', 'args'], ['String', 'String', 'Value'], 'Value', named([['name' => 'class'], ['name' => 'method', 'slot_sort' => 'identifier'], ['name' => 'args', 'shape' => ['kind' => 'set']]]), 'static method call'),
    op('index', ['base', 'index'], ['Value', 'Value'], 'Value', named([['name' => 'base'], ['name' => 'index']]), 'array offset fetch'),
    op('propfetch', ['receiver', 'property'], ['Value', 'String'], 'Value', named([['name' => 'receiver'], ['name' => 'property', 'slot_sort' => 'identifier']]), 'object property fetch'),
    op('staticprop', ['class', 'property'], ['String', 'String'], 'Value', named([['name' => 'class'], ['name' => 'property', 'slot_sort' => 'identifier']]), 'static property fetch'),
    op('ternary', ['cond', 'then_expr', 'else_expr'], ['Value', 'Value', 'Value'], 'Value', named([['name' => 'cond'], ['name' => 'then_expr', 'evaluation' => 'unevaluated'], ['name' => 'else_expr', 'evaluation' => 'unevaluated']]), 'PHP ternary expression'),
    op('nullcoalesce', ['lhs', 'rhs'], ['Value', 'Value'], 'Value', named([['name' => 'lhs'], ['name' => 'rhs', 'evaluation' => 'unevaluated']]), 'short-circuit null coalescing'),
    op('and', ['lhs', 'rhs'], ['Value', 'Value'], 'Value', named([['name' => 'lhs'], ['name' => 'rhs', 'evaluation' => 'unevaluated']]), 'short-circuit logical conjunction'),
    op('or', ['lhs', 'rhs'], ['Value', 'Value'], 'Value', named([['name' => 'lhs'], ['name' => 'rhs', 'evaluation' => 'unevaluated']]), 'short-circuit logical disjunction'),
    op('not', ['operand'], ['Value'], 'Value', named([['name' => 'operand']]), 'logical negation'),
    op('neg', ['operand'], ['Value'], 'Value', named([['name' => 'operand']]), 'numeric negation'),
    op('pos', ['operand'], ['Value'], 'Value', named([['name' => 'operand']]), 'unary plus'),
    op('bitnot', ['operand'], ['Value'], 'Value', named([['name' => 'operand']]), 'bitwise not'),
];

foreach ([
    'add' => '+', 'sub' => '-', 'mul' => '*', 'div' => '/', 'mod' => '%', 'concat' => '.',
    'eq' => '==', 'ne' => '!=', 'identical' => '===', 'not_identical' => '!==',
    'lt' => '<', 'le' => '<=', 'gt' => '>', 'ge' => '>=',
    'bitand' => '&', 'bitor' => '|', 'bitxor' => '^', 'shl' => '<<', 'shr' => '>>',
] as $name => $symbol) {
    $ops[] = op($name, ['lhs', 'rhs'], ['Value', 'Value'], 'Value', named([['name' => 'lhs'], ['name' => 'rhs']]), "PHP {$symbol} operator");
}

usort($ops, static fn(array $a, array $b): int => $a['file'] <=> $b['file']);
foreach ($ops as $entry) {
    write_json($specDir . '/' . $entry['file'], $entry['spec']);
}

$sortFiles = [];
foreach (['Value', 'Bool', 'Int', 'Real', 'String', 'Unit', 'Stmt'] as $sort) {
    $file = 'sort_' . strtolower($sort) . '.spec.json';
    $sortFiles[] = $file;
    write_json($specDir . '/' . $file, [
        'kind' => 'sort',
        'fn_name' => 'php:sort:' . strtolower($sort),
        'version' => '0.1.0-draft',
        'name' => $sort,
        'locus' => 'menagerie/php-language-signature/README.md#sorts',
    ]);
}

$effects = [
    effect('read', 'Read', ['target'], named([['name' => 'target']]), 'record read from target state cell'),
    effect('write', 'Write', ['target'], named([['name' => 'target']]), 'record write to target state cell'),
    effect('io', 'Io', [], positional(0), 'record IO or network interaction'),
    effect('panic', 'Panic', [], positional(0), 'record throw, die, exit, or fatal trigger_error behavior'),
    effect('unresolved_call', 'Call', ['name'], named([['name' => 'name']]), 'record a call whose callee contract is not available'),
    effect('opaque_loop', 'Loop', ['loopCid'], named([['name' => 'loopCid']]), 'record an opaque loop keyed by its lifted sub-IR CID'),
];
foreach ($effects as $entry) {
    write_json($specDir . '/' . $entry['file'], $entry['spec']);
    write_json($specDir . '/effsig_' . $entry['name'] . '.spec.json', [
        'kind' => 'effect-signature',
        'fn_name' => 'php:effect-signature:' . $entry['name'],
        'version' => '0.1.0-draft',
        'name' => $entry['signature'],
        'locus' => 'menagerie/php-language-signature/README.md#effects',
    ]);
}

write_json($specDir . '/language_signature_php.spec.json', [
    'kind' => 'language_signature',
    'fn_name' => 'php',
    'version' => '0.1.0-draft',
    'sorts' => $sortFiles,
    'operations' => array_map(static fn(array $entry): string => $entry['file'], $ops),
    'equations' => [],
    'effects' => array_map(static fn(array $entry): string => $entry['file'], $effects),
    'effect_signatures' => array_map(static fn(array $entry): string => 'effsig_' . $entry['name'] . '.spec.json', $effects),
    'locus' => 'menagerie/php-language-signature/README.md',
]);

function op(string $name, array $formals, array $sorts, string $returnSort, array $arityShape, string $wp): array
{
    return [
        'file' => 'op_' . $name . '.spec.json',
        'spec' => algorithm('php:' . $name, $formals, $sorts, $returnSort, $name, $arityShape, $wp),
    ];
}

function effect(string $name, string $signature, array $formals, array $arityShape, string $wp): array
{
    $sorts = array_fill(0, count($formals), 'String');
    return [
        'name' => $name,
        'signature' => $signature,
        'file' => 'eff_' . $name . '.spec.json',
        'spec' => algorithm('php:effect:' . str_replace('_', '-', $name), $formals, $sorts, 'Unit', $name . '-effect', $arityShape, $wp, [['kind' => 'effect-signature', 'name' => $signature]]),
    ];
}

function algorithm(string $fnName, array $formals, array $sorts, string $returnSort, string $operator, array $arityShape, string $wp, array $effects = []): array
{
    return [
        'kind' => 'algorithm',
        'fn_name' => $fnName,
        'version' => '0.1.0-draft',
        'formals' => $formals,
        'formal_sorts' => array_map('sort_ctor', $sorts),
        'return_sort' => sort_ctor($returnSort),
        'pre' => ['kind' => 'atomic', 'name' => 'true', 'args' => []],
        'post' => [
            'kind' => 'operation-contract',
            'operator' => $operator,
            'arity' => $sorts,
            'result' => $returnSort,
            'wp' => $wp,
            'arity_shape' => $arityShape,
        ],
        'effects' => ['effects' => $effects],
        'locus' => 'menagerie/php-language-signature/README.md#operations',
    ];
}

function sort_ctor(string $name): array
{
    return ['kind' => 'ctor', 'name' => $name, 'args' => []];
}

function named(array $slots): array
{
    return ['kind' => 'named', 'slots' => $slots];
}

function positional(int $arity): array
{
    return ['kind' => 'positional', 'arity' => $arity];
}

function write_json(string $path, array $value): void
{
    file_put_contents($path, json_encode($value, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE) . "\n");
}
