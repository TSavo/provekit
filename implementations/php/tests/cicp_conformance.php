<?php

declare(strict_types=1);

require_once __DIR__ . '/../provekit-ir-symbolic/src/Canonicalizer/Blake3.php';
require_once __DIR__ . '/../provekit-ir-symbolic/src/Canonicalizer/Jcs.php';

use ProvekIt\Canonicalizer\Blake3;
use ProvekIt\Canonicalizer\Jcs;

$vectorDir = dirname(__DIR__, 3) . '/protocol/conformance/cicp';
$catalog = readJson($vectorDir . '/vectors.json');

foreach ($catalog['vectors'] as $vector) {
    $body = readJson($vectorDir . '/' . $vector['body']);

    if ($vector['shouldPass'] === true) {
        $actual = Blake3::cid(Jcs::encode($body));
        assertSame($vector['expectedCid'], $actual, $vector['name'] . ' CID');
        continue;
    }

    $error = null;
    try {
        assertNoMissingBlastRadiusInputCids($body);
    } catch (Throwable $e) {
        $error = $e->getMessage();
    }

    assertTrue($error !== null, $vector['name'] . ' failed closed');
    assertTrue(
        str_contains($error, $vector['errorContains']),
        $vector['name'] . ' error contains "' . $vector['errorContains'] . '"; got "' . $error . '"'
    );
}

echo "CICP conformance vectors passed\n";

function readJson(string $path): array
{
    $data = json_decode(file_get_contents($path), true, 512, JSON_THROW_ON_ERROR);
    if (!is_array($data)) {
        throw new RuntimeException($path . ' did not decode to a JSON object');
    }
    return $data;
}

function assertSame(string $expected, string $actual, string $label): void
{
    if ($actual !== $expected) {
        throw new RuntimeException($label . " mismatch\n  got:  " . $actual . "\n  want: " . $expected);
    }
}

function assertNoMissingBlastRadiusInputCids(array $body): void
{
    if (($body['kind'] ?? null) !== 'CIBlastRadius') {
        return;
    }

    $inputCids = stringSetField($body, 'inputCids');
    $requiredCids = array_merge(
        stringFields($body, [
            'protocolCatalogCid',
            'jobDefinitionCid',
            'commandCid',
            'runnerIdentityCid',
            'sourceClosureCid',
            'policyCid',
        ]),
        stringListFields($body, [
            'toolchainCids',
            'lockfileCids',
            'generatedInputCids',
            'fixtureCids',
            'relevantSpecCids',
        ])
    );

    sort($requiredCids, SORT_STRING);
    foreach ($requiredCids as $cid) {
        if (!isset($inputCids[$cid])) {
            throw new RuntimeException('inputCids missing required CID ' . $cid);
        }
    }
}

function stringSetField(array $body, string $field): array
{
    if (!array_key_exists($field, $body) || !is_array($body[$field]) || !array_is_list($body[$field])) {
        throw new RuntimeException($field . ' must be an array');
    }

    $set = [];
    foreach ($body[$field] as $index => $value) {
        if (!is_string($value)) {
            throw new RuntimeException($field . '[' . $index . '] must be a string');
        }
        $set[$value] = true;
    }
    return $set;
}

function stringFields(array $body, array $fields): array
{
    $values = [];
    foreach ($fields as $field) {
        if (isset($body[$field]) && is_string($body[$field])) {
            $values[] = $body[$field];
        }
    }
    return $values;
}

function stringListFields(array $body, array $fields): array
{
    $values = [];
    foreach ($fields as $field) {
        if (!isset($body[$field]) || !is_array($body[$field])) {
            continue;
        }
        foreach ($body[$field] as $value) {
            if (is_string($value)) {
                $values[] = $value;
            }
        }
    }
    return $values;
}

function assertTrue(bool $condition, string $label): void
{
    if (!$condition) {
        throw new RuntimeException('assertion failed: ' . $label);
    }
}
