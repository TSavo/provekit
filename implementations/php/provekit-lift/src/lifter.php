<?php
/** ProvekIt PHP Lifter — parses PHP source, lifts contracts, speaks RPC.
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

use ProvekIt\Ir\{Collector, ContractDecl};
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
                // Skip for now — complex parse
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
            // Target only — create placeholder contract
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
                $sort = new \ProvekIt\Ir\Sort($json['sort']['name'] ?? 'Ref');
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
                new \ProvekIt\Ir\Sort($json['sort']['name'] ?? 'Ref')
            ),
            'ctor' => new \ProvekIt\Ir\CtorTerm(
                $json['name'],
                array_map(fn($a) => $this->jsonToTerm($a), $json['args'] ?? [])
            ),
            default => new \ProvekIt\Ir\VarTerm('_'),
        };
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
            $members[$minted['cid']] = $minted['canonicalBytes'];
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
                    $ws = $req['params']['source_paths'][0]
                        ?? $req['params']['workspace_root']
                        ?? getcwd();

                    $outDir = $ws;
                    $result = PhpLifter::liftAndMint($ws, $outDir);

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
