<?php
/** ProvekIt PHP LSP daemon: parses PHP source, extracts contracts + call edges.
 *  Mirrors the Go LSP (`provekit-lsp-go`).
 *  Protocol: provekit-lsp-shared/1 over stdio.
 *  Methods: initialize, analyzeDocument, lift, parse (legacy), shutdown.
 */

declare(strict_types=1);

require_once __DIR__ . '/../../provekit-ir-symbolic/src/Ir/Term.php';
require_once __DIR__ . '/../../provekit-ir-symbolic/src/Ir/Formula.php';
require_once __DIR__ . '/../../provekit-ir-symbolic/src/Ir/Declaration.php';
require_once __DIR__ . '/../../provekit-ir-symbolic/src/Canonicalizer/Blake3.php';
require_once __DIR__ . '/../../provekit-ir-symbolic/src/Canonicalizer/Jcs.php';
require_once __DIR__ . '/ForwardPropagator.php';

use ProvekIt\Ir\{ContractDecl, BridgeDecl, Collector};
use ProvekIt\Canonicalizer\{Blake3, Jcs};

const PROVEKIT_LSP_KIT_ID = 'php';
const PROVEKIT_LSP_SHARED_PROTOCOL_VERSION = 'provekit-lsp-shared/1';

// ════════════════════════════════════════════
// Annotation scanner (// @provekit-contract, // @provekit-implement)
// ════════════════════════════════════════════

class AnnotationScanner
{
    /**
     * Scan PHP source for line-comment annotations.
     * @return array{declarations: array, callEdges: array}
     */
    public static function scan(string $source, string $path): array
    {
        $lines = explode("\n", $source);
        $declarations = [];
        $callEdges = [];

        $funcName = null;
        for ($i = 0; $i < count($lines); $i++) {
            $trimmed = trim($lines[$i]);

            // Detect function definitions
            if (preg_match('/\bfunction\s+(\w+)\b/', $trimmed, $m)) {
                $funcName = $m[1];
            }

            // @provekit-contract
            if (preg_match('#^//\s*@provekit-contract#', $trimmed)) {
                // Look ahead up to 10 lines for the function definition.
                $target = $funcName;
                for ($j = $i + 1; $j < min($i + 10, count($lines)); $j++) {
                    if (preg_match('/\bfunction\s+(\w+)\b/', $lines[$j], $fm)) {
                        $target = $fm[1];
                        break;
                    }
                }
                if ($target) {
                    $declarations[] = [
                        'kind' => 'contract',
                        'name' => $target,
                        'outBinding' => 'out',
                        'post' => ['kind' => 'atomic', 'name' => 'true', 'args' => []],
                    ];
                }
            }

            // @provekit-implement <cid>
            if (preg_match('#^//\s*@provekit-implement\s+(\S+)#', $trimmed, $m)) {
                $cid = $m[1];
                if ($funcName) {
                    $declarations[] = [
                        'kind' => 'bridge',
                        'name' => $funcName,
                        'sourceSymbol' => $funcName,
                        'sourceLayer' => 'php',
                        'sourceContractCid' => 'pending-php:' . $funcName,
                        'targetContractCid' => $cid,
                        'targetProofCid' => $cid,
                        'targetLayer' => $cid[0] === 'b' ? 'rust' : 'openapi',
                    ];

                    // Also emit a call edge for this bridge
                    $callEdges[] = [
                        'kind' => 'call-edge',
                        'sourceContractCid' => 'pending-php:' . $funcName,
                        'targetContractCid' => null,
                        'targetSymbol' => $cid,
                        'callSiteLocus' => ['file' => $path, 'line' => $i + 1, 'column' => 0],
                        'evidenceTerm' => ['kind' => 'atomic', 'name' => 'true', 'args' => []],
                    ];
                }
            }

            // @provekit.target <kit>:<name>
            if (preg_match('#^//\s*@provekit\.target\s+(\S+)#', $trimmed, $m)) {
                $target = $m[1];
                if ($funcName) {
                    $callEdges[] = [
                        'kind' => 'call-edge',
                        'sourceContractCid' => 'pending-php:' . $funcName,
                        'targetContractCid' => null,
                        'targetSymbol' => $target,
                        'callSiteLocus' => ['file' => $path, 'line' => $i + 1, 'column' => 0],
                        'evidenceTerm' => ['kind' => 'atomic', 'name' => 'true', 'args' => []],
                    ];
                }
            }
        }

        return ['declarations' => $declarations, 'callEdges' => $callEdges];
    }

    /**
     * Walk function bodies for same-kit PHP call expressions.
     */
    public static function walkCallEdges(string $source, string $path, array $decls): array
    {
        $declaredNames = [];
        foreach ($decls as $decl) {
            if (($decl['kind'] ?? '') === 'contract' && !empty($decl['name'])) {
                $declaredNames[$decl['name']] = true;
            }
        }
        if ($declaredNames === []) {
            return [];
        }

        $tokens = token_get_all($source);
        $callEdges = [];
        $seen = [];
        $line = 1;
        $column = 0;
        $braceDepth = 0;
        $pendingFunction = null;
        $functionStack = [];
        $declarationNameIndexes = [];

        for ($i = 0; $i < count($tokens); $i++) {
            $token = $tokens[$i];
            $text = self::tokenText($token);
            $startLine = $line;
            $startColumn = $column;

            if (is_array($token) && $token[0] === T_FUNCTION) {
                $nameIndex = self::nextNamedFunctionIndex($tokens, $i + 1);
                if ($nameIndex !== null) {
                    $pendingFunction = self::tokenText($tokens[$nameIndex]);
                    $declarationNameIndexes[$nameIndex] = true;
                }
            } elseif ($text === '{') {
                $braceDepth++;
                if ($pendingFunction !== null) {
                    $functionStack[] = ['name' => $pendingFunction, 'braceDepth' => $braceDepth];
                    $pendingFunction = null;
                }
            } elseif ($text === '}') {
                while ($functionStack !== [] && end($functionStack)['braceDepth'] === $braceDepth) {
                    array_pop($functionStack);
                }
                $braceDepth = max(0, $braceDepth - 1);
            } elseif (
                is_array($token)
                && $token[0] === T_STRING
                && !isset($declarationNameIndexes[$i])
                && $functionStack !== []
            ) {
                $callee = $text;
                $nextIndex = self::nextNonTriviaIndex($tokens, $i + 1);
                if (isset($declaredNames[$callee]) && $nextIndex !== null && self::tokenText($tokens[$nextIndex]) === '(') {
                    $caller = end($functionStack)['name'];
                    $edge = [
                        'kind' => 'call-edge',
                        'sourceContractCid' => 'pending-php:' . $caller,
                        'targetContractCid' => null,
                        'targetSymbol' => 'php-kit:' . $callee,
                        'callSiteLocus' => ['file' => $path, 'line' => $startLine, 'column' => $startColumn],
                        'evidenceTerm' => ['kind' => 'atomic', 'name' => 'true', 'args' => []],
                    ];
                    $key = $edge['sourceContractCid'] . '|' . $edge['targetSymbol'] . '|' . $startLine . '|' . $startColumn;
                    if (!isset($seen[$key])) {
                        $callEdges[] = $edge;
                        $seen[$key] = true;
                    }
                }
            }

            self::advancePosition($text, $line, $column);
        }

        return $callEdges;
    }

    private static function tokenText(mixed $token): string
    {
        return is_array($token) ? $token[1] : $token;
    }

    private static function nextNamedFunctionIndex(array $tokens, int $start): ?int
    {
        for ($i = $start; $i < count($tokens); $i++) {
            $token = $tokens[$i];
            if (self::isTrivia($token) || self::tokenText($token) === '&') {
                continue;
            }
            return is_array($token) && $token[0] === T_STRING ? $i : null;
        }
        return null;
    }

    private static function nextNonTriviaIndex(array $tokens, int $start): ?int
    {
        for ($i = $start; $i < count($tokens); $i++) {
            if (!self::isTrivia($tokens[$i])) {
                return $i;
            }
        }
        return null;
    }

    private static function isTrivia(mixed $token): bool
    {
        return is_array($token) && in_array($token[0], [T_WHITESPACE, T_COMMENT, T_DOC_COMMENT], true);
    }

    private static function advancePosition(string $text, int &$line, int &$column): void
    {
        for ($i = 0; $i < strlen($text); $i++) {
            if ($text[$i] === "\n") {
                $line++;
                $column = 0;
            } else {
                $column++;
            }
        }
    }
}

// ════════════════════════════════════════════
// RPC Mode
// ════════════════════════════════════════════

$stdin = fopen('php://stdin', 'r');
while (($line = fgets($stdin)) !== false) {
    $line = trim($line);
    if ($line === '') continue;
    $req = json_decode($line, true);
    if (!is_array($req) || !isset($req['id'], $req['method'])) continue;

    $id = $req['id'];
    $method = $req['method'];
    $params = $req['params'] ?? [];

    try {
        match ($method) {
            'initialize' => send(json_encode([
                'jsonrpc' => '2.0', 'id' => $id,
                'result' => [
                    'name'             => 'provekit-lsp-php',
                    'version'          => '1.0.0',
                    'protocol_version' => PROVEKIT_LSP_SHARED_PROTOCOL_VERSION,
                    'kit_id'           => PROVEKIT_LSP_KIT_ID,
                    'protocol_catalog_cid' => PROVEKIT_LSP_PROTOCOL_CATALOG_CID,
                    'capabilities'     => [
                        'source_surfaces' => ['php-source'],
                        'entry_kinds' => ['bind-lift-entry', 'call-edge'],
                        'diagnostic_codes' => [
                            'provekit.lsp.parse_error',
                            'provekit.lsp.lift_gap',
                            'provekit.lsp.implication_failed',
                        ],
                        'status_kinds' => ['materialize', 'emit', 'check', 'prove'],
                    ],
                ],
            ], JSON_UNESCAPED_SLASHES)),

            'analyzeDocument' => (function () use ($id, $params) {
                $kitId = $params['kit_id'] ?? PROVEKIT_LSP_KIT_ID;
                if ($kitId !== PROVEKIT_LSP_KIT_ID) {
                    send(json_encode([
                        'jsonrpc' => '2.0', 'id' => $id,
                        'error' => ['code' => -32602, 'message' => "provekit-lsp-php only handles kit_id 'php', got '{$kitId}'"],
                    ], JSON_UNESCAPED_SLASHES));
                    return;
                }

                $path = $params['file'] ?? $params['path'] ?? 'source.php';
                $uri = $params['uri'] ?? ('file://' . $path);
                $source = $params['text'] ?? $params['source'] ?? '';
                $analysis = analyzeDocumentSource($source, $path);

                send(json_encode([
                    'jsonrpc' => '2.0', 'id' => $id,
                    'result' => [
                        'kind' => 'lsp-document-analysis',
                        'schema_version' => '1',
                        'kit_id' => PROVEKIT_LSP_KIT_ID,
                        'uri' => $uri,
                        'file' => $path,
                        'document_cid' => Blake3::cid($source),
                        'protocol_catalog_cid' => PROVEKIT_LSP_PROTOCOL_CATALOG_CID,
                        'entries' => $analysis['entries'],
                        'diagnostics' => $analysis['diagnostics'],
                        'statuses' => [],
                        'project' => null,
                    ],
                ], JSON_UNESCAPED_SLASHES));
            })(),

            'lift' => (function () use ($id, $params) {
                $workspaceRoot = $params['workspace_root'] ?? '.';
                $sourcePaths   = $params['source_paths'] ?? [];

                if (!is_array($sourcePaths) || empty($sourcePaths)) {
                    send(json_encode([
                        'jsonrpc' => '2.0', 'id' => $id,
                        'error' => ['code' => -32602, 'message' => 'lift: source_paths must be a non-empty array'],
                    ]));
                    return;
                }

                $ir          = [];
                $diagnostics = [];

                foreach ($sourcePaths as $sp) {
                    $fullPath = $sp && $sp[0] === '/'
                        ? $sp
                        : rtrim($workspaceRoot, '/') . '/' . $sp;
                    if (!is_file($fullPath) || !str_ends_with($fullPath, '.php')) {
                        continue;
                    }
                    $source = @file_get_contents($fullPath);
                    if ($source === false) {
                        $diagnostics[] = ['kind' => 'read-error', 'path' => $fullPath];
                        continue;
                    }
                    $scanned = AnnotationScanner::scan($source, $fullPath);
                    foreach ($scanned['declarations'] as $decl) {
                        $ir[] = $decl;
                    }
                    foreach (forwardDiagnosticsForSource($source) as $diagnostic) {
                        $diagnostics[] = $diagnostic;
                    }
                }

                send(json_encode([
                    'jsonrpc' => '2.0', 'id' => $id,
                    'result' => [
                        'kind'          => 'ir-document',
                        'ir'            => $ir,
                        'callEdges'     => [],
                        'diagnostics'   => $diagnostics,
                        'opacityReport' => [],
                        'refusals'      => [],
                    ],
                ]));
            })(),

            'parse' => (function () use ($id, $params) {
                $path = $params['path'] ?? '';
                $source = $params['source'] ?? '';

                if (!$source) {
                    send(json_encode([
                        'jsonrpc' => '2.0', 'id' => $id,
                        'error' => ['code' => -32602, 'message' => 'source is required'],
                    ]));
                    return;
                }

                $scanned = AnnotationScanner::scan($source, $path);
                $callEdges = AnnotationScanner::walkCallEdges($source, $path, $scanned['declarations']);

                // Merge manual call edges with auto-detected ones
                $allCallEdges = array_merge($scanned['callEdges'], $callEdges);
                $diagnostics = forwardDiagnosticsForSource($source);

                send(json_encode([
                    'jsonrpc' => '2.0', 'id' => $id,
                    'result' => [
                        'declarations' => $scanned['declarations'],
                        'callEdges' => $allCallEdges,
                        'diagnostics' => $diagnostics,
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

function send(string $json): void { echo $json . "\n"; flush(); }

/**
 * @return array{entries: array<int, array<string, mixed>>, diagnostics: array<int, array<string, mixed>>}
 */
function analyzeDocumentSource(string $source, string $path): array
{
    $scanned = AnnotationScanner::scan($source, $path);
    $callEdges = array_merge(
        $scanned['callEdges'],
        AnnotationScanner::walkCallEdges($source, $path, $scanned['declarations']),
    );
    $diagnostics = array_map(
        fn(array $diagnostic): array => sharedForwardDiagnostic($diagnostic),
        forwardDiagnosticsForSource($source),
    );

    return [
        'entries' => analysisEntries($scanned['declarations'], $callEdges, $source),
        'diagnostics' => $diagnostics,
    ];
}

/**
 * @param array<int, array<string, mixed>> $declarations
 * @param array<int, array<string, mixed>> $callEdges
 * @return array<int, array<string, mixed>>
 */
function analysisEntries(array $declarations, array $callEdges, string $source): array
{
    $range = wholeDocumentRange($source);
    $entries = [];
    foreach ($declarations as $declaration) {
        $entries[] = [
            'kind' => 'bind-lift-entry',
            'entry' => $declaration,
            'range' => $range,
        ];
    }
    foreach ($callEdges as $edge) {
        $entries[] = [
            'kind' => 'call-edge',
            'entry' => $edge,
            'range' => callEdgeRange($edge),
        ];
    }
    return $entries;
}

/**
 * @return array{start_line: int, start_col: int, end_line: int, end_col: int}
 */
function wholeDocumentRange(string $source): array
{
    $line = 1;
    $column = 0;
    $length = strlen($source);
    for ($i = 0; $i < $length; $i++) {
        if ($source[$i] === "\n") {
            $line++;
            $column = 0;
        } else {
            $column++;
        }
    }
    return ['start_line' => 1, 'start_col' => 0, 'end_line' => $line, 'end_col' => $column];
}

/**
 * @param array<string, mixed> $edge
 * @return array{start_line: int, start_col: int, end_line: int, end_col: int}
 */
function callEdgeRange(array $edge): array
{
    $locus = is_array($edge['callSiteLocus'] ?? null) ? $edge['callSiteLocus'] : [];
    $target = preg_replace('/^php-kit:/', '', (string)($edge['targetSymbol'] ?? ''));
    $width = max(1, strlen(is_string($target) ? $target : ''));
    $line = (int)($locus['line'] ?? 1);
    $column = (int)($locus['column'] ?? $locus['col'] ?? 0);
    return ['start_line' => $line, 'start_col' => $column, 'end_line' => $line, 'end_col' => $column + $width];
}

/**
 * @param array<string, mixed> $diagnostic
 * @return array<string, mixed>
 */
function sharedForwardDiagnostic(array $diagnostic): array
{
    return [
        'code' => (string)($diagnostic['code'] ?? PROVEKIT_LSP_IMPLICATION_FAILED_CODE),
        'data' => $diagnostic['data'] ?? [],
        'kit_id' => PROVEKIT_LSP_KIT_ID,
        'message' => (string)($diagnostic['message'] ?? 'callee precondition not established at this callsite'),
        'producer' => 'forward-propagation',
        'protocol_catalog_cid' => PROVEKIT_LSP_PROTOCOL_CATALOG_CID,
        'range' => sharedRangeFromLsp($diagnostic['range'] ?? []),
        'severity' => 'error',
    ];
}

/**
 * @param mixed $range
 * @return array{start_line: int, start_col: int, end_line: int, end_col: int}
 */
function sharedRangeFromLsp(mixed $range): array
{
    $start = is_array($range) && is_array($range['start'] ?? null) ? $range['start'] : [];
    $end = is_array($range) && is_array($range['end'] ?? null) ? $range['end'] : [];
    return [
        'start_line' => ((int)($start['line'] ?? 0)) + 1,
        'start_col' => (int)($start['character'] ?? 0),
        'end_line' => ((int)($end['line'] ?? $start['line'] ?? 0)) + 1,
        'end_col' => (int)($end['character'] ?? (($start['character'] ?? 0) + 1)),
    ];
}

/**
 * @return array<int, array<string, mixed>>
 */
function forwardDiagnosticsForSource(string $source): array
{
    return array_map(
        fn(LspDiagnostic $diagnostic): array => $diagnostic->toArray(),
        ForwardPropagator::floorV1SeedIndex()->emitDiagnostics(
            ForwardPropagator::lowerFloorSource($source)
        ),
    );
}
