<?php

declare(strict_types=1);

namespace ProvekIt\LiftPhpSource;

use ProvekIt\Canonicalizer\Blake3;
use ProvekIt\Canonicalizer\Jcs;

const DIALECT = 'php-source';
const VERSION = '0.1.0-draft';

function prim_sort(string $name): array
{
    return ['kind' => 'primitive', 'name' => $name];
}

function true_formula(): array
{
    return ['kind' => 'atomic', 'name' => 'true', 'args' => []];
}

function eq_formula(array $lhs, array $rhs): array
{
    return ['kind' => 'atomic', 'name' => '=', 'args' => [$lhs, $rhs]];
}

function var_term(string $name): array
{
    return ['kind' => 'var', 'name' => $name];
}

function const_term(mixed $value, string $sortName): array
{
    return ['kind' => 'const', 'value' => $value, 'sort' => prim_sort($sortName)];
}

function int_const(int $value): array
{
    return const_term($value, 'Int');
}

function real_const(float $value): array
{
    return const_term($value, 'Real');
}

function string_const(string $value): array
{
    return const_term($value, 'String');
}

function bool_const(bool $value): array
{
    return const_term($value, 'Bool');
}

function unit_const(): array
{
    return const_term(null, 'Unit');
}

function ctor(string $name, array ...$args): array
{
    if (!str_starts_with($name, 'php:')) {
        throw new \InvalidArgumentException('operation name must use php: namespace: ' . $name);
    }
    if (in_array($name, ['php:unknown', 'php:binop', 'php:skip'], true)) {
        throw new \InvalidArgumentException('forbidden PHP operation name: ' . $name);
    }
    return ['kind' => 'ctor', 'name' => $name, 'args' => $args];
}

function seq(array $first, array $second): array
{
    return ctor('php:seq', $first, $second);
}

/**
 * @param array<int, array> $statements
 */
function fold_seq(array $statements): array
{
    $statements = array_values($statements);
    if ($statements === []) {
        return unit_const();
    }
    $result = $statements[0];
    for ($i = 1; $i < count($statements); $i++) {
        $result = seq($result, $statements[$i]);
    }
    return $result;
}

function locus(string $path, int $line, int $col = 1): array
{
    return ['file' => $path, 'line' => $line, 'col' => $col];
}

function source_span(int $startLine, int $startCol, int $endLine, int $endCol): array
{
    return [
        'start_line' => $startLine,
        'start_col' => $startCol,
        'end_line' => $endLine,
        'end_col' => $endCol,
    ];
}

/**
 * @param array<int, string> $formals
 */
function body_ast_template(array $bodyTerm, array $formals): array
{
    $paramIndexes = [];
    foreach (array_values($formals) as $i => $name) {
        if (!array_key_exists($name, $paramIndexes)) {
            $paramIndexes[$name] = $i + 1;
        }
    }
    return normalize_body_template($bodyTerm, $paramIndexes);
}

/**
 * @param array<string, int> $paramIndexes
 */
function normalize_body_template(mixed $node, array $paramIndexes): mixed
{
    if (!is_array($node)) {
        return $node;
    }

    if (($node['kind'] ?? null) === 'var' && is_string($node['name'] ?? null) && array_key_exists($node['name'], $paramIndexes)) {
        return ['kind' => 'param_ref', 'index' => $paramIndexes[$node['name']]];
    }

    $out = [];
    foreach ($node as $key => $value) {
        $out[$key] = normalize_body_template($value, $paramIndexes);
    }
    return $out;
}

/**
 * @param array<int, string> $formals
 */
function body_source_payload(
    string $sourcePath,
    string $bodyText,
    array $bodyTerm,
    array $formals,
    int $startLine,
    ?int $endLine = null
): array {
    $astTemplate = body_ast_template($bodyTerm, $formals);
    $lineCount = max(1, substr_count($bodyText, "\n") + 1);
    $spanEndLine = $endLine ?? ($startLine + $lineCount);
    $bodyLines = preg_split('/\R/', $bodyText) ?: [''];
    $lastLine = $bodyLines === [] ? '' : (string)$bodyLines[count($bodyLines) - 1];

    return [
        'file' => $sourcePath,
        'span' => source_span($startLine, 1, $spanEndLine, max(1, strlen($lastLine) + 1)),
        'source_cid' => cid_of_json($bodyText),
        'body_text' => $bodyText,
        'ast_template' => $astTemplate,
        'template_cid' => cid_of_json($astTemplate),
        'param_names' => array_values($formals),
    ];
}

/**
 * @param array<int, string> $formals
 * @param array<int, array> $effects
 */
function function_contract(
    string $fnName,
    array $formals,
    array $bodyTerm,
    array $effects,
    string $sourcePath,
    int $line,
    ?array $bodySource = null
): array {
    $contract = [
        'schemaVersion' => '1',
        'kind' => 'function-contract',
        'fnName' => $fnName,
        'formals' => array_values($formals),
        'formalSorts' => array_map(static fn(): array => prim_sort('Value'), $formals),
        'returnSort' => prim_sort('Value'),
        'pre' => true_formula(),
        'post' => eq_formula(var_term('return_value'), $bodyTerm),
        'bodyCid' => null,
        'effects' => $effects,
        'locus' => locus($sourcePath, $line, 1),
        'autoMintedMementos' => [],
    ];
    if ($bodySource !== null) {
        $contract['body_source'] = $bodySource;
    }
    return $contract;
}

function source_unit_contract(string $sourcePath, string $sourceBytes, array $operationalTerm): array
{
    return [
        'schemaVersion' => '1',
        'kind' => 'function-contract',
        'fnName' => '<source-unit:' . $sourcePath . '>',
        'formals' => [],
        'formalSorts' => [],
        'returnSort' => prim_sort('Stmt'),
        'pre' => true_formula(),
        'post' => eq_formula(
            var_term('return_value'),
            ctor('php:source-unit', string_const($sourceBytes), $operationalTerm)
        ),
        'bodyCid' => null,
        'effects' => [],
        'locus' => locus($sourcePath, 1, 1),
        'autoMintedMementos' => [],
    ];
}

function canonical_json_bytes(mixed $value): string
{
    return Jcs::encode($value);
}

function cid_of_json(mixed $value): string
{
    return Blake3::cid(canonical_json_bytes($value));
}

function contract_rhs(array $contract): array
{
    return $contract['post']['args'][1];
}
