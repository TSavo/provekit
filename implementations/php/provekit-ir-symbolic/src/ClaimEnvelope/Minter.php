<?php
/** ProvekIt — Claim envelope minter. */

namespace ProvekIt\ClaimEnvelope;

use ProvekIt\Canonicalizer\{Blake3, Ed25519, Jcs};
use ProvekIt\Ir\{ContractDecl, BridgeDecl, Sort, Collector};

class Minter
{
    private Ed25519 $signer;

    public function __construct(Ed25519 $signer)
    {
        $this->signer = $signer;
    }

    /** Mint a contract declaration into a signed claim envelope. */
    public function mintContract(
        ContractDecl $decl,
        string $producedBy,
        string $producedAt,
    ): array {
        $formulaValue = fn($f) => $f === null ? null : $f->jsonSerialize();

        // Compute property hash: BLAKE3(JCS({pre?, post?, inv?, outBinding}))
        $propObj = ['outBinding' => $decl->outBinding];
        if ($decl->pre !== null)  $propObj['pre'] = $formulaValue($decl->pre);
        if ($decl->post !== null) $propObj['post'] = $formulaValue($decl->post);
        if ($decl->inv !== null)  $propObj['inv'] = $formulaValue($decl->inv);
        $propHash = Blake3::cid(Jcs::encode($propObj));

        // Compute binding hash: BLAKE3(JCS({producerId, contractName, propertyHash}))
        $bindObj = [
            'producerId' => $producedBy,
            'contractName' => $decl->name,
            'propertyHash' => $propHash,
        ];
        $bindHash = Blake3::cid(Jcs::encode($bindObj));

        // Content CID: BLAKE3(JCS(name + outBinding + pre/post/inv))
        $contentObj = ['name' => $decl->name, 'outBinding' => $decl->outBinding];
        if ($decl->pre !== null)  $contentObj['pre'] = $formulaValue($decl->pre);
        if ($decl->post !== null) $contentObj['post'] = $formulaValue($decl->post);
        if ($decl->inv !== null)  $contentObj['inv'] = $formulaValue($decl->inv);
        $contentCid = Blake3::cid(Jcs::encode($contentObj));

        // Build header
        $header = [
            'kind' => 'contract',
            'schemaVersion' => '2',
            'cid' => $contentCid,
            'name' => $decl->name,
            'outBinding' => $decl->outBinding,
            'bindingHash' => $bindHash,
            'propertyHash' => $propHash,
            'inputCids' => [],
            'verdict' => 'holds',
        ];
        if ($decl->pre !== null) {
            $header['pre'] = $formulaValue($decl->pre);
        }
        if ($decl->post !== null) {
            $header['post'] = $formulaValue($decl->post);
        }
        if ($decl->inv !== null) {
            $header['inv'] = $formulaValue($decl->inv);
        }

        // Metadata is signed but opaque to the substrate verifier.
        $metadata = [
            'authoring' => [
                'producerKind' => 'kit-author',
                'author' => $producedBy,
                'note' => 'self-contract from php slab',
            ],
            'producedBy' => $producedBy,
            'producedAt' => $producedAt,
        ];
        if ($decl->pre !== null) {
            $metadata['preHash'] = Blake3::cid(Jcs::encode($formulaValue($decl->pre)));
        }
        if ($decl->post !== null) {
            $metadata['postHash'] = Blake3::cid(Jcs::encode($formulaValue($decl->post)));
        }
        if ($decl->inv !== null) {
            $metadata['invHash'] = Blake3::cid(Jcs::encode($formulaValue($decl->inv)));
        }

        // Build envelope. The signature covers JCS({header, metadata}); the
        // member CID is BLAKE3-512(JCS(envelope-with-signature)).
        $env = [
            'signer' => 'ed25519:' . $this->signer->pubKeyBase64(),
            'declaredAt' => $producedAt,
        ];
        $sigPayload = Jcs::encode(['header' => $header, 'metadata' => $metadata]);
        $env['signature'] = 'ed25519:' . $this->signer->signBase64($sigPayload);
        $attestationCid = Blake3::cid(Jcs::encode($env));

        // Full memento
        $memento = [
            'envelope' => $env,
            'header' => $header,
            'metadata' => $metadata,
        ];

        $canonical = Jcs::encode($memento);

        return [
            'cid' => $contentCid,
            'envelopeCid' => $attestationCid,
            'canonicalBytes' => $canonical,
        ];
    }

    /** Mint bridge declarations similarly */
    public function mintBridge(BridgeDecl $decl): array
    {
        $contentObj = [
            'name' => $decl->name,
            'sourceSymbol' => $decl->sourceSymbol,
            'sourceLayer' => $decl->sourceLayer,
            'sourceContractCid' => $decl->sourceContractCid,
            'targetContractCid' => $decl->targetContractCid,
            'targetProofCid' => $decl->targetProofCid,
            'targetLayer' => $decl->targetLayer,
        ];
        $cid = Blake3::cid(Jcs::encode($contentObj));

        return [
            'cid' => $cid,
            'canonicalBytes' => Jcs::encode($decl),
        ];
    }

    /**
     * Mint a v1.4 BridgeDeclaration (layered envelope/header/body,
     * tagged-union target). Per spec bridge-target-dimensionality.md §1.R1-R6.
     *
     * Canonical reference: rust/provekit-claim-envelope/src/lib.rs fn mint_bridge_v14.
     *
     * @param array $args with keys: name, sourceSymbol, sourceLayer,
     *   sourceContractCid, target (['kind'=>'contract'|'contractSet','cid'=>'...']),
     *   declaredAt, plus optional metadata fields.
     */
    public function mintBridgeV14(array $args): array
    {
        // Validate and normalize tagged-union target before signing (P1 #19).
        // Extra keys or missing kind/cid would produce a malformed header that
        // downstream verifiers reject. Reject early with a clear error.
        $target = $args['target'] ?? null;
        if (!is_array($target)) {
            throw new \InvalidArgumentException("mintBridgeV14: 'target' must be an array");
        }
        $validKinds = ['contract', 'contractSet'];
        $targetKind = $target['kind'] ?? null;
        if (!in_array($targetKind, $validKinds, true)) {
            throw new \InvalidArgumentException(
                "mintBridgeV14: target.kind must be one of [" . implode(', ', $validKinds) . "], got: " . json_encode($targetKind)
            );
        }
        if (empty($target['cid']) || !is_string($target['cid'])) {
            throw new \InvalidArgumentException("mintBridgeV14: target.cid must be a non-empty string");
        }
        // Emit only the canonical two fields — strip any extra keys.
        $normalizedTarget = ['cid' => $target['cid'], 'kind' => $targetKind];

        // Build header (7 canonical fields per §1.R3)
        $header = [
            'schemaVersion' => '1',
            'kind' => 'bridge',
            'name' => $args['name'],
            'sourceSymbol' => $args['sourceSymbol'],
            'sourceLayer' => $args['sourceLayer'],
            'sourceContractCid' => $args['sourceContractCid'],
            'target' => $normalizedTarget,
        ];

        // Build metadata (omit missing fields per §1.R2)
        $metaKeys = ['targetWitnessCid','targetBinaryCid','targetLayer',
                      'targetContractSetCid','producedBy','producedAt'];
        $meta = [];
        foreach ($metaKeys as $k) {
            if (!empty($args[$k])) $meta[$k] = $args[$k];
        }

        // Sign: JCS({header, metadata})
        $sigPayload = ['header' => $header, 'metadata' => $meta];
        $sigPayloadJcs = Jcs::encode($sigPayload);
        $sig = $this->signer->signBase64($sigPayloadJcs);

        // Build envelope
        $env = [
            'signer' => 'ed25519:' . $this->signer->pubKeyBase64(),
            'declaredAt' => $args['declaredAt'],
            'signature' => 'ed25519:' . $sig,
        ];

        // Full memento: {envelope, header, metadata}
        $memento = ['envelope' => $env, 'header' => $header, 'metadata' => $meta];
        $canonical = Jcs::encode($memento);

        return [
            'cid' => Blake3::cid(Jcs::encode($env)),
            'canonicalBytes' => $canonical,
        ];
    }
}
