<?php
/** ProvekIt PHP self-contracts: JSON-RPC 2.0 NDJSON handler.
 *  Daemon lifecycle: initialize -> (lift)* -> shutdown.
 *  Persists across lift calls, exits on EOF or shutdown method.
 */

declare(strict_types=1);

require_once __DIR__ . '/main.php';

// Only activate in --rpc mode
if (!in_array('--rpc', $GLOBALS['argv'] ?? [])) return;

function runRpc(): void
{
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
                        'name' => 'php-self-contracts',
                        'version' => '1.0.0',
                        'protocol_version' => 'provekit-lift/1',
                        'capabilities' => [
                            'authoring_surfaces' => ['php-self-contracts'],
                            'ir_version' => 'v1.1.0',
                            'emits_signed_mementos' => true,
                        ],
                    ],
                ])),

                'lift' => (function () use ($id, $params) {
                    $ws = $params['workspace_root']
                        ?? $params['source_paths'][0]
                        ?? dirname(__DIR__, 2);
                    $result = mintAll($ws);

                    $bytes = file_get_contents($result['proofPath']);
                    $b64 = base64_encode($bytes);

                    send(json_encode([
                        'jsonrpc' => '2.0', 'id' => $id,
                        'result' => [
                            'kind' => 'proof-envelope',
                            'filename_cid' => $result['cid'],
                            'contract_set_cid' => $result['contractSetCid'],
                            'bytes_base64' => $b64,
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

function send(string $json): void { echo $json . "\n"; flush(); }

runRpc();
