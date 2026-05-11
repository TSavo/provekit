<?php

declare(strict_types=1);

require_once __DIR__ . '/../src/bootstrap.php';

use ProvekIt\Canonicalizer\Jcs;
use ProvekIt\LiftPhpSource\PhpSourceCompiler;
use ProvekIt\LiftPhpSource\PhpSourceLifter;
use function ProvekIt\LiftPhpSource\initialize_result;

function fail_test(string $message): never
{
    fwrite(STDERR, $message . "\n");
    exit(1);
}

function assert_true(bool $condition, string $message): void
{
    if (!$condition) {
        fail_test($message);
    }
}

function assert_same(mixed $expected, mixed $actual, string $message): void
{
    if ($expected !== $actual) {
        fail_test($message . ' expected=' . var_export($expected, true) . ' actual=' . var_export($actual, true));
    }
}

function assert_matches(string $pattern, string $actual, string $message): void
{
    if (preg_match($pattern, $actual) !== 1) {
        fail_test($message . ' pattern=' . $pattern . ' actual=' . $actual);
    }
}

function contract(array $ir, string $fnName): array
{
    foreach ($ir as $item) {
        if (($item['fnName'] ?? null) === $fnName) {
            return $item;
        }
    }
    fail_test('missing contract ' . $fnName . ' in ' . json_encode(array_column($ir, 'fnName')));
}

function rhs(array $contract): array
{
    return $contract['post']['args'][1];
}

function ctor_names(mixed $node): array
{
    $names = [];
    if (is_array($node)) {
        if (($node['kind'] ?? null) === 'ctor') {
            $names[] = $node['name'];
        }
        foreach (($node['args'] ?? []) as $child) {
            array_push($names, ...ctor_names($child));
        }
    }
    return $names;
}

function test_lift_emits_source_unit_php_ops_and_unique_names(): void
{
    $source = <<<'PHP'
<?php
namespace Acme;

function add($x, $y) {
    $z = $x + $y;
    return $z;
}

class Counter {
    public function inc($x) {
        return $x + 1;
    }
}
PHP;

    $result = (new PhpSourceLifter())->liftSource($source, 'src/math.php');

    assert_same([], $result['refusals'], 'plain function and method should lift');
    assert_same(
        ['<source-unit:src/math.php>', 'Acme\\add', 'Acme\\Counter::inc'],
        array_column($result['ir'], 'fnName'),
        'source unit and unique PHP names'
    );

    $sourceUnit = rhs($result['ir'][0]);
    assert_same('php:source-unit', $sourceUnit['name'], 'source-unit op');
    assert_same($source, $sourceUnit['args'][0]['value'], 'source-unit carries original bytes');

    $add = contract($result['ir'], 'Acme\\add');
    assert_same(['x', 'y'], $add['formals'], 'function formals');
    $names = ctor_names(rhs($add));
    assert_true(in_array('php:seq', $names, true), 'body uses php:seq');
    assert_true(in_array('php:assign', $names, true), 'body uses php:assign');
    assert_true(in_array('php:add', $names, true), 'body uses php:add');
    assert_true(in_array('php:return', $names, true), 'body uses php:return');
    foreach (ctor_names($result['ir']) as $name) {
        assert_true(str_starts_with($name, 'php:'), 'all operation CIDs use php: namespace');
        assert_true(!in_array($name, ['php:unknown', 'php:binop', 'php:skip'], true), 'forbidden catch-all op absent');
    }
}

function test_refuses_unhandled_syntax_without_unknown_ops(): void
{
    $source = <<<'PHP'
<?php
function bad($xs) {
    return array_map(fn($x) => $x + 1, $xs);
}
PHP;

    $result = (new PhpSourceLifter())->liftSource($source, 'src/bad.php');

    assert_same(1, count($result['ir']), 'refused file still emits source-unit only');
    assert_same('<source-unit:src/bad.php>', $result['ir'][0]['fnName'], 'source-unit emitted for refused file');
    assert_same(1, count($result['refusals']), 'one refusal');
    $refusal = $result['refusals'][0];
    assert_same('unhandled-syntax', $refusal['kind'], 'refusal kind');
    assert_same('bad', $refusal['function'], 'refusal function');
    assert_same(3, $refusal['line'], 'refusal line');
    assert_true(str_contains($refusal['reason'], 'ArrowFunction'), 'refusal names syntax');
    $canonical = Jcs::encode($result);
    assert_true(!str_contains($canonical, 'php:unknown'), 'no unknown op in result');
    assert_true(!str_contains($canonical, 'php:skip'), 'no skip op in result');
}

function test_effects_are_canonical_wire_shapes_and_sorted(): void
{
    $source = <<<'PHP'
<?php
function tick($x) {
    $GLOBALS["counter"] = $GLOBALS["counter"] + $x;
    echo $GLOBALS["counter"];
    while ($x > 0) {
        $x = $x - 1;
    }
    missing($GLOBALS["counter"]);
    if ($x < 0) {
        throw $x;
    }
    return $GLOBALS["counter"];
}
PHP;

    $result = (new PhpSourceLifter())->liftSource($source, 'src/effects.php');

    assert_same([], $result['refusals'], 'effects fixture should lift');
    $tick = contract($result['ir'], 'tick');
    $effects = $tick['effects'];
    assert_same(
        ['reads', 'writes', 'io', 'panics', 'unresolved_call', 'opaque_loop'],
        array_map(static fn(array $effect): string => $effect['kind'], $effects),
        'effects sorted by Rust Effect::sort_key order'
    );
    assert_same('GLOBALS.counter', $effects[0]['target'], 'read target');
    assert_same('GLOBALS.counter', $effects[1]['target'], 'write target');
    assert_same('missing', $effects[4]['name'], 'unresolved call uses name key');
    assert_matches('/^blake3-512:[0-9a-f]{128}$/', $effects[5]['loopCid'], 'opaque loop cid');
}

function test_compile_lift_roundtrip_body_term_is_byte_identical(): void
{
    $source = <<<'PHP'
<?php
function f($x) {
    $y = $x + 1;
    return $y;
}
PHP;

    $lifter = new PhpSourceLifter();
    $first = $lifter->liftSource($source, 'src/roundtrip.php');
    assert_same([], $first['refusals'], 'first lift should succeed');
    $contract = contract($first['ir'], 'f');
    $body = rhs($contract);

    $compiled = (new PhpSourceCompiler())->compileBodyTerm($body, 'f', $contract['formals']);
    $second = $lifter->liftSource($compiled, 'src/roundtrip.php');
    assert_same([], $second['refusals'], 'compiled body should relift');
    $reliftedBody = rhs(contract($second['ir'], 'f'));

    assert_same(Jcs::encode($body), Jcs::encode($reliftedBody), 'body IR round-trip canonical bytes');
}

function test_initialize_declares_php_source_draft(): void
{
    $result = initialize_result();

    assert_same('0.1.0-draft', $result['version'], 'initialize version');
    assert_same('provekit-lift/1', $result['protocol_version'], 'initialize protocol version');
    assert_same('php-source', $result['dialect'], 'initialize dialect');
    assert_same(['php-source'], $result['capabilities']['authoring_surfaces'], 'initialize surfaces');
    assert_same(false, $result['capabilities']['emits_signed_mementos'], 'initialize does not sign');
}

function test_php_language_signature_catalog_has_required_shapes(): void
{
    $specDir = dirname(__DIR__, 4) . '/menagerie/php-language-signature/specs';
    $signaturePath = $specDir . '/language_signature_php.spec.json';
    assert_true(is_file($signaturePath), 'PHP language signature exists');
    $signature = json_decode(file_get_contents($signaturePath), true, 512, JSON_THROW_ON_ERROR);
    assert_same('0.1.0-draft', $signature['version'], 'signature version');
    assert_true(in_array('op_source-unit.spec.json', $signature['operations'], true), 'source-unit operation listed');

    foreach ($signature['operations'] as $operationFile) {
        $spec = json_decode(file_get_contents($specDir . '/' . $operationFile), true, 512, JSON_THROW_ON_ERROR);
        assert_true(isset($spec['post']['arity_shape']), $operationFile . ' has arity_shape');
        assert_true(str_starts_with($spec['fn_name'], 'php:'), $operationFile . ' uses php namespace');
        assert_true(!in_array($spec['fn_name'], ['php:unknown', 'php:binop', 'php:skip'], true), $operationFile . ' avoids forbidden ops');
    }

    $assign = json_decode(file_get_contents($specDir . '/op_assign.spec.json'), true, 512, JSON_THROW_ON_ERROR);
    assert_same(['target', 'value'], array_column($assign['post']['arity_shape']['slots'], 'name'), 'assign named slots');

    $and = json_decode(file_get_contents($specDir . '/op_and.spec.json'), true, 512, JSON_THROW_ON_ERROR);
    assert_same('unevaluated', $and['post']['arity_shape']['slots'][1]['evaluation'], 'and RHS unevaluated');

    $coalesce = json_decode(file_get_contents($specDir . '/op_nullcoalesce.spec.json'), true, 512, JSON_THROW_ON_ERROR);
    assert_same('unevaluated', $coalesce['post']['arity_shape']['slots'][1]['evaluation'], 'nullcoalesce RHS unevaluated');
}

function test_compile_nested_assign_expression_roundtrip(): void
{
    // $a = $b = 1: the inner $b = 1 is php:assign in expression position (value of outer assign).
    // The compiler must handle php:assign in exprNode(), not only in stmtNode().
    $source = <<<'PHP'
<?php
function f() {
    $a = $b = 1;
    return $a + $b;
}
PHP;

    $lifter = new PhpSourceLifter();
    $first = $lifter->liftSource($source, 'src/nested_assign.php');
    assert_same([], $first['refusals'], 'nested assign should lift without refusals');
    $contract = contract($first['ir'], 'f');
    $body = rhs($contract);

    // The IR must contain php:assign in expression position (nested).
    $names = ctor_names($body);
    assert_true(in_array('php:assign', $names, true), 'body IR contains php:assign');

    // Compile back to PHP source; this must not throw.
    $compiled = (new PhpSourceCompiler())->compileBodyTerm($body, 'f', $contract['formals']);

    // Re-lift the compiled source; IR must be byte-identical (round-trip).
    $second = $lifter->liftSource($compiled, 'src/nested_assign.php');
    assert_same([], $second['refusals'], 'compiled nested-assign body should relift');
    $reliftedBody = rhs(contract($second['ir'], 'f'));
    assert_same(Jcs::encode($body), Jcs::encode($reliftedBody), 'nested-assign IR round-trip canonical bytes');

    // The compiled output must represent the nested assignment as an assign-expression.
    // PhpParser emits it as "$a = $b = 1" (chained); the fallback emits "($b = 1)".
    // Either way the compiled source must contain the nested variable assignment.
    assert_true(
        str_contains($compiled, '$b = 1') || str_contains($compiled, '($b = 1)'),
        'compiled PHP contains nested assignment expression; got: ' . $compiled
    );
}

$tests = array_filter(
    get_defined_functions()['user'],
    static fn(string $name): bool => str_starts_with($name, 'test_')
);

foreach ($tests as $test) {
    $test();
}

echo 'PHP source lifter tests passed (' . count($tests) . " tests)\n";
