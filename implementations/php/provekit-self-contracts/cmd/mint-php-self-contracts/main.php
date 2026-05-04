<?php
/** ProvekIt PHP self-contracts — Main mint entry point (direct mode). */

declare(strict_types=1);

require_once __DIR__ . '/../../provekit-ir-symbolic/src/Ir/Term.php';
require_once __DIR__ . '/../../provekit-ir-symbolic/src/Ir/Formula.php';
require_once __DIR__ . '/../../provekit-ir-symbolic/src/Ir/Declaration.php';
require_once __DIR__ . '/../../provekit-ir-symbolic/src/Canonicalizer/Blake3.php';
require_once __DIR__ . '/../../provekit-ir-symbolic/src/Canonicalizer/Jcs.php';
require_once __DIR__ . '/../../provekit-ir-symbolic/src/Canonicalizer/Ed25519.php';
require_once __DIR__ . '/../../provekit-ir-symbolic/src/ClaimEnvelope/Minter.php';
require_once __DIR__ . '/../../provekit-ir-symbolic/src/ProofEnvelope/Builder.php';
require_once __DIR__ . '/../slabs/slabs.php';

use ProvekIt\Ir\Collector;
use ProvekIt\Canonicalizer\{Blake3, Jcs, Ed25519};
use ProvekIt\ClaimEnvelope\Minter;
use ProvekIt\ProofEnvelope\Builder;
use ProvekIt\SelfContracts\Slabs;

function mintAll(string $outDir): array
{
    $signer = Ed25519::foundation();
    $minter = new Minter($signer);
    $builder = new Builder($signer);
    $producedBy = 'provekit-lift-php@0.1.0';
    $producedAt = '2026-05-03T18:00:00Z';
    $contractCids = [];

    // Pass 1: mint contracts from all slabs
    $members = [];
    foreach (Slabs() as $slab) {
        Collector::reset();
        ($slab->run)();
        $decls = Collector::finish();

        foreach ($decls['contracts'] as $contract) {
            $minted = $minter->mintContract($contract, $producedBy, $producedAt);
            $members[$minted['cid']] = $minted['canonicalBytes'];
            $contractCids[$contract->name] = $minted['cid'];
        }
    }

    // Pass 2: mint bridges (cross-kit resolved)
    // TODO: wire cross-kit bridge resolution from other kits' proof CIDs

    // Build proof envelope
    $built = $builder->build('ir-document', '0.1.0', $members, $producedAt);

    // Write .proof file
    $proofPath = $outDir . '/' . $built['cid'] . '.proof';
    file_put_contents($proofPath, $built['bytes']);

    return [
        'cid' => $built['cid'],
        'contractCount' => count($members),
        'proofPath' => $proofPath,
        'contractCids' => $contractCids,
    ];
}

// Direct invocation (non-RPC)
if (PHP_SAPI === 'cli' && !in_array('--rpc', $argv)) {
    $outDir = $argv[1] ?? __DIR__ . '/../../';
    $result = mintAll($outDir);
    echo "catalog CID: {$result['cid']}\n";
    echo "contracts: {$result['contractCount']}\n";
    echo ".proof: {$result['proofPath']}\n";
}
