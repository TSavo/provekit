<?php
/** ProvekIt PHP Lifter — parses PHP source, lifts contracts, speaks RPC.
 *  Five adapters: DocBlock, Symfony Validator, PHPStan/Psalm, PHPUnit, Pest.
 *  Single-binary entry: `php lifter.php --rpc`
 */

declare(strict_types=1);

require_once __DIR__ . '/../provekit-ir-symbolic/src/Ir/Term.php';
require_once __DIR__ . '/../provekit-ir-symbolic/src/Ir/Formula.php';
require_once __DIR__ . '/../provekit-ir-symbolic/src/Ir/Declaration.php';
require_once __DIR__ . '/../provekit-ir-symbolic/src/Canonicalizer/Blake3.php';
require_once __DIR__ . '/../provekit-ir-symbolic/src/Canonicalizer/Jcs.php';
require_once __DIR__ . '/../provekit-ir-symbolic/src/Canonicalizer/Ed25519.php';
require_once __DIR__ . '/../provekit-ir-symbolic/src/ClaimEnvelope/Minter.php';
require_once __DIR__ . '/../provekit-ir-symbolic/src/ProofEnvelope/Builder.php';

use ProvekIt\Ir\{Collector, ContractDecl, IrFormula, IrTerm, Sort};
use ProvekIt\Ir\AtomicFormula;
use ProvekIt\Ir\ConnectiveFormula;
use ProvekIt\Ir\QuantifierFormula;
use ProvekIt\Ir\VarTerm;
use ProvekIt\Ir\ConstTerm;
use ProvekIt\Ir\CtorTerm;
use ProvekIt\Canonicalizer\{Blake3, Jcs, Ed25519};
use ProvekIt\ClaimEnvelope\Minter;
use ProvekIt\ProofEnvelope\Builder;

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

            // ── DocBlock annotations (@provekit.*, PHPStan/Psalm) ──
            if ($t['kind'] === 'T_DOC_COMMENT') {
                $result = $this->liftDocBlock();
                if ($result !== null) { $decls[] = $result; $seen++; }
                $result2 = $this->liftPhpstanFromDocblock();
                if ($result2 !== null) { $decls[] = $result2; $seen++; }
            }

            // ── Symfony Validator attributes ──
            if ($t['raw'] === '#[' || $t['kind'] === 'T_ATTRIBUTE') {
                $result = $this->liftSymfonyValidator();
                if ($result !== null) { $decls[] = $result; $seen++; }
            }

            // ── PHPUnit assertions ──
            if ($t['kind'] === 'T_STRING' && preg_match('/^assert/i', $t['raw'])) {
                $result = $this->liftPhpUnitAssertion();
                if ($result !== null) { $decls[] = $result; $seen++; }
            }

            // ── Pest assertions ──
            if ($t['kind'] === 'T_STRING' && $t['raw'] === 'expect') {
                $result = $this->liftPestAssertion();
                if ($result !== null) { $decls[] = $result; $seen++; }
            }

            $this->pos++;
        }

        return ['decls' => $decls, 'seen' => $seen];
    }

    // ════════════════════════════════════════════
    // Adapter 1: DocBlock (@provekit.target, @provekit.post, @provekit.contract)
    // ════════════════════════════════════════════

    private function liftDocBlock(): ?ContractDecl
    {
        $doc = $this->tokens[$this->pos]['raw'];
        $line = $this->tokens[$this->pos]['line'];
        $this->pos++;

        $target = null; $post = null;
        foreach (explode("\n", $doc) as $docLine) {
            $docLine = trim($docLine, " *\t\r");
            if (preg_match('/^@provekit\.target\s+(.+)$/', $docLine, $m)) $target = trim($m[1]);
            if (preg_match('/^@provekit\.post\s+(\{.+)/', $docLine, $m)) $post = trim($m[1]);
        }

        $funcName = $this->skipToNextFunction() ?? 'unknown';

        if ($target && $post) {
            $formula = json_decode($post, true);
            if (!$formula || !isset($formula['kind'])) return null;
            return new ContractDecl(name: $funcName, post: $this->jsonToFormula($formula));
        }
        if ($target) {
            return new ContractDecl(name: $funcName, post: new AtomicFormula('true', []));
        }
        return null;
    }

    // ════════════════════════════════════════════
    // Adapter 2: Symfony Validator (attributes + docblock constraints)
    // ════════════════════════════════════════════

    /** Recognized constraint -> IR atomic */
    private const SYMFONY_CONSTRAINT_MAP = [
        'NotBlank'   => 'not_null',
        'NotNull'    => 'not_null',
        'IsTrue'     => '=',
        'IsFalse'    => '=',
        'Email'      => 'email',
        'Url'        => 'url',
        'Uuid'       => 'uuid',
        'Ip'         => 'ip',
        'Length'     => null,              // handled specially (min/max args)
        'Range'      => null,
        'GreaterThan'       => '>',
        'GreaterThanOrEqual'=> "\u{2265}",
        'LessThan'          => '<',
        'LessThanOrEqual'   => "\u{2264}",
        'Positive'   => "\u{2265}",
        'Negative'   => "\u{2264}",
        'PositiveOrZero' => "\u{2265}",
        'NegativeOrZero' => "\u{2264}",
        'Count'      => null,
        'Choice'     => null,
        'Regex'      => 'matches',
        'Time'       => null,
        'Date'       => null,
        'DateTime'   => null,
    ];

    private function liftSymfonyValidator(): ?ContractDecl
    {
        $t = $this->tokens[$this->pos];
        $line = $t['line'];

        // Collect all attributes on this property/method by reading until we pass the attribute block
        $attrs = [];
        $propName = null;
        $className = null;
        $startPos = $this->pos;

        // First pass: collect the attribute block and find the class/property name
        for ($i = $startPos; $i < min($startPos + 60, count($this->tokens)); $i++) {
            $tok = $this->tokens[$i];

            // Named attribute: #[Assert\Constraint(args...)]
            if ($tok['kind'] === 'T_ATTRIBUTE' || $tok['raw'] === '#[') {
                $attr = $this->parseAttribute($i);
                if ($attr) {
                    $attrs[] = $attr;
                    // Skip past the attribute
                    $depth = 1;
                    for ($j = $i + 1; $j < count($this->tokens) && $depth > 0; $j++) {
                        if ($this->tokens[$j]['raw'] === '[') $depth++;
                        elseif ($this->tokens[$j]['raw'] === ']') $depth--;
                        $i = $j;
                    }
                    $i++;
                }
            }

            // Track current class
            if ($tok['kind'] === 'T_CLASS' && isset($this->tokens[$i + 2])) {
                $className = $this->tokens[$i + 2]['raw'];
            }

            // Property/variable name
            if ($tok['kind'] === 'T_VARIABLE' && !$propName) {
                $propName = ltrim($tok['raw'], '$');
            }

            // End of attribute block
            if ($tok['kind'] === 'T_PUBLIC' || $tok['kind'] === 'T_PRIVATE'
                || $tok['kind'] === 'T_PROTECTED' || $tok['kind'] === 'T_CONST') {
                // property declaration follows — we already have attrs
                if (isset($this->tokens[$i + 2]) && $this->tokens[$i + 2]['kind'] === 'T_VARIABLE') {
                    $propName = ltrim($this->tokens[$i + 2]['raw'], '$');
                }
                break;
            }

            // Stop at function definition or EOF
            if ($tok['kind'] === 'T_FUNCTION' || $tok['raw'] === ';') break;
        }

        if (empty($attrs) || !$propName) return null;

        $constraints = $this->symfonyConstraintsToFormula($attrs, $propName);
        if ($constraints === null) return null;

        $name = ($className ? $className . '_' : '') . $propName . '_valid';
        $formula = new QuantifierFormula('forall', 'x', Sort::Ref(), $constraints);

        return new ContractDecl(
            name: $name,
            post: $formula,
        );
    }

    private function parseAttribute(int &$pos): ?array
    {
        $idx = $pos;
        while ($idx < count($this->tokens) && $this->tokens[$idx]['raw'] !== '#[') $idx++;
        if ($idx >= count($this->tokens)) return null;

        $idx++; // skip #[
        // Skip whitespace
        while ($idx < count($this->tokens) && $this->tokens[$idx]['kind'] === 'T_WHITESPACE') $idx++;

        // Get namespace path (e.g., Assert\NotBlank or NotBlank)
        $name = '';
        while ($idx < count($this->tokens) && in_array($this->tokens[$idx]['kind'], ['T_STRING', 'T_NS_SEPARATOR'])) {
            $name .= $this->tokens[$idx]['raw'];
            $idx++;
        }

        // Skip the namespace prefix — we just want the leaf class name
        $parts = explode('\\', $name);
        $className = end($parts);

        // Parse arguments: (min: 2, max: 100)
        $args = [];
        if ($idx < count($this->tokens) && $this->tokens[$idx]['raw'] === '(') {
            $idx++;
            $depth = 1;
            $argStr = '';
            while ($idx < count($this->tokens) && $depth > 0) {
                $raw = $this->tokens[$idx]['raw'];
                if ($raw === '(') $depth++;
                elseif ($raw === ')') { $depth--; if ($depth === 0) break; }
                $argStr .= $raw;
                $idx++;
            }

            // Parse named args: "min: 2, max: 100"
            foreach (explode(',', $argStr) as $pair) {
                $pair = trim($pair);
                if (preg_match('/^(\w+)\s*:\s*(.+)$/', $pair, $m)) {
                    $val = trim($m[2], "'\" ");
                    $args[$m[1]] = is_numeric($val) ? (int)$val : $val;
                }
            }
        }

        return ['constraint' => $className, 'args' => $args];
    }

    private function symfonyConstraintsToFormula(array $attrs, string $propName): ?IrFormula
    {
        $operands = [];

        foreach ($attrs as $attr) {
            $c = $attr['constraint'];
            $args = $attr['args'];
            $field = new CtorTerm($propName, [new VarTerm('x')]);

            // Map known constraints
            $predicate = self::SYMFONY_CONSTRAINT_MAP[$c] ?? null;
            if ($predicate === 'not_null') {
                $operands[] = new AtomicFormula('not_null', [$field]);
            } elseif ($predicate !== null) {
                $operands[] = new AtomicFormula($predicate, [$field]);
            }

            // Handle special constraints with arguments
            switch ($c) {
                case 'Length':
                    if (isset($args['min'])) {
                        $operands[] = new AtomicFormula("\u{2265}", [
                            new CtorTerm('strlen', [$field]),
                            new ConstTerm((int)$args['min'], Sort::Int()),
                        ]);
                    }
                    if (isset($args['max'])) {
                        $operands[] = new AtomicFormula("\u{2264}", [
                            new CtorTerm('strlen', [$field]),
                            new ConstTerm((int)$args['max'], Sort::Int()),
                        ]);
                    }
                    if (isset($args['exact'])) {
                        $operands[] = new AtomicFormula('=', [
                            new CtorTerm('strlen', [$field]),
                            new ConstTerm((int)$args['exact'], Sort::Int()),
                        ]);
                    }
                    break;

                case 'Range':
                    if (isset($args['min'])) {
                        $operands[] = new AtomicFormula("\u{2265}", [
                            $field, new ConstTerm((int)$args['min'], Sort::Int()),
                        ]);
                    }
                    if (isset($args['max'])) {
                        $operands[] = new AtomicFormula("\u{2264}", [
                            $field, new ConstTerm((int)$args['max'], Sort::Int()),
                        ]);
                    }
                    break;

                case 'Regex':
                    if (isset($args['pattern'])) {
                        $operands[] = new AtomicFormula('matches', [
                            $field, new ConstTerm($args['pattern'], Sort::String()),
                        ]);
                    }
                    break;

                case 'Choice':
                    if (isset($args['choices']) && is_array($args['choices'])) {
                        $choices = array_map(fn($v) => new ConstTerm($v, Sort::String()), $args['choices']);
                        $operands[] = new AtomicFormula('member', [
                            $field,
                            new CtorTerm('Set', $choices),
                        ]);
                    }
                    break;
            }
        }

        if (empty($operands)) return null;
        if (count($operands) === 1) return $operands[0];
        return new ConnectiveFormula('and', $operands);
    }

    // ════════════════════════════════════════════
    // Adapter 3: PHPStan / Psalm docblock annotations
    // ════════════════════════════════════════════

    /** @param int<lo, hi> / array<int, string> / non-empty-string etc. */
    private function liftPhpstanFromDocblock(): ?ContractDecl
    {
        $doc = $this->tokens[$this->pos]['raw'];

        $decls = [];

        // Find @param type $name annotations
        if (preg_match_all('/@param\s+(\S+)\s+\$(\w+)/', $doc, $params, PREG_SET_ORDER)) {
            foreach ($params as $m) {
                $type = $m[1];
                $name = $m[2];
                $f = $this->phpstanTypeToFormula($type, $name);
                if ($f) {
                    $decls[] = new ContractDecl(
                        name: "param_{$name}",
                        post: new QuantifierFormula('forall', 'x', Sort::Ref(), $f),
                    );
                }
            }
        }

        // Find @return type annotations
        if (preg_match('/@return\s+(\S+)/', $doc, $m)) {
            $type = $m[1];
            $f = $this->phpstanTypeToFormula($type, 'out');
            if ($f) {
                $decls[] = new ContractDecl(
                    name: 'return_value',
                    post: new QuantifierFormula('forall', 'x', Sort::Ref(), $f),
                );
            }
        }

        return !empty($decls) ? $decls[0] : null; // Return first for now
    }

    private function phpstanTypeToFormula(string $type, string $varName): ?IrFormula
    {
        $field = new CtorTerm($varName, [new VarTerm('x')]);
        $operands = [];

        // int<lo, hi>
        if (preg_match('/^int<(\d+),\s*(\d+)>$/', $type, $m)) {
            $lo = (int)$m[1]; $hi = (int)$m[2];
            $operands[] = new AtomicFormula("\u{2265}", [$field, new ConstTerm($lo, Sort::Int())]);
            $operands[] = new AtomicFormula("\u{2264}", [$field, new ConstTerm($hi, Sort::Int())]);
        }

        // int >= lo
        if (preg_match('/^int\b.*\b>=(\d+)/', $type, $m)) {
            $operands[] = new AtomicFormula("\u{2265}", [$field, new ConstTerm((int)$m[1], Sort::Int())]);
        }

        // positive-int, negative-int
        if ($type === 'positive-int' || $type === 'positive_int') {
            $operands[] = new AtomicFormula("\u{2265}", [$field, new ConstTerm(1, Sort::Int())]);
        }
        if ($type === 'negative-int' || $type === 'negative_int') {
            $operands[] = new AtomicFormula("\u{2264}", [$field, new ConstTerm(-1, Sort::Int())]);
        }

        // non-negative-int
        if (str_contains($type, 'non-negative')) {
            $operands[] = new AtomicFormula("\u{2265}", [$field, new ConstTerm(0, Sort::Int())]);
        }

        // non-empty-string, non-empty-array
        if (str_starts_with($type, 'non-empty-')) {
            $operands[] = new AtomicFormula('not_null', [$field]);
            $operands[] = new AtomicFormula('>', [
                new CtorTerm('strlen', [$field]), new ConstTerm(0, Sort::Int()),
            ]);
        }

        // null, mixed, void, never
        if ($type === 'null' || $type === 'mixed') return null;

        if (empty($operands)) {
            // Bare scalar type: just add not_null
            $operands[] = new AtomicFormula('not_null', [$field]);
        }

        if (count($operands) === 1) return $operands[0];
        return new ConnectiveFormula('and', $operands);
    }

    // ════════════════════════════════════════════
    // Adapter 4: PHPUnit assertions
    // ════════════════════════════════════════════

    private function liftPhpUnitAssertion(): ?ContractDecl
    {
        $raw = $this->tokens[$this->pos]['raw'];
        $line = $this->tokens[$this->pos]['line'];

        $hasArrow = false;
        for ($i = max(0, $this->pos - 2); $i < $this->pos; $i++) {
            if (in_array($this->tokens[$i]['raw'] ?? '', ['->', '::'])) {
                $hasArrow = true; break;
            }
        }
        if (!$hasArrow) return null;

        $method = strtolower($raw);
        if (!str_starts_with($method, 'assert')) return null;

        $name = "phpunit_{$method}_L{$line}";

        return new ContractDecl(
            name: $name,
            inv: new AtomicFormula($method, [
                new CtorTerm('testFunc', [new VarTerm('x')]),
                new ConstTerm('expected', Sort::String()),
            ]),
        );
    }

    // ════════════════════════════════════════════
    // Adapter 5: Pest assertions
    // ════════════════════════════════════════════

    /** Pest: expect($x)->toBe(4) / expect($x)->toBeGreaterThan(0) */
    private function liftPestAssertion(): ?ContractDecl
    {
        $line = $this->tokens[$this->pos]['line'];
        $this->pos++; // skip 'expect'

        // Skip whitespace and opening paren
        while ($this->pos < count($this->tokens)
            && in_array($this->tokens[$this->pos]['kind'], ['T_WHITESPACE', 'CHAR'])
            && $this->tokens[$this->pos]['raw'] !== ')'
        ) {
            if ($this->tokens[$this->pos]['raw'] === '(') { $this->pos++; break; }
            $this->pos++;
        }

        // Read the subject expression
        $subject = '';
        $parenDepth = 1;
        while ($this->pos < count($this->tokens) && $parenDepth > 0) {
            $raw = $this->tokens[$this->pos]['raw'];
            if ($raw === '(') $parenDepth++;
            elseif ($raw === ')') { $parenDepth--; if ($parenDepth === 0) break; }
            $subject .= $raw;
            $this->pos++;
        }

        // Skip -> to chain
        while ($this->pos < count($this->tokens)
            && $this->tokens[$this->pos]['raw'] !== '->'
            && $this->tokens[$this->pos]['raw'] !== ')'
        ) {
            $this->pos++;
        }
        if ($this->tokens[$this->pos]['raw'] !== '->') return null;
        $this->pos++; // skip ->

        // Read the chain method
        while ($this->pos < count($this->tokens)
            && in_array($this->tokens[$this->pos]['kind'], ['T_WHITESPACE'])
        ) {
            $this->pos++;
        }

        $method = $this->tokens[$this->pos]['raw'];
        if (!in_array($method, ['toBe', 'toBeGreaterThan', 'toBeGreaterThanOrEqual',
                                'toBeLessThan', 'toBeLessThanOrEqual',
                                'toEqual', 'toBeTrue', 'toBeFalse',
                                'toBeNull', 'toBeEmpty'])) {
            return null;
        }

        // Map Pest method -> IR atomic name
        $pestMap = [
            'toBe' => '=',
            'toBeGreaterThan' => '>',
            'toBeGreaterThanOrEqual' => "\u{2265}",
            'toBeLessThan' => '<',
            'toBeLessThanOrEqual' => "\u{2264}",
            'toEqual' => '=',
            'toBeTrue' => '=',
            'toBeFalse' => '=',
            'toBeNull' => 'is_null',
            'toBeEmpty' => 'not_null',
        ];
        $atomicName = $pestMap[$method] ?? 'true';

        $name = "pest_{$method}_L{$line}";
        $subjectTerm = new ConstTerm(trim($subject), Sort::Ref());

        return new ContractDecl(
            name: $name,
            inv: new AtomicFormula($atomicName, [$subjectTerm]),
        );
    }

    // ════════════════════════════════════════════
    // Helpers
    // ════════════════════════════════════════════

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

    private function jsonToFormula(array $json): IrFormula
    {
        if (!isset($json['kind'])) return new AtomicFormula('true', []);

        switch ($json['kind']) {
            case 'atomic':
                $args = array_map(fn($a) => $this->jsonToTerm($a), $json['args'] ?? []);
                return new AtomicFormula($json['name'] ?? 'true', $args);
            case 'and':
            case 'or':
            case 'not':
            case 'implies':
                $ops = array_map(fn($o) => $this->jsonToFormula($o), $json['operands'] ?? []);
                return new ConnectiveFormula($json['kind'], $ops);
            case 'forall':
            case 'exists':
                $sort = new Sort($json['sort']['name'] ?? 'Ref');
                return new QuantifierFormula(
                    $json['kind'], $json['name'], $sort,
                    $this->jsonToFormula($json['body'])
                );
            default:
                return new AtomicFormula('true', []);
        }
    }

    private function jsonToTerm(array $json): IrTerm
    {
        return match ($json['kind'] ?? '') {
            'var' => new VarTerm($json['name']),
            'const' => new ConstTerm(
                $json['value'],
                new Sort($json['sort']['name'] ?? 'Ref')
            ),
            'ctor' => new CtorTerm(
                $json['name'],
                array_map(fn($a) => $this->jsonToTerm($a), $json['args'] ?? [])
            ),
            default => new VarTerm('_'),
        };
    }
}

// ════════════════════════════════════════════
// Lifter orchestrator
// ════════════════════════════════════════════

class PhpLifter
{
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
            if (str_contains($path, '/vendor/') || str_contains($path, '/cache/')) continue;

            $filesScanned++;
            try {
                $parser = new PhpFileParser();
                $result = $parser->parse($path);
                foreach ($result['decls'] as $d) $allDecls[] = $d;
            } catch (\Throwable $e) {
                $warnings[] = "{$path}: {$e->getMessage()}";
            }
        }

        return ['decls' => $allDecls, 'filesScanned' => $filesScanned, 'warnings' => $warnings];
    }

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
            $members[$minted['cid']] = $minted['canonicalBytes'];
            $contractCids[$decl->name] = $minted['cid'];
        }

        $built = $builder->build('php-lift', '0.1.0', $members, $producedAt);
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
                            'authoring_surfaces' => ['php', 'symfony-validator', 'phpstan', 'phpunit', 'pest'],
                            'ir_version' => 'v1.1.0',
                            'emits_signed_mementos' => true,
                        ],
                    ],
                ])),

                'lift' => (function () use ($id, $req) {
                    $ws = $req['params']['source_paths'][0]
                        ?? $req['params']['workspace_root']
                        ?? getcwd();

                    $result = PhpLifter::liftAndMint($ws, $ws);
                    send(json_encode([
                        'jsonrpc' => '2.0', 'id' => $id,
                        'result' => [
                            'kind' => 'proof-envelope',
                            'filename_cid' => $result['cid'],
                            'contract_set_cid' => $result['contractSetCid'],
                            'bytes_base64' => base64_encode($result['bytes']),
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

// Direct mode
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
