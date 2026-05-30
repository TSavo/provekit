<?php

declare(strict_types=1);

require_once __DIR__ . '/../src/bootstrap.php';

use ProvekIt\Canonicalizer\Jcs;
use ProvekIt\LiftPhpSource\PhpSourceCompiler;
use ProvekIt\LiftPhpSource\PhpSourceLifter;
use ProvekIt\LiftPhpSource\PhpSourceRecognizer;
use function ProvekIt\LiftPhpSource\dispatch_rpc;
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

function assert_has_key(string $key, array $actual, string $message): void
{
    if (!array_key_exists($key, $actual)) {
        fail_test($message . ' missing key=' . $key . ' actual=' . var_export($actual, true));
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

function temp_dir(string $name): string
{
    $base = sys_get_temp_dir() . DIRECTORY_SEPARATOR . 'provekit-php-' . $name . '-' . bin2hex(random_bytes(4));
    if (!mkdir($base, 0777, true) && !is_dir($base)) {
        fail_test('failed to create temp dir ' . $base);
    }
    return $base;
}

function remove_tree(string $path): void
{
    if (!is_dir($path)) {
        return;
    }
    $it = new RecursiveIteratorIterator(
        new RecursiveDirectoryIterator($path, FilesystemIterator::SKIP_DOTS),
        RecursiveIteratorIterator::CHILD_FIRST
    );
    foreach ($it as $file) {
        $file->isDir() ? rmdir($file->getPathname()) : unlink($file->getPathname());
    }
    rmdir($path);
}

function binding_from_contract(array $contract, array $overrides = []): array
{
    $bodySource = $contract['body_source'] ?? null;
    assert_true(is_array($bodySource), 'contract carries body_source');
    return array_merge([
        'concept_name' => 'concept:php-arithmetic',
        'library_tag' => 'provekit-php-arithmetic',
        'family' => 'concept:family:arithmetic',
        'ast_template' => $bodySource['ast_template'],
        'template_cid' => $bodySource['template_cid'],
        'param_names' => $bodySource['param_names'],
        'contract_cid' => 'blake3-512:' . str_repeat('c', 128),
    ], $overrides);
}

function write_project_recognize_templates(string $root, array $bindings): void
{
    $dir = $root . '/.provekit/lift/php-source';
    if (!mkdir($dir, 0777, true) && !is_dir($dir)) {
        fail_test('failed to create project template dir ' . $dir);
    }
    $members = array_map(static fn(array $binding): array => [
        'kind' => 'library-sugar-binding-entry',
        'concept_name' => $binding['concept_name'],
        'target_library_tag' => $binding['library_tag'],
        'family' => $binding['family'],
        'body_source' => [
            'ast_template' => $binding['ast_template'],
            'template_cid' => $binding['template_cid'],
            'param_names' => $binding['param_names'],
        ],
        'contract_cid' => $binding['contract_cid'],
    ], $bindings);
    file_put_contents(
        $dir . '/recognize-templates.json',
        json_encode(['members' => $members], JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE)
    );
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

function test_lift_function_entries_emit_body_source_text_ast_template_and_cid(): void
{
    $source = <<<'PHP'
<?php
function add($x, $y) {
    $z = $x + $y;
    return $z;
}
PHP;

    $result = (new PhpSourceLifter())->liftSource($source, 'src/add.php');

    assert_same([], $result['refusals'], 'add fixture should lift');
    $add = contract($result['ir'], 'add');
    assert_has_key('body_source', $add, 'function entry carries body_source');
    $bodySource = $add['body_source'];
    assert_same('src/add.php', $bodySource['file'], 'body_source file');
    assert_true(str_contains($bodySource['body_text'], '$z = $x + $y;'), 'body_text carries assignment source');
    assert_true(str_contains($bodySource['body_text'], 'return $z;'), 'body_text carries return source');
    assert_true(!str_contains($bodySource['body_text'], 'function add'), 'body_text is body-only source');
    assert_same(['x', 'y'], $bodySource['param_names'], 'body_source param names');
    assert_has_key('ast_template', $bodySource, 'body_source carries ast_template');
    assert_matches('/^blake3-512:[0-9a-f]{128}$/', $bodySource['template_cid'], 'body_source template cid');
    $canonical = Jcs::encode($bodySource['ast_template']);
    assert_true(str_contains($canonical, '"kind":"param_ref"'), 'ast_template normalizes formal params');
    assert_true(str_contains($canonical, '"index":1'), 'ast_template contains first param marker');
    assert_true(str_contains($canonical, '"index":2'), 'ast_template contains second param marker');
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
    assert_same('pep/1.7.0', $result['protocol_version'], 'initialize protocol version');
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

function test_recognizer_emits_exact_tag_for_alpha_equivalent_sugar_body(): void
{
    $sugar = <<<'PHP'
<?php
function sugar_add($x) {
    return $x + 1;
}
PHP;
    $sugarResult = (new PhpSourceLifter())->liftSource($sugar, 'src/sugar.php');
    assert_same([], $sugarResult['refusals'], 'sugar fixture should lift');
    $binding = binding_from_contract(contract($sugarResult['ir'], 'sugar_add'));

    $root = temp_dir('recognize-alpha');
    try {
        mkdir($root . '/src');
        file_put_contents($root . '/src/app.php', <<<'PHP'
<?php
function user_add($input) {
    return $input + 1;
}
PHP);

        $response = (new PhpSourceRecognizer())->recognizePaths($root, ['src/app.php'], [$binding]);
        $tags = $response['tags'];
        assert_same(1, count($tags), 'alpha-equivalent body should produce one tag');
        $tag = $tags[0];
        assert_same('src/app.php', $tag['file'], 'tag file');
        assert_same('user_add', $tag['function_name'], 'tag function name');
        assert_same('concept:php-arithmetic', $tag['concept_name'], 'tag concept');
        assert_same('provekit-php-arithmetic', $tag['library_tag'], 'tag library');
        assert_same('concept:family:arithmetic', $tag['family'], 'tag family');
        assert_same($binding['template_cid'], $tag['template_cid'], 'tag template cid');
        assert_same($binding['contract_cid'], $tag['contract_cid'], 'tag contract cid');
        assert_same('exact', $tag['match_tier'], 'tag match tier');
        assert_same([['index' => 1, 'source_text' => 'input']], $tag['param_bindings'], 'tag binds user param spelling');
        assert_same(2, $tag['span']['start_line'], 'tag span start line');
        assert_true($tag['span']['end_line'] >= 4, 'tag span end line');
    } finally {
        remove_tree($root);
    }
}

function test_recognizer_returns_empty_tags_for_different_body(): void
{
    $sugar = <<<'PHP'
<?php
function sugar_add($x) {
    return $x + 1;
}
PHP;
    $sugarResult = (new PhpSourceLifter())->liftSource($sugar, 'src/sugar.php');
    assert_same([], $sugarResult['refusals'], 'sugar fixture should lift');
    $binding = binding_from_contract(contract($sugarResult['ir'], 'sugar_add'));

    $root = temp_dir('recognize-negative');
    try {
        mkdir($root . '/src');
        file_put_contents($root . '/src/app.php', <<<'PHP'
<?php
function user_sub($x) {
    return $x - 1;
}
PHP);

        $response = (new PhpSourceRecognizer())->recognizePaths($root, ['src/app.php'], [$binding]);
        assert_same([], $response['tags'], 'different body should not match');
    } finally {
        remove_tree($root);
    }
}

function test_recognizer_rpc_uses_project_owned_templates_without_forwarded_bindings(): void
{
    $sugar = <<<'PHP'
<?php
function sugar_add($x) {
    return $x + 1;
}
PHP;
    $sugarResult = (new PhpSourceLifter())->liftSource($sugar, 'src/sugar.php');
    assert_same([], $sugarResult['refusals'], 'sugar fixture should lift');
    $binding = binding_from_contract(contract($sugarResult['ir'], 'sugar_add'));

    $root = temp_dir('recognize-rpc');
    try {
        write_project_recognize_templates($root, [$binding]);
        mkdir($root . '/src');
        file_put_contents($root . '/src/app.php', <<<'PHP'
<?php
function user_add($x) {
    return $x + 1;
}
PHP);

        $response = dispatch_rpc([
            'jsonrpc' => '2.0',
            'id' => 99,
            'method' => 'provekit.plugin.recognize',
            'params' => [
                'project_root' => $root,
                'source_paths' => ['src/app.php'],
            ],
        ]);
        assert_same(99, $response['id'], 'rpc id preserved');
        assert_has_key('result', $response, 'recognize rpc success response');
        assert_same(1, count($response['result']['tags']), 'recognize rpc emits tag');
    } finally {
        remove_tree($root);
    }
}

$tests = array_filter(
    get_defined_functions()['user'],
    static fn(string $name): bool => str_starts_with($name, 'test_')
);

foreach ($tests as $test) {
    $test();
}

echo 'PHP source lifter tests passed (' . count($tests) . " tests)\n";
