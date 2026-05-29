<?php

declare(strict_types=1);

namespace ProvekIt\LiftPhpSource;

function initialize_result(): array
{
    return [
        'name' => 'provekit-lift-php-source',
        'version' => VERSION,
        'protocol_version' => 'pep/1.7.0',
        'dialect' => DIALECT,
        'capabilities' => [
            'authoring_surfaces' => [DIALECT],
            'ir_version' => 'v1.1.0',
            'emits_signed_mementos' => false,
        ],
    ];
}

function run_rpc(): void
{
    $stdin = fopen('php://stdin', 'r');
    while (($line = fgets($stdin)) !== false) {
        $line = trim($line);
        if ($line === '') {
            continue;
        }
        try {
            $request = json_decode($line, true, 512, JSON_THROW_ON_ERROR);
            if (!is_array($request)) {
                write_rpc(error_response(null, -32600, 'INVALID_REQUEST'));
                continue;
            }
            write_rpc(dispatch_rpc($request));
        } catch (\JsonException $e) {
            write_rpc(error_response(null, -32700, 'PARSE_ERROR: ' . $e->getMessage()));
        } catch (\Throwable $e) {
            write_rpc(error_response($request['id'] ?? null, -32603, $e->getMessage()));
        }
    }
}

function dispatch_rpc(array $request): array
{
    $id = $request['id'] ?? null;
    $method = $request['method'] ?? '';
    $params = is_array($request['params'] ?? null) ? $request['params'] : [];
    return match ($method) {
        'initialize' => success_response($id, initialize_result()),
        'lift' => lift_rpc($id, $params),
        'compile' => compile_rpc($id, $params),
        'provekit.plugin.recognize' => recognize_rpc($id, $params),
        'shutdown' => success_response($id, null),
        default => error_response($id, -32601, 'METHOD_NOT_FOUND: ' . $method),
    };
}

function lift_rpc(mixed $id, array $params): array
{
    $surface = is_string($params['surface'] ?? null) ? $params['surface'] : DIALECT;
    if ($surface !== DIALECT) {
        return error_response($id, 1003, 'SURFACE_NOT_SUPPORTED: ' . $surface);
    }
    $sourcePaths = array_values(array_filter(
        is_array($params['source_paths'] ?? null) ? $params['source_paths'] : [],
        static fn(mixed $path): bool => is_string($path)
    ));
    if ($sourcePaths === []) {
        return error_response($id, -32602, 'source_paths must be a non-empty array of strings');
    }
    $workspaceRoot = is_string($params['workspace_root'] ?? null) ? $params['workspace_root'] : '.';
    $result = (new PhpSourceLifter())->liftPaths($workspaceRoot, $sourcePaths);
    return success_response($id, [
        'kind' => 'ir-document',
        'ir' => $result['ir'],
        'callEdges' => $result['callEdges'],
        'diagnostics' => $result['diagnostics'],
        'opacityReport' => $result['opacityReport'],
        'refusals' => $result['refusals'],
    ]);
}

function compile_rpc(mixed $id, array $params): array
{
    $ir = $params['ir'] ?? null;
    if (!is_array($ir)) {
        return error_response($id, -32602, 'ir must be an array of function-contract mementos');
    }
    return success_response($id, ['kind' => 'compiled-formula', 'body' => (new PhpSourceCompiler())->compileIrDocument($ir)]);
}

function recognize_rpc(mixed $id, array $params): array
{
    $projectRoot = $params['project_root'] ?? null;
    if (!is_string($projectRoot) || $projectRoot === '') {
        return error_response($id, -32602, 'project_root must be a non-empty string');
    }
    $sourcePaths = array_values(array_filter(
        is_array($params['source_paths'] ?? null) ? $params['source_paths'] : [],
        static fn(mixed $path): bool => is_string($path)
    ));
    $bindingTemplates = array_values(array_filter(
        is_array($params['binding_templates'] ?? null) ? $params['binding_templates'] : [],
        static fn(mixed $binding): bool => is_array($binding)
    ));

    try {
        return success_response($id, (new PhpSourceRecognizer())->recognizePaths($projectRoot, $sourcePaths, $bindingTemplates));
    } catch (\InvalidArgumentException $e) {
        return error_response($id, -32602, $e->getMessage());
    }
}

function success_response(mixed $id, mixed $result): array
{
    return ['jsonrpc' => '2.0', 'id' => $id, 'result' => $result];
}

function error_response(mixed $id, int $code, string $message): array
{
    return ['jsonrpc' => '2.0', 'id' => $id, 'error' => ['code' => $code, 'message' => $message]];
}

function write_rpc(array $response): void
{
    fwrite(STDOUT, json_encode($response, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE) . "\n");
}
