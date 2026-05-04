<?php
/** ProvekIt PHP self-contracts — JSON-RPC 2.0 NDJSON handler. */

declare(strict_types=1);

require_once __DIR__ . '/main.php';

// Only activate in --rpc mode
if (!in_array('--rpc', $GLOBALS['argv'] ?? [])) return;

function runRpc(): void
{
    $stdin = fopen('php://stdin', 'r');
    stream_set_blocking($stdin, false);

    while (($line = fgets($stdin)) !== false) {
        $line = trim($line);
        if ($line === '') continue;

        $req = json_decode($line, true);
        if (!is_array($req) || !isset($req['id']) || !isset($req['method'])) continue;

        $id = $req['id'];
        $method = $req['method'];
        $params = $req['params'] ?? [];

        try {
            match ($method) {
                'initialize' => sendRpcResponse($id, [
                    'name' => 'provekit-lift-php',
                    'version' => '1.0.0',
                    'protocol_version' => 'provekit-lift/1',
                    'capabilities' => [
                        'authoring_surfaces' => ['php-self-contracts'],
                        'ir_version' => 'v1.1.0',
                        'emits_signed_mementos' => true,
                    ],
                ]),

                'lift' => (function () use ($id, $params) {
                    $workspace = $params['workspace_root'] ?? ($params['source_paths'][0] ?? __DIR__ . '/../../');
                    $outDir = $workspace;
                    $result = mintAll($outDir);

                    $bytes = file_get_contents($result['proofPath']);
                    $b64 = base64_encode($bytes);

                    $contractCids = array_values(array_filter($result['contractCids'] ?? [], fn($c) => str_starts_with($c, 'blake3-512:')));
                    sort($contractCids);
                    $contractSetCid = \ProvekIt\Canonicalizer\Blake3::cid(
                        \ProvekIt\Canonicalizer\Jcs::encode($contractCids)
                    );

                    sendRpcResponse($id, [
                        'kind' => 'proof-envelope',
                        'filename_cid' => $result['cid'],
                        'bytes_base64' => $b64,
                        'contract_set_cid' => $contractSetCid,
                        'contract_count' => $result['contractCount'],
                    ]);
                })(),

                'shutdown' => (function () use ($id) {
                    sendRpcResponse($id, null);
                    exit(0);
                })(),

                default => sendRpcError($id, -32601, "METHOD_NOT_FOUND: {$method}"),
            };
        } catch (\Throwable $e) {
            sendRpcError($id, -32603, $e->getMessage());
        }
    }
}

function sendRpcResponse($id, $result): void
{
    echo json_encode([
        'jsonrpc' => '2.0',
        'id' => $id,
        'result' => $result,
    ], JSON_UNESCAPED_SLASHES) . "\n";
    flush();
}

function sendRpcError($id, int $code, string $message): void
{
    echo json_encode([
        'jsonrpc' => '2.0',
        'id' => $id,
        'error' => ['code' => $code, 'message' => $message],
    ], JSON_UNESCAPED_SLASHES) . "\n";
    flush();
}

runRpc();
