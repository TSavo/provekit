<?php
/** ProvekIt PHP LSP daemon — parses PHP source, extracts contracts + call edges.
 *  Mirrors the Go LSP (`provekit-lsp-go`).
 *  Methods: initialize, parse, shutdown.
 *  Response shape: declarations + callEdges.
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
                if ($funcName) {
                    $declarations[] = [
                        'kind' => 'contract',
                        'name' => $funcName,
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
                        'callSiteLocus' => ['file' => $path, 'line' => $i + 1, 'col' => 0],
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
                        'callSiteLocus' => ['file' => $path, 'line' => $i + 1, 'col' => 0],
                        'evidenceTerm' => ['kind' => 'atomic', 'name' => 'true', 'args' => []],
                    ];
                }
            }
        }

        return ['declarations' => $declarations, 'callEdges' => $callEdges];
    }

    /**
     * Walk function bodies for potential call expressions
     * (emits call edges for same-kit and cross-kit resolution).
     */
    public static function walkCallEdges(string $source, string $path, array $decls): array
    {
        $callEdges = [];

        foreach ($decls as $decl) {
            if (($decl['kind'] ?? '') !== 'contract') continue;
            $name = $decl['name'] ?? '';
            if (!$name) continue;

            // Find call expressions in the function body: `ClassName::method()`
            // For same-kit: `$this->otherMethod()`
            if (preg_match_all('/\$this->(\w+)\(/', $source, $matches)) {
                foreach ($matches[1] as $called) {
                    $callEdges[] = [
                        'kind' => 'call-edge',
                        'sourceContractCid' => 'pending-php:' . $name,
                        'targetContractCid' => null,
                        'targetSymbol' => 'php:' . $called,
                        'callSiteLocus' => ['file' => $path, 'line' => 0, 'col' => 0],
                        'evidenceTerm' => ['kind' => 'atomic', 'name' => 'true', 'args' => []],
                    ];
                }
            }

            // Cross-kit: `C\function()` → cgo-like resolution
            if (preg_match_all('/(?:new\s+)?(\w+)\s*\(/', $source, $matches)) {
                foreach ($matches[1] as $called) {
                    if (in_array(strtolower($called), ['if', 'for', 'while', 'function', 'return', 'echo', 'new', 'array', 'list'])) continue;
                    $callEdges[] = [
                        'kind' => 'call-edge',
                        'sourceContractCid' => 'pending-php:' . $name,
                        'targetContractCid' => null,
                        'targetSymbol' => 'php:' . $called,
                        'callSiteLocus' => ['file' => $path, 'line' => 0, 'col' => 0],
                        'evidenceTerm' => ['kind' => 'atomic', 'name' => 'true', 'args' => []],
                    ];
                }
            }
        }

        return $callEdges;
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
                'result' => ['name' => 'provekit-lsp-php', 'version' => '1.0.0', 'capabilities' => []],
            ])),

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
                $diagnostics = array_map(
                    fn(LspDiagnostic $diagnostic): array => $diagnostic->toArray(),
                    ForwardPropagator::floorV1SeedIndex()->emitDiagnostics(
                        ForwardPropagator::lowerFloorSource($source)
                    ),
                );

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
