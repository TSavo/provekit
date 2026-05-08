<?php
declare(strict_types=1);

require_once __DIR__ . '/../src/ForwardPropagator.php';

function fail_test(string $message): void
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

function check_positive_entry(): BaselineEntry
{
    return BaselineEntry::new(
        'checkPositive',
        ForwardPost::known(['x > 0']),
        ForwardPost::known(['returns true']),
    );
}

function consume_return_entry(): BaselineEntry
{
    return BaselineEntry::new(
        'consumeReturn',
        ForwardPost::known(['returns true']),
        ForwardPost::empty(),
    );
}

function call_check_positive(): ForwardStmt
{
    return ForwardStmt::call('checkPositive', LspRange::singleLine(4, 12, 25));
}

function call_consume_return(): ForwardStmt
{
    return ForwardStmt::call('consumeReturn', LspRange::singleLine(5, 12, 25));
}

function testCallsiteSatisfiesPreNoDiagnostic(): void
{
    $propagator = new ForwardPropagator([check_positive_entry()]);
    $diagnostics = $propagator->emitDiagnostics([
        ForwardStmt::assign(ForwardPost::known(['x > 0', 'caller kept an extra fact'])),
        call_check_positive(),
    ]);

    assert_same(0, count($diagnostics), 'satisfying callsite should not emit diagnostics');
}

function testCallsiteViolatesPreDiagnosticEmitted(): void
{
    $propagator = new ForwardPropagator([check_positive_entry()]);
    $diagnostics = $propagator->emitDiagnostics([
        ForwardStmt::assign(ForwardPost::known(['x <= 0'])),
        call_check_positive(),
    ]);

    assert_same(1, count($diagnostics), 'violating callsite should emit one diagnostic');
    $diagnostic = $diagnostics[0]->toArray();
    assert_same('implication-failed', $diagnostic['code'], 'diagnostic code');
    assert_same('provekit', $diagnostic['source'], 'diagnostic source');
    assert_same(1, $diagnostic['severity'], 'diagnostic severity');
    assert_same('checkPositive', $diagnostic['data']['callee'], 'diagnostic callee');
    assert_same(['x > 0'], $diagnostic['data']['missing_conjuncts'], 'missing conjuncts');
    assert_true(str_starts_with($diagnostic['data']['current_post_cid'], 'blake3-512:'), 'current_post_cid prefix');
    assert_true(str_starts_with($diagnostic['data']['baseline_index_cid'], 'blake3-512:'), 'baseline_index_cid prefix');
}

function testBranchMergePartialSatisfaction(): void
{
    $propagator = new ForwardPropagator([check_positive_entry()]);
    $diagnostics = $propagator->emitDiagnostics([
        ForwardStmt::ifElse(
            [ForwardStmt::assign(ForwardPost::known(['x > 0']))],
            [ForwardStmt::assign(ForwardPost::empty())],
        ),
        call_check_positive(),
    ]);

    assert_same(1, count($diagnostics), 'join path should emit one diagnostic');
    assert_same(['x > 0'], $diagnostics[0]->toArray()['data']['missing_conjuncts'], 'join path missing conjuncts');
}

function testTopFallbackSuppressesFalsePositive(): void
{
    $propagator = new ForwardPropagator([check_positive_entry()]);
    $diagnostics = $propagator->emitDiagnostics([
        ForwardStmt::unsupported(),
        call_check_positive(),
    ]);

    assert_same(0, count($diagnostics), 'top fallback should suppress implication-failed diagnostics');
}

function testFailedPreconditionDoesNotPropagateCalleePostcondition(): void
{
    $propagator = new ForwardPropagator([check_positive_entry(), consume_return_entry()]);
    $diagnostics = $propagator->emitDiagnostics([
        ForwardStmt::assign(ForwardPost::known(['x <= 0'])),
        call_check_positive(),
        call_consume_return(),
    ]);

    assert_same(2, count($diagnostics), 'failed precondition should not add the callee postcondition');
    assert_same('checkPositive', $diagnostics[0]->toArray()['data']['callee'], 'first diagnostic callee');
    assert_same('consumeReturn', $diagnostics[1]->toArray()['data']['callee'], 'second diagnostic callee');
}

function testClassMethodResetPreventsFactLeak(): void
{
    $source = <<<'PHP'
<?php
class Demo {
    public function establishesFact(): void {
        checkPositive(5);
    }

    private function violates(): void {
        checkPositive(-1);
    }
}
PHP;

    $diagnostics = ForwardPropagator::floorV1SeedIndex()
        ->emitDiagnostics(ForwardPropagator::lowerFloorSource($source));

    assert_same(1, count($diagnostics), 'class method declarations should reset forward state');
}

function testUnbracedLoopBodyUsesTopFallback(): void
{
    $source = <<<'PHP'
<?php
function demo($condition): void {
    while ($condition)
        checkPositive(-1);
}
PHP;

    $diagnostics = ForwardPropagator::floorV1SeedIndex()
        ->emitDiagnostics(ForwardPropagator::lowerFloorSource($source));

    assert_same(0, count($diagnostics), 'unbraced loop bodies should keep top fallback suppression');
}

function testNextLineLoopBraceUsesTopFallbackForWholeBody(): void
{
    $source = <<<'PHP'
<?php
function demo($condition): void {
    while ($condition)
    {
        checkPositive(-1);
        checkPositive(-2);
    }
}
PHP;

    $diagnostics = ForwardPropagator::floorV1SeedIndex()
        ->emitDiagnostics(ForwardPropagator::lowerFloorSource($source));
    $stmts = ForwardPropagator::lowerFloorSource($source);
    $unsupportedCount = count(array_filter($stmts, static fn (ForwardStmt $stmt): bool => $stmt->kind === 'unsupported'));

    assert_same(0, count($diagnostics), 'next-line loop braces should keep top fallback for the whole body');
    assert_same(2, $unsupportedCount, 'both loop body calls should lower under top fallback');
}

function testCommentsAndStringsDoNotCreateCallsites(): void
{
    $source = <<<'PHP'
<?php
function demo(): void {
    // checkPositive(-1);
    $message = "checkPositive(-1)";
}
PHP;

    $diagnostics = ForwardPropagator::floorV1SeedIndex()
        ->emitDiagnostics(ForwardPropagator::lowerFloorSource($source));

    assert_same(0, count($diagnostics), 'comments and strings must not lower to callsites');
}

function testCheckPositiveScannerUsesIdentifierBoundaries(): void
{
    $source = <<<'PHP'
<?php
function demo(): void {
    notcheckPositive(-1);
    $checkPositive(-1);
}
PHP;

    $diagnostics = ForwardPropagator::floorV1SeedIndex()
        ->emitDiagnostics(ForwardPropagator::lowerFloorSource($source));

    assert_same(0, count($diagnostics), 'embedded names and variable functions should not lower to checkPositive callsites');
}

function testCheckPositiveScannerAllowsWhitespaceBeforeParen(): void
{
    $source = <<<'PHP'
<?php
function demo(): void {
    checkPositive (-1);
}
PHP;

    $diagnostics = ForwardPropagator::floorV1SeedIndex()
        ->emitDiagnostics(ForwardPropagator::lowerFloorSource($source));

    assert_same(1, count($diagnostics), 'whitespace before the call paren should still lower checkPositive callsites');
}

function testParseFloorFixtureEmitsForwardPropagationDiagnostic(): void
{
    $root = realpath(__DIR__ . '/../../../..');
    assert_true($root !== false, 'repo root must resolve');
    $source = file_get_contents($root . '/tests/lsp/floor-fixture/php.php');
    assert_true(is_string($source), 'fixture must be readable');

    $cmd = ['php', __DIR__ . '/../src/lspd.php'];
    $proc = proc_open(
        $cmd,
        [0 => ['pipe', 'r'], 1 => ['pipe', 'w'], 2 => ['pipe', 'w']],
        $pipes,
    );
    assert_true(is_resource($proc), 'lspd process should start');

    $request = json_encode([
        'jsonrpc' => '2.0',
        'id' => 1,
        'method' => 'parse',
        'params' => ['path' => 'php.php', 'source' => $source],
    ], JSON_THROW_ON_ERROR);
    fwrite($pipes[0], $request . "\n");
    fclose($pipes[0]);

    $line = fgets($pipes[1]);
    fclose($pipes[1]);
    $stderr = stream_get_contents($pipes[2]);
    fclose($pipes[2]);
    $code = proc_close($proc);
    assert_same(0, $code, 'lspd process exit code: ' . $stderr);
    assert_true(is_string($line), 'lspd should emit one response line');

    $response = json_decode(trim($line), true, flags: JSON_THROW_ON_ERROR);
    $diagnostics = $response['result']['diagnostics'] ?? null;
    assert_true(is_array($diagnostics), 'diagnostics field should be an array');
    assert_same(1, count($diagnostics), 'parse fixture should emit one diagnostic');
    assert_same('implication-failed', $diagnostics[0]['code'], 'parse diagnostic code');
    assert_same('provekit.lsp.implication_failed', $diagnostics[0]['data']['kind'], 'parse diagnostic kind');
    assert_same('checkPositive', $diagnostics[0]['data']['callee'], 'parse diagnostic callee');
}

testCallsiteSatisfiesPreNoDiagnostic();
testCallsiteViolatesPreDiagnosticEmitted();
testBranchMergePartialSatisfaction();
testTopFallbackSuppressesFalsePositive();
testFailedPreconditionDoesNotPropagateCalleePostcondition();
testClassMethodResetPreventsFactLeak();
testUnbracedLoopBodyUsesTopFallback();
testNextLineLoopBraceUsesTopFallbackForWholeBody();
testCommentsAndStringsDoNotCreateCallsites();
testCheckPositiveScannerUsesIdentifierBoundaries();
testCheckPositiveScannerAllowsWhitespaceBeforeParen();
testParseFloorFixtureEmitsForwardPropagationDiagnostic();

echo "ForwardPropagatorTest passed\n";
