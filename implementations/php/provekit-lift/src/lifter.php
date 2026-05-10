<?php
/** ProvekIt PHP Lifter: parses PHP source, lifts contracts, speaks RPC.
 *  Single-binary entry: `php lifter.php --rpc`
 */

declare(strict_types=1);

require_once __DIR__ . '/../../provekit-ir-symbolic/src/Ir/Term.php';
require_once __DIR__ . '/../../provekit-ir-symbolic/src/Ir/Formula.php';
require_once __DIR__ . '/../../provekit-ir-symbolic/src/Ir/Declaration.php';
require_once __DIR__ . '/../../provekit-ir-symbolic/src/Canonicalizer/Blake3.php';
require_once __DIR__ . '/../../provekit-ir-symbolic/src/Canonicalizer/Jcs.php';
require_once __DIR__ . '/../../provekit-ir-symbolic/src/Canonicalizer/Ed25519.php';
require_once __DIR__ . '/../../provekit-ir-symbolic/src/ClaimEnvelope/Minter.php';
require_once __DIR__ . '/../../provekit-ir-symbolic/src/ProofEnvelope/Builder.php';

use ProvekIt\Ir\{Collector, ContractDecl};
use ProvekIt\Canonicalizer\{Blake3, Jcs, Ed25519};
use ProvekIt\ClaimEnvelope\Minter;
use ProvekIt\ProofEnvelope\Builder;

class ImplicationDecl implements JsonSerializable
{
    public function __construct(
        public readonly string $name,
        public readonly string $antecedent,
        public readonly string $consequent,
        public readonly string $antecedentSlot,
        public readonly string $consequentSlot,
        public readonly string $prover,
        public readonly string $proofWitness = '',
    ) {}

    public function jsonSerialize(): array
    {
        return [
            'name' => $this->name,
            'antecedent' => $this->antecedent,
            'consequent' => $this->consequent,
            'antecedentSlot' => $this->antecedentSlot,
            'consequentSlot' => $this->consequentSlot,
            'prover' => $this->prover,
            'proofWitness' => $this->proofWitness,
        ];
    }
}

// ════════════════════════════════════════════
// PHP Source Parser (native token_get_all)
// ════════════════════════════════════════════

class PhpFileParser
{
    /** @var array{kind: string, line: int, raw: string}[] */
    private array $tokens;
    private int $pos = 0;
    private string $source;
    private string $path;

    public function parse(string $path): array
    {
        $this->source = file_get_contents($path);
        $this->path = $path;
        $this->pos = 0;

        $raw = token_get_all($this->source);
        $this->tokens = [];
        foreach ($raw as $token) {
            if (is_array($token)) {
                $this->tokens[] = ['kind' => token_name($token[0]), 'line' => $token[2], 'raw' => $token[1]];
            } else {
                $this->tokens[] = ['kind' => 'CHAR', 'line' => -1, 'raw' => $token];
            }
        }

        return $this->walkFile();
    }

    private function walkFile(): array
    {
        $decls = [];
        $seen = 0;

        while ($this->pos < count($this->tokens)) {
            $t = $this->tokens[$this->pos];

            // DocBlock annotations
            if ($t['kind'] === 'T_DOC_COMMENT') {
                $result = $this->liftDocBlock();
                if ($result !== null) {
                    $decls[] = $result;
                    $seen++;
                }
            }

            // PHP 8 attributes
            if ($t['raw'] === '#[' || $t['kind'] === 'T_ATTRIBUTE') {
                // Skip for now: complex parse
            }

            // Function definition followed by contract
            if ($t['kind'] === 'T_FUNCTION') {
                // Check for preceding line-comment annotations
                $this->pos++;
            }

            // PHPUnit assertions
            if ($t['kind'] === 'T_VARIABLE' && str_starts_with($t['raw'], '$this') || $t['kind'] === 'T_STRING') {
                $result = $this->liftPhpUnitAssertion();
                if ($result !== null) {
                    $decls[] = $result;
                    $seen++;
                }
            }

            $this->pos++;
        }

        return ['decls' => $decls, 'seen' => $seen];
    }

    // ── DocBlock annotations ──

    private function liftDocBlock(): ?ContractDecl
    {
        $doc = $this->tokens[$this->pos]['raw'];
        $line = $this->tokens[$this->pos]['line'];
        $this->pos++;

        // Parse @provekit.target and @provekit.post from docblock
        $target = null;
        $post = null;

        foreach (explode("\n", $doc) as $docLine) {
            $docLine = trim($docLine, " *\t\r");
            if (preg_match('/^@provekit\.target\s+(.+)$/', $docLine, $m)) {
                $target = trim($m[1]);
            }
            if (preg_match('/^@provekit\.post\s+(\{.+)/', $docLine, $m)) {
                $post = trim($m[1]);
            }
            if (preg_match('/^@provekit\.contract$/', $docLine)) {
                // Simple contract marker
            }
        }

        // Find the function name that follows
        $funcName = $this->skipToNextFunction() ?? 'unknown';

        if ($target && $post) {
            // Parse JSON formula from @provekit.post
            $formula = json_decode($post, true);
            if (!$formula || !isset($formula['kind'])) return null;

            // Convert JSON array to IrFormula
            $irFormula = $this->jsonToFormula($formula);

            return new ContractDecl(
                name: $funcName,
                post: $irFormula,
            );
        }

        if ($target) {
            // Target only: create placeholder contract
            return new ContractDecl(
                name: $funcName,
                post: new \ProvekIt\Ir\AtomicFormula('true', []),
            );
        }

        return null;
    }

    // ── PHPUnit assertions ──

    private function liftPhpUnitAssertion(): ?ContractDecl
    {
        $t = $this->tokens[$this->pos];

        // Match $this->assert*(...) or self::assert*(...)
        $raw = $t['raw'];
        if ($t['kind'] !== 'T_STRING') return null;

        // Find T_OBJECT_OPERATOR or T_DOUBLE_COLON before us
        $hasArrow = false;
        for ($i = max(0, $this->pos - 2); $i < $this->pos; $i++) {
            if (in_array($this->tokens[$i]['raw'] ?? '', ['->', '::'])) {
                $hasArrow = true;
                break;
            }
        }
        if (!$hasArrow) return null;

        // Common PHPUnit assertion methods
        $method = strtolower($raw);
        if (!str_starts_with($method, 'assert')) return null;

        $parts = explode('(', $this->fetchRestOfLine());
        if (count($parts) < 2) return null;

        $args = $this->extractAssertArgs($method, $parts);
        if ($args === null) return null;

        $name = "phpunit_" . $method . "_L" . ($t['line'] ?? 0);

        return new ContractDecl(
            name: $name,
            inv: $args,
        );
    }

    // ── Helpers ──

    private function skipToNextFunction(): ?string
    {
        for ($i = $this->pos; $i < min($this->pos + 30, count($this->tokens)); $i++) {
            if ($this->tokens[$i]['kind'] === 'T_FUNCTION'
                && isset($this->tokens[$i + 2])
                && $this->tokens[$i + 2]['kind'] === 'T_STRING') {
                $name = $this->tokens[$i + 2]['raw'];
                $this->pos = $i;
                return $name;
            }
        }
        return null;
    }

    private function fetchRestOfLine(): string
    {
        $buf = '';
        for ($i = $this->pos; $i < min($this->pos + 20, count($this->tokens)); $i++) {
            $buf .= $this->tokens[$i]['raw'];
        }
        return $buf;
    }

    private function extractAssertArgs(string $method, array $parts): mixed
    {
        $combined = implode('(', array_slice($parts, 1));
        $combined = trim($combined, "); \t\n\r");

        try {
            return match ($method) {
                'assertequals', 'assertsame' => \ProvekIt\Ir\Eq(
                    \ProvekIt\Ir\Ctor1('testFunc', \ProvekIt\Ir\Str('call')),
                    \ProvekIt\Ir\Str('expected')
                ),
                'asserttrue' => \ProvekIt\Ir\Ctor1('AssertTrue', \ProvekIt\Ir\Str($combined)),
                'assertfalse' => \ProvekIt\Ir\Ctor1('AssertFalse', \ProvekIt\Ir\Str($combined)),
                'assertgreaterthan' => \ProvekIt\Ir\Gt(
                    \ProvekIt\Ir\Ctor1('testFunc', \ProvekIt\Ir\Str('call')),
                    \ProvekIt\Ir\Num(0)
                ),
                'assertnotnull' => \ProvekIt\Ir\NotNull(
                    \ProvekIt\Ir\Ctor1('testFunc', \ProvekIt\Ir\Str('call'))
                ),
                'assertequal' => \ProvekIt\Ir\Eq(
                    \ProvekIt\Ir\Ctor1('testFunc', \ProvekIt\Ir\Str('call')),
                    \ProvekIt\Ir\Str('expected')
                ),
                default => \ProvekIt\Ir\TrueAtom(),
            };
        } catch (\Throwable) {
            return \ProvekIt\Ir\TrueAtom();
        }
    }

    private function jsonToFormula(array $json): \ProvekIt\Ir\IrFormula
    {
        if (!isset($json['kind'])) return \ProvekIt\Ir\TrueAtom();

        switch ($json['kind']) {
            case 'atomic':
                $args = array_map(fn($a) => $this->jsonToTerm($a), $json['args'] ?? []);
                return new \ProvekIt\Ir\AtomicFormula($json['name'] ?? 'true', $args);
            case 'and':
            case 'or':
            case 'not':
            case 'implies':
                $ops = array_map(fn($o) => $this->jsonToFormula($o), $json['operands'] ?? []);
                return new \ProvekIt\Ir\ConnectiveFormula($json['kind'], $ops);
            case 'forall':
            case 'exists':
                // Sort is abstract; construct the Primitive variant via the
                // static helper. (Function/Dependent quantifier sorts could
                // be added here when the lifter starts emitting them.)
                $sort = \ProvekIt\Ir\Sort::Primitive($json['sort']['name'] ?? 'Ref');
                return new \ProvekIt\Ir\QuantifierFormula(
                    $json['kind'], $json['name'], $sort,
                    $this->jsonToFormula($json['body'])
                );
            default:
                return \ProvekIt\Ir\TrueAtom();
        }
    }

    private function jsonToTerm(array $json): \ProvekIt\Ir\IrTerm
    {
        return match ($json['kind'] ?? '') {
            'var' => new \ProvekIt\Ir\VarTerm($json['name']),
            'const' => new \ProvekIt\Ir\ConstTerm(
                $json['value'],
                \ProvekIt\Ir\Sort::Primitive($json['sort']['name'] ?? 'Ref')
            ),
            'ctor' => new \ProvekIt\Ir\CtorTerm(
                $json['name'],
                array_map(fn($a) => $this->jsonToTerm($a), $json['args'] ?? [])
            ),
            default => new \ProvekIt\Ir\VarTerm('_'),
        };
    }
}

class PhpProductionLifter
{
    /** @return array{decls: ContractDecl[], implications: ImplicationDecl[]} */
    public static function liftSource(string $source, string $sourceFile): array
    {
        [$functions, $tests] = self::collectBlocks($source);
        $decls = [];
        $implications = [];

        self::liftPhpUnitTests($functions, $tests, $sourceFile, $decls, $implications);
        self::liftProductionWalk($functions, $sourceFile, $decls, $implications);

        return ['decls' => $decls, 'implications' => $implications];
    }

    /** @return array{0: array<int, array{name: string, params: string[], body: array<int, array{text: string, line: int}>}>, 1: array<int, array{name: string, body: array<int, array{text: string, line: int}>}>} */
    private static function collectBlocks(string $source): array
    {
        $lines = explode("\n", $source);
        $functions = [];
        $tests = [];

        for ($i = 0; $i < count($lines); $i++) {
            if (!preg_match('/\bfunction\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(([^)]*)\).*{/', $lines[$i], $m)) {
                continue;
            }
            $name = $m[1];
            $body = self::collectBody($lines, $i);
            if (self::isTestFunction($name)) {
                $tests[] = ['name' => $name, 'body' => $body];
            } else {
                $functions[] = [
                    'name' => $name,
                    'params' => self::parseParams($m[2]),
                    'body' => $body,
                ];
            }
        }

        return [$functions, $tests];
    }

    /** @param string[] $lines */
    private static function collectBody(array $lines, int &$i): array
    {
        $body = [];
        $depth = self::braceDelta($lines[$i]);
        while ($depth > 0 && $i + 1 < count($lines)) {
            $i++;
            $nextDepth = $depth + self::braceDelta($lines[$i]);
            if (!($nextDepth <= 0 && trim($lines[$i]) === '}')) {
                $body[] = ['text' => $lines[$i], 'line' => $i + 1];
            }
            $depth = $nextDepth;
        }
        return $body;
    }

    private static function braceDelta(string $line): int
    {
        return substr_count($line, '{') - substr_count($line, '}');
    }

    /** @return string[] */
    private static function parseParams(string $params): array
    {
        $out = [];
        foreach (explode(',', $params) as $part) {
            if (preg_match('/\$([A-Za-z_][A-Za-z0-9_]*)/', $part, $m)) {
                $out[] = $m[1];
            }
        }
        return $out;
    }

    private static function isTestFunction(string $name): bool
    {
        return str_starts_with($name, 'test') || str_ends_with($name, 'Test');
    }

    /** @param array<int, array{name: string, params: string[], body: array<int, array{text: string, line: int}>}> $functions */
    private static function liftProductionWalk(array $functions, string $sourceFile, array &$decls, array &$implications): void
    {
        $preconditions = [];
        foreach ($functions as $fn) {
            $pre = self::liftFunctionPrecondition($fn);
            if ($pre !== null) {
                $preconditions[] = ['name' => $fn['name'], 'params' => $fn['params'], 'precondition' => $pre];
            }
        }

        $used = [];
        foreach ($functions as $caller) {
            foreach ($preconditions as $callee) {
                if ($caller['name'] === $callee['name']) continue;
                self::emitWalksForCallee($caller, $callee, $sourceFile, $decls, $implications, $used);
            }
        }
    }

    private static function liftFunctionPrecondition(array $function): ?\ProvekIt\Ir\IrFormula
    {
        $formulas = [];
        foreach ($function['body'] as $idx => $line) {
            $text = trim($line['text']);
            if (preg_match('/\bassert\s*\((.+)\)\s*;?/', $text, $m)) {
                $formulas[] = self::liftFormula($m[1]);
                continue;
            }
            if (preg_match('/\bif\s*\((.+)\)/', $text, $m)) {
                $throws = str_contains($text, 'throw');
                for ($j = $idx + 1; !$throws && $j < min($idx + 4, count($function['body'])); $j++) {
                    $throws = str_contains($function['body'][$j]['text'], 'throw');
                }
                if ($throws) {
                    $formulas[] = self::liftFormula($m[1], true);
                }
            }
        }
        if ($formulas === []) return null;
        return count($formulas) === 1 ? $formulas[0] : new \ProvekIt\Ir\ConnectiveFormula('and', $formulas);
    }

    private static function emitWalksForCallee(array $caller, array $callee, string $sourceFile, array &$decls, array &$implications, array &$used): void
    {
        foreach (self::findCallsites($caller, $callee['name']) as $hit) {
            if (count($hit['args']) !== count($callee['params'])) continue;

            $wp = $callee['precondition'];
            foreach ($callee['params'] as $i => $formal) {
                $wp = self::substituteFormula($wp, $formal, $hit['args'][$i]);
            }

            $base = $callee['name'] . '@' . $sourceFile . ':' . $hit['line'] . ':' . $hit['col'];
            self::appendEdge($decls, $implications, $used, $base . '::callsite', $wp, $wp, $caller['name'], $callee['name'], 'php-wp-walk');

            $previousWp = $wp;
            for ($i = $hit['stmtIndex'] - 1; $i >= 0; $i--) {
                $binding = self::bindingFromLine($caller['body'][$i]['text']);
                if ($binding !== null) {
                    $nextWp = self::substituteFormula($previousWp, $binding['name'], $binding['term']);
                    self::appendEdge($decls, $implications, $used, $base . '::let:' . $binding['name'], $nextWp, $previousWp, $caller['name'], $callee['name'], 'php-wp-walk');
                    $previousWp = $nextWp;
                }
            }

            self::appendEdge($decls, $implications, $used, $base . '::entry', $previousWp, $previousWp, $caller['name'], $callee['name'], 'php-wp-walk');
        }
    }

    private static function findCallsites(array $caller, string $calleeName): array
    {
        $hits = [];
        $needle = $calleeName . '(';
        foreach ($caller['body'] as $idx => $line) {
            if (preg_match('/\bfunction\s+' . preg_quote($calleeName, '/') . '\s*\(/', $line['text'])) continue;
            $pos = strpos($line['text'], $needle);
            if ($pos === false) continue;
            $argsText = self::extractDelimited($line['text'], $pos + strlen($calleeName));
            $hits[] = [
                'line' => $line['line'],
                'col' => $pos + 1,
                'stmtIndex' => $idx,
                'args' => array_map(fn(string $arg) => self::termFromExpr($arg), self::splitArgs($argsText)),
            ];
        }
        return $hits;
    }

    private static function appendEdge(array &$decls, array &$implications, array &$used, string $rawName, \ProvekIt\Ir\IrFormula $pre, \ProvekIt\Ir\IrFormula $post, string $caller, string $callee, string $prover): void
    {
        $name = self::uniqueName($rawName, $used);
        $decls[] = new ContractDecl($name, 'result', $pre, $post);
        $implications[] = new ImplicationDecl(
            $name . '::pre-implies-post',
            $name,
            $name,
            'pre',
            'post',
            $prover,
            $caller . '->' . $callee,
        );
    }

    /** @param array<int, array{name: string, params: string[], body: array<int, array{text: string, line: int}>}> $functions */
    private static function liftPhpUnitTests(array $functions, array $tests, string $sourceFile, array &$decls, array &$implications): void
    {
        $known = array_fill_keys(array_map(fn(array $f) => $f['name'], $functions), true);
        $used = [];
        foreach ($tests as $test) {
            $observed = [];
            foreach ($test['body'] as $line) {
                $binding = self::bindingFromLine($line['text']);
                if ($binding !== null && self::callNameFromTerm($binding['term']) !== null && isset($known[self::callNameFromTerm($binding['term'])])) {
                    $callName = self::callNameFromTerm($binding['term']);
                    $pos = strpos($line['text'], $callName . '(');
                    $observed[] = [
                        'local' => $binding['name'],
                        'base' => $callName . '@' . $sourceFile . ':' . $line['line'] . ':' . (($pos === false ? 0 : $pos) + 1),
                        'term' => $binding['term'],
                    ];
                    continue;
                }

                if (!preg_match('/(?:\$this->|self::)(assertSame|assertEquals|assertNotSame|assertNotEquals)\s*\((.*)\)\s*;/', trim($line['text']), $m)) {
                    continue;
                }
                $args = self::splitArgs($m[2]);
                if (count($args) < 2) continue;

                $left = self::observedByLocal($observed, $args[0]);
                $right = self::observedByLocal($observed, $args[1]);
                $call = $left ?? $right;
                if ($call === null) continue;

                $lhs = $left !== null ? $call['term'] : self::termFromExpr($args[0]);
                $rhs = $right !== null ? $call['term'] : self::termFromExpr($args[1]);
                $isNeq = str_contains(strtolower($m[1]), 'not');
                $assertion = new \ProvekIt\Ir\AtomicFormula($isNeq ? "\u{2260}" : '=', [$lhs, $rhs]);
                self::appendTestValueScope($decls, $implications, $used, $call, $assertion, $test['name']);
            }
        }
    }

    private static function appendTestValueScope(array &$decls, array &$implications, array &$used, array $call, \ProvekIt\Ir\IrFormula $assertion, string $testName): void
    {
        $factsName = self::uniqueName($call['base'] . '::facts', $used);
        $assertionName = self::uniqueName($call['base'] . '::assertion', $used);
        $decls[] = new ContractDecl($factsName, 'out', null, null, \ProvekIt\Ir\Eq(\ProvekIt\Ir\V($call['local']), $call['term']));
        $decls[] = new ContractDecl($assertionName, 'out', null, null, $assertion);
        $implications[] = new ImplicationDecl(
            self::uniqueName($call['base'] . '::facts-implies-assertion', $used),
            $factsName,
            $assertionName,
            'inv',
            'inv',
            'php-test-value-scope',
            $testName . ' assertion',
        );
    }

    private static function liftFormula(string $expr, bool $negate = false): \ProvekIt\Ir\IrFormula
    {
        $ops = [
            '>=' => ["\u{2265}", '<'],
            '<=' => ["\u{2264}", '>'],
            '===' => ['=', "\u{2260}"],
            '==' => ['=', "\u{2260}"],
            '!==' => ["\u{2260}", '='],
            '!=' => ["\u{2260}", '='],
            '>' => ['>', "\u{2264}"],
            '<' => ['<', "\u{2265}"],
        ];
        $expr = self::trimOuterParens(trim($expr));
        foreach ($ops as $text => [$name, $inverse]) {
            $pos = strpos($expr, $text);
            if ($pos === false) continue;
            return new \ProvekIt\Ir\AtomicFormula(
                $negate ? $inverse : $name,
                [
                    self::termFromExpr(substr($expr, 0, $pos)),
                    self::termFromExpr(substr($expr, $pos + strlen($text))),
                ],
            );
        }
        return \ProvekIt\Ir\Eq(self::termFromExpr($expr), new \ProvekIt\Ir\ConstTerm(!$negate, \ProvekIt\Ir\Sort::Bool()));
    }

    private static function termFromExpr(string $expr): \ProvekIt\Ir\IrTerm
    {
        $expr = trim(trim($expr), ';');
        $expr = self::trimOuterParens($expr);
        if (preg_match('/^\$([A-Za-z_][A-Za-z0-9_]*)$/', $expr, $m)) {
            return \ProvekIt\Ir\V($m[1]);
        }
        if (preg_match('/^-?\d+$/', $expr)) {
            return \ProvekIt\Ir\Num((int)$expr);
        }
        if ($expr === 'true' || $expr === 'false') {
            return new \ProvekIt\Ir\ConstTerm($expr === 'true', \ProvekIt\Ir\Sort::Bool());
        }
        if ((str_starts_with($expr, '"') && str_ends_with($expr, '"')) || (str_starts_with($expr, "'") && str_ends_with($expr, "'"))) {
            return \ProvekIt\Ir\Str(substr($expr, 1, -1));
        }
        if (preg_match('/^([A-Za-z_][A-Za-z0-9_]*)\s*\((.*)\)$/', $expr, $m)) {
            return new \ProvekIt\Ir\CtorTerm($m[1], array_map(fn(string $arg) => self::termFromExpr($arg), self::splitArgs($m[2])));
        }
        return \ProvekIt\Ir\V(ltrim($expr, '$'));
    }

    private static function substituteFormula(\ProvekIt\Ir\IrFormula $formula, string $name, \ProvekIt\Ir\IrTerm $replacement): \ProvekIt\Ir\IrFormula
    {
        if ($formula instanceof \ProvekIt\Ir\AtomicFormula) {
            return new \ProvekIt\Ir\AtomicFormula($formula->name, array_map(fn($arg) => self::substituteTerm($arg, $name, $replacement), $formula->args));
        }
        if ($formula instanceof \ProvekIt\Ir\ConnectiveFormula) {
            return new \ProvekIt\Ir\ConnectiveFormula($formula->kind, array_map(fn($op) => self::substituteFormula($op, $name, $replacement), $formula->operands));
        }
        return $formula;
    }

    private static function substituteTerm(\ProvekIt\Ir\IrTerm $term, string $name, \ProvekIt\Ir\IrTerm $replacement): \ProvekIt\Ir\IrTerm
    {
        if ($term instanceof \ProvekIt\Ir\VarTerm && $term->name === $name) {
            return $replacement;
        }
        if ($term instanceof \ProvekIt\Ir\CtorTerm) {
            return new \ProvekIt\Ir\CtorTerm($term->name, array_map(fn($arg) => self::substituteTerm($arg, $name, $replacement), $term->args));
        }
        return $term;
    }

    private static function bindingFromLine(string $line): ?array
    {
        if (!preg_match('/^\s*\$([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.+?)\s*;\s*$/', trim($line), $m)) {
            return null;
        }
        return ['name' => $m[1], 'term' => self::termFromExpr($m[2])];
    }

    private static function callNameFromTerm(\ProvekIt\Ir\IrTerm $term): ?string
    {
        return $term instanceof \ProvekIt\Ir\CtorTerm ? $term->name : null;
    }

    private static function observedByLocal(array $observed, string $expr): ?array
    {
        $needle = ltrim(trim($expr), '$');
        foreach ($observed as $call) {
            if ($call['local'] === $needle) return $call;
        }
        return null;
    }

    private static function extractDelimited(string $line, int $openPos): string
    {
        if (!isset($line[$openPos]) || $line[$openPos] !== '(') return '';
        $depth = 0;
        for ($i = $openPos; $i < strlen($line); $i++) {
            if ($line[$i] === '(') $depth++;
            if ($line[$i] === ')') {
                $depth--;
                if ($depth === 0) {
                    return trim(substr($line, $openPos + 1, $i - $openPos - 1));
                }
            }
        }
        return '';
    }

    /** @return string[] */
    private static function splitArgs(string $args): array
    {
        $out = [];
        $depth = 0;
        $start = 0;
        for ($i = 0; $i < strlen($args); $i++) {
            if ($args[$i] === '(') $depth++;
            if ($args[$i] === ')') $depth--;
            if ($args[$i] === ',' && $depth === 0) {
                $out[] = trim(substr($args, $start, $i - $start));
                $start = $i + 1;
            }
        }
        $tail = trim(substr($args, $start));
        if ($tail !== '') $out[] = $tail;
        return $out;
    }

    private static function trimOuterParens(string $expr): string
    {
        while (strlen($expr) >= 2 && $expr[0] === '(' && substr($expr, -1) === ')') {
            $expr = trim(substr($expr, 1, -1));
        }
        return $expr;
    }

    private static function uniqueName(string $raw, array &$used): string
    {
        if (!isset($used[$raw])) {
            $used[$raw] = true;
            return $raw;
        }
        for ($i = 1;; $i++) {
            $candidate = $raw . '::' . $i;
            if (!isset($used[$candidate])) {
                $used[$candidate] = true;
                return $candidate;
            }
        }
    }
}

// ════════════════════════════════════════════
// Lifter orchestrator
// ════════════════════════════════════════════

class PhpLifter
{
    /** Walk a directory, parse .php files, collect declarations. */
    public static function liftDir(string $root): array
    {
        $allDecls = [];
        $filesScanned = 0;
        $warnings = [];

        $iter = new RecursiveIteratorIterator(
            new RecursiveDirectoryIterator($root, FilesystemIterator::SKIP_DOTS)
        );

        foreach ($iter as $entry) {
            if ($entry->getExtension() !== 'php') continue;
            $path = $entry->getPathname();

            // Skip vendor, cache, node_modules
            if (str_contains($path, '/vendor/') || str_contains($path, '/cache/')) continue;

            $filesScanned++;
            try {
                $parser = new PhpFileParser();
                $result = $parser->parse($path);
                foreach ($result['decls'] as $d) {
                    $allDecls[] = $d;
                }
            } catch (\Throwable $e) {
                $warnings[] = "{$path}: {$e->getMessage()}";
            }
        }

        return [
            'decls' => $allDecls,
            'filesScanned' => $filesScanned,
            'warnings' => $warnings,
        ];
    }

    /** Walk a directory and return raw IR declarations plus implications. */
    public static function liftIrDir(string $root): array
    {
        $allDecls = [];
        $allImplications = [];
        $filesScanned = 0;
        $warnings = [];

        $iter = new RecursiveIteratorIterator(
            new RecursiveDirectoryIterator($root, FilesystemIterator::SKIP_DOTS)
        );

        foreach ($iter as $entry) {
            if ($entry->getExtension() !== 'php') continue;
            $path = $entry->getPathname();
            if (str_contains($path, '/vendor/') || str_contains($path, '/cache/')) continue;

            $filesScanned++;
            try {
                $source = file_get_contents($path);
                if ($source === false) continue;
                $lifted = PhpProductionLifter::liftSource($source, basename($path));
                array_push($allDecls, ...$lifted['decls']);
                array_push($allImplications, ...$lifted['implications']);
            } catch (\Throwable $e) {
                $warnings[] = "{$path}: {$e->getMessage()}";
            }
        }

        return [
            'decls' => $allDecls,
            'implications' => $allImplications,
            'filesScanned' => $filesScanned,
            'warnings' => $warnings,
        ];
    }

    /** Full lift + mint + bundle pipeline */
    public static function liftAndMint(string $root, string $outDir): array
    {
        $lifted = self::liftDir($root);

        $signer = Ed25519::foundation();
        $minter = new Minter($signer);
        $builder = new Builder($signer);

        $members = [];
        $contractCids = [];
        $producedBy = 'provekit-lift-php@0.1.0';
        $producedAt = '2026-05-03T18:00:00Z';

        foreach ($lifted['decls'] as $decl) {
            $minted = $minter->mintContract($decl, $producedBy, $producedAt);
            $members[$minted['envelopeCid']] = $minted['canonicalBytes'];
            $contractCids[$decl->name] = $minted['cid'];
        }

        if (empty($members)) {
            $built = $builder->build('empty-lift', '0.1.0', [], $producedAt);
        } else {
            $built = $builder->build('php-lift', '0.1.0', $members, $producedAt);
        }

        $proofPath = $outDir . '/' . $built['cid'] . '.proof';
        if (!is_dir($outDir)) mkdir($outDir, 0755, true);
        file_put_contents($proofPath, $built['bytes']);

        $contractCidValues = array_values(array_filter($contractCids, fn($c) => str_starts_with($c, 'blake3-512:')));
        sort($contractCidValues);
        $contractSetCid = Blake3::cid(Jcs::encode($contractCidValues));

        return [
            'cid' => $built['cid'],
            'contractSetCid' => $contractSetCid,
            'proofPath' => $proofPath,
            'bytes' => $built['bytes'],
            'contractCount' => count($members),
            'filesScanned' => $lifted['filesScanned'],
            'warnings' => $lifted['warnings'],
        ];
    }
}

// ════════════════════════════════════════════
// RPC Mode
// ════════════════════════════════════════════

if (in_array('--rpc', $argv)) {
    $stdin = fopen('php://stdin', 'r');
    while (($line = fgets($stdin)) !== false) {
        $line = trim($line);
        if ($line === '') continue;
        $req = json_decode($line, true);
        if (!is_array($req) || !isset($req['id'], $req['method'])) continue;

        $id = $req['id'];
        $method = $req['method'];

        try {
            match ($method) {
                'initialize' => send(json_encode([
                    'jsonrpc' => '2.0', 'id' => $id,
                    'result' => [
                        'name' => 'provekit-lift-php',
                        'version' => '1.0.0',
                        'protocol_version' => 'provekit-lift/1',
                        'capabilities' => [
                            'authoring_surfaces' => ['php', 'phpunit', 'php-docblock'],
                            'ir_version' => 'v1.1.0',
                            'emits_signed_mementos' => true,
                        ],
                    ],
                ])),

                'lift' => (function () use ($id, $req) {
                    $ws = $req['params']['workspace_root'] ?? getcwd();
                    $result = PhpLifter::liftIrDir($ws);

                    send(json_encode([
                        'jsonrpc' => '2.0', 'id' => $id,
                        'result' => [
                            'kind' => 'ir-document',
                            'ir' => $result['decls'],
                            'implications' => $result['implications'],
                            'diagnostics' => $result['warnings'],
                        ],
                    ]));
                })(),

                'shutdown' => (function () use ($id) {
                    send(json_encode(['jsonrpc' => '2.0', 'id' => $id, 'result' => null]));
                    exit(0);
                })(),

                default => send(json_encode([
                    'jsonrpc' => '2.0', 'id' => $id,
                    'error' => ['code' => -32601, 'message' => "METHOD_NOT_FOUND: {$method}"],
                ])),
            };
        } catch (\Throwable $e) {
            send(json_encode([
                'jsonrpc' => '2.0', 'id' => $id,
                'error' => ['code' => -32603, 'message' => $e->getMessage()],
            ]));
        }
    }

    fclose($stdin);
    exit(0);
}

// ════════════════════════════════════════════
// Direct mode
// ════════════════════════════════════════════

if (PHP_SAPI === 'cli') {
    $ws = $argv[1] ?? getcwd();
    $out = $argv[2] ?? $ws;
    $result = PhpLifter::liftAndMint($ws, $out);
    echo "files: {$result['filesScanned']}\n";
    echo "contracts: {$result['contractCount']}\n";
    echo "cid: {$result['cid']}\n";
    echo ".proof: {$result['proofPath']}\n";
}

function send(string $json): void { echo $json . "\n"; flush(); }
