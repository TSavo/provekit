<?php
/** ProvekIt PHP self-contracts: Main mint orchestrator.
 *  Side A pattern: walks the canonical PHP slab, mints each contract
 *  as a signed layered memento, bundles into a .proof envelope.
 *  Mirrors ruby/lib/provekit/self_contracts.rb.
 */

declare(strict_types=1);

$baseDir = dirname(__DIR__, 2);
$irDir = $baseDir . '/../provekit-ir-symbolic/src';
require_once $irDir . '/Ir/Term.php';
require_once $irDir . '/Ir/Term.php';
require_once $irDir . '/Ir/Formula.php';
require_once $irDir . '/Ir/Declaration.php';
require_once $irDir . '/Canonicalizer/Blake3.php';
require_once $irDir . '/Canonicalizer/Jcs.php';
require_once $irDir . '/Canonicalizer/Ed25519.php';
require_once $irDir . '/ClaimEnvelope/Minter.php';
require_once $irDir . '/ProofEnvelope/Builder.php';
require_once __DIR__ . '/../../slabs/slabs.php';

use ProvekIt\Ir\{Collector, ContractDecl};
use ProvekIt\Canonicalizer\{Blake3, Jcs, Ed25519};
use ProvekIt\ClaimEnvelope\Minter;
use ProvekIt\ProofEnvelope\Builder;

const PRODUCED_BY  = 'provekit-php-self-contracts@1.0';
const DECLARED_AT  = '2026-05-03T18:00:00Z';
const CATALOG_NAME = 'php-self-contracts';
const CATALOG_VERSION = '1.0.0';

function mintAll(string $outDir): array
{
    $signer = Ed25519::foundation();
    $minter = new Minter($signer);
    $builder = new Builder($signer);

    $members = [];
    $contractCids = [];

    foreach (\ProvekIt\SelfContracts\Slabs() as $slab) {
        Collector::reset();
        ($slab->run)();
        $finished = Collector::finish();

        foreach ($finished['contracts'] as $contract) {
            $minted = $minter->mintContract($contract, PRODUCED_BY, DECLARED_AT);
            $members[$minted['envelopeCid']] = $minted['canonicalBytes'];
            $contractCids[$contract->name] = $minted['cid'];
        }
    }

    $built = $builder->build(CATALOG_NAME, CATALOG_VERSION, $members, DECLARED_AT);

    if (!is_dir($outDir)) { mkdir($outDir, 0755, true); }
    $proofPath = $outDir . '/' . $built['cid'] . '.proof';
    file_put_contents($proofPath, $built['bytes']);

    $cidValues = array_values(array_filter($contractCids, fn($c) => str_starts_with($c, 'blake3-512:')));
    sort($cidValues);
    $contractSetCid = Blake3::cid(Jcs::encode($cidValues));

    return [
        'cid' => $built['cid'],
        'contractSetCid' => $contractSetCid,
        'contractCount' => count($members),
        'proofPath' => $proofPath,
    ];
}

// Direct invocation (non-RPC): prints to stdout for `make mint-php` parsing
if (PHP_SAPI === 'cli' && !in_array('--rpc', $argv ?? [])) {
    $outDir = $argv[1] ?? dirname(__DIR__, 2);
    $result = mintAll($outDir);
    echo "{$result['cid']}\n";
    echo "contractSetCid: {$result['contractSetCid']}\n";
}
